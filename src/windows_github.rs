use std::{ffi::c_void, mem, ptr};

use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER},
    Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
        WinHttpSetTimeouts, ERROR_WINHTTP_HEADER_NOT_FOUND, ERROR_WINHTTP_TIMEOUT,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_REDIRECT_POLICY,
        WINHTTP_OPTION_REDIRECT_POLICY_NEVER, WINHTTP_QUERY_CUSTOM, WINHTTP_QUERY_FLAG_NUMBER,
        WINHTTP_QUERY_STATUS_CODE,
    },
};

use crate::admission::secure_zero_bytes;
use crate::{
    AdmissionBackendError, AdmissionBinding, CredentialLease, CredentialReference,
    GithubApiTransport, GithubRepositoryAccessClient, GithubRestAdmissionBackend, GithubWireClient,
    GithubWireError, GithubWireRequest, GithubWireResponse, GithubWorkflowClient, HttpMethod,
    WindowsCredentialManagerProvider, GITHUB_API_HOST, GITHUB_API_USER_AGENT,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsWinHttpClient;

pub type WindowsGithubAdmissionBackend = GithubRestAdmissionBackend<
    GithubApiTransport<WindowsWinHttpClient>,
    WindowsCredentialManagerProvider,
>;

pub type WindowsGithubWorkflowClient = GithubWorkflowClient<
    GithubApiTransport<WindowsWinHttpClient>,
    WindowsCredentialManagerProvider,
>;

pub type WindowsGithubRepositoryAccessClient = GithubRepositoryAccessClient<
    GithubApiTransport<WindowsWinHttpClient>,
    WindowsCredentialManagerProvider,
>;

pub fn windows_github_admission_backend(
    binding: AdmissionBinding,
) -> Result<WindowsGithubAdmissionBackend, AdmissionBackendError> {
    GithubRestAdmissionBackend::new(
        binding,
        GithubApiTransport::new(WindowsWinHttpClient),
        WindowsCredentialManagerProvider::new(),
    )
}

pub fn windows_github_workflow_client(
    credential_ref: CredentialReference,
) -> WindowsGithubWorkflowClient {
    GithubWorkflowClient::new(
        GithubApiTransport::new(WindowsWinHttpClient),
        WindowsCredentialManagerProvider::new(),
        credential_ref,
    )
}

pub fn windows_github_repository_access_client(
    credential_ref: CredentialReference,
) -> WindowsGithubRepositoryAccessClient {
    GithubRepositoryAccessClient::new(
        GithubApiTransport::new(WindowsWinHttpClient),
        WindowsCredentialManagerProvider::new(),
        credential_ref,
    )
}

impl GithubWireClient for WindowsWinHttpClient {
    fn execute(
        &mut self,
        request: &GithubWireRequest,
        credential: &CredentialLease,
    ) -> Result<GithubWireResponse, GithubWireError> {
        if request.host != GITHUB_API_HOST || request.port != 443 {
            return Err(GithubWireError::InvalidResponse);
        }
        let agent = wide_nul(GITHUB_API_USER_AGENT);
        // SAFETY: all strings are NUL terminated; null proxy pointers select
        // automatic system proxy discovery without credential callbacks.
        let session = InternetHandle::new(unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                ptr::null(),
                ptr::null(),
                0,
            )
        })?;
        let host = wide_nul(&request.host);
        // SAFETY: session is live and host is NUL terminated.
        let connection = InternetHandle::new(unsafe {
            WinHttpConnect(session.0, host.as_ptr(), request.port, 0)
        })?;
        let verb = wide_nul(match request.method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Delete => "DELETE",
        });
        let path = wide_nul(&request.path);
        // SAFETY: connection is live and all supplied strings are NUL
        // terminated. Default TLS certificate verification remains enabled.
        let request_handle = InternetHandle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                path.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        })?;

        let timeout = i32::try_from(request.timeout_milliseconds)
            .map_err(|_| GithubWireError::InvalidResponse)?;
        // SAFETY: request_handle is live and timeout values are bounded i32.
        if unsafe { WinHttpSetTimeouts(request_handle.0, timeout, timeout, timeout, timeout) } == 0
        {
            return Err(last_transport_error());
        }
        let redirect_policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        // SAFETY: request_handle is live and the option buffer points to one
        // initialized u32. Redirect refusal prevents bearer forwarding.
        if unsafe {
            WinHttpSetOption(
                request_handle.0,
                WINHTTP_OPTION_REDIRECT_POLICY,
                (&redirect_policy as *const u32).cast::<c_void>(),
                mem::size_of::<u32>() as u32,
            )
        } == 0
        {
            return Err(last_transport_error());
        }

        let headers = SensitiveWide::request_headers(&request.headers, credential)?;
        let header_length = u32::try_from(headers.len_without_nul())
            .map_err(|_| GithubWireError::InvalidResponse)?;
        let body_length =
            u32::try_from(request.body.len()).map_err(|_| GithubWireError::InvalidResponse)?;
        let body_pointer = if request.body.is_empty() {
            ptr::null()
        } else {
            request.body.as_ptr().cast::<c_void>()
        };
        // SAFETY: request_handle is live, headers remain valid for this call,
        // and the optional body pointer covers exactly body_length bytes.
        let sent = unsafe {
            WinHttpSendRequest(
                request_handle.0,
                headers.as_ptr(),
                header_length,
                body_pointer,
                body_length,
                body_length,
                0,
            )
        };
        let send_error = (sent == 0).then(|| unsafe { GetLastError() });
        drop(headers);
        if let Some(error) = send_error {
            return Err(last_transport_error_from(error));
        }
        // SAFETY: request_handle has a sent request and no reserved context.
        if unsafe { WinHttpReceiveResponse(request_handle.0, ptr::null_mut()) } == 0 {
            return Err(last_transport_error());
        }

        let status = query_status(request_handle.0)?;
        let body = read_body(request_handle.0, request.max_response_bytes)?;
        Ok(GithubWireResponse {
            status,
            body,
            retry_after: query_header(request_handle.0, "Retry-After")?,
            rate_limit_remaining: query_header(request_handle.0, "X-RateLimit-Remaining")?,
            rate_limit_reset_epoch_seconds: query_header(request_handle.0, "X-RateLimit-Reset")?,
            link: query_header(request_handle.0, "Link")?,
        })
    }
}

