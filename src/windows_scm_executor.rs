//! Windows SCM control executor for future HUX-2 (mutation arm sealed closed).
//!
//! Authorization model:
//! - [`SyntheticScmTestGrant`] opens a gate usable **only** with mock/synthetic
//!   flows. It is forgeable by design and must never be treated as Owner auth.
//! - [`TrustedOwnerAuthorizationVerifier`] is the sole live-auth entry. This
//!   Train ships [`UnissuedOwnerAuthorizationVerifier`], which always refuses.
//!
//! Win32 `QueryServiceStatusEx(SC_STATUS_PROCESS_INFO)` returns
//! [`SERVICE_STATUS_PROCESS`], not `ENUM_SERVICE_STATUS_PROCESSW`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    achieved_from_observation, capture_restore_plan, next_control_phase_with_admission,
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle, ServiceControlPhase,
    ServiceObservation, ServicePrestateCapture, ServiceRestorePlan, ServiceStartMode,
};

/// Independent GitHub admission selector observation used by restore verification.
///
/// Callers must not pass a bare boolean as accepted fact; the observer performs
/// a fresh read (fake or live) at verification time.
pub trait AdmissionSelectorObserver {
    fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError>;
}

/// Request presented to a trusted Owner authorization verifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedOwnerAuthorizationRequest {
    pub service_name: String,
    pub image_token: String,
    pub allow_start: bool,
    pub allow_stop: bool,
    pub allow_change_config: bool,
    /// Opaque Owner material (never logged). Empty in this Train.
    pub owner_material: Vec<u8>,
}

/// Live/trusted authorization entry. Implementations must not accept synthetic grants.
pub trait TrustedOwnerAuthorizationVerifier {
    fn verify(
        &self,
        request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<LiveScmExecutionGate, ScmApiError>;
}

/// Default verifier for SOURCE_AND_SANDBOX_ONLY: never issues live rights.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnissuedOwnerAuthorizationVerifier;

impl TrustedOwnerAuthorizationVerifier for UnissuedOwnerAuthorizationVerifier {
    fn verify(
        &self,
        _request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<LiveScmExecutionGate, ScmApiError> {
        Err(ScmApiError::AuthorizationNotIssued)
    }
}

/// Explicitly forgeable test grant. Not Owner authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntheticScmTestGrant {
    service_name: String,
    image_token: String,
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
}

impl SyntheticScmTestGrant {
    pub fn new(
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            image_token: image_token.into(),
            allow_start,
            allow_stop,
            allow_change_config,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScmGrantKind {
    Closed,
    /// Forgeable; valid only for mock/synthetic qualification.
    SyntheticTestOnly,
    /// Reserved for future trusted verifier issuance (not constructed this Train).
    #[allow(dead_code)]
    TrustedLive,
}

/// Hard gate for SCM mutation. Public bool construction is impossible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveScmExecutionGate {
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
    grant_kind: ScmGrantKind,
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
        grant_kind: ScmGrantKind::Closed,
        bound_service_name: String::new(),
        bound_image_token: String::new(),
    };

    /// Opens a **synthetic test** gate. Never use for live Windows mutation.
    pub fn from_synthetic_test_grant(grant: &SyntheticScmTestGrant) -> Result<Self, ScmApiError> {
        if grant.service_name.trim().is_empty() || grant.service_name.contains('*') {
            return Err(ScmApiError::NameDrift);
        }
        Ok(Self {
            allow_start: grant.allow_start,
            allow_stop: grant.allow_stop,
            allow_change_config: grant.allow_change_config,
            grant_kind: ScmGrantKind::SyntheticTestOnly,
            bound_service_name: grant.service_name.clone(),
            bound_image_token: grant.image_token.clone(),
        })
    }

