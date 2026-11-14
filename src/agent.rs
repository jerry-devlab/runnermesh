use std::{
    cell::RefCell,
    collections::BTreeMap,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    decide_admission, handle_runners_command, AdmissionControlSnapshot, AdmissionDecision,
    AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, BuildProvenance,
    DesiredAdmissionState, DiscoveredLifecycle, DiscoveryReport, DoctorCheck, DoctorReport,
    DoctorStatus, ExecutionIdentityEvidence, GlobalPolicyInputs, HardSafetyState, LinkKind,
    LinkSnapshot, LinkState, MemoryRegistryStore, MultiRunnerReconcileReport, OwnershipEvidence,
    PerRunnerAchieved, PerRunnerDesired, PerRunnerObservation, PerRunnerPlan, PrivilegedScmAction,
    PrivilegedScmHelper, PrivilegedScmReport, PrivilegedScmRequest, ProbeId, ProbeRuntimeState,
    ProbeSnapshot, ReasonCode, RegistryBackendBinding, RegistryError, RegistryStore, RunnerPhase,
    RunnerPresentationStatus, RunnerRecord, RunnerRegistry, RunnerRegistryEditor,
    ScmControlBackend, SystemPreferences, UiPreferences, UserMode, ZenOverride,
};

/// The currently supported persisted Agent configuration schema.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Persistent user intent. Runtime process, link, probe, host, and derived
/// admission observations deliberately do not appear here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub schema_version: u32,
    pub user_mode: UserMode,
    pub zen: ZenOverride,
    pub probe_enabled: BTreeMap<ProbeId, bool>,
    pub auto_idle_threshold_seconds: u64,
    pub ui_preferences: UiPreferences,
    pub start_on_login: bool,
    pub update_checks_enabled: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            user_mode: UserMode::Auto,
            zen: ZenOverride::Disabled,
            probe_enabled: BTreeMap::new(),
            auto_idle_threshold_seconds: 300,
            ui_preferences: UiPreferences::default(),
            start_on_login: false,
            update_checks_enabled: true,
        }
    }
}

/// Reconstructable observation provided by a platform-specific observer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentObservation {
    pub health: AgentHealth,
    pub health_reason_code: Option<ReasonCode>,
    pub hard_safety: HardSafetyState,
    pub runner_phase: RunnerPhase,
    pub execution_identity: ExecutionIdentityEvidence,
    pub work_root: OwnershipEvidence,
    pub admission_control: AdmissionControlSnapshot,
    pub links: Vec<LinkSnapshot>,
    pub probes: Vec<ProbeSnapshot>,
    /// Current-user presentation facts gathered read-only by the platform
    /// observer. They remain reconstructable observation rather than intent.
    pub system_preferences: SystemPreferences,
}

impl AgentObservation {
    pub fn unobserved() -> Self {
        Self {
            health: AgentHealth::Starting,
            health_reason_code: Some(static_reason("agent-starting")),
            hard_safety: HardSafetyState::Unknown,
            runner_phase: RunnerPhase::Unknown,
            execution_identity: ExecutionIdentityEvidence::Unknown,
            work_root: OwnershipEvidence::Unknown,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: Some(static_reason("not-observed")),
            }],
            probes: Vec::new(),
            system_preferences: SystemPreferences::default(),
        }
    }
}

/// Boundary for gathering reconstructable state. G03 implementations are
/// synthetic; platform observers are introduced by later Goals.
pub trait AgentObserver {
    fn observe(&mut self) -> Result<AgentObservation, String>;
}

/// Boundary for applying the Agent's desired admission intent. G03 uses only
/// synthetic implementations and does not control an official runner.
pub trait AgentReconciler {
    fn reconcile(
        &mut self,
        decision: &AdmissionDecision,
        observation: &AgentObservation,
    ) -> Result<AdmissionControlSnapshot, String>;
}

/// Persistent configuration storage boundary.
pub trait ConfigStore {
    fn load(&self) -> Result<Option<AgentConfig>, ConfigStoreError>;
    fn save(&self, config: &AgentConfig) -> Result<(), ConfigStoreError>;
}

/// Failures from schema-validated, atomic config persistence.
#[derive(Debug)]
pub enum ConfigStoreError {
    Io(io::Error),
    Json(serde_json::Error),
    IncompatibleSchema { found: u32, supported: u32 },
}

impl fmt::Display for ConfigStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "configuration I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "configuration JSON failed: {error}"),
            Self::IncompatibleSchema { found, supported } => {
                write!(
                    formatter,
                    "configuration schema {found} is incompatible with {supported}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::IncompatibleSchema { .. } => None,
        }
    }
}

/// Source-development file store. The caller chooses its path; no production
/// location is implied by this type.
#[derive(Clone, Debug)]
pub struct FileConfigStore {
    path: PathBuf,
}

impl FileConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ConfigStore for FileConfigStore {
    fn load(&self) -> Result<Option<AgentConfig>, ConfigStoreError> {
        let guards = crate::installation::guard_existing_directories(&self.path)
            .map_err(|error| ConfigStoreError::Io(io::Error::other(error.to_string())))?;
        if crate::installation::is_reparse_point(&self.path)
            .map_err(|error| ConfigStoreError::Io(io::Error::other(error.to_string())))?
        {
            return Err(ConfigStoreError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "configuration path is a reparse point",
            )));
        }
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ConfigStoreError::Io(error)),
        };
        guards
            .verify()
            .map_err(|error| ConfigStoreError::Io(io::Error::other(error.to_string())))?;
        let config: AgentConfig = serde_json::from_slice(&bytes).map_err(ConfigStoreError::Json)?;
        validate_schema(&config)?;
        Ok(Some(config))
    }

    fn save(&self, config: &AgentConfig) -> Result<(), ConfigStoreError> {
        validate_schema(config)?;
        let guards = crate::installation::guard_existing_directories(&self.path)
            .map_err(|error| ConfigStoreError::Io(io::Error::other(error.to_string())))?;
        if crate::installation::is_reparse_point(&self.path)
            .map_err(|error| ConfigStoreError::Io(io::Error::other(error.to_string())))?
        {
            return Err(ConfigStoreError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "configuration path is a reparse point",
            )));
        }
        let bytes = serde_json::to_vec_pretty(config).map_err(ConfigStoreError::Json)?;
        write_bytes_atomically_with(&self.path, &bytes, |_| {
            guards
                .verify()
                .map_err(|error| io::Error::other(error.to_string()))
        })
        .map_err(ConfigStoreError::Io)?;
        guards
            .verify()
            .map_err(|error| ConfigStoreError::Io(io::Error::other(error.to_string())))
    }
}

/// In-memory store for deterministic Agent Core tests and synthetic hosts.
#[derive(Debug, Default)]
pub struct MemoryConfigStore {
    config: RefCell<Option<AgentConfig>>,
}

impl MemoryConfigStore {
    pub fn new(initial: AgentConfig) -> Self {
        Self {
            config: RefCell::new(Some(initial)),
        }
    }

    pub fn persisted(&self) -> Option<AgentConfig> {
        self.config.borrow().clone()
    }
}

impl ConfigStore for MemoryConfigStore {
    fn load(&self) -> Result<Option<AgentConfig>, ConfigStoreError> {
        Ok(self.config.borrow().clone())
    }

    fn save(&self, config: &AgentConfig) -> Result<(), ConfigStoreError> {
        validate_schema(config)?;
        *self.config.borrow_mut() = Some(config.clone());
        Ok(())
    }
}

/// Pluggable multi-runner lifecycle backend driving runner observation,
/// admission selector withdrawal, and narrow privileged SCM dispatch.
pub trait MultiRunnerLifecycleBackend: Send + Sync {
    /// Gathers live or simulated observations for the enrolled runner keys.
    fn observe_runners(&self, local_runner_keys: &[String]) -> Vec<PerRunnerObservation>;

    /// Dispatches a single scoped SCM control action to the target runner's service.
    fn dispatch_scm(&self, request: &PrivilegedScmRequest) -> PrivilegedScmReport;

    /// Coordinates withdrawal of the reserved admission selector for the given scope and runner.
    fn withdraw_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String>;

    /// Coordinates advertisement of the reserved admission selector for the given scope and runner.
    fn advertise_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String>;

    /// Dispatches an owned user-session action.
    fn dispatch_session(
        &self,
        record: &RunnerRecord,
        action: crate::runner_session_backend::PreparedSessionAction,
    ) -> Result<bool, String> {
        let _ = (record, action);
        Ok(false)
    }

    /// Optional target registration so multi-runner observers can map local keys to remote scopes.
    fn register_runner_target(
        &self,
        local_runner_key: &str,
        scope: crate::RegistrationScope,
        runner_id: u64,
    ) {
        self.register_runner_target_with_service(local_runner_key, scope, runner_id, None);
    }

    /// Target registration including optional real service name so SCM queries use the real service name.
    fn register_runner_target_with_service(
        &self,
        local_runner_key: &str,
        scope: crate::RegistrationScope,
        runner_id: u64,
        service_name: Option<&str>,
    ) {
        self.register_runner_target_detailed(
            local_runner_key,
            scope,
            runner_id,
            service_name,
            None,
            false,
        );
    }

    /// Target registration directly from an enrolled RunnerRecord.
    fn register_runner_record(&self, record: &RunnerRecord) {
        let service_name = match &record.backend {
            RegistryBackendBinding::WindowsService { service_name } => Some(service_name.as_str()),
            _ => None,
        };
        let is_user_session = matches!(&record.backend, RegistryBackendBinding::UserSession);
        self.register_runner_target_detailed(
            &record.local_runner_key,
            record.registration_scope.clone(),
            record.runner_id,
            service_name,
            Some(&record.runner_home),
            is_user_session,
        );
    }

    /// Comprehensive target registration with home path and session kind.
    fn register_runner_target_detailed(
        &self,
        _local_runner_key: &str,
        _scope: crate::RegistrationScope,
        _runner_id: u64,
        _service_name: Option<&str>,
        _runner_home: Option<&Path>,
        _is_user_session: bool,
    ) {
    }
}

/// Thread-safe sandbox multi-runner lifecycle backend for tests and simulated qualification.
pub struct SandboxMultiRunnerLifecycleBackend<B> {
    helper: PrivilegedScmHelper<B>,
    observations: Arc<Mutex<Vec<PerRunnerObservation>>>,
    withdrawn_selectors: Arc<Mutex<Vec<(crate::RegistrationScope, u64)>>>,
    advertised_selectors: Arc<Mutex<Vec<(crate::RegistrationScope, u64)>>>,
    dispatched_session_actions:
        Arc<Mutex<Vec<(String, crate::runner_session_backend::PreparedSessionAction)>>>,
}

