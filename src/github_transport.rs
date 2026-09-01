use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    CredentialLease, GithubHttpRequest, GithubHttpResponse, GithubHttpTransport,
    GithubTransportError, HttpMethod,
};

pub const GITHUB_API_HOST: &str = "api.github.com";
pub const GITHUB_API_VERSION: &str = "2026-03-10";
pub const GITHUB_API_PORT: u16 = 443;
pub const GITHUB_API_USER_AGENT: &str = "RunnerMesh/0.1";
pub const DEFAULT_GITHUB_TIMEOUT_MILLISECONDS: u32 = 30_000;
pub const DEFAULT_GITHUB_MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// A secret-free wire request. The credential remains a separate lease passed
/// directly to the wire client so request debug output cannot expose it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubWireRequest {
    pub method: HttpMethod,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout_milliseconds: u32,
    pub max_response_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubWireResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after: Option<String>,
    pub rate_limit_remaining: Option<String>,
    pub rate_limit_reset_epoch_seconds: Option<String>,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GithubWireError {
    Unavailable,
    Timeout,
    InvalidResponse,
}

pub trait GithubWireClient {
    fn execute(
        &mut self,
        request: &GithubWireRequest,
        credential: &CredentialLease,
    ) -> Result<GithubWireResponse, GithubWireError>;
}