    /// Live path: delegates to a trusted verifier (unissued this Train).
    pub fn from_trusted_owner_authorization<V: TrustedOwnerAuthorizationVerifier>(
        verifier: &V,
        request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<Self, ScmApiError> {
        verifier.verify(request)
    }

    pub fn allows_mutation(&self) -> bool {
        !matches!(self.grant_kind, ScmGrantKind::Closed)
            && (self.allow_start || self.allow_stop || self.allow_change_config)
    }

    pub fn is_synthetic_test_only(&self) -> bool {
        matches!(self.grant_kind, ScmGrantKind::SyntheticTestOnly)
    }

    pub fn is_trusted_live(&self) -> bool {
        matches!(self.grant_kind, ScmGrantKind::TrustedLive)
    }

    fn authorizes(
        &self,
        service_name: &str,
        image_token: &str,
        need_start: bool,
        need_stop: bool,
        need_config: bool,
    ) -> bool {
        if matches!(self.grant_kind, ScmGrantKind::Closed) {
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
    AuthorizationNotIssued,
    AdmissionObserveFailed,
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
            Self::AuthorizationNotIssued => "scm.authorization-not-issued",
            Self::AdmissionObserveFailed => "scm.admission-observe-failed",
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
    /// Remaining successful `query_exact` calls before the next query fails.
    fail_after_queries: Option<(u32, ScmApiError)>,
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

    /// Allow `successful_before_fail` more successful queries, then fail once.
    pub fn set_fail_after_queries(&self, successful_before_fail: u32, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_after_queries = Some((successful_before_fail, error));
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

    pub fn change_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .change_calls
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
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((remaining, error)) = guard.fail_after_queries {
            if remaining == 0 {
                guard.fail_after_queries = None;
                return Err(error);
            }
            guard.fail_after_queries = Some((remaining.saturating_sub(1), error));
        }
        let record = guard
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

/// Fake GitHub admission backend for synthetic HUX-2 (never talks to GitHub).
#[derive(Clone, Default)]
pub struct FakeGithubAdmissionBackend {
    inner: Arc<Mutex<FakeGithubState>>,
}

#[derive(Clone, Debug)]
struct FakeGithubState {
    selector_present: bool,
    /// Desired presence after pending eventual-visibility lag expires.
    pending_selector_present: Option<bool>,
    /// Observe calls remaining before `pending_selector_present` becomes visible.
    eventual_visibility_lag: u32,
    fail_next_observe: Option<ScmApiError>,
    fail_next_withdraw: Option<ScmApiError>,
    fail_next_restore: Option<ScmApiError>,
    disconnected: bool,
    stale_token: bool,
    /// Opaque registration-scope token expected by the Agent binding.
    expected_scope_token: Option<String>,
    /// Opaque registration-scope token last observed from the fake API surface.
    observed_scope_token: Option<String>,
    withdraw_calls: u32,
    restore_calls: u32,
    observe_calls: u32,
}

impl Default for FakeGithubState {
    fn default() -> Self {
        Self {
            selector_present: true,
            pending_selector_present: None,
            eventual_visibility_lag: 0,
            fail_next_observe: None,
            fail_next_withdraw: None,
            fail_next_restore: None,
            disconnected: false,
            stale_token: false,
            expected_scope_token: None,
            observed_scope_token: None,
            withdraw_calls: 0,
            restore_calls: 0,
            observe_calls: 0,
        }
    }
}

impl FakeGithubAdmissionBackend {
    pub fn new_with_selector_present(present: bool) -> Self {
        let backend = Self::default();
        backend
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selector_present = present;
        backend
    }

    pub fn set_fail_next_observe(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next_observe = Some(error);
    }

    pub fn set_fail_next_withdraw(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next_withdraw = Some(error);
    }

    pub fn set_fail_next_restore(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next_restore = Some(error);
    }

    pub fn set_disconnected(&self, disconnected: bool) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .disconnected = disconnected;
    }

    /// Models a stale / expired GitHub token: all mutations and observations fail closed.
    pub fn set_stale_token(&self, stale: bool) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stale_token = stale;
    }

    /// After a successful withdraw/restore, keep the previous observed value for
    /// `lag` observe calls before the new presence becomes visible.
    pub fn set_eventual_visibility_lag(&self, lag: u32) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .eventual_visibility_lag = lag;
    }

    /// Binds FakeGithub observations/mutations to an opaque registration scope.
    /// When `observed` diverges from `expected`, the backend fails closed with
    /// `IdentityMismatch` and does not rewrite selector presence.
    pub fn set_scope_tokens(&self, expected: impl Into<String>, observed: impl Into<String>) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.expected_scope_token = Some(expected.into());
        guard.observed_scope_token = Some(observed.into());
    }

    fn refuse_scope_drift(guard: &FakeGithubState) -> Result<(), ScmApiError> {
        match (
            guard.expected_scope_token.as_deref(),
            guard.observed_scope_token.as_deref(),
        ) {
            (Some(expected), Some(observed)) if expected != observed => {
                Err(ScmApiError::IdentityMismatch)
            }
            _ => Ok(()),
        }
    }

    pub fn withdraw_capacity(&self) -> Result<(), ScmApiError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if guard.stale_token {
            return Err(ScmApiError::AccessDenied);
        }
        if guard.disconnected {
            return Err(ScmApiError::Unavailable);
        }
        Self::refuse_scope_drift(&guard)?;
        if let Some(error) = guard.fail_next_withdraw.take() {
            return Err(error);
        }
        Self::queue_presence(&mut guard, false);
        guard.withdraw_calls = guard.withdraw_calls.saturating_add(1);
        Ok(())
    }

    pub fn restore_capacity(&self) -> Result<(), ScmApiError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if guard.stale_token {
            return Err(ScmApiError::AccessDenied);
        }
        if guard.disconnected {
            return Err(ScmApiError::Unavailable);
        }
        Self::refuse_scope_drift(&guard)?;
        if let Some(error) = guard.fail_next_restore.take() {
            return Err(error);
        }
        Self::queue_presence(&mut guard, true);
        guard.restore_calls = guard.restore_calls.saturating_add(1);
        Ok(())
    }

    fn queue_presence(guard: &mut FakeGithubState, present: bool) {
        if guard.eventual_visibility_lag == 0 {
            guard.selector_present = present;
            guard.pending_selector_present = None;
            return;
        }
        guard.pending_selector_present = Some(present);
    }

    fn advance_eventual_visibility(guard: &mut FakeGithubState) {
        let Some(pending) = guard.pending_selector_present else {
            return;
        };
        // Consume one lag slot per observe while keeping the previous value.
        if guard.eventual_visibility_lag > 0 {
            guard.eventual_visibility_lag = guard.eventual_visibility_lag.saturating_sub(1);
            return;
        }
        guard.selector_present = pending;
        guard.pending_selector_present = None;
    }

    pub fn withdraw_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .withdraw_calls
    }

    pub fn restore_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .restore_calls
    }

    pub fn observe_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .observe_calls
    }
}

impl AdmissionSelectorObserver for FakeGithubAdmissionBackend {
    fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.observe_calls = guard.observe_calls.saturating_add(1);
        if guard.stale_token {
            return Err(ScmApiError::AccessDenied);
        }
        if let Some(error) = guard.fail_next_observe.take() {
            return Err(error);
        }
        if guard.disconnected {
            return Err(ScmApiError::Unavailable);
        }
        Self::refuse_scope_drift(&guard)?;
        Self::advance_eventual_visibility(&mut guard);
        Ok(!guard.selector_present)
    }
}

