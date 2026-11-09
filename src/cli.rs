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
    Runners { command: AgentCommand, json: bool },
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
        args if args.first().map(String::as_str) == Some("runners") => parse_runners_cli(args),
        _ => Err(CliError::Usage),
    }
}

fn parse_runners_cli(arguments: &[String]) -> Result<CliCommand, CliError> {
    let json = arguments.iter().any(|argument| argument == "--json");
    let prepare_only = arguments.iter().any(|argument| argument == "--prepare");
    let when_idle = arguments.iter().any(|argument| argument == "--when-idle");
    let positional: Vec<&str> = arguments
        .iter()
        .map(String::as_str)
        .filter(|argument| !matches!(*argument, "--json" | "--prepare" | "--when-idle"))
        .collect();
    let command = match positional.as_slice() {
        ["runners", "list"] => AgentCommand::RunnersList,
        ["runners", "discover"] => AgentCommand::RunnersDiscover,
        ["runners", "inspect", key] => AgentCommand::RunnersInspect {
            local_runner_key: (*key).to_owned(),
        },
        ["runners", "enroll", key] => AgentCommand::RunnersEnroll {
            local_runner_key: (*key).to_owned(),
            prepare_only,
        },
        ["runners", "enable", key] => AgentCommand::RunnersEnable {
            local_runner_key: (*key).to_owned(),
            prepare_only,
        },
        ["runners", "disable", key] => AgentCommand::RunnersDisable {
            local_runner_key: (*key).to_owned(),
            when_idle,
            prepare_only,
        },
        ["runners", "reconcile", key] => AgentCommand::RunnersReconcile {
            local_runner_key: (*key).to_owned(),
            prepare_only,
        },
        ["runners", "forget", key] => AgentCommand::RunnersForget {
            local_runner_key: (*key).to_owned(),
            prepare_only,
        },
        ["runners", "status"] => AgentCommand::RunnersStatus {
            local_runner_key: None,
        },
        ["runners", "status", key] => AgentCommand::RunnersStatus {
            local_runner_key: Some((*key).to_owned()),
        },
        _ => return Err(CliError::Usage),
    };
    Ok(CliCommand::Runners { command, json })
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
        CliCommand::Runners { command, json } => {
            let report = request_runners(transport, command)?;
            if json {
                render_json(&report)
            } else {
                Ok(render_runners_text(&report))
            }
        }
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
                "usage: status [--json] | doctor [--json] | mode <mode> | zen <on|off> | probe <list|enable|disable> | runner status | runners <list|discover|inspect|enroll|enable|disable|reconcile|forget|status> | version",
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
        AgentResponse::Doctor(_) | AgentResponse::Runners(_) => Err(CliError::UnexpectedResponse(
            "a doctor/runners response for a snapshot command",
        )),
    }
}

fn request_runners(
    transport: &impl AgentTransport,
    command: AgentCommand,
) -> Result<crate::RunnerCommandReport, CliError> {
    match send(transport, command)? {
        AgentResponse::Runners(report) => Ok(report),
        AgentResponse::Rejected { reason_code } => Err(CliError::AgentRejected(reason_code)),
        AgentResponse::Snapshot(_) | AgentResponse::Accepted { .. } | AgentResponse::Doctor(_) => {
            Err(CliError::UnexpectedResponse(
                "a non-runners response for a runners command",
            ))
        }
    }
}

