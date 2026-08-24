use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize};

use crate::{NodeState, UserMode};

/// The observed lifecycle phase of the official runner.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunnerPhase {
    Stopped,
    Starting,
    Listening,
    Busy,
    DrainPending,
    Stopping,
    Unknown,
}

impl fmt::Display for RunnerPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Stopped => "STOPPED",
            Self::Starting => "STARTING",
            Self::Listening => "LISTENING",
            Self::Busy => "BUSY",
            Self::DrainPending => "DRAIN_PENDING",
            Self::Stopping => "STOPPING",
            Self::Unknown => "UNKNOWN",
        };

        formatter.write_str(name)
    }
}

/// A normalized, non-localized policy reason code.
///
/// Reason codes are lowercase kebab-case tokens so that every frontend can
/// present its own localized explanation without changing machine semantics.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ReasonCode(String);

impl ReasonCode {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        validate_kebab_token(&value).map(|()| Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ReasonCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|value| Self::new(value).map_err(de::Error::custom))
    }
}

/// A stable identifier for a normalized probe.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProbeId(String);

impl ProbeId {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        validate_kebab_token(&value).map(|()| Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProbeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProbeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|value| Self::new(value).map_err(de::Error::custom))
    }
}

fn validate_kebab_token(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("must not be empty");
    }

    let mut previous_was_hyphen = false;
    for character in value.chars() {
        if character == '-' {
            if previous_was_hyphen {
                return Err("must not contain adjacent hyphens");
            }
            previous_was_hyphen = true;
        } else if character.is_ascii_lowercase() || character.is_ascii_digit() {
            previous_was_hyphen = false;
        } else {
            return Err("must use lowercase ASCII letters, digits, and hyphens only");
        }
    }

    if value.starts_with('-') || value.ends_with('-') {
        return Err("must not start or end with a hyphen");
    }

    Ok(())
}

/// The Agent's admission result. This is an intent-level contract; it does not
/// itself mutate the official runner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdmissionDecision {
    pub allow_new_work: bool,
    pub desired_node_state: NodeState,
    pub reason_code: ReasonCode,
    pub drain_requested: bool,
}

/// The persistent human-exclusive override layered above [`UserMode`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZenOverride {
    Disabled,
    Enabled,
}

impl fmt::Display for ZenOverride {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Disabled => "disabled",
            Self::Enabled => "enabled",
        };

        formatter.write_str(name)
    }
}

/// Health of the Agent control plane, independent from runner connectivity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentHealth {
    Starting,
    Healthy,
    Degraded,
    Unhealthy,
}

impl fmt::Display for AgentHealth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Starting => "STARTING",
            Self::Healthy => "HEALTHY",
            Self::Degraded => "DEGRADED",
            Self::Unhealthy => "UNHEALTHY",
        };

        formatter.write_str(name)
    }
}

/// Connection kinds exposed by the v0.1 control plane.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    GithubActions,
}

impl fmt::Display for LinkKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("github-actions")
    }
}

/// Evidence-based state of a remote connection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LinkState {
    Connected,
    Connecting,
    Disconnected,
    Unknown,
    NotConfigured,
}

impl fmt::Display for LinkState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Connected => "CONNECTED",
            Self::Connecting => "CONNECTING",
            Self::Disconnected => "DISCONNECTED",
            Self::Unknown => "UNKNOWN",
            Self::NotConfigured => "NOT_CONFIGURED",
        };

        formatter.write_str(name)
    }
}

/// Typed connection evidence. Details are represented by machine reason codes,
/// never frontend strings or a generic `remote_connected` boolean.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkSnapshot {
    pub kind: LinkKind,
    pub state: LinkState,
    pub reason_code: Option<ReasonCode>,
}

/// Runtime state of a configured probe. Disabled configuration is represented
/// separately by [`ProbeSnapshot::enabled`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProbeRuntimeState {
    Active,
    Inactive,
    Unknown,
    Unavailable,
    Suspended,
}

impl fmt::Display for ProbeRuntimeState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Active => "ACTIVE",
            Self::Inactive => "INACTIVE",
            Self::Unknown => "UNKNOWN",
            Self::Unavailable => "UNAVAILABLE",
            Self::Suspended => "SUSPENDED",
        };

        formatter.write_str(name)
    }
}

/// Normalized probe evidence for policy, CLI, and Tray consumers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProbeSnapshot {
    pub id: ProbeId,
    pub enabled: bool,
    pub runtime_state: ProbeRuntimeState,
    pub reason_code: Option<ReasonCode>,
}

/// The presentation theme selected by the user.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

/// The presentation language selected by the user.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LanguagePreference {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en-US")]
    EnUs,
}

/// Preferences that affect presentation only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UiPreferences {
    pub theme: ThemePreference,
    pub language: LanguagePreference,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            language: LanguagePreference::System,
        }
    }
}

