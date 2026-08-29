use std::collections::BTreeSet;

use crate::{ProbeHealth, ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode};

/// A normalized activity/workload evidence producer. Policy receives only the
/// returned snapshot, never provider-specific source types.
pub trait ActivityWorkloadProbe {
    fn id(&self) -> &ProbeId;
    fn observe(&mut self) -> ProbeSnapshot;
}

/// Source failure categories intentionally do not carry OS error text into the
/// stable machine contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeReadError {
    Unavailable,
    Failed,
}

/// Read-only source for the number of seconds since the last user input.
pub trait UserActivitySource {
    fn idle_seconds(&self) -> Result<Option<u64>, ProbeReadError>;
}

/// User Activity Probe uses a configurable threshold: recent input is Active,
/// while input older than the threshold is Inactive (idle/away evidence).
pub struct UserActivityProbe<S> {
    source: S,
    idle_threshold_seconds: u64,
    id: ProbeId,
}

impl<S> UserActivityProbe<S> {
    pub fn new(source: S, idle_threshold_seconds: u64) -> Self {
        Self {
            source,
            idle_threshold_seconds,
            id: static_probe_id("user-activity"),
        }
    }
}

impl<S: UserActivitySource> ActivityWorkloadProbe for UserActivityProbe<S> {
    fn id(&self) -> &ProbeId {
        &self.id
    }

    fn observe(&mut self) -> ProbeSnapshot {
        match self.source.idle_seconds() {
            Ok(Some(idle_seconds)) if idle_seconds >= self.idle_threshold_seconds => snapshot(
                &self.id,
                ProbeHealth::Healthy,
                ProbeRuntimeState::Inactive,
                "user-idle",
            ),
            Ok(Some(_)) => snapshot(
                &self.id,
                ProbeHealth::Healthy,
                ProbeRuntimeState::Active,
                "user-input-recent",
            ),
            Ok(None) | Err(ProbeReadError::Unavailable) => snapshot(
                &self.id,
                ProbeHealth::Unavailable,
                ProbeRuntimeState::Unavailable,
                "user-activity-unavailable",
            ),
            Err(ProbeReadError::Failed) => snapshot(
                &self.id,
                ProbeHealth::Degraded,
                ProbeRuntimeState::Unknown,
                "user-activity-read-failed",
            ),
        }
    }
}

/// Read-only source for Steam's current application identifier.
pub trait SteamAppIdSource {
    fn running_app_id(&self) -> Result<Option<u32>, ProbeReadError>;
}

/// Steam Game Probe distinguishes a launched Steam application from the Steam
/// client process. A non-zero `RunningAppID` is Active; `steam.exe` existence
/// alone is never queried or treated as activity evidence.
pub struct SteamGameProbe<S> {
    source: S,
    id: ProbeId,
}

impl<S> SteamGameProbe<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            id: static_probe_id("steam-game"),
        }
    }
}

impl<S: SteamAppIdSource> ActivityWorkloadProbe for SteamGameProbe<S> {
    fn id(&self) -> &ProbeId {
        &self.id
    }

    fn observe(&mut self) -> ProbeSnapshot {
        match self.source.running_app_id() {
            Ok(Some(app_id)) if app_id != 0 => snapshot(
                &self.id,
                ProbeHealth::Healthy,
                ProbeRuntimeState::Active,
                "steam-app-active",
            ),
            Ok(Some(_)) => snapshot(
                &self.id,
                ProbeHealth::Healthy,
                ProbeRuntimeState::Inactive,
                "steam-no-app-running",
            ),
            Ok(None) | Err(ProbeReadError::Unavailable) => snapshot(
                &self.id,
                ProbeHealth::Unavailable,
                ProbeRuntimeState::Unavailable,
                "steam-source-unavailable",
            ),
            Err(ProbeReadError::Failed) => snapshot(
                &self.id,
                ProbeHealth::Degraded,
                ProbeRuntimeState::Unknown,
                "steam-read-failed",
            ),
        }
    }
}

/// Read-only source for executable names observed on the current host.
pub trait ProcessSource {
    fn executable_names(&self) -> Result<Vec<String>, ProbeReadError>;
}

