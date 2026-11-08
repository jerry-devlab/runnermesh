//! Windows SCM control executor for future HUX-2 (mutation gate sealed closed).
//!
//! Real Start/Stop/ChangeConfig require a sealed [`OwnerScmAuthorizationProof`].
//! Arbitrary `LiveScmExecutionGate { allow_*: true }` construction is impossible.
//! Unit/integration tests inject [`MockScmBackend`] or use
//! [`OwnerScmAuthorizationProof::synthetic_for_tests`].

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    achieved_from_observation, capture_restore_plan, next_control_phase_with_admission,
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle, ServiceControlPhase,
    ServiceObservation, ServicePrestateCapture, ServiceRestorePlan, ServiceStartMode,
};

/// Sealed owner authorization for live SCM mutation.
///
/// Production callers cannot invent open gates from public bool fields. The seal
/// binds service name + image token + requested rights.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerScmAuthorizationProof {
    service_name: String,
    image_token: String,
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
    seal: String,
}

impl OwnerScmAuthorizationProof {
    const SEAL_DOMAIN: &'static str = "runnermesh-scm-owner-auth-v1";

    /// Synthetic/test authorization only. Never treat as production Owner token.
    pub fn synthetic_for_tests(
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> Self {
        let service_name = service_name.into();
        let image_token = image_token.into();
        let seal = Self::compute_seal(
            &service_name,
            &image_token,
            allow_start,
            allow_stop,
            allow_change_config,
        );
        Self {
            service_name,
            image_token,
            allow_start,
            allow_stop,
            allow_change_config,
            seal,
        }
    }

    fn compute_seal(
        service_name: &str,
        image_token: &str,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> String {
        let material = format!(
            "{}|{}|{}|{}|{}|{}",
            Self::SEAL_DOMAIN,
            service_name.to_ascii_lowercase(),
            image_token.to_ascii_lowercase(),
            allow_start as u8,
            allow_stop as u8,
            allow_change_config as u8
        );
        let digest = Sha256::digest(material.as_bytes());
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn is_seal_valid(&self) -> bool {
        self.seal
            == Self::compute_seal(
                &self.service_name,
                &self.image_token,
                self.allow_start,
                self.allow_stop,
                self.allow_change_config,
            )
    }
}

/// Hard gate for live SCM mutation. Fields are private; open only via sealed proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveScmExecutionGate {
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
    sealed: bool,
    bound_service_name: String,
    bound_image_token: String,
}

impl Default for LiveScmExecutionGate {
    fn default() -> Self {
        Self::CLOSED
    }
}

impl LiveScmExecutionGate {
    pub const CLOSED: Self = Self {
        allow_start: false,
        allow_stop: false,
        allow_change_config: false,
        sealed: true,
        bound_service_name: String::new(),
        bound_image_token: String::new(),
    };

    /// Opens mutation rights only when the sealed Owner proof validates.
    pub fn from_owner_authorization(
        proof: &OwnerScmAuthorizationProof,
    ) -> Result<Self, ScmApiError> {
        if !proof.is_seal_valid() {
            return Err(ScmApiError::GateClosed);
        }
        if proof.service_name.trim().is_empty() || proof.service_name.contains('*') {
            return Err(ScmApiError::NameDrift);
        }
        Ok(Self {
            allow_start: proof.allow_start,
            allow_stop: proof.allow_stop,
            allow_change_config: proof.allow_change_config,
            sealed: true,
            bound_service_name: proof.service_name.clone(),
            bound_image_token: proof.image_token.clone(),
        })
    }

    pub fn allows_mutation(&self) -> bool {
        self.sealed && (self.allow_start || self.allow_stop || self.allow_change_config)
    }

    fn authorizes(
        &self,
        service_name: &str,
        image_token: &str,
        need_start: bool,
        need_stop: bool,
        need_config: bool,
    ) -> bool {
        if !self.sealed {
            return false;
        }
        if !self.bound_service_name.is_empty()
            && !self.bound_service_name.eq_ignore_ascii_case(service_name)
        {
            return false;
        }
        if !self.bound_image_token.is_empty()
            && !self.bound_image_token.eq_ignore_ascii_case(image_token)
        {
            return false;
        }
        if need_start && !self.allow_start {
            return false;
        }
        if need_stop && !self.allow_stop {
            return false;
        }
        if need_config && !self.allow_change_config {
            return false;
        }
        true
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
    RestoreFailed,
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
    /// Independent restore readback verdict. Never true unless every restore
    /// step succeeded and final observation matched the bound plan.
    pub restore_pass: bool,
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
    RestoreIncomplete,
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
            Self::RestoreIncomplete => "scm.restore-incomplete",
        }
    }
}

/// Exact host snapshot used for identity binding. Image tokens are opaque.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScmExactSnapshot {
    pub observation: ServiceObservation,
    pub image_token: String,
    pub execution_identity_token: String,
}