impl<B: ScmControlBackend + Send + Sync + 'static> SandboxMultiRunnerLifecycleBackend<B> {
    pub fn new(helper: PrivilegedScmHelper<B>) -> Self {
        Self {
            helper,
            observations: Arc::new(Mutex::new(Vec::new())),
            withdrawn_selectors: Arc::new(Mutex::new(Vec::new())),
            advertised_selectors: Arc::new(Mutex::new(Vec::new())),
            dispatched_session_actions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_observations(&self, obs: Vec<PerRunnerObservation>) {
        *self.observations.lock().unwrap_or_else(|p| p.into_inner()) = obs;
    }

    pub fn withdrawn_selectors(&self) -> Vec<(crate::RegistrationScope, u64)> {
        self.withdrawn_selectors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn advertised_selectors(&self) -> Vec<(crate::RegistrationScope, u64)> {
        self.advertised_selectors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn dispatched_session_actions(
        &self,
    ) -> Vec<(String, crate::runner_session_backend::PreparedSessionAction)> {
        self.dispatched_session_actions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl<B: ScmControlBackend + Send + Sync + 'static> MultiRunnerLifecycleBackend
    for SandboxMultiRunnerLifecycleBackend<B>
{
    fn observe_runners(&self, _local_runner_keys: &[String]) -> Vec<PerRunnerObservation> {
        self.observations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn dispatch_scm(&self, request: &PrivilegedScmRequest) -> PrivilegedScmReport {
        self.helper.dispatch(request)
    }

    fn withdraw_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        self.withdrawn_selectors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((scope.clone(), runner_id));
        Ok(true)
    }

    fn advertise_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        self.advertised_selectors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((scope.clone(), runner_id));
        Ok(true)
    }

    fn dispatch_session(
        &self,
        record: &RunnerRecord,
        action: crate::runner_session_backend::PreparedSessionAction,
    ) -> Result<bool, String> {
        self.dispatched_session_actions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((record.local_runner_key.clone(), action));
        Ok(true)
    }
}

/// Coordinator for observing and applying GitHub admission selectors for multiple runners.
pub trait MultiRunnerAdmissionCoordinator: Send + Sync {
    /// Observes the admission selector state for a runner. Returns Some(true) if present,
    /// Some(false) if absent, or None if unknown/unmanaged.
    fn observe_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<Option<bool>, String>;

    /// Advertises the admission selector for the given scope and runner.
    fn advertise_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String>;

    /// Withdraws the admission selector for the given scope and runner.
    fn withdraw_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String>;
}

/// Coordinator for owned user-session runners.
pub trait MultiRunnerSessionCoordinator: Send + Sync {
    /// Dispatches a prepared user-session action for the given runner record.
    fn dispatch_session(
        &self,
        record: &RunnerRecord,
        action: crate::runner_session_backend::PreparedSessionAction,
    ) -> Result<bool, String>;
}

/// Injectable process transport for owned user session runners.
pub trait UserSessionProcessTransport: Send + Sync {
    fn launch_listener(&self, record: &RunnerRecord) -> Result<bool, String>;
    fn request_graceful_stop(&self, record: &RunnerRecord) -> Result<bool, String>;
}

/// Production default process transport for user session runners (sealed closed).
#[derive(Clone, Debug, Default)]
pub struct InertUserSessionProcessTransport;

impl UserSessionProcessTransport for InertUserSessionProcessTransport {
    fn launch_listener(&self, _record: &RunnerRecord) -> Result<bool, String> {
        Err("session.transport-closed-production-inert".to_owned())
    }

    fn request_graceful_stop(&self, _record: &RunnerRecord) -> Result<bool, String> {
        Err("session.transport-closed-production-inert".to_owned())
    }
}

/// Simulated user session process transport for tests and qualification.
#[derive(Clone, Debug, Default)]
pub struct SimulatedUserSessionProcessTransport {
    pub launched: Arc<Mutex<Vec<String>>>,
    pub stopped: Arc<Mutex<Vec<String>>>,
}

impl SimulatedUserSessionProcessTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UserSessionProcessTransport for SimulatedUserSessionProcessTransport {
    fn launch_listener(&self, record: &RunnerRecord) -> Result<bool, String> {
        self.launched
            .lock()
            .unwrap()
            .push(record.local_runner_key.clone());
        Ok(true)
    }

    fn request_graceful_stop(&self, record: &RunnerRecord) -> Result<bool, String> {
        self.stopped
            .lock()
            .unwrap()
            .push(record.local_runner_key.clone());
        Ok(true)
    }
}

/// Standard fail-closed user session coordinator that verifies execution identity,
/// home directory, run.cmd entrypoint, work root ownership, and process state.
pub struct StandardSessionCoordinator {
    transport: Arc<dyn UserSessionProcessTransport>,
}

impl Default for StandardSessionCoordinator {
    fn default() -> Self {
        Self {
            transport: Arc::new(InertUserSessionProcessTransport),
        }
    }
}

impl StandardSessionCoordinator {
    pub fn new(transport: Arc<dyn UserSessionProcessTransport>) -> Self {
        Self { transport }
    }

    pub fn inert() -> Self {
        Self::default()
    }
}

impl MultiRunnerSessionCoordinator for StandardSessionCoordinator {
    fn dispatch_session(
        &self,
        record: &RunnerRecord,
        action: crate::runner_session_backend::PreparedSessionAction,
    ) -> Result<bool, String> {
        if !matches!(record.backend, RegistryBackendBinding::UserSession) {
            return Err("session.not-user-session-backend".to_owned());
        }
        if !record.runner_home.is_dir() {
            return Err("session.runner-home-missing".to_owned());
        }
        let entrypoint = record.runner_home.join("run.cmd");
        if !entrypoint.is_file() {
            return Err("session.entrypoint-missing".to_owned());
        }
        if !record.work_root.exists()
            && !record
                .work_root
                .parent()
                .map(|p| p.is_dir())
                .unwrap_or(false)
        {
            return Err("session.work-root-not-owned".to_owned());
        }

        // Reparse point boundary checks (fail-closed)
        #[cfg(windows)]
        {
            if crate::installation::is_reparse_point(&record.runner_home).unwrap_or(false) {
                return Err("session.runner-home-reparse-point".to_owned());
            }
            if record.work_root.exists()
                && crate::installation::is_reparse_point(&record.work_root).unwrap_or(false)
            {
                return Err("session.work-root-reparse-point".to_owned());
            }
        }

        // Fresh process observation
        let (listener_active, worker_active, _alive) =
            crate::windows_runner_discovery::observe_runner_processes_for_home(&record.runner_home);

        match action {
            crate::runner_session_backend::PreparedSessionAction::RequestGracefulStop => {
                if worker_active {
                    return Err("session.busy-worker".to_owned());
                }
                if !listener_active {
                    return Ok(true);
                }
                self.transport.request_graceful_stop(record)
            }
            crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener => {
                if worker_active {
                    return Err("session.busy-worker".to_owned());
                }
                if listener_active {
                    return Ok(true);
                }
                #[cfg(windows)]
                {
                    let supervisor = crate::WindowsUserSessionSupervisorAdapter::for_runner_home(
                        &record.runner_home,
                    );
                    if supervisor.readiness() != crate::WindowsSupervisorReadiness::Ready {
                        return Err("session.readiness-check-failed".to_owned());
                    }
                }
                self.transport.launch_listener(record)
            }
            crate::runner_session_backend::PreparedSessionAction::AdoptVerifiedListener => {
                if worker_active {
                    return Err("session.busy-worker".to_owned());
                }
                if !listener_active {
                    return Err("session.no-listener-to-adopt".to_owned());
                }
                Ok(true)
            }
        }
    }
}

/// Multi-runner admission coordinator bound to an authentic AdmissionControlBackend.
pub struct BoundRunnerAdmissionCoordinator<B> {
    scope: crate::RegistrationScope,
    runner_id: u64,
    backend: Arc<Mutex<B>>,
}

impl<B: crate::AdmissionControlBackend> BoundRunnerAdmissionCoordinator<B> {
    pub fn new(scope: crate::RegistrationScope, runner_id: u64, backend: B) -> Self {
        Self {
            scope,
            runner_id,
            backend: Arc::new(Mutex::new(backend)),
        }
    }
}

impl<B: crate::AdmissionControlBackend + Send + Sync> MultiRunnerAdmissionCoordinator
    for BoundRunnerAdmissionCoordinator<B>
{
    fn observe_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<Option<bool>, String> {
        if *scope != self.scope || runner_id != self.runner_id {
            return Err(format!(
                "admission.scope-or-runner-id-mismatch: expected ({:?}, {}), got ({scope:?}, {runner_id})",
                self.scope, self.runner_id
            ));
        }
        let mut guard = self.backend.lock().map_err(|e| e.to_string())?;
        match guard.observe_admission_selector() {
            Ok(obs) => Ok(match obs.selector {
                crate::AdmissionSelectorState::Present => Some(true),
                crate::AdmissionSelectorState::Absent => Some(false),
                crate::AdmissionSelectorState::Unknown => None,
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    fn advertise_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        if *scope != self.scope || runner_id != self.runner_id {
            return Err("admission.scope-or-runner-id-mismatch".to_owned());
        }
        let mut guard = self.backend.lock().map_err(|e| e.to_string())?;
        match guard.advertise_capacity() {
            Ok(obs) => Ok(obs.selector == crate::AdmissionSelectorState::Present),
            Err(e) => Err(e.to_string()),
        }
    }

    fn withdraw_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        if *scope != self.scope || runner_id != self.runner_id {
            return Err("admission.scope-or-runner-id-mismatch".to_owned());
        }
        let mut guard = self.backend.lock().map_err(|e| e.to_string())?;
        match guard.withdraw_capacity() {
            Ok(obs) => Ok(obs.selector == crate::AdmissionSelectorState::Absent),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// In-process stateful admission coordinator for testing, qualification, and sandbox environments.
#[derive(Clone, Default)]
pub struct InProcessAdmissionCoordinator {
    selectors: Arc<Mutex<BTreeMap<(crate::RegistrationScope, u64), bool>>>,
    fail_advertise: Arc<Mutex<bool>>,
}

impl InProcessAdmissionCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_fail_advertise(&self, fail: bool) {
        *self.fail_advertise.lock().unwrap() = fail;
    }

    pub fn with_selector(
        self,
        scope: crate::RegistrationScope,
        runner_id: u64,
        present: bool,
    ) -> Self {
        self.selectors
            .lock()
            .unwrap()
            .insert((scope, runner_id), present);
        self
    }

    pub fn set_selector(&self, scope: crate::RegistrationScope, runner_id: u64, present: bool) {
        self.selectors
            .lock()
            .unwrap()
            .insert((scope, runner_id), present);
    }
}

impl MultiRunnerAdmissionCoordinator for InProcessAdmissionCoordinator {
    fn observe_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<Option<bool>, String> {
        Ok(self
            .selectors
            .lock()
            .unwrap()
            .get(&(scope.clone(), runner_id))
            .copied())
    }

    fn advertise_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        if *self.fail_advertise.lock().unwrap() {
            return Err("admission.network-failure-simulated".to_owned());
        }
        self.selectors
            .lock()
            .unwrap()
            .insert((scope.clone(), runner_id), true);
        Ok(true)
    }

    fn withdraw_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        self.selectors
            .lock()
            .unwrap()
            .insert((scope.clone(), runner_id), false);
        Ok(true)
    }
}

/// Registration info for an enrolled runner target.
#[derive(Clone, Debug)]
pub struct RunnerTargetRegistration {
    pub scope: crate::RegistrationScope,
    pub runner_id: u64,
    pub service_name: Option<String>,
    pub runner_home: Option<PathBuf>,
    pub is_user_session: bool,
}

type RunnerAdmissionMap =
    BTreeMap<(crate::RegistrationScope, u64), Arc<dyn MultiRunnerAdmissionCoordinator>>;

/// Authoritative multi-runner lifecycle backend for installed production runtime.
///
/// Obtains fresh observations from the Windows SCM and runner processes for each enrolled runner key,
/// dispatches narrow SCM actions through PrivilegedScmHelper (sealed closed in production),
/// and coordinates admission label advertisement and withdrawal with fail-closed semantics.
pub struct AuthoritativeMultiRunnerLifecycleBackend<B> {
    helper: PrivilegedScmHelper<B>,
    admission: Option<Arc<dyn MultiRunnerAdmissionCoordinator>>,
    admission_coordinators: Arc<Mutex<RunnerAdmissionMap>>,
    session: Option<Arc<dyn MultiRunnerSessionCoordinator>>,
    runner_targets: Arc<Mutex<BTreeMap<String, RunnerTargetRegistration>>>,
}

impl<B: ScmControlBackend + Send + Sync + 'static> AuthoritativeMultiRunnerLifecycleBackend<B> {
    pub fn new(helper: PrivilegedScmHelper<B>) -> Self {
        Self {
            helper,
            admission: None,
            admission_coordinators: Arc::new(Mutex::new(BTreeMap::new())),
            session: None,
            runner_targets: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_admission_coordinator(
        mut self,
        coordinator: Arc<dyn MultiRunnerAdmissionCoordinator>,
    ) -> Self {
        self.admission = Some(coordinator);
        self
    }

    pub fn register_runner_admission_coordinator(
        &self,
        scope: crate::RegistrationScope,
        runner_id: u64,
        coordinator: Arc<dyn MultiRunnerAdmissionCoordinator>,
    ) {
        self.admission_coordinators
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert((scope, runner_id), coordinator);
    }

    fn get_admission_coordinator(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Option<Arc<dyn MultiRunnerAdmissionCoordinator>> {
        let coords = self
            .admission_coordinators
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(coord) = coords.get(&(scope.clone(), runner_id)) {
            return Some(Arc::clone(coord));
        }
        if coords.is_empty() {
            return self.admission.as_ref().map(Arc::clone);
        }
        None
    }

    pub fn with_session_coordinator(
        mut self,
        coordinator: Arc<dyn MultiRunnerSessionCoordinator>,
    ) -> Self {
        self.session = Some(coordinator);
        self
    }
}

impl<B: ScmControlBackend + Send + Sync + 'static> MultiRunnerLifecycleBackend
    for AuthoritativeMultiRunnerLifecycleBackend<B>
{
    fn register_runner_target(
        &self,
        local_runner_key: &str,
        scope: crate::RegistrationScope,
        runner_id: u64,
    ) {
        self.register_runner_target_with_service(local_runner_key, scope, runner_id, None);
    }

    fn register_runner_target_with_service(
        &self,
        local_runner_key: &str,
        scope: crate::RegistrationScope,
        runner_id: u64,
        service_name: Option<&str>,
    ) {
        self.register_runner_target_detailed(
            local_runner_key,
            scope,
            runner_id,
            service_name,
            None,
            false,
        );
    }

    fn register_runner_target_detailed(
        &self,
        local_runner_key: &str,
        scope: crate::RegistrationScope,
        runner_id: u64,
        service_name: Option<&str>,
        runner_home: Option<&Path>,
        is_user_session: bool,
    ) {
        self.runner_targets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                local_runner_key.to_string(),
                RunnerTargetRegistration {
                    scope,
                    runner_id,
                    service_name: service_name.map(|s| s.to_string()),
                    runner_home: runner_home.map(|p| p.to_path_buf()),
                    is_user_session,
                },
            );
    }

    fn observe_runners(&self, local_runner_keys: &[String]) -> Vec<PerRunnerObservation> {
        let mut observations = Vec::new();
        let targets = self
            .runner_targets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        for key in local_runner_keys {
            let target_reg = targets.get(key);
            let is_user_session = target_reg
                .as_ref()
                .map(|r| r.is_user_session)
                .unwrap_or(false);
            let runner_home = target_reg.as_ref().and_then(|r| r.runner_home.as_deref());

            let (lifecycle, ownership, worker_present) = if is_user_session {
                if let Some(home) = runner_home {
                    let (listener_active, worker_active, _alive) =
                        crate::windows_runner_discovery::observe_runner_processes_for_home(home);
                    if worker_active {
                        (DiscoveredLifecycle::Busy, true, true)
                    } else if listener_active {
                        (DiscoveredLifecycle::Listening, true, false)
                    } else {
                        (DiscoveredLifecycle::Stopped, true, false)
                    }
                } else {
                    (DiscoveredLifecycle::Unknown, false, false)
                }
            } else {
                let service_query = target_reg
                    .and_then(|r| r.service_name.as_deref())
                    .unwrap_or(key.as_str());

                let snap = self.helper.backend().query_exact(service_query);
                match snap {
                    Ok(s) => {
                        let busy = matches!(s.observation.state, DiscoveredLifecycle::Busy);
                        (
                            s.observation.state,
                            s.observation.ownership_verified && s.observation.image_path_matches,
                            busy,
                        )
                    }
                    Err(_) => (DiscoveredLifecycle::Unknown, false, false),
                }
            };

            let (selector_present, selector_unknown) = match target_reg {
                Some(reg) => {
                    if let Some(coord) = self.get_admission_coordinator(&reg.scope, reg.runner_id) {
                        match coord.observe_selector(&reg.scope, reg.runner_id) {
                            Ok(Some(present)) => (Some(present), false),
                            Ok(None) => (None, true),
                            Err(_) => (None, true),
                        }
                    } else {
                        (None, true)
                    }
                }
                None => (None, true),
            };

            observations.push(PerRunnerObservation {
                local_runner_key: key.clone(),
                ownership_verified: ownership,
                lifecycle,
                worker_present,
                selector_present,
                selector_observation_unknown: selector_unknown,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            });
        }
        observations
    }

    fn dispatch_scm(&self, request: &PrivilegedScmRequest) -> PrivilegedScmReport {
        self.helper.dispatch(request)
    }

    fn withdraw_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        let coord = self
            .get_admission_coordinator(scope, runner_id)
            .ok_or_else(|| format!("admission.no-coordinator-for-runner:{scope:?}:{runner_id}"))?;
        coord.withdraw_selector(scope, runner_id)
    }

    fn advertise_selector(
        &self,
        scope: &crate::RegistrationScope,
        runner_id: u64,
    ) -> Result<bool, String> {
        let coord = self
            .get_admission_coordinator(scope, runner_id)
            .ok_or_else(|| format!("admission.no-coordinator-for-runner:{scope:?}:{runner_id}"))?;
        coord.advertise_selector(scope, runner_id)
    }

    fn dispatch_session(
        &self,
        record: &RunnerRecord,
        action: crate::runner_session_backend::PreparedSessionAction,
    ) -> Result<bool, String> {
        match &self.session {
            Some(coord) => coord.dispatch_session(record, action),
            None => StandardSessionCoordinator::default().dispatch_session(record, action),
        }
    }
}

/// In-process single authority for `Observe -> Decide -> Reconcile`.
pub struct AgentCore<O, R, S> {
    observer: O,
    reconciler: R,
    store: S,
    config: AgentConfig,
    build: BuildProvenance,
    observation: AgentObservation,
    decision: AdmissionDecision,
    admission_control: AdmissionControlSnapshot,
    runner_registry: RunnerRegistryEditor,
    registry_store: Box<dyn RegistryStore>,
    last_runner_discovery: Option<DiscoveryReport>,
    multi_runner_backend: Option<Arc<dyn MultiRunnerLifecycleBackend>>,
    last_runner_reports: Vec<PrivilegedScmReport>,
}

impl<O, R, S> AgentCore<O, R, S>
where
    O: AgentObserver,
    R: AgentReconciler,
    S: ConfigStore,
{
    pub fn new(
        observer: O,
        reconciler: R,
        store: S,
        build: BuildProvenance,
    ) -> Result<Self, AgentCoreError> {
        Self::with_registry_store(
            observer,
            reconciler,
            store,
            build,
            Box::new(MemoryRegistryStore::default()),
        )
    }

    /// Constructs an Agent with an explicit RegistryStore (sandbox file or memory).
    pub fn with_registry_store(
        observer: O,
        reconciler: R,
        store: S,
        build: BuildProvenance,
        registry_store: Box<dyn RegistryStore>,
    ) -> Result<Self, AgentCoreError> {
        let config = store.load()?.unwrap_or_default();
        validate_schema(&config)?;
        let loaded = registry_store.load().map_err(AgentCoreError::Registry)?;
        let runner_registry = match loaded {
            Some(registry) => {
                RunnerRegistryEditor::new(registry).map_err(AgentCoreError::Registry)?
            }
            None => RunnerRegistryEditor::empty(),
        };
        let observation = AgentObservation::unobserved();
        let decision = decide(&config, &observation);
        let admission_control = observation
            .admission_control
            .clone()
            .with_desired(DesiredAdmissionState::from_decision(&decision));

        Ok(Self {
            observer,
            reconciler,
            store,
            config,
            build,
            observation,
            decision,
            admission_control,
            runner_registry,
            registry_store,
            last_runner_discovery: None,
            multi_runner_backend: None,
            last_runner_reports: Vec::new(),
        })
    }

    /// Attaches a multi-runner lifecycle backend for runner observation,
    /// admission selector withdrawal, and SCM dispatch.
    pub fn set_multi_runner_backend(&mut self, backend: Arc<dyn MultiRunnerLifecycleBackend>) {
        for record in self.runner_registry.registry().records.iter() {
            backend.register_runner_record(record);
        }
        self.multi_runner_backend = Some(backend);
    }

    pub fn multi_runner_backend(&self) -> Option<&Arc<dyn MultiRunnerLifecycleBackend>> {
        self.multi_runner_backend.as_ref()
    }

    pub fn last_runner_reports(&self) -> &[PrivilegedScmReport] {
        &self.last_runner_reports
    }

    /// Reconciles all enrolled runners and dispatches necessary SCM actions via the attached backend.
    pub fn reconcile_and_dispatch_multi_runners(
        &mut self,
        transaction_id: &str,
    ) -> Vec<PrivilegedScmReport> {
        let Some(backend) = self.multi_runner_backend.as_ref().cloned() else {
            return Vec::new();
        };

        let keys: Vec<String> = self
            .runner_registry
            .registry()
            .active_records()
            .map(|r| r.local_runner_key.clone())
            .collect();
        if keys.is_empty() {
            return Vec::new();
        }

        let observations = backend.observe_runners(&keys);
        let reconcile_report = self.reconcile_runners(&observations);
        let mut reports = Vec::new();

        for result in &reconcile_report.results {
            let Some(record) = self
                .runner_registry
                .registry()
                .get(&result.local_runner_key)
            else {
                continue;
            };

            let obs = observations
                .iter()
                .find(|o| o.local_runner_key == result.local_runner_key);

            // Step 1: If plan requires selector withdrawal or advertisement, coordinate it
            let advertise_result = if result.plan == PerRunnerPlan::AdvertiseSelector {
                backend.advertise_selector(&record.registration_scope, record.runner_id)
            } else {
                Ok(false)
            };

            if let Err(ref err) = advertise_result {
                reports.push(PrivilegedScmReport {
                    service_name: format!("admission:{}", record.local_runner_key),
                    transaction_id: transaction_id.to_string(),
                    applied: false,
                    achieved_state: result.observed_lifecycle,
                    achieved_start_mode: crate::ServiceStartMode::Disabled,
                    refusal_reason: Some(format!("admission.advertise-failed:{err}")),
                });
            }

            if result.plan == PerRunnerPlan::WithdrawSelector {
                if let Err(err) =
                    backend.withdraw_selector(&record.registration_scope, record.runner_id)
                {
                    reports.push(PrivilegedScmReport {
                        service_name: format!("admission:{}", record.local_runner_key),
                        transaction_id: transaction_id.to_string(),
                        applied: false,
                        achieved_state: result.observed_lifecycle,
                        achieved_start_mode: crate::ServiceStartMode::Disabled,
                        refusal_reason: Some(format!("admission.withdraw-failed:{err}")),
                    });
                }
            }

            // Step 2: Strictly verify dispatch guards before issuing Stop or Start
            let selector_confirmed_absent = obs
                .map(|o| o.selector_present == Some(false) && !o.selector_observation_unknown)
                .unwrap_or(false);

            let selector_confirmed_present = if result.plan == PerRunnerPlan::AdvertiseSelector {
                matches!(advertise_result, Ok(true))
            } else {
                obs.map(|o| o.selector_present == Some(true) && !o.selector_observation_unknown)
                    .unwrap_or(false)
            };

            let worker_idle = result.observed_lifecycle != DiscoveredLifecycle::Busy
                && !obs.map(|o| o.worker_present).unwrap_or(false);

            let plan_allows_stop = result.plan == PerRunnerPlan::Noop
                && (result.achieved == PerRunnerAchieved::Drained
                    || result.achieved == PerRunnerAchieved::Offline
                    || result.reason_code.as_str() == "runners-listener-stop-unproven");

            match &record.backend {
                RegistryBackendBinding::WindowsService { service_name } => {
                    let action = if (!record.desired_enabled
                        || result.desired == PerRunnerDesired::Offline)
                        && result.observed_lifecycle != DiscoveredLifecycle::Stopped
                        && selector_confirmed_absent
                        && worker_idle
                        && plan_allows_stop
                    {
                        Some(PrivilegedScmAction::StopService)
                    } else if record.desired_enabled
                        && result.desired == PerRunnerDesired::Full
                        && result.observed_lifecycle == DiscoveredLifecycle::Stopped
                        && selector_confirmed_present
                    {
                        Some(PrivilegedScmAction::StartService)
                    } else {
                        None
                    };

                    if let Some(action) = action {
                        let expected_image_token =
                            crate::runner_service_image_token(&record.runner_home);
                        let expected_execution_identity_token =
                            record.execution_identity_ref.key.clone();

                        let request = PrivilegedScmRequest {
                            action,
                            service_name: service_name.clone(),
                            expected_image_token,
                            expected_execution_identity_token,
                            target_host_token: String::new(),
                            candidate_commit: String::new(),
                            transaction_id: transaction_id.to_string(),
                        };
                        reports.push(backend.dispatch_scm(&request));
                    }
                }
                RegistryBackendBinding::UserSession => {
                    let session_action = if (!record.desired_enabled
                        || result.desired == PerRunnerDesired::Offline)
                        && result.observed_lifecycle != DiscoveredLifecycle::Stopped
                        && selector_confirmed_absent
                        && worker_idle
                        && plan_allows_stop
                    {
                        Some(crate::runner_session_backend::PreparedSessionAction::RequestGracefulStop)
                    } else if record.desired_enabled
                        && result.desired == PerRunnerDesired::Full
                        && result.observed_lifecycle == DiscoveredLifecycle::Stopped
                        && selector_confirmed_present
                    {
                        Some(crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener)
                    } else {
                        None
                    };

                    if let Some(action) = session_action {
                        let res = backend.dispatch_session(record, action.clone());
                        let rep = match res {
                            Ok(applied) => PrivilegedScmReport {
                                service_name: format!("session:{}", record.local_runner_key),
                                transaction_id: transaction_id.to_string(),
                                applied,
                                achieved_state: if applied {
                                    match action {
                                        crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener => {
                                            DiscoveredLifecycle::Listening
                                        }
                                        crate::runner_session_backend::PreparedSessionAction::RequestGracefulStop => {
                                            DiscoveredLifecycle::Stopped
                                        }
                                        crate::runner_session_backend::PreparedSessionAction::AdoptVerifiedListener => {
                                            DiscoveredLifecycle::Listening
                                        }
                                    }
                                } else {
                                    result.observed_lifecycle
                                },
                                achieved_start_mode: crate::ServiceStartMode::Disabled,
                                refusal_reason: None,
                            },
                            Err(err) => PrivilegedScmReport {
                                service_name: format!("session:{}", record.local_runner_key),
                                transaction_id: transaction_id.to_string(),
                                applied: false,
                                achieved_state: result.observed_lifecycle,
                                achieved_start_mode: crate::ServiceStartMode::Disabled,
                                refusal_reason: Some(err),
                            },
                        };
                        reports.push(rep);
                    }
                }
            }
        }

        self.last_runner_reports = reports.clone();
        reports
    }

    /// Injects a read-only discovery report for subsequent enroll/inspect flows.
    /// Discovery itself remains non-mutating.
    pub fn set_runner_discovery(&mut self, report: DiscoveryReport) {
        self.last_runner_discovery = Some(report);
    }

    pub fn runner_registry(&self) -> &RunnerRegistry {
        self.runner_registry.registry()
    }

    /// Reconciles all enrolled local runners against the single authoritative
    /// Agent policy (user mode, zen, hard safety, effective probes).
    pub fn reconcile_runners(
        &self,
        observations: &[PerRunnerObservation],
    ) -> MultiRunnerReconcileReport {
        let globals = GlobalPolicyInputs {
            user_mode: self.config.user_mode,
            zen: self.config.zen,
            hard_safety: self.observation.hard_safety,
            probes: self.effective_probes(),
        };
        crate::reconcile_multi_runners(self.runner_registry.registry(), &globals, observations)
    }

    /// Reconciles all enrolled runners and dispatches necessary SCM actions via the privileged helper.
    ///
    /// Multi-runner isolation invariant:
    /// Disabling Runner A dispatches control actions strictly for Runner A's bound service,
    /// while leaving Runner B's service completely untouched with zero dispatched actions.
    pub fn reconcile_and_dispatch_runners<B: ScmControlBackend>(
        &self,
        observations: &[PerRunnerObservation],
        helper: &PrivilegedScmHelper<B>,
        transaction_id: &str,
    ) -> Vec<PrivilegedScmReport> {
        let reconcile_report = self.reconcile_runners(observations);
        let mut reports = Vec::new();

        for result in &reconcile_report.results {
            let Some(record) = self
                .runner_registry
                .registry()
                .get(&result.local_runner_key)
            else {
                continue;
            };

            let service_name = match &record.backend {
                RegistryBackendBinding::WindowsService { service_name } => service_name.clone(),
                RegistryBackendBinding::UserSession => continue,
            };

            let obs = observations
                .iter()
                .find(|o| o.local_runner_key == result.local_runner_key);

            let selector_confirmed_absent = obs
                .map(|o| o.selector_present == Some(false) && !o.selector_observation_unknown)
                .unwrap_or(false);

            let worker_idle = result.observed_lifecycle != DiscoveredLifecycle::Busy
                && !obs.map(|o| o.worker_present).unwrap_or(false);

            let plan_allows_stop = result.plan == PerRunnerPlan::Noop
                && (result.achieved == PerRunnerAchieved::Drained
                    || result.achieved == PerRunnerAchieved::Offline
                    || result.reason_code.as_str() == "runners-listener-stop-unproven");

            let selector_confirmed_present = obs
                .map(|o| o.selector_present == Some(true) && !o.selector_observation_unknown)
                .unwrap_or(false);

            let action = if (!record.desired_enabled || result.desired == PerRunnerDesired::Offline)
                && result.observed_lifecycle != DiscoveredLifecycle::Stopped
                && selector_confirmed_absent
                && worker_idle
                && plan_allows_stop
            {
                Some(PrivilegedScmAction::StopService)
            } else if record.desired_enabled
                && result.desired == PerRunnerDesired::Full
                && result.observed_lifecycle == DiscoveredLifecycle::Stopped
                && selector_confirmed_present
            {
                Some(PrivilegedScmAction::StartService)
            } else {
                None
            };

            if let Some(action) = action {
                let expected_image_token = crate::runner_service_image_token(&record.runner_home);
                let expected_execution_identity_token = record.execution_identity_ref.key.clone();

                let request = PrivilegedScmRequest {
                    action,
                    service_name,
                    expected_image_token,
                    expected_execution_identity_token,
                    target_host_token: String::new(),
                    candidate_commit: String::new(),
                    transaction_id: transaction_id.to_string(),
                };
                reports.push(helper.dispatch(&request));
            }
        }

        reports
    }

    /// Migrates the optional v0.1 single binding into the registry and persists.
    pub fn migrate_v01_binding_if_needed(
        &mut self,
        binding: &crate::InstalledRuntimeBinding,
        service_name: Option<&str>,
    ) -> Result<Option<String>, AgentCoreError> {
        if self
            .runner_registry
            .registry()
            .legacy_v01_binding_key
            .is_some()
        {
            return Ok(self
                .runner_registry
                .registry()
                .legacy_v01_binding_key
                .clone());
        }
        let record = self
            .runner_registry
            .migrate_v01_binding(binding, service_name)
            .map_err(AgentCoreError::Registry)?;
        let key = record.local_runner_key.clone();
        self.persist_registry()?;
        Ok(Some(key))
    }

    fn persist_registry(&mut self) -> Result<(), AgentCoreError> {
        self.registry_store
            .save(self.runner_registry.registry())
            .map_err(AgentCoreError::Registry)
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn snapshot(&self) -> AgentSnapshot {
        let mut probes = self.effective_probes();
        if self.config.zen == ZenOverride::Enabled {
            for probe in probes.iter_mut().filter(|probe| probe.enabled) {
                probe.runtime_state = ProbeRuntimeState::Suspended;
                probe.reason_code = Some(static_reason("zen-suspended"));
            }
        }

        AgentSnapshot {
            schema_version: self.config.schema_version,
            build: self.build.clone(),
            health: self.observation.health,
            health_reason_code: self.observation.health_reason_code.clone(),
            zen: self.config.zen,
            user_mode: self.config.user_mode,
            desired_node_state: self.decision.desired_node_state,
            achieved_node_state: self.admission_control.achieved_node_state(
                self.decision.desired_node_state,
                self.observation.runner_phase,
            ),
            admission: self.decision.clone(),
            admission_control: self.admission_control.clone(),
            runner_phase: self.observation.runner_phase,
            links: self.observation.links.clone(),
            probes,
            ui_preferences: self.config.ui_preferences.clone(),
            effective_ui_preferences: self
                .config
                .ui_preferences
                .resolve(self.observation.system_preferences),
            start_on_login_preference: self.config.start_on_login,
            auto_idle_threshold_seconds: self.config.auto_idle_threshold_seconds,
            update_checks_enabled: self.config.update_checks_enabled,
        }
    }

    /// Runs the complete G03 synthetic loop in its fixed architectural order.
    pub fn observe_decide_reconcile(&mut self) -> Result<AgentSnapshot, AgentCoreError> {
        let observation = self.observer.observe().map_err(AgentCoreError::Observer)?;
        let decision = decide(&self.config, &observation);
        let admission_control = self
            .reconciler
            .reconcile(&decision, &observation)
            .map_err(AgentCoreError::Reconciler)?;
        self.observation = observation;
        self.decision = decision;
        self.admission_control = admission_control;

        if self.multi_runner_backend.is_some() {
            let _ = self.reconcile_and_dispatch_multi_runners("agent-cycle-tx");
        }

        Ok(self.snapshot())
    }

    /// Handles typed frontend commands while retaining all policy and state
    /// authority in the Agent Core.
    pub fn handle_command(
        &mut self,
        command: AgentCommand,
    ) -> Result<AgentResponse, AgentCoreError> {
        match command {
            AgentCommand::GetSnapshot
            | AgentCommand::GetRunnerStatus
            | AgentCommand::GetVersion => Ok(AgentResponse::Snapshot(self.snapshot())),
            AgentCommand::RunDoctor => Ok(AgentResponse::Doctor(self.doctor_report())),
            AgentCommand::SetMode { mode } => {
                self.update_config(|config| config.user_mode = mode)?;
                self.accept_after_reconcile()
            }
            AgentCommand::SetZen { zen } => {
                self.update_config(|config| config.zen = zen)?;
                self.accept_after_reconcile()
            }
            AgentCommand::SetProbeEnabled { probe_id, enabled } => {
                self.update_config(|config| {
                    config.probe_enabled.insert(probe_id, enabled);
                })?;
                self.accept_after_reconcile()
            }
            AgentCommand::SetUiPreferences { ui_preferences } => {
                self.update_config(|config| config.ui_preferences = ui_preferences)?;
                self.accept_after_reconcile()
            }
            AgentCommand::SetStartOnLoginPreference { enabled } => {
                self.update_config(|config| config.start_on_login = enabled)?;
                self.accept_after_reconcile()
            }
            AgentCommand::SetAutoIdleThreshold { seconds } => {
                self.update_config(|config| config.auto_idle_threshold_seconds = seconds)?;
                self.accept_after_reconcile()
            }
            AgentCommand::SetUpdateChecksEnabled { enabled } => {
                self.update_config(|config| config.update_checks_enabled = enabled)?;
                self.accept_after_reconcile()
            }
            AgentCommand::CheckForUpdates
            | AgentCommand::OpenConfig
            | AgentCommand::OpenDataDirectory
            | AgentCommand::OpenLogs
            | AgentCommand::ExitAfterDrain => Ok(AgentResponse::Rejected {
                reason_code: static_reason("not-implemented"),
            }),
            AgentCommand::RunnersList
            | AgentCommand::RunnersDiscover
            | AgentCommand::RunnersInspect { .. }
            | AgentCommand::RunnersEnroll { .. }
            | AgentCommand::RunnersEnable { .. }
            | AgentCommand::RunnersDisable { .. }
            | AgentCommand::RunnersDisableAll { .. }
            | AgentCommand::RunnersEnableAll { .. }
            | AgentCommand::RunnersReconcile { .. }
            | AgentCommand::RunnersForget { .. }
            | AgentCommand::RunnersStatus { .. } => self.handle_runners_command(command),
        }
    }

    fn handle_runners_command(
        &mut self,
        command: AgentCommand,
    ) -> Result<AgentResponse, AgentCoreError> {
        if matches!(command, AgentCommand::RunnersDiscover) {
            // Refresh from the host read-only adapter when no fixture was injected.
            // Still non-exhaustive and never mutates Services/labels.
            if self.last_runner_discovery.is_none()
                || self
                    .last_runner_discovery
                    .as_ref()
                    .is_some_and(|report| report.candidates.is_empty())
            {
                self.last_runner_discovery =
                    Some(crate::discover_local_runners_read_only(Vec::new()));
            }
        }
        // Snapshot intent before mutation so a failed disk write cannot leave
        // memory pretending the change was durable.
        let registry_before = self.runner_registry.clone();
        match handle_runners_command(
            &mut self.runner_registry,
            self.last_runner_discovery.as_ref(),
            &command,
        ) {
            Ok(mut report) => {
                if report.applied {
                    if let Err(error) = self.persist_registry() {
                        self.runner_registry = registry_before;
                        return Err(error);
                    }
                    if let AgentCommand::RunnersEnroll {
                        local_runner_key, ..
                    } = &command
                    {
                        if let Some(record) = self.runner_registry.registry().get(local_runner_key)
                        {
                            if let Some(backend) = &self.multi_runner_backend {
                                backend.register_runner_record(record);
                            }
                        }
                    }
                }
                if matches!(command, AgentCommand::RunnersStatus { .. }) {
                    if let Some(backend) = &self.multi_runner_backend {
                        let keys: Vec<String> = report
                            .records
                            .iter()
                            .map(|r| r.local_runner_key.clone())
                            .collect();
                        let observations = backend.observe_runners(&keys);
                        let now_unix_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);

                        for record_view in &mut report.records {
                            if let Some(obs) = observations
                                .iter()
                                .find(|o| o.local_runner_key == record_view.local_runner_key)
                            {
                                record_view.presentation_status = if !obs.ownership_verified
                                    || obs.lifecycle == DiscoveredLifecycle::Unknown
                                {
                                    RunnerPresentationStatus::Unknown
                                } else if obs.worker_present
                                    || obs.lifecycle == DiscoveredLifecycle::Busy
                                {
                                    RunnerPresentationStatus::Busy
                                } else if !record_view.desired_enabled
                                    && obs.lifecycle != DiscoveredLifecycle::Stopped
                                {
                                    RunnerPresentationStatus::Draining
                                } else if obs.lifecycle == DiscoveredLifecycle::Stopped {
                                    RunnerPresentationStatus::Stopped
                                } else if record_view.desired_enabled {
                                    RunnerPresentationStatus::Enabled
                                } else {
                                    RunnerPresentationStatus::Disabled
                                };
                            }

                            let bound_service = self
                                .runner_registry
                                .registry()
                                .get(&record_view.local_runner_key)
                                .and_then(|rec| match &rec.backend {
                                    RegistryBackendBinding::WindowsService { service_name } => {
                                        Some(service_name.clone())
                                    }
                                    _ => None,
                                });

                            // Record partial failure and refusal from last reports if applicable
                            if let Some(last_report) = self.last_runner_reports.iter().find(|r| {
                                r.service_name == record_view.local_runner_key
                                    || bound_service.as_deref() == Some(&r.service_name)
                                    || r.service_name
                                        .ends_with(&format!(":{}", record_view.local_runner_key))
                            }) {
                                if !last_report.applied && last_report.refusal_reason.is_some() {
                                    record_view.item_applied = Some(false);
                                    let code = last_report
                                        .refusal_reason
                                        .as_deref()
                                        .and_then(|r| ReasonCode::new(r.replace('.', "-")).ok())
                                        .unwrap_or_else(|| static_reason("runners-command-failed"));
                                    record_view.item_reason_code = Some(code);
                                    report.partial_failure = true;
                                }
                            }
                        }

                        // Attach live read-only resource samples
                        let mut samples_by_key = Vec::new();
                        for record in self.runner_registry.registry().records.iter() {
                            let (listener_active, worker_active, _) =
                                crate::windows_runner_discovery::observe_runner_processes_for_home(
                                    &record.runner_home,
                                );
                            let mut samples = Vec::new();
                            if listener_active {
                                samples.push(crate::runner_resource::RunnerMemorySample {
                                    role: crate::runner_resource::RunnerResourceRole::Listener,
                                    working_set_bytes: None,
                                    sampled_at_unix_ms: now_unix_ms,
                                    confidence:
                                        crate::runner_resource::ResourceSampleConfidence::Estimate,
                                });
                            }
                            if worker_active {
                                samples.push(crate::runner_resource::RunnerMemorySample {
                                    role: crate::runner_resource::RunnerResourceRole::Worker,
                                    working_set_bytes: None,
                                    sampled_at_unix_ms: now_unix_ms,
                                    confidence:
                                        crate::runner_resource::ResourceSampleConfidence::Estimate,
                                });
                            }
                            if !samples.is_empty() {
                                samples_by_key.push((record.local_runner_key.clone(), samples));
                            }
                        }
                        report = report.with_resource_samples(&samples_by_key, now_unix_ms);
                    }
                }
                Ok(AgentResponse::Runners(report))
            }
            Err(RegistryError::NotFound) => Ok(AgentResponse::Rejected {
                reason_code: static_reason("runners-not-found"),
            }),
            Err(RegistryError::NotEnrollmentEligible) => Ok(AgentResponse::Rejected {
                reason_code: static_reason("runners-not-enrollment-eligible"),
            }),
            Err(RegistryError::SameScopeManagedAdmissionCollision) => Ok(AgentResponse::Rejected {
                reason_code: static_reason("runners-same-scope-collision"),
            }),
            Err(RegistryError::Forgotten) => Ok(AgentResponse::Rejected {
                reason_code: static_reason("runners-forgotten"),
            }),
            Err(_) => Ok(AgentResponse::Rejected {
                reason_code: static_reason("runners-command-failed"),
            }),
        }
    }

    fn accept_after_reconcile(&mut self) -> Result<AgentResponse, AgentCoreError> {
        let snapshot = self.observe_decide_reconcile()?;
        Ok(AgentResponse::Accepted { snapshot })
    }

    fn update_config(
        &mut self,
        update: impl FnOnce(&mut AgentConfig),
    ) -> Result<(), AgentCoreError> {
        let mut next = self.config.clone();
        update(&mut next);
        self.store.save(&next)?;
        self.config = next;
        self.decision = decide(&self.config, &self.observation);
        Ok(())
    }

    fn effective_probes(&self) -> Vec<ProbeSnapshot> {
        let mut probes = self.observation.probes.clone();
        for probe in &mut probes {
            if let Some(enabled) = self.config.probe_enabled.get(&probe.id) {
                probe.enabled = *enabled;
            }
        }
        probes
    }

    fn doctor_report(&self) -> DoctorReport {
        let (status, fallback_reason) = match self.observation.health {
            AgentHealth::Healthy => (DoctorStatus::Pass, None),
            AgentHealth::Degraded => (DoctorStatus::Warn, Some(static_reason("agent-degraded"))),
            AgentHealth::Unhealthy => (DoctorStatus::Fail, Some(static_reason("agent-unhealthy"))),
            AgentHealth::Starting => (DoctorStatus::Unknown, Some(static_reason("agent-starting"))),
        };

        let mut checks = vec![DoctorCheck {
            id: static_reason("agent-health"),
            status,
            reason_code: self
                .observation
                .health_reason_code
                .clone()
                .or(fallback_reason),
        }];

        let registry = self.runner_registry.registry();
        let registry_status = if registry
            .records
            .iter()
            .any(|r| r.local_runner_key.is_empty())
        {
            DoctorStatus::Fail
        } else {
            DoctorStatus::Pass
        };
        let migration_reason = if registry.legacy_v01_binding_key.is_some() {
            Some(static_reason("registry-v01-migrated"))
        } else {
            None
        };
        checks.push(DoctorCheck {
            id: static_reason("runner-registry"),
            status: registry_status,
            reason_code: migration_reason,
        });

        DoctorReport { checks }
    }
}

/// Agent Core failure categories. These are internal errors, not localized
/// frontend strings or stable JSON response payloads.
#[derive(Debug)]
pub enum AgentCoreError {
    Config(ConfigStoreError),
    Registry(RegistryError),
    Observer(String),
    Reconciler(String),
}

impl fmt::Display for AgentCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Registry(error) => write!(formatter, "runner registry failed: {error}"),
            Self::Observer(error) => write!(formatter, "observer failed: {error}"),
            Self::Reconciler(error) => write!(formatter, "reconciler failed: {error}"),
        }
    }
}

impl std::error::Error for AgentCoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Registry(error) => Some(error),
            Self::Observer(_) | Self::Reconciler(_) => None,
        }
    }
}

