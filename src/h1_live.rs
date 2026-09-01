use std::path::{Component, Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use serde::{Deserialize, Serialize};

use crate::{
    assess_h1_workflow_source, verify_h1_readiness, AdmissionBackendError, AdmissionBinding,
    AdmissionControlBackend, AdmissionSelectorState, CredentialProvider, CredentialReference,
    EvidenceProvenance, EvidenceState, GithubHttpRequest, GithubHttpTransport, H1ReadinessEvidence,
    H1ReadinessReceipt, H1RestoreBaseline, H1WorkflowTemplateAssessment, HttpMethod,
    RegistrationScope, H1_READINESS_SCHEMA_VERSION, H1_TRANSACTION_FAMILY_ID,
    RESERVED_ADMISSION_LABEL,
};

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpaqueIdentityReference {
    pub provider: String,
    pub key: String,
}

impl OpaqueIdentityReference {
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
            Err("identity reference components must be opaque printable tokens")
        }
    }

    pub fn is_valid(&self) -> bool {
        valid_reference_component(&self.provider) && valid_reference_component(&self.key)
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactLocalRunnerBinding {
    pub runner_home: PathBuf,
    pub work_root: PathBuf,
    pub listener_image: PathBuf,
    pub worker_image: PathBuf,
    pub execution_identity_ref: OpaqueIdentityReference,
}

impl ExactLocalRunnerBinding {
    pub fn new(
        runner_home: impl Into<PathBuf>,
        work_root: impl Into<PathBuf>,
        execution_identity_ref: OpaqueIdentityReference,
    ) -> Result<Self, &'static str> {
        let runner_home = runner_home.into();
        let work_root = work_root.into();
        let binding = Self {
            listener_image: runner_home.join("bin").join("Runner.Listener.exe"),
            worker_image: runner_home.join("bin").join("Runner.Worker.exe"),
            runner_home,
            work_root,
            execution_identity_ref,
        };
        if binding.is_valid() {
            Ok(binding)
        } else {
            Err("exact local runner binding is invalid")
        }
    }

    pub fn is_valid(&self) -> bool {
        absolute_path_like(&self.runner_home)
            && absolute_path_like(&self.work_root)
            && absolute_path_like(&self.listener_image)
            && absolute_path_like(&self.worker_image)
            && traversal_free(&self.runner_home)
            && traversal_free(&self.work_root)
            && traversal_free(&self.listener_image)
            && traversal_free(&self.worker_image)
            && normalized_path(&self.runner_home) != normalized_path(&self.work_root)
            && normalized_path(&self.listener_image)
                == normalized_path(&self.runner_home.join("bin").join("Runner.Listener.exe"))
            && normalized_path(&self.worker_image)
                == normalized_path(&self.runner_home.join("bin").join("Runner.Worker.exe"))
            && self.execution_identity_ref.is_valid()
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedWorkflowBinding {
    pub owner: String,
    pub repository: String,
    pub workflow_path: String,
    pub immutable_ref: String,
    pub expected_blob_sha: String,
    pub expected_runner_name: String,
}

impl TrustedWorkflowBinding {
    pub fn is_valid(&self) -> bool {
        valid_path_component(&self.owner)
            && valid_path_component(&self.repository)
            && valid_workflow_path(&self.workflow_path)
            && full_git_oid(&self.immutable_ref)
            && full_git_oid(&self.expected_blob_sha)
            && !self.expected_runner_name.trim().is_empty()
            && !self.expected_runner_name.contains(['\r', '\n'])
    }

    pub fn repository_full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repository)
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreReadinessBinding {
    pub transaction_family: String,
    pub baseline: H1RestoreBaseline,
    pub recovery_plan_ref: String,
}

impl RestoreReadinessBinding {
    pub fn is_valid(&self) -> bool {
        self.transaction_family == H1_TRANSACTION_FAMILY_ID
            && valid_reference_component(&self.recovery_plan_ref)
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct H1LiveBinding {
    pub admission: AdmissionBinding,
    pub local: ExactLocalRunnerBinding,
    pub workflow: TrustedWorkflowBinding,
    pub restore: RestoreReadinessBinding,
}

impl H1LiveBinding {
    pub fn is_valid(&self) -> bool {
        self.admission.is_valid()
            && self.admission.has_valid_ownership()
            && self
                .admission
                .reserved_label
                .eq_ignore_ascii_case(RESERVED_ADMISSION_LABEL)
            && self.local.is_valid()
            && self.workflow.is_valid()
            && self.workflow.expected_runner_name == self.admission.runner_name
            && self.workflow_is_in_registration_scope()
            && self.restore.is_valid()
    }

    fn workflow_is_in_registration_scope(&self) -> bool {
        match &self.admission.scope {
            RegistrationScope::Organization { organization } => {
                self.workflow.owner.eq_ignore_ascii_case(organization)
            }
            RegistrationScope::Repository { owner, repository } => {
                self.workflow.owner.eq_ignore_ascii_case(owner)
                    && self.workflow.repository.eq_ignore_ascii_case(repository)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactLocalBindingObservation {
    pub runner_home: EvidenceState,
    pub work_root: EvidenceState,
    pub listener_image: EvidenceState,
    pub worker_image: EvidenceState,
    pub execution_identity: EvidenceState,
    pub work_root_ownership: EvidenceState,
    pub active_bound_worker: Option<bool>,
}

impl ExactLocalBindingObservation {
    pub fn exact_identity_ready(self) -> EvidenceState {
        combine_states([
            self.runner_home,
            self.work_root,
            self.listener_image,
            self.worker_image,
            self.execution_identity,
            self.work_root_ownership,
        ])
    }
}

pub trait ExactLocalBindingSource {
    fn observe(&mut self, binding: &ExactLocalRunnerBinding) -> ExactLocalBindingObservation;
}

/// Read-only identity/ownership boundary. The filesystem collector never
/// substitutes path existence for execution-identity or work-root ownership.
pub trait LocalIdentityOwnershipVerifier {
    fn execution_identity(&mut self, binding: &ExactLocalRunnerBinding) -> EvidenceState;
    fn work_root_ownership(&mut self, binding: &ExactLocalRunnerBinding) -> EvidenceState;
    fn active_bound_worker(&mut self, binding: &ExactLocalRunnerBinding) -> Option<bool>;
}

pub struct FilesystemExactLocalBindingSource<V> {
    verifier: V,
}

impl<V> FilesystemExactLocalBindingSource<V> {
    pub fn new(verifier: V) -> Self {
        Self { verifier }
    }

    pub fn verifier(&self) -> &V {
        &self.verifier
    }
}

impl<V: LocalIdentityOwnershipVerifier> ExactLocalBindingSource
    for FilesystemExactLocalBindingSource<V>
{
    fn observe(&mut self, binding: &ExactLocalRunnerBinding) -> ExactLocalBindingObservation {
        if !binding.is_valid() {
            return ExactLocalBindingObservation {
                runner_home: EvidenceState::Fail,
                work_root: EvidenceState::Fail,
                listener_image: EvidenceState::Fail,
                worker_image: EvidenceState::Fail,
                execution_identity: EvidenceState::Fail,
                work_root_ownership: EvidenceState::Fail,
                active_bound_worker: None,
            };
        }
        ExactLocalBindingObservation {
            runner_home: path_kind_state(&binding.runner_home, true),
            work_root: path_kind_state(&binding.work_root, true),
            listener_image: path_kind_state(&binding.listener_image, false),
            worker_image: path_kind_state(&binding.worker_image, false),
            execution_identity: self.verifier.execution_identity(binding),
            work_root_ownership: self.verifier.work_root_ownership(binding),
            active_bound_worker: self.verifier.active_bound_worker(binding),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GithubAdmissionReadiness {
    pub authority_configured: EvidenceState,
    pub exact_runner_identity: EvidenceState,
    pub reserved_selector: EvidenceState,
    pub selector_unique: EvidenceState,
    pub selector: AdmissionSelectorState,
    pub runner_online: Option<bool>,
}

pub fn observe_github_admission_readiness<B: AdmissionControlBackend>(
    backend: &mut B,
    binding: &AdmissionBinding,
) -> GithubAdmissionReadiness {
    let mut readiness = GithubAdmissionReadiness {
        authority_configured: EvidenceState::Unknown,
        exact_runner_identity: EvidenceState::Unknown,
        reserved_selector: if binding.is_valid() && binding.has_valid_ownership() {
            EvidenceState::Pass
        } else {
            EvidenceState::Fail
        },
        selector_unique: EvidenceState::Unknown,
        selector: AdmissionSelectorState::Unknown,
        runner_online: None,
    };
    match backend.observe_admission_selector() {
        Ok(observed) => {
            readiness.authority_configured = EvidenceState::Pass;
            readiness.exact_runner_identity = EvidenceState::Pass;
            readiness.selector_unique = EvidenceState::Pass;
            readiness.selector = observed.selector;
            readiness.runner_online = Some(observed.runner_online);
        }
        Err(error) => match error {
            AdmissionBackendError::CredentialUnavailable
            | AdmissionBackendError::CredentialResolutionFailed
            | AdmissionBackendError::CredentialMalformed
            | AdmissionBackendError::AuthenticationFailed => {
                readiness.authority_configured = EvidenceState::Fail;
            }
            AdmissionBackendError::ApiUnavailable
            | AdmissionBackendError::Timeout
            | AdmissionBackendError::RateLimited { .. } => {}
            AdmissionBackendError::RunnerNotFound | AdmissionBackendError::RunnerIdentityDrift => {
                readiness.authority_configured = EvidenceState::Pass;
                readiness.exact_runner_identity = EvidenceState::Fail;
            }
            AdmissionBackendError::ReservedLabelOwnershipDrift
            | AdmissionBackendError::BindingInvalid => {
                readiness.authority_configured = EvidenceState::Pass;
                readiness.reserved_selector = EvidenceState::Fail;
            }
            AdmissionBackendError::SelectorCollision => {
                readiness.authority_configured = EvidenceState::Pass;
                readiness.selector_unique = EvidenceState::Fail;
            }
            AdmissionBackendError::MutationReadbackUnconfirmed { selector } => {
                readiness.authority_configured = EvidenceState::Pass;
                readiness.selector = selector;
            }
            AdmissionBackendError::MalformedResponse
            | AdmissionBackendError::SelectorObservationUnknown => {
                readiness.authority_configured = EvidenceState::Pass;
            }
        },
    }
    readiness
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowPresence {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedWorkflowObservation {
    pub presence: WorkflowPresence,
    pub repository_full_name: Option<String>,
    pub workflow_path: Option<String>,
    pub immutable_ref: Option<String>,
    pub blob_sha: Option<String>,
    pub source_assessment: Option<H1WorkflowTemplateAssessment>,
    pub runtime_runner_binding: EvidenceState,
}

impl TrustedWorkflowObservation {
    pub fn unknown() -> Self {
        Self {
            presence: WorkflowPresence::Unknown,
            repository_full_name: None,
            workflow_path: None,
            immutable_ref: None,
            blob_sha: None,
            source_assessment: None,
            runtime_runner_binding: EvidenceState::Unknown,
        }
    }

    pub fn absent(binding: &TrustedWorkflowBinding) -> Self {
        Self {
            presence: WorkflowPresence::Absent,
            repository_full_name: Some(binding.repository_full_name()),
            workflow_path: Some(binding.workflow_path.clone()),
            immutable_ref: Some(binding.immutable_ref.clone()),
            blob_sha: None,
            source_assessment: None,
            runtime_runner_binding: EvidenceState::Unknown,
        }
    }
}

pub fn verify_trusted_workflow(
    binding: &TrustedWorkflowBinding,
    observation: &TrustedWorkflowObservation,
) -> EvidenceState {
    if !binding.is_valid() {
        return EvidenceState::Fail;
    }
    match observation.presence {
        WorkflowPresence::Unknown => EvidenceState::Unknown,
        WorkflowPresence::Absent => EvidenceState::Fail,
        WorkflowPresence::Present => {
            let exact = observation.repository_full_name.as_deref()
                == Some(binding.repository_full_name().as_str())
                && observation.workflow_path.as_deref() == Some(binding.workflow_path.as_str())
                && observation.immutable_ref.as_deref() == Some(binding.immutable_ref.as_str())
                && observation.blob_sha.as_deref() == Some(binding.expected_blob_sha.as_str())
                && observation
                    .source_assessment
                    .is_some_and(H1WorkflowTemplateAssessment::source_contract_ready);
            if !exact || observation.runtime_runner_binding == EvidenceState::Fail {
                EvidenceState::Fail
            } else {
                observation.runtime_runner_binding
            }
        }
    }
}

pub struct GithubWorkflowClient<T, C> {
    transport: T,
    credentials: C,
    credential_ref: CredentialReference,
}

impl<T, C> GithubWorkflowClient<T, C> {
    pub fn new(transport: T, credentials: C, credential_ref: CredentialReference) -> Self {
        Self {
            transport,
            credentials,
            credential_ref,
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }
}

impl<T: GithubHttpTransport, C: CredentialProvider> GithubWorkflowClient<T, C> {
    pub fn observe(
        &mut self,
        binding: &TrustedWorkflowBinding,
    ) -> Result<TrustedWorkflowObservation, AdmissionBackendError> {
        if !binding.is_valid() {
            return Err(AdmissionBackendError::BindingInvalid);
        }
        let credential = self.credentials.resolve(&self.credential_ref)?;
        let response = self
            .transport
            .send(
                &GithubHttpRequest {
                    method: HttpMethod::Get,
                    path: workflow_contents_path(binding),
                    body: None,
                },
                &credential,
            )
            .map_err(|error| match error {
                crate::GithubTransportError::Unavailable => AdmissionBackendError::ApiUnavailable,
                crate::GithubTransportError::Timeout => AdmissionBackendError::Timeout,
                crate::GithubTransportError::InvalidResponse => {
                    AdmissionBackendError::MalformedResponse
                }
            })?;
        match response.status {
            200 => {}
            401 | 403 if response.retry_after_seconds.is_none() => {
                return Err(AdmissionBackendError::AuthenticationFailed)
            }
            403 | 429 => {
                return Err(AdmissionBackendError::RateLimited {
                    retry_after_seconds: response.retry_after_seconds.unwrap_or(1),
                })
            }
            404 => return Ok(TrustedWorkflowObservation::absent(binding)),
            500..=599 => return Err(AdmissionBackendError::ApiUnavailable),
            _ => return Err(AdmissionBackendError::MalformedResponse),
        }
        let parsed: GithubWorkflowContent = serde_json::from_slice(&response.body)
            .map_err(|_| AdmissionBackendError::MalformedResponse)?;
        if parsed.kind != "file"
            || parsed.path != binding.workflow_path
            || !full_git_oid(&parsed.sha)
            || !parsed.encoding.eq_ignore_ascii_case("base64")
        {
            return Err(AdmissionBackendError::MalformedResponse);
        }
        let source = decode_base64(&parsed.content)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or(AdmissionBackendError::MalformedResponse)?;
        Ok(TrustedWorkflowObservation {
            presence: WorkflowPresence::Present,
            repository_full_name: Some(binding.repository_full_name()),
            workflow_path: Some(parsed.path),
            immutable_ref: Some(binding.immutable_ref.clone()),
            blob_sha: Some(parsed.sha),
            source_assessment: Some(assess_h1_workflow_source(&source)),
            // The contents API proves the immutable source assertion but not
            // the private repository variable value used at runtime.
            runtime_runner_binding: EvidenceState::Unknown,
        })
    }
}

#[derive(Deserialize)]
struct GithubWorkflowContent {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    sha: String,
    encoding: String,
    content: String,
}

/// GET-only GitHub adapter that proves the exact runner is visible from the
/// exact trusted repository. It cannot mutate runner groups, repository access,
/// labels, registration, workflows, or dispatches.
pub struct GithubRepositoryAccessClient<T, C> {
    transport: T,
    credentials: C,
    credential_ref: CredentialReference,
}

impl<T, C> GithubRepositoryAccessClient<T, C> {
    pub fn new(transport: T, credentials: C, credential_ref: CredentialReference) -> Self {
        Self {
            transport,
            credentials,
            credential_ref,
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }
}

impl<T: GithubHttpTransport, C: CredentialProvider> GithubRepositoryAccessClient<T, C> {
    pub fn observe(
        &mut self,
        binding: &H1LiveBinding,
    ) -> Result<RepositoryRunnerAccessObservation, AdmissionBackendError> {
        if !binding.is_valid() || self.credential_ref != binding.admission.credential_ref {
            return Err(AdmissionBackendError::BindingInvalid);
        }
        let credential = self.credentials.resolve(&self.credential_ref)?;
        let response = self
            .transport
            .send(
                &GithubHttpRequest {
                    method: HttpMethod::Get,
                    path: repository_runner_path(binding),
                    body: None,
                },
                &credential,
            )
            .map_err(|error| match error {
                crate::GithubTransportError::Unavailable => AdmissionBackendError::ApiUnavailable,
                crate::GithubTransportError::Timeout => AdmissionBackendError::Timeout,
                crate::GithubTransportError::InvalidResponse => {
                    AdmissionBackendError::MalformedResponse
                }
            })?;
        match response.status {
            200 => {}
            401 | 403 if response.retry_after_seconds.is_none() => {
                return Err(AdmissionBackendError::AuthenticationFailed);
            }
            403 | 429 => {
                return Err(AdmissionBackendError::RateLimited {
                    retry_after_seconds: response.retry_after_seconds.unwrap_or(1),
                });
            }
            404 => {
                return Ok(RepositoryRunnerAccessObservation::from_bound_client(
                    binding,
                    EvidenceState::Fail,
                ));
            }
            500..=599 => return Err(AdmissionBackendError::ApiUnavailable),
            _ => return Err(AdmissionBackendError::MalformedResponse),
        }
        if response.has_next_page {
            return Err(AdmissionBackendError::MalformedResponse);
        }
        let runner: GithubRepositoryRunner = serde_json::from_slice(&response.body)
            .map_err(|_| AdmissionBackendError::MalformedResponse)?;
        if runner.id != binding.admission.runner_id || runner.name != binding.admission.runner_name
        {
            return Err(AdmissionBackendError::RunnerIdentityDrift);
        }
        Ok(RepositoryRunnerAccessObservation::from_bound_client(
            binding,
            EvidenceState::Pass,
        ))
    }
}

#[derive(Deserialize)]
struct GithubRepositoryRunner {
    id: u64,
    name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RouteState {
    Present,
    Absent,
    Unknown,
}

/// Read-only proof that the exact bound runner is selectable by the exact
/// trusted workflow repository. Organization-scoped runners require this
/// separate repository-access proof because runner visibility alone does not
/// establish runner-group or selected-repository access.
#[derive(Clone, Eq, PartialEq)]
pub struct RepositoryRunnerAccessObservation {
    binding: Option<H1LiveBinding>,
    access: EvidenceState,
}

impl std::fmt::Debug for RepositoryRunnerAccessObservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RepositoryRunnerAccessObservation")
            .field("binding", &self.binding.as_ref().map(|_| "[BOUND]"))
            .field("access", &self.access)
            .finish()
    }
}

impl RepositoryRunnerAccessObservation {
    pub fn unknown() -> Self {
        Self {
            binding: None,
            access: EvidenceState::Unknown,
        }
    }

    fn from_bound_client(binding: &H1LiveBinding, access: EvidenceState) -> Self {
        Self {
            binding: Some(binding.clone()),
            access,
        }
    }

    pub fn state(&self) -> EvidenceState {
        self.access
    }
}

pub fn verify_repository_runner_access(
    binding: &H1LiveBinding,
    observation: &RepositoryRunnerAccessObservation,
) -> EvidenceState {
    if !binding.is_valid() {
        return EvidenceState::Fail;
    }
    if observation.binding.as_ref() != Some(binding) {
        return match observation.access {
            EvidenceState::Unknown if observation.binding.is_none() => EvidenceState::Unknown,
            _ => EvidenceState::Fail,
        };
    }
    observation.access
}

impl RouteState {
    fn evidence(self) -> EvidenceState {
        match self {
            Self::Present => EvidenceState::Pass,
            Self::Absent => EvidenceState::Fail,
            Self::Unknown => EvidenceState::Unknown,
        }
    }
}

pub fn verify_h1_routing(
    binding: &H1LiveBinding,
    workflow: &TrustedWorkflowObservation,
    admission: GithubAdmissionReadiness,
    repository_access: &RepositoryRunnerAccessObservation,
) -> RouteState {
    let workflow = verify_trusted_workflow(&binding.workflow, workflow);
    let repository_access = verify_repository_runner_access(binding, repository_access);
    if workflow == EvidenceState::Fail
        || repository_access == EvidenceState::Fail
        || admission.authority_configured == EvidenceState::Fail
        || admission.exact_runner_identity == EvidenceState::Fail
        || admission.reserved_selector == EvidenceState::Fail
        || admission.selector_unique == EvidenceState::Fail
        || admission.selector == AdmissionSelectorState::Absent
        || admission.runner_online == Some(false)
    {
        RouteState::Absent
    } else if workflow == EvidenceState::Pass
        && repository_access == EvidenceState::Pass
        && admission.authority_configured == EvidenceState::Pass
        && admission.exact_runner_identity == EvidenceState::Pass
        && admission.reserved_selector == EvidenceState::Pass
        && admission.selector_unique == EvidenceState::Pass
        && admission.selector == AdmissionSelectorState::Present
        && admission.runner_online == Some(true)
    {
        RouteState::Present
    } else {
        RouteState::Unknown
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct H1LiveReadinessInputs<'a> {
    pub binding: &'a H1LiveBinding,
    pub source_ready: EvidenceState,
    pub host_prestate_ready: EvidenceState,
    pub github: GithubAdmissionReadiness,
    pub local: ExactLocalBindingObservation,
    pub workflow: &'a TrustedWorkflowObservation,
    pub repository_access: &'a RepositoryRunnerAccessObservation,
    pub rollback_ready: EvidenceState,
    pub recovery_ready: EvidenceState,
    pub owner_gate_ready: EvidenceState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct H1LiveReadinessCollection {
    pub evidence: H1ReadinessEvidence,
    pub receipt: H1ReadinessReceipt,
    pub live_readiness_executed: bool,
}

pub fn collect_h1_live_readiness(inputs: H1LiveReadinessInputs) -> H1LiveReadinessCollection {
    let trusted_workflow = verify_trusted_workflow(&inputs.binding.workflow, inputs.workflow);
    let routing = verify_h1_routing(
        inputs.binding,
        inputs.workflow,
        inputs.github,
        inputs.repository_access,
    );
    let evidence = H1ReadinessEvidence {
        schema_version: H1_READINESS_SCHEMA_VERSION,
        // This public composition seam accepts injected observations and is
        // therefore always synthetic. A future concrete live collector must
        // issue the crate-private attestation before H1 can be prepared.
        provenance: EvidenceProvenance::Synthetic,
        source_ready: inputs.source_ready,
        host_prestate_ready: inputs.host_prestate_ready,
        github_authority_configured: inputs.github.authority_configured,
        exact_runner_identity_ready: combine_states([
            inputs.github.exact_runner_identity,
            inputs.local.exact_identity_ready(),
        ]),
        reserved_selector_ready: inputs.github.reserved_selector,
        selector_unique: inputs.github.selector_unique,
        trusted_workflow_ready: trusted_workflow,
        routing_ready: routing.evidence(),
        rollback_ready: inputs.rollback_ready,
        recovery_ready: inputs.recovery_ready,
        owner_gate_ready: inputs.owner_gate_ready,
    };
    let receipt = verify_h1_readiness(evidence);
    H1LiveReadinessCollection {
        evidence,
        receipt,
        live_readiness_executed: false,
    }
}

fn workflow_contents_path(binding: &TrustedWorkflowBinding) -> String {
    let path = binding
        .workflow_path
        .split('/')
        .map(percent_encode_component)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "/repos/{}/{}/contents/{}?ref={}",
        percent_encode_component(&binding.owner),
        percent_encode_component(&binding.repository),
        path,
        percent_encode_component(&binding.immutable_ref)
    )
}

fn repository_runner_path(binding: &H1LiveBinding) -> String {
    format!(
        "/repos/{}/{}/actions/runners/{}",
        percent_encode_component(&binding.workflow.owner),
        percent_encode_component(&binding.workflow.repository),
        binding.admission.runner_id,
    )
}

fn path_kind_state(path: &Path, directory: bool) -> EvidenceState {
    let mut leaf = None;
    for (index, ancestor) in path.ancestors().enumerate() {
        match ancestor.symlink_metadata() {
            Ok(metadata) if metadata_is_link_or_reparse(&metadata) => return EvidenceState::Fail,
            Ok(metadata) if index == 0 => leaf = Some(metadata),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return EvidenceState::Fail
            }
            Err(_) => return EvidenceState::Unknown,
        }
    }
    match leaf {
        Some(metadata) if directory && metadata.is_dir() => EvidenceState::Pass,
        Some(metadata) if !directory && metadata.is_file() => EvidenceState::Pass,
        Some(_) | None => EvidenceState::Fail,
    }
}

fn metadata_is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    if windows_file_attributes_are_reparse(metadata.file_attributes()) {
        return true;
    }
    false
}

#[cfg(windows)]
fn windows_file_attributes_are_reparse(attributes: u32) -> bool {
    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            encoded.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let compact = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if compact.is_empty() || compact.len() % 4 != 0 {
        return None;
    }
    let mut decoded = Vec::with_capacity(compact.len() / 4 * 3);
    let (chunks, remainder) = compact.as_chunks::<4>();
    debug_assert!(remainder.is_empty());
    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' {
                return None;
            }
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            if !last {
                return None;
            }
            0
        } else {
            base64_value(chunk[3])?
        };
        decoded.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            decoded.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            decoded.push((c << 6) | d);
        }
    }
    Some(decoded)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn combine_states<const N: usize>(states: [EvidenceState; N]) -> EvidenceState {
    if states.contains(&EvidenceState::Fail) {
        EvidenceState::Fail
    } else if states.contains(&EvidenceState::Unknown) {
        EvidenceState::Unknown
    } else {
        EvidenceState::Pass
    }
}

fn absolute_path_like(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    let value = path.to_string_lossy().as_bytes().to_vec();
    value.len() >= 3
        && value[0].is_ascii_alphabetic()
        && value[1] == b':'
        && matches!(value[2], b'\\' | b'/')
}

fn traversal_free(path: &Path) -> bool {
    !path
        .components()
        .any(|component| component == Component::ParentDir)
        && !path
            .to_string_lossy()
            .replace('\\', "/")
            .split('/')
            .any(|part| part == "..")
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn valid_workflow_path(value: &str) -> bool {
    value.starts_with(".github/workflows/")
        && (value.ends_with(".yml") || value.ends_with(".yaml"))
        && value.is_ascii()
        && !value.contains(['\r', '\n', '\\'])
        && !value.split('/').any(|part| matches!(part, "" | "." | ".."))
}

fn full_git_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::{
        BaselineAdmissionState, CredentialLease, GithubHttpResponse, GithubTransportError,
        ReadinessDisposition, RegistrationScope, RemoteAdmissionObservation,
        ReservedLabelOwnership,
    };

    use super::*;

    const COMMIT: &str = "1111111111111111111111111111111111111111";
    const BLOB: &str = "2222222222222222222222222222222222222222";

    fn admission_binding() -> AdmissionBinding {
        let scope = RegistrationScope::Organization {
            organization: "example-org".to_owned(),
        };
        AdmissionBinding::new(
            scope.clone(),
            42,
            "synthetic-runner",
            CredentialReference::new("windows-credential-manager", "synthetic-h1").unwrap(),
            Some(ReservedLabelOwnership::for_runner(scope, 42)),
        )
        .unwrap()
    }

    fn workflow_binding() -> TrustedWorkflowBinding {
        TrustedWorkflowBinding {
            owner: "example-org".to_owned(),
            repository: "trusted-qualification".to_owned(),
            workflow_path: ".github/workflows/h1.yml".to_owned(),
            immutable_ref: COMMIT.to_owned(),
            expected_blob_sha: BLOB.to_owned(),
            expected_runner_name: "synthetic-runner".to_owned(),
        }
    }

    fn live_binding() -> H1LiveBinding {
        H1LiveBinding {
            admission: admission_binding(),
            local: local_binding(),
            workflow: workflow_binding(),
            restore: RestoreReadinessBinding {
                transaction_family: H1_TRANSACTION_FAMILY_ID.to_owned(),
                baseline: H1RestoreBaseline {
                    admission: BaselineAdmissionState::Advertised,
                    local_runner_expected_online: true,
                },
                recovery_plan_ref: "synthetic-recovery-plan".to_owned(),
            },
        }
    }

    fn local_binding() -> ExactLocalRunnerBinding {
        ExactLocalRunnerBinding::new(
            std::env::temp_dir().join("runnermesh-synthetic-runner"),
            std::env::temp_dir().join("runnermesh-synthetic-work"),
            OpaqueIdentityReference::new("synthetic-identity", "runner-owner").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn exact_binding_is_typed_and_normal_json_contains_no_secret_material() {
        let binding = live_binding();
        assert!(binding.is_valid());
        let json = serde_json::to_string(&binding).unwrap();
        assert!(json.contains(RESERVED_ADMISSION_LABEL));
        assert!(json.contains("synthetic-h1"));
        assert!(!json.contains("synthetic-token-shape"));
    }

    #[test]
    fn local_path_or_reserved_ownership_drift_fails_closed() {
        let mut local = local_binding();
        local.listener_image = local.runner_home.join("bin").join("other.exe");
        assert!(!local.is_valid());

        let mut local = local_binding();
        local.execution_identity_ref = OpaqueIdentityReference {
            provider: "synthetic-identity".to_owned(),
            key: "truncated\0identity".to_owned(),
        };
        assert!(!local.is_valid());

        let mut admission = admission_binding();
        admission.ownership = None;
        let binding = H1LiveBinding {
            admission,
            local: local_binding(),
            workflow: workflow_binding(),
            restore: RestoreReadinessBinding {
                transaction_family: H1_TRANSACTION_FAMILY_ID.to_owned(),
                baseline: H1RestoreBaseline {
                    admission: BaselineAdmissionState::Advertised,
                    local_runner_expected_online: true,
                },
                recovery_plan_ref: "synthetic-recovery-plan".to_owned(),
            },
        };
        assert!(!binding.is_valid());

        let mut workflow = workflow_binding();
        workflow.expected_runner_name = "other-runner".to_owned();
        let binding = H1LiveBinding {
            admission: admission_binding(),
            local: local_binding(),
            workflow,
            restore: RestoreReadinessBinding {
                transaction_family: H1_TRANSACTION_FAMILY_ID.to_owned(),
                baseline: H1RestoreBaseline {
                    admission: BaselineAdmissionState::Advertised,
                    local_runner_expected_online: true,
                },
                recovery_plan_ref: "synthetic-recovery-plan".to_owned(),
            },
        };
        assert!(!binding.is_valid());

        let mut binding = live_binding();
        binding.workflow.owner = "unrelated-org".to_owned();
        assert!(!binding.is_valid());

        let mut binding = live_binding();
        binding.admission.scope = RegistrationScope::Repository {
            owner: "example-org".to_owned(),
            repository: "different-repository".to_owned(),
        };
        assert!(!binding.is_valid());
    }

    struct PassingLocalVerifier;

    impl LocalIdentityOwnershipVerifier for PassingLocalVerifier {
        fn execution_identity(&mut self, _binding: &ExactLocalRunnerBinding) -> EvidenceState {
            EvidenceState::Pass
        }

        fn work_root_ownership(&mut self, _binding: &ExactLocalRunnerBinding) -> EvidenceState {
            EvidenceState::Pass
        }

        fn active_bound_worker(&mut self, _binding: &ExactLocalRunnerBinding) -> Option<bool> {
            Some(false)
        }
    }

    #[test]
    fn filesystem_binding_source_refuses_invalid_binding_before_verifier_evidence() {
        let mut binding = local_binding();
        binding.execution_identity_ref = OpaqueIdentityReference {
            provider: "synthetic-identity".to_owned(),
            key: "truncated\0identity".to_owned(),
        };
        let observed =
            FilesystemExactLocalBindingSource::new(PassingLocalVerifier).observe(&binding);

        assert_eq!(observed.runner_home, EvidenceState::Fail);
        assert_eq!(observed.work_root, EvidenceState::Fail);
        assert_eq!(observed.listener_image, EvidenceState::Fail);
        assert_eq!(observed.worker_image, EvidenceState::Fail);
        assert_eq!(observed.execution_identity, EvidenceState::Fail);
        assert_eq!(observed.work_root_ownership, EvidenceState::Fail);
        assert_eq!(observed.active_bound_worker, None);
    }

    #[test]
    fn filesystem_binding_source_requires_paths_and_injected_identity_evidence() {
        let binding = local_binding();
        std::fs::create_dir_all(binding.runner_home.join("bin")).unwrap();
        std::fs::create_dir_all(&binding.work_root).unwrap();
        std::fs::write(&binding.listener_image, b"synthetic").unwrap();
        std::fs::write(&binding.worker_image, b"synthetic").unwrap();

        let observed =
            FilesystemExactLocalBindingSource::new(PassingLocalVerifier).observe(&binding);
        assert_eq!(observed.exact_identity_ready(), EvidenceState::Pass);
        assert_eq!(observed.work_root, EvidenceState::Pass);
        assert_eq!(observed.active_bound_worker, Some(false));

        std::fs::remove_dir_all(&binding.runner_home).unwrap();
        std::fs::remove_dir_all(&binding.work_root).unwrap();
    }

    #[test]
    fn filesystem_binding_source_requires_the_exact_work_root_path() {
        let mut binding = local_binding();
        binding.work_root = binding.work_root.join("missing-bound-work-root");

        let observed =
            FilesystemExactLocalBindingSource::new(PassingLocalVerifier).observe(&binding);

        assert_eq!(observed.work_root, EvidenceState::Fail);
        assert_eq!(observed.exact_identity_ready(), EvidenceState::Fail);
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_binding_source_refuses_a_symlinked_ancestor() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "runnermesh-symlink-ancestor-{}",
            std::process::id()
        ));
        let real = root.join("real");
        std::fs::create_dir_all(real.join("bin")).unwrap();
        std::fs::write(real.join("bin").join("Runner.Listener.exe"), b"synthetic").unwrap();
        symlink(&real, root.join("linked")).unwrap();

        assert_eq!(
            path_kind_state(
                &root.join("linked").join("bin").join("Runner.Listener.exe"),
                false,
            ),
            EvidenceState::Fail
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_reparse_attribute_is_refused() {
        assert!(windows_file_attributes_are_reparse(
            FILE_ATTRIBUTE_REPARSE_POINT
        ));
        assert!(!windows_file_attributes_are_reparse(0));
    }

    struct FakeAdmissionBackend(Result<RemoteAdmissionObservation, AdmissionBackendError>);

    impl AdmissionControlBackend for FakeAdmissionBackend {
        fn observe_admission_selector(
            &mut self,
        ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            self.0.clone()
        }

        fn advertise_capacity(
            &mut self,
        ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            panic!("readiness must not mutate")
        }

        fn withdraw_capacity(
            &mut self,
        ) -> Result<RemoteAdmissionObservation, AdmissionBackendError> {
            panic!("readiness must not mutate")
        }
    }

    #[test]
    fn admission_readiness_classifies_auth_identity_collision_and_timeout() {
        for (result, authority, identity, unique) in [
            (
                Err(AdmissionBackendError::AuthenticationFailed),
                EvidenceState::Fail,
                EvidenceState::Unknown,
                EvidenceState::Unknown,
            ),
            (
                Err(AdmissionBackendError::RunnerIdentityDrift),
                EvidenceState::Pass,
                EvidenceState::Fail,
                EvidenceState::Unknown,
            ),
            (
                Err(AdmissionBackendError::SelectorCollision),
                EvidenceState::Pass,
                EvidenceState::Unknown,
                EvidenceState::Fail,
            ),
            (
                Err(AdmissionBackendError::Timeout),
                EvidenceState::Unknown,
                EvidenceState::Unknown,
                EvidenceState::Unknown,
            ),
        ] {
            let readiness = observe_github_admission_readiness(
                &mut FakeAdmissionBackend(result),
                &admission_binding(),
            );
            assert_eq!(readiness.authority_configured, authority);
            assert_eq!(readiness.exact_runner_identity, identity);
            assert_eq!(readiness.selector_unique, unique);
        }
    }

    #[test]
    fn workflow_and_routing_verifiers_distinguish_present_absent_and_unknown() {
        let live_binding = live_binding();
        let binding = &live_binding.workflow;
        let assessment = assess_h1_workflow_source(crate::h1_workflow_template());
        let present = TrustedWorkflowObservation {
            presence: WorkflowPresence::Present,
            repository_full_name: Some(binding.repository_full_name()),
            workflow_path: Some(binding.workflow_path.clone()),
            immutable_ref: Some(binding.immutable_ref.clone()),
            blob_sha: Some(binding.expected_blob_sha.clone()),
            source_assessment: Some(assessment),
            runtime_runner_binding: EvidenceState::Pass,
        };
        assert_eq!(
            verify_trusted_workflow(binding, &present),
            EvidenceState::Pass
        );
        assert_eq!(
            verify_trusted_workflow(binding, &TrustedWorkflowObservation::absent(binding)),
            EvidenceState::Fail
        );
        assert_eq!(
            verify_trusted_workflow(binding, &TrustedWorkflowObservation::unknown()),
            EvidenceState::Unknown
        );

        let admission = GithubAdmissionReadiness {
            authority_configured: EvidenceState::Pass,
            exact_runner_identity: EvidenceState::Pass,
            reserved_selector: EvidenceState::Pass,
            selector_unique: EvidenceState::Pass,
            selector: AdmissionSelectorState::Present,
            runner_online: Some(true),
        };
        let access = RepositoryRunnerAccessObservation::from_bound_client(
            &live_binding,
            EvidenceState::Pass,
        );
        assert_eq!(
            verify_h1_routing(&live_binding, &present, admission, &access),
            RouteState::Present
        );
        assert_eq!(
            verify_h1_routing(
                &live_binding,
                &present,
                GithubAdmissionReadiness {
                    selector: AdmissionSelectorState::Absent,
                    ..admission
                },
                &access,
            ),
            RouteState::Absent
        );
        assert_eq!(
            verify_h1_routing(
                &live_binding,
                &present,
                GithubAdmissionReadiness {
                    selector: AdmissionSelectorState::Unknown,
                    ..admission
                },
                &access,
            ),
            RouteState::Unknown
        );

        let mut wrong_repository_binding = live_binding.clone();
        wrong_repository_binding.workflow.repository = "unrelated".to_owned();
        assert!(wrong_repository_binding.is_valid());
        let wrong_repository = RepositoryRunnerAccessObservation::from_bound_client(
            &wrong_repository_binding,
            EvidenceState::Pass,
        );
        assert_eq!(
            verify_repository_runner_access(&live_binding, &wrong_repository),
            EvidenceState::Fail
        );
        assert_eq!(
            verify_h1_routing(&live_binding, &present, admission, &wrong_repository),
            RouteState::Absent
        );
        let mut wrong_runner_binding = live_binding.clone();
        wrong_runner_binding.admission.runner_id += 1;
        wrong_runner_binding.admission.ownership = Some(ReservedLabelOwnership::for_runner(
            wrong_runner_binding.admission.scope.clone(),
            wrong_runner_binding.admission.runner_id,
        ));
        assert!(wrong_runner_binding.is_valid());
        let wrong_runner = RepositoryRunnerAccessObservation::from_bound_client(
            &wrong_runner_binding,
            EvidenceState::Pass,
        );
        assert_eq!(
            verify_repository_runner_access(&live_binding, &wrong_runner),
            EvidenceState::Fail
        );
        let replayed_access = access.clone();
        let mut renamed_binding = live_binding.clone();
        renamed_binding.admission.runner_name = "renamed-runner".to_owned();
        renamed_binding.workflow.expected_runner_name = "renamed-runner".to_owned();
        assert!(renamed_binding.is_valid());
        assert_eq!(
            verify_repository_runner_access(&renamed_binding, &replayed_access),
            EvidenceState::Fail
        );
        let mut rekeyed_binding = live_binding.clone();
        rekeyed_binding.admission.credential_ref =
            CredentialReference::new("windows-credential-manager", "rotated-reference").unwrap();
        assert!(rekeyed_binding.is_valid());
        assert_eq!(
            verify_repository_runner_access(&rekeyed_binding, &replayed_access),
            EvidenceState::Fail
        );
        assert_eq!(
            verify_h1_routing(
                &live_binding,
                &present,
                admission,
                &RepositoryRunnerAccessObservation::unknown(),
            ),
            RouteState::Unknown
        );
    }

    #[test]
    fn readiness_derives_routing_and_does_not_accept_stale_positive_access() {
        let binding = live_binding();
        let workflow = TrustedWorkflowObservation {
            presence: WorkflowPresence::Present,
            repository_full_name: Some(binding.workflow.repository_full_name()),
            workflow_path: Some(binding.workflow.workflow_path.clone()),
            immutable_ref: Some(binding.workflow.immutable_ref.clone()),
            blob_sha: Some(binding.workflow.expected_blob_sha.clone()),
            source_assessment: Some(assess_h1_workflow_source(crate::h1_workflow_template())),
            runtime_runner_binding: EvidenceState::Pass,
        };
        let repository_access =
            RepositoryRunnerAccessObservation::from_bound_client(&binding, EvidenceState::Pass);
        let github = GithubAdmissionReadiness {
            authority_configured: EvidenceState::Pass,
            exact_runner_identity: EvidenceState::Pass,
            reserved_selector: EvidenceState::Pass,
            selector_unique: EvidenceState::Pass,
            selector: AdmissionSelectorState::Absent,
            runner_online: Some(true),
        };
        let local = ExactLocalBindingObservation {
            runner_home: EvidenceState::Pass,
            work_root: EvidenceState::Pass,
            listener_image: EvidenceState::Pass,
            worker_image: EvidenceState::Pass,
            execution_identity: EvidenceState::Pass,
            work_root_ownership: EvidenceState::Pass,
            active_bound_worker: Some(false),
        };

        let collected = collect_h1_live_readiness(H1LiveReadinessInputs {
            binding: &binding,
            source_ready: EvidenceState::Pass,
            host_prestate_ready: EvidenceState::Pass,
            github,
            local,
            workflow: &workflow,
            repository_access: &repository_access,
            rollback_ready: EvidenceState::Pass,
            recovery_ready: EvidenceState::Pass,
            owner_gate_ready: EvidenceState::Pass,
        });

        assert_eq!(collected.evidence.routing_ready, EvidenceState::Fail);
        assert!(!collected.receipt.h1_mutation_allowed);
    }

    #[derive(Default)]
    struct FakeTransport {
        responses: VecDeque<Result<GithubHttpResponse, GithubTransportError>>,
        methods: Vec<HttpMethod>,
        paths: Vec<String>,
    }

    impl GithubHttpTransport for FakeTransport {
        fn send(
            &mut self,
            request: &GithubHttpRequest,
            _credential: &CredentialLease,
        ) -> Result<GithubHttpResponse, GithubTransportError> {
            self.methods.push(request.method);
            self.paths.push(request.path.clone());
            self.responses.pop_front().unwrap()
        }
    }

    struct FakeProvider;

    impl CredentialProvider for FakeProvider {
        fn resolve(
            &mut self,
            _reference: &CredentialReference,
        ) -> Result<CredentialLease, AdmissionBackendError> {
            CredentialLease::from_secret("synthetic-token-shape")
                .map_err(|_| AdmissionBackendError::CredentialMalformed)
        }
    }

    #[test]
    fn workflow_client_reads_exact_content_identity_without_dispatch() {
        let binding = workflow_binding();
        let source = crate::h1_workflow_template();
        let body = serde_json::json!({
            "type": "file",
            "path": binding.workflow_path,
            "sha": BLOB,
            "encoding": "base64",
            "content": encode_base64(source.as_bytes())
        });
        let transport = FakeTransport {
            responses: [Ok(GithubHttpResponse {
                status: 200,
                body: serde_json::to_vec(&body).unwrap(),
                retry_after_seconds: None,
                has_next_page: false,
            })]
            .into_iter()
            .collect(),
            ..FakeTransport::default()
        };
        let mut client = GithubWorkflowClient::new(
            transport,
            FakeProvider,
            CredentialReference::new("windows-credential-manager", "synthetic-h1").unwrap(),
        );
        let observation = client.observe(&binding).unwrap();
        assert_eq!(
            verify_trusted_workflow(&binding, &observation),
            EvidenceState::Unknown
        );
        assert_eq!(observation.runtime_runner_binding, EvidenceState::Unknown);
        assert!(observation
            .source_assessment
            .is_some_and(H1WorkflowTemplateAssessment::source_contract_ready));
        assert_eq!(client.transport().paths.len(), 1);
        assert!(client.transport().paths[0].contains("/contents/"));
        assert!(!client.transport().paths[0].contains("dispatch"));
    }

    #[test]
    fn repository_access_client_proves_only_exact_repository_runner_visibility() {
        let binding = live_binding();
        let body = serde_json::json!({
            "id": binding.admission.runner_id,
            "name": binding.admission.runner_name,
        });
        let transport = FakeTransport {
            responses: [Ok(GithubHttpResponse {
                status: 200,
                body: serde_json::to_vec(&body).unwrap(),
                retry_after_seconds: None,
                has_next_page: false,
            })]
            .into_iter()
            .collect(),
            ..FakeTransport::default()
        };
        let mut client = GithubRepositoryAccessClient::new(
            transport,
            FakeProvider,
            CredentialReference::new("windows-credential-manager", "synthetic-h1").unwrap(),
        );

        let observation = client.observe(&binding).unwrap();

        assert_eq!(
            verify_repository_runner_access(&binding, &observation),
            EvidenceState::Pass
        );
        assert_eq!(client.transport().methods, [HttpMethod::Get]);
        assert_eq!(
            client.transport().paths,
            ["/repos/example-org/trusted-qualification/actions/runners/42"]
        );
    }

    #[test]
    fn repository_access_client_fails_closed_for_absence_and_identity_drift() {
        let binding = live_binding();
        let absent_transport = FakeTransport {
            responses: [Ok(GithubHttpResponse {
                status: 404,
                body: Vec::new(),
                retry_after_seconds: None,
                has_next_page: false,
            })]
            .into_iter()
            .collect(),
            ..FakeTransport::default()
        };
        let mut absent_client = GithubRepositoryAccessClient::new(
            absent_transport,
            FakeProvider,
            CredentialReference::new("windows-credential-manager", "synthetic-h1").unwrap(),
        );
        assert_eq!(
            absent_client.observe(&binding).unwrap().state(),
            EvidenceState::Fail
        );

        let drift_body = serde_json::json!({
            "id": binding.admission.runner_id,
            "name": "unrelated-runner",
        });
        let drift_transport = FakeTransport {
            responses: [Ok(GithubHttpResponse {
                status: 200,
                body: serde_json::to_vec(&drift_body).unwrap(),
                retry_after_seconds: None,
                has_next_page: false,
            })]
            .into_iter()
            .collect(),
            ..FakeTransport::default()
        };
        let mut drift_client = GithubRepositoryAccessClient::new(
            drift_transport,
            FakeProvider,
            CredentialReference::new("windows-credential-manager", "synthetic-h1").unwrap(),
        );
        assert_eq!(
            drift_client.observe(&binding),
            Err(AdmissionBackendError::RunnerIdentityDrift)
        );
    }

    #[test]
    fn synthetic_readiness_can_prove_adapters_but_never_authorize_h1() {
        let binding = live_binding();
        let workflow = TrustedWorkflowObservation {
            presence: WorkflowPresence::Present,
            repository_full_name: Some(binding.workflow.repository_full_name()),
            workflow_path: Some(binding.workflow.workflow_path.clone()),
            immutable_ref: Some(binding.workflow.immutable_ref.clone()),
            blob_sha: Some(binding.workflow.expected_blob_sha.clone()),
            source_assessment: Some(assess_h1_workflow_source(crate::h1_workflow_template())),
            runtime_runner_binding: EvidenceState::Pass,
        };
        let repository_access =
            RepositoryRunnerAccessObservation::from_bound_client(&binding, EvidenceState::Pass);
        let pass_local = ExactLocalBindingObservation {
            runner_home: EvidenceState::Pass,
            work_root: EvidenceState::Pass,
            listener_image: EvidenceState::Pass,
            worker_image: EvidenceState::Pass,
            execution_identity: EvidenceState::Pass,
            work_root_ownership: EvidenceState::Pass,
            active_bound_worker: Some(false),
        };
        let pass_github = GithubAdmissionReadiness {
            authority_configured: EvidenceState::Pass,
            exact_runner_identity: EvidenceState::Pass,
            reserved_selector: EvidenceState::Pass,
            selector_unique: EvidenceState::Pass,
            selector: AdmissionSelectorState::Present,
            runner_online: Some(true),
        };
        let collection = collect_h1_live_readiness(H1LiveReadinessInputs {
            binding: &binding,
            source_ready: EvidenceState::Pass,
            host_prestate_ready: EvidenceState::Pass,
            github: pass_github,
            local: pass_local,
            workflow: &workflow,
            repository_access: &repository_access,
            rollback_ready: EvidenceState::Pass,
            recovery_ready: EvidenceState::Pass,
            owner_gate_ready: EvidenceState::Pass,
        });
        assert_eq!(
            collection.receipt.disposition,
            ReadinessDisposition::PassSynthetic
        );
        assert!(!collection.receipt.h1_mutation_allowed);
        assert!(!collection.live_readiness_executed);
    }

    #[test]
    fn fail_dominates_unknown_and_injected_collection_stays_synthetic() {
        assert_eq!(
            combine_states([EvidenceState::Pass, EvidenceState::Unknown]),
            EvidenceState::Unknown
        );
        assert_eq!(
            combine_states([EvidenceState::Unknown, EvidenceState::Fail]),
            EvidenceState::Fail
        );
        let binding = live_binding();
        let workflow = TrustedWorkflowObservation::unknown();
        let repository_access = RepositoryRunnerAccessObservation::unknown();
        let evidence = collect_h1_live_readiness(H1LiveReadinessInputs {
            binding: &binding,
            source_ready: EvidenceState::Pass,
            host_prestate_ready: EvidenceState::Unknown,
            github: GithubAdmissionReadiness {
                authority_configured: EvidenceState::Unknown,
                exact_runner_identity: EvidenceState::Unknown,
                reserved_selector: EvidenceState::Pass,
                selector_unique: EvidenceState::Unknown,
                selector: AdmissionSelectorState::Unknown,
                runner_online: None,
            },
            local: ExactLocalBindingObservation {
                runner_home: EvidenceState::Pass,
                work_root: EvidenceState::Pass,
                listener_image: EvidenceState::Pass,
                worker_image: EvidenceState::Pass,
                execution_identity: EvidenceState::Unknown,
                work_root_ownership: EvidenceState::Unknown,
                active_bound_worker: None,
            },
            workflow: &workflow,
            repository_access: &repository_access,
            rollback_ready: EvidenceState::Unknown,
            recovery_ready: EvidenceState::Fail,
            owner_gate_ready: EvidenceState::Unknown,
        });
        assert_eq!(evidence.evidence.provenance, EvidenceProvenance::Synthetic);
        assert_eq!(evidence.receipt.disposition, ReadinessDisposition::Blocked);
        assert!(!evidence.live_readiness_executed);
        assert!(!evidence.receipt.h1_mutation_allowed);
    }

    fn encode_base64(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = chunk.get(1).copied().unwrap_or(0);
            let c = chunk.get(2).copied().unwrap_or(0);
            encoded.push(char::from(TABLE[(a >> 2) as usize]));
            encoded.push(char::from(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize]));
            if chunk.len() > 1 {
                encoded.push(char::from(TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize]));
            } else {
                encoded.push('=');
            }
            if chunk.len() > 2 {
                encoded.push(char::from(TABLE[(c & 0x3f) as usize]));
            } else {
                encoded.push('=');
            }
        }
        encoded
    }
}
