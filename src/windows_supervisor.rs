//! Windows/user-session supervisor preparation for the H1 boundary.
//!
//! The G10R adapter deliberately prepares and validates operations without
//! executing them. The explicit G11 executor below owns the bounded real
//! official-runner launch and cooperative drain mechanism; it is never wired
//! to the normal pre-H1 reconciler. The G10R sandbox test exercises a harmless
//! child process only.

use std::{
    collections::HashMap,
    fmt, fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::GetLastError,
    System::{
        Console::{AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT},
        Threading::{CREATE_NEW_CONSOLE, CREATE_NEW_PROCESS_GROUP},
    },
};

use crate::SupervisorAction;

/// A validated user-session launch context. It carries no runner registration,
/// work-root, or execution-identity mutation authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserSessionLaunch {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
}

/// The concrete Windows action shape which an H1-qualified executor may later
/// consume. Constructing one has no operating-system side effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedWindowsSupervisorAction {
    StartUserSession(UserSessionLaunch),
    RequestGracefulDrain,
    StopListener,
    StopListenerAfterDrain,
    RestartConnection,
    AdoptVerifiedListener,
}

/// Read-only readiness result for the concrete user-session adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsSupervisorReadiness {
    Ready,
    RunnerHomeMissing,
    EntrypointMissing,
}

/// Prepares a future official-runner operation using an explicitly selected
/// runner home. It never scans arbitrary locations and never starts or stops a
/// process. `run.cmd` is the standard Windows runner user-session entrypoint;
/// its real lifecycle semantics remain deliberately unqualified until H1.
#[derive(Clone, Debug)]
pub struct WindowsUserSessionSupervisorAdapter {
    runner_home: PathBuf,
    entrypoint: PathBuf,
}

impl WindowsUserSessionSupervisorAdapter {
    pub fn for_runner_home(runner_home: impl Into<PathBuf>) -> Self {
        let runner_home = runner_home.into();
        Self {
            entrypoint: runner_home.join("run.cmd"),
            runner_home,
        }
    }

    /// Inspects only filesystem metadata to prove that a future H1 executor
    /// can be supplied a bounded user-session launch context.
    pub fn readiness(&self) -> WindowsSupervisorReadiness {
        if !self.runner_home.is_dir() {
            WindowsSupervisorReadiness::RunnerHomeMissing
        } else if !self.entrypoint.is_file() {
            WindowsSupervisorReadiness::EntrypointMissing
        } else {
            WindowsSupervisorReadiness::Ready
        }
    }

    /// Returns a prepared, side-effect-free action. The caller must separately
    /// satisfy the existing identity/work-root checks before any H1 executor
    /// is permitted to apply it.
    pub fn prepare(
        &self,
        action: SupervisorAction,
    ) -> Result<PreparedWindowsSupervisorAction, WindowsSupervisorReadiness> {
        if self.readiness() != WindowsSupervisorReadiness::Ready {
            return Err(self.readiness());
        }
        Ok(match action {
            SupervisorAction::Start => {
                PreparedWindowsSupervisorAction::StartUserSession(UserSessionLaunch {
                    executable: self.entrypoint.clone(),
                    arguments: Vec::new(),
                    working_directory: self.runner_home.clone(),
                })
            }
            SupervisorAction::RequestDrain => PreparedWindowsSupervisorAction::RequestGracefulDrain,
            SupervisorAction::Stop => PreparedWindowsSupervisorAction::StopListener,
            SupervisorAction::StopAfterDrain => {
                PreparedWindowsSupervisorAction::StopListenerAfterDrain
            }
            SupervisorAction::RestartConnection => {
                PreparedWindowsSupervisorAction::RestartConnection
            }
            SupervisorAction::AdoptExistingListener => {
                PreparedWindowsSupervisorAction::AdoptVerifiedListener
            }
        })
    }

    pub fn runner_home(&self) -> &Path {
        &self.runner_home
    }
}