/// Immutable build identity rendered by frontends without consulting their own
/// version state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildProvenance {
    pub version: String,
    pub commit: String,
    pub channel: String,
    pub target: String,
}

/// The authoritative machine snapshot consumed by every frontend.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSnapshot {
    pub schema_version: u32,
    pub build: BuildProvenance,
    pub health: AgentHealth,
    pub zen: ZenOverride,
    pub user_mode: UserMode,
    pub node_state: NodeState,
    pub admission: AdmissionDecision,
    pub runner_phase: RunnerPhase,
    pub links: Vec<LinkSnapshot>,
    pub probes: Vec<ProbeSnapshot>,
    pub ui_preferences: UiPreferences,
}

/// A machine-readable doctor check result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorCheck {
    pub id: ReasonCode,
    pub status: DoctorStatus,
    pub reason_code: Option<ReasonCode>,
}

/// Outcome vocabulary for one doctor check.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DoctorStatus {
    Pass,
    Warn,
    Fail,
    Unknown,
}

impl fmt::Display for DoctorStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
            Self::Unknown => "UNKNOWN",
        };

        formatter.write_str(name)
    }
}

/// A presentation-independent diagnostic report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

/// Commands submitted by every frontend to the single Agent authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AgentCommand {
    GetSnapshot,
    RunDoctor,
    SetMode { mode: UserMode },
    SetZen { zen: ZenOverride },
    SetProbeEnabled { probe_id: ProbeId, enabled: bool },
    GetRunnerStatus,
    GetVersion,
}

/// Typed response from the Agent. Errors use stable machine reason codes so
/// that frontends retain presentation-only localization.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum AgentResponse {
    Snapshot(AgentSnapshot),
    Doctor(DoctorReport),
    Accepted { snapshot: AgentSnapshot },
    Rejected { reason_code: ReasonCode },
}

#[cfg(test)]
mod tests {
    use super::{
        AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot,
        BuildProvenance, DoctorCheck, DoctorReport, DoctorStatus, LanguagePreference, LinkKind,
        LinkSnapshot, LinkState, ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode,
        RunnerPhase, ThemePreference, UiPreferences, ZenOverride,
    };
    use crate::{NodeState, UserMode};

    #[test]
    fn runtime_enum_json_contracts_are_exact() {
        let runner_phase_cases = [
            (RunnerPhase::Stopped, "STOPPED"),
            (RunnerPhase::Starting, "STARTING"),
            (RunnerPhase::Listening, "LISTENING"),
            (RunnerPhase::Busy, "BUSY"),
            (RunnerPhase::DrainPending, "DRAIN_PENDING"),
            (RunnerPhase::Stopping, "STOPPING"),
            (RunnerPhase::Unknown, "UNKNOWN"),
        ];
        for (value, expected) in runner_phase_cases {
            assert_round_trips(value, expected);
        }

        for (value, expected) in [
            (ZenOverride::Disabled, "disabled"),
            (ZenOverride::Enabled, "enabled"),
        ] {
            assert_round_trips(value, expected);
        }

        for (value, expected) in [
            (AgentHealth::Starting, "STARTING"),
            (AgentHealth::Healthy, "HEALTHY"),
            (AgentHealth::Degraded, "DEGRADED"),
            (AgentHealth::Unhealthy, "UNHEALTHY"),
        ] {
            assert_round_trips(value, expected);
        }

        assert_round_trips(LinkKind::GithubActions, "github-actions");
        assert_round_trips(LinkState::Connected, "CONNECTED");
        assert_round_trips(LinkState::Connecting, "CONNECTING");
        assert_round_trips(LinkState::Disconnected, "DISCONNECTED");
        assert_round_trips(LinkState::Unknown, "UNKNOWN");
        assert_round_trips(LinkState::NotConfigured, "NOT_CONFIGURED");
        assert_round_trips(ProbeRuntimeState::Active, "ACTIVE");
        assert_round_trips(ProbeRuntimeState::Inactive, "INACTIVE");
        assert_round_trips(ProbeRuntimeState::Unknown, "UNKNOWN");
        assert_round_trips(ProbeRuntimeState::Unavailable, "UNAVAILABLE");
        assert_round_trips(ProbeRuntimeState::Suspended, "SUSPENDED");
        assert_round_trips(ThemePreference::System, "system");
        assert_round_trips(ThemePreference::Light, "light");
        assert_round_trips(ThemePreference::Dark, "dark");
        assert_round_trips(LanguagePreference::System, "system");
        assert_round_trips(LanguagePreference::ZhCn, "zh-CN");
        assert_round_trips(LanguagePreference::EnUs, "en-US");
        assert_round_trips(DoctorStatus::Pass, "PASS");
        assert_round_trips(DoctorStatus::Warn, "WARN");
        assert_round_trips(DoctorStatus::Fail, "FAIL");
        assert_round_trips(DoctorStatus::Unknown, "UNKNOWN");
    }

