use std::fmt;

use crate::{ExecutionIdentityEvidence, OwnershipEvidence, RunnerPhase};

/// Ownership classification for a listener or worker process fixture. This is
/// deliberately more specific than process existence: an unknown or foreign
/// process is never an authority to control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOwnership {
    Absent,
    Owned,
    Foreign,
    Unknown,
}

/// Synthetic facts about a runner process tree. G09 consumes fixtures through
/// this boundary; it does not discover, start, or stop a real process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisorObservation {
    pub phase: RunnerPhase,
    pub listener: ProcessOwnership,
    pub worker: ProcessOwnership,
    pub execution_identity: ExecutionIdentityEvidence,
    pub work_root: OwnershipEvidence,
}

/// Requested lifecycle intent. These values express Supervisor semantics only;
/// a future qualified backend is responsible for any actual host operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorRequest {
    EnsureStarted,
    EnsureDrained,
    EnsureStopped,
    Restart,
    Reconnect,
    Adopt,
}

/// An action that a deterministic synthetic backend may record. No production
/// backend is supplied by this Goal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorAction {
    Start,
    RequestDrain,
    Stop,
    StopAfterDrain,
    RestartConnection,
    AdoptExistingListener,
}

/// A reason a lifecycle request was deliberately not planned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorRefusal {
    ProcessOwnershipAmbiguous,
    ForeignProcessObserved,
    ExecutionIdentityNotVerified,
    ExecutionIdentityMismatch,
    WorkRootNotVerified,
    WorkRootNotOwned,
    NoListenerToAdopt,
}

/// Deterministic result of reconciling a request with synthetic observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorOutcome {
    Applied(Vec<SupervisorAction>),
    Noop,
    Refused(SupervisorRefusal),
}

/// Fixture-only executor seam. Implementations are expected to record actions
/// or simulate their effect; they are not a real runner-control interface.
pub trait SyntheticProcessBackend {
    fn apply(&mut self, action: SupervisorAction) -> Result<(), String>;
}

/// Applies deterministic supervisor plans to a synthetic backend. The type
/// intentionally has no Windows process, service, registration, or work-root
/// implementation, keeping real lifecycle control behind the H1 gate.
pub struct SupervisorCore<B> {
    backend: B,
}

impl<B> SupervisorCore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }
}

impl<B: SyntheticProcessBackend> SupervisorCore<B> {
    /// Reconciles intent against a fixture. Refused and no-op outcomes never
    /// invoke the backend, which makes ambiguity and idempotence explicit.
    pub fn reconcile(
        &mut self,
        request: SupervisorRequest,
        observed: SupervisorObservation,
    ) -> Result<SupervisorOutcome, SupervisorError> {
        let outcome = plan(request, observed);
        if let SupervisorOutcome::Applied(actions) = &outcome {
            for action in actions {
                self.backend
                    .apply(*action)
                    .map_err(SupervisorError::Backend)?;
            }
        }
        Ok(outcome)
    }
}

/// Error returned only by the explicit synthetic backend seam.
#[derive(Debug)]
pub enum SupervisorError {
    Backend(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "synthetic backend failed: {error}"),
        }
    }
}

impl std::error::Error for SupervisorError {}

fn plan(request: SupervisorRequest, observed: SupervisorObservation) -> SupervisorOutcome {
    match request {
        SupervisorRequest::EnsureStopped if is_stopped(&observed) => SupervisorOutcome::Noop,
        SupervisorRequest::EnsureDrained if is_drain_ready(&observed) => SupervisorOutcome::Noop,
        _ => match control_refusal(&observed) {
            Some(refusal) => SupervisorOutcome::Refused(refusal),
            None => plan_verified(request, observed),
        },
    }
}

