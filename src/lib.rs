//! Stable domain contracts for RunnerMesh.

mod agent;
mod cli;
mod ipc;
mod model;
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
pub use runtime::{
    AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DoctorCheck, DoctorReport, DoctorStatus, LanguagePreference, LinkKind, LinkSnapshot, LinkState,
    ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase, ThemePreference,
    UiPreferences, ZenOverride,
};
pub use tray::{
    NativeTrayEventLoop, TrayActionResult, TrayError, TrayIconGlyph, TrayMenuEntry, TrayMenuId,
    TrayMenuItem, TrayRender, TrayUiUpdate,
};