/// Exact-identity bound executor. Live mutation requires a trusted Owner gate.
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

    /// Advances one prepare/execute step.
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
    /// Admission result is obtained from [`AdmissionSelectorObserver`] at
    /// verification time — never from a caller-supplied boolean fact.
    pub fn restore<A: AdmissionSelectorObserver>(
        &self,
        plan: &ServiceRestorePlan,
        admission: &mut A,
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

        let admission_absent = match admission.observe_selector_absent() {
            Ok(absent) => absent,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: true,
                    achieved: achieved_from_observation(&after, false),
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        };
        if admission_absent != plan.admission_selector_absent {
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

/// Unconstructable mutation arm. Public API never produces a live arm this Train.
#[derive(Clone, Debug)]
pub struct ScmMutationArm {
    _private: (),
}

/// Windows SCM backend with real query source and sealed write paths.
#[derive(Clone, Debug)]
pub struct WindowsScmBackend {
    allow_read_only_query: bool,
    /// Always `None` via [`WindowsScmBackend::mutation_inert`]. Write APIs exist
    /// in source for review but refuse without an arm.
    mutation_arm: Option<ScmMutationArm>,
}

impl Default for WindowsScmBackend {
    fn default() -> Self {
        Self::mutation_inert()
    }
}

impl WindowsScmBackend {
    pub fn mutation_inert() -> Self {
        Self {
            allow_read_only_query: true,
            mutation_arm: None,
        }
    }

    /// Future Owner-transaction entry. Not callable without a privately minted arm.
    #[allow(dead_code)]
    fn with_mutation_arm(arm: ScmMutationArm) -> Self {
        Self {
            allow_read_only_query: true,
            mutation_arm: Some(arm),
        }
    }

    fn require_exact_name(service_name: &str) -> Result<(), ScmApiError> {
        if service_name.contains('*') || service_name.trim().is_empty() {
            return Err(ScmApiError::NameDrift);
        }
        Ok(())
    }
}

impl ScmControlBackend for WindowsScmBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        if !self.allow_read_only_query {
            return Err(ScmApiError::GateClosed);
        }
        Self::require_exact_name(service_name)?;
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

    fn start(&self, service_name: &str) -> Result<(), ScmApiError> {
        Self::require_exact_name(service_name)?;
        if self.mutation_arm.is_none() {
            return Err(ScmApiError::GateClosed);
        }
        #[cfg(windows)]
        {
            windows_start_service(service_name)
        }
        #[cfg(not(windows))]
        {
            let _ = service_name;
            Err(ScmApiError::Unavailable)
        }
    }

    fn stop(&self, service_name: &str) -> Result<(), ScmApiError> {
        Self::require_exact_name(service_name)?;
        if self.mutation_arm.is_none() {
            return Err(ScmApiError::GateClosed);
        }
        #[cfg(windows)]
        {
            windows_stop_service(service_name)
        }
        #[cfg(not(windows))]
        {
            let _ = service_name;
            Err(ScmApiError::Unavailable)
        }
    }

    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        Self::require_exact_name(service_name)?;
        if self.mutation_arm.is_none() {
            return Err(ScmApiError::GateClosed);
        }
        #[cfg(windows)]
        {
            windows_change_start_mode(service_name, mode)
        }
        #[cfg(not(windows))]
        {
            let _ = (service_name, mode);
            Err(ScmApiError::Unavailable)
        }
    }
}

#[cfg(windows)]
fn windows_query_exact(service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
    use std::mem::{align_of, size_of, MaybeUninit};
    use std::ptr;
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
        QueryServiceStatusEx, QUERY_SERVICE_CONFIGW, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
        SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_STATUS_PROCESS,
    };

    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(
            scm,
            wide.as_ptr(),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        );
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }

        // SC_STATUS_PROCESS_INFO returns SERVICE_STATUS_PROCESS (fixed size).
        let mut status = MaybeUninit::<SERVICE_STATUS_PROCESS>::zeroed();
        let mut needed = 0u32;
        let status_ok = QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            status.as_mut_ptr().cast::<u8>(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        );
        if status_ok == 0 {
            CloseServiceHandle(service);
            CloseServiceHandle(scm);
            return Err(ScmApiError::Unavailable);
        }
        let status = status.assume_init();
        let state = map_dw_state(status.dwCurrentState);

        needed = 0;
        QueryServiceConfigW(service, ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            CloseServiceHandle(service);
            CloseServiceHandle(scm);
            return Err(ScmApiError::Unavailable);
        }
        let align = align_of::<QUERY_SERVICE_CONFIGW>();
        let size = needed as usize;
        let mut raw = vec![0u8; size + align];
        let unaligned = raw.as_mut_ptr() as usize;
        let aligned = (unaligned + align - 1) & !(align - 1);
        let config_ptr = aligned as *mut QUERY_SERVICE_CONFIGW;
        let config_ok = QueryServiceConfigW(service, config_ptr, needed, &mut needed);
        let last_cfg_err = windows_sys::Win32::Foundation::GetLastError();
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if config_ok == 0 {
            return Err(if last_cfg_err == ERROR_ACCESS_DENIED {
                ScmApiError::AccessDenied
            } else {
                ScmApiError::Unavailable
            });
        }
        let config = &*config_ptr;
        let image = wide_to_string(config.lpBinaryPathName);
        let start_name = wide_to_string(config.lpServiceStartName);
        let image_token = opaque_image_token(image.trim_matches('"'));
        let execution_identity_token = opaque_image_token(&start_name);
        let start_mode = map_start_type(config.dwStartType);

        let _ = ERROR_SERVICE_DOES_NOT_EXIST;
        Ok(ScmExactSnapshot {
            observation: ServiceObservation {
                service_name: service_name.to_owned(),
                state,
                start_mode,
                image_path_matches: !image.trim().is_empty(),
                ownership_verified: !start_name.trim().is_empty(),
            },
            image_token,
            execution_identity_token,
        })
    }
}

/// Real StartServiceW path — only reachable with a privately minted mutation arm.
#[cfg(windows)]
fn windows_start_service(service_name: &str) -> Result<(), ScmApiError> {
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW, SC_MANAGER_CONNECT,
        SERVICE_START,
    };
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(scm, wide.as_ptr(), SERVICE_START);
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }
        let ok = StartServiceW(service, 0, ptr::null());
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn windows_stop_service(service_name: &str) -> Result<(), ScmApiError> {
    use std::mem::MaybeUninit;
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CONTROL_STOP, SERVICE_STATUS, SERVICE_STOP,
    };
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(scm, wide.as_ptr(), SERVICE_STOP);
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }
        let mut status = MaybeUninit::<SERVICE_STATUS>::zeroed();
        let ok = ControlService(service, SERVICE_CONTROL_STOP, status.as_mut_ptr());
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn windows_change_start_mode(
    service_name: &str,
    mode: ServiceStartMode,
) -> Result<(), ScmApiError> {
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CHANGE_CONFIG, SERVICE_NO_CHANGE,
    };
    let start_type = match mode {
        ServiceStartMode::Automatic => windows_sys::Win32::System::Services::SERVICE_AUTO_START,
        ServiceStartMode::Manual => windows_sys::Win32::System::Services::SERVICE_DEMAND_START,
        ServiceStartMode::Disabled => windows_sys::Win32::System::Services::SERVICE_DISABLED,
        ServiceStartMode::Unknown => return Err(ScmApiError::StateUnknown),
    };
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(scm, wide.as_ptr(), SERVICE_CHANGE_CONFIG);
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }
        let ok = ChangeServiceConfigW(
            service,
            SERVICE_NO_CHANGE,
            start_type,
            SERVICE_NO_CHANGE,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
        );
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn map_last_error_open() -> ScmApiError {
    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST,
    };
    match unsafe { GetLastError() } {
        ERROR_ACCESS_DENIED => ScmApiError::AccessDenied,
        ERROR_SERVICE_DOES_NOT_EXIST => ScmApiError::NotFound,
        _ => ScmApiError::Unavailable,
    }
}