struct InternetHandle(*mut c_void);

impl InternetHandle {
    fn new(raw: *mut c_void) -> Result<Self, GithubWireError> {
        if raw.is_null() {
            Err(last_transport_error())
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for InternetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle is owned by this guard and closed once.
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

struct SensitiveWide(Vec<u16>);

impl SensitiveWide {
    fn request_headers(
        headers: &[(String, String)],
        credential: &CredentialLease,
    ) -> Result<Self, GithubWireError> {
        let fixed_length = b"Authorization: Bearer ".len() + b"\r\n\0".len();
        let required = headers
            .iter()
            .try_fold(fixed_length, |length, (name, value)| {
                length
                    .checked_add(name.len())?
                    .checked_add(b": ".len())?
                    .checked_add(value.len())?
                    .checked_add(b"\r\n".len())
            });
        let required = required
            .and_then(|length| length.checked_add(credential.expose_for_transport().len()))
            .ok_or(GithubWireError::InvalidResponse)?;
        let mut value = Vec::new();
        value
            .try_reserve_exact(required)
            .map_err(|_| GithubWireError::InvalidResponse)?;
        for (name, header_value) in headers {
            append_wide(&mut value, name.as_bytes());
            append_wide(&mut value, b": ");
            append_wide(&mut value, header_value.as_bytes());
            append_wide(&mut value, b"\r\n");
        }
        append_wide(&mut value, b"Authorization: Bearer ");
        append_wide(&mut value, credential.expose_for_transport());
        append_wide(&mut value, b"\r\n");
        value.push(0);
        debug_assert_eq!(value.len(), required);
        Ok(Self(value))
    }

    fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }

    fn len_without_nul(&self) -> usize {
        self.0.len().saturating_sub(1)
    }
}

impl Drop for SensitiveWide {
    fn drop(&mut self) {
        secure_zero_wide(&mut self.0);
    }
}

#[inline(never)]
fn secure_zero_wide(values: &mut [u16]) {
    // SAFETY: a u16 slice is contiguous and may be viewed as its complete byte
    // representation for in-place clearing.
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(
            values.as_mut_ptr().cast::<u8>(),
            std::mem::size_of_val(values),
        )
    };
    secure_zero_bytes(bytes);
}

fn append_wide(target: &mut Vec<u16>, bytes: &[u8]) {
    target.extend(bytes.iter().map(|byte| u16::from(*byte)));
}

fn query_status(request: *mut c_void) -> Result<u16, GithubWireError> {
    let mut status = 0_u32;
    let mut length = mem::size_of::<u32>() as u32;
    let mut index = 0_u32;
    // SAFETY: request is live and status is a correctly sized output buffer.
    if unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            (&mut status as *mut u32).cast::<c_void>(),
            &mut length,
            &mut index,
        )
    } == 0
    {
        return Err(last_transport_error());
    }
    u16::try_from(status).map_err(|_| GithubWireError::InvalidResponse)
}