/// A caller-selected official runner binding. It is verified against the
/// already-configured runner home, exact registration bytes, and the standard
/// owned `_work` root before every control action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedRunnerBinding {
    runner_home: PathBuf,
    work_root: PathBuf,
    entrypoint: PathBuf,
    registration_sha256: [u8; 32],
}

impl VerifiedRunnerBinding {
    /// Binds to an already configured runner. This never parses, rewrites, or
    /// re-registers the runner configuration.
    pub fn bind(
        runner_home: impl Into<PathBuf>,
        work_root: impl Into<PathBuf>,
    ) -> Result<Self, WindowsRunnerExecutorError> {
        let runner_home = canonical_directory(runner_home.into(), "runner home")?;
        let work_root = canonical_directory(work_root.into(), "work root")?;
        let expected_work_root = runner_home.join("_work").canonicalize().map_err(|error| {
            WindowsRunnerExecutorError::Io {
                operation: "canonicalize expected work root",
                message: error.to_string(),
            }
        })?;
        if work_root != expected_work_root {
            return Err(WindowsRunnerExecutorError::WorkRootMismatch);
        }

        let entrypoint = runner_home.join("run.cmd");
        if !entrypoint.is_file() {
            return Err(WindowsRunnerExecutorError::EntrypointMissing);
        }

        let registration = runner_home.join(".runner");
        let registration_sha256 = sha256_file(&registration)?;
        Ok(Self {
            runner_home,
            work_root,
            entrypoint,
            registration_sha256,
        })
    }

    pub fn runner_home(&self) -> &Path {
        &self.runner_home
    }

    pub fn work_root(&self) -> &Path {
        &self.work_root
    }

    fn launch(&self) -> UserSessionLaunch {
        UserSessionLaunch {
            executable: self.entrypoint.clone(),
            arguments: Vec::new(),
            working_directory: self.runner_home.clone(),
        }
    }

    pub fn prepared_start(&self) -> PreparedWindowsSupervisorAction {
        PreparedWindowsSupervisorAction::StartUserSession(self.launch())
    }

    fn revalidate(&self) -> Result<(), WindowsRunnerExecutorError> {
        if self
            .runner_home
            .canonicalize()
            .map_err(io_error("canonicalize runner home"))?
            != self.runner_home
            || self
                .work_root
                .canonicalize()
                .map_err(io_error("canonicalize work root"))?
                != self.work_root
            || self
                .runner_home
                .join("_work")
                .canonicalize()
                .map_err(io_error("canonicalize expected work root"))?
                != self.work_root
        {
            return Err(WindowsRunnerExecutorError::BindingDrift);
        }
        if !self.entrypoint.is_file() {
            return Err(WindowsRunnerExecutorError::EntrypointMissing);
        }
        if sha256_file(&self.runner_home.join(".runner"))? != self.registration_sha256 {
            return Err(WindowsRunnerExecutorError::RegistrationDrift);
        }
        Ok(())
    }
}

/// Result of one bounded executor operation. No result represents a forced
/// process termination or a registration/configuration change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsRunnerExecutionOutcome {
    Started { process_id: u32 },
    GracefulDrainRequested,
    NoopAlreadyRunning,
    NoopAlreadyExited,
    RefusedActiveJobMayExist,
    RefusedUnownedListenerAdoption,
}

/// A deliberately small process seam. The native implementation can launch
/// only the bound `run.cmd`, ask only its own process group for CTRL+BREAK,
/// and observe only children that it launched.
pub trait BoundedRunnerProcessPort {
    fn start(&mut self, launch: &UserSessionLaunch) -> Result<u32, WindowsRunnerExecutorError>;
    fn request_graceful_drain(&mut self, process_id: u32)
        -> Result<(), WindowsRunnerExecutorError>;
    fn has_exited(&mut self, process_id: u32) -> Result<bool, WindowsRunnerExecutorError>;
}

