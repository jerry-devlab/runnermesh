use std::path::{Path, PathBuf};

#[cfg(any(windows, test))]
use std::fs;

#[cfg(windows)]
use serde::Deserialize;

use crate::{
    AdmissionBinding, LinkKind, LinkSnapshot, LinkState, ReasonCode, RegistrationScope, RunnerPhase,
};

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
#[derive(Clone)]
pub struct WindowsRunnerSource {
    runner_home: PathBuf,
    exact_binding: Option<ExactRuntimeBinding>,
}

#[derive(Clone)]
struct ExactRuntimeBinding {
    work_root: PathBuf,
    execution_identity_ref: crate::OpaqueIdentityReference,
    admission: AdmissionBinding,
}

impl WindowsRunnerSource {
    pub fn new(runner_home: impl Into<PathBuf>) -> Self {
        Self {
            runner_home: runner_home.into(),
            exact_binding: None,
        }
    }

    pub(crate) fn for_exact_binding(
        local: &crate::ExactLocalRunnerBinding,
        admission: &AdmissionBinding,
    ) -> Self {
        Self {
            runner_home: local.runner_home.clone(),
            exact_binding: Some(ExactRuntimeBinding {
                work_root: local.work_root.clone(),
                execution_identity_ref: local.execution_identity_ref.clone(),
                admission: admission.clone(),
            }),
        }
    }
}

