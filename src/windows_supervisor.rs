//! Windows/user-session supervisor preparation for the H1 boundary.
//!
//! This adapter deliberately prepares and validates operations without
//! executing them. G11 owns qualification of any real official-runner launch,
//! drain, stop, reconnect, or adoption mechanism. The G10R sandbox test below
//! exercises a harmless child process only; it is never wired to the Agent's
//! pre-H1 reconciler.

use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{
        PreparedWindowsSupervisorAction, WindowsSupervisorReadiness,
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
}
