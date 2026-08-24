//! Stable domain contracts for RunnerMesh.

mod model;
mod runtime;

pub use model::{NodeState, UserMode};
pub use runtime::{
    AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DoctorCheck, DoctorReport, DoctorStatus, LanguagePreference, LinkKind, LinkSnapshot, LinkState,
    ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase, ThemePreference,
    UiPreferences, ZenOverride,
};