/// Bounded executor for one user-session official runner. It never discovers
/// runner homes, touches services, kills a process, edits configuration, or
/// adopts an unowned listener.
pub struct WindowsOfficialRunnerExecutor<P = NativeRunnerProcessPort> {
    binding: VerifiedRunnerBinding,
    port: P,
    active: Option<ActiveRunner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveRunner {
    process_id: u32,
    graceful_drain_requested: bool,
}

impl WindowsOfficialRunnerExecutor<NativeRunnerProcessPort> {
    pub fn new(binding: VerifiedRunnerBinding) -> Self {
        Self::with_port(binding, NativeRunnerProcessPort::default())
    }
}

impl<P> WindowsOfficialRunnerExecutor<P>
where
    P: BoundedRunnerProcessPort,
{
    pub fn with_port(binding: VerifiedRunnerBinding, port: P) -> Self {
        Self {
            binding,
            port,
            active: None,
        }
    }

    pub fn binding(&self) -> &VerifiedRunnerBinding {
        &self.binding
    }

    pub fn port_mut(&mut self) -> &mut P {
        &mut self.port
    }

    /// Applies only a previously prepared action that exactly matches this
    /// binding. The executor rejects arbitrary `UserSessionLaunch` values.
    pub fn apply(
        &mut self,
        action: PreparedWindowsSupervisorAction,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        self.binding.revalidate()?;
        match action {
            PreparedWindowsSupervisorAction::StartUserSession(launch) => {
                if launch != self.binding.launch() {
                    return Err(WindowsRunnerExecutorError::LaunchBindingMismatch);
                }
                self.start()
            }
            PreparedWindowsSupervisorAction::RequestGracefulDrain
            | PreparedWindowsSupervisorAction::StopListener => self.request_graceful_drain(),
            PreparedWindowsSupervisorAction::StopListenerAfterDrain => self.stop_after_drain(),
            PreparedWindowsSupervisorAction::RestartConnection => self.restart_after_exit(),
            PreparedWindowsSupervisorAction::AdoptVerifiedListener => {
                Ok(WindowsRunnerExecutionOutcome::RefusedUnownedListenerAdoption)
            }
        }
    }

    fn start(&mut self) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        if self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyRunning);
        }
        let process_id = self.port.start(&self.binding.launch())?;
        self.active = Some(ActiveRunner {
            process_id,
            graceful_drain_requested: false,
        });
        Ok(WindowsRunnerExecutionOutcome::Started { process_id })
    }

    fn request_graceful_drain(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        let Some(active) = self.active else {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        };
        if self.port.has_exited(active.process_id)? {
            self.active = None;
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        }
        if active.graceful_drain_requested {
            return Ok(WindowsRunnerExecutionOutcome::GracefulDrainRequested);
        }
        self.port.request_graceful_drain(active.process_id)?;
        self.active = Some(ActiveRunner {
            graceful_drain_requested: true,
            ..active
        });
        Ok(WindowsRunnerExecutionOutcome::GracefulDrainRequested)
    }

    fn stop_after_drain(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        let Some(active) = self.active else {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        };
        if self.port.has_exited(active.process_id)? {
            self.active = None;
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        }
        Ok(WindowsRunnerExecutionOutcome::RefusedActiveJobMayExist)
    }

    fn restart_after_exit(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        if self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::RefusedActiveJobMayExist);
        }
        self.start()
    }

    fn active_runner_is_live(&mut self) -> Result<bool, WindowsRunnerExecutorError> {
        let Some(active) = self.active else {
            return Ok(false);
        };
        if self.port.has_exited(active.process_id)? {
            self.active = None;
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

/// Native port used by the ordinary user-session executor. The process map is
/// intentionally limited to children that this instance launched.
#[derive(Default)]
pub struct NativeRunnerProcessPort {
    children: HashMap<u32, std::process::Child>,
}

impl BoundedRunnerProcessPort for NativeRunnerProcessPort {
    fn start(&mut self, launch: &UserSessionLaunch) -> Result<u32, WindowsRunnerExecutorError> {
        let child = Command::new("cmd.exe")
            .args(["/d", "/s", "/c"])
            .arg(&launch.executable)
            .current_dir(&launch.working_directory)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(io_error("start exact runner entrypoint"))?;
        let process_id = child.id();
        if self.children.insert(process_id, child).is_some() {
            return Err(WindowsRunnerExecutorError::UnexpectedProcessCollision);
        }
        Ok(process_id)
    }

    fn request_graceful_drain(
        &mut self,
        process_id: u32,
    ) -> Result<(), WindowsRunnerExecutorError> {
        if !self.children.contains_key(&process_id) {
            return Err(WindowsRunnerExecutorError::UnownedProcess);
        }
        // CTRL+BREAK is the official runner's cooperative console shutdown
        // signal. This is deliberately not TerminateProcess/kill.
        // The Agent is a Windows-subsystem executable and normally owns no
        // console. Attach only to the private console of its exact child so
        // CTRL+BREAK cannot reach an unrelated process group.
        if unsafe { AttachConsole(process_id) } == 0 {
            return Err(WindowsRunnerExecutorError::ConsoleAttachFailed(unsafe {
                GetLastError()
            }));
        }
        let signalled = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, process_id) };
        let signal_error = if signalled == 0 {
            Some(unsafe { GetLastError() })
        } else {
            None
        };
        if unsafe { FreeConsole() } == 0 {
            return Err(WindowsRunnerExecutorError::ConsoleDetachFailed(unsafe {
                GetLastError()
            }));
        }
        if let Some(error) = signal_error {
            return Err(WindowsRunnerExecutorError::ConsoleSignalFailed(error));
        }
        Ok(())
    }

    fn has_exited(&mut self, process_id: u32) -> Result<bool, WindowsRunnerExecutorError> {
        let Some(child) = self.children.get_mut(&process_id) else {
            return Err(WindowsRunnerExecutorError::UnownedProcess);
        };
        let exited = child
            .try_wait()
            .map_err(io_error("observe owned runner child"))?
            .is_some();
        if exited {
            self.children.remove(&process_id);
        }
        Ok(exited)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowsRunnerExecutorError {
    RunnerHomeMissing,
    WorkRootMissing,
    WorkRootMismatch,
    EntrypointMissing,
    RegistrationDrift,
    BindingDrift,
    LaunchBindingMismatch,
    UnexpectedProcessCollision,
    UnownedProcess,
    ConsoleAttachFailed(u32),
    ConsoleDetachFailed(u32),
    ConsoleSignalFailed(u32),
    Io {
        operation: &'static str,
        message: String,
    },
}

impl fmt::Display for WindowsRunnerExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunnerHomeMissing => write!(formatter, "runner home is missing"),
            Self::WorkRootMissing => write!(formatter, "work root is missing"),
            Self::WorkRootMismatch => {
                write!(formatter, "work root is not the bound runner _work root")
            }
            Self::EntrypointMissing => write!(formatter, "runner run.cmd entrypoint is missing"),
            Self::RegistrationDrift => write!(formatter, "runner registration fingerprint changed"),
            Self::BindingDrift => write!(formatter, "runner home or work root binding changed"),
            Self::LaunchBindingMismatch => {
                write!(formatter, "prepared launch does not match the bound runner")
            }
            Self::UnexpectedProcessCollision => {
                write!(formatter, "owned runner process id collision")
            }
            Self::UnownedProcess => {
                write!(formatter, "runner process is not owned by this executor")
            }
            Self::ConsoleAttachFailed(error) => {
                write!(formatter, "runner console attach failed: {error}")
            }
            Self::ConsoleDetachFailed(error) => {
                write!(formatter, "runner console detach failed: {error}")
            }
            Self::ConsoleSignalFailed(error) => {
                write!(formatter, "CTRL+BREAK delivery failed: {error}")
            }
            Self::Io { operation, message } => write!(formatter, "{operation}: {message}"),
        }
    }
}

