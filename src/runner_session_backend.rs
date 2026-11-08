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
    /// True when a recorded Listener PID no longer resolves to an owned process.
    /// Stale PID evidence must never be adopted as a live Listener.
    pub listener_pid_stale: bool,
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
    StaleListenerPid,
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
            Self::StaleListenerPid => "session.stale-listener-pid",
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
        if observed.worker_present || observed.lifecycle == DiscoveredLifecycle::Busy {
            return SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker);
        }
        if observed.listener_present {
            return SessionPrepareOutcome::Noop;
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
        if observed.worker_present || observed.lifecycle == DiscoveredLifecycle::Busy {
            // Adoption never claims a Busy Worker is idle ownership evidence.
            return SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker);
        }
        if matches!(observed.lifecycle, DiscoveredLifecycle::Unknown) {
            return SessionPrepareOutcome::Refused(SessionBackendRefusal::StateUnknown);
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

    /// Evaluates launch plans across multiple enrolled session runners.
    ///
    /// Proves that an error or refusal on one runner does not block or alter
    /// planning on another independent runner.
    pub fn prepare_multi_launch(
        &self,
        candidates: &[(&RunnerRecord, &SessionObservation)],
    ) -> Vec<(String, SessionPrepareOutcome)> {
        candidates
            .iter()
            .map(|(record, obs)| {
                (
                    record.local_runner_key.clone(),
                    self.prepare_launch(record, obs),
                )
            })
            .collect()
    }

    /// Evaluates graceful stop plans across multiple enrolled session runners.
    pub fn prepare_multi_graceful_stop(
        &self,
        candidates: &[(&RunnerRecord, &SessionObservation)],
    ) -> Vec<(String, SessionPrepareOutcome)> {
        candidates
            .iter()
            .map(|(record, obs)| {
                (
                    record.local_runner_key.clone(),
                    self.prepare_graceful_stop(record, obs),
                )
            })
            .collect()
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
    if observed.listener_pid_stale {
        return Some(SessionBackendRefusal::StaleListenerPid);
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
            listener_pid_stale: false,
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

    #[test]
    fn adopt_refuses_busy_unknown_and_missing_listener() {
        let backend = SyntheticUserSessionBackend;
        let record = record();
        assert!(matches!(
            backend.prepare_adopt(&record, &observation(DiscoveredLifecycle::Busy)),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
        ));
        assert!(matches!(
            backend.prepare_adopt(&record, &observation(DiscoveredLifecycle::Unknown)),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::StateUnknown)
        ));
        let mut stopped_no_listener = observation(DiscoveredLifecycle::Stopped);
        stopped_no_listener.listener_present = false;
        assert!(matches!(
            backend.prepare_adopt(&record, &stopped_no_listener),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::NoListenerToAdopt)
        ));
        assert_eq!(
            backend.prepare_adopt(&record, &observation(DiscoveredLifecycle::Listening)),
            SessionPrepareOutcome::Planned(PreparedSessionAction::AdoptVerifiedListener)
        );
    }

    #[test]
    fn missing_home_or_entrypoint_refuses_launch() {
        let backend = SyntheticUserSessionBackend;
        let record = record();
        let mut no_home = observation(DiscoveredLifecycle::Stopped);
        no_home.runner_home_present = false;
        assert!(matches!(
            backend.prepare_launch(&record, &no_home),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::RunnerHomeMissing)
        ));
        let mut no_entry = observation(DiscoveredLifecycle::Stopped);
        no_entry.entrypoint_present = false;
        assert!(matches!(
            backend.prepare_launch(&record, &no_entry),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::EntrypointMissing)
        ));
    }

    #[test]
    fn stale_listener_pid_refuses_adopt_and_stop_without_launch_claim() {
        let backend = SyntheticUserSessionBackend;
        let record = record();
        let mut stale = observation(DiscoveredLifecycle::Listening);
        stale.listener_pid_stale = true;
        assert!(matches!(
            backend.prepare_adopt(&record, &stale),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::StaleListenerPid)
        ));
        assert!(matches!(
            backend.prepare_graceful_stop(&record, &stale),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::StaleListenerPid)
        ));
        // Launch against a Stopped observation with a stale recorded PID still
        // refuses — never treat a dead PID as ownership for a new plan.
        let mut stale_stopped = observation(DiscoveredLifecycle::Stopped);
        stale_stopped.listener_pid_stale = true;
        assert!(matches!(
            backend.prepare_launch(&record, &stale_stopped),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::StaleListenerPid)
        ));
    }

    #[test]
    fn launch_refuses_when_worker_is_present_or_busy_and_noops_when_listener_present() {
        let backend = SyntheticUserSessionBackend;
        let record = record();
        let mut stopped_with_worker = observation(DiscoveredLifecycle::Stopped);
        stopped_with_worker.worker_present = true;
        assert_eq!(
            backend.prepare_launch(&record, &stopped_with_worker),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
        );

        let mut stopped_with_listener = observation(DiscoveredLifecycle::Stopped);
        stopped_with_listener.listener_present = true;
        assert_eq!(
            backend.prepare_launch(&record, &stopped_with_listener),
            SessionPrepareOutcome::Noop
        );

        let busy = observation(DiscoveredLifecycle::Busy);
        assert_eq!(
            backend.prepare_launch(&record, &busy),
            SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
        );
    }

    #[test]
    fn multi_instance_session_launch_and_stop_isolation() {
        let backend = SyntheticUserSessionBackend;
        let mut record_a = record();
        record_a.local_runner_key = "session-a".to_owned();
        let mut record_b = record();
        record_b.local_runner_key = "session-b".to_owned();
        let mut record_c = record();
        record_c.local_runner_key = "session-c".to_owned();
        let mut record_d = record();
        record_d.local_runner_key = "session-d".to_owned();

        // Runner A: clean Stopped -> LaunchOwnedListener
        let obs_a = observation(DiscoveredLifecycle::Stopped);
        // Runner B: Busy -> refused BusyWorker
        let obs_b = observation(DiscoveredLifecycle::Busy);
        // Runner C: Stale PID -> refused StaleListenerPid
        let mut obs_c = observation(DiscoveredLifecycle::Stopped);
        obs_c.listener_pid_stale = true;
        // Runner D: Foreign Session -> refused ForeignSession
        let mut obs_d = observation(DiscoveredLifecycle::Stopped);
        obs_d.current_user_owned = false;

        let batch = [
            (&record_a, &obs_a),
            (&record_b, &obs_b),
            (&record_c, &obs_c),
            (&record_d, &obs_d),
        ];

        let results = backend.prepare_multi_launch(&batch);
        assert_eq!(results.len(), 4);
        assert_eq!(
            results[0],
            (
                "session-a".to_owned(),
                SessionPrepareOutcome::Planned(PreparedSessionAction::LaunchOwnedListener)
            )
        );
        assert_eq!(
            results[1],
            (
                "session-b".to_owned(),
                SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
            )
        );
        assert_eq!(
            results[2],
            (
                "session-c".to_owned(),
                SessionPrepareOutcome::Refused(SessionBackendRefusal::StaleListenerPid)
            )
        );
        assert_eq!(
            results[3],
            (
                "session-d".to_owned(),
                SessionPrepareOutcome::Refused(SessionBackendRefusal::ForeignSession)
            )
        );

        // Multi-stop: A is Listening -> RequestGracefulStop; B is Busy -> Refused(BusyWorker)
        let obs_a_listen = observation(DiscoveredLifecycle::Listening);
        let stop_batch = [(&record_a, &obs_a_listen), (&record_b, &obs_b)];
        let stop_results = backend.prepare_multi_graceful_stop(&stop_batch);
        assert_eq!(stop_results.len(), 2);
        assert_eq!(
            stop_results[0],
            (
                "session-a".to_owned(),
                SessionPrepareOutcome::Planned(PreparedSessionAction::RequestGracefulStop)
            )
        );
        assert_eq!(
            stop_results[1],
            (
                "session-b".to_owned(),
                SessionPrepareOutcome::Refused(SessionBackendRefusal::BusyWorker)
            )
        );
    }
}
