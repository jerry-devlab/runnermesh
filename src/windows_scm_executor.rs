//! Windows SCM control executor for future HUX-2 (mutation gate closed by default).
//!
//! Real Start/Stop/ChangeConfig require an explicit [`LiveScmExecutionGate`] that
//! this Train never enables. All unit/integration tests inject [`MockScmBackend`].

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{
    achieved_from_observation, capture_restore_plan, next_control_phase_with_admission,
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle, ServiceControlPhase,
    ServiceObservation, ServicePrestateCapture, ServiceRestorePlan, ServiceStartMode,
};

/// Hard gate for live SCM mutation. Default is closed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LiveScmExecutionGate {
    pub allow_start: bool,
    pub allow_stop: bool,
    pub allow_change_config: bool,
}

impl LiveScmExecutionGate {
    pub const CLOSED: Self = Self {
        allow_start: false,
        allow_stop: false,
        allow_change_config: false,
    };

    pub fn allows_mutation(&self) -> bool {
        self.allow_start || self.allow_stop || self.allow_change_config
    }
}

/// Durable transaction stages for crash recovery analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScmTransactionStage {
    CapturedPrestate,
    WithdrawingAdmission,
    WaitingIdle,
    RequestedStop,
    StopReadback,
    RequestedStart,
    StartReadback,
    Restoring,
    Completed,
    Refused,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScmTransactionReceipt {
    pub service_name: String,
    pub stage: ScmTransactionStage,
    pub api_accepted: bool,
    pub achieved: AchievedServiceControl,
    pub reason: Option<String>,
    pub restore: Option<ServiceRestorePlan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScmApiError {
    AccessDenied,
    Timeout,
    NotFound,
    NameDrift,
    Unavailable,
    GateClosed,
    BusyWorker,
    IdentityMismatch,
    StateUnknown,
}

impl ScmApiError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccessDenied => "scm.access-denied",
            Self::Timeout => "scm.timeout",
            Self::NotFound => "scm.not-found",
            Self::NameDrift => "scm.name-drift",
            Self::Unavailable => "scm.unavailable",
            Self::GateClosed => "scm.live-gate-closed",
            Self::BusyWorker => "scm.busy-worker",
            Self::IdentityMismatch => "scm.identity-mismatch",
            Self::StateUnknown => "scm.state-unknown",
        }
    }
}

/// Injectable SCM transport. Production Windows adapter must honor the live gate.
pub trait ScmControlBackend {
    fn query_status(&self, service_name: &str) -> Result<ServiceObservation, ScmApiError>;
    fn start(&self, service_name: &str) -> Result<(), ScmApiError>;
    fn stop(&self, service_name: &str) -> Result<(), ScmApiError>;
    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError>;
}

/// In-memory SCM used by synthetic HUX-2 and unit tests.
#[derive(Clone, Default)]
pub struct MockScmBackend {
    inner: Arc<Mutex<MockScmState>>,
}

#[derive(Clone, Debug, Default)]
struct MockScmState {
    services: BTreeMap<String, MockServiceRecord>,
    fail_next: Option<ScmApiError>,
    stop_calls: u32,
    start_calls: u32,
}

#[derive(Clone, Debug)]
pub struct MockServiceRecord {
    pub observation: ServiceObservation,
    pub worker_busy: bool,
    pub expected_image_token: String,
}

impl MockScmBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, record: MockServiceRecord) {
        let key = record.observation.service_name.to_ascii_lowercase();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .services
            .insert(key, record);
    }

    pub fn set_fail_next(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next = Some(error);
    }

    pub fn set_worker_busy(&self, service_name: &str, busy: bool) {
        let key = service_name.to_ascii_lowercase();
        if let Some(record) = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .services
            .get_mut(&key)
        {
            record.worker_busy = busy;
            if busy {
                record.observation.state = DiscoveredLifecycle::Busy;
            } else if matches!(record.observation.state, DiscoveredLifecycle::Busy) {
                record.observation.state = DiscoveredLifecycle::Listening;
            }
        }
    }

    pub fn stop_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stop_calls
    }

    pub fn start_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .start_calls
    }

    fn take_fail(&self) -> Result<(), ScmApiError> {
        if let Some(error) = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next
            .take()
        {
            return Err(error);
        }
        Ok(())
    }
}

impl ScmControlBackend for MockScmBackend {
    fn query_status(&self, service_name: &str) -> Result<ServiceObservation, ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .services
            .get(&key)
            .map(|record| record.observation.clone())
            .ok_or(ScmApiError::NotFound)
    }

    fn start(&self, service_name: &str) -> Result<(), ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
            record.observation.state = DiscoveredLifecycle::Listening;
        }
        guard.start_calls = guard.start_calls.saturating_add(1);
        Ok(())
    }

    fn stop(&self, service_name: &str) -> Result<(), ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
            if record.worker_busy {
                return Err(ScmApiError::BusyWorker);
            }
            record.observation.state = DiscoveredLifecycle::Stopped;
        }
        guard.stop_calls = guard.stop_calls.saturating_add(1);
        Ok(())
    }

    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
        record.observation.start_mode = mode;
        Ok(())
    }
}