fn plan_verified(request: SupervisorRequest, observed: SupervisorObservation) -> SupervisorOutcome {
    let actions = match request {
        SupervisorRequest::EnsureStarted => match observed.phase {
            RunnerPhase::Stopped => vec![SupervisorAction::Start],
            RunnerPhase::Starting | RunnerPhase::Listening | RunnerPhase::Busy => {
                return SupervisorOutcome::Noop
            }
            RunnerPhase::DrainPending | RunnerPhase::Stopping | RunnerPhase::Unknown => {
                return SupervisorOutcome::Refused(SupervisorRefusal::ProcessOwnershipAmbiguous)
            }
        },
        SupervisorRequest::EnsureDrained => match observed.phase {
            RunnerPhase::Busy => vec![SupervisorAction::RequestDrain],
            RunnerPhase::Stopped | RunnerPhase::Listening => return SupervisorOutcome::Noop,
            RunnerPhase::Starting
            | RunnerPhase::DrainPending
            | RunnerPhase::Stopping
            | RunnerPhase::Unknown => {
                return SupervisorOutcome::Refused(SupervisorRefusal::ProcessOwnershipAmbiguous)
            }
        },
        SupervisorRequest::EnsureStopped => match observed.phase {
            RunnerPhase::Busy => vec![
                SupervisorAction::RequestDrain,
                SupervisorAction::StopAfterDrain,
            ],
            RunnerPhase::Listening => vec![SupervisorAction::Stop],
            RunnerPhase::Stopped => return SupervisorOutcome::Noop,
            RunnerPhase::Starting
            | RunnerPhase::DrainPending
            | RunnerPhase::Stopping
            | RunnerPhase::Unknown => {
                return SupervisorOutcome::Refused(SupervisorRefusal::ProcessOwnershipAmbiguous)
            }
        },
        SupervisorRequest::Restart => match observed.phase {
            RunnerPhase::Stopped => vec![SupervisorAction::Start],
            RunnerPhase::Listening => vec![SupervisorAction::Stop, SupervisorAction::Start],
            RunnerPhase::Busy => vec![
                SupervisorAction::RequestDrain,
                SupervisorAction::StopAfterDrain,
                SupervisorAction::Start,
            ],
            RunnerPhase::Starting
            | RunnerPhase::DrainPending
            | RunnerPhase::Stopping
            | RunnerPhase::Unknown => {
                return SupervisorOutcome::Refused(SupervisorRefusal::ProcessOwnershipAmbiguous)
            }
        },
        SupervisorRequest::Reconnect => match observed.phase {
            RunnerPhase::Listening | RunnerPhase::Busy => vec![SupervisorAction::RestartConnection],
            RunnerPhase::Stopped => vec![SupervisorAction::Start],
            RunnerPhase::Starting
            | RunnerPhase::DrainPending
            | RunnerPhase::Stopping
            | RunnerPhase::Unknown => {
                return SupervisorOutcome::Refused(SupervisorRefusal::ProcessOwnershipAmbiguous)
            }
        },
        SupervisorRequest::Adopt => {
            if observed.listener == ProcessOwnership::Absent {
                return SupervisorOutcome::Refused(SupervisorRefusal::NoListenerToAdopt);
            }
            vec![SupervisorAction::AdoptExistingListener]
        }
    };

    SupervisorOutcome::Applied(actions)
}

fn control_refusal(observed: &SupervisorObservation) -> Option<SupervisorRefusal> {
    if [observed.listener, observed.worker].contains(&ProcessOwnership::Foreign) {
        return Some(SupervisorRefusal::ForeignProcessObserved);
    }
    if [observed.listener, observed.worker].contains(&ProcessOwnership::Unknown) {
        return Some(SupervisorRefusal::ProcessOwnershipAmbiguous);
    }
    match observed.execution_identity {
        ExecutionIdentityEvidence::Verified => {}
        ExecutionIdentityEvidence::Mismatch => {
            return Some(SupervisorRefusal::ExecutionIdentityMismatch)
        }
        ExecutionIdentityEvidence::Unknown => {
            return Some(SupervisorRefusal::ExecutionIdentityNotVerified)
        }
    }
    match observed.work_root {
        OwnershipEvidence::Verified => None,
        OwnershipEvidence::NotOwned => Some(SupervisorRefusal::WorkRootNotOwned),
        OwnershipEvidence::Unknown => Some(SupervisorRefusal::WorkRootNotVerified),
    }
}

fn is_stopped(observed: &SupervisorObservation) -> bool {
    observed.phase == RunnerPhase::Stopped
        && observed.listener == ProcessOwnership::Absent
        && observed.worker == ProcessOwnership::Absent
}