impl From<ConfigStoreError> for AgentCoreError {
    fn from(error: ConfigStoreError) -> Self {
        Self::Config(error)
    }
}

fn validate_schema(config: &AgentConfig) -> Result<(), ConfigStoreError> {
    if config.schema_version == CONFIG_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ConfigStoreError::IncompatibleSchema {
            found: config.schema_version,
            supported: CONFIG_SCHEMA_VERSION,
        })
    }
}

fn decide(config: &AgentConfig, observation: &AgentObservation) -> AdmissionDecision {
    let mut probes = observation.probes.clone();
    for probe in &mut probes {
        if let Some(enabled) = config.probe_enabled.get(&probe.id) {
            probe.enabled = *enabled;
        }
    }
    decide_admission(
        config.user_mode,
        config.zen,
        observation.hard_safety,
        &probes,
    )
}

fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static reason codes must be valid")
}

fn write_bytes_atomically_with(
    path: &Path,
    bytes: &[u8],
    before_replace: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = next_temporary_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        before_replace(&temporary)?;
        replace_existing_file(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn next_temporary_path(path: &Path) -> PathBuf {
    static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let id = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{name}.{}.{}.tmp", process::id(), id))
}

#[cfg(not(windows))]
fn replace_existing_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace_existing_file(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from_wide = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to_wide = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        fs, io,
        path::PathBuf,
        process,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::*;

    #[test]
    fn observe_decide_reconcile_keeps_manual_policy_in_agent_authority() {
        let observer = QueueObserver::new(vec![healthy_observation()]);
        let reconciler = RecordingReconciler::default();
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::new(observer, reconciler, store, build()).unwrap();

        let response = core
            .handle_command(AgentCommand::SetMode {
                mode: UserMode::Work,
            })
            .unwrap();

        let AgentResponse::Accepted { snapshot } = response else {
            panic!("mode changes must be acknowledged with an Agent snapshot");
        };
        assert_eq!(snapshot.desired_node_state, NodeState::Drained);
        assert_eq!(snapshot.achieved_node_state, None);
        assert!(!snapshot.admission.allow_new_work);
        assert_eq!(snapshot.admission.reason_code.as_str(), "manual-work");
        assert!(snapshot.admission.drain_requested);
        assert_eq!(core.config().user_mode, UserMode::Work);
        assert_eq!(core.reconciler.decisions.len(), 1);
    }

    #[test]
    fn zen_precedes_an_explicit_force_ci_mode() {
        let observer = QueueObserver::new(vec![healthy_observation(), healthy_observation()]);
        let reconciler = RecordingReconciler::default();
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::new(observer, reconciler, store, build()).unwrap();

        core.handle_command(AgentCommand::SetMode {
            mode: UserMode::ForceCi,
        })
        .unwrap();
        let response = core
            .handle_command(AgentCommand::SetZen {
                zen: ZenOverride::Enabled,
            })
            .unwrap();

        let AgentResponse::Accepted { snapshot } = response else {
            panic!("Zen changes must be acknowledged with an Agent snapshot");
        };
        assert_eq!(snapshot.user_mode, UserMode::ForceCi);
        assert_eq!(snapshot.zen, ZenOverride::Enabled);
        assert_eq!(snapshot.desired_node_state, NodeState::Offline);
        assert_eq!(snapshot.admission.reason_code.as_str(), "zen-enabled");
        assert!(snapshot.admission.drain_requested);
    }

    #[test]
    fn probe_enablement_is_persisted_intent_and_runtime_state_is_reconstructed() {
        let observation = AgentObservation {
            health: AgentHealth::Healthy,
            health_reason_code: Some(ReasonCode::new("host-observed").unwrap()),
            hard_safety: crate::HardSafetyState::Clear,
            runner_phase: RunnerPhase::Listening,
            execution_identity: ExecutionIdentityEvidence::Unknown,
            work_root: OwnershipEvidence::Unknown,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: None,
            }],
            probes: vec![ProbeSnapshot {
                id: ProbeId::new("steam-game").unwrap(),
                enabled: true,
                health: crate::ProbeHealth::Healthy,
                runtime_state: ProbeRuntimeState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            system_preferences: SystemPreferences::default(),
        };
        let observer = QueueObserver::new(vec![observation]);
        let reconciler = RecordingReconciler::default();
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::new(observer, reconciler, store, build()).unwrap();

        let response = core
            .handle_command(AgentCommand::SetProbeEnabled {
                probe_id: ProbeId::new("steam-game").unwrap(),
                enabled: false,
            })
            .unwrap();
        let AgentResponse::Accepted { snapshot } = response else {
            panic!("probe changes must be acknowledged with an Agent snapshot");
        };

        assert!(!snapshot.probes[0].enabled);
        assert_eq!(snapshot.probes[0].runtime_state, ProbeRuntimeState::Unknown);
        assert_eq!(
            core.config()
                .probe_enabled
                .get(&ProbeId::new("steam-game").unwrap()),
            Some(&false)
        );
    }

    #[test]
    fn presentation_preferences_persist_without_changing_admission_policy() {
        let observer = QueueObserver::new(vec![healthy_observation(), healthy_observation()]);
        let reconciler = RecordingReconciler::default();
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::new(observer, reconciler, store, build()).unwrap();
        core.observe_decide_reconcile().unwrap();
        let admission_before = core.snapshot().admission;

        let response = core
            .handle_command(AgentCommand::SetUiPreferences {
                ui_preferences: UiPreferences {
                    theme: ThemePreference::Dark,
                    language: crate::LanguagePreference::ZhCn,
                    menu_hints_enabled: true,
                },
            })
            .unwrap();
        let AgentResponse::Accepted { snapshot } = response else {
            panic!("presentation preference updates must return an Agent snapshot");
        };

        assert_eq!(snapshot.ui_preferences.theme, ThemePreference::Dark);
        assert_eq!(
            snapshot.ui_preferences.language,
            crate::LanguagePreference::ZhCn
        );
        assert_eq!(snapshot.admission, admission_before);
        assert_eq!(core.config().ui_preferences, snapshot.ui_preferences);
    }

    #[test]
    fn host_health_reason_is_preserved_in_snapshot_and_doctor() {
        let mut observation = healthy_observation();
        observation.health = AgentHealth::Degraded;
        observation.health_reason_code =
            Some(ReasonCode::new("host-observation-incomplete").unwrap());
        let observer = QueueObserver::new(vec![observation]);
        let reconciler = RecordingReconciler::default();
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::new(observer, reconciler, store, build()).unwrap();

        let snapshot = core.observe_decide_reconcile().unwrap();
        assert_eq!(snapshot.health, AgentHealth::Degraded);
        assert_eq!(
            snapshot.health_reason_code.unwrap().as_str(),
            "host-observation-incomplete"
        );

        let AgentResponse::Doctor(report) = core.handle_command(AgentCommand::RunDoctor).unwrap()
        else {
            panic!("doctor command must return the Agent health report");
        };
        assert_eq!(
            report.checks[0].reason_code.as_ref().unwrap().as_str(),
            "host-observation-incomplete"
        );
    }

    #[test]
    fn zen_suspends_enabled_probe_runtime_without_disabling_configuration() {
        let observation = AgentObservation {
            health: AgentHealth::Healthy,
            health_reason_code: Some(ReasonCode::new("host-observed").unwrap()),
            hard_safety: crate::HardSafetyState::Clear,
            runner_phase: RunnerPhase::Listening,
            execution_identity: ExecutionIdentityEvidence::Unknown,
            work_root: OwnershipEvidence::Unknown,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            links: Vec::new(),
            probes: vec![ProbeSnapshot {
                id: ProbeId::new("user-activity").unwrap(),
                enabled: true,
                health: crate::ProbeHealth::Healthy,
                runtime_state: ProbeRuntimeState::Active,
                reason_code: Some(ReasonCode::new("user-input-recent").unwrap()),
            }],
            system_preferences: SystemPreferences::default(),
        };
        let observer = QueueObserver::new(vec![observation]);
        let reconciler = RecordingReconciler::default();
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::new(observer, reconciler, store, build()).unwrap();

        let response = core
            .handle_command(AgentCommand::SetZen {
                zen: ZenOverride::Enabled,
            })
            .unwrap();
        let AgentResponse::Accepted { snapshot } = response else {
            panic!("Zen changes must return an Agent snapshot");
        };

        assert!(snapshot.probes[0].enabled);
        assert_eq!(
            snapshot.probes[0].runtime_state,
            ProbeRuntimeState::Suspended
        );
        assert_eq!(snapshot.admission.reason_code.as_str(), "zen-enabled");
    }

    #[test]
    fn file_store_rejects_unknown_schema_and_atomic_staging_preserves_existing_bytes() {
        let root = temporary_root();
        let path = root.join("config.json");
        fs::write(&path, b"old-config").unwrap();

        let error = write_bytes_atomically_with(&path, b"new-config", |_| {
            Err(io::Error::other("simulated interruption before replace"))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&path).unwrap(), b"old-config");
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
                .count(),
            0
        );

        let store = FileConfigStore::new(&path);
        let config = AgentConfig::default();
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap(), Some(config));

        let unknown_schema = AgentConfig {
            schema_version: CONFIG_SCHEMA_VERSION + 1,
            ..AgentConfig::default()
        };
        fs::write(&path, serde_json::to_vec(&unknown_schema).unwrap()).unwrap();
        assert!(matches!(
            store.load(),
            Err(ConfigStoreError::IncompatibleSchema { .. })
        ));

        // Corrupt durable bytes must fail closed rather than inventing defaults.
        fs::write(&path, b"{not-json").unwrap();
        assert!(matches!(store.load(), Err(ConfigStoreError::Json(_))));
        // Residue beside the durable file must not resurrect a valid load.
        fs::write(root.join("config.json.tmp"), b"{\"schema_version\":1}").unwrap();
        assert!(matches!(store.load(), Err(ConfigStoreError::Json(_))));

        fs::remove_dir_all(root).unwrap();
    }

    fn build() -> BuildProvenance {
        BuildProvenance {
            version: "0.1.0-dev".to_owned(),
            commit: "0123456789abcdef".to_owned(),
            channel: "dev".to_owned(),
            target: "synthetic".to_owned(),
        }
    }

    fn healthy_observation() -> AgentObservation {
        AgentObservation {
            health: AgentHealth::Healthy,
            health_reason_code: Some(ReasonCode::new("host-observed").unwrap()),
            hard_safety: crate::HardSafetyState::Clear,
            runner_phase: RunnerPhase::Listening,
            execution_identity: ExecutionIdentityEvidence::Unknown,
            work_root: OwnershipEvidence::Unknown,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Full,
            ),
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            probes: Vec::new(),
            system_preferences: SystemPreferences::default(),
        }
    }

    #[derive(Default)]
    struct RecordingReconciler {
        decisions: Vec<AdmissionDecision>,
    }

    impl AgentReconciler for RecordingReconciler {
        fn reconcile(
            &mut self,
            decision: &AdmissionDecision,
            observation: &AgentObservation,
        ) -> Result<AdmissionControlSnapshot, String> {
            self.decisions.push(decision.clone());
            Ok(observation
                .admission_control
                .clone()
                .with_desired(DesiredAdmissionState::from_decision(decision)))
        }
    }

    struct QueueObserver {
        observations: RefCell<Vec<AgentObservation>>,
    }

    impl QueueObserver {
        fn new(observations: Vec<AgentObservation>) -> Self {
            Self {
                observations: RefCell::new(observations.into_iter().rev().collect()),
            }
        }
    }

    impl AgentObserver for QueueObserver {
        fn observe(&mut self) -> Result<AgentObservation, String> {
            self.observations
                .borrow_mut()
                .pop()
                .ok_or_else(|| "no synthetic observation available".to_owned())
        }
    }

    fn temporary_root() -> PathBuf {
        static NEXT_ROOT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("runnermesh-g03-{}-{id}", process::id()));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn runner_enable_disable_persists_across_agent_restart_in_sandbox() {
        use crate::{
            DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity,
            DiscoveredServiceBinding, DiscoveryCapabilities, DiscoveryDisposition, DiscoveryReport,
            EnrollmentState, FileRegistryStore, OpaqueIdentityReference, RegistrationScope,
            RunnerBackendKind, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        };

        let root = temporary_root();
        let registry_path = root.join("runners-registry.json");
        let identity = OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )
        .unwrap();
        let candidate = |key: &str, repo: &str, id: u64| DiscoveredRunner {
            local_discovery_key: key.to_owned(),
            disposition: DiscoveryDisposition::ObservedOnly,
            refusal: None,
            backend: RunnerBackendKind::UserSession,
            identity: DiscoveredRunnerIdentity {
                registration_scope: Some(RegistrationScope::Repository {
                    owner: "fixture-owner".to_owned(),
                    repository: repo.to_owned(),
                }),
                runner_id: Some(id),
                expected_runner_name: Some(format!("fixture-{id}")),
                runner_home: root.join(key),
                work_root: Some(root.join(key).join("_work")),
                execution_identity_ref: Some(identity.clone()),
            },
            service: None as Option<DiscoveredServiceBinding>,
            session: None,
            capabilities: DiscoveryCapabilities {
                service_control_contract: false,
                user_session_control_contract: true,
                enrollment_eligible: true,
            },
            same_scope_collision: false,
            observed_lifecycle: DiscoveredLifecycle::Stopped,
        };

        {
            let store = MemoryConfigStore::default();
            let mut core = AgentCore::with_registry_store(
                QueueObserver::new(vec![healthy_observation()]),
                RecordingReconciler::default(),
                store,
                build(),
                Box::new(FileRegistryStore::new(&registry_path)),
            )
            .unwrap();
            core.set_runner_discovery(DiscoveryReport {
                schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
                read_only: true,
                exhaustive: false,
                service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
                session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
                process_observation: crate::DiscoveryEnumerationStatus::Ok,
                candidates: vec![
                    candidate("runner-a", "repo-a", 1),
                    candidate("runner-b", "repo-b", 2),
                ],
            });
            for key in ["runner-a", "runner-b"] {
                let response = core
                    .handle_command(AgentCommand::RunnersEnroll {
                        local_runner_key: key.to_owned(),
                        prepare_only: false,
                    })
                    .unwrap();
                assert!(matches!(response, AgentResponse::Runners(_)));
            }
            let disabled = core
                .handle_command(AgentCommand::RunnersDisable {
                    local_runner_key: "runner-a".to_owned(),
                    when_idle: true,
                    prepare_only: false,
                })
                .unwrap();
            assert!(matches!(disabled, AgentResponse::Runners(_)));
            // Force-CI must not overwrite persisted disable intent.
            core.handle_command(AgentCommand::SetMode {
                mode: UserMode::ForceCi,
            })
            .unwrap();
            assert!(
                !core
                    .runner_registry()
                    .get("runner-a")
                    .unwrap()
                    .desired_enabled
            );
            assert!(
                core.runner_registry()
                    .get("runner-b")
                    .unwrap()
                    .desired_enabled
            );
        }

        let store = MemoryConfigStore::default();
        let reloaded = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation()]),
            RecordingReconciler::default(),
            store,
            build(),
            Box::new(FileRegistryStore::new(&registry_path)),
        )
        .unwrap();
        let a = reloaded.runner_registry().get("runner-a").unwrap();
        let b = reloaded.runner_registry().get("runner-b").unwrap();
        assert!(!a.desired_enabled);
        assert!(b.desired_enabled);
        assert_eq!(a.enrollment_state, EnrollmentState::Enrolled);
        assert_eq!(b.enrollment_state, EnrollmentState::Enrolled);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_entrypoint_runner_a_disable_preserves_runner_b_intent_and_active_execution() {
        use crate::{
            DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity,
            DiscoveredServiceBinding, DiscoveryCapabilities, DiscoveryDisposition, DiscoveryReport,
            FileRegistryStore, OpaqueIdentityReference, PerRunnerAchieved, PerRunnerDesired,
            PerRunnerObservation, PerRunnerPlan, RegistrationScope, RunnerBackendKind,
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        };

        let root = temporary_root();
        let registry_path = root.join("runners-registry.json");
        let identity = OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .unwrap();

        let make_candidate = |key: &str, repo: &str, id: u64, same_scope: bool| DiscoveredRunner {
            local_discovery_key: key.to_owned(),
            disposition: DiscoveryDisposition::ObservedOnly,
            refusal: None,
            backend: RunnerBackendKind::UserSession,
            identity: DiscoveredRunnerIdentity {
                registration_scope: Some(RegistrationScope::Repository {
                    owner: "fixture-owner".to_owned(),
                    repository: repo.to_owned(),
                }),
                runner_id: Some(id),
                expected_runner_name: Some(format!("fixture-{id}")),
                runner_home: root.join(key),
                work_root: Some(root.join(key).join("_work")),
                execution_identity_ref: Some(identity.clone()),
            },
            service: None as Option<DiscoveredServiceBinding>,
            session: None,
            capabilities: DiscoveryCapabilities {
                service_control_contract: false,
                user_session_control_contract: true,
                enrollment_eligible: true,
            },
            same_scope_collision: same_scope,
            observed_lifecycle: DiscoveredLifecycle::Stopped,
        };

        // 1. Initialize AgentCore with persistent file store
        let store = MemoryConfigStore::default();
        let mut core = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation(); 16]),
            RecordingReconciler::default(),
            store,
            build(),
            Box::new(FileRegistryStore::new(&registry_path)),
        )
        .unwrap();

        // 2. Discover two distinct runners across different scopes (repo-a and repo-b)
        // plus runner-c sharing repo-b (colliding under ADR 0004).
        core.set_runner_discovery(DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![
                make_candidate("runner-a", "repo-a", 101, false),
                make_candidate("runner-b", "repo-b", 102, false),
                make_candidate("runner-c", "repo-b", 103, true),
            ],
        });

        // 3. Enroll runner-a and runner-b via regular Agent command entrypoint
        for key in ["runner-a", "runner-b"] {
            let resp = core
                .handle_command(AgentCommand::RunnersEnroll {
                    local_runner_key: key.to_owned(),
                    prepare_only: false,
                })
                .unwrap();
            assert!(matches!(resp, AgentResponse::Runners(_)));
        }

        // Verify initial intent via regular Agent entrypoint: both enabled
        let status1 = core
            .handle_command(AgentCommand::RunnersStatus {
                local_runner_key: None,
            })
            .unwrap();
        let AgentResponse::Runners(report1) = status1 else {
            panic!("expected runners report");
        };
        let view_a1 = report1
            .records
            .iter()
            .find(|r| r.local_runner_key == "runner-a")
            .unwrap();
        let view_b1 = report1
            .records
            .iter()
            .find(|r| r.local_runner_key == "runner-b")
            .unwrap();
        assert!(view_a1.desired_enabled);
        assert!(view_b1.desired_enabled);

        // Set baseline user mode to Idle via regular Agent command entrypoint
        core.handle_command(AgentCommand::SetMode {
            mode: UserMode::Idle,
        })
        .unwrap();

        // 4. Initial multi-runner reconciliation: both listening and idle -> Full capacity
        let obs_both_listening = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep1 = core.reconcile_runners(&obs_both_listening);
        assert_eq!(rep1.results.len(), 2);
        assert_eq!(rep1.results[0].desired, PerRunnerDesired::Full);
        assert_eq!(rep1.results[0].achieved, PerRunnerAchieved::Full);
        assert_eq!(rep1.results[1].desired, PerRunnerDesired::Full);
        assert_eq!(rep1.results[1].achieved, PerRunnerAchieved::Full);

        // 5. Disable runner-a via regular Agent command entrypoint
        let disable_resp = core
            .handle_command(AgentCommand::RunnersDisable {
                local_runner_key: "runner-a".to_owned(),
                when_idle: true,
                prepare_only: false,
            })
            .unwrap();
        let AgentResponse::Runners(disable_report) = disable_resp else {
            panic!("expected runners report on disable");
        };
        assert!(disable_report.applied);

        // 6. Verify Intent Independence: runner-a is Disabled, runner-b remains Enabled!
        let status2 = core
            .handle_command(AgentCommand::RunnersStatus {
                local_runner_key: None,
            })
            .unwrap();
        let AgentResponse::Runners(report2) = status2 else {
            panic!("expected runners report");
        };
        let view_a2 = report2
            .records
            .iter()
            .find(|r| r.local_runner_key == "runner-a")
            .unwrap();
        let view_b2 = report2
            .records
            .iter()
            .find(|r| r.local_runner_key == "runner-b")
            .unwrap();
        assert!(!view_a2.desired_enabled, "runner-a must be disabled");
        assert!(
            view_b2.desired_enabled,
            "runner-b intent must remain enabled and unaffected"
        );

        // 7. Verify Execution Independence:
        // Runner A is draining/withdrawing selector, while Runner B is actively executing a CI job!
        let obs_a_draining_b_busy = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Busy,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep2 = core.reconcile_runners(&obs_a_draining_b_busy);
        let res_a2 = rep2
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-a")
            .unwrap();
        let res_b2 = rep2
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-b")
            .unwrap();
        // Runner A plans selector withdrawal:
        assert_eq!(res_a2.desired, PerRunnerDesired::Drained);
        assert_eq!(res_a2.plan, PerRunnerPlan::WithdrawSelector);
        // Runner B remains desired Full; active worker execution continues (plan is Noop, never interrupted):
        assert_eq!(res_b2.desired, PerRunnerDesired::Full);
        assert_eq!(res_b2.plan, PerRunnerPlan::Noop);
        assert_eq!(res_b2.achieved, PerRunnerAchieved::Full);

        // 8. Runner A finishes withdrawal & stops; Runner B completes CI job & returns to Listening:
        let obs_a_stopped_b_completed = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Stopped,
                worker_present: false,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep3 = core.reconcile_runners(&obs_a_stopped_b_completed);
        let res_a3 = rep3
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-a")
            .unwrap();
        let res_b3 = rep3
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-b")
            .unwrap();
        assert_eq!(res_a3.desired, PerRunnerDesired::Drained);
        assert_eq!(res_a3.achieved, PerRunnerAchieved::Drained);
        assert_eq!(res_a3.plan, PerRunnerPlan::Noop);
        // Runner B is Full and completely unimpeded:
        assert_eq!(res_b3.desired, PerRunnerDesired::Full);
        assert_eq!(res_b3.achieved, PerRunnerAchieved::Full);
        assert_eq!(res_b3.plan, PerRunnerPlan::Noop);

        // 9. Simulate Agent Restart: construct fresh AgentCore from disk store
        drop(core);
        let store2 = MemoryConfigStore::default();
        let mut reloaded = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation(); 16]),
            RecordingReconciler::default(),
            store2,
            build(),
            Box::new(FileRegistryStore::new(&registry_path)),
        )
        .unwrap();

        let status_reloaded = reloaded
            .handle_command(AgentCommand::RunnersStatus {
                local_runner_key: None,
            })
            .unwrap();
        let AgentResponse::Runners(report_reloaded) = status_reloaded else {
            panic!("expected runners report");
        };
        let view_a_rel = report_reloaded
            .records
            .iter()
            .find(|r| r.local_runner_key == "runner-a")
            .unwrap();
        let view_b_rel = report_reloaded
            .records
            .iter()
            .find(|r| r.local_runner_key == "runner-b")
            .unwrap();
        assert!(!view_a_rel.desired_enabled, "runner-a remains disabled");
        assert!(view_b_rel.desired_enabled, "runner-b remains enabled");

        // 10. Ineligible candidate is rejected fail-closed at regular Agent entrypoint
        let mut ineligible_candidate = make_candidate("runner-ineligible", "repo-b", 103, true);
        ineligible_candidate.capabilities.enrollment_eligible = false;
        reloaded.set_runner_discovery(DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![ineligible_candidate],
        });
        let ineligible_resp = reloaded
            .handle_command(AgentCommand::RunnersEnroll {
                local_runner_key: "runner-ineligible".to_owned(),
                prepare_only: false,
            })
            .unwrap();
        assert_eq!(
            ineligible_resp,
            AgentResponse::Rejected {
                reason_code: super::static_reason("runners-not-enrollment-eligible")
            }
        );

        // 11. ADR 0004 same-scope managed collision guard during multi-runner reconciliation
        let obs_collision = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Stopped,
                worker_present: false,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: true, // collision detected!
            },
        ];
        let rep_coll = reloaded.reconcile_runners(&obs_collision);
        let res_coll = rep_coll
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-b")
            .unwrap();
        assert_eq!(res_coll.desired, PerRunnerDesired::Drained);
        assert_eq!(res_coll.plan, PerRunnerPlan::WithdrawSelector);
        assert_eq!(res_coll.achieved, PerRunnerAchieved::Transitional);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_core_single_authority_reconciles_multi_runners_with_complete_precedence_matrix() {
        use crate::{
            DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity,
            DiscoveredServiceBinding, DiscoveryCapabilities, DiscoveryDisposition, DiscoveryReport,
            OpaqueIdentityReference, PerRunnerAchieved, PerRunnerDesired, PerRunnerObservation,
            PerRunnerPlan, RegistrationScope, RunnerBackendKind,
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        };

        let root = temporary_root();
        let identity = OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();

        let mut core = AgentCore::new(
            QueueObserver::new(vec![healthy_observation(); 16]),
            RecordingReconciler::default(),
            MemoryConfigStore::default(),
            build(),
        )
        .unwrap();

        let svc_exe = root.join("svc").join("RunnerService.exe");
        core.set_runner_discovery(DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![
                DiscoveredRunner {
                    local_discovery_key: "runner-svc".to_owned(),
                    disposition: DiscoveryDisposition::ObservedOnly,
                    refusal: None,
                    backend: RunnerBackendKind::WindowsService,
                    identity: DiscoveredRunnerIdentity {
                        registration_scope: Some(RegistrationScope::Repository {
                            owner: "owner".to_owned(),
                            repository: "repo-svc".to_owned(),
                        }),
                        runner_id: Some(101),
                        expected_runner_name: Some("fixture-svc".to_owned()),
                        runner_home: root.join("svc"),
                        work_root: Some(root.join("svc").join("_work")),
                        execution_identity_ref: Some(identity.clone()),
                    },
                    service: Some(DiscoveredServiceBinding {
                        service_name: "actions.runner.svc".to_owned(),
                        display_name: Some("Service Runner".to_owned()),
                        image_path: Some(svc_exe),
                        state: DiscoveredLifecycle::Listening,
                    }),
                    session: None,
                    capabilities: DiscoveryCapabilities {
                        service_control_contract: true,
                        user_session_control_contract: false,
                        enrollment_eligible: true,
                    },
                    same_scope_collision: false,
                    observed_lifecycle: DiscoveredLifecycle::Listening,
                },
                DiscoveredRunner {
                    local_discovery_key: "runner-sess".to_owned(),
                    disposition: DiscoveryDisposition::ObservedOnly,
                    refusal: None,
                    backend: RunnerBackendKind::UserSession,
                    identity: DiscoveredRunnerIdentity {
                        registration_scope: Some(RegistrationScope::Repository {
                            owner: "owner".to_owned(),
                            repository: "repo-sess".to_owned(),
                        }),
                        runner_id: Some(102),
                        expected_runner_name: Some("fixture-sess".to_owned()),
                        runner_home: root.join("sess"),
                        work_root: Some(root.join("sess").join("_work")),
                        execution_identity_ref: Some(identity.clone()),
                    },
                    service: None,
                    session: None,
                    capabilities: DiscoveryCapabilities {
                        service_control_contract: false,
                        user_session_control_contract: true,
                        enrollment_eligible: true,
                    },
                    same_scope_collision: false,
                    observed_lifecycle: DiscoveredLifecycle::Listening,
                },
            ],
        });

        // 1. Enroll both runners.
        for key in ["runner-svc", "runner-sess"] {
            core.handle_command(AgentCommand::RunnersEnroll {
                local_runner_key: key.to_owned(),
                prepare_only: false,
            })
            .unwrap();
        }

        // Set baseline user mode to Idle.
        core.handle_command(AgentCommand::SetMode {
            mode: UserMode::Idle,
        })
        .unwrap();

        // 2. Observations: both listening, selector present. Both should achieve Full.
        let obs_both_full = vec![
            PerRunnerObservation {
                local_runner_key: "runner-svc".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-sess".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];

        let rep1 = core.reconcile_runners(&obs_both_full);
        assert_eq!(rep1.results.len(), 2);
        assert_eq!(rep1.results[0].desired, PerRunnerDesired::Full);
        assert_eq!(rep1.results[0].achieved, PerRunnerAchieved::Full);
        assert_eq!(rep1.results[1].desired, PerRunnerDesired::Full);
        assert_eq!(rep1.results[1].achieved, PerRunnerAchieved::Full);

        // 3. Disable runner-svc while runner-sess remains enabled.
        // Proves A/B independence: runner-sess remains desired Full and achieved Full.
        core.handle_command(AgentCommand::RunnersDisable {
            local_runner_key: "runner-svc".to_owned(),
            when_idle: true,
            prepare_only: false,
        })
        .unwrap();

        let rep2 = core.reconcile_runners(&obs_both_full);
        let svc2 = rep2
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        let sess2 = rep2
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-sess")
            .unwrap();
        assert_eq!(svc2.desired, PerRunnerDesired::Drained);
        assert_eq!(svc2.plan, PerRunnerPlan::WithdrawSelector);
        assert_eq!(sess2.desired, PerRunnerDesired::Full);
        assert_eq!(sess2.achieved, PerRunnerAchieved::Full);
        assert_eq!(sess2.plan, PerRunnerPlan::Noop);

        // 4. Precedence: Force-CI mode set on AgentCore.
        // Explicit disable on runner-svc OUTRANKS Force-CI: runner-svc stays Drained!
        core.handle_command(AgentCommand::SetMode {
            mode: UserMode::ForceCi,
        })
        .unwrap();

        let rep3 = core.reconcile_runners(&obs_both_full);
        let svc3 = rep3
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        let sess3 = rep3
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-sess")
            .unwrap();
        assert_eq!(svc3.desired, PerRunnerDesired::Drained);
        assert_eq!(sess3.desired, PerRunnerDesired::Full);

        // 5. Precedence: Zen override enabled on AgentCore.
        // Zen outranks Force-CI: all runners drain/offline, but registry desired_enabled is never mutated.
        core.handle_command(AgentCommand::SetZen {
            zen: ZenOverride::Enabled,
        })
        .unwrap();

        let rep4 = core.reconcile_runners(&obs_both_full);
        let svc4 = rep4
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        let sess4 = rep4
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-sess")
            .unwrap();
        // Explicitly disabled runner desires Drained; enabled runner under Zen desires Offline:
        assert_eq!(svc4.desired, PerRunnerDesired::Drained);
        assert_eq!(svc4.plan, PerRunnerPlan::WithdrawSelector);
        assert_eq!(sess4.desired, PerRunnerDesired::Offline);
        assert_eq!(sess4.plan, PerRunnerPlan::WithdrawSelector);
        // Desired enabled in registry unchanged by Zen:
        assert!(
            !core
                .runner_registry()
                .get("runner-svc")
                .unwrap()
                .desired_enabled
        );
        assert!(
            core.runner_registry()
                .get("runner-sess")
                .unwrap()
                .desired_enabled
        );

        // 6. Worker busy preservation:
        // When runner-svc is busy with active worker, withdrawal plan must be HoldBusy, never killing worker.
        let obs_busy = vec![
            PerRunnerObservation {
                local_runner_key: "runner-svc".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Busy,
                worker_present: true,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-sess".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Stopped,
                worker_present: false,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep5 = core.reconcile_runners(&obs_busy);
        let svc5 = rep5
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        assert_eq!(svc5.plan, PerRunnerPlan::HoldBusy);
        assert_eq!(svc5.achieved, PerRunnerAchieved::Transitional);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_core_handles_registry_store_corruption_fail_closed() {
        use super::AgentCoreError;
        use crate::FileRegistryStore;

        let root = temporary_root();
        let registry_path = root.join("corrupt-runners-registry.json");
        fs::write(
            &registry_path,
            b"{\"schema_version\": 9999, \"invalid_syntax\": true}",
        )
        .unwrap();

        let store = MemoryConfigStore::default();
        let result = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation()]),
            RecordingReconciler::default(),
            store,
            build(),
            Box::new(FileRegistryStore::new(&registry_path)),
        );

        assert!(matches!(result, Err(AgentCoreError::Registry(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_cycle_multi_runner_disable_a_leaves_b_zero_control_actions() {
        use crate::{
            DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity,
            DiscoveredServiceBinding, DiscoveryCapabilities, DiscoveryDisposition, DiscoveryReport,
            LiveScmExecutionGate, MockScmBackend, MockServiceRecord, OpaqueIdentityReference,
            PerRunnerObservation, PrivilegedScmHelper, RegistrationScope, RunnerBackendKind,
            ScmApiError, ServiceObservation, ServiceStartMode, SyntheticScmTestGrant,
            WindowsScmBackend, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        };

        let root = temporary_root();
        let home_a = root.join("runner-a");
        let home_b = root.join("runner-b");
        fs::create_dir_all(&home_a).unwrap();
        fs::create_dir_all(&home_b).unwrap();

        let identity = OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "1111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap();

        let mut core = AgentCore::new(
            QueueObserver::new(vec![healthy_observation(); 16]),
            RecordingReconciler::default(),
            MemoryConfigStore::default(),
            build(),
        )
        .unwrap();

        // 1. Discover Runner A and Runner B as Windows services
        core.set_runner_discovery(DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![
                DiscoveredRunner {
                    local_discovery_key: "runner-a".to_owned(),
                    disposition: DiscoveryDisposition::ObservedOnly,
                    refusal: None,
                    backend: RunnerBackendKind::WindowsService,
                    identity: DiscoveredRunnerIdentity {
                        registration_scope: Some(RegistrationScope::Repository {
                            owner: "fixture-org".to_owned(),
                            repository: "repo-a".to_owned(),
                        }),
                        runner_id: Some(101),
                        expected_runner_name: Some("runner-a".to_owned()),
                        runner_home: home_a.clone(),
                        work_root: Some(home_a.join("_work")),
                        execution_identity_ref: Some(identity.clone()),
                    },
                    service: Some(DiscoveredServiceBinding {
                        service_name: "actions.runner.a".to_owned(),
                        display_name: Some("Runner A Service".to_owned()),
                        image_path: Some(home_a.join("bin").join("RunnerService.exe")),
                        state: DiscoveredLifecycle::Listening,
                    }),
                    session: None,
                    capabilities: DiscoveryCapabilities {
                        service_control_contract: true,
                        user_session_control_contract: false,
                        enrollment_eligible: true,
                    },
                    same_scope_collision: false,
                    observed_lifecycle: DiscoveredLifecycle::Listening,
                },
                DiscoveredRunner {
                    local_discovery_key: "runner-b".to_owned(),
                    disposition: DiscoveryDisposition::ObservedOnly,
                    refusal: None,
                    backend: RunnerBackendKind::WindowsService,
                    identity: DiscoveredRunnerIdentity {
                        registration_scope: Some(RegistrationScope::Repository {
                            owner: "fixture-org".to_owned(),
                            repository: "repo-b".to_owned(),
                        }),
                        runner_id: Some(102),
                        expected_runner_name: Some("runner-b".to_owned()),
                        runner_home: home_b.clone(),
                        work_root: Some(home_b.join("_work")),
                        execution_identity_ref: Some(identity.clone()),
                    },
                    service: Some(DiscoveredServiceBinding {
                        service_name: "actions.runner.b".to_owned(),
                        display_name: Some("Runner B Service".to_owned()),
                        image_path: Some(home_b.join("bin").join("RunnerService.exe")),
                        state: DiscoveredLifecycle::Listening,
                    }),
                    session: None,
                    capabilities: DiscoveryCapabilities {
                        service_control_contract: true,
                        user_session_control_contract: false,
                        enrollment_eligible: true,
                    },
                    same_scope_collision: false,
                    observed_lifecycle: DiscoveredLifecycle::Listening,
                },
            ],
        });

        // 2. Enroll both runners via regular Agent command entrypoint
        for key in ["runner-a", "runner-b"] {
            core.handle_command(AgentCommand::RunnersEnroll {
                local_runner_key: key.to_owned(),
                prepare_only: false,
            })
            .unwrap();
        }

        // Set baseline user mode to Idle via regular Agent command entrypoint
        core.handle_command(AgentCommand::SetMode {
            mode: UserMode::Idle,
        })
        .unwrap();

        // 3. Set up mock SCM backend with initial state for both runners
        let image_token_a = crate::runner_service_image_token(&home_a);
        let image_token_b = crate::runner_service_image_token(&home_b);

        let mock_scm = MockScmBackend::new();
        mock_scm.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "actions.runner.a".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            image_token: image_token_a.clone(),
            execution_identity_token: identity.key.clone(),
        });
        mock_scm.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "actions.runner.b".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: true, // Runner B is actively executing a CI job
            image_token: image_token_b.clone(),
            execution_identity_token: identity.key.clone(),
        });

        // Synthetic gate authorized for actions.runner.a Stop
        let gate_a = LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
            "actions.runner.a",
            &image_token_a,
            false,
            true,
            false,
        ))
        .unwrap();
        let helper_a = PrivilegedScmHelper::new(mock_scm.clone(), gate_a);

        // 4. Initial cycle: both runners are enabled.
        // Runner A: Listening, Runner B: Listening & Busy with job
        let obs_cycle1 = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];

        // Before any disable, dispatch produces ZERO actions (both enabled and running)
        let reports1 = core.reconcile_and_dispatch_runners(&obs_cycle1, &helper_a, "tx-cycle-1");
        assert!(reports1.is_empty());
        assert_eq!(mock_scm.stop_calls(), 0);
        assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
        assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
        assert!(mock_scm.service_actions("actions.runner.a").is_empty());
        assert!(mock_scm.service_actions("actions.runner.b").is_empty());

        // 5. Disable Runner A via regular AgentCommand entrypoint
        let disable_resp = core
            .handle_command(AgentCommand::RunnersDisable {
                local_runner_key: "runner-a".to_owned(),
                when_idle: true,
                prepare_only: false,
            })
            .unwrap();
        assert!(matches!(disable_resp, AgentResponse::Runners(_)));

        // Verify Runner A is disabled, Runner B remains enabled
        assert!(
            !core
                .runner_registry()
                .get("runner-a")
                .unwrap()
                .desired_enabled
        );
        assert!(
            core.runner_registry()
                .get("runner-b")
                .unwrap()
                .desired_enabled
        );

        // 6. Regular Agent cycle with Runner A disabled.
        // Phase 1: Selector still present (withdrawing) -> MUST NOT stop A!
        let obs_selector_present = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep_sel =
            core.reconcile_and_dispatch_runners(&obs_selector_present, &helper_a, "tx-sel");
        assert!(
            rep_sel.is_empty(),
            "MUST NOT stop A while selector is still present"
        );
        assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);

        // Phase 2: Withdrawal status is unknown -> MUST NOT stop A!
        let obs_selector_unknown = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: None,
                selector_observation_unknown: true,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep_unk =
            core.reconcile_and_dispatch_runners(&obs_selector_unknown, &helper_a, "tx-unk");
        assert!(
            rep_unk.is_empty(),
            "MUST NOT stop A while selector status is unknown"
        );
        assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);

        // Phase 3: Selector confirmed absent, but Worker is still busy executing a job -> MUST NOT stop A!
        let obs_worker_busy = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Busy,
                worker_present: true,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep_busy = core.reconcile_and_dispatch_runners(&obs_worker_busy, &helper_a, "tx-busy");
        assert!(rep_busy.is_empty(), "MUST NOT stop A while Worker is busy");
        assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);

        // Phase 4: Independent readback confirms selector absent AND Worker idle AND plan allows stop
        let obs_drained = vec![
            PerRunnerObservation {
                local_runner_key: "runner-a".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: false,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "runner-b".to_owned(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Listening,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ];
        let rep_drain = core.reconcile_and_dispatch_runners(&obs_drained, &helper_a, "tx-drain");
        assert_eq!(rep_drain.len(), 1);
        assert_eq!(rep_drain[0].service_name, "actions.runner.a");
        assert!(rep_drain[0].applied);
        assert_eq!(rep_drain[0].achieved_state, DiscoveredLifecycle::Stopped);

        // Runner A service was stopped exactly once (in Phase 4)
        assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 1);
        assert_eq!(
            mock_scm.service_actions("actions.runner.a"),
            vec!["stop".to_string()]
        );

        // CRUCIAL ISOLATION PROOF: Runner B received ZERO control actions across all phases
        assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
        assert_eq!(mock_scm.service_start_calls("actions.runner.b"), 0);
        assert!(mock_scm.service_actions("actions.runner.b").is_empty());

        // 7. Defense-in-depth: Live Windows SCM backend unconditionally refuses mutation (GateClosed)
        let inert_helper = PrivilegedScmHelper::new(
            WindowsScmBackend::mutation_inert(),
            LiveScmExecutionGate::CLOSED,
        );
        let reports_inert =
            core.reconcile_and_dispatch_runners(&obs_drained, &inert_helper, "tx-inert");
        assert_eq!(reports_inert.len(), 1);
        assert!(!reports_inert[0].applied);
        assert_eq!(
            reports_inert[0].refusal_reason.as_deref(),
            Some(ScmApiError::GateClosed.as_str())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn regression_c2_authoritative_backend_exact_scm_readback_and_fail_closed() {
        let mock_scm = crate::MockScmBackend::new();
        mock_scm.insert(crate::MockServiceRecord {
            observation: crate::ServiceObservation {
                service_name: "actions.runner.live-1".to_string(),
                state: DiscoveredLifecycle::Listening,
                start_mode: crate::ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            image_token: "tok-1".to_string(),
            execution_identity_token: "ident-1".to_string(),
        });

        let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
        let backend = AuthoritativeMultiRunnerLifecycleBackend::new(helper);

        // 1. Exact observation readback
        let obs = backend.observe_runners(&[
            "actions.runner.live-1".to_string(),
            "actions.runner.unknown".to_string(),
        ]);
        assert_eq!(obs.len(), 2);
        assert_eq!(obs[0].local_runner_key, "actions.runner.live-1");
        assert_eq!(obs[0].lifecycle, DiscoveredLifecycle::Listening);
        assert!(obs[0].ownership_verified);
        assert!(!obs[0].worker_present);

        assert_eq!(obs[1].local_runner_key, "actions.runner.unknown");
        assert_eq!(obs[1].lifecycle, DiscoveredLifecycle::Unknown);
        assert!(!obs[1].ownership_verified);

        // 2. Fail-closed selector actions without live admission authorization
        let scope = crate::RegistrationScope::Repository {
            owner: "org".to_string(),
            repository: "repo".to_string(),
        };
        assert!(backend.withdraw_selector(&scope, 101).is_err());
        assert!(backend.advertise_selector(&scope, 101).is_err());
    }

    #[test]
    fn regression_c2_reconcile_dispatches_advertise_selector() {
        let root = std::env::temp_dir().join(format!("test-adv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let mock_scm = crate::MockScmBackend::new();
        let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
        let sandbox_backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper));

        let store = MemoryConfigStore::new(AgentConfig::default());
        let mut core = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation(); 10]),
            RecordingReconciler::default(),
            store,
            BuildProvenance {
                version: "0.1.0".to_string(),
                commit: "test".to_string(),
                channel: "dev".to_string(),
                target: "win".to_string(),
            },
            Box::new(crate::FileRegistryStore::new(root.join("reg.json"))),
        )
        .unwrap();
        core.set_multi_runner_backend(sandbox_backend.clone());

        let identity = crate::OpaqueIdentityReference::new(
            crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )
        .unwrap();

        // Enroll a runner
        let discovery = DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![crate::DiscoveredRunner {
                local_discovery_key: "runner-adv".to_string(),
                disposition: crate::DiscoveryDisposition::ObservedOnly,
                refusal: None,
                backend: crate::RunnerBackendKind::WindowsService,
                identity: crate::DiscoveredRunnerIdentity {
                    registration_scope: Some(crate::RegistrationScope::Repository {
                        owner: "org-a".to_string(),
                        repository: "repo-a".to_string(),
                    }),
                    runner_id: Some(555),
                    expected_runner_name: Some("runner-adv".to_string()),
                    runner_home: root.join("home"),
                    work_root: Some(root.join("work")),
                    execution_identity_ref: Some(identity.clone()),
                },
                service: Some(crate::DiscoveredServiceBinding {
                    service_name: "actions.runner.adv".to_string(),
                    display_name: None,
                    image_path: None,
                    state: DiscoveredLifecycle::Listening,
                }),
                session: None,
                capabilities: crate::DiscoveryCapabilities {
                    service_control_contract: true,
                    user_session_control_contract: false,
                    enrollment_eligible: true,
                },
                same_scope_collision: false,
                observed_lifecycle: DiscoveredLifecycle::Listening,
            }],
        };
        core.set_runner_discovery(discovery);
        core.handle_command(AgentCommand::RunnersEnroll {
            local_runner_key: "runner-adv".to_string(),
            prepare_only: false,
        })
        .unwrap();

        // Runner is enabled, mode is ForceCi -> Reconcile will plan AdvertiseSelector
        core.handle_command(AgentCommand::SetMode {
            mode: UserMode::ForceCi,
        })
        .unwrap();

        sandbox_backend.set_observations(vec![PerRunnerObservation {
            local_runner_key: "runner-adv".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(false), // Needs to be advertised!
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        }]);

        let _ = core.observe_decide_reconcile().unwrap();

        let adv = sandbox_backend.advertised_selectors();
        assert_eq!(
            adv.len(),
            1,
            "AdvertiseSelector must be dispatched to backend"
        );
        assert_eq!(adv[0].1, 555);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn regression_c3_reconcile_dispatches_user_session_action() {
        let root = std::env::temp_dir().join(format!("test-session-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let mock_scm = crate::MockScmBackend::new();
        let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
        let sandbox_backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper));

        let store = MemoryConfigStore::new(AgentConfig::default());
        let mut core = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation(); 10]),
            RecordingReconciler::default(),
            store,
            BuildProvenance {
                version: "0.1.0".to_string(),
                commit: "test".to_string(),
                channel: "dev".to_string(),
                target: "win".to_string(),
            },
            Box::new(crate::FileRegistryStore::new(root.join("reg.json"))),
        )
        .unwrap();
        core.set_multi_runner_backend(sandbox_backend.clone());

        let identity = crate::OpaqueIdentityReference::new(
            crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )
        .unwrap();

        // Enroll a UserSession runner
        let discovery = DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![crate::DiscoveredRunner {
                local_discovery_key: "runner-sess".to_string(),
                disposition: crate::DiscoveryDisposition::ObservedOnly,
                refusal: None,
                backend: crate::RunnerBackendKind::UserSession,
                identity: crate::DiscoveredRunnerIdentity {
                    registration_scope: Some(crate::RegistrationScope::Repository {
                        owner: "org-sess".to_string(),
                        repository: "repo-sess".to_string(),
                    }),
                    runner_id: Some(777),
                    expected_runner_name: Some("runner-sess".to_string()),
                    runner_home: root.join("home"),
                    work_root: Some(root.join("work")),
                    execution_identity_ref: Some(identity.clone()),
                },
                service: None,
                session: Some(crate::DiscoveredSessionBinding {
                    current_user_owned: true,
                    listener_present: true,
                    worker_present: false,
                }),
                capabilities: crate::DiscoveryCapabilities {
                    service_control_contract: false,
                    user_session_control_contract: true,
                    enrollment_eligible: true,
                },
                same_scope_collision: false,
                observed_lifecycle: DiscoveredLifecycle::Listening,
            }],
        };
        core.set_runner_discovery(discovery);
        core.handle_command(AgentCommand::RunnersEnroll {
            local_runner_key: "runner-sess".to_string(),
            prepare_only: false,
        })
        .unwrap();

        // Disable runner -> should dispatch RequestGracefulStop
        core.handle_command(AgentCommand::RunnersDisable {
            local_runner_key: "runner-sess".to_string(),
            when_idle: true,
            prepare_only: false,
        })
        .unwrap();

        sandbox_backend.set_observations(vec![PerRunnerObservation {
            local_runner_key: "runner-sess".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(false),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        }]);

        let _ = core.observe_decide_reconcile().unwrap();

        let actions = sandbox_backend.dispatched_session_actions();
        assert_eq!(
            actions.len(),
            1,
            "Graceful stop must be dispatched for user session runner"
        );
        assert_eq!(actions[0].0, "runner-sess");
        assert_eq!(
            actions[0].1,
            crate::runner_session_backend::PreparedSessionAction::RequestGracefulStop
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(windows)]
    fn regression_c3_v01_binding_migration_is_idempotent_and_persisted() {
        let root = std::env::temp_dir().join(format!("test-mig-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let reg_path = root.join("reg.json");

        let store = MemoryConfigStore::new(AgentConfig::default());
        let mut core = AgentCore::with_registry_store(
            crate::agent_runtime::RuntimeObserver::new(None, Vec::new()),
            crate::agent_runtime::RuntimeReconciler::Unconfigured(
                crate::agent_runtime::NoRunnerControl::new(),
            ),
            store,
            BuildProvenance {
                version: "0.1.0".to_string(),
                commit: "test".to_string(),
                channel: "dev".to_string(),
                target: "win".to_string(),
            },
            Box::new(crate::FileRegistryStore::new(reg_path.clone())),
        )
        .unwrap();

        let scope = crate::RegistrationScope::Repository {
            owner: "owner-mig".to_string(),
            repository: "repo-mig".to_string(),
        };
        let binding = crate::InstalledRuntimeBinding::new(
            crate::AdmissionBinding::new(
                scope.clone(),
                888,
                "legacy-runner",
                crate::CredentialReference::new("windows-credential-manager", "fixture-ref")
                    .unwrap(),
                Some(crate::ReservedLabelOwnership::for_runner(scope, 888)),
            )
            .unwrap(),
            crate::ExactLocalRunnerBinding::new(
                root.join("runner_home"),
                root.join("work"),
                crate::OpaqueIdentityReference::new(
                    crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
                    "0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
            )
            .unwrap(),
            Vec::new(),
        )
        .unwrap();

        // First migration succeeds and creates record
        let migrated_key = core
            .migrate_v01_binding_if_needed(&binding, None)
            .unwrap()
            .unwrap();
        assert_eq!(core.runner_registry().records.len(), 1);
        let record = core.runner_registry().get(&migrated_key).unwrap();
        assert_eq!(record.runner_id, 888);

        // Second migration is idempotent (no-op)
        let second_migrated = core
            .migrate_v01_binding_if_needed(&binding, None)
            .unwrap()
            .unwrap();
        assert_eq!(migrated_key, second_migrated);
        assert_eq!(core.runner_registry().records.len(), 1);

        // Reload from disk store to prove durability
        let loaded = crate::FileRegistryStore::new(reg_path)
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(
            loaded.legacy_v01_binding_key.as_deref(),
            Some(migrated_key.as_str())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn regression_c2_authoritative_backend_with_admission_coordinator_loop() {
        let mock_scm = crate::MockScmBackend::new();
        mock_scm.insert(crate::MockServiceRecord {
            observation: crate::ServiceObservation {
                service_name: "actions.runner.live-c2".to_string(),
                state: DiscoveredLifecycle::Listening,
                start_mode: crate::ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            image_token: "tok-c2".to_string(),
            execution_identity_token: "ident-c2".to_string(),
        });

        let scope = crate::RegistrationScope::Repository {
            owner: "c2-owner".to_string(),
            repository: "c2-repo".to_string(),
        };

        let admission_coord =
            Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope.clone(), 555, true));
        let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
        let backend = AuthoritativeMultiRunnerLifecycleBackend::new(helper)
            .with_admission_coordinator(admission_coord.clone());

        backend.register_runner_target("runner-c2", scope.clone(), 555);

        // 1. Live observation with attached coordinator
        let obs = backend.observe_runners(&["runner-c2".to_string()]);
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].selector_present, Some(true));
        assert!(!obs[0].selector_observation_unknown);

        // 2. Withdrawal through coordinator
        assert!(backend.withdraw_selector(&scope, 555).unwrap());
        let obs_after_withdraw = backend.observe_runners(&["runner-c2".to_string()]);
        assert_eq!(obs_after_withdraw[0].selector_present, Some(false));
        assert!(!obs_after_withdraw[0].selector_observation_unknown);

        // 3. Readvertise through coordinator
        assert!(backend.advertise_selector(&scope, 555).unwrap());
        let obs_after_adv = backend.observe_runners(&["runner-c2".to_string()]);
        assert_eq!(obs_after_adv[0].selector_present, Some(true));
    }

    #[test]
    fn regression_c3_authoritative_backend_session_coordinator_dispatch() {
        let root = std::env::temp_dir().join(format!("test-c3-sess-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let runner_home = root.join("runner_sess");
        fs::create_dir_all(&runner_home).unwrap();
        let run_cmd = runner_home.join("run.cmd");
        fs::write(&run_cmd, b"@echo off\r\n").unwrap();
        let work_root = root.join("work_sess");
        fs::create_dir_all(&work_root).unwrap();

        let identity = OpaqueIdentityReference::new(
            crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "2222222222222222222222222222222222222222222222222222222222222222",
        )
        .unwrap();

        let mut record = RunnerRecord {
            local_runner_key: "runner-sess-test".to_string(),
            registration_scope: crate::RegistrationScope::Repository {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
            },
            runner_id: 777,
            expected_runner_name: "sess-runner".to_string(),
            runner_home: runner_home.clone(),
            work_root: work_root.clone(),
            execution_identity_ref: identity,
            backend: RegistryBackendBinding::UserSession,
            desired_enabled: true,
            enrollment_state: crate::EnrollmentState::Enrolled,
            credential_ref: None,
            managed_admission: false,
        };

        let helper =
            PrivilegedScmHelper::new(crate::MockScmBackend::new(), LiveScmExecutionGate::CLOSED);
        let backend_inert = AuthoritativeMultiRunnerLifecycleBackend::new(helper);

        // Default unconfigured session coordinator is sealed closed (inert) in production
        assert_eq!(
            backend_inert.dispatch_session(
                &record,
                crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener
            ),
            Err("session.transport-closed-production-inert".to_string())
        );

        let transport = Arc::new(SimulatedUserSessionProcessTransport::new());
        let backend = AuthoritativeMultiRunnerLifecycleBackend::new(PrivilegedScmHelper::new(
            crate::MockScmBackend::new(),
            LiveScmExecutionGate::CLOSED,
        ))
        .with_session_coordinator(Arc::new(StandardSessionCoordinator::new(transport.clone())));

        // Valid session dispatch for graceful stop and launch
        assert!(backend
            .dispatch_session(
                &record,
                crate::runner_session_backend::PreparedSessionAction::RequestGracefulStop
            )
            .unwrap());
        assert!(backend
            .dispatch_session(
                &record,
                crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener
            )
            .unwrap());
        assert_eq!(
            *transport.launched.lock().unwrap(),
            vec!["runner-sess-test".to_string()]
        );

        // Fail-closed when run.cmd is missing
        fs::remove_file(&run_cmd).unwrap();
        let err = backend.dispatch_session(
            &record,
            crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener,
        );
        assert_eq!(err, Err("session.entrypoint-missing".to_string()));

        // Fail-closed when runner_home is missing
        fs::remove_dir_all(&runner_home).unwrap();
        let err = backend.dispatch_session(
            &record,
            crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener,
        );
        assert_eq!(err, Err("session.runner-home-missing".to_string()));

        // Fail-closed when not user session backend
        record.backend = RegistryBackendBinding::WindowsService {
            service_name: "svc".to_string(),
        };
        let err = backend.dispatch_session(
            &record,
            crate::runner_session_backend::PreparedSessionAction::LaunchOwnedListener,
        );
        assert_eq!(err, Err("session.not-user-session-backend".to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn regression_c4_batch_enable_all_and_disable_all_ux() {
        let root = std::env::temp_dir().join(format!("test-c4-ux-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let store = MemoryConfigStore::new(AgentConfig::default());
        let mut core = AgentCore::with_registry_store(
            QueueObserver::new(vec![healthy_observation(); 10]),
            RecordingReconciler::default(),
            store,
            BuildProvenance {
                version: "0.1.0".to_string(),
                commit: "test".to_string(),
                channel: "dev".to_string(),
                target: "win".to_string(),
            },
            Box::new(crate::FileRegistryStore::new(root.join("reg.json"))),
        )
        .unwrap();

        let identity = OpaqueIdentityReference::new(
            crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "3333333333333333333333333333333333333333333333333333333333333333",
        )
        .unwrap();

        let home_1 = root.join("runner_1");
        let home_2 = root.join("runner_2");
        fs::create_dir_all(&home_1).unwrap();
        fs::create_dir_all(&home_2).unwrap();

        let discovery = DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![
                DiscoveredRunner {
                    local_discovery_key: "r1".to_string(),
                    disposition: DiscoveryDisposition::ObservedOnly,
                    refusal: None,
                    backend: RunnerBackendKind::WindowsService,
                    identity: DiscoveredRunnerIdentity {
                        registration_scope: Some(crate::RegistrationScope::Repository {
                            owner: "o".to_string(),
                            repository: "r1".to_string(),
                        }),
                        runner_id: Some(1),
                        expected_runner_name: Some("r1".to_string()),
                        runner_home: home_1.clone(),
                        work_root: Some(home_1.join("_work")),
                        execution_identity_ref: Some(identity.clone()),
                    },
                    service: Some(DiscoveredServiceBinding {
                        service_name: "actions.runner.r1".to_string(),
                        display_name: None,
                        image_path: None,
                        state: DiscoveredLifecycle::Listening,
                    }),
                    session: None,
                    capabilities: crate::DiscoveryCapabilities {
                        service_control_contract: true,
                        user_session_control_contract: false,
                        enrollment_eligible: true,
                    },
                    same_scope_collision: false,
                    observed_lifecycle: DiscoveredLifecycle::Listening,
                },
                DiscoveredRunner {
                    local_discovery_key: "r2".to_string(),
                    disposition: DiscoveryDisposition::ObservedOnly,
                    refusal: None,
                    backend: RunnerBackendKind::WindowsService,
                    identity: DiscoveredRunnerIdentity {
                        registration_scope: Some(crate::RegistrationScope::Repository {
                            owner: "o".to_string(),
                            repository: "r2".to_string(),
                        }),
                        runner_id: Some(2),
                        expected_runner_name: Some("r2".to_string()),
                        runner_home: home_2.clone(),
                        work_root: Some(home_2.join("_work")),
                        execution_identity_ref: Some(identity),
                    },
                    service: Some(DiscoveredServiceBinding {
                        service_name: "actions.runner.r2".to_string(),
                        display_name: None,
                        image_path: None,
                        state: DiscoveredLifecycle::Listening,
                    }),
                    session: None,
                    capabilities: crate::DiscoveryCapabilities {
                        service_control_contract: true,
                        user_session_control_contract: false,
                        enrollment_eligible: true,
                    },
                    same_scope_collision: false,
                    observed_lifecycle: DiscoveredLifecycle::Listening,
                },
            ],
        };
        core.set_runner_discovery(discovery);

        // Enroll r1 and r2
        core.handle_command(AgentCommand::RunnersEnroll {
            local_runner_key: "r1".to_string(),
            prepare_only: false,
        })
        .unwrap();
        core.handle_command(AgentCommand::RunnersEnroll {
            local_runner_key: "r2".to_string(),
            prepare_only: false,
        })
        .unwrap();

        // 1. Batch disable-all
        let AgentResponse::Runners(disable_report) = core
            .handle_command(AgentCommand::RunnersDisableAll {
                when_idle: true,
                prepare_only: false,
            })
            .unwrap()
        else {
            panic!("expected runners response");
        };
        assert!(disable_report.applied);
        assert_eq!(disable_report.records.len(), 2);
        assert!(!disable_report.records[0].desired_enabled);
        assert!(!disable_report.records[1].desired_enabled);
        assert_eq!(disable_report.records[0].item_applied, Some(true));
        assert_eq!(disable_report.records[1].item_applied, Some(true));

        // 2. Batch enable-all
        let AgentResponse::Runners(enable_report) = core
            .handle_command(AgentCommand::RunnersEnableAll {
                prepare_only: false,
            })
            .unwrap()
        else {
            panic!("expected runners response");
        };
        assert!(enable_report.applied);
        assert!(enable_report.records[0].desired_enabled);
        assert!(enable_report.records[1].desired_enabled);
        assert_eq!(enable_report.records[0].item_applied, Some(true));
        assert_eq!(enable_report.records[1].item_applied, Some(true));

        // 3. Status reflects attached observation
        let helper =
            PrivilegedScmHelper::new(crate::MockScmBackend::new(), LiveScmExecutionGate::CLOSED);
        let sandbox_backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper));
        sandbox_backend.set_observations(vec![
            PerRunnerObservation {
                local_runner_key: "r1".to_string(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Busy,
                worker_present: true,
                selector_present: Some(true),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
            PerRunnerObservation {
                local_runner_key: "r2".to_string(),
                ownership_verified: true,
                lifecycle: DiscoveredLifecycle::Stopped,
                worker_present: false,
                selector_present: Some(false),
                selector_observation_unknown: false,
                legacy_generic_label_routing: false,
                same_scope_managed_collision: false,
            },
        ]);
        core.set_multi_runner_backend(sandbox_backend);

        let AgentResponse::Runners(status_report) = core
            .handle_command(AgentCommand::RunnersStatus {
                local_runner_key: None,
            })
            .unwrap()
        else {
            panic!("expected runners response");
        };
        let r1 = status_report
            .records
            .iter()
            .find(|r| r.local_runner_key == "r1")
            .unwrap();
        let r2 = status_report
            .records
            .iter()
            .find(|r| r.local_runner_key == "r2")
            .unwrap();
        assert_eq!(r1.presentation_status, RunnerPresentationStatus::Busy);
        assert_eq!(r2.presentation_status, RunnerPresentationStatus::Stopped);

        let _ = fs::remove_dir_all(root);
    }
}