pub trait GithubClock {
    fn unix_seconds(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemGithubClock;

impl GithubClock for SystemGithubClock {
    fn unix_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

/// Production request adapter for the fixed public GitHub API authority. It
/// deliberately has no configurable host, preventing an opaque bearer from
/// being redirected to an Owner-supplied or compromised endpoint.
pub struct GithubApiTransport<W, C = SystemGithubClock> {
    wire: W,
    clock: C,
    timeout_milliseconds: u32,
    max_response_bytes: usize,
}

impl<W> GithubApiTransport<W, SystemGithubClock> {
    pub fn new(wire: W) -> Self {
        Self::with_clock(wire, SystemGithubClock)
    }
}

impl<W, C> GithubApiTransport<W, C> {
    pub fn with_clock(wire: W, clock: C) -> Self {
        Self {
            wire,
            clock,
            timeout_milliseconds: DEFAULT_GITHUB_TIMEOUT_MILLISECONDS,
            max_response_bytes: DEFAULT_GITHUB_MAX_RESPONSE_BYTES,
        }
    }

    #[cfg(test)]
    fn wire(&self) -> &W {
        &self.wire
    }

    #[cfg(test)]
    fn with_limits(mut self, timeout_milliseconds: u32, max_response_bytes: usize) -> Self {
        self.timeout_milliseconds = timeout_milliseconds.max(1);
        self.max_response_bytes = max_response_bytes.max(1);
        self
    }
}

impl<W: GithubWireClient, C: GithubClock> GithubHttpTransport for GithubApiTransport<W, C> {
    fn send(
        &mut self,
        request: &GithubHttpRequest,
        credential: &CredentialLease,
    ) -> Result<GithubHttpResponse, GithubTransportError> {
        if !valid_api_path(&request.path) {
            return Err(GithubTransportError::InvalidResponse);
        }
        let body = request
            .body
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| GithubTransportError::InvalidResponse)?
            .unwrap_or_default();
        let mut headers = vec![
            (
                "Accept".to_owned(),
                "application/vnd.github+json".to_owned(),
            ),
            (
                "X-GitHub-Api-Version".to_owned(),
                GITHUB_API_VERSION.to_owned(),
            ),
        ];
        if !body.is_empty() {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
        }
        let response = self
            .wire
            .execute(
                &GithubWireRequest {
                    method: request.method,
                    host: GITHUB_API_HOST.to_owned(),
                    port: GITHUB_API_PORT,
                    path: request.path.clone(),
                    headers,
                    body,
                    timeout_milliseconds: self.timeout_milliseconds,
                    max_response_bytes: self.max_response_bytes,
                },
                credential,
            )
            .map_err(|error| match error {
                GithubWireError::Unavailable => GithubTransportError::Unavailable,
                GithubWireError::Timeout => GithubTransportError::Timeout,
                GithubWireError::InvalidResponse => GithubTransportError::InvalidResponse,
            })?;
        if response.body.len() > self.max_response_bytes {
            return Err(GithubTransportError::InvalidResponse);
        }
        let retry_after_seconds = parse_retry_after(&response, self.clock.unix_seconds());
        Ok(GithubHttpResponse {
            status: response.status,
            body: response.body,
            retry_after_seconds,
            has_next_page: response.link.as_deref().is_some_and(has_next_link),
        })
    }
}

fn valid_api_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 4096
        && path.is_ascii()
        && !path.bytes().any(|byte| byte.is_ascii_control())
        && !path.contains("\\")
        && !path.contains("://")
        && !path
            .split('?')
            .next()
            .unwrap_or_default()
            .split('/')
            .any(|part| matches!(part, "." | ".."))
}

fn parse_retry_after(response: &GithubWireResponse, now: u64) -> Option<u64> {
    if let Some(seconds) = response.retry_after.as_deref().and_then(parse_header_u64) {
        return Some(seconds.max(1));
    }
    let exhausted = response
        .rate_limit_remaining
        .as_deref()
        .and_then(parse_header_u64)
        == Some(0);
    exhausted.then(|| {
        response
            .rate_limit_reset_epoch_seconds
            .as_deref()
            .and_then(parse_header_u64)
            .map(|reset| reset.saturating_sub(now).max(1))
            .unwrap_or(1)
    })
}

fn parse_header_u64(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn has_next_link(value: &str) -> bool {
    value.split(',').any(|entry| {
        entry
            .split(';')
            .skip(1)
            .any(|parameter| parameter.trim().eq_ignore_ascii_case("rel=\"next\""))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[derive(Default)]
    struct FakeWire {
        results: VecDeque<Result<GithubWireResponse, GithubWireError>>,
        requests: Vec<GithubWireRequest>,
        credential_present: bool,
    }

    impl GithubWireClient for FakeWire {
        fn execute(
            &mut self,
            request: &GithubWireRequest,
            credential: &CredentialLease,
        ) -> Result<GithubWireResponse, GithubWireError> {
            self.credential_present = !credential.expose_for_transport().is_empty();
            self.requests.push(request.clone());
            self.results.pop_front().unwrap()
        }
    }

    #[derive(Clone, Copy)]
    struct FixedClock(u64);

    impl GithubClock for FixedClock {
        fn unix_seconds(&self) -> u64 {
            self.0
        }
    }

    fn response(status: u16) -> GithubWireResponse {
        GithubWireResponse {
            status,
            body: b"{}".to_vec(),
            retry_after: None,
            rate_limit_remaining: None,
            rate_limit_reset_epoch_seconds: None,
            link: None,
        }
    }

    #[test]
    fn adapter_targets_only_the_fixed_github_authority_without_secret_headers() {
        let wire = FakeWire {
            results: [Ok(response(200))].into_iter().collect(),
            ..FakeWire::default()
        };
        let mut transport = GithubApiTransport::with_clock(wire, FixedClock(1_000));
        let credential = CredentialLease::from_secret("synthetic-token-shape").unwrap();
        transport
            .send(
                &GithubHttpRequest {
                    method: HttpMethod::Get,
                    path: "/orgs/example/actions/runners?per_page=100&page=1".to_owned(),
                    body: None,
                },
                &credential,
            )
            .unwrap();

        let wire = transport.wire();
        assert!(wire.credential_present);
        let request = &wire.requests[0];
        assert_eq!(request.host, GITHUB_API_HOST);
        assert_eq!(request.port, 443);
        assert!(request.headers.contains(&(
            "X-GitHub-Api-Version".to_owned(),
            GITHUB_API_VERSION.to_owned()
        )));
        let debug = format!("{request:?}");
        assert!(!debug.contains("synthetic-token-shape"));
        assert!(!request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization")));
    }

    #[test]
    fn adapter_serializes_only_the_typed_add_one_body() {
        let wire = FakeWire {
            results: [Ok(response(200))].into_iter().collect(),
            ..FakeWire::default()
        };
        let mut transport = GithubApiTransport::with_clock(wire, FixedClock(1_000));
        transport
            .send(
                &GithubHttpRequest {
                    method: HttpMethod::Post,
                    path: "/orgs/example/actions/runners/42/labels".to_owned(),
                    body: Some(serde_json::json!({"labels": ["runnermesh-admit"]})),
                },
                &CredentialLease::from_secret("synthetic-token-shape").unwrap(),
            )
            .unwrap();
        assert_eq!(
            transport.wire().requests[0].body,
            br#"{"labels":["runnermesh-admit"]}"#
        );
    }

    #[test]
    fn rate_limit_headers_and_pagination_are_normalized() {
        let mut response = response(403);
        response.rate_limit_remaining = Some("0".to_owned());
        response.rate_limit_reset_epoch_seconds = Some("1017".to_owned());
        response.link = Some("<https://api.github.com/x?page=2>; rel=\"next\"".to_owned());
        let wire = FakeWire {
            results: [Ok(response)].into_iter().collect(),
            ..FakeWire::default()
        };
        let mut transport = GithubApiTransport::with_clock(wire, FixedClock(1_000));
        let response = transport
            .send(
                &GithubHttpRequest {
                    method: HttpMethod::Get,
                    path: "/orgs/example/actions/runners".to_owned(),
                    body: None,
                },
                &CredentialLease::from_secret("synthetic-token-shape").unwrap(),
            )
            .unwrap();
        assert_eq!(response.retry_after_seconds, Some(17));
        assert!(response.has_next_page);
    }

    #[test]
    fn timeout_invalid_path_and_oversized_response_fail_closed() {
        let mut timeout = GithubApiTransport::with_clock(
            FakeWire {
                results: [Err(GithubWireError::Timeout)].into_iter().collect(),
                ..FakeWire::default()
            },
            FixedClock(0),
        );
        let credential = CredentialLease::from_secret("synthetic-token-shape").unwrap();
        let request = GithubHttpRequest {
            method: HttpMethod::Get,
            path: "/orgs/example/actions/runners".to_owned(),
            body: None,
        };
        assert_eq!(
            timeout.send(&request, &credential).unwrap_err(),
            GithubTransportError::Timeout
        );

        let mut invalid = GithubApiTransport::with_clock(FakeWire::default(), FixedClock(0));
        let mut invalid_request = request.clone();
        invalid_request.path = "https://untrusted.invalid/".to_owned();
        assert_eq!(
            invalid.send(&invalid_request, &credential).unwrap_err(),
            GithubTransportError::InvalidResponse
        );

        let mut too_large_response = response(200);
        too_large_response.body = vec![0; 9];
        let mut too_large = GithubApiTransport::with_clock(
            FakeWire {
                results: [Ok(too_large_response)].into_iter().collect(),
                ..FakeWire::default()
            },
            FixedClock(0),
        )
        .with_limits(1_000, 8);
        assert_eq!(
            too_large.send(&request, &credential).unwrap_err(),
            GithubTransportError::InvalidResponse
        );
    }
}