    #[test]
    fn invalid_runtime_enum_spellings_are_rejected() {
        for invalid in ["listening", "DRAIN-PENDING", "WAITING"] {
            assert!(serde_json::from_str::<RunnerPhase>(&format!("\"{invalid}\"")).is_err());
        }
        for invalid in ["active", "UNAVAILABLE ", "disabled"] {
            assert!(serde_json::from_str::<ProbeRuntimeState>(&format!("\"{invalid}\"")).is_err());
        }
        for invalid in ["zh-cn", "en-us", "English"] {
            assert!(serde_json::from_str::<LanguagePreference>(&format!("\"{invalid}\"")).is_err());
        }
    }

    #[test]
    fn reason_codes_and_probe_ids_accept_only_machine_tokens() {
        for valid in ["auto-idle-permits", "steam-app-42", "g02"] {
            assert_eq!(ReasonCode::new(valid).unwrap().as_str(), valid);
            assert_eq!(ProbeId::new(valid).unwrap().as_str(), valid);
        }

        for invalid in [
            "",
            "Auto",
            "with_space",
            "-start",
            "end-",
            "two--hyphens",
            "中文",
        ] {
            assert!(ReasonCode::new(invalid).is_err(), "{invalid}");
            assert!(ProbeId::new(invalid).is_err(), "{invalid}");
            assert!(serde_json::from_str::<ReasonCode>(&format!("\"{invalid}\"")).is_err());
            assert!(serde_json::from_str::<ProbeId>(&format!("\"{invalid}\"")).is_err());
        }
    }

    #[test]
    fn snapshot_command_and_response_round_trip_without_presentation_strings() {
        let snapshot = sample_snapshot();
        let response = AgentResponse::Accepted {
            snapshot: snapshot.clone(),
        };
        let encoded = serde_json::to_value(&response).unwrap();

        assert_eq!(encoded["type"], "accepted");
        assert_eq!(encoded["payload"]["snapshot"]["user_mode"], "auto");
        assert_eq!(encoded["payload"]["snapshot"]["node_state"], "DRAINED");
        assert_eq!(
            encoded["payload"]["snapshot"]["probes"][0]["runtime_state"],
            "UNKNOWN"
        );
        assert_eq!(
            serde_json::from_value::<AgentResponse>(encoded).unwrap(),
            response
        );

        let command = AgentCommand::SetProbeEnabled {
            probe_id: ProbeId::new("steam-game").unwrap(),
            enabled: false,
        };
        assert_eq!(
            serde_json::to_string(&command).unwrap(),
            "{\"type\":\"set-probe-enabled\",\"probe_id\":\"steam-game\",\"enabled\":false}"
        );
        assert_eq!(
            serde_json::from_str::<AgentCommand>("{\"type\":\"set-mode\",\"mode\":\"force-ci\"}")
                .unwrap(),
            AgentCommand::SetMode {
                mode: UserMode::ForceCi
            }
        );
        assert!(serde_json::from_str::<AgentCommand>(
            "{\"type\":\"set-mode\",\"mode\":\"ForceCi\"}"
        )
        .is_err());
    }

    #[test]
    fn doctor_response_round_trips() {
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                id: ReasonCode::new("agent-state").unwrap(),
                status: DoctorStatus::Pass,
                reason_code: None,
            }],
        };
        let response = AgentResponse::Doctor(report);
        assert_eq!(
            serde_json::from_str::<AgentResponse>(&serde_json::to_string(&response).unwrap())
                .unwrap(),
            response
        );
    }

    fn sample_snapshot() -> AgentSnapshot {
        AgentSnapshot {
            schema_version: 1,
            build: BuildProvenance {
                version: "0.1.0-dev".to_owned(),
                commit: "0123456789abcdef".to_owned(),
                channel: "dev".to_owned(),
                target: "x86_64-pc-windows-msvc".to_owned(),
            },
            health: AgentHealth::Healthy,
            zen: ZenOverride::Disabled,
            user_mode: UserMode::Auto,
            node_state: NodeState::Drained,
            admission: AdmissionDecision {
                allow_new_work: false,
                desired_node_state: NodeState::Drained,
                reason_code: ReasonCode::new("awaiting-auto-evidence").unwrap(),
                drain_requested: false,
            },
            runner_phase: RunnerPhase::Unknown,
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: Some(ReasonCode::new("insufficient-link-evidence").unwrap()),
            }],
            probes: vec![ProbeSnapshot {
                id: ProbeId::new("user-activity").unwrap(),
                enabled: true,
                runtime_state: ProbeRuntimeState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            ui_preferences: UiPreferences::default(),
        }
    }

    fn assert_round_trips<T>(value: T, expected: &str)
    where
        T: std::fmt::Debug + serde::de::DeserializeOwned + Eq + serde::Serialize,
    {
        let json = format!("\"{expected}\"");
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        assert_eq!(serde_json::from_str::<T>(&json).unwrap(), value);
    }
}