fn request_doctor(transport: &impl AgentTransport) -> Result<DoctorReport, CliError> {
    let response = send(transport, AgentCommand::RunDoctor)?;
    match response {
        AgentResponse::Doctor(report) => Ok(report),
        AgentResponse::Rejected { reason_code } => Err(CliError::AgentRejected(reason_code)),
        AgentResponse::Snapshot(_) | AgentResponse::Accepted { .. } | AgentResponse::Runners(_) => {
            Err(CliError::UnexpectedResponse(
                "a snapshot/runners response for a doctor command",
            ))
        }
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
        IpcResponseBody::Success(response) => Ok(*response),
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
    let achieved = snapshot
        .achieved_node_state
        .map(|state| state.to_string())
        .unwrap_or_else(|| "TRANSITIONAL".to_owned());
    let admission_reason = snapshot
        .admission_control
        .reason_code
        .as_ref()
        .map(|reason| reason.as_str())
        .unwrap_or("none");
    format!(
        "Agent: {}\nDesired capacity: {} · {}\nAchieved capacity: {}\nAdmission control: {} · {} · {}\nRunner: {}\nGitHub: {}\nZen: {}",
        snapshot.health,
        snapshot.desired_node_state,
        snapshot.user_mode,
        achieved,
        snapshot.admission_control.lifecycle,
        admission_reason,
        if snapshot.admission_control.active_bound_worker {
            "worker-active"
        } else {
            "worker-absent"
        },
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
    let achieved = snapshot
        .achieved_node_state
        .map(|state| state.to_string())
        .unwrap_or_else(|| "TRANSITIONAL".to_owned());
    let reason = snapshot
        .admission_control
        .reason_code
        .as_ref()
        .map(|reason| reason.as_str())
        .unwrap_or("none");
    format!(
        "Runner: {}\nDesired capacity: {}\nAchieved capacity: {}\nAdmission control: {} ({})\nPolicy reason: {}",
        snapshot.runner_phase,
        snapshot.desired_node_state,
        achieved,
        snapshot.admission_control.lifecycle,
        reason,
        snapshot.admission.reason_code
    )
}

fn render_runners_text(report: &crate::RunnerCommandReport) -> String {
    let mut lines = vec![format!(
        "runners read_only={} prepare_only={} applied={} discovered={} collisions={} partial_failure={}",
        report.read_only,
        report.prepare_only,
        report.applied,
        report.discovered_keys.len(),
        report.same_scope_collisions,
        report.partial_failure
    )];
    if let Some(reason) = &report.reason_code {
        lines.push(format!("reason: {reason}"));
    }
    if let Some(aggregate) = &report.resource_aggregate {
        lines.push(crate::format_resource_estimate(aggregate, false));
    }
    for record in &report.records {
        let item = match record.item_applied {
            Some(true) => "applied",
            Some(false) => "failed",
            None => "pending",
        };
        lines.push(format!(
            "- {} id={} enabled={} presentation={:?} state={:?} backend={} item={}",
            record.local_runner_key,
            record.runner_id,
            record.desired_enabled,
            record.presentation_status,
            record.enrollment_state,
            record.backend_kind,
            item
        ));
        if let Some(reason) = &record.item_reason_code {
            lines.push(format!("  item_reason: {reason}"));
        }
    }
    for key in &report.discovered_keys {
        lines.push(format!("discovered: {key}"));
    }
    for candidate in &report.discovered_candidates {
        let refusal = candidate.refusal_reason.as_deref().unwrap_or("none");
        lines.push(format!(
            "candidate: {} backend={} disposition={} eligible={} collision={} refusal={}",
            candidate.local_discovery_key,
            candidate.backend_kind,
            candidate.disposition,
            candidate.enrollment_eligible,
            candidate.same_scope_collision,
            refusal
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        execute_cli, parse_cli, render_doctor_text, render_runner_status, render_runners_text,
        render_snapshot_text, AgentTransport, CliCommand,
    };
    use crate::{
        AdmissionControlSnapshot, AdmissionDecision, AdmissionLifecycleState,
        AdmissionSelectorState, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot,
        BuildProvenance, DesiredAdmissionState, DoctorCheck, DoctorReport, DoctorStatus,
        ExactRunnerIdentityState, IpcRequest, IpcResponse, IpcResponseBody, IpcTransportError,
        LinkKind, LinkSnapshot, LinkState, NodeState, ProbeId, ProbeRuntimeState, ProbeSnapshot,
        ReasonCode, ReservedLabelOwnershipState, RunnerPhase, UiPreferences, UserMode, ZenOverride,
        IPC_PROTOCOL_VERSION,
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
        assert_eq!(
            parse_cli(&arguments(["runners", "list", "--json"])).unwrap(),
            CliCommand::Runners {
                command: AgentCommand::RunnersList,
                json: true,
            }
        );
        assert_eq!(
            parse_cli(&arguments([
                "runners",
                "disable",
                "key-a",
                "--when-idle",
                "--prepare"
            ]))
            .unwrap(),
            CliCommand::Runners {
                command: AgentCommand::RunnersDisable {
                    local_runner_key: "key-a".to_owned(),
                    when_idle: true,
                    prepare_only: true,
                },
                json: false,
            }
        );
    }

    #[test]
    fn text_status_explains_desired_withdrawal_blocked_and_unachieved_state() {
        let mut snapshot = snapshot();
        snapshot.admission_control = AdmissionControlSnapshot {
            desired: DesiredAdmissionState::Drained,
            lifecycle: AdmissionLifecycleState::WithdrawalBlocked,
            selector: AdmissionSelectorState::Present,
            exact_runner_identity: ExactRunnerIdentityState::Verified,
            reserved_label_ownership: ReservedLabelOwnershipState::Verified,
            active_bound_worker: true,
            reason_code: Some(ReasonCode::new("admission-api-unavailable").unwrap()),
            retry: None,
        };

        for text in [
            render_snapshot_text(&snapshot),
            render_runner_status(&snapshot),
        ] {
            assert!(text.contains("Desired capacity: DRAINED"));
            assert!(text.contains("Achieved capacity: TRANSITIONAL"));
            assert!(text.contains("WITHDRAWAL_BLOCKED"));
            assert!(text.contains("admission-api-unavailable"));
        }
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
    fn runners_json_renders_faithful_machine_contract_and_respects_prepare_flags() {
        use crate::{
            DiscoveredCandidateView, EnrollmentState, RunnerCommandReport,
            RunnerPresentationStatus, RunnerRecordView,
        };

        let sample_report = RunnerCommandReport {
            schema_version: 1,
            read_only: false,
            prepare_only: true,
            applied: false,
            reason_code: Some(ReasonCode::new("runners-dry-run-prepared").unwrap()),
            records: vec![RunnerRecordView {
                local_runner_key: "runner-test".to_owned(),
                runner_id: 101,
                expected_runner_name: "test-runner".to_owned(),
                desired_enabled: true,
                enrollment_state: EnrollmentState::Enrolled,
                managed_admission: true,
                backend_kind: "user-session".to_owned(),
                presentation_status: RunnerPresentationStatus::Enabled,
                memory_samples: Vec::new(),
                item_applied: Some(true),
                item_reason_code: None,
            }],
            discovered_keys: vec!["disc-1".to_owned()],
            same_scope_collisions: 0,
            resource_aggregate: None,
            partial_failure: false,
            discovered_candidates: vec![DiscoveredCandidateView {
                local_discovery_key: "disc-1".to_owned(),
                backend_kind: "user-session".to_owned(),
                disposition: "OBSERVED_ONLY".to_owned(),
                refusal_reason: None,
                enrollment_eligible: true,
                same_scope_collision: false,
            }],
        };

        // Test runners status --json
        let transport = FakeTransport::with_response(AgentResponse::Runners(sample_report.clone()));
        let output = execute_cli(
            &arguments(["runners", "status", "--json"]),
            &transport,
            "0.1.0",
        )
        .unwrap();
        let deserialized: RunnerCommandReport = serde_json::from_str(&output).unwrap();
        assert_eq!(deserialized, sample_report);
        assert_eq!(
            transport.requests(),
            vec![AgentCommand::RunnersStatus {
                local_runner_key: None,
            }]
        );

        // Test runners list --json
        let transport_list =
            FakeTransport::with_response(AgentResponse::Runners(sample_report.clone()));
        let output_list = execute_cli(
            &arguments(["runners", "list", "--json"]),
            &transport_list,
            "0.1.0",
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<RunnerCommandReport>(&output_list).unwrap(),
            sample_report
        );
        assert_eq!(transport_list.requests(), vec![AgentCommand::RunnersList]);

        // Test runners enroll with --prepare
        let transport_enroll =
            FakeTransport::with_response(AgentResponse::Runners(sample_report.clone()));
        execute_cli(
            &arguments(["runners", "enroll", "runner-test", "--prepare"]),
            &transport_enroll,
            "0.1.0",
        )
        .unwrap();
        assert_eq!(
            transport_enroll.requests(),
            vec![AgentCommand::RunnersEnroll {
                local_runner_key: "runner-test".to_owned(),
                prepare_only: true,
            }]
        );

        // Test runners disable with --when-idle
        let transport_disable =
            FakeTransport::with_response(AgentResponse::Runners(sample_report.clone()));
        execute_cli(
            &arguments(["runners", "disable", "runner-test", "--when-idle"]),
            &transport_disable,
            "0.1.0",
        )
        .unwrap();
        assert_eq!(
            transport_disable.requests(),
            vec![AgentCommand::RunnersDisable {
                local_runner_key: "runner-test".to_owned(),
                when_idle: true,
                prepare_only: false,
            }]
        );
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
                body: IpcResponseBody::Success(Box::new(self.response.clone())),
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
            health_reason_code: Some(ReasonCode::new("host-observed").unwrap()),
            zen: ZenOverride::Disabled,
            user_mode: UserMode::Auto,
            desired_node_state: NodeState::Drained,
            achieved_node_state: None,
            admission: AdmissionDecision {
                allow_new_work: false,
                desired_node_state: NodeState::Drained,
                reason_code: ReasonCode::new("awaiting-auto-policy").unwrap(),
                drain_requested: true,
            },
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            runner_phase: RunnerPhase::Unknown,
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            probes: vec![ProbeSnapshot {
                id: ProbeId::new("steam-game").unwrap(),
                enabled: true,
                health: crate::ProbeHealth::Healthy,
                runtime_state: ProbeRuntimeState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            ui_preferences: UiPreferences::default(),
            effective_ui_preferences: UiPreferences::default()
                .resolve(crate::SystemPreferences::default()),
            start_on_login_preference: false,
            auto_idle_threshold_seconds: 300,
            update_checks_enabled: true,
        }
    }

    #[test]
    fn doctor_and_runners_text_omit_private_path_and_secret_shapes() {
        use crate::{
            DiscoveredCandidateView, EnrollmentState, RunnerCommandReport,
            RunnerPresentationStatus, RunnerRecordView,
        };

        let doctor = DoctorReport {
            checks: vec![DoctorCheck {
                id: ReasonCode::new("agent-health").unwrap(),
                status: DoctorStatus::Pass,
                reason_code: Some(ReasonCode::new("host-observed").unwrap()),
            }],
        };
        let doctor_text = render_doctor_text(&doctor);
        assert!(doctor_text.contains("agent-health"));
        assert!(!doctor_text.contains(r"C:\"));
        assert!(!doctor_text.contains("/Users/"));
        assert!(!doctor_text.contains("ghp_"));

        let report = RunnerCommandReport {
            schema_version: 1,
            read_only: true,
            prepare_only: false,
            applied: false,
            reason_code: Some(ReasonCode::new("runners-partial-failure").unwrap()),
            records: vec![RunnerRecordView {
                local_runner_key: "runner-a".to_owned(),
                runner_id: 7,
                expected_runner_name: "fixture".to_owned(),
                desired_enabled: true,
                enrollment_state: EnrollmentState::Enrolled,
                managed_admission: false,
                backend_kind: "user-session".to_owned(),
                presentation_status: RunnerPresentationStatus::Enabled,
                memory_samples: Vec::new(),
                item_applied: Some(false),
                item_reason_code: Some(ReasonCode::new("runners-item-failed").unwrap()),
            }],
            discovered_keys: vec!["opaque-discovery-key".to_owned()],
            same_scope_collisions: 0,
            resource_aggregate: None,
            partial_failure: true,
            discovered_candidates: vec![DiscoveredCandidateView {
                local_discovery_key: "opaque-discovery-key".to_owned(),
                backend_kind: "user-session".to_owned(),
                disposition: "OBSERVED_ONLY".to_owned(),
                refusal_reason: None,
                enrollment_eligible: true,
                same_scope_collision: false,
            }],
        };
        let runners_text = render_runners_text(&report);
        assert!(runners_text.contains("partial_failure=true"));
        assert!(runners_text.contains("opaque-discovery-key"));
        assert!(!runners_text.contains(r"C:\fixture"));
        assert!(!runners_text.contains("actions.runner."));
        assert!(!runners_text.contains("ghp_"));
    }
}
