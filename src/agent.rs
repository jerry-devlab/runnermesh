use std::{
    cell::RefCell,
    collections::BTreeMap,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    decide_admission, AdmissionControlSnapshot, AdmissionDecision, AgentCommand, AgentHealth,
    AgentResponse, AgentSnapshot, BuildProvenance, DesiredAdmissionState, DoctorCheck,
    DoctorReport, DoctorStatus, ExecutionIdentityEvidence, HardSafetyState, LinkKind, LinkSnapshot,
    LinkState, OwnershipEvidence, ProbeId, ProbeRuntimeState, ProbeSnapshot, ReasonCode,
    RunnerPhase, SystemPreferences, UiPreferences, UserMode, ZenOverride,
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
        let config = store.load()?.unwrap_or_default();
        validate_schema(&config)?;
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
        })
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
    Observer(String),
    Reconciler(String),
}

impl fmt::Display for AgentCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Observer(error) => write!(formatter, "observer failed: {error}"),
            Self::Reconciler(error) => write!(formatter, "reconciler failed: {error}"),
        }
    }
}

impl std::error::Error for AgentCoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
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
}
