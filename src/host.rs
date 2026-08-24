use crate::{
    AgentHealth, ExecutionIdentityEvidence, LinkState, OwnershipEvidence, ProcessOwnership,
    ReasonCode, RunnerPhase, SupervisorObservation,
};

/// User-session state observed from the local host. It is evidence only; no
/// session switching, locking, or sign-in mutation is performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    Active,
    Inactive,
    Unknown,
    Unavailable,
}

/// Raw read-only host facts. `None` means the metric was not available from
/// the source, rather than a zero resource value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostEvidence {
    pub cpu_percent: Option<u8>,
    pub memory_available_bytes: Option<u64>,
    pub user_idle_seconds: Option<u64>,
    pub session: SessionState,
}

/// Normalized Agent health signal derived from host observation, with a stable
/// reason code appropriate for snapshots and doctor output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostHealth {
    pub health: AgentHealth,
    pub reason_code: ReasonCode,
}

/// Read-only host source boundary.
pub trait HostSource {
    fn collect(&mut self) -> HostEvidence;
}

/// A normalized host snapshot. CPU is sampled from two system-time readings;
/// the first read therefore deliberately reports no CPU percentage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostSnapshot {
    pub cpu_percent: Option<u8>,
    pub memory_available_bytes: Option<u64>,
    pub user_idle_seconds: Option<u64>,
    pub session: SessionState,
    pub health: HostHealth,
}

impl HostSnapshot {
    pub fn from_evidence(evidence: HostEvidence) -> Self {
        let health = if evidence.session == SessionState::Unavailable {
            HostHealth {
                health: AgentHealth::Degraded,
                reason_code: static_reason("host-session-unavailable"),
            }
        } else if evidence.cpu_percent.is_some()
            && evidence.memory_available_bytes.is_some()
            && evidence.user_idle_seconds.is_some()
            && matches!(
                evidence.session,
                SessionState::Active | SessionState::Inactive
            )
        {
            HostHealth {
                health: AgentHealth::Healthy,
                reason_code: static_reason("host-observed"),
            }
        } else {
            HostHealth {
                health: AgentHealth::Degraded,
                reason_code: static_reason("host-observation-incomplete"),
            }
        };

        Self {
            cpu_percent: evidence.cpu_percent,
            memory_available_bytes: evidence.memory_available_bytes,
            user_idle_seconds: evidence.user_idle_seconds,
            session: evidence.session,
            health,
        }
    }
}

/// Read-only Windows implementation. It samples system times and memory APIs,
/// the current session relationship, and the existing user-input source. It
/// never changes process priority, power policy, sessions, or runner state.
#[derive(Debug, Default)]
pub struct WindowsHostSource {
    previous_times: Option<SystemTimes>,
}