/// Configurable executable-name probe. RunnerMesh ships no game or application
/// database; owners explicitly supply the latency-sensitive executable names.
pub struct ProcessListProbe<S> {
    source: S,
    configured_names: BTreeSet<String>,
    id: ProbeId,
}

impl<S> ProcessListProbe<S> {
    pub fn new(source: S, configured_names: impl IntoIterator<Item = String>) -> Self {
        Self {
            source,
            configured_names: configured_names
                .into_iter()
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect(),
            id: static_probe_id("process-list"),
        }
    }
}

impl<S: ProcessSource> ActivityWorkloadProbe for ProcessListProbe<S> {
    fn id(&self) -> &ProbeId {
        &self.id
    }

    fn observe(&mut self) -> ProbeSnapshot {
        if self.configured_names.is_empty() {
            return snapshot(
                &self.id,
                ProbeHealth::Unavailable,
                ProbeRuntimeState::Unavailable,
                "process-list-not-configured",
            );
        }
        match self.source.executable_names() {
            Ok(names) => {
                let active = names
                    .iter()
                    .map(|name| name.trim().to_ascii_lowercase())
                    .any(|name| self.configured_names.contains(&name));
                if active {
                    snapshot(
                        &self.id,
                        ProbeHealth::Healthy,
                        ProbeRuntimeState::Active,
                        "process-list-match",
                    )
                } else {
                    snapshot(
                        &self.id,
                        ProbeHealth::Healthy,
                        ProbeRuntimeState::Inactive,
                        "process-list-no-match",
                    )
                }
            }
            Err(ProbeReadError::Unavailable) => snapshot(
                &self.id,
                ProbeHealth::Unavailable,
                ProbeRuntimeState::Unavailable,
                "process-list-unavailable",
            ),
            Err(ProbeReadError::Failed) => snapshot(
                &self.id,
                ProbeHealth::Degraded,
                ProbeRuntimeState::Unknown,
                "process-list-read-failed",
            ),
        }
    }
}

/// Windows read-only implementation based on `GetLastInputInfo`.
#[derive(Default)]
pub struct WindowsUserActivitySource;

impl UserActivitySource for WindowsUserActivitySource {
    fn idle_seconds(&self) -> Result<Option<u64>, ProbeReadError> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                System::SystemInformation::GetTickCount64,
                UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
            };

            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if unsafe { GetLastInputInfo(&mut info) } == 0 {
                Err(ProbeReadError::Failed)
            } else {
                let current_low = unsafe { GetTickCount64() } as u32;
                let idle_millis = current_low.wrapping_sub(info.dwTime) as u64;
                Ok(Some(idle_millis / 1_000))
            }
        }

        #[cfg(not(windows))]
        Ok(None)
    }
}

/// Windows read-only implementation of Steam's local `RunningAppID` source.
#[derive(Default)]
pub struct WindowsSteamAppIdSource;

impl SteamAppIdSource for WindowsSteamAppIdSource {
    fn running_app_id(&self) -> Result<Option<u32>, ProbeReadError> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND},
                System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
            };

            let subkey = wide("Software\\Valve\\Steam");
            let value = wide("RunningAppID");
            let mut app_id = 0_u32;
            let mut bytes = std::mem::size_of::<u32>() as u32;
            let result = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_DWORD,
                    std::ptr::null_mut(),
                    (&mut app_id as *mut u32).cast(),
                    &mut bytes,
                )
            };
            if result == 0 {
                Ok(Some(app_id))
            } else if result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND {
                Ok(None)
            } else {
                Err(ProbeReadError::Failed)
            }
        }

        #[cfg(not(windows))]
        Ok(None)
    }
}

/// Windows read-only process source. It reads the process snapshot without
/// creating a child process and does not mutate process state.
#[derive(Default)]
pub struct WindowsProcessSource;

impl ProcessSource for WindowsProcessSource {
    fn executable_names(&self) -> Result<Vec<String>, ProbeReadError> {
        #[cfg(windows)]
        {
            crate::process_snapshot::executable_names().map_err(|error| match error {
                crate::process_snapshot::ProcessSnapshotError::Unavailable => {
                    ProbeReadError::Unavailable
                }
                crate::process_snapshot::ProcessSnapshotError::Failed => ProbeReadError::Failed,
            })
        }

        #[cfg(not(windows))]
        Err(ProbeReadError::Unavailable)
    }
}

