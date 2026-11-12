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
    RunnerRegistry, RunnerRegistryEditor, ScmControlBackend, SystemPreferences, UiPreferences,
    UserMode, ZenOverride,
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
}

/// Thread-safe sandbox multi-runner lifecycle backend for tests and simulated qualification.
pub struct SandboxMultiRunnerLifecycleBackend<B> {
    helper: PrivilegedScmHelper<B>,
    observations: Arc<Mutex<Vec<PerRunnerObservation>>>,
    withdrawn_selectors: Arc<Mutex<Vec<(crate::RegistrationScope, u64)>>>,
}

impl<B: ScmControlBackend + Send + Sync + 'static> SandboxMultiRunnerLifecycleBackend<B> {
    pub fn new(helper: PrivilegedScmHelper<B>) -> Self {
        Self {
            helper,
            observations: Arc::new(Mutex::new(Vec::new())),
            withdrawn_selectors: Arc::new(Mutex::new(Vec::new())),
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

            let service_name = match &record.backend {
                RegistryBackendBinding::WindowsService { service_name } => service_name.clone(),
                RegistryBackendBinding::UserSession => continue,
            };

            let obs = observations
                .iter()
                .find(|o| o.local_runner_key == result.local_runner_key);

            // Step 1: If plan requires selector withdrawal, coordinate withdrawal
            if result.plan == PerRunnerPlan::WithdrawSelector {
                let _ = backend.withdraw_selector(&record.registration_scope, record.runner_id);
            }

            // Step 2: Strictly verify dispatch guards before issuing StopService
            let selector_confirmed_absent = obs
                .map(|o| o.selector_present == Some(false) && !o.selector_observation_unknown)
                .unwrap_or(false);

            let worker_idle = result.observed_lifecycle != DiscoveredLifecycle::Busy
                && !obs.map(|o| o.worker_present).unwrap_or(false);

            let plan_allows_stop = result.plan == PerRunnerPlan::Noop
                && (result.achieved == PerRunnerAchieved::Drained
                    || result.achieved == PerRunnerAchieved::Offline);

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
                reports.push(backend.dispatch_scm(&request));
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
                    || result.achieved == PerRunnerAchieved::Offline);

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
            Ok(report) => {
                if report.applied {
                    if let Err(error) = self.persist_registry() {
                        self.runner_registry = registry_before;
                        return Err(error);
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

        DoctorReport {
            checks: vec![DoctorCheck {
                id: static_reason("agent-health"),
                status,
                reason_code: self
                    .observation
                    .health_reason_code
                    .clone()
                    .or(fallback_reason),
            }],
        }
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

    use super::{
        write_bytes_atomically_with, AgentConfig, AgentCore, AgentObservation, AgentObserver,
        AgentReconciler, ConfigStore, ConfigStoreError, FileConfigStore, MemoryConfigStore,
        CONFIG_SCHEMA_VERSION,
    };
    use crate::{
        AdmissionControlSnapshot, AdmissionDecision, AgentCommand, AgentHealth, AgentResponse,
        BuildProvenance, DesiredAdmissionState, ExecutionIdentityEvidence, LinkKind, LinkSnapshot,
        LinkState, NodeState, OwnershipEvidence, ProbeId, ProbeRuntimeState, ProbeSnapshot,
        ReasonCode, RunnerPhase, SystemPreferences, ThemePreference, UiPreferences, UserMode,
        ZenOverride,
    };

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
}
