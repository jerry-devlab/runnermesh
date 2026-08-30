use std::path::PathBuf;

use crate::{LinkKind, LinkSnapshot, LinkState, ReasonCode, RunnerPhase};

/// Evidence about whether a runner-owned path belongs to the expected execution
/// identity. `Unknown` is intentionally not treated as verified ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnershipEvidence {
    Verified,
    NotOwned,
    Unknown,
}

/// Evidence about the execution identity of observed runner processes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionIdentityEvidence {
    Verified,
    Mismatch,
    Unknown,
}

/// Remote GitHub Actions evidence gathered without controlling the runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionEvidence {
    Connected,
    Connecting,
    Disconnected,
    Insufficient,
    NotConfigured,
}

/// Raw local facts used to derive stable runner/link state. Paths and process
/// details remain local observation data and are not serialized into public
/// snapshots by this Goal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerLocalEvidence {
    pub runner_home: Option<PathBuf>,
    pub metadata_present: bool,
    pub listener_present: bool,
    pub worker_present: bool,
    pub execution_identity: ExecutionIdentityEvidence,
    pub work_root: OwnershipEvidence,
    pub connection: ConnectionEvidence,
}

/// Stable observation result for Agent/CLI/Tray snapshots and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerObservation {
    pub phase: RunnerPhase,
    pub github_link: LinkSnapshot,
    pub runner_home_known: bool,
    pub metadata_present: bool,
    pub execution_identity: ExecutionIdentityEvidence,
    pub work_root: OwnershipEvidence,
}

/// Read-only source boundary for official runner discovery.
pub trait RunnerSource {
    fn collect(&self) -> RunnerLocalEvidence;
}

/// Maps local evidence conservatively. No call in this type starts, stops,
/// drains, registers, reconfigures, or otherwise controls a runner.
pub struct OfficialRunnerObserver<S> {
    source: S,
}

impl<S> OfficialRunnerObserver<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: RunnerSource> OfficialRunnerObserver<S> {
    pub fn observe(&self) -> RunnerObservation {
        let evidence = self.source.collect();
        RunnerObservation {
            phase: derive_phase(&evidence),
            github_link: derive_link(&evidence),
            runner_home_known: evidence.runner_home.is_some(),
            metadata_present: evidence.metadata_present,
            execution_identity: evidence.execution_identity,
            work_root: evidence.work_root,
        }
    }
}

/// Read-only Windows source. It accepts a caller-selected runner home rather
/// than scanning unrelated paths. It reads only existence/metadata evidence and
/// exact executable paths; it does not parse, alter, or expose runner
/// configuration. Same-name processes from another runner home are not local
/// evidence for this selected runner.
#[derive(Clone, Debug)]
pub struct WindowsRunnerSource {
    runner_home: PathBuf,
}

impl WindowsRunnerSource {
    pub fn new(runner_home: impl Into<PathBuf>) -> Self {
        Self {
            runner_home: runner_home.into(),
        }
    }
}

impl RunnerSource for WindowsRunnerSource {
    fn collect(&self) -> RunnerLocalEvidence {
        let home_exists = self.runner_home.is_dir();
        let metadata_present = self.runner_home.join(".runner").is_file();
        let (listener_present, worker_present) = exact_runner_process_presence(&self.runner_home);

        RunnerLocalEvidence {
            runner_home: home_exists.then(|| self.runner_home.clone()),
            metadata_present,
            listener_present,
            worker_present,
            execution_identity: ExecutionIdentityEvidence::Unknown,
            // Directory existence proves neither ownership nor a safe work-root
            // adoption claim; G09/G10 keep that distinction explicit.
            work_root: OwnershipEvidence::Unknown,
            connection: if metadata_present {
                ConnectionEvidence::Insufficient
            } else {
                ConnectionEvidence::NotConfigured
            },
        }
    }
}

#[cfg(windows)]
fn exact_runner_process_presence(runner_home: &std::path::Path) -> (bool, bool) {
    let images = crate::process_snapshot::executable_images().unwrap_or_default();
    bound_process_presence(runner_home, &images)
}

fn bound_process_presence(
    runner_home: &std::path::Path,
    images: &[crate::process_snapshot::ProcessImage],
) -> (bool, bool) {
    let expected_listener = runner_home.join("bin").join("Runner.Listener.exe");
    let expected_worker = runner_home.join("bin").join("Runner.Worker.exe");
    let Ok(expected_listener) = expected_listener.canonicalize() else {
        return (false, false);
    };
    let Ok(expected_worker) = expected_worker.canonicalize() else {
        return (false, false);
    };
    let mut listener_present = false;
    let mut worker_present = false;
    for image in images {
        let Some(path) = image
            .executable_path
            .as_ref()
            .and_then(|path| path.canonicalize().ok())
        else {
            continue;
        };
        if image
            .executable_name
            .eq_ignore_ascii_case("Runner.Listener.exe")
            && path == expected_listener
        {
            listener_present = true;
        }
        if image
            .executable_name
            .eq_ignore_ascii_case("Runner.Worker.exe")
            && path == expected_worker
        {
            worker_present = true;
        }
    }
    (listener_present, worker_present)
}