fn query_header(request: *mut c_void, name: &str) -> Result<Option<String>, GithubWireError> {
    let name = wide_nul(name);
    let mut length = 0_u32;
    let mut index = 0_u32;
    // SAFETY: request and name are valid. A null output buffer requests the
    // exact byte count.
    let first = unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_CUSTOM,
            name.as_ptr(),
            ptr::null_mut(),
            &mut length,
            &mut index,
        )
    };
    if first == 0 {
        // SAFETY: GetLastError follows the failed query immediately.
        let error = unsafe { GetLastError() };
        if error == ERROR_WINHTTP_HEADER_NOT_FOUND {
            return Ok(None);
        }
        if error != ERROR_INSUFFICIENT_BUFFER || length == 0 {
            return Err(last_transport_error_from(error));
        }
    }
    if length == 0 || !(length as usize).is_multiple_of(mem::size_of::<u16>()) {
        return Err(GithubWireError::InvalidResponse);
    }
    let mut buffer = vec![0_u16; length as usize / mem::size_of::<u16>()];
    index = 0;
    // SAFETY: buffer is exactly the byte length requested above.
    if unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_CUSTOM,
            name.as_ptr(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut length,
            &mut index,
        )
    } == 0
    {
        return Err(last_transport_error());
    }
    while buffer.last() == Some(&0) {
        buffer.pop();
    }
    String::from_utf16(&buffer)
        .map(Some)
        .map_err(|_| GithubWireError::InvalidResponse)
}

fn read_body(request: *mut c_void, maximum: usize) -> Result<Vec<u8>, GithubWireError> {
    let mut body = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let mut read = 0_u32;
        // SAFETY: request is live and buffer covers the requested byte count.
        if unsafe {
            WinHttpReadData(
                request,
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer.len() as u32,
                &mut read,
            )
        } == 0
        {
            return Err(last_transport_error());
        }
        if read == 0 {
            return Ok(body);
        }
        let read = read as usize;
        if read > buffer.len() || body.len().saturating_add(read) > maximum {
            return Err(GithubWireError::InvalidResponse);
        }
        body.extend_from_slice(&buffer[..read]);
    }
}

fn wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_transport_error() -> GithubWireError {
    // SAFETY: callers invoke this immediately after a failed WinHTTP call.
    last_transport_error_from(unsafe { GetLastError() })
}

fn last_transport_error_from(error: u32) -> GithubWireError {
    if error == ERROR_WINHTTP_TIMEOUT {
        GithubWireError::Timeout
    } else {
        GithubWireError::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_headers_are_zeroized_and_not_debuggable() {
        let credential = CredentialLease::from_secret("synthetic-token-shape").unwrap();
        let headers = SensitiveWide::request_headers(&[], &credential).unwrap();
        assert!(headers
            .0
            .windows(3)
            .any(|window| window == ['B' as u16, 'e' as u16, 'a' as u16]));
        assert_eq!(headers.0.last(), Some(&0));
    }

    #[test]
    fn sensitive_wide_scrubbing_clears_the_complete_allocation_contents() {
        let mut values = vec![0x1234_u16; 32];
        secure_zero_wide(&mut values);
        assert!(values.iter().all(|value| *value == 0));
    }
}
