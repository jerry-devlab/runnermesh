//! Windows read-only discovery adapters for official GitHub Actions runners.
//!
//! Enumerates `actions.runner.*` Windows Services and observes explicitly
//! configured session homes. Never starts/stops services, never mutates
//! registration, and never reads credential files.

use std::path::{Path, PathBuf};

use crate::{
    DiscoveredLifecycle, DiscoveryIdentityFilter, DiscoveryReport, EmptyUnsupportedDiscoverySource,
    RegistrationScope, RunnerDiscoveryInventory, ServiceCandidateFact, ServiceDiscoverySource,
    SessionCandidateFact, SessionDiscoverySource,
};

/// Runs the pure inventory over live Service enumeration plus optional session homes.
pub fn discover_local_runners_read_only(session_homes: Vec<PathBuf>) -> DiscoveryReport {
    let inventory = RunnerDiscoveryInventory::new(
        WindowsServiceDiscoverySource,
        ConfiguredSessionDiscoverySource::new(session_homes),
        EmptyUnsupportedDiscoverySource,
        DiscoveryIdentityFilter {
            expected_execution_identity: None,
        },
    );
    inventory.discover()
}

#[cfg(windows)]
const ACTIONS_RUNNER_PREFIX: &str = "actions.runner.";

/// Builds a discovery inventory that uses live Windows Service enumeration when
/// compiled for Windows; non-Windows targets get empty service results.
pub fn host_service_discovery_source() -> Box<dyn ServiceDiscoverySource> {
    #[cfg(windows)]
    {
        Box::new(WindowsServiceDiscoverySource)
    }
    #[cfg(not(windows))]
    {
        Box::new(crate::EmptyServiceDiscoverySource)
    }
}

/// Session discovery over explicitly configured runner homes (read-only).
#[derive(Clone, Default)]
pub struct ConfiguredSessionDiscoverySource {
    homes: Vec<PathBuf>,
}

impl ConfiguredSessionDiscoverySource {
    pub fn new(homes: Vec<PathBuf>) -> Self {
        Self { homes }
    }
}

impl SessionDiscoverySource for ConfiguredSessionDiscoverySource {
    fn list_session_candidates(&self) -> Vec<SessionCandidateFact> {
        self.homes
            .iter()
            .filter_map(|home| observe_session_home(home))
            .collect()
    }
}

fn observe_session_home(runner_home: &Path) -> Option<SessionCandidateFact> {
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
    let listener_present = runner_home
        .join("bin")
        .join("Runner.Listener.exe")
        .is_file();
    // Presence of binary image is not proof the process is running; keep state
    // Unknown unless metadata + home are coherent enough for ObservedOnly.
    let state = if meta.is_some() {
        DiscoveredLifecycle::Stopped
    } else {
        DiscoveredLifecycle::Unknown
    };
    Some(SessionCandidateFact {
        runner_home: runner_home.to_path_buf(),
        work_root,
        registration_scope: meta.as_ref().and_then(|m| m.scope.clone()),
        runner_id: meta.as_ref().and_then(|m| m.agent_id),
        expected_runner_name: meta.as_ref().and_then(|m| m.agent_name.clone()),
        execution_identity_ref: None,
        current_user_owned: true,
        listener_present,
        worker_present: false,
        state,
    })
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
    let trimmed = url.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    // https://github.com/{owner}/{repo} or org URL shapes.
    if parts.len() >= 5 && parts[2].eq_ignore_ascii_case("github.com") {
        let owner = parts[3].to_owned();
        if parts.len() >= 5 && !parts[4].is_empty() {
            return Some(RegistrationScope::Repository {
                owner,
                repository: parts[4].to_owned(),
            });
        }
        return Some(RegistrationScope::Organization {
            organization: owner,
        });
    }
    None
}

/// Live Windows Service enumerator for official runner service names.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsServiceDiscoverySource;

impl ServiceDiscoverySource for WindowsServiceDiscoverySource {
    fn list_service_candidates(&self) -> Vec<ServiceCandidateFact> {
        #[cfg(windows)]
        {
            enumerate_actions_runner_services()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

#[cfg(windows)]
fn enumerate_actions_runner_services() -> Vec<ServiceCandidateFact> {
    use std::{mem, ptr};
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
            return Vec::new();
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
        if first == 0 {
            let err = windows_sys::Win32::Foundation::GetLastError();
            if err != ERROR_MORE_DATA {
                CloseServiceHandle(scm);
                return Vec::new();
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
            return Vec::new();
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
            if let Some(fact) = service_fact_from_scm(scm, &name, Some(display), state) {
                out.push(fact);
            }
        }
        CloseServiceHandle(scm);
        let _ = mem::size_of::<u8>();
        out
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
            return Some(ServiceCandidateFact {
                service_name: service_name.to_owned(),
                display_name,
                image_path: None,
                runner_home: PathBuf::from(format!(r"C:\unknown\{service_name}")),
                work_root: None,
                registration_scope: None,
                runner_id: None,
                expected_runner_name: None,
                execution_identity_ref: None,
                state: DiscoveredLifecycle::Unknown,
            });
        }
        let mut needed = 0u32;
        QueryServiceConfigW(service, ptr::null_mut(), 0, &mut needed);
        let mut buffer = vec![0u8; needed as usize];
        let ok = QueryServiceConfigW(
            service,
            buffer.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW,
            needed,
            &mut needed,
        );
        CloseServiceHandle(service);
        if ok == 0 {
            return None;
        }
        let config = &*(buffer.as_ptr() as *const QUERY_SERVICE_CONFIGW);
        let image = wide_to_string(config.lpBinaryPathName);
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
        Some(ServiceCandidateFact {
            service_name: service_name.to_owned(),
            display_name,
            image_path: Some(image_path),
            runner_home,
            work_root,
            registration_scope: meta.as_ref().and_then(|m| m.scope.clone()),
            runner_id: meta.as_ref().and_then(|m| m.agent_id),
            expected_runner_name: meta.as_ref().and_then(|m| m.agent_name.clone()),
            execution_identity_ref: None,
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
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn host_service_source_is_constructible_on_all_targets() {
        let _ = host_service_discovery_source().list_service_candidates();
    }
}