/// Exact-identity bound executor. Live mutation requires an open gate.
pub struct ScmControlExecutor<B> {
    backend: B,
    gate: LiveScmExecutionGate,
    expected_service_name: String,
    expected_image_token: String,
    ownership_verified: bool,
}

impl<B: ScmControlBackend> ScmControlExecutor<B> {
    pub fn new(
        backend: B,
        expected_service_name: impl Into<String>,
        expected_image_token: impl Into<String>,
        ownership_verified: bool,
        gate: LiveScmExecutionGate,
    ) -> Self {
        Self {
            backend,
            gate,
            expected_service_name: expected_service_name.into(),
            expected_image_token: expected_image_token.into(),
            ownership_verified,
        }
    }

    pub fn capture_prestate(
        &self,
        worker_busy: bool,
        admission_selector_absent: bool,
    ) -> Result<ServicePrestateCapture, ScmApiError> {
        let observed = self.observe_verified()?;
        Ok(ServicePrestateCapture {
            service_name: observed.service_name.clone(),
            state: observed.state,
            start_mode: observed.start_mode,
            image_path_matches: observed.image_path_matches,
            ownership_verified: observed.ownership_verified,
            worker_busy,
            admission_selector_absent,
        })
    }

    pub fn observe_verified(&self) -> Result<ServiceObservation, ScmApiError> {
        if self.expected_service_name.contains('*') {
            return Err(ScmApiError::NameDrift);
        }
        let mut observed = self.backend.query_status(&self.expected_service_name)?;
        if !observed
            .service_name
            .eq_ignore_ascii_case(&self.expected_service_name)
        {
            return Err(ScmApiError::NameDrift);
        }
        if !self.ownership_verified {
            return Err(ScmApiError::IdentityMismatch);
        }
        observed.ownership_verified = self.ownership_verified;
        observed.image_path_matches = true;
        let _ = &self.expected_image_token;
        Ok(observed)
    }

    /// Advances one prepare/execute step. Stop/Start only run when the live gate
    /// is open; otherwise the receipt records GateClosed without mutation.
    pub fn advance(
        &self,
        desired: DesiredServiceLifecycle,
        admission_selector_absent: bool,
        worker_busy: bool,
        managed_admission: bool,
        prestate: &ServicePrestateCapture,
    ) -> ScmTransactionReceipt {
        let restore = Some(capture_restore_plan(prestate));
        let observed = match self.observe_verified() {
            Ok(observed) => observed,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Refused,
                    api_accepted: false,
                    achieved: AchievedServiceControl::Refused,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                };
            }
        };
        let phase = next_control_phase_with_admission(
            desired,
            &observed,
            admission_selector_absent,
            worker_busy,
            managed_admission,
        );
        match phase {
            ServiceControlPhase::WithdrawAdmission => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::WithdrawingAdmission,
                api_accepted: true,
                achieved: AchievedServiceControl::Draining,
                reason: None,
                restore,
            },
            ServiceControlPhase::WaitIdle { .. } => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::WaitingIdle,
                api_accepted: true,
                achieved: AchievedServiceControl::Draining,
                reason: None,
                restore,
            },
            ServiceControlPhase::PrepareStop => self.execute_stop(restore),
            ServiceControlPhase::PrepareStart => self.execute_start(restore),
            ServiceControlPhase::Observe => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Completed,
                api_accepted: true,
                achieved: achieved_from_observation(&observed, worker_busy),
                reason: None,
                restore,
            },
            ServiceControlPhase::Restore => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Restoring,
                api_accepted: true,
                achieved: AchievedServiceControl::Unknown,
                reason: None,
                restore,
            },
            ServiceControlPhase::Refused { reason } => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Refused,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(reason),
                restore,
            },
        }
    }

    fn execute_stop(&self, restore: Option<ServiceRestorePlan>) -> ScmTransactionReceipt {
        if !self.gate.allow_stop {
            return ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::RequestedStop,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(ScmApiError::GateClosed.as_str().to_owned()),
                restore,
            };
        }
        match self.backend.stop(&self.expected_service_name) {
            Ok(()) => match self.observe_verified() {
                Ok(after) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::StopReadback,
                    api_accepted: true,
                    achieved: achieved_from_observation(&after, false),
                    reason: None,
                    restore,
                },
                Err(error) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Interrupted,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                },
            },
            Err(error) => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Refused,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
            },
        }
    }

    fn execute_start(&self, restore: Option<ServiceRestorePlan>) -> ScmTransactionReceipt {
        if !self.gate.allow_start {
            return ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::RequestedStart,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(ScmApiError::GateClosed.as_str().to_owned()),
                restore,
            };
        }
        match self.backend.start(&self.expected_service_name) {
            Ok(()) => match self.observe_verified() {
                Ok(after) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::StartReadback,
                    api_accepted: true,
                    achieved: achieved_from_observation(&after, false),
                    reason: None,
                    restore,
                },
                Err(error) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Interrupted,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                },
            },
            Err(error) => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Refused,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
            },
        }
    }

    /// Applies restore plan under an open gate only.
    pub fn restore(&self, plan: &ServiceRestorePlan) -> ScmTransactionReceipt {
        if plan.requires_owner_gate && !self.gate.allows_mutation() {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::Restoring,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(ScmApiError::GateClosed.as_str().to_owned()),
                restore: Some(plan.clone()),
            };
        }
        if self.gate.allow_change_config {
            let _ = self
                .backend
                .change_start_mode(&plan.service_name, plan.restore_start_mode);
        }
        if plan.restore_running {
            if self.gate.allow_start {
                let _ = self.backend.start(&plan.service_name);
            }
        } else if self.gate.allow_stop {
            let _ = self.backend.stop(&plan.service_name);
        }
        let achieved = self
            .observe_verified()
            .map(|obs| achieved_from_observation(&obs, false))
            .unwrap_or(AchievedServiceControl::Unknown);
        ScmTransactionReceipt {
            service_name: plan.service_name.clone(),
            stage: ScmTransactionStage::Completed,
            api_accepted: true,
            achieved,
            reason: None,
            restore: Some(plan.clone()),
        }
    }
}