#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
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

/// Strongly typed narrow action supported by the privileged SCM helper.
///
/// Specifically excludes blanket elevation, broad service manager control,
/// foreign service configuration, and generic process termination (kill_pid).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrivilegedScmAction {
    StartService,
    StopService,
    ChangeStartMode { mode: ServiceStartMode },
}

/// Request presented to the privileged SCM helper by the Agent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedScmRequest {
    pub action: PrivilegedScmAction,
    pub service_name: String,
    pub expected_image_token: String,
    pub expected_execution_identity_token: String,
    pub transaction_id: String,
}

/// Verifiable report emitted by the privileged SCM helper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedScmReport {
    pub service_name: String,
    pub transaction_id: String,
    pub applied: bool,
    pub achieved_state: DiscoveredLifecycle,
    pub achieved_start_mode: ServiceStartMode,
    pub refusal_reason: Option<String>,
}

/// Narrow privileged helper executing strictly scoped Windows SCM mutations.
///
/// Ordinary user-session Agent delegates only exact, single-service actions here.
/// Does not accept wildcards, foreign services, or blanket administrative elevation.
pub struct PrivilegedScmHelper<B> {
    backend: B,
    gate: LiveScmExecutionGate,
}

impl<B: ScmControlBackend> PrivilegedScmHelper<B> {
    pub fn new(backend: B, gate: LiveScmExecutionGate) -> Self {
        Self { backend, gate }
    }

    pub fn dispatch(&self, request: &PrivilegedScmRequest) -> PrivilegedScmReport {
        let name = request.service_name.trim();
        if name.is_empty()
            || name.contains('*')
            || name.contains('?')
            || name.contains('\\')
            || name.contains('/')
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::NameDrift.as_str().to_owned()),
            };
        }
        if request.transaction_id.trim().is_empty() {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
            };
        }

        // 1. Verify gate permission for the specific action
        let (need_start, need_stop, need_config) = match request.action {
            PrivilegedScmAction::StartService => (true, false, false),
            PrivilegedScmAction::StopService => (false, true, false),
            PrivilegedScmAction::ChangeStartMode { .. } => (false, false, true),
        };
        if !self.gate.authorizes(
            name,
            &request.expected_image_token,
            need_start,
            need_stop,
            need_config,
        ) {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::GateClosed.as_str().to_owned()),
            };
        }

        // 2. Query exact host state & verify identity (foreign service refusal)
        let snapshot = match self.backend.query_exact(name) {
            Ok(snap) => snap,
            Err(error) => {
                return PrivilegedScmReport {
                    service_name: request.service_name.clone(),
                    transaction_id: request.transaction_id.clone(),
                    applied: false,
                    achieved_state: DiscoveredLifecycle::Unknown,
                    achieved_start_mode: ServiceStartMode::Disabled,
                    refusal_reason: Some(error.as_str().to_owned()),
                };
            }
        };

        if !snapshot.observation.service_name.eq_ignore_ascii_case(name) {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(ScmApiError::NameDrift.as_str().to_owned()),
            };
        }
        if !snapshot.observation.ownership_verified
            || !snapshot.observation.image_path_matches
            || !snapshot
                .image_token
                .eq_ignore_ascii_case(&request.expected_image_token)
            || !snapshot
                .execution_identity_token
                .eq_ignore_ascii_case(&request.expected_execution_identity_token)
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(ScmApiError::IdentityMismatch.as_str().to_owned()),
            };
        }

        // 3. Worker Busy check for StopService
        if matches!(request.action, PrivilegedScmAction::StopService)
            && matches!(snapshot.observation.state, DiscoveredLifecycle::Busy)
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(ScmApiError::BusyWorker.as_str().to_owned()),
            };
        }

        // 4. Perform narrow mutation
        let mutate_result = match request.action {
            PrivilegedScmAction::StartService => self.backend.start(name),
            PrivilegedScmAction::StopService => self.backend.stop(name),
            PrivilegedScmAction::ChangeStartMode { mode } => {
                self.backend.change_start_mode(name, mode)
            }
        };
        if let Err(error) = mutate_result {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(error.as_str().to_owned()),
            };
        }

        // 5. Immediate post-action readback
        let after = match self.backend.query_exact(name) {
            Ok(snap) => snap,
            Err(error) => {
                return PrivilegedScmReport {
                    service_name: request.service_name.clone(),
                    transaction_id: request.transaction_id.clone(),
                    applied: true,
                    achieved_state: DiscoveredLifecycle::Unknown,
                    achieved_start_mode: ServiceStartMode::Disabled,
                    refusal_reason: Some(error.as_str().to_owned()),
                };
            }
        };

        // Verify transition succeeded
        let matches_expectation = match request.action {
            PrivilegedScmAction::StartService => matches!(
                after.observation.state,
                DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
            ),
            PrivilegedScmAction::StopService => {
                matches!(after.observation.state, DiscoveredLifecycle::Stopped)
            }
            PrivilegedScmAction::ChangeStartMode { mode } => after.observation.start_mode == mode,
        };

        if !matches_expectation {
            PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: true,
                achieved_state: after.observation.state,
                achieved_start_mode: after.observation.start_mode,
                refusal_reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
            }
        } else {
            PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: true,
                achieved_state: after.observation.state,
                achieved_start_mode: after.observation.start_mode,
                refusal_reason: None,
            }
        }
    }
}

pub const SCM_TRANSACTION_JOURNAL_SCHEMA_VERSION: u32 = 1;

