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
    os::{windows::fs::MetadataExt, windows::process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sha2::{Digest, Sha256};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

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
    DeferDrainUntilRunOnceCompletion,
    WaitForRunOnceCompletion,
    RefuseIdleWithdrawal,
    RestartAfterRunOnceExit,
    WaitForVerifiedBoundRunnerExit,
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
                    arguments: vec!["--once".to_owned()],
                    working_directory: self.runner_home.clone(),
                })
            }
            SupervisorAction::RequestDrain => {
                PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion
            }
            // There is no race-free local idle-listener withdrawal primitive.
            // This preparation is an explicit safe refusal, never a signal.
            SupervisorAction::Stop => PreparedWindowsSupervisorAction::RefuseIdleWithdrawal,
            SupervisorAction::StopAfterDrain => {
                PreparedWindowsSupervisorAction::WaitForRunOnceCompletion
            }
            SupervisorAction::RestartConnection => {
                PreparedWindowsSupervisorAction::RestartAfterRunOnceExit
            }
            SupervisorAction::AdoptExistingListener => {
                PreparedWindowsSupervisorAction::WaitForVerifiedBoundRunnerExit
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
    entrypoint_sha256: [u8; 32],
    listener_image: PathBuf,
    listener_image_sha256: [u8; 32],
    worker_image: PathBuf,
    worker_image_sha256: [u8; 32],
    work_root_creation_time: u64,
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
        let expected_work_root =
            canonicalize_win32_path(&runner_home.join("_work")).map_err(|error| {
                WindowsRunnerExecutorError::Io {
                    operation: "canonicalize expected work root",
                    message: error.to_string(),
                }
            })?;
        if work_root != expected_work_root {
            return Err(WindowsRunnerExecutorError::WorkRootMismatch);
        }

        let entrypoint = canonical_file(runner_home.join("run.cmd"), "runner entrypoint")?;
        let listener_image = canonical_file(
            runner_home.join("bin").join("Runner.Listener.exe"),
            "runner listener image",
        )?;
        let worker_image = canonical_file(
            runner_home.join("bin").join("Runner.Worker.exe"),
            "runner worker image",
        )?;

        let registration = runner_home.join(".runner");
        let registration_sha256 = sha256_file(&registration)?;
        let entrypoint_sha256 = sha256_file(&entrypoint)?;
        let listener_image_sha256 = sha256_file(&listener_image)?;
        let worker_image_sha256 = sha256_file(&worker_image)?;
        let work_root_creation_time = fs::metadata(&work_root)
            .map_err(io_error("read bound work-root metadata"))?
            .creation_time();
        Ok(Self {
            runner_home,
            work_root,
            entrypoint,
            registration_sha256,
            entrypoint_sha256,
            listener_image,
            listener_image_sha256,
            worker_image,
            worker_image_sha256,
            work_root_creation_time,
        })
    }

    pub fn runner_home(&self) -> &Path {
        &self.runner_home
    }

    pub fn work_root(&self) -> &Path {
        &self.work_root
    }

    fn listener_image(&self) -> &Path {
        &self.listener_image
    }

    fn worker_image(&self) -> &Path {
        &self.worker_image
    }

    fn launch(&self) -> UserSessionLaunch {
        UserSessionLaunch {
            executable: self.entrypoint.clone(),
            arguments: vec!["--once".to_owned()],
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
            .map(win32_compatible_path)
            .map_err(io_error("canonicalize runner home"))?
            != self.runner_home
            || self
                .work_root
                .canonicalize()
                .map(win32_compatible_path)
                .map_err(io_error("canonicalize work root"))?
                != self.work_root
            || self
                .runner_home
                .join("_work")
                .canonicalize()
                .map(win32_compatible_path)
                .map_err(io_error("canonicalize expected work root"))?
                != self.work_root
        {
            return Err(WindowsRunnerExecutorError::BindingDrift);
        }
        if canonical_file(self.entrypoint.clone(), "runner entrypoint")? != self.entrypoint
            || sha256_file(&self.entrypoint)? != self.entrypoint_sha256
        {
            return Err(WindowsRunnerExecutorError::EntrypointDrift);
        }
        if sha256_file(&self.runner_home.join(".runner"))? != self.registration_sha256 {
            return Err(WindowsRunnerExecutorError::RegistrationDrift);
        }
        if canonical_file(self.listener_image.clone(), "runner listener image")?
            != self.listener_image
            || canonical_file(self.worker_image.clone(), "runner worker image")?
                != self.worker_image
            || sha256_file(&self.listener_image)? != self.listener_image_sha256
            || sha256_file(&self.worker_image)? != self.worker_image_sha256
        {
            return Err(WindowsRunnerExecutorError::RunnerImageDrift);
        }
        if fs::metadata(&self.work_root)
            .map_err(io_error("read bound work-root metadata"))?
            .creation_time()
            != self.work_root_creation_time
        {
            return Err(WindowsRunnerExecutorError::WorkRootIdentityDrift);
        }
        Ok(())
    }
}

