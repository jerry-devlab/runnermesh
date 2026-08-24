//! Stable domain contracts for RunnerMesh.

mod agent;
mod ipc;
mod model;
mod runtime;

pub use agent::{
    AgentConfig, AgentCore, AgentCoreError, AgentObservation, AgentObserver, AgentReconciler,
    ConfigStore, ConfigStoreError, FileConfigStore, MemoryConfigStore, CONFIG_SCHEMA_VERSION,
};
pub use ipc::{
    IpcClient, IpcEndpoint, IpcError, IpcErrorCode, IpcRequest, IpcResponse, IpcResponseBody,
    IpcServer, IpcTransportError, IPC_PROTOCOL_VERSION,
};
pub use model::{NodeState, UserMode};
pub use runtime::{
    AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DoctorCheck, DoctorReport, DoctorStatus, LanguagePreference, LinkKind, LinkSnapshot, LinkState,
    ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase, ThemePreference,
    UiPreferences, ZenOverride,
};