/// Durable transaction journal written to resilient private storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScmTransactionJournal {
    pub schema_version: u32,
    pub transaction_id: String,
    pub service_name: String,
    pub expected_image_token: String,
    pub expected_execution_identity_token: String,
    pub prestate: ServicePrestateCapture,
    pub stage: ScmTransactionStage,
    pub restore_plan: ServiceRestorePlan,
    pub updated_at_unix_ms: u64,
}

impl ScmTransactionJournal {
    pub fn new(
        prestate: ServicePrestateCapture,
        expected_image_token: impl Into<String>,
        expected_execution_identity_token: impl Into<String>,
        updated_at_unix_ms: u64,
    ) -> Self {
        let restore_plan = capture_restore_plan(&prestate);
        Self {
            schema_version: SCM_TRANSACTION_JOURNAL_SCHEMA_VERSION,
            transaction_id: prestate.transaction_id.clone(),
            service_name: prestate.service_name.clone(),
            expected_image_token: expected_image_token.into(),
            expected_execution_identity_token: expected_execution_identity_token.into(),
            stage: ScmTransactionStage::CapturedPrestate,
            restore_plan,
            prestate,
            updated_at_unix_ms,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.stage,
            ScmTransactionStage::Completed
                | ScmTransactionStage::Refused
                | ScmTransactionStage::RestoreFailed
        )
    }

    pub fn record_stage(&mut self, stage: ScmTransactionStage, updated_at_unix_ms: u64) {
        self.stage = stage;
        self.updated_at_unix_ms = updated_at_unix_ms;
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        crate::installation::atomic_write(path, &bytes)
    }

    pub fn load_from_path(path: &Path) -> io::Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        if crate::installation::is_reparse_point(path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing reparse journal {}", path.display()),
            ));
        }
        let bytes = fs::read(path)?;
        let journal: Self = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        if journal.schema_version != SCM_TRANSACTION_JOURNAL_SCHEMA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported journal schema version {}",
                    journal.schema_version
                ),
            ));
        }
        Ok(Some(journal))
    }

    pub fn clear(path: &Path) -> io::Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        if crate::installation::is_reparse_point(path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing reparse journal {}", path.display()),
            ));
        }
        fs::remove_file(path)?;
        Ok(true)
    }
}

