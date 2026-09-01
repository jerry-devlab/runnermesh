//! Stable domain contracts for RunnerMesh.

mod admission;
mod agent;
#[cfg(windows)]
pub mod agent_runtime;
mod cli;
mod credential;
#[cfg(any(windows, test))]
mod github_transport;
mod h1_live;
mod host;
mod ipc;
mod model;
mod policy;
mod probe;
mod process_snapshot;
mod qualification;
mod runner_observer;
mod runtime;
mod supervisor;
mod tray;
#[cfg(windows)]
mod windows_github;
#[cfg(windows)]
pub mod windows_preferences;
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
pub use ipc::{
    IpcClient, IpcEndpoint, IpcError, IpcErrorCode, IpcRequest, IpcResponse, IpcResponseBody,
    IpcServer, IpcTransportError, IPC_PROTOCOL_VERSION,
};
pub use model::{NodeState, ParseUserModeError, UserMode};
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
pub use runner_observer::{
    ConnectionEvidence, ExecutionIdentityEvidence, OfficialRunnerObserver, OwnershipEvidence,
    RunnerLocalEvidence, RunnerObservation, RunnerSource, WindowsRunnerSource,
};
pub use runtime::{
    AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DoctorCheck, DoctorReport, DoctorStatus, EffectiveLocale, EffectiveTheme,
    EffectiveUiPreferences, HardSafetyState, LanguagePreference, LinkKind, LinkSnapshot, LinkState,
    ProbeHealth, ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase,
    SystemPreferences, ThemePreference, UiPreferences, ZenOverride,
};
pub use supervisor::{
    ProcessOwnership, SupervisorAction, SupervisorCore, SupervisorError, SupervisorObservation,
    SupervisorOutcome, SupervisorRefusal, SupervisorRequest, SyntheticProcessBackend,
};
pub use tray::{
    localized_menu_hint, NativeTrayEventLoop, TrayActionResult, TrayError, TrayHelpKey,
    TrayIconGlyph, TrayMenuEntry, TrayMenuId, TrayMenuItem, TrayRender, TrayUiUpdate,
};
#[cfg(windows)]
pub use windows_github::{
    windows_github_admission_backend, windows_github_repository_access_client,
    windows_github_workflow_client, WindowsGithubAdmissionBackend,
    WindowsGithubRepositoryAccessClient, WindowsGithubWorkflowClient,
};
#[cfg(windows)]
pub use windows_supervisor::{
    PreparedWindowsSupervisorAction, UserSessionLaunch, WindowsSupervisorReadiness,
    WindowsUserSessionSupervisorAdapter,
};
