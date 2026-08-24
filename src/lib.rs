//! Stable domain contracts for RunnerMesh.

mod agent;
mod cli;
mod ipc;
mod model;
mod policy;
mod probe;
mod runtime;
mod tray;

pub use agent::{
    AgentConfig, AgentCore, AgentCoreError, AgentObservation, AgentObserver, AgentReconciler,
    ConfigStore, ConfigStoreError, FileConfigStore, MemoryConfigStore, CONFIG_SCHEMA_VERSION,
};
pub use cli::{execute_cli, parse_cli, AgentTransport, CliCommand, CliError, LocalAgentTransport};
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
pub use runtime::{
    AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DoctorCheck, DoctorReport, DoctorStatus, HardSafetyState, LanguagePreference, LinkKind,
    LinkSnapshot, LinkState, ProbeHealth, ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode,
    RunnerPhase, ThemePreference, UiPreferences, ZenOverride,
};
pub use tray::{
    NativeTrayEventLoop, TrayActionResult, TrayError, TrayIconGlyph, TrayMenuEntry, TrayMenuId,
    TrayMenuItem, TrayRender, TrayUiUpdate,
};