fn is_drain_ready(observed: &SupervisorObservation) -> bool {
    matches!(
        observed.phase,
        RunnerPhase::Stopped | RunnerPhase::Listening
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ProcessOwnership, SupervisorAction, SupervisorCore, SupervisorObservation,
        SupervisorOutcome, SupervisorRefusal, SupervisorRequest, SyntheticProcessBackend,
    };
    use crate::{ExecutionIdentityEvidence, OwnershipEvidence, RunnerPhase};

    #[test]
    fn stopped_fixture_starts_once_and_repeated_start_is_a_noop() {
        let mut core = SupervisorCore::new(RecordingBackend::default());
        let stopped = fixture(
            RunnerPhase::Stopped,
            ProcessOwnership::Absent,
            ProcessOwnership::Absent,
        );

        assert_eq!(
            core.reconcile(SupervisorRequest::EnsureStarted, stopped)
                .unwrap(),
            SupervisorOutcome::Applied(vec![SupervisorAction::Start])
        );
        assert_eq!(core.backend().actions, vec![SupervisorAction::Start]);

        let listening = fixture(
            RunnerPhase::Listening,
            ProcessOwnership::Owned,
            ProcessOwnership::Absent,
        );
        assert_eq!(
            core.reconcile(SupervisorRequest::EnsureStarted, listening)
                .unwrap(),
            SupervisorOutcome::Noop
        );
        assert_eq!(core.backend().actions, vec![SupervisorAction::Start]);
    }

    #[test]
    fn busy_drain_requests_graceful_drain_without_stopping_a_worker() {
        let mut core = SupervisorCore::new(RecordingBackend::default());
        let busy = fixture(
            RunnerPhase::Busy,
            ProcessOwnership::Owned,
            ProcessOwnership::Owned,
        );

        assert_eq!(
            core.reconcile(SupervisorRequest::EnsureDrained, busy)
                .unwrap(),
            SupervisorOutcome::Applied(vec![SupervisorAction::RequestDrain])
        );
        assert_eq!(core.backend().actions, vec![SupervisorAction::RequestDrain]);
    }

    #[test]
    fn stop_and_restart_sequences_preserve_drain_before_busy_worker_control() {
        let busy = fixture(
            RunnerPhase::Busy,
            ProcessOwnership::Owned,
            ProcessOwnership::Owned,
        );
        let mut stop_core = SupervisorCore::new(RecordingBackend::default());
        assert_eq!(
            stop_core
                .reconcile(SupervisorRequest::EnsureStopped, busy)
                .unwrap(),
            SupervisorOutcome::Applied(vec![
                SupervisorAction::RequestDrain,
                SupervisorAction::StopAfterDrain,
            ])
        );

        let mut restart_core = SupervisorCore::new(RecordingBackend::default());
        assert_eq!(
            restart_core
                .reconcile(SupervisorRequest::Restart, busy)
                .unwrap(),
            SupervisorOutcome::Applied(vec![
                SupervisorAction::RequestDrain,
                SupervisorAction::StopAfterDrain,
                SupervisorAction::Start,
            ])
        );
    }

    #[test]
    fn reconnect_and_verified_listener_adoption_use_fixture_actions() {
        let listening = fixture(
            RunnerPhase::Listening,
            ProcessOwnership::Owned,
            ProcessOwnership::Absent,
        );
        let mut core = SupervisorCore::new(RecordingBackend::default());

        assert_eq!(
            core.reconcile(SupervisorRequest::Reconnect, listening)
                .unwrap(),
            SupervisorOutcome::Applied(vec![SupervisorAction::RestartConnection])
        );
        assert_eq!(
            core.reconcile(SupervisorRequest::Adopt, listening).unwrap(),
            SupervisorOutcome::Applied(vec![SupervisorAction::AdoptExistingListener])
        );
    }

    #[test]
    fn ambiguous_or_unowned_observation_refuses_without_touching_backend() {
        let mut core = SupervisorCore::new(RecordingBackend::default());
        let mut ambiguous = fixture(
            RunnerPhase::Listening,
            ProcessOwnership::Unknown,
            ProcessOwnership::Absent,
        );
        assert_eq!(
            core.reconcile(SupervisorRequest::Adopt, ambiguous).unwrap(),
            SupervisorOutcome::Refused(SupervisorRefusal::ProcessOwnershipAmbiguous)
        );
        ambiguous.listener = ProcessOwnership::Owned;
        ambiguous.work_root = OwnershipEvidence::Unknown;
        assert_eq!(
            core.reconcile(SupervisorRequest::Restart, ambiguous)
                .unwrap(),
            SupervisorOutcome::Refused(SupervisorRefusal::WorkRootNotVerified)
        );
        assert!(core.backend().actions.is_empty());
    }

    fn fixture(
        phase: RunnerPhase,
        listener: ProcessOwnership,
        worker: ProcessOwnership,
    ) -> SupervisorObservation {
        SupervisorObservation {
            phase,
            listener,
            worker,
            execution_identity: ExecutionIdentityEvidence::Verified,
            work_root: OwnershipEvidence::Verified,
        }
    }

    #[derive(Default)]
    struct RecordingBackend {
        actions: Vec<SupervisorAction>,
    }

    impl SyntheticProcessBackend for RecordingBackend {
        fn apply(&mut self, action: SupervisorAction) -> Result<(), String> {
            self.actions.push(action);
            Ok(())
        }
    }
}