/// Injectable SCM transport. Production Windows adapter must honor the live gate.
pub trait ScmControlBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError>;
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
    fail_change: Option<ScmApiError>,
    stop_calls: u32,
    start_calls: u32,
    change_calls: u32,
}

#[derive(Clone, Debug)]
pub struct MockServiceRecord {
    pub observation: ServiceObservation,
    pub worker_busy: bool,
    pub image_token: String,
    pub execution_identity_token: String,
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

    pub fn set_fail_change(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_change = Some(error);
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
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let record = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .services
            .get(&key)
            .cloned()
            .ok_or(ScmApiError::NotFound)?;
        Ok(ScmExactSnapshot {
            observation: record.observation,
            image_token: record.image_token,
            execution_identity_token: record.execution_identity_token,
        })
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
        if let Some(error) = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_change
            .take()
        {
            return Err(error);
        }
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
            record.observation.start_mode = mode;
        }
        guard.change_calls = guard.change_calls.saturating_add(1);
        Ok(())
    }
}

/// Exact-identity bound executor. Live mutation requires a sealed owner gate.
pub struct ScmControlExecutor<B> {
    backend: B,
    gate: LiveScmExecutionGate,
    expected_service_name: String,
    expected_image_token: String,
    expected_execution_identity_token: String,
}

impl<B: ScmControlBackend> ScmControlExecutor<B> {
    pub fn new(
        backend: B,
        expected_service_name: impl Into<String>,
        expected_image_token: impl Into<String>,
        expected_execution_identity_token: impl Into<String>,
        gate: LiveScmExecutionGate,
    ) -> Self {
        Self {
            backend,
            gate,
            expected_service_name: expected_service_name.into(),
            expected_image_token: expected_image_token.into(),
            expected_execution_identity_token: expected_execution_identity_token.into(),
        }
    }

    pub fn capture_prestate(
        &self,
        worker_busy: bool,
        admission_selector_absent: bool,
    ) -> Result<ServicePrestateCapture, ScmApiError> {
        let snap = self.query_bound_snapshot()?;
        let observed = &snap.observation;
        Ok(ServicePrestateCapture {
            service_name: observed.service_name.clone(),
            state: observed.state,
            start_mode: observed.start_mode,
            image_path_matches: observed.image_path_matches,
            ownership_verified: observed.ownership_verified,
            worker_busy,
            admission_selector_absent,
            image_token: snap.image_token,
            execution_identity_token: snap.execution_identity_token,
            transaction_id: new_transaction_id(),
        })
    }

    /// Returns the backend observation **without** rewriting trust flags.
    pub fn observe_verified(&self) -> Result<ServiceObservation, ScmApiError> {
        Ok(self.query_bound_snapshot()?.observation)
    }

