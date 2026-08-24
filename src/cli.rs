use std::{fmt, time::Duration};

use crate::{
    AgentCommand, AgentResponse, AgentSnapshot, DoctorReport, IpcClient, IpcEndpoint, IpcRequest,
    IpcResponse, IpcResponseBody, IpcTransportError, LinkKind, LinkState, ProbeId, UserMode,
    ZenOverride, IPC_PROTOCOL_VERSION,
};

/// Parsed first-release command surface. It intentionally mirrors
/// [`AgentCommand`] instead of owning operational state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliCommand {
    Status { json: bool },
    Doctor { json: bool },
    Mode { mode: UserMode },
    Zen { zen: ZenOverride },
    ProbeList,
    ProbeEnabled { probe_id: ProbeId, enabled: bool },
    RunnerStatus,
    Version,
}

/// Parses arguments after the executable name into the stable G05 command set.
pub fn parse_cli(arguments: &[String]) -> Result<CliCommand, CliError> {
    match arguments {
        [command] if command == "status" => Ok(CliCommand::Status { json: false }),
        [command, format] if command == "status" && format == "--json" => {
            Ok(CliCommand::Status { json: true })
        }
        [command] if command == "doctor" => Ok(CliCommand::Doctor { json: false }),
        [command, format] if command == "doctor" && format == "--json" => {
            Ok(CliCommand::Doctor { json: true })
        }
        [command, mode] if command == "mode" => Ok(CliCommand::Mode {
            mode: mode.parse().map_err(CliError::InvalidMode)?,
        }),
        [command, state] if command == "zen" && state == "on" => Ok(CliCommand::Zen {
            zen: ZenOverride::Enabled,
        }),
        [command, state] if command == "zen" && state == "off" => Ok(CliCommand::Zen {
            zen: ZenOverride::Disabled,
        }),
        [command, action, probe_id] if command == "probe" && action == "enable" => {
            Ok(CliCommand::ProbeEnabled {
                probe_id: ProbeId::new(probe_id.clone()).map_err(CliError::InvalidProbeId)?,
                enabled: true,
            })
        }
        [command, action, probe_id] if command == "probe" && action == "disable" => {
            Ok(CliCommand::ProbeEnabled {
                probe_id: ProbeId::new(probe_id.clone()).map_err(CliError::InvalidProbeId)?,
                enabled: false,
            })
        }
        [command, action] if command == "probe" && action == "list" => Ok(CliCommand::ProbeList),
        [command, action] if command == "runner" && action == "status" => {
            Ok(CliCommand::RunnerStatus)
        }
        [command] if command == "version" => Ok(CliCommand::Version),
        _ => Err(CliError::Usage),
    }
}

/// The transport boundary used by CLI tests and the real local IPC client.
pub trait AgentTransport {
    fn call(&self, request: IpcRequest) -> Result<IpcResponse, IpcTransportError>;
}

/// The normal local CLI transport. Resolving the endpoint lazily ensures that
/// `runnermesh version` works without requiring an Agent or Windows IPC.
#[derive(Clone, Debug)]
pub struct LocalAgentTransport {
    timeout: Duration,
}

