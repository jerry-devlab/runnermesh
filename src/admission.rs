use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    AdmissionDecision, AgentObservation, AgentReconciler, ExecutionIdentityEvidence, NodeState,
    OwnershipEvidence, ReasonCode, RunnerPhase,
};

pub const RESERVED_ADMISSION_LABEL: &str = "runnermesh-admit";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesiredAdmissionState {
    Full,
    Drained,
}

impl DesiredAdmissionState {
    pub fn from_decision(decision: &AdmissionDecision) -> Self {
        if decision.allow_new_work && !decision.drain_requested {
            Self::Full
        } else {
            Self::Drained
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdmissionLifecycleState {
    Full,
    Advertising,
    ReAdvertising,
    Listening,
    Busy,
    WithdrawRequested,
    Withdrawing,
    WithdrawalBlocked,
    DrainPending,
    Drained,
    Unknown,
    Refused,
    NotConfigured,
}

impl fmt::Display for AdmissionLifecycleState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Full => "FULL",
            Self::Advertising => "ADVERTISING",
            Self::ReAdvertising => "RE_ADVERTISING",
            Self::Listening => "LISTENING",
            Self::Busy => "BUSY",
            Self::WithdrawRequested => "WITHDRAW_REQUESTED",
            Self::Withdrawing => "WITHDRAWING",
            Self::WithdrawalBlocked => "WITHDRAWAL_BLOCKED",
            Self::DrainPending => "DRAIN_PENDING",
            Self::Drained => "DRAINED",
            Self::Unknown => "UNKNOWN",
            Self::Refused => "REFUSED",
            Self::NotConfigured => "NOT_CONFIGURED",
        };
        formatter.write_str(value)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdmissionSelectorState {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExactRunnerIdentityState {
    Verified,
    Unknown,
    Drift,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReservedLabelOwnershipState {
    Verified,
    Unknown,
    Drift,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetryDirective {
    pub attempt: u8,
    pub after_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdmissionControlSnapshot {
    pub desired: DesiredAdmissionState,
    pub lifecycle: AdmissionLifecycleState,
    pub selector: AdmissionSelectorState,
    pub exact_runner_identity: ExactRunnerIdentityState,
    pub reserved_label_ownership: ReservedLabelOwnershipState,
    pub active_bound_worker: bool,
    pub reason_code: Option<ReasonCode>,
    pub retry: Option<RetryDirective>,
}

impl AdmissionControlSnapshot {
    pub fn not_configured(desired: DesiredAdmissionState) -> Self {
        Self {
            desired,
            lifecycle: AdmissionLifecycleState::NotConfigured,
            selector: AdmissionSelectorState::Unknown,
            exact_runner_identity: ExactRunnerIdentityState::Unknown,
            reserved_label_ownership: ReservedLabelOwnershipState::Unknown,
            active_bound_worker: false,
            reason_code: Some(reason("admission-not-configured")),
            retry: None,
        }
    }

    pub fn with_desired(mut self, desired: DesiredAdmissionState) -> Self {
        self.desired = desired;
        if self.lifecycle == AdmissionLifecycleState::NotConfigured {
            self.reason_code = Some(reason("admission-not-configured"));
        }
        self
    }

    pub fn achieved_full(&self) -> bool {
        self.desired == DesiredAdmissionState::Full
            && self.selector == AdmissionSelectorState::Present
            && self.exact_runner_identity == ExactRunnerIdentityState::Verified
            && self.reserved_label_ownership == ReservedLabelOwnershipState::Verified
            && matches!(
                self.lifecycle,
                AdmissionLifecycleState::Listening | AdmissionLifecycleState::Busy
            )
    }

    pub fn achieved_drained(&self) -> bool {
        self.desired == DesiredAdmissionState::Drained
            && self.lifecycle == AdmissionLifecycleState::Drained
            && self.selector == AdmissionSelectorState::Absent
            && !self.active_bound_worker
            && self.exact_runner_identity == ExactRunnerIdentityState::Verified
            && self.reserved_label_ownership == ReservedLabelOwnershipState::Verified
    }

    pub fn achieved_node_state(
        &self,
        desired_node_state: NodeState,
        runner_phase: RunnerPhase,
    ) -> Option<NodeState> {
        if self.achieved_full() {
            Some(NodeState::Full)
        } else if self.achieved_drained() {
            if desired_node_state == NodeState::Offline && runner_phase == RunnerPhase::Stopped {
                Some(NodeState::Offline)
            } else {
                Some(NodeState::Drained)
            }
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalAdmissionEvidence {
    pub active_bound_worker: bool,
    pub consistent: bool,
}

impl LocalAdmissionEvidence {
    pub fn from_runner_phase(phase: RunnerPhase, exact_binding_consistent: bool) -> Self {
        Self {
            active_bound_worker: phase == RunnerPhase::Busy,
            consistent: exact_binding_consistent && phase != RunnerPhase::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteAdmissionObservation {
    pub selector: AdmissionSelectorState,
    pub runner_busy: bool,
    pub runner_online: bool,
}

pub trait AdmissionControlBackend {
    fn observe_admission_selector(
        &mut self,
    ) -> Result<RemoteAdmissionObservation, AdmissionBackendError>;

    fn advertise_capacity(&mut self) -> Result<RemoteAdmissionObservation, AdmissionBackendError>;

    fn withdraw_capacity(&mut self) -> Result<RemoteAdmissionObservation, AdmissionBackendError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_delay_seconds: u64,
    pub maximum_delay_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_seconds: 2,
            maximum_delay_seconds: 30,
        }
    }
}

pub struct AdmissionController<B> {
    backend: B,
    retry_policy: RetryPolicy,
    consecutive_transient_failures: u8,
}

impl<B> AdmissionController<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            retry_policy: RetryPolicy::default(),
            consecutive_transient_failures: 0,
        }
    }

    pub fn with_retry_policy(backend: B, retry_policy: RetryPolicy) -> Self {
        Self {
            backend,
            retry_policy,
            consecutive_transient_failures: 0,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }
}

impl<B: AdmissionControlBackend> AdmissionController<B> {
    pub fn reconcile(
        &mut self,
        desired: DesiredAdmissionState,
        local: LocalAdmissionEvidence,
    ) -> AdmissionControlSnapshot {
        let observed = match self.backend.observe_admission_selector() {
            Ok(observed) => observed,
            Err(error) => return self.failed_snapshot(desired, local, error),
        };

        if !local.consistent {
            self.consecutive_transient_failures = 0;
            return AdmissionControlSnapshot {
                desired,
                lifecycle: AdmissionLifecycleState::Unknown,
                selector: observed.selector,
                exact_runner_identity: ExactRunnerIdentityState::Unknown,
                reserved_label_ownership: ReservedLabelOwnershipState::Verified,
                active_bound_worker: local.active_bound_worker || observed.runner_busy,
                reason_code: Some(reason("admission-local-evidence-inconsistent")),
                retry: None,
            };
        }

        let settled = match (desired, observed.selector) {
            (DesiredAdmissionState::Full, AdmissionSelectorState::Present)
            | (DesiredAdmissionState::Drained, AdmissionSelectorState::Absent) => observed,
            (DesiredAdmissionState::Full, AdmissionSelectorState::Absent) => {
                match self.backend.advertise_capacity() {
                    Ok(readback) => readback,
                    Err(error) => {
                        return self.failed_after_observation(desired, local, observed, error)
                    }
                }
            }
            (DesiredAdmissionState::Drained, AdmissionSelectorState::Present) => {
                match self.backend.withdraw_capacity() {
                    Ok(readback) => readback,
                    Err(error) => {
                        return self.failed_after_observation(desired, local, observed, error)
                    }
                }
            }
            (_, AdmissionSelectorState::Unknown) => {
                return self.failed_snapshot(
                    desired,
                    local,
                    AdmissionBackendError::SelectorObservationUnknown,
                )
            }
        };

        self.consecutive_transient_failures = 0;
        let active_bound_worker = local.active_bound_worker || settled.runner_busy;
        let (lifecycle, reason_code) = match desired {
            DesiredAdmissionState::Full
                if settled.selector == AdmissionSelectorState::Present
                    && !settled.runner_online =>
            {
                (
                    AdmissionLifecycleState::Unknown,
                    Some(reason("admission-runner-unavailable")),
                )
            }
            DesiredAdmissionState::Full if settled.selector == AdmissionSelectorState::Present => (
                if active_bound_worker {
                    AdmissionLifecycleState::Busy
                } else {
                    AdmissionLifecycleState::Listening
                },
                None,
            ),
            DesiredAdmissionState::Full => (
                AdmissionLifecycleState::Advertising,
                Some(reason("admission-selector-presence-unconfirmed")),
            ),
            DesiredAdmissionState::Drained
                if settled.selector == AdmissionSelectorState::Absent && active_bound_worker =>
            {
                (
                    AdmissionLifecycleState::DrainPending,
                    Some(reason("admission-in-flight-worker-completing")),
                )
            }
            DesiredAdmissionState::Drained
                if settled.selector == AdmissionSelectorState::Absent =>
            {
                (AdmissionLifecycleState::Drained, None)
            }
            DesiredAdmissionState::Drained => (
                AdmissionLifecycleState::Withdrawing,
                Some(reason("admission-selector-absence-unconfirmed")),
            ),
        };

        AdmissionControlSnapshot {
            desired,
            lifecycle,
            selector: settled.selector,
            exact_runner_identity: ExactRunnerIdentityState::Verified,
            reserved_label_ownership: ReservedLabelOwnershipState::Verified,
            active_bound_worker,
            reason_code,
            retry: None,
        }
    }

    fn failed_snapshot(
        &mut self,
        desired: DesiredAdmissionState,
        local: LocalAdmissionEvidence,
        error: AdmissionBackendError,
    ) -> AdmissionControlSnapshot {
        let refusal = error.is_refusal();
        let transient = error.is_transient();
        let retry = if transient {
            self.consecutive_transient_failures =
                self.consecutive_transient_failures.saturating_add(1);
            if self.consecutive_transient_failures <= self.retry_policy.max_attempts {
                let requested = error.retry_after_seconds().unwrap_or_else(|| {
                    self.retry_policy.base_delay_seconds.saturating_mul(
                        1_u64 << self.consecutive_transient_failures.saturating_sub(1),
                    )
                });
                Some(RetryDirective {
                    attempt: self.consecutive_transient_failures,
                    after_seconds: requested.min(self.retry_policy.maximum_delay_seconds),
                })
            } else {
                None
            }
        } else {
            self.consecutive_transient_failures = 0;
            None
        };

        AdmissionControlSnapshot {
            desired,
            lifecycle: if refusal {
                AdmissionLifecycleState::Refused
            } else if desired == DesiredAdmissionState::Drained {
                AdmissionLifecycleState::WithdrawalBlocked
            } else {
                AdmissionLifecycleState::Unknown
            },
            selector: error.last_known_selector(),
            exact_runner_identity: if matches!(
                error,
                AdmissionBackendError::RunnerIdentityDrift | AdmissionBackendError::RunnerNotFound
            ) {
                ExactRunnerIdentityState::Drift
            } else {
                ExactRunnerIdentityState::Unknown
            },
            reserved_label_ownership: if matches!(
                error,
                AdmissionBackendError::ReservedLabelOwnershipDrift
                    | AdmissionBackendError::SelectorCollision
            ) {
                ReservedLabelOwnershipState::Drift
            } else {
                ReservedLabelOwnershipState::Unknown
            },
            active_bound_worker: local.active_bound_worker,
            reason_code: Some(error.reason_code()),
            retry,
        }
    }

    fn failed_after_observation(
        &mut self,
        desired: DesiredAdmissionState,
        local: LocalAdmissionEvidence,
        observed: RemoteAdmissionObservation,
        error: AdmissionBackendError,
    ) -> AdmissionControlSnapshot {
        let refusal = error.is_refusal();
        let mut snapshot = self.failed_snapshot(desired, local, error);
        snapshot.selector = observed.selector;
        snapshot.active_bound_worker = local.active_bound_worker || observed.runner_busy;
        if !refusal {
            snapshot.exact_runner_identity = ExactRunnerIdentityState::Verified;
            snapshot.reserved_label_ownership = ReservedLabelOwnershipState::Verified;
        }
        snapshot
    }
}

/// Product integration point for the authoritative Agent
/// `Observe -> Decide -> Reconcile` loop. The observer supplies exact local
/// evidence, policy supplies desired capacity, and the admission controller
/// independently observes/reconciles the remote selector.
pub struct AdmissionAgentReconciler<B> {
    controller: AdmissionController<B>,
}

impl<B> AdmissionAgentReconciler<B> {
    pub fn new(controller: AdmissionController<B>) -> Self {
        Self { controller }
    }

    pub fn controller(&self) -> &AdmissionController<B> {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut AdmissionController<B> {
        &mut self.controller
    }
}

impl<B: AdmissionControlBackend> AgentReconciler for AdmissionAgentReconciler<B> {
    fn reconcile(
        &mut self,
        decision: &AdmissionDecision,
        observation: &AgentObservation,
    ) -> Result<AdmissionControlSnapshot, String> {
        let exact_binding_consistent = observation.execution_identity
            == ExecutionIdentityEvidence::Verified
            && observation.work_root == OwnershipEvidence::Verified;
        Ok(self.controller.reconcile(
            DesiredAdmissionState::from_decision(decision),
            LocalAdmissionEvidence::from_runner_phase(
                observation.runner_phase,
                exact_binding_consistent,
            ),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionBackendError {
    CredentialUnavailable,
    CredentialResolutionFailed,
    CredentialMalformed,
    AuthenticationFailed,
    RateLimited { retry_after_seconds: u64 },
    ApiUnavailable,
    Timeout,
    MalformedResponse,
    SelectorObservationUnknown,
    RunnerNotFound,
    RunnerIdentityDrift,
    ReservedLabelOwnershipDrift,
    SelectorCollision,
    MutationReadbackUnconfirmed { selector: AdmissionSelectorState },
    BindingInvalid,
}

impl AdmissionBackendError {
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::ApiUnavailable | Self::Timeout
        )
    }

    fn is_refusal(&self) -> bool {
        matches!(
            self,
            Self::RunnerNotFound
                | Self::RunnerIdentityDrift
                | Self::ReservedLabelOwnershipDrift
                | Self::SelectorCollision
                | Self::BindingInvalid
        )
    }

    fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_seconds,
            } => Some(*retry_after_seconds),
            _ => None,
        }
    }

    fn last_known_selector(&self) -> AdmissionSelectorState {
        match self {
            Self::MutationReadbackUnconfirmed { selector } => *selector,
            _ => AdmissionSelectorState::Unknown,
        }
    }

    fn reason_code(&self) -> ReasonCode {
        reason(match self {
            Self::CredentialUnavailable => "admission-credential-unavailable",
            Self::CredentialResolutionFailed => "admission-credential-resolution-failed",
            Self::CredentialMalformed => "admission-credential-malformed",
            Self::AuthenticationFailed => "admission-authentication-failed",
            Self::RateLimited { .. } => "admission-rate-limited",
            Self::ApiUnavailable => "admission-api-unavailable",
            Self::Timeout => "admission-api-timeout",
            Self::MalformedResponse => "admission-response-malformed",
            Self::SelectorObservationUnknown => "admission-selector-observation-unknown",
            Self::RunnerNotFound => "admission-runner-unavailable",
            Self::RunnerIdentityDrift => "admission-runner-identity-drift",
            Self::ReservedLabelOwnershipDrift => "admission-reserved-label-ownership-drift",
            Self::SelectorCollision => "admission-selector-collision",
            Self::MutationReadbackUnconfirmed { .. } => "admission-mutation-readback-unconfirmed",
            Self::BindingInvalid => "admission-binding-invalid",
        })
    }
}

impl fmt::Display for AdmissionBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason_code().as_str())
    }
}

impl std::error::Error for AdmissionBackendError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RegistrationScope {
    Organization { organization: String },
    Repository { owner: String, repository: String },
}

impl RegistrationScope {
    fn validate(&self) -> bool {
        match self {
            Self::Organization { organization } => valid_path_component(organization),
            Self::Repository { owner, repository } => {
                valid_path_component(owner) && valid_path_component(repository)
            }
        }
    }

    fn runners_path(&self) -> String {
        match self {
            Self::Organization { organization } => {
                format!("/orgs/{organization}/actions/runners")
            }
            Self::Repository { owner, repository } => {
                format!("/repos/{owner}/{repository}/actions/runners")
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialReference {
    pub provider: String,
    pub key: String,
}

impl CredentialReference {
    pub fn new(provider: impl Into<String>, key: impl Into<String>) -> Result<Self, &'static str> {
        let reference = Self {
            provider: provider.into(),
            key: key.into(),
        };
        if valid_reference_component(&reference.provider)
            && valid_reference_component(&reference.key)
        {
            Ok(reference)
        } else {
            Err("credential reference components must be non-secret printable tokens")
        }
    }
}

pub struct CredentialLease {
    secret: Vec<u8>,
}

impl CredentialLease {
    pub fn from_secret(secret: impl AsRef<[u8]>) -> Result<Self, &'static str> {
        Self::from_owned_secret(secret.as_ref().to_vec())
    }

    pub(crate) fn from_owned_secret(mut secret: Vec<u8>) -> Result<Self, &'static str> {
        if secret.is_empty()
            || secret.len() > 4096
            || secret.iter().any(|byte| !byte.is_ascii_graphic())
        {
            secret.fill(0);
            return Err("credential material must be bounded printable ASCII without whitespace");
        }
        Ok(Self { secret })
    }

    pub fn expose_for_transport(&self) -> &[u8] {
        &self.secret
    }
}

impl fmt::Debug for CredentialLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialLease([REDACTED])")
    }
}

impl Drop for CredentialLease {
    fn drop(&mut self) {
        self.secret.fill(0);
    }
}

pub trait CredentialProvider {
    fn resolve(
        &mut self,
        reference: &CredentialReference,
    ) -> Result<CredentialLease, AdmissionBackendError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReservedLabelOwnership {
    pub scope: RegistrationScope,
    pub runner_id: u64,
    pub label: String,
}

impl ReservedLabelOwnership {
    pub fn for_runner(scope: RegistrationScope, runner_id: u64) -> Self {
        Self {
            scope,
            runner_id,
            label: RESERVED_ADMISSION_LABEL.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionBinding {
    pub scope: RegistrationScope,
    pub runner_id: u64,
    pub runner_name: String,
    pub reserved_label: String,
    pub credential_ref: CredentialReference,
    pub ownership: Option<ReservedLabelOwnership>,
}

impl AdmissionBinding {
    pub fn new(
        scope: RegistrationScope,
        runner_id: u64,
        runner_name: impl Into<String>,
        credential_ref: CredentialReference,
        ownership: Option<ReservedLabelOwnership>,
    ) -> Result<Self, AdmissionBackendError> {
        let binding = Self {
            scope,
            runner_id,
            runner_name: runner_name.into(),
            reserved_label: RESERVED_ADMISSION_LABEL.to_owned(),
            credential_ref,
            ownership,
        };
        if binding.is_valid() {
            Ok(binding)
        } else {
            Err(AdmissionBackendError::BindingInvalid)
        }
    }

    pub fn is_valid(&self) -> bool {
        self.scope.validate()
            && self.runner_id != 0
            && !self.runner_name.trim().is_empty()
            && !self.runner_name.contains(['\r', '\n'])
            && self
                .reserved_label
                .eq_ignore_ascii_case(RESERVED_ADMISSION_LABEL)
    }

    pub fn has_valid_ownership(&self) -> bool {
        self.ownership.as_ref().is_some_and(|ownership| {
            ownership.scope == self.scope
                && ownership.runner_id == self.runner_id
                && ownership
                    .label
                    .eq_ignore_ascii_case(RESERVED_ADMISSION_LABEL)
        })
    }

    fn runner_path(&self) -> String {
        format!("{}/{}", self.scope.runners_path(), self.runner_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubHttpRequest {
    pub method: HttpMethod,
    pub path: String,
    pub body: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after_seconds: Option<u64>,
    pub has_next_page: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GithubTransportError {
    Unavailable,
    Timeout,
    InvalidResponse,
}

pub trait GithubHttpTransport {
    fn send(
        &mut self,
        request: &GithubHttpRequest,
        credential: &CredentialLease,
    ) -> Result<GithubHttpResponse, GithubTransportError>;
}

pub struct GithubRestAdmissionBackend<T, C> {
    binding: AdmissionBinding,
    transport: T,
    credentials: C,
}

impl<T, C> GithubRestAdmissionBackend<T, C> {
    pub fn new(
        binding: AdmissionBinding,
        transport: T,
        credentials: C,
    ) -> Result<Self, AdmissionBackendError> {
        if !binding.is_valid() {
            return Err(AdmissionBackendError::BindingInvalid);
        }
        Ok(Self {
            binding,
            transport,
            credentials,
        })
    }

    pub fn binding(&self) -> &AdmissionBinding {
        &self.binding
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

impl<T: GithubHttpTransport, C: CredentialProvider> GithubRestAdmissionBackend<T, C> {
    fn observe_exact_runner(
        &mut self,
    ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
        let mut page = 1_u16;
        let mut runners = Vec::new();
        let mut expected_total = None;
        loop {
            if page > 100 {
                return Err(AdmissionBackendError::SelectorObservationUnknown);
            }
            let response = self.execute(GithubHttpRequest {
                method: HttpMethod::Get,
                path: format!(
                    "{}?per_page=100&page={page}",
                    self.binding.scope.runners_path()
                ),
                body: None,
            })?;
            let parsed: GithubRunnerList = serde_json::from_slice(&response.body)
                .map_err(|_| AdmissionBackendError::MalformedResponse)?;
            expected_total.get_or_insert(parsed.total_count);
            runners.extend(parsed.runners);
            if !response.has_next_page {
                break;
            }
            page += 1;
        }

        if expected_total.is_some_and(|total| usize::try_from(total).ok() != Some(runners.len())) {
            return Err(AdmissionBackendError::SelectorObservationUnknown);
        }

        let selector_collision = runners.iter().any(|runner| {
            runner.id != self.binding.runner_id
                && runner
                    .labels
                    .iter()
                    .any(|label| label.name.eq_ignore_ascii_case(RESERVED_ADMISSION_LABEL))
        });
        if selector_collision {
            return Err(AdmissionBackendError::SelectorCollision);
        }

        let exact = runners
            .iter()
            .filter(|runner| runner.id == self.binding.runner_id)
            .collect::<Vec<_>>();
        if exact.is_empty() {
            return Err(AdmissionBackendError::RunnerNotFound);
        }
        if exact.len() != 1 || exact[0].name != self.binding.runner_name {
            return Err(AdmissionBackendError::RunnerIdentityDrift);
        }
        let exact = exact[0];
        let matching_labels = exact
            .labels
            .iter()
            .filter(|label| label.name.eq_ignore_ascii_case(RESERVED_ADMISSION_LABEL))
            .collect::<Vec<_>>();
        if matching_labels.len() > 1 || matching_labels.iter().any(|label| label.kind != "custom") {
            return Err(AdmissionBackendError::ReservedLabelOwnershipDrift);
        }
        if !matching_labels.is_empty() && !self.binding.has_valid_ownership() {
            return Err(AdmissionBackendError::ReservedLabelOwnershipDrift);
        }

        let runner_online = match exact.status.as_str() {
            status if status.eq_ignore_ascii_case("online") => true,
            status if status.eq_ignore_ascii_case("offline") => false,
            _ => return Err(AdmissionBackendError::MalformedResponse),
        };
        Ok(RemoteAdmissionObservation {
            selector: if matching_labels.is_empty() {
                AdmissionSelectorState::Absent
            } else {
                AdmissionSelectorState::Present
            },
            runner_busy: exact.busy,
            runner_online,
        })
    }

    fn require_owned_binding(&self) -> Result<(), AdmissionBackendError> {
        if self.binding.has_valid_ownership() {
            Ok(())
        } else {
            Err(AdmissionBackendError::ReservedLabelOwnershipDrift)
        }
    }

    fn execute(
        &mut self,
        request: GithubHttpRequest,
    ) -> Result<GithubHttpResponse, AdmissionBackendError> {
        let credential = self.credentials.resolve(&self.binding.credential_ref)?;
        let response = self
            .transport
            .send(&request, &credential)
            .map_err(|error| match error {
                GithubTransportError::Unavailable => AdmissionBackendError::ApiUnavailable,
                GithubTransportError::Timeout => AdmissionBackendError::Timeout,
                GithubTransportError::InvalidResponse => AdmissionBackendError::MalformedResponse,
            })?;
        match response.status {
            200..=299 => Ok(response),
            401 | 403 if response.retry_after_seconds.is_none() => {
                Err(AdmissionBackendError::AuthenticationFailed)
            }
            403 | 429 => Err(AdmissionBackendError::RateLimited {
                retry_after_seconds: response.retry_after_seconds.unwrap_or(1),
            }),
            404 => Err(AdmissionBackendError::RunnerNotFound),
            500..=599 => Err(AdmissionBackendError::ApiUnavailable),
            _ => Err(AdmissionBackendError::MalformedResponse),
        }
    }
}

impl<T: GithubHttpTransport, C: CredentialProvider> AdmissionControlBackend
    for GithubRestAdmissionBackend<T, C>
{
    fn observe_admission_selector(
        &mut self,
    ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
        self.observe_exact_runner()
    }

    fn advertise_capacity(&mut self) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
        self.require_owned_binding()?;
        let before = self.observe_exact_runner()?;
        if before.selector == AdmissionSelectorState::Present {
            return Ok(before);
        }
        self.execute(GithubHttpRequest {
            method: HttpMethod::Post,
            path: format!("{}/labels", self.binding.runner_path()),
            body: Some(serde_json::json!({ "labels": [RESERVED_ADMISSION_LABEL] })),
        })?;
        let readback = self.observe_exact_runner()?;
        if readback.selector == AdmissionSelectorState::Present {
            Ok(readback)
        } else {
            Err(AdmissionBackendError::MutationReadbackUnconfirmed {
                selector: readback.selector,
            })
        }
    }

    fn withdraw_capacity(&mut self) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
        self.require_owned_binding()?;
        let before = self.observe_exact_runner()?;
        if before.selector == AdmissionSelectorState::Absent {
            return Ok(before);
        }
        self.execute(GithubHttpRequest {
            method: HttpMethod::Delete,
            path: format!(
                "{}/labels/{RESERVED_ADMISSION_LABEL}",
                self.binding.runner_path()
            ),
            body: None,
        })?;
        let readback = self.observe_exact_runner()?;
        if readback.selector == AdmissionSelectorState::Absent {
            Ok(readback)
        } else {
            Err(AdmissionBackendError::MutationReadbackUnconfirmed {
                selector: readback.selector,
            })
        }
    }
}

#[derive(Deserialize)]
struct GithubRunnerList {
    total_count: u64,
    runners: Vec<GithubRunner>,
}

#[derive(Deserialize)]
struct GithubRunner {
    id: u64,
    name: String,
    status: String,
    busy: bool,
    labels: Vec<GithubLabel>,
}

#[derive(Deserialize)]
struct GithubLabel {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

fn valid_path_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_reference_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static admission reason codes must be valid")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[derive(Default)]
    struct SyntheticBackend {
        results: VecDeque<Result<RemoteAdmissionObservation, AdmissionBackendError>>,
        observations: u32,
        advertisements: u32,
        withdrawals: u32,
        destructive_worker_actions: u32,
    }

    impl SyntheticBackend {
        fn with_results(
            results: impl IntoIterator<Item = Result<RemoteAdmissionObservation, AdmissionBackendError>>,
        ) -> Self {
            Self {
                results: results.into_iter().collect(),
                ..Self::default()
            }
        }

        fn next(&mut self) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            self.results
                .pop_front()
                .expect("synthetic backend must have one queued result")
        }
    }

    impl AdmissionControlBackend for SyntheticBackend {
        fn observe_admission_selector(
            &mut self,
        ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            self.observations += 1;
            self.next()
        }

        fn advertise_capacity(
            &mut self,
        ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            self.advertisements += 1;
            self.next()
        }

        fn withdraw_capacity(
            &mut self,
        ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            self.withdrawals += 1;
            self.next()
        }
    }

    fn remote(selector: AdmissionSelectorState, busy: bool) -> RemoteAdmissionObservation {
        RemoteAdmissionObservation {
            selector,
            runner_busy: busy,
            runner_online: true,
        }
    }

    fn consistent(worker: bool) -> LocalAdmissionEvidence {
        LocalAdmissionEvidence {
            active_bound_worker: worker,
            consistent: true,
        }
    }

    #[test]
    fn two_phase_withdrawal_never_kills_and_waits_for_worker_and_absence() {
        let backend = SyntheticBackend::with_results([
            Ok(remote(AdmissionSelectorState::Present, true)),
            Ok(remote(AdmissionSelectorState::Absent, true)),
            Ok(remote(AdmissionSelectorState::Absent, false)),
        ]);
        let mut controller = AdmissionController::new(backend);

        let pending = controller.reconcile(DesiredAdmissionState::Drained, consistent(true));
        assert_eq!(pending.lifecycle, AdmissionLifecycleState::DrainPending);
        assert!(!pending.achieved_drained());
        assert_eq!(controller.backend().withdrawals, 1);
        assert_eq!(controller.backend().destructive_worker_actions, 0);

        let drained = controller.reconcile(DesiredAdmissionState::Drained, consistent(false));
        assert!(drained.achieved_drained());
        assert_eq!(controller.backend().destructive_worker_actions, 0);
    }

    #[test]
    fn withdrawal_failure_is_visible_and_bounded_without_local_fallback() {
        for error in [
            AdmissionBackendError::CredentialUnavailable,
            AdmissionBackendError::AuthenticationFailed,
            AdmissionBackendError::ApiUnavailable,
            AdmissionBackendError::Timeout,
            AdmissionBackendError::RateLimited {
                retry_after_seconds: 17,
            },
        ] {
            let backend = SyntheticBackend::with_results([Err(error.clone())]);
            let mut controller = AdmissionController::new(backend);
            let snapshot = controller.reconcile(DesiredAdmissionState::Drained, consistent(true));
            assert_eq!(
                snapshot.lifecycle,
                AdmissionLifecycleState::WithdrawalBlocked
            );
            assert!(!snapshot.achieved_drained());
            assert_eq!(controller.backend().destructive_worker_actions, 0);
            if error.is_transient() {
                assert!(snapshot.retry.is_some());
            } else {
                assert!(snapshot.retry.is_none());
            }
        }
    }

    #[test]
    fn known_present_selector_remains_visible_when_withdrawal_mutation_fails() {
        let backend = SyntheticBackend::with_results([
            Ok(remote(AdmissionSelectorState::Present, false)),
            Err(AdmissionBackendError::ApiUnavailable),
        ]);
        let mut controller = AdmissionController::new(backend);
        let snapshot = controller.reconcile(DesiredAdmissionState::Drained, consistent(false));

        assert_eq!(
            snapshot.lifecycle,
            AdmissionLifecycleState::WithdrawalBlocked
        );
        assert_eq!(snapshot.selector, AdmissionSelectorState::Present);
        assert_eq!(
            snapshot.exact_runner_identity,
            ExactRunnerIdentityState::Verified
        );
        assert!(!snapshot.achieved_drained());
    }

    #[test]
    fn successful_add_requires_positive_presence_readback() {
        let backend = SyntheticBackend::with_results([
            Ok(remote(AdmissionSelectorState::Absent, false)),
            Err(AdmissionBackendError::MutationReadbackUnconfirmed {
                selector: AdmissionSelectorState::Absent,
            }),
        ]);
        let mut controller = AdmissionController::new(backend);
        let snapshot = controller.reconcile(DesiredAdmissionState::Full, consistent(false));

        assert_eq!(snapshot.lifecycle, AdmissionLifecycleState::Unknown);
        assert_eq!(snapshot.selector, AdmissionSelectorState::Absent);
        assert!(!snapshot.achieved_full());
    }

    #[test]
    fn full_refuses_to_claim_capacity_when_the_exact_runner_is_offline() {
        let backend = SyntheticBackend::with_results([Ok(RemoteAdmissionObservation {
            selector: AdmissionSelectorState::Present,
            runner_busy: false,
            runner_online: false,
        })]);
        let mut controller = AdmissionController::new(backend);
        let snapshot = controller.reconcile(DesiredAdmissionState::Full, consistent(false));

        assert_eq!(snapshot.lifecycle, AdmissionLifecycleState::Unknown);
        assert_eq!(snapshot.selector, AdmissionSelectorState::Present);
        assert_eq!(
            snapshot.reason_code.as_ref().map(ReasonCode::as_str),
            Some("admission-runner-unavailable")
        );
        assert!(!snapshot.achieved_full());
    }

    #[test]
    fn restart_reconstruction_observes_remote_and_local_state_again() {
        let backend = SyntheticBackend::with_results([
            Ok(remote(AdmissionSelectorState::Absent, true)),
            Ok(remote(AdmissionSelectorState::Absent, false)),
        ]);
        let mut controller = AdmissionController::new(backend);

        let restarted_busy = controller.reconcile(DesiredAdmissionState::Drained, consistent(true));
        assert_eq!(
            restarted_busy.lifecycle,
            AdmissionLifecycleState::DrainPending
        );

        let restarted_idle =
            controller.reconcile(DesiredAdmissionState::Drained, consistent(false));
        assert!(restarted_idle.achieved_drained());
        assert_eq!(controller.backend().observations, 2);
    }

    #[test]
    fn authoritative_agent_reconciler_combines_policy_remote_and_exact_local_evidence() {
        let backend =
            SyntheticBackend::with_results([Ok(remote(AdmissionSelectorState::Absent, false))]);
        let controller = AdmissionController::new(backend);
        let mut reconciler = AdmissionAgentReconciler::new(controller);
        let decision = AdmissionDecision {
            allow_new_work: false,
            desired_node_state: NodeState::Drained,
            reason_code: reason("manual-work"),
            drain_requested: true,
        };
        let observation = AgentObservation {
            health: crate::AgentHealth::Healthy,
            health_reason_code: None,
            hard_safety: crate::HardSafetyState::Clear,
            runner_phase: RunnerPhase::Listening,
            execution_identity: ExecutionIdentityEvidence::Verified,
            work_root: OwnershipEvidence::Verified,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            links: Vec::new(),
            probes: Vec::new(),
            system_preferences: crate::SystemPreferences::default(),
        };

        let snapshot = reconciler.reconcile(&decision, &observation).unwrap();
        assert!(snapshot.achieved_drained());
        assert_eq!(snapshot.lifecycle, AdmissionLifecycleState::Drained);
    }

    #[test]
    fn authoritative_agent_reconciler_refuses_work_root_drift_before_mutation() {
        let backend =
            SyntheticBackend::with_results([Ok(remote(AdmissionSelectorState::Present, false))]);
        let controller = AdmissionController::new(backend);
        let mut reconciler = AdmissionAgentReconciler::new(controller);
        let decision = AdmissionDecision {
            allow_new_work: false,
            desired_node_state: NodeState::Drained,
            reason_code: reason("manual-work"),
            drain_requested: true,
        };
        let observation = AgentObservation {
            health: crate::AgentHealth::Healthy,
            health_reason_code: None,
            hard_safety: crate::HardSafetyState::Clear,
            runner_phase: RunnerPhase::Listening,
            execution_identity: ExecutionIdentityEvidence::Verified,
            work_root: OwnershipEvidence::NotOwned,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            links: Vec::new(),
            probes: Vec::new(),
            system_preferences: crate::SystemPreferences::default(),
        };

        let snapshot = reconciler.reconcile(&decision, &observation).unwrap();
        assert_eq!(snapshot.lifecycle, AdmissionLifecycleState::Unknown);
        assert_eq!(
            snapshot.reason_code.as_ref().map(ReasonCode::as_str),
            Some("admission-local-evidence-inconsistent")
        );
        assert_eq!(reconciler.controller().backend().observations, 1);
        assert_eq!(reconciler.controller().backend().withdrawals, 0);
    }

    #[derive(Default)]
    struct FakeCredentialProvider {
        unavailable: bool,
    }

    impl CredentialProvider for FakeCredentialProvider {
        fn resolve(
            &mut self,
            _reference: &CredentialReference,
        ) -> Result<CredentialLease, AdmissionBackendError> {
            if self.unavailable {
                Err(AdmissionBackendError::CredentialUnavailable)
            } else {
                CredentialLease::from_secret("synthetic-not-a-real-token")
                    .map_err(|_| AdmissionBackendError::CredentialUnavailable)
            }
        }
    }

    #[derive(Default)]
    struct FakeTransport {
        responses: VecDeque<Result<GithubHttpResponse, GithubTransportError>>,
        requests: Vec<GithubHttpRequest>,
        credential_was_nonempty: bool,
    }

    impl GithubHttpTransport for FakeTransport {
        fn send(
            &mut self,
            request: &GithubHttpRequest,
            credential: &CredentialLease,
        ) -> Result<GithubHttpResponse, GithubTransportError> {
            self.credential_was_nonempty = !credential.expose_for_transport().is_empty();
            self.requests.push(request.clone());
            self.responses
                .pop_front()
                .expect("fake transport must have one queued response")
        }
    }

    fn binding(owned: bool) -> AdmissionBinding {
        let scope = RegistrationScope::Organization {
            organization: "example-org".to_owned(),
        };
        AdmissionBinding::new(
            scope.clone(),
            42,
            "trusted-runner",
            CredentialReference::new("windows-credential-manager", "runnermesh-admission").unwrap(),
            owned.then(|| ReservedLabelOwnership::for_runner(scope, 42)),
        )
        .unwrap()
    }

    fn response(body: serde_json::Value) -> Result<GithubHttpResponse, GithubTransportError> {
        Ok(GithubHttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
            retry_after_seconds: None,
            has_next_page: false,
        })
    }

    fn runner(labels: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "total_count": 1,
            "runners": [{
                "id": 42,
                "name": "trusted-runner",
                "status": "online",
                "busy": false,
                "labels": labels.iter().map(|name| serde_json::json!({
                    "name": name,
                    "type": "custom"
                })).collect::<Vec<_>>()
            }]
        })
    }

    #[test]
    fn rest_backend_uses_add_one_and_remove_one_without_set_all_semantics() {
        let transport = FakeTransport {
            responses: [
                response(runner(&["unrelated-label"])),
                response(serde_json::json!({})),
                response(runner(&["unrelated-label", RESERVED_ADMISSION_LABEL])),
                response(runner(&["unrelated-label", RESERVED_ADMISSION_LABEL])),
                response(serde_json::json!({})),
                response(runner(&["unrelated-label"])),
            ]
            .into_iter()
            .collect(),
            ..FakeTransport::default()
        };
        let mut backend = GithubRestAdmissionBackend::new(
            binding(true),
            transport,
            FakeCredentialProvider::default(),
        )
        .unwrap();

        assert_eq!(
            backend.advertise_capacity().unwrap().selector,
            AdmissionSelectorState::Present
        );
        assert_eq!(
            backend.withdraw_capacity().unwrap().selector,
            AdmissionSelectorState::Absent
        );

        let requests = &backend.transport().requests;
        assert_eq!(requests[1].method, HttpMethod::Post);
        assert_eq!(
            requests[1].body,
            Some(serde_json::json!({ "labels": [RESERVED_ADMISSION_LABEL] }))
        );
        assert_eq!(requests[4].method, HttpMethod::Delete);
        assert!(requests[4].path.ends_with("/42/labels/runnermesh-admit"));
        assert!(requests
            .iter()
            .all(|request| !request.path.contains("groups")));
        assert!(backend.transport().credential_was_nonempty);
    }

    #[test]
    fn rest_backend_refuses_runner_identity_and_selector_collision_without_mutation() {
        let identity_drift = serde_json::json!({
            "total_count": 1,
            "runners": [{
                "id": 42,
                "name": "different-runner",
                "status": "online",
                "busy": false,
                "labels": []
            }]
        });
        let collision = serde_json::json!({
            "total_count": 2,
            "runners": [
                {
                    "id": 42,
                    "name": "trusted-runner",
                    "status": "online",
                    "busy": false,
                    "labels": []
                },
                {
                    "id": 99,
                    "name": "unrelated-runner",
                    "status": "online",
                    "busy": false,
                    "labels": [{"name": "RUNNERMESH-ADMIT", "type": "custom"}]
                }
            ]
        });

        for (body, expected) in [
            (identity_drift, AdmissionBackendError::RunnerIdentityDrift),
            (collision, AdmissionBackendError::SelectorCollision),
        ] {
            let transport = FakeTransport {
                responses: [response(body)].into_iter().collect(),
                ..FakeTransport::default()
            };
            let mut backend = GithubRestAdmissionBackend::new(
                binding(true),
                transport,
                FakeCredentialProvider::default(),
            )
            .unwrap();
            assert_eq!(backend.withdraw_capacity().unwrap_err(), expected);
            assert_eq!(backend.transport().requests.len(), 1);
            assert_eq!(backend.transport().requests[0].method, HttpMethod::Get);
        }
    }

    #[test]
    fn rest_backend_refuses_inconsistent_counts_and_unknown_runner_status() {
        let inconsistent_count = serde_json::json!({
            "total_count": 2,
            "runners": [{
                "id": 42,
                "name": "trusted-runner",
                "status": "online",
                "busy": false,
                "labels": []
            }]
        });
        let unknown_status = serde_json::json!({
            "total_count": 1,
            "runners": [{
                "id": 42,
                "name": "trusted-runner",
                "status": "unexpected",
                "busy": false,
                "labels": []
            }]
        });
        for (body, expected) in [
            (
                inconsistent_count,
                AdmissionBackendError::SelectorObservationUnknown,
            ),
            (unknown_status, AdmissionBackendError::MalformedResponse),
        ] {
            let transport = FakeTransport {
                responses: [response(body)].into_iter().collect(),
                ..FakeTransport::default()
            };
            let mut backend = GithubRestAdmissionBackend::new(
                binding(true),
                transport,
                FakeCredentialProvider::default(),
            )
            .unwrap();
            assert_eq!(backend.observe_admission_selector().unwrap_err(), expected);
            assert_eq!(backend.transport().requests.len(), 1);
        }
    }

    #[test]
    fn existing_reserved_label_without_ownership_receipt_refuses_removal() {
        let transport = FakeTransport {
            responses: [response(runner(&[RESERVED_ADMISSION_LABEL]))]
                .into_iter()
                .collect(),
            ..FakeTransport::default()
        };
        let mut backend = GithubRestAdmissionBackend::new(
            binding(false),
            transport,
            FakeCredentialProvider::default(),
        )
        .unwrap();

        assert_eq!(
            backend.withdraw_capacity().unwrap_err(),
            AdmissionBackendError::ReservedLabelOwnershipDrift
        );
        assert!(backend.transport().requests.is_empty());
    }

    #[test]
    fn missing_credential_fails_before_transport_and_debug_is_redacted() {
        let mut provider = FakeCredentialProvider { unavailable: true };
        assert_eq!(
            provider
                .resolve(&CredentialReference::new("provider", "key").unwrap())
                .unwrap_err(),
            AdmissionBackendError::CredentialUnavailable
        );
        let lease = CredentialLease::from_secret("synthetic-secret-shape").unwrap();
        assert_eq!(format!("{lease:?}"), "CredentialLease([REDACTED])");

        let mut backend = GithubRestAdmissionBackend::new(
            binding(true),
            FakeTransport::default(),
            FakeCredentialProvider { unavailable: true },
        )
        .unwrap();
        assert_eq!(
            backend.observe_admission_selector().unwrap_err(),
            AdmissionBackendError::CredentialUnavailable
        );
        assert!(backend.transport().requests.is_empty());
    }

    #[test]
    fn persisted_binding_contains_only_a_secret_reference() {
        let serialized = serde_json::to_string(&binding(true)).unwrap();

        assert!(serialized.contains("windows-credential-manager"));
        assert!(serialized.contains("runnermesh-admission"));
        assert!(!serialized.contains("synthetic-not-a-real-token"));
        assert!(!serialized.contains("synthetic-secret-shape"));
    }

    #[test]
    fn retry_policy_is_bounded_and_honors_rate_limit_with_a_cap() {
        let backend = SyntheticBackend::with_results([
            Err(AdmissionBackendError::RateLimited {
                retry_after_seconds: 120,
            }),
            Err(AdmissionBackendError::ApiUnavailable),
            Err(AdmissionBackendError::Timeout),
            Err(AdmissionBackendError::ApiUnavailable),
        ]);
        let mut controller = AdmissionController::with_retry_policy(
            backend,
            RetryPolicy {
                max_attempts: 3,
                base_delay_seconds: 2,
                maximum_delay_seconds: 30,
            },
        );

        let retries = (0..4)
            .map(|_| {
                controller
                    .reconcile(DesiredAdmissionState::Drained, consistent(false))
                    .retry
            })
            .collect::<Vec<_>>();
        assert_eq!(retries[0].unwrap().after_seconds, 30);
        assert_eq!(retries[1].unwrap().attempt, 2);
        assert_eq!(retries[2].unwrap().attempt, 3);
        assert!(
            retries[3].is_none(),
            "no fourth automatic retry is scheduled"
        );
        assert_eq!(controller.backend().destructive_worker_actions, 0);
    }

    #[test]
    fn stable_snapshot_json_distinguishes_desired_from_achieved_state() {
        let snapshot = AdmissionControlSnapshot {
            desired: DesiredAdmissionState::Drained,
            lifecycle: AdmissionLifecycleState::WithdrawalBlocked,
            selector: AdmissionSelectorState::Present,
            exact_runner_identity: ExactRunnerIdentityState::Verified,
            reserved_label_ownership: ReservedLabelOwnershipState::Verified,
            active_bound_worker: true,
            reason_code: Some(reason("admission-api-unavailable")),
            retry: Some(RetryDirective {
                attempt: 1,
                after_seconds: 2,
            }),
        };
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["desired"], "DRAINED");
        assert_eq!(json["lifecycle"], "WITHDRAWAL_BLOCKED");
        assert_eq!(json["selector"], "PRESENT");
        assert!(!snapshot.achieved_drained());
    }
}