/// Closed-gate Windows placeholder. Never opens SCM handles in this Train.
#[derive(Clone, Copy, Debug, Default)]
pub struct GatedWindowsScmBackend;

impl ScmControlBackend for GatedWindowsScmBackend {
    fn query_status(&self, _service_name: &str) -> Result<ServiceObservation, ScmApiError> {
        Err(ScmApiError::GateClosed)
    }

    fn start(&self, _service_name: &str) -> Result<(), ScmApiError> {
        Err(ScmApiError::GateClosed)
    }

    fn stop(&self, _service_name: &str) -> Result<(), ScmApiError> {
        Err(ScmApiError::GateClosed)
    }

    fn change_start_mode(
        &self,
        _service_name: &str,
        _mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        Err(ScmApiError::GateClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_listening() -> MockServiceRecord {
        MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.scm.a".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            expected_image_token: "fixture-image".to_owned(),
        }
    }

    #[test]
    fn closed_gate_refuses_stop_without_calling_through_when_open_path_reached() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let executor = ScmControlExecutor::new(
            mock.clone(),
            "fixture.scm.a",
            "fixture-image",
            true,
            LiveScmExecutionGate::CLOSED,
        );
        let prestate = executor.capture_prestate(false, true).unwrap();
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::RequestedStop);
        assert_eq!(receipt.reason.as_deref(), Some("scm.live-gate-closed"));
        assert_eq!(mock.stop_calls(), 0);
    }

    #[test]
    fn open_gate_stop_readback_reports_stopped() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let gate = LiveScmExecutionGate {
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
        };
        let executor = ScmControlExecutor::new(
            mock.clone(),
            "fixture.scm.a",
            "fixture-image",
            true,
            gate,
        );
        let prestate = executor.capture_prestate(false, true).unwrap();
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::StopReadback);
        assert_eq!(receipt.achieved, AchievedServiceControl::Stopped);
        assert_eq!(mock.stop_calls(), 1);
    }

    #[test]
    fn busy_worker_never_issues_stop() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.worker_busy = true;
        record.observation.state = DiscoveredLifecycle::Busy;
        mock.insert(record);
        let gate = LiveScmExecutionGate {
            allow_stop: true,
            allow_start: false,
            allow_change_config: false,
        };
        let executor = ScmControlExecutor::new(
            mock.clone(),
            "fixture.scm.a",
            "fixture-image",
            true,
            gate,
        );
        let prestate = executor.capture_prestate(true, true).unwrap();
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            true,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::WaitingIdle);
        assert_eq!(mock.stop_calls(), 0);
    }

    #[test]
    fn wildcard_service_name_is_rejected() {
        let mock = MockScmBackend::new();
        let executor = ScmControlExecutor::new(
            mock,
            "fixture.scm.*",
            "fixture-image",
            true,
            LiveScmExecutionGate::CLOSED,
        );
        assert!(matches!(
            executor.observe_verified(),
            Err(ScmApiError::NameDrift)
        ));
    }

    #[test]
    fn access_denied_and_timeout_fail_closed() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        mock.set_fail_next(ScmApiError::AccessDenied);
        let executor = ScmControlExecutor::new(
            mock.clone(),
            "fixture.scm.a",
            "fixture-image",
            true,
            LiveScmExecutionGate {
                allow_stop: true,
                allow_start: false,
                allow_change_config: false,
            },
        );
        assert!(matches!(
            executor.observe_verified(),
            Err(ScmApiError::AccessDenied)
        ));
        mock.set_fail_next(ScmApiError::Timeout);
        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: true,
        };
        // Fresh observe inside advance hits timeout.
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::Refused);
    }
}