    fn query_bound_snapshot(&self) -> Result<ScmExactSnapshot, ScmApiError> {
        if self.expected_service_name.contains('*') || self.expected_service_name.trim().is_empty()
        {
            return Err(ScmApiError::NameDrift);
        }
        let snap = self.backend.query_exact(&self.expected_service_name)?;
        if !snap
            .observation
            .service_name
            .eq_ignore_ascii_case(&self.expected_service_name)
        {
            return Err(ScmApiError::NameDrift);
        }
        // Never overwrite observation flags — refuse when evidence is incomplete
        // or disagrees with the bound executor identity.
        if !snap.observation.ownership_verified {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !snap.observation.image_path_matches {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !snap
            .image_token
            .eq_ignore_ascii_case(&self.expected_image_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !snap
            .execution_identity_token
            .eq_ignore_ascii_case(&self.expected_execution_identity_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        Ok(snap)
    }

    fn plan_matches_binding(&self, plan: &ServiceRestorePlan) -> Result<(), ScmApiError> {
        if !plan
            .service_name
            .eq_ignore_ascii_case(&self.expected_service_name)
        {
            return Err(ScmApiError::NameDrift);
        }
        if !plan
            .image_token
            .eq_ignore_ascii_case(&self.expected_image_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !plan
            .execution_identity_token
            .eq_ignore_ascii_case(&self.expected_execution_identity_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if plan.transaction_id.trim().is_empty() {
            return Err(ScmApiError::RestoreIncomplete);
        }
        Ok(())
    }

    /// Advances one prepare/execute step. Stop/Start only run when the sealed
    /// gate authorizes the exact bound service/image.
    pub fn advance(
        &self,
        desired: DesiredServiceLifecycle,
        admission_selector_absent: bool,
        worker_busy: bool,
        managed_admission: bool,
        prestate: &ServicePrestateCapture,
    ) -> ScmTransactionReceipt {
        let restore = Some(capture_restore_plan(prestate));
        if let Err(error) = self.plan_matches_binding(restore.as_ref().unwrap()) {
            return refused_receipt(&self.expected_service_name, error, restore);
        }
        let observed = match self.observe_verified() {
            Ok(observed) => observed,
            Err(error) => {
                return refused_receipt(&self.expected_service_name, error, restore);
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
                restore_pass: false,
            },
            ServiceControlPhase::WaitIdle { .. } => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::WaitingIdle,
                api_accepted: true,
                achieved: AchievedServiceControl::Draining,
                reason: None,
                restore,
                restore_pass: false,
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
                restore_pass: false,
            },
            ServiceControlPhase::Restore => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Restoring,
                api_accepted: true,
                achieved: AchievedServiceControl::Unknown,
                reason: None,
                restore,
                restore_pass: false,
            },
            ServiceControlPhase::Refused { reason } => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Refused,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(reason),
                restore,
                restore_pass: false,
            },
        }
    }

    fn execute_stop(&self, restore: Option<ServiceRestorePlan>) -> ScmTransactionReceipt {
        if !self.gate.authorizes(
            &self.expected_service_name,
            &self.expected_image_token,
            false,
            true,
            false,
        ) {
            return refused_receipt(
                &self.expected_service_name,
                ScmApiError::GateClosed,
                restore,
            );
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
                    restore_pass: false,
                },
                Err(error) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Interrupted,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                },
            },
            Err(error) => refused_receipt(&self.expected_service_name, error, restore),
        }
    }

