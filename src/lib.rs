//! Stable domain contracts for RunnerMesh.

mod agent;
#[cfg(windows)]
pub mod agent_runtime;
mod cli;
mod host;
mod ipc;
mod model;
mod policy;
mod probe;
mod process_snapshot;
mod runner_observer;
mod runtime;
mod supervisor;
mod tray;
#[cfg(windows)]
pub mod windows_preferences;
#[cfg(windows)]
pub mod windows_supervisor;
#[cfg(windows)]
pub mod windows_tray_theme;

pub use agent::{
    AgentConfig, AgentCore, AgentCoreError, AgentObservation, AgentObserver, AgentReconciler,
    ConfigStore, ConfigStoreError, FileConfigStore, MemoryConfigStore, CONFIG_SCHEMA_VERSION,
};
pub use cli::{execute_cli, parse_cli, AgentTransport, CliCommand, CliError, LocalAgentTransport};
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
pub use windows_supervisor::{
    PreparedWindowsSupervisorAction, UserSessionLaunch, WindowsSupervisorReadiness,
    WindowsUserSessionSupervisorAdapter,
};