impl std::error::Error for WindowsRunnerExecutorError {}

fn canonical_directory(
    path: PathBuf,
    role: &'static str,
) -> Result<PathBuf, WindowsRunnerExecutorError> {
    if !path.is_dir() {
        return Err(match role {
            "runner home" => WindowsRunnerExecutorError::RunnerHomeMissing,
            "work root" => WindowsRunnerExecutorError::WorkRootMissing,
            _ => WindowsRunnerExecutorError::BindingDrift,
        });
    }
    path.canonicalize()
        .map_err(io_error("canonicalize runner binding"))
}

fn sha256_file(path: &Path) -> Result<[u8; 32], WindowsRunnerExecutorError> {
    let bytes = fs::read(path).map_err(io_error("read runner registration"))?;
    Ok(Sha256::digest(bytes).into())
}

fn io_error(operation: &'static str) -> impl FnOnce(std::io::Error) -> WindowsRunnerExecutorError {
    move |error| WindowsRunnerExecutorError::Io {
        operation,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        fs,
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{
        BoundedRunnerProcessPort, PreparedWindowsSupervisorAction, UserSessionLaunch,
        VerifiedRunnerBinding, WindowsOfficialRunnerExecutor, WindowsRunnerExecutionOutcome,
        WindowsRunnerExecutorError, WindowsSupervisorReadiness,
        WindowsUserSessionSupervisorAdapter,
    };
    use crate::SupervisorAction;

    fn sandbox_runner_home() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("runnermesh-supervisor-test-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("run.cmd"), "@echo off\r\nexit /b 0\r\n").unwrap();
        root
    }

    #[test]
    fn adapter_prepares_all_lifecycle_actions_without_executing_a_runner() {
        let root = sandbox_runner_home();
        let adapter = WindowsUserSessionSupervisorAdapter::for_runner_home(&root);
        assert_eq!(adapter.readiness(), WindowsSupervisorReadiness::Ready);

        let start = adapter.prepare(SupervisorAction::Start).unwrap();
        assert_eq!(
            start,
            PreparedWindowsSupervisorAction::StartUserSession(super::UserSessionLaunch {
                executable: root.join("run.cmd"),
                arguments: Vec::new(),
                working_directory: root.clone(),
            })
        );
        assert_eq!(
            adapter.prepare(SupervisorAction::RequestDrain).unwrap(),
            PreparedWindowsSupervisorAction::RequestGracefulDrain
        );
        assert_eq!(
            adapter.prepare(SupervisorAction::StopAfterDrain).unwrap(),
            PreparedWindowsSupervisorAction::StopListenerAfterDrain
        );
        assert_eq!(
            adapter.prepare(SupervisorAction::Stop).unwrap(),
            PreparedWindowsSupervisorAction::StopListener
        );
        assert_eq!(
            adapter
                .prepare(SupervisorAction::RestartConnection)
                .unwrap(),
            PreparedWindowsSupervisorAction::RestartConnection
        );
        assert_eq!(
            adapter
                .prepare(SupervisorAction::AdoptExistingListener)
                .unwrap(),
            PreparedWindowsSupervisorAction::AdoptVerifiedListener
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn harmless_sandbox_child_can_start_be_observed_and_stop() {
        // This is deliberately unrelated to an official runner. It proves
        // the development test lane can exercise a bounded user-session child
        // lifecycle without reaching a Service or a runner work root.
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .spawn()
            .expect("start harmless sandbox child");
        thread::sleep(Duration::from_millis(75));
        assert!(child.try_wait().unwrap().is_none());
        child.kill().expect("stop harmless sandbox child");
        let _ = child.wait().expect("reap harmless sandbox child");
    }

    #[test]
    fn bounded_executor_only_launches_the_frozen_runner_and_drains_cooperatively() {
        let (root, binding) = bound_runner();
        let adapter = WindowsUserSessionSupervisorAdapter::for_runner_home(binding.runner_home());
        let start = adapter.prepare(SupervisorAction::Start).unwrap();
        let mut executor =
            WindowsOfficialRunnerExecutor::with_port(binding, FakeRunnerPort::default());

        let WindowsRunnerExecutionOutcome::Started { process_id } = executor.apply(start).unwrap()
        else {
            panic!("expected one owned runner launch");
        };
        assert_eq!(executor.port_mut().starts.len(), 1);
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::RequestGracefulDrain)
                .unwrap(),
            WindowsRunnerExecutionOutcome::GracefulDrainRequested
        );
        assert_eq!(executor.port_mut().graceful_breaks, vec![process_id]);
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::RequestGracefulDrain)
                .unwrap(),
            WindowsRunnerExecutionOutcome::GracefulDrainRequested
        );
        assert_eq!(executor.port_mut().graceful_breaks, vec![process_id]);
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::StopListenerAfterDrain)
                .unwrap(),
            WindowsRunnerExecutionOutcome::RefusedActiveJobMayExist
        );
        assert!(executor
            .port_mut()
            .graceful_breaks
            .iter()
            .all(|id| *id == process_id));

        executor.port_mut().exited.insert(process_id);
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::StopListenerAfterDrain)
                .unwrap(),
            WindowsRunnerExecutionOutcome::NoopAlreadyExited
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_executor_refuses_binding_drift_and_unowned_adoption() {
        let (root, binding) = bound_runner();
        let mut executor =
            WindowsOfficialRunnerExecutor::with_port(binding, FakeRunnerPort::default());

        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::AdoptVerifiedListener)
                .unwrap(),
            WindowsRunnerExecutionOutcome::RefusedUnownedListenerAdoption
        );
        fs::write(root.join(".runner"), "changed-registration").unwrap();
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::RequestGracefulDrain)
                .unwrap_err(),
            WindowsRunnerExecutorError::RegistrationDrift
        );
        assert!(executor.port_mut().starts.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_executor_rejects_a_prepared_launch_for_another_home() {
        let (root, binding) = bound_runner();
        let mut executor =
            WindowsOfficialRunnerExecutor::with_port(binding, FakeRunnerPort::default());
        let foreign = UserSessionLaunch {
            executable: root.join("foreign-run.cmd"),
            arguments: Vec::new(),
            working_directory: root.clone(),
        };
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::StartUserSession(foreign))
                .unwrap_err(),
            WindowsRunnerExecutorError::LaunchBindingMismatch
        );
        assert!(executor.port_mut().starts.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn binding_refuses_a_work_root_outside_the_exact_runner_home() {
        let root = sandbox_runner_home();
        fs::create_dir_all(root.join("_work")).unwrap();
        fs::create_dir_all(root.join("other-work")).unwrap();
        fs::write(root.join(".runner"), "synthetic-runner-identity").unwrap();
        assert_eq!(
            VerifiedRunnerBinding::bind(&root, root.join("other-work")).unwrap_err(),
            WindowsRunnerExecutorError::WorkRootMismatch
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn bound_runner() -> (std::path::PathBuf, VerifiedRunnerBinding) {
        let root = sandbox_runner_home();
        fs::create_dir_all(root.join("_work")).unwrap();
        fs::write(root.join(".runner"), "synthetic-runner-identity").unwrap();
        let binding = VerifiedRunnerBinding::bind(&root, root.join("_work")).unwrap();
        (root, binding)
    }

    #[derive(Default)]
    struct FakeRunnerPort {
        next_process_id: u32,
        starts: Vec<UserSessionLaunch>,
        graceful_breaks: Vec<u32>,
        exited: HashSet<u32>,
        active: HashMap<u32, UserSessionLaunch>,
    }

    impl BoundedRunnerProcessPort for FakeRunnerPort {
        fn start(&mut self, launch: &UserSessionLaunch) -> Result<u32, WindowsRunnerExecutorError> {
            self.next_process_id += 1;
            let process_id = self.next_process_id;
            self.starts.push(launch.clone());
            self.active.insert(process_id, launch.clone());
            Ok(process_id)
        }

        fn request_graceful_drain(
            &mut self,
            process_id: u32,
        ) -> Result<(), WindowsRunnerExecutorError> {
            if !self.active.contains_key(&process_id) {
                return Err(WindowsRunnerExecutorError::UnownedProcess);
            }
            self.graceful_breaks.push(process_id);
            Ok(())
        }

        fn has_exited(&mut self, process_id: u32) -> Result<bool, WindowsRunnerExecutorError> {
            if !self.active.contains_key(&process_id) {
                return Err(WindowsRunnerExecutorError::UnownedProcess);
            }
            Ok(self.exited.contains(&process_id))
        }
    }
}