/// Reconciles an interrupted transaction journal after an Agent crash or restart.
///
/// CRITICAL SAFETY CONTRACT:
/// A restarted Agent resuming an in-flight journal NEVER marks `restore_pass: true`
/// or reports `Completed` without fresh, independent readbacks confirming that
/// the service state, start mode, AND admission presence truthfully match the restore plan.
pub fn reconcile_interrupted_scm_journal<B: ScmControlBackend, A: AdmissionSelectorObserver>(
    journal: &mut ScmTransactionJournal,
    executor: &ScmControlExecutor<B>,
    admission: &mut A,
    current_time_unix_ms: u64,
) -> ScmTransactionReceipt {
    let current_observation = match executor.observe_verified() {
        Ok(obs) => obs,
        Err(error) => {
            journal.record_stage(ScmTransactionStage::Interrupted, current_time_unix_ms);
            return ScmTransactionReceipt {
                service_name: journal.service_name.clone(),
                stage: ScmTransactionStage::Interrupted,
                api_accepted: false,
                achieved: AchievedServiceControl::Unknown,
                reason: Some(format!("resume-observe-failed: {}", error.as_str())),
                restore: Some(journal.restore_plan.clone()),
                restore_pass: false,
            };
        }
    };

    let admission_absent = match admission.observe_selector_absent() {
        Ok(absent) => absent,
        Err(error) => {
            journal.record_stage(ScmTransactionStage::Interrupted, current_time_unix_ms);
            return ScmTransactionReceipt {
                service_name: journal.service_name.clone(),
                stage: ScmTransactionStage::Interrupted,
                api_accepted: false,
                achieved: achieved_from_observation(&current_observation, false),
                reason: Some(format!(
                    "resume-admission-observe-failed: {}",
                    error.as_str()
                )),
                restore: Some(journal.restore_plan.clone()),
                restore_pass: false,
            };
        }
    };

    let running_now = matches!(
        current_observation.state,
        DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
    );

    // Check if the host is already in the fully restored state
    let state_restored = running_now == journal.restore_plan.restore_running
        && current_observation.start_mode == journal.restore_plan.restore_start_mode
        && admission_absent == journal.restore_plan.admission_selector_absent;

    if state_restored {
        journal.record_stage(ScmTransactionStage::Completed, current_time_unix_ms);
        return ScmTransactionReceipt {
            service_name: journal.service_name.clone(),
            stage: ScmTransactionStage::Completed,
            api_accepted: true,
            achieved: achieved_from_observation(&current_observation, false),
            reason: None,
            restore: Some(journal.restore_plan.clone()),
            restore_pass: true,
        };
    }

    // Otherwise, attempt recovery by executing the bound restore plan
    journal.record_stage(ScmTransactionStage::Restoring, current_time_unix_ms);
    let receipt = executor.restore(&journal.restore_plan, admission);
    journal.record_stage(receipt.stage, current_time_unix_ms);
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticAdmission {
        absent: bool,
        fail: Option<ScmApiError>,
    }

    impl AdmissionSelectorObserver for StaticAdmission {
        fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError> {
            if let Some(error) = self.fail.take() {
                return Err(error);
            }
            Ok(self.absent)
        }
    }

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

    fn open_synthetic_gate() -> LiveScmExecutionGate {
        LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
            "fixture.scm.a",
            "fixture-image",
            true,
            true,
            true,
        ))
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
    fn unissued_trusted_verifier_never_opens_live_gate() {
        let request = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            owner_material: b"not-a-real-token".to_vec(),
        };
        assert!(matches!(
            LiveScmExecutionGate::from_trusted_owner_authorization(
                &UnissuedOwnerAuthorizationVerifier,
                &request
            ),
            Err(ScmApiError::AuthorizationNotIssued)
        ));
    }

    #[test]
    fn synthetic_grant_is_marked_test_only() {
        let gate = open_synthetic_gate();
        assert!(gate.is_synthetic_test_only());
        assert!(!gate.is_trusted_live());
    }

    #[test]
    fn synthetic_grant_cannot_mutate_gated_windows_backend() {
        // Forgeable synthetic grants open mock/synthetic gates only. The gated
        // Windows adapter remains mutation-inert regardless of grant kind.
        let gate = open_synthetic_gate();
        assert!(gate.is_synthetic_test_only());
        let backend = GatedWindowsScmBackend;
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

    #[test]
    fn closed_gate_refuses_every_mutation_intent_and_reports_closed() {
        let gate = LiveScmExecutionGate::CLOSED;
        assert!(!gate.allows_mutation());
        assert!(!gate.is_synthetic_test_only());
        assert!(!gate.is_trusted_live());
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", true, false, false));
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, true, false));
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, false, true));
    }

    #[test]
    fn granular_gate_permissions_and_identity_match_strictly_enforced() {
        let grant = SyntheticScmTestGrant::new(
            "fixture.scm.a",
            "fixture-image",
            true,  // allow_start
            false, // allow_stop
            false, // allow_change_config
        );
        let gate = LiveScmExecutionGate::from_synthetic_test_grant(&grant).unwrap();
        assert!(gate.allows_mutation());
        assert!(gate.authorizes("fixture.scm.a", "fixture-image", true, false, false));
        // Disallowed stop
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, true, false));
        // Disallowed config
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, false, true));
        // Service mismatch
        assert!(!gate.authorizes("other.service", "fixture-image", true, false, false));
        // Image token mismatch
        assert!(!gate.authorizes("fixture.scm.a", "other-image", true, false, false));
    }

    #[test]
    fn unissued_verifier_rejects_diverse_request_payloads() {
        let verifier = UnissuedOwnerAuthorizationVerifier;
        let empty_req = TrustedOwnerAuthorizationRequest {
            service_name: String::new(),
            image_token: String::new(),
            allow_start: false,
            allow_stop: false,
            allow_change_config: false,
            owner_material: Vec::new(),
        };
        assert!(matches!(
            verifier.verify(&empty_req),
            Err(ScmApiError::AuthorizationNotIssued)
        ));

        let full_req = TrustedOwnerAuthorizationRequest {
            service_name: "legitimate.service".to_owned(),
            image_token: "legitimate-token".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            owner_material: vec![1, 2, 3, 4, 5],
        };
        assert!(matches!(
            verifier.verify(&full_req),
            Err(ScmApiError::AuthorizationNotIssued)
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
        let exec = executor(mock.clone(), open_synthetic_gate());
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
    fn stop_accepted_but_readback_failure_is_interrupted_not_restore_pass() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        // capture_prestate already queried once. Allow one more successful query
        // (advance planning observe), then fail the post-stop readback.
        mock.set_fail_after_queries(1, ScmApiError::Timeout);
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::Interrupted);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.achieved, AchievedServiceControl::Unknown);
        assert_eq!(receipt.reason.as_deref(), Some("scm.timeout"));
        assert_eq!(mock.stop_calls(), 1);
    }

    #[test]
    fn start_accepted_but_readback_failure_is_interrupted_not_restore_pass() {
        let mock = MockScmBackend::new();
        let mut stopped = fixture_listening();
        stopped.observation.state = DiscoveredLifecycle::Stopped;
        mock.insert(stopped);
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        mock.set_fail_after_queries(1, ScmApiError::Unavailable);
        let receipt = exec.advance(
            DesiredServiceLifecycle::RemainRunning,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::Interrupted);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.achieved, AchievedServiceControl::Unknown);
        assert_eq!(receipt.reason.as_deref(), Some("scm.unavailable"));
        assert_eq!(mock.start_calls(), 1);
    }

    #[test]
    fn restore_requires_independent_admission_observation() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert!(receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::Completed);
    }

    #[test]
    fn restore_fails_when_admission_observer_disagrees() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        // Plan expects absent=true; observer still sees present.
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        let receipt = exec.restore(&plan, &mut admission);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
    }

    #[test]
    fn restore_fails_when_admission_observe_errors() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let mut admission = StaticAdmission {
            absent: true,
            fail: Some(ScmApiError::Unavailable),
        };
        let receipt = exec.restore(&plan, &mut admission);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.reason.as_deref(), Some("scm.unavailable"));
    }

    #[test]
    fn restore_fails_when_change_config_errors() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        mock.set_fail_change(ScmApiError::AccessDenied);
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.reason.as_deref(), Some("scm.access-denied"));
    }

    #[test]
    fn restore_start_mode_accepted_but_readback_fails_is_restore_failed_not_restore_pass() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        // capture_prestate already queried once. Fail on the post-change_start_mode query.
        mock.set_fail_after_queries(1, ScmApiError::Timeout);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
        assert!(receipt.api_accepted);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.achieved, AchievedServiceControl::Unknown);
        assert_eq!(receipt.reason.as_deref(), Some("scm.timeout"));
        assert_eq!(mock.change_calls(), 1);
    }

    #[test]
    fn restore_rejects_plan_for_different_service_binding() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let mut prestate = exec.capture_prestate(false, true).unwrap();
        prestate.service_name = "fixture.scm.other".to_owned();
        let plan = capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
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
        let exec = executor(mock.clone(), open_synthetic_gate());
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
    fn windows_backend_mutations_are_inert_without_arm() {
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

    #[cfg(windows)]
    #[test]
    fn windows_readonly_query_eventlog_returns_identity() {
        let backend = WindowsScmBackend::mutation_inert();
        let snap = backend
            .query_exact("EventLog")
            .expect("EventLog should be queryable read-only");
        assert!(snap
            .observation
            .service_name
            .eq_ignore_ascii_case("EventLog"));
        assert!(snap.observation.image_path_matches);
        assert!(snap.observation.ownership_verified);
        assert!(!snap.image_token.is_empty());
        assert!(!snap.execution_identity_token.is_empty());
        assert!(!matches!(
            snap.observation.state,
            DiscoveredLifecycle::Unknown
        ));
        assert!(!matches!(
            snap.observation.start_mode,
            ServiceStartMode::Unknown
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_readonly_query_missing_service_is_not_found() {
        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.query_exact("runnermesh.fixture.missing.service.zz"),
            Err(ScmApiError::NotFound)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_readonly_query_rejects_wildcard_name() {
        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.query_exact("Event*"),
            Err(ScmApiError::NameDrift)
        ));
    }

    #[test]
    fn fake_github_permission_denial_and_stale_token_fail_closed() {
        let admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_fail_next_withdraw(ScmApiError::AccessDenied);
        assert!(matches!(
            admission.withdraw_capacity(),
            Err(ScmApiError::AccessDenied)
        ));
        assert_eq!(admission.withdraw_calls(), 0);
        // Second attempt succeeds because fail-next was consumed.
        admission.withdraw_capacity().expect("withdraw");
        assert_eq!(admission.withdraw_calls(), 1);

        let stale = FakeGithubAdmissionBackend::new_with_selector_present(true);
        stale.set_stale_token(true);
        assert!(matches!(
            stale.withdraw_capacity(),
            Err(ScmApiError::AccessDenied)
        ));
        let mut observer = FakeGithubAdmissionBackend::new_with_selector_present(false);
        observer.set_stale_token(true);
        assert!(matches!(
            observer.observe_selector_absent(),
            Err(ScmApiError::AccessDenied)
        ));
    }

    #[test]
    fn fake_github_eventual_visibility_lags_before_readback() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_eventual_visibility_lag(1);
        admission.withdraw_capacity().expect("withdraw");
        // First observe still sees selector present (lag).
        assert!(!admission.observe_selector_absent().expect("obs1"));
        // Second observe sees absence after lag consumed.
        assert!(admission.observe_selector_absent().expect("obs2"));
        assert_eq!(admission.observe_calls(), 2);
    }

    #[test]
    fn fake_github_partial_restore_failure_preserves_withdrawn_intent() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.withdraw_capacity().expect("withdraw");
        admission.set_fail_next_restore(ScmApiError::Unavailable);
        assert!(matches!(
            admission.restore_capacity(),
            Err(ScmApiError::Unavailable)
        ));
        assert_eq!(admission.restore_calls(), 0);
        // Withdrawal remains observed; restore did not partially flip presence.
        assert!(admission.observe_selector_absent().expect("absent"));
    }

    #[test]
    fn fake_github_link_loss_during_readback_is_unavailable_not_false_absent() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.withdraw_capacity().expect("withdraw");
        admission.set_disconnected(true);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::Unavailable)
        ));
        // Reconnect must not invent absence without a fresh successful observe.
        admission.set_disconnected(false);
        assert!(admission
            .observe_selector_absent()
            .expect("absent after reconnect"));
    }

    #[test]
    fn fake_github_identity_or_name_drift_observe_fails_closed() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        admission.set_fail_next_observe(ScmApiError::IdentityMismatch);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::IdentityMismatch)
        ));
        admission.set_fail_next_observe(ScmApiError::NameDrift);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::NameDrift)
        ));
        // After drift errors clear, honest absence remains observable.
        assert!(admission.observe_selector_absent().expect("absent"));
    }

    #[test]
    fn fake_github_racing_observe_error_during_lag_does_not_claim_absence() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_eventual_visibility_lag(1);
        admission.withdraw_capacity().expect("withdraw");
        admission.set_fail_next_observe(ScmApiError::AccessDenied);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::AccessDenied)
        ));
        // Lag still applies: next successful observe remains non-absent.
        assert!(!admission.observe_selector_absent().expect("still present"));
        // Only after remaining lag is consumed does absence become visible.
        assert!(admission.observe_selector_absent().expect("absent"));
    }

    #[test]
    fn fake_github_scope_drift_refuses_mutation_and_observe_without_rewriting_presence() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_scope_tokens("scope-a", "scope-b");
        assert!(matches!(
            admission.withdraw_capacity(),
            Err(ScmApiError::IdentityMismatch)
        ));
        assert_eq!(admission.withdraw_calls(), 0);
        assert!(matches!(
            admission.restore_capacity(),
            Err(ScmApiError::IdentityMismatch)
        ));
        assert_eq!(admission.restore_calls(), 0);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::IdentityMismatch)
        ));
        // Presence was never rewritten by the refused mutations.
        admission.set_scope_tokens("scope-a", "scope-a");
        assert!(!admission.observe_selector_absent().expect("still present"));
    }

    #[test]
    fn fake_github_racing_restore_during_withdraw_lag_keeps_latest_pending() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_eventual_visibility_lag(2);
        admission.withdraw_capacity().expect("withdraw");
        // First observe consumes one lag slot; selector still appears present.
        assert!(!admission.observe_selector_absent().expect("lag1"));
        // Racing restore overwrites the pending assignment to "present".
        admission.restore_capacity().expect("restore during lag");
        assert_eq!(admission.restore_calls(), 1);
        // Remaining lag still hides the pending flip.
        assert!(!admission.observe_selector_absent().expect("lag2"));
        // After lag settles, the latest assignment (restore/present) wins —
        // never a false absence claim from the superseded withdraw.
        assert!(!admission
            .observe_selector_absent()
            .expect("restored present"));
        assert_eq!(admission.observe_calls(), 3);
    }

    fn test_fixture_record(
        name: &str,
        image_token: &str,
        identity_token: &str,
        state: DiscoveredLifecycle,
        start_mode: ServiceStartMode,
        busy: bool,
    ) -> MockServiceRecord {
        MockServiceRecord {
            observation: ServiceObservation {
                service_name: name.to_owned(),
                state,
                start_mode,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: busy,
            image_token: image_token.to_owned(),
            execution_identity_token: identity_token.to_owned(),
        }
    }

    fn test_synthetic_gate(
        service_name: &str,
        image_token: &str,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> LiveScmExecutionGate {
        LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
            service_name,
            image_token,
            allow_start,
            allow_stop,
            allow_change_config,
        ))
        .unwrap()
    }

    #[test]
    fn privileged_scm_helper_enforces_narrow_actions_and_refuses_wildcards_and_foreign_services() {
        let backend = MockScmBackend::new();
        backend.insert(test_fixture_record(
            "fixture.runner.svc",
            "hux2-image",
            "hux2-identity",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));

        let full_gate = test_synthetic_gate("fixture.runner.svc", "hux2-image", true, true, true);
        let helper = PrivilegedScmHelper::new(backend.clone(), full_gate);

        // 1. Refuse wildcard service name
        let report_wildcard = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.*".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-1".to_owned(),
        });
        assert!(!report_wildcard.applied);
        assert_eq!(
            report_wildcard.refusal_reason.as_deref(),
            Some("scm.name-drift")
        );

        // 2. Refuse empty transaction ID
        let report_empty_tx = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "".to_owned(),
        });
        assert!(!report_empty_tx.applied);
        assert_eq!(
            report_empty_tx.refusal_reason.as_deref(),
            Some("scm.restore-incomplete")
        );

        // 3. Refuse foreign service (image token mismatch)
        let report_foreign = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "wrong-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-2".to_owned(),
        });
        assert!(!report_foreign.applied);
        assert_eq!(
            report_foreign.refusal_reason.as_deref(),
            Some("scm.live-gate-closed") // Gate mismatch because expected_image_token diverged
        );

        // 4. Refuse when gate lacks permission (Start-only gate refuses StopService)
        let start_only_gate =
            test_synthetic_gate("fixture.runner.svc", "hux2-image", true, false, false);
        let helper_start_only = PrivilegedScmHelper::new(backend.clone(), start_only_gate);
        let report_denied_stop = helper_start_only.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-3".to_owned(),
        });
        assert!(!report_denied_stop.applied);
        assert_eq!(
            report_denied_stop.refusal_reason.as_deref(),
            Some("scm.live-gate-closed")
        );

        // 5. Refuse StopService when worker is busy
        backend.set_worker_busy("fixture.runner.svc", true);
        let report_busy = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-4".to_owned(),
        });
        assert!(!report_busy.applied);
        assert_eq!(
            report_busy.refusal_reason.as_deref(),
            Some("scm.busy-worker")
        );
        backend.set_worker_busy("fixture.runner.svc", false);

        // 6. Valid StartService succeeds and verifies post-mutation readback
        let report_start = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-5".to_owned(),
        });
        assert!(report_start.applied);
        assert_eq!(report_start.achieved_state, DiscoveredLifecycle::Listening);
        assert_eq!(report_start.refusal_reason, None);

        // 7. Valid ChangeStartMode succeeds
        let report_mode = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::ChangeStartMode {
                mode: ServiceStartMode::Automatic,
            },
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-6".to_owned(),
        });
        assert!(report_mode.applied);
        assert_eq!(report_mode.achieved_start_mode, ServiceStartMode::Automatic);
        assert_eq!(report_mode.refusal_reason, None);
    }

    #[test]
    fn privileged_scm_helper_requires_immediate_readback_confirmation() {
        let backend = MockScmBackend::new();
        backend.insert(test_fixture_record(
            "fixture.runner.svc",
            "hux2-image",
            "hux2-identity",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));

        let full_gate = test_synthetic_gate("fixture.runner.svc", "hux2-image", true, true, true);
        let helper = PrivilegedScmHelper::new(backend.clone(), full_gate);

        // Allow pre-mutation query to succeed, then fail immediate readback after start
        backend.set_fail_after_queries(1, ScmApiError::Timeout);
        let report = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-timeout".to_owned(),
        });
        assert!(report.applied);
        assert_eq!(report.refusal_reason.as_deref(), Some("scm.timeout"));
    }

    #[test]
    fn scm_transaction_journal_atomic_roundtrip_and_terminal_detection() {
        let root =
            std::env::temp_dir().join(format!("runnermesh-test-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test-scm-journal.json");

        let prestate = ServicePrestateCapture {
            service_name: "fixture.runner.svc".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: false,
            image_token: "hux2-image".to_owned(),
            execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-journal-1".to_owned(),
        };

        let mut journal = ScmTransactionJournal::new(prestate, "hux2-image", "hux2-identity", 1000);
        assert!(!journal.is_terminal());
        assert_eq!(journal.stage, ScmTransactionStage::CapturedPrestate);

        journal.save_to_path(&path).unwrap();

        let loaded = ScmTransactionJournal::load_from_path(&path)
            .unwrap()
            .expect("loaded");
        assert_eq!(loaded, journal);

        journal.record_stage(ScmTransactionStage::Completed, 2000);
        assert!(journal.is_terminal());

        let cleared = ScmTransactionJournal::clear(&path).unwrap();
        assert!(cleared);
        assert!(ScmTransactionJournal::load_from_path(&path)
            .unwrap()
            .is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn interrupted_scm_journal_requires_independent_readback_and_never_assumes_restore_pass() {
        let mock = MockScmBackend::new();
        mock.insert(test_fixture_record(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            DiscoveredLifecycle::Stopped, // Diverged: was Listening in prestate
            ServiceStartMode::Automatic,
            false,
        ));

        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: false,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            transaction_id: "tx-interrupted-1".to_owned(),
        };

        let mut journal =
            ScmTransactionJournal::new(prestate, "fixture-image", "fixture-identity", 1000);
        journal.record_stage(ScmTransactionStage::RequestedStop, 1500);

        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        let gate = LiveScmExecutionGate::CLOSED;
        let exec = executor(mock.clone(), gate);

        // Scenario A: Gate CLOSED and host diverged -> restore fails closed, restore_pass=false
        let receipt_refused =
            reconcile_interrupted_scm_journal(&mut journal, &exec, &mut admission, 2000);
        assert_eq!(receipt_refused.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt_refused.restore_pass);
        assert_eq!(
            receipt_refused.reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(journal.stage, ScmTransactionStage::RestoreFailed);

        // Scenario B: Query readback fails -> Interrupted, restore_pass=false
        mock.set_fail_next(ScmApiError::Unavailable);
        let receipt_unavailable =
            reconcile_interrupted_scm_journal(&mut journal, &exec, &mut admission, 2500);
        assert_eq!(receipt_unavailable.stage, ScmTransactionStage::Interrupted);
        assert!(!receipt_unavailable.restore_pass);
        assert_eq!(journal.stage, ScmTransactionStage::Interrupted);

        // Scenario C: Gate opens, restore succeeds and fresh readbacks confirm restored state
        let open_gate = open_synthetic_gate();
        let exec_open = executor(mock.clone(), open_gate);
        let receipt_success =
            reconcile_interrupted_scm_journal(&mut journal, &exec_open, &mut admission, 3000);
        assert_eq!(receipt_success.stage, ScmTransactionStage::Completed);
        assert!(receipt_success.restore_pass);
        assert_eq!(journal.stage, ScmTransactionStage::Completed);
    }
}