impl RunnerSource for WindowsRunnerSource {
    fn collect(&self) -> RunnerLocalEvidence {
        let home_exists = self.runner_home.is_dir();
        let metadata_present = self.runner_home.join(".runner").is_file();
        let (process, execution_identity, work_root) = match &self.exact_binding {
            Some(binding) => {
                let Ok(guards) = exact_binding_path_guards(&self.runner_home, binding) else {
                    return RunnerLocalEvidence {
                        runner_home: home_exists.then(|| self.runner_home.clone()),
                        metadata_present,
                        listener_present: false,
                        worker_present: false,
                        execution_identity: ExecutionIdentityEvidence::Unknown,
                        work_root: OwnershipEvidence::Unknown,
                        connection: if metadata_present {
                            ConnectionEvidence::Insufficient
                        } else {
                            ConnectionEvidence::NotConfigured
                        },
                    };
                };
                let process = exact_runner_process_observation(&self.runner_home, binding);
                let metadata = observe_bound_metadata(&self.runner_home, binding);
                let execution_identity = match (process.execution_identity, metadata) {
                    (_, BoundMetadataObservation::MismatchIdentity) => {
                        ExecutionIdentityEvidence::Mismatch
                    }
                    (ExecutionIdentityEvidence::Verified, BoundMetadataObservation::Verified) => {
                        ExecutionIdentityEvidence::Verified
                    }
                    (ExecutionIdentityEvidence::Mismatch, _) => ExecutionIdentityEvidence::Mismatch,
                    _ => ExecutionIdentityEvidence::Unknown,
                };
                let work_root = match metadata {
                    BoundMetadataObservation::MismatchWorkRoot => OwnershipEvidence::NotOwned,
                    BoundMetadataObservation::Verified => {
                        current_user_owns_runner_paths(&self.runner_home, &binding.work_root)
                    }
                    _ => OwnershipEvidence::Unknown,
                };
                if guards.iter().any(|guard| guard.verify().is_err()) {
                    (
                        process,
                        ExecutionIdentityEvidence::Unknown,
                        OwnershipEvidence::Unknown,
                    )
                } else {
                    (process, execution_identity, work_root)
                }
            }
            None => (
                exact_runner_process_observation_unbound(&self.runner_home),
                ExecutionIdentityEvidence::Unknown,
                OwnershipEvidence::Unknown,
            ),
        };
        RunnerLocalEvidence {
            runner_home: home_exists.then(|| self.runner_home.clone()),
            metadata_present,
            listener_present: process.listener_present,
            worker_present: process.worker_present,
            execution_identity,
            work_root,
            connection: if metadata_present {
                ConnectionEvidence::Insufficient
            } else {
                ConnectionEvidence::NotConfigured
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ExactProcessObservation {
    listener_present: bool,
    worker_present: bool,
    execution_identity: ExecutionIdentityEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundMetadataObservation {
    Verified,
    MismatchIdentity,
    MismatchWorkRoot,
    Unknown,
}

#[cfg(windows)]
#[derive(Deserialize)]
struct OfficialRunnerMetadata {
    #[serde(rename = "agentId", alias = "AgentId")]
    agent_id: u64,
    #[serde(rename = "agentName", alias = "AgentName")]
    agent_name: String,
    #[serde(rename = "gitHubUrl", alias = "GitHubUrl")]
    github_url: String,
    #[serde(rename = "workFolder", alias = "WorkFolder")]
    work_folder: String,
}

#[cfg(windows)]
fn observe_bound_metadata(
    runner_home: &Path,
    binding: &ExactRuntimeBinding,
) -> BoundMetadataObservation {
    use std::os::windows::{fs::MetadataExt, fs::OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

    let path = runner_home.join(".runner");
    let Ok(mut file) = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
    else {
        return BoundMetadataObservation::Unknown;
    };
    let Ok(metadata) = file.metadata() else {
        return BoundMetadataObservation::Unknown;
    };
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 || metadata.len() > 128 * 1024
    {
        return BoundMetadataObservation::Unknown;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    if std::io::Read::read_to_end(&mut file, &mut bytes).is_err() {
        return BoundMetadataObservation::Unknown;
    }
    let Ok(metadata) = serde_json::from_slice::<OfficialRunnerMetadata>(&bytes) else {
        return BoundMetadataObservation::Unknown;
    };
    if metadata.agent_id != binding.admission.runner_id
        || metadata.agent_name != binding.admission.runner_name
        || !github_url_matches_scope(&metadata.github_url, &binding.admission.scope)
    {
        return BoundMetadataObservation::MismatchIdentity;
    }
    let configured_work_root = PathBuf::from(metadata.work_folder);
    let configured_work_root = if configured_work_root.is_absolute() {
        configured_work_root
    } else {
        runner_home.join(configured_work_root)
    };
    let (Ok(configured), Ok(expected)) = (
        configured_work_root.canonicalize(),
        binding.work_root.canonicalize(),
    ) else {
        return BoundMetadataObservation::Unknown;
    };
    if !configured
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        return BoundMetadataObservation::MismatchWorkRoot;
    }
    BoundMetadataObservation::Verified
}

#[cfg(not(windows))]
fn observe_bound_metadata(
    _runner_home: &Path,
    _binding: &ExactRuntimeBinding,
) -> BoundMetadataObservation {
    BoundMetadataObservation::Unknown
}

fn github_url_matches_scope(github_url: &str, scope: &RegistrationScope) -> bool {
    let expected = match scope {
        RegistrationScope::Organization { organization } => {
            format!("https://github.com/{organization}")
        }
        RegistrationScope::Repository { owner, repository } => {
            format!("https://github.com/{owner}/{repository}")
        }
    };
    github_url
        .strip_suffix('/')
        .unwrap_or(github_url)
        .eq_ignore_ascii_case(&expected)
}

#[cfg(windows)]
fn exact_binding_path_guards(
    runner_home: &Path,
    binding: &ExactRuntimeBinding,
) -> Result<Vec<crate::installation::ExistingDirectoryGuards>, ()> {
    for file in [
        runner_home.join(".runner"),
        runner_home.join("bin").join("Runner.Listener.exe"),
        runner_home.join("bin").join("Runner.Worker.exe"),
    ] {
        if !file.is_file() || crate::installation::is_reparse_point(&file).map_err(|_| ())? {
            return Err(());
        }
    }
    [runner_home, binding.work_root.as_path()]
        .into_iter()
        .map(|path| crate::installation::guard_existing_directories(path).map_err(|_| ()))
        .collect()
}

#[cfg(not(windows))]
fn exact_binding_path_guards(
    _runner_home: &Path,
    _binding: &ExactRuntimeBinding,
) -> Result<Vec<crate::installation::ExistingDirectoryGuards>, ()> {
    Err(())
}

#[cfg(windows)]
fn current_user_owns_runner_paths(runner_home: &Path, work_root: &Path) -> OwnershipEvidence {
    match (
        current_user_owns_path(runner_home),
        current_user_owns_path(work_root),
    ) {
        (Ok(true), Ok(true)) => OwnershipEvidence::Verified,
        (Ok(false), _) | (_, Ok(false)) => OwnershipEvidence::NotOwned,
        _ => OwnershipEvidence::Unknown,
    }
}

#[cfg(not(windows))]
fn current_user_owns_runner_paths(_runner_home: &Path, _work_root: &Path) -> OwnershipEvidence {
    OwnershipEvidence::Unknown
}

#[cfg(windows)]
fn current_user_owns_path(path: &Path) -> Result<bool, ()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE},
        Security::{
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            EqualSid, GetTokenInformation, TokenUser, OWNER_SECURITY_INFORMATION,
            PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY, TOKEN_USER,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    struct OwnedDescriptor(PSECURITY_DESCRIPTOR);
    impl Drop for OwnedDescriptor {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0.cast());
            }
        }
    }

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut owner: PSID = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS || descriptor.is_null() || owner.is_null() {
        return Err(());
    }
    let _descriptor = OwnedDescriptor(descriptor);
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(());
    }
    let token = OwnedHandle(token);
    let mut required = 0_u32;
    unsafe {
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut required);
    }
    if required == 0 {
        return Err(());
    }
    let unit = std::mem::size_of::<usize>();
    let mut buffer = vec![0_usize; (required as usize).div_ceil(unit)];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(());
    }
    let current_sid = unsafe { (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    Ok(unsafe { EqualSid(owner, current_sid) } != 0)
}

#[cfg(windows)]
fn exact_runner_process_observation(
    runner_home: &std::path::Path,
    binding: &ExactRuntimeBinding,
) -> ExactProcessObservation {
    let images = crate::process_snapshot::executable_images().unwrap_or_default();
    let (listener_present, worker_present) = bound_process_presence(runner_home, &images);
    match crate::process_snapshot::current_user_matches_identity_reference(
        &binding.execution_identity_ref,
    ) {
        Ok(true) => {}
        Ok(false) => {
            return ExactProcessObservation {
                listener_present,
                worker_present,
                execution_identity: ExecutionIdentityEvidence::Mismatch,
            };
        }
        Err(_) => {
            return ExactProcessObservation {
                listener_present,
                worker_present,
                execution_identity: ExecutionIdentityEvidence::Unknown,
            };
        }
    }
    let expected_listener = runner_home.join("bin").join("Runner.Listener.exe");
    let expected_worker = runner_home.join("bin").join("Runner.Worker.exe");
    let (Ok(expected_listener), Ok(expected_worker)) = (
        expected_listener.canonicalize(),
        expected_worker.canonicalize(),
    ) else {
        return ExactProcessObservation {
            listener_present,
            worker_present,
            execution_identity: ExecutionIdentityEvidence::Unknown,
        };
    };
    let mut matched = 0_usize;
    let mut unknown = false;
    for image in &images {
        let Some(path) = image
            .executable_path
            .as_ref()
            .and_then(|path| path.canonicalize().ok())
        else {
            continue;
        };
        if path != expected_listener && path != expected_worker {
            continue;
        }
        matched += 1;
        match image.user_matches_current {
            Some(true) => {}
            Some(false) => {
                return ExactProcessObservation {
                    listener_present,
                    worker_present,
                    execution_identity: ExecutionIdentityEvidence::Mismatch,
                };
            }
            None => unknown = true,
        }
    }
    ExactProcessObservation {
        listener_present,
        worker_present,
        execution_identity: if matched == 0 || unknown {
            ExecutionIdentityEvidence::Unknown
        } else {
            ExecutionIdentityEvidence::Verified
        },
    }
}

#[cfg(windows)]
fn exact_runner_process_observation_unbound(
    runner_home: &std::path::Path,
) -> ExactProcessObservation {
    let images = crate::process_snapshot::executable_images().unwrap_or_default();
    let (listener_present, worker_present) = bound_process_presence(runner_home, &images);
    ExactProcessObservation {
        listener_present,
        worker_present,
        execution_identity: ExecutionIdentityEvidence::Unknown,
    }
}

#[cfg(any(windows, test))]
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
fn exact_runner_process_observation(
    _runner_home: &std::path::Path,
    _binding: &ExactRuntimeBinding,
) -> ExactProcessObservation {
    ExactProcessObservation {
        listener_present: false,
        worker_present: false,
        execution_identity: ExecutionIdentityEvidence::Unknown,
    }
}

#[cfg(not(windows))]
fn exact_runner_process_observation_unbound(
    _runner_home: &std::path::Path,
) -> ExactProcessObservation {
    ExactProcessObservation {
        listener_present: false,
        worker_present: false,
        execution_identity: ExecutionIdentityEvidence::Unknown,
    }
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

    #[cfg(windows)]
    use crate::{ExactLocalRunnerBinding, OpaqueIdentityReference};

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
                user_matches_current: Some(true),
            },
            crate::process_snapshot::ProcessImage {
                process_id: 11,
                executable_name: "Runner.Worker.exe".to_owned(),
                executable_path: Some(unrelated.join("bin").join("Runner.Worker.exe")),
                user_matches_current: Some(true),
            },
        ];

        assert_eq!(
            super::bound_process_presence(&target, &images),
            (false, false)
        );
        assert_eq!(images[0].process_id, 10);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn exact_runtime_binding_verifies_metadata_and_current_user_owned_paths() {
        let root = temporary_root();
        let runner_home = root.join("runner");
        let work_root = runner_home.join("_work");
        fs::create_dir_all(runner_home.join("bin")).unwrap();
        fs::create_dir_all(&work_root).unwrap();
        fs::write(
            runner_home.join("bin").join("Runner.Listener.exe"),
            b"fixture",
        )
        .unwrap();
        fs::write(
            runner_home.join("bin").join("Runner.Worker.exe"),
            b"fixture",
        )
        .unwrap();
        fs::write(
            runner_home.join(".runner"),
            br#"{"AgentId":42,"AgentName":"fixture-runner","GitHubUrl":"https://github.com/fixture-owner/fixture-repository","WorkFolder":"_work","other":"tolerated"}"#,
        )
        .unwrap();
        let scope = crate::RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: "fixture-repository".to_owned(),
        };
        let admission = crate::AdmissionBinding::new(
            scope.clone(),
            42,
            "fixture-runner",
            crate::CredentialReference::new("windows-credential-manager", "fixture-credential")
                .unwrap(),
            Some(crate::ReservedLabelOwnership::for_runner(scope, 42)),
        )
        .unwrap();
        let local = ExactLocalRunnerBinding::new(
            &runner_home,
            &work_root,
            OpaqueIdentityReference::new(
                crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
                crate::process_snapshot::current_user_identity_sha256().unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let observed = super::WindowsRunnerSource::for_exact_binding(&local, &admission).collect();
        assert_eq!(observed.work_root, OwnershipEvidence::Verified);
        assert_eq!(
            observed.execution_identity,
            ExecutionIdentityEvidence::Unknown
        );

        fs::write(
            runner_home.join(".runner"),
            br#"{"agentId":42,"agentName":"other-runner","gitHubUrl":"https://github.com/fixture-owner/fixture-repository","workFolder":"_work"}"#,
        )
        .unwrap();
        let drift = super::WindowsRunnerSource::for_exact_binding(&local, &admission).collect();
        assert_eq!(
            drift.execution_identity,
            ExecutionIdentityEvidence::Mismatch
        );

        fs::write(
            runner_home.join(".runner"),
            br#"{"agentId":43,"agentName":"fixture-runner","gitHubUrl":"https://github.com/fixture-owner/fixture-repository","workFolder":"_work"}"#,
        )
        .unwrap();
        let id_drift = super::WindowsRunnerSource::for_exact_binding(&local, &admission).collect();
        assert_eq!(
            id_drift.execution_identity,
            ExecutionIdentityEvidence::Mismatch
        );

        fs::write(
            runner_home.join(".runner"),
            br#"{"agentId":42,"agentName":"fixture-runner","gitHubUrl":"https://github.com/fixture-owner/other-repository","workFolder":"_work"}"#,
        )
        .unwrap();
        let scope_drift =
            super::WindowsRunnerSource::for_exact_binding(&local, &admission).collect();
        assert_eq!(
            scope_drift.execution_identity,
            ExecutionIdentityEvidence::Mismatch
        );
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