impl LocalAgentTransport {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl AgentTransport for LocalAgentTransport {
    fn call(&self, request: IpcRequest) -> Result<IpcResponse, IpcTransportError> {
        let endpoint = IpcEndpoint::for_current_user()?;
        IpcClient::new(endpoint, self.timeout).call(request)
    }
}

/// Executes a parsed command by rendering authoritative Agent responses. The
/// CLI never derives policy, runner, or probe state itself.
pub fn execute_cli(
    arguments: &[String],
    transport: &impl AgentTransport,
    version: &str,
) -> Result<String, CliError> {
    match parse_cli(arguments)? {
        CliCommand::Status { json } => {
            let snapshot = request_snapshot(transport, AgentCommand::GetSnapshot)?;
            if json {
                render_json(&snapshot)
            } else {
                Ok(render_snapshot_text(&snapshot))
            }
        }
        CliCommand::Doctor { json } => {
            let report = request_doctor(transport)?;
            if json {
                render_json(&report)
            } else {
                Ok(render_doctor_text(&report))
            }
        }
        CliCommand::Mode { mode } => {
            let snapshot = request_snapshot(transport, AgentCommand::SetMode { mode })?;
            Ok(render_snapshot_text(&snapshot))
        }
        CliCommand::Zen { zen } => {
            let snapshot = request_snapshot(transport, AgentCommand::SetZen { zen })?;
            Ok(render_snapshot_text(&snapshot))
        }
        CliCommand::ProbeList => {
            let snapshot = request_snapshot(transport, AgentCommand::GetSnapshot)?;
            Ok(render_probe_list(&snapshot))
        }
        CliCommand::ProbeEnabled { probe_id, enabled } => {
            let snapshot = request_snapshot(
                transport,
                AgentCommand::SetProbeEnabled { probe_id, enabled },
            )?;
            Ok(render_probe_list(&snapshot))
        }
        CliCommand::RunnerStatus => {
            let snapshot = request_snapshot(transport, AgentCommand::GetRunnerStatus)?;
            Ok(render_runner_status(&snapshot))
        }
        CliCommand::Version => Ok(format!("runnermesh {version}")),
    }
}

/// CLI failures are either input validation, typed Agent rejections, or a
/// local transport failure. No failure is presented as invented runner state.
#[derive(Debug)]
pub enum CliError {
    Usage,
    InvalidMode(crate::ParseUserModeError),
    InvalidProbeId(&'static str),
    Transport(IpcTransportError),
    AgentRejected(crate::ReasonCode),
    ResponseCorrelation { expected: u64, received: u64 },
    UnexpectedResponse(&'static str),
    Json(serde_json::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str(
                "usage: status [--json] | doctor [--json] | mode <mode> | zen <on|off> | probe <list|enable|disable> | runner status | version",
            ),
            Self::InvalidMode(error) => write!(formatter, "invalid mode: {error}"),
            Self::InvalidProbeId(error) => write!(formatter, "invalid probe ID: {error}"),
            Self::Transport(error) => write!(formatter, "Agent IPC unavailable: {error}"),
            Self::AgentRejected(reason_code) => write!(formatter, "Agent rejected command: {reason_code}"),
            Self::ResponseCorrelation { expected, received } => {
                write!(formatter, "Agent response {received} did not match request {expected}")
            }
            Self::UnexpectedResponse(expected) => write!(formatter, "Agent returned {expected}"),
            Self::Json(error) => write!(formatter, "could not render JSON: {error}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidMode(error) => Some(error),
            Self::Transport(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Usage
            | Self::InvalidProbeId(_)
            | Self::AgentRejected(_)
            | Self::ResponseCorrelation { .. }
            | Self::UnexpectedResponse(_) => None,
        }
    }
}

fn request_snapshot(
    transport: &impl AgentTransport,
    command: AgentCommand,
) -> Result<AgentSnapshot, CliError> {
    let response = send(transport, command)?;
    match response {
        AgentResponse::Snapshot(snapshot) | AgentResponse::Accepted { snapshot } => Ok(snapshot),
        AgentResponse::Rejected { reason_code } => Err(CliError::AgentRejected(reason_code)),
        AgentResponse::Doctor(_) => Err(CliError::UnexpectedResponse(
            "a doctor response for a snapshot command",
        )),
    }
}

fn request_doctor(transport: &impl AgentTransport) -> Result<DoctorReport, CliError> {
    let response = send(transport, AgentCommand::RunDoctor)?;
    match response {
        AgentResponse::Doctor(report) => Ok(report),
        AgentResponse::Rejected { reason_code } => Err(CliError::AgentRejected(reason_code)),
        AgentResponse::Snapshot(_) | AgentResponse::Accepted { .. } => Err(
            CliError::UnexpectedResponse("a snapshot response for a doctor command"),
        ),
    }
}

fn send(transport: &impl AgentTransport, command: AgentCommand) -> Result<AgentResponse, CliError> {
    const REQUEST_ID: u64 = 1;
    let response = transport
        .call(IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: REQUEST_ID,
            command,
        })
        .map_err(CliError::Transport)?;
    if response.request_id != REQUEST_ID {
        return Err(CliError::ResponseCorrelation {
            expected: REQUEST_ID,
            received: response.request_id,
        });
    }
    match response.body {
        IpcResponseBody::Success(response) => Ok(response),
        IpcResponseBody::Failure(error) => Err(CliError::AgentRejected(error.reason_code)),
    }
}

fn render_json<T: serde::Serialize>(value: &T) -> Result<String, CliError> {
    serde_json::to_string_pretty(value).map_err(CliError::Json)
}

fn render_snapshot_text(snapshot: &AgentSnapshot) -> String {
    let github = snapshot
        .links
        .iter()
        .find(|link| link.kind == LinkKind::GithubActions)
        .map(|link| link.state)
        .unwrap_or(LinkState::Unknown);
    format!(
        "Agent: {}\nCapacity: {} · {}\nRunner: {}\nGitHub: {}\nZen: {}",
        snapshot.health,
        snapshot.node_state,
        snapshot.user_mode,
        snapshot.runner_phase,
        github,
        snapshot.zen
    )
}

fn render_doctor_text(report: &DoctorReport) -> String {
    let mut output = String::from("Doctor:");
    for check in &report.checks {
        output.push_str(&format!("\n- {}: {}", check.id, check.status));
        if let Some(reason_code) = &check.reason_code {
            output.push_str(&format!(" ({reason_code})"));
        }
    }
    output
}

fn render_probe_list(snapshot: &AgentSnapshot) -> String {
    let mut output = String::from("Probes:");
    for probe in &snapshot.probes {
        let enabled = if probe.enabled { "enabled" } else { "disabled" };
        output.push_str(&format!(
            "\n- {}: {enabled}, {}",
            probe.id, probe.runtime_state
        ));
    }
    output
}

fn render_runner_status(snapshot: &AgentSnapshot) -> String {
    format!(
        "Runner: {}\nCapacity: {}\nAdmission: {}",
        snapshot.runner_phase, snapshot.node_state, snapshot.admission.reason_code
    )
}

#[cfg(test)]
mod tests {
    use super::{execute_cli, parse_cli, AgentTransport, CliCommand};
    use crate::{
        AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot,
        BuildProvenance, DoctorCheck, DoctorReport, DoctorStatus, IpcRequest, IpcResponse,
        IpcResponseBody, IpcTransportError, LinkKind, LinkSnapshot, LinkState, NodeState, ProbeId,
        ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase, UiPreferences, UserMode,
        ZenOverride, IPC_PROTOCOL_VERSION,
    };

    #[test]
    fn parser_maps_the_required_commands_to_stable_vocabulary() {
        assert_eq!(
            parse_cli(&arguments(["mode", "force-ci"])).unwrap(),
            CliCommand::Mode {
                mode: UserMode::ForceCi
            }
        );
        assert_eq!(
            parse_cli(&arguments(["zen", "on"])).unwrap(),
            CliCommand::Zen {
                zen: ZenOverride::Enabled
            }
        );
        assert_eq!(
            parse_cli(&arguments(["probe", "disable", "steam-game"])).unwrap(),
            CliCommand::ProbeEnabled {
                probe_id: ProbeId::new("steam-game").unwrap(),
                enabled: false,
            }
        );
        assert!(parse_cli(&arguments(["mode", "ForceCi"])).is_err());
        assert!(parse_cli(&arguments(["status", "--pretty"])).is_err());
    }

    #[test]
    fn status_and_doctor_json_render_authoritative_machine_contracts() {
        let status_transport = FakeTransport::with_response(AgentResponse::Snapshot(snapshot()));
        let output =
            execute_cli(&arguments(["status", "--json"]), &status_transport, "0.0.0").unwrap();
        assert_eq!(
            serde_json::from_str::<AgentSnapshot>(&output).unwrap(),
            snapshot()
        );
        assert_eq!(status_transport.requests(), vec![AgentCommand::GetSnapshot]);

        let report = DoctorReport {
            checks: vec![DoctorCheck {
                id: ReasonCode::new("agent-health").unwrap(),
                status: DoctorStatus::Pass,
                reason_code: None,
            }],
        };
        let doctor_transport = FakeTransport::with_response(AgentResponse::Doctor(report.clone()));
        let output =
            execute_cli(&arguments(["doctor", "--json"]), &doctor_transport, "0.0.0").unwrap();
        assert_eq!(
            serde_json::from_str::<DoctorReport>(&output).unwrap(),
            report
        );
        assert_eq!(doctor_transport.requests(), vec![AgentCommand::RunDoctor]);
    }

    #[test]
    fn controls_use_agent_commands_and_do_not_claim_runner_control() {
        let mode_transport = FakeTransport::with_response(AgentResponse::Accepted {
            snapshot: snapshot(),
        });
        execute_cli(&arguments(["mode", "work"]), &mode_transport, "0.0.0").unwrap();
        assert_eq!(
            mode_transport.requests(),
            vec![AgentCommand::SetMode {
                mode: UserMode::Work
            }]
        );

        let runner_transport = FakeTransport::with_response(AgentResponse::Snapshot(snapshot()));
        let output =
            execute_cli(&arguments(["runner", "status"]), &runner_transport, "0.0.0").unwrap();
        assert!(output.contains("Runner: UNKNOWN"));
        assert_eq!(
            runner_transport.requests(),
            vec![AgentCommand::GetRunnerStatus]
        );

        let version_transport = FakeTransport::with_response(AgentResponse::Snapshot(snapshot()));
        assert_eq!(
            execute_cli(&arguments(["version"]), &version_transport, "0.1.0-dev").unwrap(),
            "runnermesh 0.1.0-dev"
        );
        assert!(version_transport.requests().is_empty());
    }

    #[derive(Clone)]
    struct FakeTransport {
        response: AgentResponse,
        requests: std::cell::RefCell<Vec<AgentCommand>>,
    }

    impl FakeTransport {
        fn with_response(response: AgentResponse) -> Self {
            Self {
                response,
                requests: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<AgentCommand> {
            self.requests.borrow().clone()
        }
    }

    impl AgentTransport for FakeTransport {
        fn call(&self, request: IpcRequest) -> Result<IpcResponse, IpcTransportError> {
            self.requests.borrow_mut().push(request.command);
            Ok(IpcResponse {
                protocol_version: IPC_PROTOCOL_VERSION,
                request_id: request.request_id,
                body: IpcResponseBody::Success(self.response.clone()),
            })
        }
    }

    fn arguments(values: impl IntoIterator<Item = &'static str>) -> Vec<String> {
        values.into_iter().map(str::to_owned).collect()
    }

    fn snapshot() -> AgentSnapshot {
        AgentSnapshot {
            schema_version: 1,
            build: BuildProvenance {
                version: "0.1.0-dev".to_owned(),
                commit: "0123456789abcdef".to_owned(),
                channel: "dev".to_owned(),
                target: "synthetic".to_owned(),
            },
            health: AgentHealth::Healthy,
            zen: ZenOverride::Disabled,
            user_mode: UserMode::Auto,
            node_state: NodeState::Drained,
            admission: AdmissionDecision {
                allow_new_work: false,
                desired_node_state: NodeState::Drained,
                reason_code: ReasonCode::new("awaiting-auto-policy").unwrap(),
                drain_requested: true,
            },
            runner_phase: RunnerPhase::Unknown,
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            probes: vec![ProbeSnapshot {
                id: ProbeId::new("steam-game").unwrap(),
                enabled: true,
                runtime_state: ProbeRuntimeState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            ui_preferences: UiPreferences::default(),
        }
    }
}