impl HostSource for WindowsHostSource {
    fn collect(&mut self) -> HostEvidence {
        let current_times = read_system_times();
        let cpu_percent = current_times.and_then(|current| {
            let previous = self.previous_times.replace(current)?;
            cpu_percent_between(previous, current)
        });

        HostEvidence {
            cpu_percent,
            memory_available_bytes: read_available_memory(),
            user_idle_seconds: read_user_idle_seconds(),
            session: read_session_state(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SystemTimes {
    idle: u64,
    kernel: u64,
    user: u64,
}

fn cpu_percent_between(previous: SystemTimes, current: SystemTimes) -> Option<u8> {
    let previous_total = previous.kernel.checked_add(previous.user)?;
    let current_total = current.kernel.checked_add(current.user)?;
    let total_delta = current_total.checked_sub(previous_total)?;
    let idle_delta = current.idle.checked_sub(previous.idle)?;
    if total_delta == 0 || idle_delta > total_delta {
        return None;
    }
    let busy = total_delta - idle_delta;
    let percent = busy.saturating_mul(100) / total_delta;
    Some(percent.min(100) as u8)
}

#[cfg(windows)]
fn read_system_times() -> Option<SystemTimes> {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetSystemTimes};

    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let result = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    (result != 0).then(|| SystemTimes {
        idle: filetime_to_u64(idle),
        kernel: filetime_to_u64(kernel),
        user: filetime_to_u64(user),
    })
}

#[cfg(windows)]
fn filetime_to_u64(value: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

#[cfg(not(windows))]
fn read_system_times() -> Option<SystemTimes> {
    None
}

#[cfg(windows)]
fn read_available_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let result = unsafe { GlobalMemoryStatusEx(&mut status) };
    (result != 0).then_some(status.ullAvailPhys)
}

#[cfg(not(windows))]
fn read_available_memory() -> Option<u64> {
    None
}

#[cfg(windows)]
fn read_user_idle_seconds() -> Option<u64> {
    use crate::{UserActivitySource, WindowsUserActivitySource};

    WindowsUserActivitySource.idle_seconds().ok().flatten()
}

#[cfg(not(windows))]
fn read_user_idle_seconds() -> Option<u64> {
    None
}

#[cfg(windows)]
fn read_session_state() -> SessionState {
    use windows_sys::Win32::System::RemoteDesktop::{
        ProcessIdToSessionId, WTSGetActiveConsoleSessionId,
    };

    let mut current_session = 0;
    let found = unsafe { ProcessIdToSessionId(std::process::id(), &mut current_session) };
    if found == 0 {
        return SessionState::Unknown;
    }
    let active_console = unsafe { WTSGetActiveConsoleSessionId() };
    if active_console == u32::MAX {
        SessionState::Unknown
    } else if current_session == active_console {
        SessionState::Active
    } else {
        SessionState::Inactive
    }
}

#[cfg(not(windows))]
fn read_session_state() -> SessionState {
    SessionState::Unavailable
}

/// Lifecycle events that require transient host/runner state to be observed
/// again. The recovered state is never loaded from persisted configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryTrigger {
    AgentRestart,
    SystemResume,
    ConnectionInterrupted,
}

/// A non-mutating follow-up direction emitted by reconstruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryDirective {
    Reobserve,
    ReobserveThenReconnect,
}

/// Result of validating an existing Listener for recovery adoption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExistingListenerAdoption {
    Adopt,
    NoListener,
    Refused(AdoptionRefusal),
}

/// Specific non-adoption reasons. These are not process-control commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdoptionRefusal {
    ProcessOwnershipAmbiguous,
    ForeignProcessObserved,
    ExecutionIdentityNotVerified,
    ExecutionIdentityMismatch,
    RunnerHomeNotVerified,
    RunnerHomeNotOwned,
    WorkRootNotVerified,
    WorkRootNotOwned,
    InconsistentRunnerPhase,
}

/// Fresh evidence collected during recovery. It is intentionally separate from
/// persisted Agent intent/configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryObservation {
    pub host: HostEvidence,
    pub runner: SupervisorObservation,
    pub runner_home: OwnershipEvidence,
    pub github_link: LinkState,
}

/// Recovery source boundary for synthetic tests and future read-only platform
/// composition.
pub trait RecoverySource {
    fn collect(&mut self) -> RecoveryObservation;
}

/// Reconstructed transient state after restart, resume, or a connection
/// interruption. It reports plans only; it does not invoke the Supervisor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverySnapshot {
    pub trigger: RecoveryTrigger,
    pub host: HostSnapshot,
    pub agent_health: AgentHealth,
    pub health_reason_code: ReasonCode,
    pub existing_listener_adoption: ExistingListenerAdoption,
    pub directive: RecoveryDirective,
}

/// Rebuilds transient observations from its source each time a recovery event
/// occurs. No cached process/link/host state survives an Agent restart.
pub struct RecoveryReconstructor<S> {
    source: S,
}

impl<S> RecoveryReconstructor<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub fn source(&self) -> &S {
        &self.source
    }
}

impl<S: RecoverySource> RecoveryReconstructor<S> {
    pub fn reconstruct(&mut self, trigger: RecoveryTrigger) -> RecoverySnapshot {
        let evidence = self.source.collect();
        reconstruct(trigger, evidence)
    }
}