    fn execute_start(&self, restore: Option<ServiceRestorePlan>) -> ScmTransactionReceipt {
        if !self.gate.authorizes(
            &self.expected_service_name,
            &self.expected_image_token,
            true,
            false,
            false,
        ) {
            return refused_receipt(
                &self.expected_service_name,
                ScmApiError::GateClosed,
                restore,
            );
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
                    restore_pass: false,
                },
                Err(error) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Interrupted,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                },
            },
            Err(error) => refused_receipt(&self.expected_service_name, error, restore),
        }
    }

    /// Applies restore plan under a sealed gate with independent step readback.
    ///
    /// `admission_selector_absent_now` is an independent admission observation
    /// supplied by the caller (never inferred from SCM alone).
    pub fn restore(
        &self,
        plan: &ServiceRestorePlan,
        admission_selector_absent_now: bool,
    ) -> ScmTransactionReceipt {
        let restore = Some(plan.clone());
        if let Err(error) = self.plan_matches_binding(plan) {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }
        if plan.requires_owner_gate
            && !self.gate.authorizes(
                &plan.service_name,
                &plan.image_token,
                plan.restore_running,
                !plan.restore_running,
                true,
            )
        {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(ScmApiError::GateClosed.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        if let Err(error) = self
            .backend
            .change_start_mode(&plan.service_name, plan.restore_start_mode)
        {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let after_mode = match self.observe_verified() {
            Ok(obs) => obs,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        };
        if after_mode.start_mode != plan.restore_start_mode {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: true,
                achieved: AchievedServiceControl::Unknown,
                reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let run_result = if plan.restore_running {
            self.backend.start(&plan.service_name)
        } else {
            self.backend.stop(&plan.service_name)
        };
        if let Err(error) = run_result {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let after = match self.observe_verified() {
            Ok(obs) => obs,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        };
        let running_now = matches!(
            after.state,
            DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
        );
        if running_now != plan.restore_running {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: true,
                achieved: achieved_from_observation(&after, false),
                reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }
        if admission_selector_absent_now != plan.admission_selector_absent {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: true,
                achieved: achieved_from_observation(&after, false),
                reason: Some("scm.admission-readback-mismatch".to_owned()),
                restore,
                restore_pass: false,
            };
        }

        ScmTransactionReceipt {
            service_name: plan.service_name.clone(),
            stage: ScmTransactionStage::Completed,
            api_accepted: true,
            achieved: achieved_from_observation(&after, false),
            reason: None,
            restore,
            restore_pass: true,
        }
    }
}

fn refused_receipt(
    service_name: &str,
    error: ScmApiError,
    restore: Option<ServiceRestorePlan>,
) -> ScmTransactionReceipt {
    ScmTransactionReceipt {
        service_name: service_name.to_owned(),
        stage: ScmTransactionStage::Refused,
        api_accepted: false,
        achieved: AchievedServiceControl::Refused,
        reason: Some(error.as_str().to_owned()),
        restore,
        restore_pass: false,
    }
}

fn new_transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("scm-tx-{nanos}")
}

/// Opaque fingerprint helper for image paths (never store raw private paths).
pub fn opaque_image_token(image_path: &str) -> String {
    let digest = Sha256::digest(image_path.trim().to_ascii_lowercase().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Windows SCM backend: read-only query may run; mutation always honors the gate.
///
/// This Train never opens a gate from Owner bools. Even when compiled, Start/Stop/
/// ChangeConfig return [`ScmApiError::GateClosed`] unless a sealed proof opened
/// the executor gate (still not exercised against live SCM here).
#[derive(Clone, Debug)]
pub struct WindowsScmBackend {
    /// When false, even QueryStatus returns GateClosed (default for safety).
    allow_read_only_query: bool,
}

impl Default for WindowsScmBackend {
    fn default() -> Self {
        Self {
            // Read-only query is available for future Owner-prepared observation,
            // but mutation paths remain inert without a sealed executor gate.
            allow_read_only_query: true,
        }
    }
}

impl WindowsScmBackend {
    pub fn mutation_inert() -> Self {
        Self::default()
    }
}

impl ScmControlBackend for WindowsScmBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        if !self.allow_read_only_query {
            return Err(ScmApiError::GateClosed);
        }
        if service_name.contains('*') || service_name.trim().is_empty() {
            return Err(ScmApiError::NameDrift);
        }
        #[cfg(windows)]
        {
            windows_query_exact(service_name)
        }
        #[cfg(not(windows))]
        {
            let _ = service_name;
            Err(ScmApiError::Unavailable)
        }
    }

    fn start(&self, _service_name: &str) -> Result<(), ScmApiError> {
        // Hard stop: this adapter never issues Start in SOURCE_AND_SANDBOX_ONLY.
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

#[cfg(windows)]
fn windows_query_exact(service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
        QueryServiceStatusEx, ENUM_SERVICE_STATUS_PROCESSW, QUERY_SERVICE_CONFIGW,
        SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    };

    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(ScmApiError::AccessDenied);
        }
        let wide: Vec<u16> = service_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let service = OpenServiceW(
            scm,
            wide.as_ptr(),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        );
        if service.is_null() {
            CloseServiceHandle(scm);
            return Err(ScmApiError::NotFound);
        }

        let mut needed = 0u32;
        QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            ptr::null_mut(),
            0,
            &mut needed,
        );
        let mut status_buf = vec![0u8; needed as usize];
        let status_ok = QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            status_buf.as_mut_ptr(),
            needed,
            &mut needed,
        );
        if status_ok == 0 {
            CloseServiceHandle(service);
            CloseServiceHandle(scm);
            return Err(ScmApiError::Unavailable);
        }
        let status = &*(status_buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW);
        let state = map_dw_state(status.ServiceStatusProcess.dwCurrentState);

        needed = 0;
        QueryServiceConfigW(service, ptr::null_mut(), 0, &mut needed);
        let mut config_buf = vec![0u8; needed as usize];
        let config_ok = QueryServiceConfigW(
            service,
            config_buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW,
            needed,
            &mut needed,
        );
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if config_ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        let config = &*(config_buf.as_ptr() as *const QUERY_SERVICE_CONFIGW);
        let image = wide_to_string(config.lpBinaryPathName);
        let start_name = wide_to_string(config.lpServiceStartName);
        let image_token = opaque_image_token(image.trim_matches('"'));
        let execution_identity_token = opaque_image_token(&start_name);
        let start_mode = map_start_type(config.dwStartType);

        Ok(ScmExactSnapshot {
            observation: ServiceObservation {
                service_name: service_name.to_owned(),
                state,
                start_mode,
                // Honest flags from successful query — caller/executor compares tokens.
                image_path_matches: !image.trim().is_empty(),
                ownership_verified: !start_name.trim().is_empty(),
            },
            image_token,
            execution_identity_token,
        })
    }
}

