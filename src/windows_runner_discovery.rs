//! Windows read-only discovery adapters for official GitHub Actions runners.
//!
//! Enumerates `actions.runner.*` Windows Services and observes explicitly
//! configured session homes. Never starts/stops services, never mutates
//! registration, and never reads credential files.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(windows)]
use sha2::{Digest, Sha256};

use crate::{
    DiscoveredLifecycle, DiscoveryEnumerationStatus, DiscoveryIdentityFilter, DiscoveryReport,
    EmptyUnsupportedDiscoverySource, OpaqueIdentityReference, RegistrationScope,
    RunnerDiscoveryInventory, ServiceCandidateFact, ServiceDiscoverySource, SessionCandidateFact,
    SessionDiscoverySource,
};

#[cfg(windows)]
const SERVICE_ACCOUNT_IDENTITY_PROVIDER: &str = "windows-service-account-sha256-v1";

/// Runs the pure inventory over live Service enumeration plus optional session homes.
pub fn discover_local_runners_read_only(session_homes: Vec<PathBuf>) -> DiscoveryReport {
    let service = WindowsServiceDiscoverySource::default();
    let session = ConfiguredSessionDiscoverySource::new(session_homes);
    let process_status = session.last_process_status();
    // Do not force current-user identity onto Service runners: their execution
    // identity is the service account. Session homes already carry current-user
    // opaque refs when available.
    let inventory = RunnerDiscoveryInventory::new(
        service,
        session,
        EmptyUnsupportedDiscoverySource,
        DiscoveryIdentityFilter {
            expected_execution_identity: None,
        },
    );
    inventory.discover_with_process_status(process_status)
}

/// Privacy-safe summary of a live discovery pass. Never includes paths, names,
/// service identities, or host topology.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacySafeDiscoverySummary {
    pub candidate_count: usize,
    pub observed_only_count: usize,
    pub refused_count: usize,
    pub identity_complete_count: usize,
    pub service_enumeration: DiscoveryEnumerationStatus,
    pub session_enumeration: DiscoveryEnumerationStatus,
    pub process_observation: DiscoveryEnumerationStatus,
    pub incomplete: bool,
}

impl PrivacySafeDiscoverySummary {
    pub fn from_report(report: &DiscoveryReport) -> Self {
        Self {
            candidate_count: report.candidates.len(),
            observed_only_count: report.observed_only().count(),
            refused_count: report.refused().count(),
            identity_complete_count: report.identity_complete_count(),
            service_enumeration: report.service_enumeration,
            session_enumeration: report.session_enumeration,
            process_observation: report.process_observation,
            incomplete: report.is_incomplete(),
        }
    }
}

#[cfg(windows)]
const ACTIONS_RUNNER_PREFIX: &str = "actions.runner.";

/// Builds a discovery inventory that uses live Windows Service enumeration when
/// compiled for Windows; non-Windows targets get empty service results.
pub fn host_service_discovery_source() -> Box<dyn ServiceDiscoverySource> {
    #[cfg(windows)]
    {
        Box::new(WindowsServiceDiscoverySource::default())
    }
    #[cfg(not(windows))]
    {
        Box::new(crate::EmptyServiceDiscoverySource)
    }
}

