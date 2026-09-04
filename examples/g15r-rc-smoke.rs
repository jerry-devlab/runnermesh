#[cfg(windows)]
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use runnermesh::{
    AgentCommand, AgentResponse, AgentSnapshot, AgentTransport, DoctorReport, DoctorStatus,
    Installation, IpcRequest, IpcResponseBody, LocalAgentTransport, WindowsUserStartupBackend,
    IPC_PROTOCOL_VERSION,
};

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("G15R_RC_SMOKE=FAIL\nERROR={error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> Result<(), String> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let [agent_binary, cli_binary, agent_manifest, smoke_root, expected_commit, expected_channel] =
        arguments.as_slice()
    else {
        return Err(
            "usage: g15r-rc-smoke <agent> <cli> <manifest> <new-smoke-root> <expected-commit> <expected-channel>"
                .to_owned(),
        );
    };
    let agent_binary = require_file(agent_binary.into(), "Agent binary")?;
    let cli_binary = require_file(cli_binary.into(), "CLI binary")?;
    let agent_manifest = require_file(agent_manifest.into(), "Agent manifest")?;
    let smoke_root = require_new_absolute_root(smoke_root.into())?;
    let expected_commit = expected_commit.to_string_lossy().into_owned();
    let expected_channel = expected_channel.to_string_lossy().into_owned();

    let payload = smoke_root.join("payload");
    let install_root = smoke_root.join("installed");
    let startup_root = smoke_root.join("startup");
    fs::create_dir_all(&payload).map_err(|error| error.to_string())?;
    fs::create_dir_all(&startup_root).map_err(|error| error.to_string())?;
    fs::copy(&agent_binary, payload.join("runnermesh-agent.exe"))
        .map_err(|error| error.to_string())?;
    fs::copy(&cli_binary, payload.join("runnermesh.exe")).map_err(|error| error.to_string())?;
    fs::copy(&agent_manifest, payload.join("runnermesh-agent.manifest"))
        .map_err(|error| error.to_string())?;

    let installation = Installation::new(&install_root);
    let receipt = installation
        .install("0.1.0", &payload)
        .map_err(|error| error.to_string())?;
    if !receipt.activated {
        return Err("fresh sandbox install did not activate 0.1.0".to_owned());
    }
    let startup = WindowsUserStartupBackend::new(&startup_root);
    if !installation
        .enable_autostart(&startup)
        .map_err(|error| error.to_string())?
    {
        return Err("fresh sandbox install did not create autostart".to_owned());
    }

    let stable_agent = installation.stable_agent_entry();
    let installed_cli = installation
        .active_agent_path()
        .map_err(|error| error.to_string())?
        .parent()
        .map(|path| path.join("runnermesh.exe"))
        .ok_or("active Agent path had no version directory")?;
    let mut child = Command::new(&stable_agent)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    let ready = wait_for_runtime_ready(&mut child, &install_root);
    if let Err(error) = ready {
        stop_sandbox_child(&mut child, false);
        return Err(error);
    }
    let runtime_result = validate_ready_runtime(
        &mut child,
        &installed_cli,
        &expected_commit,
        &expected_channel,
    );
    if let Err(error) = runtime_result {
        stop_sandbox_child(&mut child, true);
        return Err(error);
    }

    let uninstall = installation
        .uninstall(&startup)
        .map_err(|error| error.to_string())?;
    if uninstall.removed_versions == 0
        || install_root.join("versions").exists()
        || install_root.join("bin").exists()
        || install_root.join("current.json").exists()
        || install_root.join(".runnermesh-installation.json").exists()
    {
        return Err("sandbox uninstall did not remove all owned runtime content".to_owned());
    }
    fs::remove_dir_all(&smoke_root).map_err(|error| error.to_string())?;

    println!(
        "G15R_RC_SMOKE=PASS\nINSTALLED_AGENT=PASS\nNATIVE_TRAY=PASS\nCLI_IPC=PASS\nDOCTOR=PASS\nTYPED_EXIT=PASS\nOWNED_UNINSTALL=PASS"
    );
    Ok(())
}