/// Result of one bounded executor operation. No result represents a signal,
/// forced process termination, or a registration/configuration change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsRunnerExecutionOutcome {
    Started { process_id: u32 },
    DrainDeferredUntilRunOnceCompletion,
    WaitingForRunOnceCompletion,
    NoopAlreadyRunning,
    NoopAlreadyExited,
    RefusedActiveRunOnceMayExist,
    IdleWithdrawalAtomicityUnproven,
    SafeWaitForExactBoundRunner { process_id: u32 },
    IgnoredUnrelatedRunner,
}

/// Read-only result for the exact runner-home process scope. A recognized
/// runner at a different executable path remains outside this executor's
/// authority. A missing executable path is intentionally ambiguous.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoundRunnerObservation {
    #[default]
    Absent,
    ExactBoundListener {
        process_id: u32,
    },
    UnrelatedRunnerPresent,
    AmbiguousExecutablePath,
}

/// A deliberately small process seam. The native implementation launches only
/// the bound `run.cmd --once`, observes only its children, and can perform a
/// read-only exact-image scan for safe wait-only reconstruction. It deliberately
/// has no signal or termination operation.
pub trait BoundedRunnerProcessPort {
    fn start(&mut self, launch: &UserSessionLaunch) -> Result<u32, WindowsRunnerExecutorError>;
    fn has_exited(&mut self, process_id: u32) -> Result<bool, WindowsRunnerExecutorError>;
    fn observe_exact_bound_runner(
        &mut self,
        binding: &VerifiedRunnerBinding,
    ) -> Result<BoundRunnerObservation, WindowsRunnerExecutorError>;
}