#[cfg(not(windows))]
fn exact_runner_process_presence(_runner_home: &std::path::Path) -> (bool, bool) {
    (false, false)
}

fn derive_phase(evidence: &RunnerLocalEvidence) -> RunnerPhase {
    match (evidence.listener_present, evidence.worker_present) {
        (true, true) | (false, true) => RunnerPhase::Busy,
        (true, false) => RunnerPhase::Listening,
        (false, false) if evidence.metadata_present => RunnerPhase::Stopped,
        (false, false) => RunnerPhase::Unknown,
    }
}

fn derive_link(evidence: &RunnerLocalEvidence) -> LinkSnapshot {
    let (state, reason) = match evidence.connection {
        ConnectionEvidence::Connected => (LinkState::Connected, "github-link-connected"),
        ConnectionEvidence::Connecting => (LinkState::Connecting, "github-link-connecting"),
        ConnectionEvidence::Disconnected => (LinkState::Disconnected, "github-link-disconnected"),
        ConnectionEvidence::Insufficient => {
            // A local Listener is not proof of an authenticated remote link.
            (LinkState::Unknown, "github-link-insufficient-evidence")
        }
        ConnectionEvidence::NotConfigured => {
            (LinkState::NotConfigured, "github-link-not-configured")
        }
    };
    LinkSnapshot {
        kind: LinkKind::GithubActions,
        state,
        reason_code: Some(static_reason(reason)),
    }
}

fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static runner-observer reason codes must be valid")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{
        ConnectionEvidence, ExecutionIdentityEvidence, OfficialRunnerObserver, OwnershipEvidence,
        RunnerLocalEvidence, RunnerSource,
    };
    use crate::{LinkState, RunnerPhase};

    #[test]
    fn unrelated_same_name_process_images_remain_outside_the_exact_runner_home() {
        let root = temporary_root();
        let target = root.join("target");
        let unrelated = root.join("unrelated");
        for home in [&target, &unrelated] {
            fs::create_dir_all(home.join("bin")).unwrap();
            fs::write(home.join("bin").join("Runner.Listener.exe"), b"fixture").unwrap();
            fs::write(home.join("bin").join("Runner.Worker.exe"), b"fixture").unwrap();
        }
        let images = vec![
            crate::process_snapshot::ProcessImage {
                process_id: 10,
                executable_name: "Runner.Listener.exe".to_owned(),
                executable_path: Some(unrelated.join("bin").join("Runner.Listener.exe")),
            },
            crate::process_snapshot::ProcessImage {
                process_id: 11,
                executable_name: "Runner.Worker.exe".to_owned(),
                executable_path: Some(unrelated.join("bin").join("Runner.Worker.exe")),
            },
        ];

        assert_eq!(
            super::bound_process_presence(&target, &images),
            (false, false)
        );
        assert_eq!(images[0].process_id, 10);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn process_presence_does_not_claim_connected_without_link_evidence() {
        let observed = OfficialRunnerObserver::new(FakeSource(evidence(
            true,
            false,
            ConnectionEvidence::Insufficient,
        )))
        .observe();

        assert_eq!(observed.phase, RunnerPhase::Listening);
        assert_eq!(observed.github_link.state, LinkState::Unknown);
        assert_eq!(
            observed.github_link.reason_code.unwrap().as_str(),
            "github-link-insufficient-evidence"
        );
    }

    #[test]
    fn worker_evidence_is_busy_and_explicit_connection_evidence_is_preserved() {
        let observed = OfficialRunnerObserver::new(FakeSource(evidence(
            false,
            true,
            ConnectionEvidence::Connected,
        )))
        .observe();

        assert_eq!(observed.phase, RunnerPhase::Busy);
        assert_eq!(observed.github_link.state, LinkState::Connected);
        assert_eq!(
            observed.execution_identity,
            ExecutionIdentityEvidence::Unknown
        );
        assert_eq!(observed.work_root, OwnershipEvidence::Unknown);
    }

    #[test]
    fn absent_metadata_is_not_configured_and_does_not_invent_a_runner_phase() {
        let mut local = evidence(false, false, ConnectionEvidence::NotConfigured);
        local.metadata_present = false;
        let observed = OfficialRunnerObserver::new(FakeSource(local)).observe();

        assert_eq!(observed.phase, RunnerPhase::Unknown);
        assert_eq!(observed.github_link.state, LinkState::NotConfigured);
    }

    fn evidence(
        listener_present: bool,
        worker_present: bool,
        connection: ConnectionEvidence,
    ) -> RunnerLocalEvidence {
        RunnerLocalEvidence {
            runner_home: Some(PathBuf::from("runner-home")),
            metadata_present: true,
            listener_present,
            worker_present,
            execution_identity: ExecutionIdentityEvidence::Unknown,
            work_root: OwnershipEvidence::Unknown,
            connection,
        }
    }

    struct FakeSource(RunnerLocalEvidence);

    impl RunnerSource for FakeSource {
        fn collect(&self) -> RunnerLocalEvidence {
            self.0.clone()
        }
    }

    fn temporary_root() -> PathBuf {
        static NEXT_ROOT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("runnermesh-runner-observer-{}-{id}", process::id()));
        fs::create_dir(&root).unwrap();
        root
    }
}