#[cfg(windows)]
fn validate_ready_runtime(
    child: &mut Child,
    installed_cli: &Path,
    expected_commit: &str,
    expected_channel: &str,
) -> Result<(), String> {
    let snapshot: AgentSnapshot = run_json_command(installed_cli, &["status", "--json"])?;
    if snapshot.build.commit != expected_commit
        || snapshot.build.channel != expected_channel
        || snapshot.build.target != "x86_64-pc-windows-msvc"
    {
        return Err("installed Agent provenance did not match the admitted build".to_owned());
    }
    let doctor: DoctorReport = run_json_command(installed_cli, &["doctor", "--json"])?;
    if doctor
        .checks
        .iter()
        .any(|check| matches!(check.status, DoctorStatus::Fail | DoctorStatus::Unknown))
    {
        return Err("installed Agent doctor returned FAIL or UNKNOWN".to_owned());
    }
    for required in [
        "agent-runtime",
        "native-tray",
        "local-ipc",
        "probe-system",
        "agent-runtime-ready",
    ] {
        if !doctor
            .checks
            .iter()
            .any(|check| check.id.as_str() == required && check.status == DoctorStatus::Pass)
        {
            return Err(format!(
                "installed Agent doctor did not PASS required check {required}"
            ));
        }
    }
    request_typed_exit()?;
    wait_for_clean_exit(child)
}

#[cfg(windows)]
fn require_file(path: PathBuf, label: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("{label} must be an existing absolute file"));
    }
    Ok(path)
}

#[cfg(windows)]
fn require_new_absolute_root(path: PathBuf) -> Result<PathBuf, String> {
    if !path.is_absolute() || path.exists() {
        return Err("smoke root must be a new absolute path".to_owned());
    }
    Ok(path)
}

#[cfg(windows)]
fn wait_for_runtime_ready(child: &mut Child, install_root: &Path) -> Result<(), String> {
    let stage_path = install_root
        .join("state")
        .join("agent")
        .join("runtime-stage.txt");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!(
                "installed Agent exited before readiness with {status}: {}",
                take_stderr(child)
            ));
        }
        if fs::read_to_string(&stage_path).is_ok_and(|stage| stage.trim() == "runtime-ready") {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err("installed Agent did not reach runtime-ready within 20 seconds".to_owned())
}

#[cfg(windows)]
fn run_json_command<T: serde::de::DeserializeOwned>(
    cli: &Path,
    arguments: &[&str],
) -> Result<T, String> {
    let output = Command::new(cli)
        .args(arguments)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "installed CLI {:?} failed with {}: {}",
            arguments,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn request_typed_exit() -> Result<(), String> {
    let response = LocalAgentTransport::new(Duration::from_secs(10))
        .call(IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: 1,
            command: AgentCommand::ExitAfterDrain,
        })
        .map_err(|error| error.to_string())?;
    if matches!(
        response.body,
        IpcResponseBody::Success(response)
            if matches!(*response, AgentResponse::Accepted { .. })
    ) {
        Ok(())
    } else {
        Err("installed Agent did not accept typed exit".to_owned())
    }
}

#[cfg(windows)]
fn wait_for_clean_exit(child: &mut Child) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            if status.success() {
                return Ok(());
            }
            return Err(format!(
                "installed Agent failed during typed exit with {status}: {}",
                take_stderr(child)
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
    force_stop_sandbox_child(child);
    Err("installed Agent did not finish typed exit within 20 seconds".to_owned())
}

#[cfg(windows)]
fn stop_sandbox_child(child: &mut Child, typed_exit_safe: bool) {
    if child.try_wait().is_ok_and(|status| status.is_some()) {
        return;
    }
    if typed_exit_safe {
        let _ = request_typed_exit();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    force_stop_sandbox_child(child);
}

#[cfg(windows)]
fn force_stop_sandbox_child(child: &mut Child) {
    // This process is the verifier-owned, unconfigured sandbox Agent. It has
    // no runner binding and cannot own or terminate a Worker.
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn take_stderr(child: &mut Child) -> String {
    let mut output = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut output);
    }
    output.trim().to_owned()
}

#[cfg(not(windows))]
fn main() {
    println!("G15R_RC_SMOKE=N/A_NON_WINDOWS");
}