#[cfg(windows)]
fn map_dw_state(state: u32) -> DiscoveredLifecycle {
    use windows_sys::Win32::System::Services::{
        SERVICE_PAUSED, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STOPPED,
        SERVICE_STOP_PENDING,
    };
    match state {
        SERVICE_RUNNING => DiscoveredLifecycle::Listening,
        SERVICE_STOPPED => DiscoveredLifecycle::Stopped,
        SERVICE_START_PENDING | SERVICE_STOP_PENDING | SERVICE_PAUSED => {
            DiscoveredLifecycle::Unknown
        }
        _ => DiscoveredLifecycle::Unknown,
    }
}

#[cfg(windows)]
fn map_start_type(start_type: u32) -> ServiceStartMode {
    use windows_sys::Win32::System::Services::{
        SERVICE_AUTO_START, SERVICE_DEMAND_START, SERVICE_DISABLED,
    };
    match start_type {
        SERVICE_AUTO_START => ServiceStartMode::Automatic,
        SERVICE_DEMAND_START => ServiceStartMode::Manual,
        SERVICE_DISABLED => ServiceStartMode::Disabled,
        _ => ServiceStartMode::Unknown,
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

/// Backward-compatible alias: always mutation-inert.
#[derive(Clone, Copy, Debug, Default)]
pub struct GatedWindowsScmBackend;

impl ScmControlBackend for GatedWindowsScmBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        WindowsScmBackend::mutation_inert().query_exact(service_name)
    }

    fn start(&self, service_name: &str) -> Result<(), ScmApiError> {
        WindowsScmBackend::mutation_inert().start(service_name)
    }

    fn stop(&self, service_name: &str) -> Result<(), ScmApiError> {
        WindowsScmBackend::mutation_inert().stop(service_name)
    }

    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        WindowsScmBackend::mutation_inert().change_start_mode(service_name, mode)
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
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
        }
    }

    fn open_gate() -> LiveScmExecutionGate {
        LiveScmExecutionGate::from_owner_authorization(
            &OwnerScmAuthorizationProof::synthetic_for_tests(
                "fixture.scm.a",
                "fixture-image",
                true,
                true,
                true,
            ),
        )
        .unwrap()
    }

    fn executor(
        mock: MockScmBackend,
        gate: LiveScmExecutionGate,
    ) -> ScmControlExecutor<MockScmBackend> {
        ScmControlExecutor::new(
            mock,
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            gate,
        )
    }

    #[test]
    fn observe_verified_does_not_overwrite_false_trust_flags() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.observation.ownership_verified = false;
        mock.insert(record);
        let exec = executor(mock, LiveScmExecutionGate::CLOSED);
        assert!(matches!(
            exec.observe_verified(),
            Err(ScmApiError::IdentityMismatch)
        ));
    }

    #[test]
    fn image_token_mismatch_is_refused() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.image_token = "other-image".to_owned();
        mock.insert(record);
        let exec = executor(mock, LiveScmExecutionGate::CLOSED);
        assert!(matches!(
            exec.observe_verified(),
            Err(ScmApiError::IdentityMismatch)
        ));
    }

    #[test]
    fn tampered_owner_proof_cannot_open_gate() {
        let mut proof = OwnerScmAuthorizationProof::synthetic_for_tests(
            "fixture.scm.a",
            "fixture-image",
            true,
            true,
            true,
        );
        proof.seal = "deadbeef".to_owned();
        assert!(matches!(
            LiveScmExecutionGate::from_owner_authorization(&proof),
            Err(ScmApiError::GateClosed)
        ));
    }

    #[test]
    fn closed_gate_refuses_stop_without_mutation() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), LiveScmExecutionGate::CLOSED);
        let prestate = exec.capture_prestate(false, true).unwrap();
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.reason.as_deref(), Some("scm.live-gate-closed"));
        assert_eq!(mock.stop_calls(), 0);
        assert!(!receipt.restore_pass);
    }

    #[test]
    fn open_gate_stop_readback_reports_stopped() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), open_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let receipt = exec.advance(
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
    fn restore_fails_when_change_config_errors() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        mock.set_fail_change(ScmApiError::AccessDenied);
        let exec = executor(mock, open_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let receipt = exec.restore(&plan, true);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.reason.as_deref(), Some("scm.access-denied"));
    }

    #[test]
    fn restore_fails_when_admission_readback_mismatches() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        // Plan expects selector absent=true; supply false independent readback.
        let receipt = exec.restore(&plan, false);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt.restore_pass);
    }

    #[test]
    fn restore_pass_requires_full_independent_readback() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let receipt = exec.restore(&plan, true);
        assert!(receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::Completed);
    }

    #[test]
    fn restore_rejects_plan_for_different_service_binding() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_gate());
        let mut prestate = exec.capture_prestate(false, true).unwrap();
        prestate.service_name = "fixture.scm.other".to_owned();
        let plan = capture_restore_plan(&prestate);
        let receipt = exec.restore(&plan, true);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
    }

    #[test]
    fn busy_worker_never_issues_stop() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.worker_busy = true;
        record.observation.state = DiscoveredLifecycle::Busy;
        mock.insert(record);
        let exec = executor(mock.clone(), open_gate());
        let prestate = exec.capture_prestate(true, true).unwrap();
        let receipt = exec.advance(
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
    fn windows_backend_mutations_are_inert() {
        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.start("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.stop("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.change_start_mode("fixture.scm.a", ServiceStartMode::Manual),
            Err(ScmApiError::GateClosed)
        ));
    }
}