/// Bounded executor for one user-session official runner. It never discovers
/// runner homes, touches services, kills a process, edits configuration, or
/// takes control of an externally observed process. Restart reconstruction is
/// wait-only until the exact observed process exits naturally.
pub struct WindowsOfficialRunnerExecutor<P = NativeRunnerProcessPort> {
    binding: VerifiedRunnerBinding,
    port: P,
    active: Option<ActiveRunner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveRunner {
    process_id: u32,
    drain_pending: bool,
    tracking: ActiveRunnerTracking,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveRunnerTracking {
    OwnedChild,
    WaitOnlyExactBound,
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

    /// Performs restart reconstruction without adopting or signalling an
    /// existing runner. Exact image evidence may cause this executor to wait;
    /// ambiguous evidence refuses the operation and unrelated runners are
    /// ignored.
    pub fn reconstruct_safely(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        self.binding.revalidate()?;
        if self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyRunning);
        }
        self.observe_external_runner()
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
            PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion => {
                self.defer_drain_until_run_once_completion()
            }
            PreparedWindowsSupervisorAction::WaitForRunOnceCompletion
            | PreparedWindowsSupervisorAction::WaitForVerifiedBoundRunnerExit => {
                self.wait_for_run_once_completion()
            }
            PreparedWindowsSupervisorAction::RefuseIdleWithdrawal => {
                Ok(WindowsRunnerExecutionOutcome::IdleWithdrawalAtomicityUnproven)
            }
            PreparedWindowsSupervisorAction::RestartAfterRunOnceExit => self.restart_after_exit(),
        }
    }

    fn start(&mut self) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        if self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyRunning);
        }
        match self.observe_external_runner()? {
            WindowsRunnerExecutionOutcome::NoopAlreadyExited
            | WindowsRunnerExecutionOutcome::IgnoredUnrelatedRunner => {}
            outcome => return Ok(outcome),
        }
        let process_id = self.port.start(&self.binding.launch())?;
        self.active = Some(ActiveRunner {
            process_id,
            drain_pending: false,
            tracking: ActiveRunnerTracking::OwnedChild,
        });
        Ok(WindowsRunnerExecutionOutcome::Started { process_id })
    }

    fn defer_drain_until_run_once_completion(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        if self.active.is_none() {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        }
        if !self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        }
        let active = self.active.expect("a live runner remains tracked");
        if active.drain_pending {
            return Ok(WindowsRunnerExecutionOutcome::DrainDeferredUntilRunOnceCompletion);
        }
        self.active = Some(ActiveRunner {
            drain_pending: true,
            ..active
        });
        Ok(WindowsRunnerExecutionOutcome::DrainDeferredUntilRunOnceCompletion)
    }

    fn wait_for_run_once_completion(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        if self.active.is_none() {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        }
        if !self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited);
        }
        Ok(WindowsRunnerExecutionOutcome::WaitingForRunOnceCompletion)
    }

    fn restart_after_exit(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        if self.active_runner_is_live()? {
            return Ok(WindowsRunnerExecutionOutcome::RefusedActiveRunOnceMayExist);
        }
        self.start()
    }

    fn active_runner_is_live(&mut self) -> Result<bool, WindowsRunnerExecutorError> {
        let Some(active) = self.active else {
            return Ok(false);
        };
        let exited = match active.tracking {
            ActiveRunnerTracking::OwnedChild => self.port.has_exited(active.process_id)?,
            ActiveRunnerTracking::WaitOnlyExactBound => {
                match self.port.observe_exact_bound_runner(&self.binding)? {
                    BoundRunnerObservation::Absent
                    | BoundRunnerObservation::UnrelatedRunnerPresent => true,
                    BoundRunnerObservation::ExactBoundListener { process_id }
                        if process_id == active.process_id =>
                    {
                        false
                    }
                    BoundRunnerObservation::ExactBoundListener { .. }
                    | BoundRunnerObservation::AmbiguousExecutablePath => {
                        return Err(WindowsRunnerExecutorError::AmbiguousExistingRunner)
                    }
                }
            }
        };
        if exited {
            self.active = None;
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn observe_external_runner(
        &mut self,
    ) -> Result<WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError> {
        match self.port.observe_exact_bound_runner(&self.binding)? {
            BoundRunnerObservation::Absent => Ok(WindowsRunnerExecutionOutcome::NoopAlreadyExited),
            BoundRunnerObservation::UnrelatedRunnerPresent => {
                Ok(WindowsRunnerExecutionOutcome::IgnoredUnrelatedRunner)
            }
            BoundRunnerObservation::AmbiguousExecutablePath => {
                Err(WindowsRunnerExecutorError::AmbiguousExistingRunner)
            }
            BoundRunnerObservation::ExactBoundListener { process_id } => {
                self.active = Some(ActiveRunner {
                    process_id,
                    drain_pending: false,
                    tracking: ActiveRunnerTracking::WaitOnlyExactBound,
                });
                Ok(WindowsRunnerExecutionOutcome::SafeWaitForExactBoundRunner { process_id })
            }
        }
    }
}

/// Native port used by the ordinary user-session executor. The child map is
/// intentionally limited to processes that this instance launched. Independent
/// exact-image observation is read-only and never grants control authority.
#[derive(Default)]
pub struct NativeRunnerProcessPort {
    children: HashMap<u32, std::process::Child>,
}

impl BoundedRunnerProcessPort for NativeRunnerProcessPort {
    fn start(&mut self, launch: &UserSessionLaunch) -> Result<u32, WindowsRunnerExecutorError> {
        let child = Command::new("cmd.exe")
            .args(["/d", "/s", "/c"])
            .arg(&launch.executable)
            .args(&launch.arguments)
            .current_dir(&launch.working_directory)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(io_error("start exact runner entrypoint"))?;
        let process_id = child.id();
        if self.children.insert(process_id, child).is_some() {
            return Err(WindowsRunnerExecutorError::UnexpectedProcessCollision);
        }
        Ok(process_id)
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

    fn observe_exact_bound_runner(
        &mut self,
        binding: &VerifiedRunnerBinding,
    ) -> Result<BoundRunnerObservation, WindowsRunnerExecutorError> {
        let images = crate::process_snapshot::executable_images()
            .map_err(|_| WindowsRunnerExecutorError::ProcessSnapshotUnavailable)?;
        let mut exact_listener = Vec::new();
        let mut exact_worker = Vec::new();
        let mut unrelated = false;
        for image in images.into_iter().filter(|image| {
            image
                .executable_name
                .eq_ignore_ascii_case("Runner.Listener.exe")
                || image
                    .executable_name
                    .eq_ignore_ascii_case("Runner.Worker.exe")
        }) {
            let Some(path) = image.executable_path else {
                return Ok(BoundRunnerObservation::AmbiguousExecutablePath);
            };
            let canonical = canonicalize_win32_path(&path)
                .map_err(io_error("canonicalize runner process image"))?;
            if image
                .executable_name
                .eq_ignore_ascii_case("Runner.Listener.exe")
                && canonical == binding.listener_image()
            {
                exact_listener.push(image.process_id);
            } else if image
                .executable_name
                .eq_ignore_ascii_case("Runner.Worker.exe")
                && canonical == binding.worker_image()
            {
                exact_worker.push(image.process_id);
            } else {
                unrelated = true;
            }
        }
        if exact_listener.len() > 1
            || exact_worker.len() > 1
            || exact_listener.is_empty() && !exact_worker.is_empty()
        {
            return Ok(BoundRunnerObservation::AmbiguousExecutablePath);
        }
        if let Some(process_id) = exact_listener.into_iter().next() {
            return Ok(BoundRunnerObservation::ExactBoundListener { process_id });
        }
        if unrelated {
            Ok(BoundRunnerObservation::UnrelatedRunnerPresent)
        } else {
            Ok(BoundRunnerObservation::Absent)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowsRunnerExecutorError {
    RunnerHomeMissing,
    WorkRootMissing,
    WorkRootMismatch,
    EntrypointMissing,
    EntrypointDrift,
    RegistrationDrift,
    RunnerImageMissing,
    RunnerImageDrift,
    WorkRootIdentityDrift,
    BindingDrift,
    LaunchBindingMismatch,
    UnexpectedProcessCollision,
    UnownedProcess,
    ProcessSnapshotUnavailable,
    AmbiguousExistingRunner,
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
            Self::EntrypointDrift => write!(formatter, "runner run.cmd fingerprint changed"),
            Self::RegistrationDrift => write!(formatter, "runner registration fingerprint changed"),
            Self::RunnerImageMissing => write!(formatter, "runner executable image is missing"),
            Self::RunnerImageDrift => {
                write!(formatter, "runner executable image fingerprint changed")
            }
            Self::WorkRootIdentityDrift => write!(formatter, "runner work-root identity changed"),
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
            Self::ProcessSnapshotUnavailable => {
                write!(formatter, "runner process snapshot unavailable")
            }
            Self::AmbiguousExistingRunner => {
                write!(formatter, "exact runner process ownership is ambiguous")
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
    canonicalize_win32_path(&path).map_err(io_error("canonicalize runner binding"))
}

fn canonical_file(
    path: PathBuf,
    role: &'static str,
) -> Result<PathBuf, WindowsRunnerExecutorError> {
    if !path.is_file() {
        return Err(match role {
            "runner entrypoint" => WindowsRunnerExecutorError::EntrypointMissing,
            "runner listener image" | "runner worker image" => {
                WindowsRunnerExecutorError::RunnerImageMissing
            }
            _ => WindowsRunnerExecutorError::BindingDrift,
        });
    }
    canonicalize_win32_path(&path).map_err(io_error("canonicalize runner binding file"))
}

/// Rust returns verbatim `\\?\` paths from `canonicalize` on Windows. They
/// remain correct filesystem identities, but `cmd.exe` cannot use the
/// verbatim form as a batch-file entrypoint. Keep the binding canonical while
/// returning a normal Win32 spelling for the exact `run.cmd` launch contract.
fn canonicalize_win32_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize().map(win32_compatible_path)
}

fn win32_compatible_path(path: PathBuf) -> PathBuf {
    // `Path::strip_prefix` compares parsed Windows path components. A
    // verbatim path's prefix is intentionally a distinct component, so use
    // the documented textual spelling at this process-creation boundary.
    let spelling = path.to_string_lossy();
    if let Some(unc_tail) = spelling.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc_tail}"));
    }
    if let Some(local_tail) = spelling.strip_prefix(r"\\?\") {
        return PathBuf::from(local_tail);
    }
    path
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
        collections::HashSet,
        fs,
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{
        BoundRunnerObservation, BoundedRunnerProcessPort, PreparedWindowsSupervisorAction,
        UserSessionLaunch, VerifiedRunnerBinding, WindowsOfficialRunnerExecutor,
        WindowsRunnerExecutionOutcome, WindowsRunnerExecutorError, WindowsSupervisorReadiness,
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
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(
            root.join("bin").join("Runner.Listener.exe"),
            "synthetic-listener",
        )
        .unwrap();
        fs::write(
            root.join("bin").join("Runner.Worker.exe"),
            "synthetic-worker",
        )
        .unwrap();
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
                arguments: vec!["--once".to_owned()],
                working_directory: root.clone(),
            })
        );
        assert_eq!(
            adapter.prepare(SupervisorAction::RequestDrain).unwrap(),
            PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion
        );
        assert_eq!(
            adapter.prepare(SupervisorAction::StopAfterDrain).unwrap(),
            PreparedWindowsSupervisorAction::WaitForRunOnceCompletion
        );
        assert_eq!(
            adapter.prepare(SupervisorAction::Stop).unwrap(),
            PreparedWindowsSupervisorAction::RefuseIdleWithdrawal
        );
        assert_eq!(
            adapter
                .prepare(SupervisorAction::RestartConnection)
                .unwrap(),
            PreparedWindowsSupervisorAction::RestartAfterRunOnceExit
        );
        assert_eq!(
            adapter
                .prepare(SupervisorAction::AdoptExistingListener)
                .unwrap(),
            PreparedWindowsSupervisorAction::WaitForVerifiedBoundRunnerExit
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
    fn bounded_executor_launches_run_once_and_defers_busy_drain_without_signals() {
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
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::DrainDeferredUntilRunOnceCompletion
        );
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::DrainDeferredUntilRunOnceCompletion
        );
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::WaitForRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::WaitingForRunOnceCompletion
        );
        assert_eq!(executor.port_mut().starts[0].arguments, vec!["--once"]);

        executor.port_mut().exited.insert(process_id);
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::WaitForRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::NoopAlreadyExited
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completed_run_once_drain_does_not_relaunch_until_full_requests_one_replacement() {
        let (root, binding) = bound_runner();
        let mut executor =
            WindowsOfficialRunnerExecutor::with_port(binding, FakeRunnerPort::default());
        let start = executor.binding().prepared_start();
        let WindowsRunnerExecutionOutcome::Started { process_id } = executor.apply(start).unwrap()
        else {
            panic!("expected the first exact run-once launch");
        };
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::DrainDeferredUntilRunOnceCompletion
        );
        executor.port_mut().exited.insert(process_id);
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::NoopAlreadyExited,
            "a DRAINED target leaves the naturally completed lease offline"
        );
        assert_eq!(executor.port_mut().starts.len(), 1);

        let full = executor.binding().prepared_start();
        assert!(matches!(
            executor.apply(full).unwrap(),
            WindowsRunnerExecutionOutcome::Started { .. }
        ));
        assert_eq!(executor.port_mut().starts.len(), 2);
        let full_again = executor.binding().prepared_start();
        assert_eq!(
            executor.apply(full_again).unwrap(),
            WindowsRunnerExecutionOutcome::NoopAlreadyRunning,
            "a second admission cannot dispatch a second job in the same lease"
        );
        assert_eq!(executor.port_mut().starts.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_reconstruction_waits_for_exact_bound_runner_without_adopting_or_signalling() {
        let (root, binding) = bound_runner();
        let port = FakeRunnerPort {
            observed: BoundRunnerObservation::ExactBoundListener { process_id: 44 },
            ..FakeRunnerPort::default()
        };
        let mut executor = WindowsOfficialRunnerExecutor::with_port(binding, port);
        assert_eq!(
            executor.reconstruct_safely().unwrap(),
            WindowsRunnerExecutionOutcome::SafeWaitForExactBoundRunner { process_id: 44 }
        );
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap(),
            WindowsRunnerExecutionOutcome::DrainDeferredUntilRunOnceCompletion
        );
        assert!(executor.port_mut().starts.is_empty());
        executor.port_mut().observed = BoundRunnerObservation::Absent;
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::WaitForVerifiedBoundRunnerExit)
                .unwrap(),
            WindowsRunnerExecutionOutcome::NoopAlreadyExited
        );
        let start = executor.binding().prepared_start();
        assert!(matches!(
            executor.apply(start).unwrap(),
            WindowsRunnerExecutionOutcome::Started { .. }
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrelated_same_name_runner_does_not_block_the_exact_bound_launch() {
        let (root, binding) = bound_runner();
        let port = FakeRunnerPort {
            observed: BoundRunnerObservation::UnrelatedRunnerPresent,
            ..FakeRunnerPort::default()
        };
        let mut executor = WindowsOfficialRunnerExecutor::with_port(binding, port);
        assert_eq!(
            executor.reconstruct_safely().unwrap(),
            WindowsRunnerExecutionOutcome::IgnoredUnrelatedRunner
        );
        let start = executor.binding().prepared_start();
        assert!(matches!(
            executor.apply(start).unwrap(),
            WindowsRunnerExecutionOutcome::Started { .. }
        ));
        assert_eq!(executor.port_mut().starts.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ambiguous_runner_image_path_refuses_without_launching() {
        let (root, binding) = bound_runner();
        let port = FakeRunnerPort {
            observed: BoundRunnerObservation::AmbiguousExecutablePath,
            ..FakeRunnerPort::default()
        };
        let mut executor = WindowsOfficialRunnerExecutor::with_port(binding, port);
        let start = executor.binding().prepared_start();
        assert_eq!(
            executor.apply(start).unwrap_err(),
            WindowsRunnerExecutorError::AmbiguousExistingRunner
        );
        assert!(executor.port_mut().starts.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_executor_refuses_registration_drift_before_any_control_or_reconstruction() {
        let (root, binding) = bound_runner();
        let mut executor =
            WindowsOfficialRunnerExecutor::with_port(binding, FakeRunnerPort::default());

        fs::write(root.join(".runner"), "changed-registration").unwrap();
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap_err(),
            WindowsRunnerExecutorError::RegistrationDrift
        );
        assert!(executor.port_mut().starts.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_executor_refuses_recreated_work_root_identity() {
        let (root, binding) = bound_runner();
        fs::rename(root.join("_work"), root.join("old-work")).unwrap();
        fs::create_dir(root.join("_work")).unwrap();
        let mut executor =
            WindowsOfficialRunnerExecutor::with_port(binding, FakeRunnerPort::default());
        assert_eq!(
            executor
                .apply(PreparedWindowsSupervisorAction::DeferDrainUntilRunOnceCompletion)
                .unwrap_err(),
            WindowsRunnerExecutorError::WorkRootIdentityDrift
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
            arguments: vec!["--once".to_owned()],
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

    #[test]
    fn prepared_batch_launch_uses_a_cmd_compatible_path() {
        let (root, binding) = bound_runner();
        let PreparedWindowsSupervisorAction::StartUserSession(launch) = binding.prepared_start()
        else {
            panic!("the verified binding must prepare one user-session launch");
        };
        assert!(launch.executable.ends_with("run.cmd"));
        assert_eq!(launch.arguments, vec!["--once"]);
        assert!(
            !launch.executable.to_string_lossy().starts_with(r"\\?\"),
            "cmd.exe must not receive a verbatim batch-file path"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn win32_compatible_path_removes_verbatim_prefixes() {
        assert_eq!(
            super::win32_compatible_path(std::path::PathBuf::from(r"\\?\C:\runner\run.cmd")),
            std::path::PathBuf::from(r"C:\runner\run.cmd")
        );
        assert_eq!(
            super::win32_compatible_path(std::path::PathBuf::from(
                r"\\?\UNC\server\share\runner\run.cmd"
            )),
            std::path::PathBuf::from(r"\\server\share\runner\run.cmd")
        );
    }

    #[test]
    fn busy_drain_api_has_no_console_control_signal() {
        let forbidden = ["CTRL", "+", "BREAK"].concat();
        assert!(
            !include_str!("windows_supervisor.rs").contains(&forbidden),
            "the bounded executor must not reintroduce a console-signal drain API"
        );
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
        exited: HashSet<u32>,
        active: HashSet<u32>,
        observed: BoundRunnerObservation,
    }

    impl BoundedRunnerProcessPort for FakeRunnerPort {
        fn start(&mut self, launch: &UserSessionLaunch) -> Result<u32, WindowsRunnerExecutorError> {
            self.next_process_id += 1;
            let process_id = self.next_process_id;
            self.starts.push(launch.clone());
            self.active.insert(process_id);
            Ok(process_id)
        }

        fn has_exited(&mut self, process_id: u32) -> Result<bool, WindowsRunnerExecutorError> {
            if !self.active.contains(&process_id) {
                return Err(WindowsRunnerExecutorError::UnownedProcess);
            }
            Ok(self.exited.contains(&process_id))
        }

        fn observe_exact_bound_runner(
            &mut self,
            _binding: &VerifiedRunnerBinding,
        ) -> Result<BoundRunnerObservation, WindowsRunnerExecutorError> {
            Ok(self.observed)
        }
    }
}
