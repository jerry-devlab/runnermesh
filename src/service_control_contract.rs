//! Exact Windows Service controller contract (prepare-only).
//!
//! Defines the minimum-privilege state machine, admission drain, Worker
//! protection, and restore obligations for a future Owner-gated HUX-2 live
//! exercise. This module never opens the Service Control Manager.

use serde::{Deserialize, Serialize};

use crate::{DiscoveredLifecycle, ServiceBackendRefusal, ServiceObservation, ServiceStartMode};

/// Narrow SCM rights required for one exact service. Wildcards are forbidden.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServiceControlRight {
    QueryStatus,
    QueryConfig,
    Start,
    Stop,
    ChangeConfig,
}

/// Desired lifecycle requested by policy after admission withdrawal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesiredServiceLifecycle {
    RemainRunning,
    DrainThenStop,
    StayStopped,
}

/// Achieved control outcome. Never asserted from a single API acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AchievedServiceControl {
    Running,
    Draining,
    Stopped,
    Unknown,
    Refused,
}

/// Prestate capture required before any Owner-authorized mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicePrestateCapture {
    pub service_name: String,
    pub state: DiscoveredLifecycle,
    pub start_mode: ServiceStartMode,
    pub image_path_matches: bool,
    pub ownership_verified: bool,
    pub worker_busy: bool,
    pub admission_selector_absent: bool,
}

/// Restore plan derived from prestate. Execution remains Owner-gated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRestorePlan {
    pub service_name: String,
    pub restore_start_mode: ServiceStartMode,
    pub restore_running: bool,
    pub requires_owner_gate: bool,
}

/// One step in the prepare-only control state machine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum ServiceControlPhase {
    Observe,
    WithdrawAdmission,
    WaitIdle { worker_busy: bool },
    PrepareStop,
    PrepareStart,
    Restore,
    Refused { reason: String },
}

/// Evaluates the next prepare-only phase without touching SCM.
pub fn next_control_phase(
    desired: DesiredServiceLifecycle,
    observed: &ServiceObservation,
    admission_selector_absent: bool,
    worker_busy: bool,
) -> ServiceControlPhase {
    if !observed.ownership_verified || !observed.image_path_matches {
        return ServiceControlPhase::Refused {
            reason: ServiceBackendRefusal::OwnershipUnverified
                .as_str()
                .to_owned(),
        };
    }
    if matches!(observed.state, DiscoveredLifecycle::Unknown) {
        return ServiceControlPhase::Refused {
            reason: ServiceBackendRefusal::StateUnknown.as_str().to_owned(),
        };
    }
    match desired {
        DesiredServiceLifecycle::RemainRunning => {
            if matches!(
                observed.state,
                DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
            ) {
                ServiceControlPhase::Observe
            } else {
                ServiceControlPhase::PrepareStart
            }
        }
        DesiredServiceLifecycle::StayStopped => {
            if matches!(observed.state, DiscoveredLifecycle::Stopped) {
                ServiceControlPhase::Observe
            } else if worker_busy {
                ServiceControlPhase::Refused {
                    reason: ServiceBackendRefusal::BusyWorker.as_str().to_owned(),
                }
            } else {
                ServiceControlPhase::PrepareStop
            }
        }
        DesiredServiceLifecycle::DrainThenStop => {
            if !admission_selector_absent {
                ServiceControlPhase::WithdrawAdmission
            } else if worker_busy {
                ServiceControlPhase::WaitIdle { worker_busy: true }
            } else if matches!(observed.state, DiscoveredLifecycle::Stopped) {
                ServiceControlPhase::Observe
            } else {
                ServiceControlPhase::PrepareStop
            }
        }
    }
}

/// Captures restore obligations from the observed prestate.
pub fn capture_restore_plan(prestate: &ServicePrestateCapture) -> ServiceRestorePlan {
    ServiceRestorePlan {
        service_name: prestate.service_name.clone(),
        restore_start_mode: prestate.start_mode,
        restore_running: matches!(
            prestate.state,
            DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
        ),
        requires_owner_gate: true,
    }
}

/// Maps observation into an honest achieved control signal.
pub fn achieved_from_observation(
    observed: &ServiceObservation,
    worker_busy: bool,
) -> AchievedServiceControl {
    if matches!(observed.state, DiscoveredLifecycle::Unknown) {
        return AchievedServiceControl::Unknown;
    }
    if worker_busy {
        return AchievedServiceControl::Draining;
    }
    match observed.state {
        DiscoveredLifecycle::Stopped => AchievedServiceControl::Stopped,
        DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy => {
            AchievedServiceControl::Running
        }
        DiscoveredLifecycle::Unknown => AchievedServiceControl::Unknown,
    }
}

/// Minimum rights for a read-only observation pass.
pub fn read_only_rights() -> &'static [ServiceControlRight] {
    &[
        ServiceControlRight::QueryStatus,
        ServiceControlRight::QueryConfig,
    ]
}

/// Minimum rights for an Owner-gated start/stop transaction on one exact service.
pub fn mutation_rights() -> &'static [ServiceControlRight] {
    &[
        ServiceControlRight::QueryStatus,
        ServiceControlRight::QueryConfig,
        ServiceControlRight::Start,
        ServiceControlRight::Stop,
        ServiceControlRight::ChangeConfig,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(state: DiscoveredLifecycle, busy_safe: bool) -> ServiceObservation {
        ServiceObservation {
            service_name: "actions.runner.fixture".to_owned(),
            state,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: busy_safe,
        }
    }

    #[test]
    fn drain_requires_selector_absent_before_stop() {
        let observed = observation(DiscoveredLifecycle::Listening, true);
        let phase = next_control_phase(
            DesiredServiceLifecycle::DrainThenStop,
            &observed,
            false,
            false,
        );
        assert!(matches!(phase, ServiceControlPhase::WithdrawAdmission));
    }

    #[test]
    fn busy_worker_blocks_stop_and_waits_when_draining() {
        let observed = observation(DiscoveredLifecycle::Busy, true);
        let wait = next_control_phase(
            DesiredServiceLifecycle::DrainThenStop,
            &observed,
            true,
            true,
        );
        assert!(matches!(
            wait,
            ServiceControlPhase::WaitIdle { worker_busy: true }
        ));
        let refused =
            next_control_phase(DesiredServiceLifecycle::StayStopped, &observed, true, true);
        assert!(matches!(refused, ServiceControlPhase::Refused { .. }));
    }

    #[test]
    fn restore_plan_always_requires_owner_gate() {
        let prestate = ServicePrestateCapture {
            service_name: "actions.runner.fixture".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: false,
        };
        let plan = capture_restore_plan(&prestate);
        assert!(plan.requires_owner_gate);
        assert!(plan.restore_running);
        assert_eq!(plan.restore_start_mode, ServiceStartMode::Automatic);
    }

    #[test]
    fn unknown_state_never_claims_stopped() {
        let observed = observation(DiscoveredLifecycle::Unknown, true);
        assert_eq!(
            achieved_from_observation(&observed, false),
            AchievedServiceControl::Unknown
        );
    }
}
