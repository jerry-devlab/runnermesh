//! Synthetic owned user-session runner lifecycle backend (LRM-05).
//!
//! Prepares launch/adopt/graceful-stop plans for enrolled user-session runners
//! without executing processes, touching foreign sessions, or reusing
//! cross-identity work roots. Real execution remains Owner-gated.

use crate::{DiscoveredLifecycle, RegistryBackendBinding, RunnerRecord};

/// Observation facts for an owned user-session runner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionObservation {
    pub current_user_owned: bool,
    pub runner_home_present: bool,
    pub entrypoint_present: bool,
    pub work_root_owned: bool,
    pub listener_present: bool,
    pub worker_present: bool,
    pub lifecycle: DiscoveredLifecycle,
}

/// Prepared user-session action. Construction has no process side effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedSessionAction {
    LaunchOwnedListener,
    AdoptVerifiedListener,
    RequestGracefulStop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionBackendRefusal {
    NotUserSessionBackend,
    ForeignSession,
    RunnerHomeMissing,
    EntrypointMissing,
    WorkRootNotOwned,
    BusyWorker,
    StateUnknown,
    NoListenerToAdopt,
}

impl SessionBackendRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotUserSessionBackend => "session.not-user-session-backend",
            Self::ForeignSession => "session.foreign-session",
            Self::RunnerHomeMissing => "session.runner-home-missing",
            Self::EntrypointMissing => "session.entrypoint-missing",
            Self::WorkRootNotOwned => "session.work-root-not-owned",
            Self::BusyWorker => "session.busy-worker",
            Self::StateUnknown => "session.state-unknown",
            Self::NoListenerToAdopt => "session.no-listener-to-adopt",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPrepareOutcome {
    Planned(PreparedSessionAction),
    Noop,
    Refused(SessionBackendRefusal),
}

#[derive(Clone, Debug, Default)]
pub struct SyntheticUserSessionBackend;

impl SyntheticUserSessionBackend {
    pub fn prepare_launch(
        &self,
        record: &RunnerRecord,
        observed: &SessionObservation,
    ) -> SessionPrepareOutcome {
        if let Some(refusal) = common_refusal(record, observed) {
            return SessionPrepareOutcome::Refused(refusal);
        }
        match observed.lifecycle {
            DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy => {
                SessionPrepareOutcome::Noop
            }
            DiscoveredLifecycle::Stopped => {
                SessionPrepareOutcome::Planned(PreparedSessionAction::LaunchOwnedListener)
            }
            DiscoveredLifecycle::Unknown => {
                SessionPrepareOutcome::Refused(SessionBackendRefusal::StateUnknown)
            }
        }
    }

    pub fn prepare_adopt(
        &self,
        record: &RunnerRecord,
        observed: &SessionObservation,
    ) -> SessionPrepareOutcome {
        if let Some(refusal) = common_refusal(record, observed) {
            return SessionPrepareOutcome::Refused(refusal);
        }
        if !observed.listener_present {
            return SessionPrepareOutcome::Refused(SessionBackendRefusal::NoListenerToAdopt);
        }
        SessionPrepareOutcome::Planned(PreparedSessionAction::AdoptVerifiedListener)
    }

    pub fn prepare_graceful_stop(
        &self,
        record: &RunnerRecord,
        observed: &SessionObservation,
    ) -> SessionPrepareOutcome {
        if let Some(refusal) = common_refusal(record, observed) {
            return SessionPrepareOutcome::Refused(refusal);
        }
        if observed.worker_present || observed.lifecycle == DiscoveredLifecycle::Busy {
            return SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker);
        }
        match observed.lifecycle {
            DiscoveredLifecycle::Stopped => SessionPrepareOutcome::Noop,
            DiscoveredLifecycle::Unknown => {
                SessionPrepareOutcome::Refused(SessionBackendRefusal::StateUnknown)
            }
            DiscoveredLifecycle::Listening => {
                SessionPrepareOutcome::Planned(PreparedSessionAction::RequestGracefulStop)
            }
            DiscoveredLifecycle::Busy => {
                SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
            }
        }
    }
}

fn common_refusal(
    record: &RunnerRecord,
    observed: &SessionObservation,
) -> Option<SessionBackendRefusal> {
    if !matches!(record.backend, RegistryBackendBinding::UserSession) {
        return Some(SessionBackendRefusal::NotUserSessionBackend);
    }
    if !observed.current_user_owned {
        return Some(SessionBackendRefusal::ForeignSession);
    }
    if !observed.runner_home_present {
        return Some(SessionBackendRefusal::RunnerHomeMissing);
    }
    if !observed.entrypoint_present {
        return Some(SessionBackendRefusal::EntrypointMissing);
    }
    if !observed.work_root_owned {
        return Some(SessionBackendRefusal::WorkRootNotOwned);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EnrollmentState, OpaqueIdentityReference, RegistrationScope,
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
    };
    use std::path::PathBuf;

    fn record() -> RunnerRecord {
        RunnerRecord {
            local_runner_key: "key-session".to_owned(),
            registration_scope: RegistrationScope::Repository {
                owner: "fixture-owner".to_owned(),
                repository: "repo-a".to_owned(),
            },
            runner_id: 21,
            expected_runner_name: "fixture-session".to_owned(),
            runner_home: PathBuf::from(r"C:\fixture\runners\session"),
            work_root: PathBuf::from(r"C:\fixture\runners\session\_work"),
            execution_identity_ref: OpaqueIdentityReference::new(
                WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .unwrap(),
            backend: RegistryBackendBinding::UserSession,
            desired_enabled: true,
            enrollment_state: EnrollmentState::Enrolled,
            credential_ref: None,
            managed_admission: false,
        }
    }

    fn observation(lifecycle: DiscoveredLifecycle) -> SessionObservation {
        SessionObservation {
            current_user_owned: true,
            runner_home_present: true,
            entrypoint_present: true,
            work_root_owned: true,
            listener_present: matches!(
                lifecycle,
                DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
            ),
            worker_present: lifecycle == DiscoveredLifecycle::Busy,
            lifecycle,
        }
    }

    #[test]
    fn launch_and_stop_plans_respect_idle_busy_unknown() {
        let backend = SyntheticUserSessionBackend;
        let record = record();
        assert_eq!(
            backend.prepare_launch(&record, &observation(DiscoveredLifecycle::Stopped)),
            SessionPrepareOutcome::Planned(PreparedSessionAction::LaunchOwnedListener)
        );
        assert!(matches!(
            backend.prepare_graceful_stop(&record, &observation(DiscoveredLifecycle::Busy)),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
        ));
        assert_eq!(
            backend.prepare_graceful_stop(&record, &observation(DiscoveredLifecycle::Listening)),
            SessionPrepareOutcome::Planned(PreparedSessionAction::RequestGracefulStop)
        );
        assert!(matches!(
            backend.prepare_launch(&record, &observation(DiscoveredLifecycle::Unknown)),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::StateUnknown)
        ));
    }

    #[test]
    fn foreign_session_and_unowned_work_root_are_refused() {
        let backend = SyntheticUserSessionBackend;
        let record = record();
        let mut foreign = observation(DiscoveredLifecycle::Stopped);
        foreign.current_user_owned = false;
        assert!(matches!(
            backend.prepare_launch(&record, &foreign),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::ForeignSession)
        ));
        let mut unowned = observation(DiscoveredLifecycle::Stopped);
        unowned.work_root_owned = false;
        assert!(matches!(
            backend.prepare_launch(&record, &unowned),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::WorkRootNotOwned)
        ));
    }
}