fn snapshot(
    id: &ProbeId,
    health: ProbeHealth,
    runtime_state: ProbeRuntimeState,
    reason: &'static str,
) -> ProbeSnapshot {
    ProbeSnapshot {
        id: id.clone(),
        enabled: true,
        health,
        runtime_state,
        reason_code: Some(static_reason(reason)),
    }
}

fn static_probe_id(value: &'static str) -> ProbeId {
    ProbeId::new(value).expect("static probe IDs must be valid")
}

fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static probe reason codes must be valid")
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ActivityWorkloadProbe, ProbeReadError, ProcessListProbe, ProcessSource, SteamAppIdSource,
        SteamGameProbe, UserActivityProbe, UserActivitySource,
    };
    use crate::{ProbeHealth, ProbeRuntimeState};

    #[test]
    fn user_activity_fixture_preserves_active_inactive_unknown_and_unavailable() {
        assert_state(
            UserActivityProbe::new(FakeIdle(Ok(Some(20))), 300).observe(),
            ProbeHealth::Healthy,
            ProbeRuntimeState::Active,
        );
        assert_state(
            UserActivityProbe::new(FakeIdle(Ok(Some(300))), 300).observe(),
            ProbeHealth::Healthy,
            ProbeRuntimeState::Inactive,
        );
        assert_state(
            UserActivityProbe::new(FakeIdle(Err(ProbeReadError::Failed)), 300).observe(),
            ProbeHealth::Degraded,
            ProbeRuntimeState::Unknown,
        );
        assert_state(
            UserActivityProbe::new(FakeIdle(Ok(None)), 300).observe(),
            ProbeHealth::Unavailable,
            ProbeRuntimeState::Unavailable,
        );
    }

    #[test]
    fn steam_fixture_uses_app_id_not_client_presence() {
        assert_state(
            SteamGameProbe::new(FakeSteam(Ok(Some(42)))).observe(),
            ProbeHealth::Healthy,
            ProbeRuntimeState::Active,
        );
        assert_state(
            SteamGameProbe::new(FakeSteam(Ok(Some(0)))).observe(),
            ProbeHealth::Healthy,
            ProbeRuntimeState::Inactive,
        );
        assert_state(
            SteamGameProbe::new(FakeSteam(Ok(None))).observe(),
            ProbeHealth::Unavailable,
            ProbeRuntimeState::Unavailable,
        );
    }

    #[test]
    fn process_list_is_configured_and_case_insensitive_without_database() {
        assert_state(
            ProcessListProbe::new(
                FakeProcesses(Ok(vec!["LatencyTool.EXE".to_owned()])),
                ["latencytool.exe".to_owned()],
            )
            .observe(),
            ProbeHealth::Healthy,
            ProbeRuntimeState::Active,
        );
        assert_state(
            ProcessListProbe::new(FakeProcesses(Ok(Vec::new())), Vec::new()).observe(),
            ProbeHealth::Unavailable,
            ProbeRuntimeState::Unavailable,
        );
        assert_state(
            ProcessListProbe::new(
                FakeProcesses(Err(ProbeReadError::Failed)),
                ["latencytool.exe".to_owned()],
            )
            .observe(),
            ProbeHealth::Degraded,
            ProbeRuntimeState::Unknown,
        );
    }

    struct FakeIdle(Result<Option<u64>, ProbeReadError>);

    impl UserActivitySource for FakeIdle {
        fn idle_seconds(&self) -> Result<Option<u64>, ProbeReadError> {
            self.0
        }
    }

    struct FakeSteam(Result<Option<u32>, ProbeReadError>);

    impl SteamAppIdSource for FakeSteam {
        fn running_app_id(&self) -> Result<Option<u32>, ProbeReadError> {
            self.0
        }
    }

    struct FakeProcesses(Result<Vec<String>, ProbeReadError>);

    impl ProcessSource for FakeProcesses {
        fn executable_names(&self) -> Result<Vec<String>, ProbeReadError> {
            self.0.clone()
        }
    }

    fn assert_state(
        snapshot: crate::ProbeSnapshot,
        health: ProbeHealth,
        runtime_state: ProbeRuntimeState,
    ) {
        assert_eq!(snapshot.health, health);
        assert_eq!(snapshot.runtime_state, runtime_state);
    }
}
