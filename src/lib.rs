//! Stable domain contracts for RunnerMesh.

mod admission;
mod agent;
#[cfg(windows)]
pub mod agent_runtime;
mod cli;
mod credential;
#[cfg(any(windows, test))]
mod github_transport;
mod h1_artifact;
mod h1_live;
mod host;
mod hux2_synthetic;
mod hux_sandbox;
mod installation;
mod ipc;
mod lrm_integration;
mod model;
mod package;
mod policy;
mod probe;
mod process_snapshot;
mod qualification;
mod resource_policy_probe;
mod runner_commands;
mod runner_discovery;
mod runner_observer;
mod runner_reconcile;
mod runner_registry;
mod runner_resource;
mod runner_service_backend;
mod runner_session_backend;
mod runtime;
mod runtime_binding;
mod scoped_admission_sim;
mod service_control_contract;
mod supervisor;
mod tray;
mod update;
#[cfg(windows)]
mod windows_github;
#[cfg(windows)]
pub mod windows_preferences;
mod windows_runner_discovery;
mod windows_scm_executor;
#[cfg(windows)]
pub mod windows_supervisor;
#[cfg(windows)]
pub mod windows_tray_theme;

pub use admission::{
    AdmissionAgentReconciler, AdmissionBackendError, AdmissionBinding, AdmissionControlBackend,
    AdmissionControlSnapshot, AdmissionController, AdmissionLifecycleState, AdmissionSelectorState,
    CredentialLease, CredentialProvider, CredentialReference, DesiredAdmissionState,
    ExactRunnerIdentityState, GithubHttpRequest, GithubHttpResponse, GithubHttpTransport,
    GithubRestAdmissionBackend, GithubTransportError, HttpMethod, LocalAdmissionEvidence,
    RegistrationScope, RemoteAdmissionObservation, ReservedLabelOwnership,
    ReservedLabelOwnershipState, RetryDirective, RetryPolicy, RESERVED_ADMISSION_LABEL,
};
pub use agent::{
    AgentConfig, AgentCore, AgentCoreError, AgentObservation, AgentObserver, AgentReconciler,
    ConfigStore, ConfigStoreError, FileConfigStore, MemoryConfigStore, CONFIG_SCHEMA_VERSION,
};
pub use cli::{execute_cli, parse_cli, AgentTransport, CliCommand, CliError, LocalAgentTransport};
#[cfg(windows)]
pub(crate) use credential::WindowsCredentialManagerProvider;
pub use credential::{
    CredentialProviderAdapter, CredentialStore, CredentialStoreError,
    WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
};
#[cfg(windows)]
pub(crate) use github_transport::{
    GithubApiTransport, GithubWireClient, GithubWireError, GithubWireRequest, GithubWireResponse,
    GITHUB_API_HOST, GITHUB_API_USER_AGENT,
};
pub use h1_artifact::{assess_h1_artifacts, H1ArtifactAssessment};
pub use h1_live::{
    collect_h1_live_readiness, observe_github_admission_readiness, verify_h1_routing,
    verify_repository_runner_access, verify_trusted_workflow, ExactLocalBindingObservation,
    ExactLocalBindingSource, ExactLocalRunnerBinding, FilesystemExactLocalBindingSource,
    GithubAdmissionReadiness, H1LiveBinding, H1LiveReadinessCollection, H1LiveReadinessInputs,
    LocalIdentityOwnershipVerifier, OpaqueIdentityReference, RepositoryRunnerAccessObservation,
    RestoreReadinessBinding, RouteState, TrustedWorkflowBinding, TrustedWorkflowObservation,
    WorkflowPresence,
};
#[cfg(windows)]
pub(crate) use h1_live::{GithubRepositoryAccessClient, GithubWorkflowClient};
pub use host::{
    AdoptionRefusal, ExistingListenerAdoption, HostEvidence, HostHealth, HostSnapshot, HostSource,
    RecoveryDirective, RecoveryObservation, RecoveryReconstructor, RecoverySnapshot,
    RecoverySource, RecoveryTrigger, SessionState, WindowsHostSource,
};
pub use hux2_synthetic::{
    run_hux2_synthetic_failure_matrix, run_hux2_synthetic_happy_path, Hux2SyntheticReceipt,
    SyntheticDisposition,
};
pub use hux_sandbox::{
    assert_registry_intent_round_trip, prepare_two_runner_sandbox, HuxSandboxLayout,
};
#[cfg(windows)]
pub use installation::WindowsUserStartupBackend;
pub use installation::{
    AutostartBackend, AutostartChange, AutostartEntry, InstallError, InstallReceipt, Installation,
    InstallationState, SandboxAutostartBackend, UninstallReceipt, VersionManifest,
};
pub use ipc::{
    IpcClient, IpcEndpoint, IpcError, IpcErrorCode, IpcRequest, IpcResponse, IpcResponseBody,
    IpcServer, IpcTransportError, IPC_PROTOCOL_VERSION,
};
pub use lrm_integration::{
    run_lrm_synthetic_matrix, LrmOwnerPrepReceipt, LrmPrepDisposition, LrmSyntheticMatrixReport,
    LrmSyntheticMatrixRow, LRM_OWNER_PREP_SCHEMA_VERSION, LRM_SYNTHETIC_MATRIX_ID,
};
pub use model::{NodeState, ParseUserModeError, UserMode};
pub use package::{
    PackageDoctor, PackageDoctorReport, PackageError, PackageInput, PackageManifest,
    PackageProvenance, PackageReceipt, PackageVerifier, ProductizationCheck,
    ProductizationCheckStatus, WINDOWS_X64_TARGET,
};
pub use policy::decide_admission;
pub use probe::{
    ActivityWorkloadProbe, ProbeReadError, ProcessListProbe, ProcessSource, SteamAppIdSource,
    SteamGameProbe, UserActivityProbe, UserActivitySource, WindowsProcessSource,
    WindowsSteamAppIdSource, WindowsUserActivitySource,
};
pub use qualification::{
    assess_h1_workflow_source, assess_h1_workflow_template, h1_workflow_template,
    verify_h1_readiness, BaselineAdmissionState, EvidenceProvenance, EvidenceState,
    H1LiveReadinessAttestation, H1ReadinessEvidence, H1ReadinessReceipt, H1RestoreBaseline,
    H1TransactionCheckpoint, H1TransactionError, H1TransactionEvent, H1TransactionModel,
    H1TransactionPhase, H1TransactionReceipt, H1TransactionResumeAttestation,
    H1WorkflowTemplateAssessment, QualificationDisposition, ReadinessBlocker, ReadinessCheck,
    ReadinessDisposition, RestoreDisposition, H1_READINESS_SCHEMA_VERSION,
    H1_TRANSACTION_FAMILY_ID, H1_TRANSACTION_SCHEMA_VERSION,
};
pub use resource_policy_probe::{probe_resource_policy_capabilities, ResourcePolicyCapability};
pub use runner_commands::{
    handle_runners_command, DiscoveredCandidateView, RunnerCommandReport, RunnerPresentationStatus,
    RunnerRecordView,
};
pub use runner_discovery::{
    DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity, DiscoveredServiceBinding,
    DiscoveredSessionBinding, DiscoveryCapabilities, DiscoveryDisposition,
    DiscoveryEnumerationStatus, DiscoveryIdentityFilter, DiscoveryRefusalReason, DiscoveryReport,
    EmptyServiceDiscoverySource, EmptySessionDiscoverySource, EmptyUnsupportedDiscoverySource,
    RunnerBackendKind, RunnerDiscoveryInventory, ServiceCandidateFact, ServiceDiscoverySource,
    SessionCandidateFact, SessionDiscoverySource, SyntheticServiceDiscoverySource,
    SyntheticSessionDiscoverySource, SyntheticUnsupportedDiscoverySource, UnsupportedCandidateFact,
    UnsupportedDiscoverySource, RUNNER_DISCOVERY_SCHEMA_VERSION,
};
pub use runner_observer::{
    ConnectionEvidence, ExecutionIdentityEvidence, OfficialRunnerObserver, OwnershipEvidence,
    RunnerLocalEvidence, RunnerObservation, RunnerSource, WindowsRunnerSource,
};
pub use runner_reconcile::{
    mark_same_scope_collisions, reconcile_multi_runners, GlobalPolicyInputs,
    MultiRunnerReconcileReport, PerRunnerAchieved, PerRunnerDesired, PerRunnerObservation,
    PerRunnerPlan, PerRunnerReconcileResult,
};
pub use runner_registry::{
    EnrollmentState, FileRegistryStore, MemoryRegistryStore, RegistryBackendBinding, RegistryError,
    RegistryStore, RunnerRecord, RunnerRegistry, RunnerRegistryEditor,
    RUNNER_REGISTRY_SCHEMA_VERSION,
};
pub use runner_resource::{
    aggregate_runner_memory, format_resource_estimate, ResourceSampleConfidence,
    RunnerMemorySample, RunnerResourceAggregate, RunnerResourceRole,
};
pub use runner_service_backend::{
    PreparedServiceAction, ServiceBackendRefusal, ServiceObservation, ServicePrepareOutcome,
    ServiceStartMode, SyntheticWindowsServiceBackend,
};
pub use runner_session_backend::{
    PreparedSessionAction, SessionBackendRefusal, SessionObservation, SessionPrepareOutcome,
    SyntheticUserSessionBackend,
};
pub use runtime::{
    AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DoctorCheck, DoctorReport, DoctorStatus, EffectiveLocale, EffectiveTheme,
    EffectiveUiPreferences, HardSafetyState, LanguagePreference, LinkKind, LinkSnapshot, LinkState,
    ProbeHealth, ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase,
    SystemPreferences, ThemePreference, UiPreferences, ZenOverride,
};
#[cfg(windows)]
pub use runtime_binding::current_windows_identity_reference;
pub use runtime_binding::{
    InstalledRuntimeBinding, INSTALLED_RUNTIME_BINDING_SCHEMA_VERSION,
    WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
};
pub use scoped_admission_sim::{
    evaluate_v01_selector_compatibility, propose_scoped_selector_migration,
    ScopedSelectorMigrationPlan, SelectorCompatibility, SimulatedAdmissionBinding,
    PROPOSED_SCOPED_SELECTOR_PREFIX,
};
pub use service_control_contract::{
    achieved_from_observation, capture_restore_plan, mutation_rights, next_control_phase,
    next_control_phase_with_admission, read_only_rights, AchievedServiceControl,
    DesiredServiceLifecycle, ServiceControlPhase, ServiceControlRight, ServicePrestateCapture,
    ServiceRestorePlan,
};
pub use supervisor::{
    ProcessOwnership, SupervisorAction, SupervisorCore, SupervisorError, SupervisorObservation,
    SupervisorOutcome, SupervisorRefusal, SupervisorRequest, SyntheticProcessBackend,
};
pub use tray::{
    localized_menu_hint, NativeTrayEventLoop, TrayActionResult, TrayError, TrayHelpKey,
    TrayIconGlyph, TrayMenuEntry, TrayMenuId, TrayMenuItem, TrayRender, TrayUiUpdate,
};
pub use update::{
    payload_sha256, HealthObservation, ReconcileReceipt, SafePointObservation, UpdateCoordinator,
    UpdateError, UpdateOutcome, UpdatePhase, UpdateReceipt, UpdateRequest, UpdateTransaction,
};
#[cfg(windows)]
pub use windows_github::{
    windows_github_admission_backend, windows_github_repository_access_client,
    windows_github_workflow_client, WindowsGithubAdmissionBackend,
    WindowsGithubRepositoryAccessClient, WindowsGithubWorkflowClient,
};
pub use windows_runner_discovery::{
    discover_local_runners_read_only, host_service_discovery_source,
    ConfiguredSessionDiscoverySource, PrivacySafeDiscoverySummary, WindowsServiceDiscoverySource,
};
pub use windows_scm_executor::{
    opaque_image_token, AdmissionSelectorObserver, FakeGithubAdmissionBackend,
    GatedWindowsScmBackend, LiveScmExecutionGate, MockScmBackend, MockServiceRecord, ScmApiError,
    ScmControlBackend, ScmControlExecutor, ScmExactSnapshot, ScmTransactionReceipt,
    ScmTransactionStage, SyntheticScmTestGrant, TrustedOwnerAuthorizationRequest,
    TrustedOwnerAuthorizationVerifier, UnissuedOwnerAuthorizationVerifier, WindowsScmBackend,
};
#[cfg(windows)]
pub use windows_supervisor::{
    PreparedWindowsSupervisorAction, UserSessionLaunch, WindowsSupervisorReadiness,
    WindowsUserSessionSupervisorAdapter,
};