fn current_user_identity_optional() -> Option<OpaqueIdentityReference> {
    #[cfg(windows)]
    {
        crate::current_windows_identity_reference().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn opaque_from_account_name(account: &str) -> Option<OpaqueIdentityReference> {
    let normalized = account.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    let digest = hex_sha256(normalized.as_bytes());
    OpaqueIdentityReference::new(SERVICE_ACCOUNT_IDENTITY_PROVIDER, digest).ok()
}

#[cfg(windows)]
fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Session discovery over explicitly configured runner homes (read-only).
#[derive(Clone, Default)]
pub struct ConfiguredSessionDiscoverySource {
    homes: Vec<PathBuf>,
    last_process_status: std::sync::Arc<Mutex<DiscoveryEnumerationStatus>>,
}

impl ConfiguredSessionDiscoverySource {
    pub fn new(homes: Vec<PathBuf>) -> Self {
        Self {
            homes,
            last_process_status: std::sync::Arc::new(Mutex::new(DiscoveryEnumerationStatus::Ok)),
        }
    }

    pub fn last_process_status(&self) -> DiscoveryEnumerationStatus {
        *self
            .last_process_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl SessionDiscoverySource for ConfiguredSessionDiscoverySource {
    fn list_session_candidates(&self) -> Vec<SessionCandidateFact> {
        let mut process_status = DiscoveryEnumerationStatus::Ok;
        let facts = self
            .homes
            .iter()
            .filter_map(|home| observe_session_home(home, &mut process_status))
            .collect();
        *self
            .last_process_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = process_status;
        facts
    }

    fn enumeration_status(&self) -> DiscoveryEnumerationStatus {
        DiscoveryEnumerationStatus::Ok
    }
}

fn observe_session_home(
    runner_home: &Path,
    process_status: &mut DiscoveryEnumerationStatus,
) -> Option<SessionCandidateFact> {
    if !runner_home.is_dir() {
        return Some(SessionCandidateFact {
            runner_home: runner_home.to_path_buf(),
            work_root: None,
            registration_scope: None,
            runner_id: None,
            expected_runner_name: None,
            execution_identity_ref: None,
            current_user_owned: false,
            listener_present: false,
            worker_present: false,
            state: DiscoveredLifecycle::Unknown,
        });
    }
    let meta = read_runner_metadata(runner_home);
    let work_root = meta
        .as_ref()
        .and_then(|m| m.work_folder.clone())
        .map(|relative| {
            let path = PathBuf::from(&relative);
            if path.is_absolute() {
                path
            } else {
                runner_home.join(path)
            }
        });
    let (listener_running, worker_running, snapshot_ok) =
        observe_runner_processes_for_home(runner_home);
    if !snapshot_ok {
        *process_status = DiscoveryEnumerationStatus::Unavailable;
    }
    // Binary presence is not process evidence. Prefer live process attribution.
    let state = if worker_running {
        DiscoveredLifecycle::Busy
    } else if listener_running {
        DiscoveredLifecycle::Listening
    } else if snapshot_ok {
        // Snapshot succeeded and no matching process was found.
        DiscoveredLifecycle::Stopped
    } else {
        DiscoveredLifecycle::Unknown
    };
    let identity = current_user_identity_optional();
    Some(SessionCandidateFact {
        runner_home: runner_home.to_path_buf(),
        work_root,
        registration_scope: meta.as_ref().and_then(|m| m.scope.clone()),
        runner_id: meta.as_ref().and_then(|m| m.agent_id),
        expected_runner_name: meta.as_ref().and_then(|m| m.agent_name.clone()),
        execution_identity_ref: identity,
        current_user_owned: true,
        listener_present: listener_running,
        worker_present: worker_running,
        state,
    })
}

fn observe_runner_processes_for_home(runner_home: &Path) -> (bool, bool, bool) {
    #[cfg(windows)]
    {
        match crate::process_snapshot::executable_images() {
            Ok(images) => {
                let home = normalize_path_key(runner_home);
                let expected_listener =
                    normalize_path_key(&runner_home.join("bin").join("Runner.Listener.exe"));
                let expected_worker =
                    normalize_path_key(&runner_home.join("bin").join("Runner.Worker.exe"));
                let mut listener = false;
                let mut worker = false;
                for image in images {
                    let Some(path) = image.executable_path.as_ref() else {
                        continue;
                    };
                    let path_key = normalize_path_key(path);
                    // Path-boundary under home (never raw prefix: ...\a vs ...\abc).
                    if !path_is_under_home_key(&path_key, &home) {
                        continue;
                    }
                    // Require current-user ownership evidence when available.
                    if image.user_matches_current == Some(false) {
                        continue;
                    }
                    let name = image.executable_name.to_ascii_lowercase();
                    if name == "runner.listener.exe"
                        && (path_key == expected_listener
                            || path_key.ends_with("\\bin\\runner.listener.exe"))
                    {
                        listener = true;
                    } else if name == "runner.worker.exe"
                        && (path_key == expected_worker
                            || path_key.ends_with("\\bin\\runner.worker.exe"))
                    {
                        worker = true;
                    }
                }
                (listener, worker, true)
            }
            Err(_) => (false, false, false),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = runner_home;
        (false, false, true)
    }
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

/// True when `path_key` is `home` or a descendant separated by `\`.
/// Rejects sibling prefix collisions (`home\a` vs `home\abc`).
fn path_is_under_home_key(path_key: &str, home_key: &str) -> bool {
    if path_key == home_key {
        return true;
    }
    if home_key.is_empty() {
        return false;
    }
    let boundary = format!("{home_key}\\");
    path_key.starts_with(&boundary)
}

#[derive(Clone, Default)]
struct RunnerMetadataLite {
    agent_id: Option<u64>,
    agent_name: Option<String>,
    scope: Option<RegistrationScope>,
    work_folder: Option<String>,
}

fn read_runner_metadata(runner_home: &Path) -> Option<RunnerMetadataLite> {
    let path = runner_home.join(".runner");
    let bytes = std::fs::read(&path).ok()?;
    let json = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let agent_id = value
        .get("agentId")
        .or_else(|| value.get("AgentId"))
        .and_then(|v| v.as_u64());
    let agent_name = value
        .get("agentName")
        .or_else(|| value.get("AgentName"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let work_folder = value
        .get("workFolder")
        .or_else(|| value.get("WorkFolder"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let github_url = value
        .get("gitHubUrl")
        .or_else(|| value.get("GitHubUrl"))
        .and_then(|v| v.as_str());
    let scope = github_url.and_then(scope_from_github_url);
    Some(RunnerMetadataLite {
        agent_id,
        agent_name,
        scope,
        work_folder,
    })
}

fn scope_from_github_url(url: &str) -> Option<RegistrationScope> {
    let trimmed = url.trim().trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    // Accept https://github.com/... and http://github.com/... without requiring
    // a brittle index-based split that mis-classifies org landing URLs.
    let without_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let mut parts = without_scheme.split('/');
    let host = parts.next()?;
    if host != "github.com" {
        return None;
    }
    let first = parts.next()?.to_owned();
    if first.is_empty() {
        return None;
    }
    match first.as_str() {
        "orgs" => {
            let organization = parts.next()?.to_owned();
            if organization.is_empty() {
                return None;
            }
            Some(RegistrationScope::Organization { organization })
        }
        "enterprises" => {
            // Enterprise landing URLs are not repository scopes; keep Unknown.
            None
        }
        other => {
            let repository = parts.next().unwrap_or("").to_owned();
            if repository.is_empty() {
                Some(RegistrationScope::Organization {
                    organization: other.to_owned(),
                })
            } else {
                Some(RegistrationScope::Repository {
                    owner: other.to_owned(),
                    repository,
                })
            }
        }
    }
}

/// Live Windows Service enumerator for official runner service names.
#[derive(Clone, Debug, Default)]
pub struct WindowsServiceDiscoverySource {
    last_status: std::sync::Arc<Mutex<DiscoveryEnumerationStatus>>,
}

impl ServiceDiscoverySource for WindowsServiceDiscoverySource {
    fn list_service_candidates(&self) -> Vec<ServiceCandidateFact> {
        #[cfg(windows)]
        {
            let (status, facts) = enumerate_actions_runner_services();
            *self
                .last_status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = status;
            facts
        }
        #[cfg(not(windows))]
        {
            *self
                .last_status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = DiscoveryEnumerationStatus::Ok;
            Vec::new()
        }
    }

    fn enumeration_status(&self) -> DiscoveryEnumerationStatus {
        *self
            .last_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(windows)]
fn enumerate_actions_runner_services() -> (DiscoveryEnumerationStatus, Vec<ServiceCandidateFact>) {
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::ERROR_MORE_DATA,
        System::Services::{
            CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW,
            ENUM_SERVICE_STATUS_PROCESSW, SC_ENUM_PROCESS_INFO, SC_MANAGER_CONNECT,
            SC_MANAGER_ENUMERATE_SERVICE, SERVICE_STATE_ALL, SERVICE_WIN32,
        },
    };

    unsafe {
        let scm = OpenSCManagerW(
            ptr::null(),
            ptr::null(),
            SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE,
        );
        if scm.is_null() {
            return (DiscoveryEnumerationStatus::Unavailable, Vec::new());
        }

        let mut bytes_needed: u32 = 0;
        let mut services_returned: u32 = 0;
        let mut resume: u32 = 0;
        let first = EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            ptr::null_mut(),
            0,
            &mut bytes_needed,
            &mut services_returned,
            &mut resume,
            ptr::null(),
        );
        let mut out = Vec::new();
        let mut incomplete = false;
        if first == 0 {
            let err = windows_sys::Win32::Foundation::GetLastError();
            if err != ERROR_MORE_DATA {
                CloseServiceHandle(scm);
                return (DiscoveryEnumerationStatus::Unavailable, Vec::new());
            }
        }
        let mut buffer = vec![0u8; bytes_needed as usize];
        resume = 0;
        let ok = EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            buffer.as_mut_ptr(),
            bytes_needed,
            &mut bytes_needed,
            &mut services_returned,
            &mut resume,
            ptr::null(),
        );
        if ok == 0 {
            CloseServiceHandle(scm);
            return (DiscoveryEnumerationStatus::Unavailable, Vec::new());
        }

        let statuses = buffer.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW;
        for index in 0..services_returned as usize {
            let entry = &*statuses.add(index);
            let name = wide_to_string(entry.lpServiceName);
            if !name.to_ascii_lowercase().starts_with(ACTIONS_RUNNER_PREFIX) {
                continue;
            }
            let display = wide_to_string(entry.lpDisplayName);
            let state = map_service_state(entry.ServiceStatusProcess);
            match service_fact_from_scm(scm, &name, Some(display), state) {
                Some(fact) => out.push(fact),
                None => incomplete = true,
            }
        }
        CloseServiceHandle(scm);
        let status = if incomplete {
            DiscoveryEnumerationStatus::Incomplete
        } else {
            DiscoveryEnumerationStatus::Ok
        };
        (status, out)
    }
}

#[cfg(windows)]
fn service_fact_from_scm(
    scm: windows_sys::Win32::System::Services::SC_HANDLE,
    service_name: &str,
    display_name: Option<String>,
    state: DiscoveredLifecycle,
) -> Option<ServiceCandidateFact> {
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenServiceW, QueryServiceConfigW, QUERY_SERVICE_CONFIGW,
        SERVICE_QUERY_CONFIG,
    };

    unsafe {
        let wide_name: Vec<u16> = service_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let service = OpenServiceW(scm, wide_name.as_ptr(), SERVICE_QUERY_CONFIG);
        if service.is_null() {
            return None;
        }
        let mut needed = 0u32;
        QueryServiceConfigW(service, ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            CloseServiceHandle(service);
            return None;
        }
        // QUERY_SERVICE_CONFIGW must be aligned; trailing strings live in the same buffer.
        let align = std::mem::align_of::<QUERY_SERVICE_CONFIGW>();
        let mut raw = vec![0u8; needed as usize + align];
        let unaligned = raw.as_mut_ptr() as usize;
        let aligned = (unaligned + align - 1) & !(align - 1);
        let config_ptr = aligned as *mut QUERY_SERVICE_CONFIGW;
        let ok = QueryServiceConfigW(service, config_ptr, needed, &mut needed);
        if ok == 0 {
            CloseServiceHandle(service);
            return None;
        }
        let config = &*config_ptr;
        let image = wide_to_string(config.lpBinaryPathName);
        let start_name = wide_to_string(config.lpServiceStartName);
        CloseServiceHandle(service);
        let image_path = PathBuf::from(image.trim_matches('"'));
        let runner_home = runner_home_from_image(&image_path)
            .unwrap_or_else(|| image_path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let meta = read_runner_metadata(&runner_home);
        let work_root = meta.as_ref().and_then(|m| {
            m.work_folder.as_ref().map(|folder| {
                let path = PathBuf::from(folder);
                if path.is_absolute() {
                    path
                } else {
                    runner_home.join(path)
                }
            })
        });
        // When the service account cannot be hashed, leave identity missing
        // rather than inventing current-user ownership for a Service runner.
        let execution_identity_ref = opaque_from_account_name(&start_name);
        Some(ServiceCandidateFact {
            service_name: service_name.to_owned(),
            display_name,
            image_path: Some(image_path),
            runner_home,
            work_root,
            registration_scope: meta.as_ref().and_then(|m| m.scope.clone()),
            runner_id: meta.as_ref().and_then(|m| m.agent_id),
            expected_runner_name: meta.as_ref().and_then(|m| m.agent_name.clone()),
            execution_identity_ref,
            state,
        })
    }
}

#[cfg(windows)]
fn runner_home_from_image(image_path: &Path) -> Option<PathBuf> {
    // Typical layout: <home>\bin\RunnerService.exe
    let parent = image_path.parent()?;
    if parent
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
    {
        return parent.parent().map(Path::to_path_buf);
    }
    Some(parent.to_path_buf())
}

#[cfg(windows)]
fn map_service_state(
    status: windows_sys::Win32::System::Services::SERVICE_STATUS_PROCESS,
) -> DiscoveredLifecycle {
    use windows_sys::Win32::System::Services::{
        SERVICE_PAUSED, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STOPPED,
        SERVICE_STOP_PENDING,
    };
    match status.dwCurrentState {
        SERVICE_RUNNING => DiscoveredLifecycle::Listening,
        SERVICE_STOPPED => DiscoveredLifecycle::Stopped,
        SERVICE_START_PENDING | SERVICE_STOP_PENDING | SERVICE_PAUSED => {
            DiscoveredLifecycle::Unknown
        }
        _ => DiscoveredLifecycle::Unknown,
    }
}

#[cfg(windows)]
unsafe fn wide_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn path_boundary_rejects_sibling_prefix_homes() {
        let home = normalize_path_key(Path::new(r"C:\runners\a"));
        assert!(path_is_under_home_key(
            &normalize_path_key(Path::new(r"C:\runners\a\bin\Runner.Listener.exe")),
            &home
        ));
        assert!(path_is_under_home_key(&home, &home));
        // Sibling `a` vs `abc` must not match via raw prefix.
        assert!(!path_is_under_home_key(
            &normalize_path_key(Path::new(r"C:\runners\abc\bin\Runner.Listener.exe")),
            &home
        ));
        assert!(!path_is_under_home_key(
            &normalize_path_key(Path::new(r"C:\runners\a-extra\bin\Runner.Listener.exe")),
            &home
        ));
        assert!(!path_is_under_home_key(
            &normalize_path_key(Path::new(r"C:\other\bin\Runner.Listener.exe")),
            &home
        ));
        assert!(!path_is_under_home_key(
            &normalize_path_key(Path::new(r"C:\runners\a\bin\Runner.Listener.exe")),
            ""
        ));
    }

    #[test]
    fn listener_worker_attribution_requires_bin_image_under_home() {
        let home = PathBuf::from(r"C:\fixture\runners\exact-home");
        let home_key = normalize_path_key(&home);
        let ok_listener = normalize_path_key(&home.join("bin").join("Runner.Listener.exe"));
        let sibling = normalize_path_key(Path::new(
            r"C:\fixture\runners\exact-home-sibling\bin\Runner.Listener.exe",
        ));
        let wrong_layout = normalize_path_key(&home.join("Runner.Listener.exe"));
        assert!(path_is_under_home_key(&ok_listener, &home_key));
        assert!(!path_is_under_home_key(&sibling, &home_key));
        assert!(path_is_under_home_key(&wrong_layout, &home_key));
        // Wrong layout under home is still filtered by the bin\ name suffix check
        // in observe_runner_processes_for_home.
        assert!(!wrong_layout.ends_with("\\bin\\runner.listener.exe"));
        assert!(ok_listener.ends_with("\\bin\\runner.listener.exe"));
    }

    #[test]
    fn configured_session_source_observes_explicit_homes_read_only() {
        let root = std::env::temp_dir().join(format!(
            "runnermesh-session-disc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = root.join("runner-home");
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(
            home.join(".runner"),
            b"\xEF\xBB\xBF{\"agentId\":7,\"agentName\":\"fixture\",\"gitHubUrl\":\"https://github.com/fixture-owner/fixture-repo\",\"workFolder\":\"_work\"}",
        )
        .unwrap();
        let source = ConfiguredSessionDiscoverySource::new(vec![home.clone()]);
        let facts = source.list_session_candidates();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].runner_id, Some(7));
        assert_eq!(
            facts[0].registration_scope,
            Some(RegistrationScope::Repository {
                owner: "fixture-owner".to_owned(),
                repository: "fixture-repo".to_owned(),
            })
        );
        // Binary on disk alone must not claim Listening.
        assert_ne!(facts[0].state, DiscoveredLifecycle::Listening);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scope_parser_accepts_org_landing_and_repo_urls() {
        assert_eq!(
            scope_from_github_url("https://github.com/fixture-org"),
            Some(RegistrationScope::Organization {
                organization: "fixture-org".to_owned()
            })
        );
        assert_eq!(
            scope_from_github_url("https://github.com/orgs/fixture-org"),
            Some(RegistrationScope::Organization {
                organization: "fixture-org".to_owned()
            })
        );
        assert_eq!(
            scope_from_github_url("https://github.com/fixture-owner/fixture-repo"),
            Some(RegistrationScope::Repository {
                owner: "fixture-owner".to_owned(),
                repository: "fixture-repo".to_owned()
            })
        );
        assert_eq!(
            scope_from_github_url("https://github.com/enterprises/fixture-ent"),
            None
        );
    }

    #[test]
    fn host_service_source_reports_status_not_false_empty_success() {
        let source = WindowsServiceDiscoverySource::default();
        let _ = source.list_service_candidates();
        let status = source.enumeration_status();
        // On a normal developer host this is Ok or Incomplete; Unavailable must
        // never be silently rewritten into an Ok empty success by callers.
        assert!(matches!(
            status,
            DiscoveryEnumerationStatus::Ok
                | DiscoveryEnumerationStatus::Incomplete
                | DiscoveryEnumerationStatus::Unavailable
        ));
        let report = discover_local_runners_read_only(Vec::new());
        let summary = PrivacySafeDiscoverySummary::from_report(&report);
        assert_eq!(summary.incomplete, report.is_incomplete());
        assert!(
            !report.is_false_empty_success()
                || summary.service_enumeration == DiscoveryEnumerationStatus::Unavailable
        );
    }

    #[test]
    fn privacy_safe_summary_omits_private_fields() {
        let report = DiscoveryReport::empty_ok();
        let summary = PrivacySafeDiscoverySummary::from_report(&report);
        assert_eq!(summary.candidate_count, 0);
        assert!(!summary.incomplete);
    }
}