fn reconstruct(trigger: RecoveryTrigger, evidence: RecoveryObservation) -> RecoverySnapshot {
    let host = HostSnapshot::from_evidence(evidence.host);
    let directive = match (trigger, evidence.github_link) {
        (RecoveryTrigger::ConnectionInterrupted, LinkState::Disconnected) => {
            RecoveryDirective::ReobserveThenReconnect
        }
        _ => RecoveryDirective::Reobserve,
    };

    RecoverySnapshot {
        trigger,
        agent_health: host.health.health,
        health_reason_code: host.health.reason_code.clone(),
        existing_listener_adoption: decide_existing_listener_adoption(
            evidence.runner,
            evidence.runner_home,
        ),
        directive,
        host,
    }
}

fn decide_existing_listener_adoption(
    runner: SupervisorObservation,
    runner_home: OwnershipEvidence,
) -> ExistingListenerAdoption {
    if runner.listener == ProcessOwnership::Absent {
        return ExistingListenerAdoption::NoListener;
    }
    if [runner.listener, runner.worker].contains(&ProcessOwnership::Foreign) {
        return ExistingListenerAdoption::Refused(AdoptionRefusal::ForeignProcessObserved);
    }
    if [runner.listener, runner.worker].contains(&ProcessOwnership::Unknown) {
        return ExistingListenerAdoption::Refused(AdoptionRefusal::ProcessOwnershipAmbiguous);
    }
    match runner.execution_identity {
        ExecutionIdentityEvidence::Verified => {}
        ExecutionIdentityEvidence::Mismatch => {
            return ExistingListenerAdoption::Refused(AdoptionRefusal::ExecutionIdentityMismatch)
        }
        ExecutionIdentityEvidence::Unknown => {
            return ExistingListenerAdoption::Refused(AdoptionRefusal::ExecutionIdentityNotVerified)
        }
    }
    match runner_home {
        OwnershipEvidence::Verified => {}
        OwnershipEvidence::NotOwned => {
            return ExistingListenerAdoption::Refused(AdoptionRefusal::RunnerHomeNotOwned)
        }
        OwnershipEvidence::Unknown => {
            return ExistingListenerAdoption::Refused(AdoptionRefusal::RunnerHomeNotVerified)
        }
    }
    match runner.work_root {
        OwnershipEvidence::Verified => {}
        OwnershipEvidence::NotOwned => {
            return ExistingListenerAdoption::Refused(AdoptionRefusal::WorkRootNotOwned)
        }
        OwnershipEvidence::Unknown => {
            return ExistingListenerAdoption::Refused(AdoptionRefusal::WorkRootNotVerified)
        }
    }
    if matches!(
        runner.phase,
        RunnerPhase::Listening | RunnerPhase::Busy | RunnerPhase::DrainPending
    ) {
        ExistingListenerAdoption::Adopt
    } else {
        ExistingListenerAdoption::Refused(AdoptionRefusal::InconsistentRunnerPhase)
    }
}

fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static host reason codes must be valid")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{
        AdoptionRefusal, ExistingListenerAdoption, HostEvidence, HostSnapshot, RecoveryDirective,
        RecoveryObservation, RecoveryReconstructor, RecoverySource, RecoveryTrigger, SessionState,
    };
    use crate::{
        ExecutionIdentityEvidence, LinkState, OwnershipEvidence, ProcessOwnership, RunnerPhase,
        SupervisorObservation,
    };

    #[test]
    fn complete_host_evidence_produces_snapshot_health_and_reason() {
        let snapshot = HostSnapshot::from_evidence(host_evidence());

        assert_eq!(snapshot.health.health, crate::AgentHealth::Healthy);
        assert_eq!(snapshot.health.reason_code.as_str(), "host-observed");
    }

    #[test]
    fn incomplete_or_unavailable_host_evidence_is_degraded() {
        let mut incomplete = host_evidence();
        incomplete.cpu_percent = None;
        let incomplete_snapshot = HostSnapshot::from_evidence(incomplete);
        assert_eq!(
            incomplete_snapshot.health.health,
            crate::AgentHealth::Degraded
        );
        assert_eq!(
            incomplete_snapshot.health.reason_code.as_str(),
            "host-observation-incomplete"
        );

        let unavailable = HostSnapshot::from_evidence(HostEvidence {
            session: SessionState::Unavailable,
            ..host_evidence()
        });
        assert_eq!(
            unavailable.health.reason_code.as_str(),
            "host-session-unavailable"
        );
    }

    #[test]
    fn restart_reconstructs_fresh_observation_and_adopts_only_verified_listener() {
        let source = QueueRecoverySource::new(vec![RecoveryObservation {
            host: host_evidence(),
            runner: runner(RunnerPhase::Listening),
            runner_home: OwnershipEvidence::Verified,
            github_link: LinkState::Connected,
        }]);
        let mut reconstructor = RecoveryReconstructor::new(source);

        let recovered = reconstructor.reconstruct(RecoveryTrigger::AgentRestart);
        assert_eq!(recovered.agent_health, crate::AgentHealth::Healthy);
        assert_eq!(recovered.health_reason_code.as_str(), "host-observed");
        assert_eq!(
            recovered.existing_listener_adoption,
            ExistingListenerAdoption::Adopt
        );
        assert_eq!(recovered.directive, RecoveryDirective::Reobserve);
    }

    #[test]
    fn resume_refuses_ambiguous_home_and_interruption_only_directs_reconnect() {
        let source = QueueRecoverySource::new(vec![
            RecoveryObservation {
                host: host_evidence(),
                runner: runner(RunnerPhase::Listening),
                runner_home: OwnershipEvidence::Unknown,
                github_link: LinkState::Connected,
            },
            RecoveryObservation {
                host: host_evidence(),
                runner: runner(RunnerPhase::Busy),
                runner_home: OwnershipEvidence::Verified,
                github_link: LinkState::Disconnected,
            },
        ]);
        let mut reconstructor = RecoveryReconstructor::new(source);

        let resumed = reconstructor.reconstruct(RecoveryTrigger::SystemResume);
        assert_eq!(
            resumed.existing_listener_adoption,
            ExistingListenerAdoption::Refused(AdoptionRefusal::RunnerHomeNotVerified)
        );

        let interrupted = reconstructor.reconstruct(RecoveryTrigger::ConnectionInterrupted);
        assert_eq!(
            interrupted.directive,
            RecoveryDirective::ReobserveThenReconnect
        );
        assert_eq!(
            interrupted.existing_listener_adoption,
            ExistingListenerAdoption::Adopt
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_source_collects_read_only_host_evidence() {
        use super::{HostSource, WindowsHostSource};

        let mut source = WindowsHostSource::default();
        let evidence = source.collect();
        assert!(matches!(
            evidence.session,
            SessionState::Active | SessionState::Inactive | SessionState::Unknown
        ));
    }

    fn host_evidence() -> HostEvidence {
        HostEvidence {
            cpu_percent: Some(42),
            memory_available_bytes: Some(8 * 1024 * 1024 * 1024),
            user_idle_seconds: Some(600),
            session: SessionState::Active,
        }
    }

    fn runner(phase: RunnerPhase) -> SupervisorObservation {
        SupervisorObservation {
            phase,
            listener: ProcessOwnership::Owned,
            worker: if phase == RunnerPhase::Busy {
                ProcessOwnership::Owned
            } else {
                ProcessOwnership::Absent
            },
            execution_identity: ExecutionIdentityEvidence::Verified,
            work_root: OwnershipEvidence::Verified,
        }
    }

    struct QueueRecoverySource {
        observations: VecDeque<RecoveryObservation>,
    }

    impl QueueRecoverySource {
        fn new(observations: Vec<RecoveryObservation>) -> Self {
            Self {
                observations: observations.into(),
            }
        }
    }

    impl RecoverySource for QueueRecoverySource {
        fn collect(&mut self) -> RecoveryObservation {
            self.observations
                .pop_front()
                .expect("a synthetic recovery observation is required")
        }
    }
}
