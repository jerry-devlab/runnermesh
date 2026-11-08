//! Versioned local Runner registry persistence (LRM-02).
//!
//! Stores non-secret per-runner enrollment and desired-enabled intent only.
//! Observations are never persisted as truth. Credential *values* are never
//! serialized; only opaque credential references may appear. This module does
//! not start/stop services, mutate GitHub registrations, or alter v0.1
//! `AgentConfig` schema.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    installation::{atomic_write, guard_existing_directories, is_reparse_point},
    AdmissionBinding, CredentialReference, DiscoveredRunner, DiscoveryDisposition,
    InstalledRuntimeBinding, OpaqueIdentityReference, RegistrationScope, RunnerBackendKind,
    RESERVED_ADMISSION_LABEL,
};

pub const RUNNER_REGISTRY_SCHEMA_VERSION: u32 = 1;

/// Persisted enrollment boundary for one exact runner identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnrollmentState {
    Enrolled,
    /// Record retained after operator forget without unregistering the official runner.
    Forgotten,
}

/// Persisted backend binding. Control execution remains out of scope for LRM-02.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RegistryBackendBinding {
    WindowsService { service_name: String },
    UserSession,
}

impl RegistryBackendBinding {
    pub fn kind(&self) -> RunnerBackendKind {
        match self {
            Self::WindowsService { .. } => RunnerBackendKind::WindowsService,
            Self::UserSession => RunnerBackendKind::UserSession,
        }
    }

    fn is_valid(&self) -> bool {
        match self {
            Self::WindowsService { service_name } => {
                !service_name.trim().is_empty()
                    && service_name.len() <= 256
                    && !service_name.contains(['\0', '\r', '\n'])
            }
            Self::UserSession => true,
        }
    }
}

/// One persisted non-secret runner record.
///
/// Omits `Debug` because opaque identity references are intentionally non-Debug.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRecord {
    pub local_runner_key: String,
    pub registration_scope: RegistrationScope,
    pub runner_id: u64,
    pub expected_runner_name: String,
    pub runner_home: PathBuf,
    pub work_root: PathBuf,
    pub execution_identity_ref: OpaqueIdentityReference,
    pub backend: RegistryBackendBinding,
    pub desired_enabled: bool,
    pub enrollment_state: EnrollmentState,
    pub credential_ref: Option<CredentialReference>,
    /// When true, this record claims managed reserved-label admission ownership
    /// for its registration scope. Same-scope collisions are refused.
    pub managed_admission: bool,
}

impl RunnerRecord {
    pub fn is_valid(&self) -> bool {
        !self.local_runner_key.trim().is_empty()
            && self.local_runner_key.len() <= 256
            && !self
                .local_runner_key
                .contains(['\0', '\r', '\n', '/', '\\'])
            && self.registration_scope_valid()
            && self.runner_id != 0
            && !self.expected_runner_name.trim().is_empty()
            && !self.expected_runner_name.contains(['\r', '\n'])
            && absolute_path_like(&self.runner_home)
            && absolute_path_like(&self.work_root)
            && self.execution_identity_ref.is_valid()
            && self.backend.is_valid()
            && self
                .credential_ref
                .as_ref()
                .map(CredentialReference::is_valid)
                .unwrap_or(true)
    }

    fn registration_scope_valid(&self) -> bool {
        // RegistrationScope::validate is private; round-trip through AdmissionBinding
        // rules by checking path components via JSON shape we already trust from
        // serde + non-empty known fields.
        match &self.registration_scope {
            RegistrationScope::Organization { organization } => valid_scope_component(organization),
            RegistrationScope::Repository { owner, repository } => {
                valid_scope_component(owner) && valid_scope_component(repository)
            }
        }
    }

    pub fn is_active_enrollment(&self) -> bool {
        self.enrollment_state == EnrollmentState::Enrolled
    }
}

/// Versioned registry document persisted beside Agent config, not inside it.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRegistry {
    pub schema_version: u32,
    pub records: Vec<RunnerRecord>,
    /// Optional key of the record migrated from the v0.1 single binding.
    pub legacy_v01_binding_key: Option<String>,
}

impl Default for RunnerRegistry {
    fn default() -> Self {
        Self {
            schema_version: RUNNER_REGISTRY_SCHEMA_VERSION,
            records: Vec::new(),
            legacy_v01_binding_key: None,
        }
    }
}

impl RunnerRegistry {
    pub fn is_valid(&self) -> bool {
        self.schema_version == RUNNER_REGISTRY_SCHEMA_VERSION
            && self.records.iter().all(RunnerRecord::is_valid)
            && unique_keys(&self.records)
            && self.legacy_v01_binding_key.as_ref().is_none_or(|key| {
                self.records
                    .iter()
                    .any(|record| record.local_runner_key == *key)
            })
    }

    pub fn active_records(&self) -> impl Iterator<Item = &RunnerRecord> {
        self.records
            .iter()
            .filter(|record| record.is_active_enrollment())
    }

    pub fn get(&self, local_runner_key: &str) -> Option<&RunnerRecord> {
        self.records
            .iter()
            .find(|record| record.local_runner_key == local_runner_key)
    }

    pub fn get_mut(&mut self, local_runner_key: &str) -> Option<&mut RunnerRecord> {
        self.records
            .iter_mut()
            .find(|record| record.local_runner_key == local_runner_key)
    }
}

/// Failures from schema-validated registry persistence and enrollment.
#[derive(Debug)]
pub enum RegistryError {
    Io(io::Error),
    Json(serde_json::Error),
    IncompatibleSchema { found: u32, supported: u32 },
    InvalidRecord(&'static str),
    DuplicateKey,
    DuplicateHome,
    DuplicateWorkRoot,
    DuplicateServerIdentity,
    SameScopeManagedAdmissionCollision,
    NotEnrollmentEligible,
    NotFound,
    Forgotten,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "registry I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "registry JSON failed: {error}"),
            Self::IncompatibleSchema { found, supported } => write!(
                formatter,
                "registry schema {found} is incompatible with {supported}"
            ),
            Self::InvalidRecord(reason) => write!(formatter, "invalid runner record: {reason}"),
            Self::DuplicateKey => formatter.write_str("duplicate local runner key"),
            Self::DuplicateHome => formatter.write_str("duplicate runner home"),
            Self::DuplicateWorkRoot => formatter.write_str("duplicate runner work root"),
            Self::DuplicateServerIdentity => {
                formatter.write_str("duplicate server runner identity")
            }
            Self::SameScopeManagedAdmissionCollision => {
                formatter.write_str("same-scope managed admission collision")
            }
            Self::NotEnrollmentEligible => {
                formatter.write_str("candidate is not enrollment eligible")
            }
            Self::NotFound => formatter.write_str("runner record not found"),
            Self::Forgotten => formatter.write_str("runner record is forgotten"),
        }
    }
}

impl std::error::Error for RegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

/// Registry persistence boundary.
pub trait RegistryStore: Send + Sync {
    fn load(&self) -> Result<Option<RunnerRegistry>, RegistryError>;
    fn save(&self, registry: &RunnerRegistry) -> Result<(), RegistryError>;
}

/// Source-development / sandbox file store. Caller selects the path.
#[derive(Clone, Debug)]
pub struct FileRegistryStore {
    path: PathBuf,
}

impl FileRegistryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl RegistryStore for FileRegistryStore {
    fn load(&self) -> Result<Option<RunnerRegistry>, RegistryError> {
        let guards = guard_existing_directories(&self.path)
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))?;
        if is_reparse_point(&self.path)
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))?
        {
            return Err(RegistryError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "registry path is a reparse point",
            )));
        }
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(RegistryError::Io(error)),
        };
        guards
            .verify()
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))?;
        let registry: RunnerRegistry =
            serde_json::from_slice(&bytes).map_err(RegistryError::Json)?;
        validate_registry(&registry)?;
        Ok(Some(registry))
    }

    fn save(&self, registry: &RunnerRegistry) -> Result<(), RegistryError> {
        validate_registry(registry)?;
        let guards = guard_existing_directories(&self.path)
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))?;
        if is_reparse_point(&self.path)
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))?
        {
            return Err(RegistryError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "registry path is a reparse point",
            )));
        }
        let bytes = serde_json::to_vec_pretty(registry).map_err(RegistryError::Json)?;
        guards
            .verify()
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))?;
        atomic_write(&self.path, &bytes).map_err(RegistryError::Io)?;
        guards
            .verify()
            .map_err(|error| RegistryError::Io(io::Error::other(error.to_string())))
    }
}

/// In-memory store for deterministic tests and Agent defaults.
#[derive(Default)]
pub struct MemoryRegistryStore {
    registry: std::sync::Mutex<Option<RunnerRegistry>>,
}

impl MemoryRegistryStore {
    pub fn persisted(&self) -> Option<RunnerRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl RegistryStore for MemoryRegistryStore {
    fn load(&self) -> Result<Option<RunnerRegistry>, RegistryError> {
        Ok(self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone())
    }

    fn save(&self, registry: &RunnerRegistry) -> Result<(), RegistryError> {
        validate_registry(registry)?;
        *self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(registry.clone());
        Ok(())
    }
}

/// Pure enrollment/intent operations over an in-memory registry document.
#[derive(Clone, Default)]
pub struct RunnerRegistryEditor {
    registry: RunnerRegistry,
}

impl RunnerRegistryEditor {
    pub fn new(registry: RunnerRegistry) -> Result<Self, RegistryError> {
        validate_registry(&registry)?;
        Ok(Self { registry })
    }

    pub fn empty() -> Self {
        Self {
            registry: RunnerRegistry::default(),
        }
    }

    pub fn registry(&self) -> &RunnerRegistry {
        &self.registry
    }

    pub fn into_registry(self) -> RunnerRegistry {
        self.registry
    }

    /// Migrates a v0.1 single installed binding into one enrolled record.
    /// Idempotent when the legacy key already exists.
    pub fn migrate_v01_binding(
        &mut self,
        binding: &InstalledRuntimeBinding,
        service_name: Option<&str>,
    ) -> Result<&RunnerRecord, RegistryError> {
        if !binding.is_valid() {
            return Err(RegistryError::InvalidRecord("v0.1 binding is invalid"));
        }
        if let Some(existing_key) = self.registry.legacy_v01_binding_key.clone() {
            return self
                .registry
                .get(&existing_key)
                .ok_or(RegistryError::NotFound);
        }

        let backend = match service_name {
            Some(name) => RegistryBackendBinding::WindowsService {
                service_name: name.to_owned(),
            },
            None => RegistryBackendBinding::UserSession,
        };
        let key = legacy_v01_key(&binding.admission, &binding.local.runner_home);
        let record = RunnerRecord {
            local_runner_key: key.clone(),
            registration_scope: binding.admission.scope.clone(),
            runner_id: binding.admission.runner_id,
            expected_runner_name: binding.admission.runner_name.clone(),
            runner_home: binding.local.runner_home.clone(),
            work_root: binding.local.work_root.clone(),
            execution_identity_ref: binding.local.execution_identity_ref.clone(),
            backend,
            desired_enabled: true,
            enrollment_state: EnrollmentState::Enrolled,
            credential_ref: Some(binding.admission.credential_ref.clone()),
            managed_admission: binding.admission.has_valid_ownership(),
        };
        self.insert_record(record)?;
        self.registry.legacy_v01_binding_key = Some(key);
        self.registry
            .get(self.registry.legacy_v01_binding_key.as_deref().unwrap())
            .ok_or(RegistryError::NotFound)
    }

    /// Rolls registry state back to a v0.1-compatible single binding view when
    /// exactly the legacy record remains enrolled.
    pub fn export_v01_binding(&self) -> Result<AdmissionBinding, RegistryError> {
        let key = self
            .registry
            .legacy_v01_binding_key
            .as_deref()
            .ok_or(RegistryError::NotFound)?;
        let record = self.registry.get(key).ok_or(RegistryError::NotFound)?;
        if !record.is_active_enrollment() {
            return Err(RegistryError::Forgotten);
        }
        let credential_ref = record
            .credential_ref
            .clone()
            .ok_or(RegistryError::InvalidRecord("missing credential reference"))?;
        let ownership = if record.managed_admission {
            Some(crate::ReservedLabelOwnership {
                scope: record.registration_scope.clone(),
                runner_id: record.runner_id,
                label: RESERVED_ADMISSION_LABEL.to_owned(),
            })
        } else {
            None
        };
        AdmissionBinding::new(
            record.registration_scope.clone(),
            record.runner_id,
            record.expected_runner_name.clone(),
            credential_ref,
            ownership,
        )
        .map_err(|_| RegistryError::InvalidRecord("cannot rebuild v0.1 admission binding"))
    }

    pub fn enroll_from_discovery(
        &mut self,
        candidate: &DiscoveredRunner,
        credential_ref: Option<CredentialReference>,
        managed_admission: bool,
    ) -> Result<&RunnerRecord, RegistryError> {
        if candidate.disposition != DiscoveryDisposition::ObservedOnly
            || !candidate.capabilities.enrollment_eligible
        {
            return Err(RegistryError::NotEnrollmentEligible);
        }
        let identity = &candidate.identity;
        let scope = identity
            .registration_scope
            .clone()
            .ok_or(RegistryError::NotEnrollmentEligible)?;
        let runner_id = identity
            .runner_id
            .ok_or(RegistryError::NotEnrollmentEligible)?;
        let execution_identity_ref = identity
            .execution_identity_ref
            .clone()
            .ok_or(RegistryError::NotEnrollmentEligible)?;
        let work_root = identity
            .work_root
            .clone()
            .ok_or(RegistryError::NotEnrollmentEligible)?;
        let expected_runner_name = identity
            .expected_runner_name
            .clone()
            .unwrap_or_else(|| format!("runner-{runner_id}"));
        let backend = match candidate.backend {
            RunnerBackendKind::WindowsService => {
                let service_name = candidate
                    .service
                    .as_ref()
                    .map(|service| service.service_name.clone())
                    .filter(|name| !name.trim().is_empty())
                    .ok_or(RegistryError::NotEnrollmentEligible)?;
                RegistryBackendBinding::WindowsService { service_name }
            }
            RunnerBackendKind::UserSession => RegistryBackendBinding::UserSession,
            RunnerBackendKind::ScheduledTask | RunnerBackendKind::Unknown => {
                return Err(RegistryError::NotEnrollmentEligible);
            }
        };

        let record = RunnerRecord {
            local_runner_key: candidate.local_discovery_key.clone(),
            registration_scope: scope,
            runner_id,
            expected_runner_name,
            runner_home: identity.runner_home.clone(),
            work_root,
            execution_identity_ref,
            backend,
            desired_enabled: true,
            enrollment_state: EnrollmentState::Enrolled,
            credential_ref,
            managed_admission,
        };
        self.insert_record(record)?;
        self.registry
            .get(&candidate.local_discovery_key)
            .ok_or(RegistryError::NotFound)
    }

    pub fn set_desired_enabled(
        &mut self,
        local_runner_key: &str,
        desired_enabled: bool,
    ) -> Result<&RunnerRecord, RegistryError> {
        let record = self
            .registry
            .get_mut(local_runner_key)
            .ok_or(RegistryError::NotFound)?;
        if record.enrollment_state != EnrollmentState::Enrolled {
            return Err(RegistryError::Forgotten);
        }
        record.desired_enabled = desired_enabled;
        self.registry
            .get(local_runner_key)
            .ok_or(RegistryError::NotFound)
    }

    pub fn forget(&mut self, local_runner_key: &str) -> Result<&RunnerRecord, RegistryError> {
        let record = self
            .registry
            .get_mut(local_runner_key)
            .ok_or(RegistryError::NotFound)?;
        record.enrollment_state = EnrollmentState::Forgotten;
        record.desired_enabled = false;
        record.managed_admission = false;
        self.registry
            .get(local_runner_key)
            .ok_or(RegistryError::NotFound)
    }

    fn insert_record(&mut self, record: RunnerRecord) -> Result<(), RegistryError> {
        if !record.is_valid() {
            return Err(RegistryError::InvalidRecord("record failed validation"));
        }
        if self.registry.get(&record.local_runner_key).is_some() {
            return Err(RegistryError::DuplicateKey);
        }
        let home_key = normalize_path_key(&record.runner_home);
        if self
            .registry
            .active_records()
            .any(|existing| normalize_path_key(&existing.runner_home) == home_key)
        {
            return Err(RegistryError::DuplicateHome);
        }
        let work_root_key = normalize_path_key(&record.work_root);
        if self
            .registry
            .active_records()
            .any(|existing| normalize_path_key(&existing.work_root) == work_root_key)
        {
            return Err(RegistryError::DuplicateWorkRoot);
        }
        if self.registry.active_records().any(|existing| {
            existing.registration_scope == record.registration_scope
                && existing.runner_id == record.runner_id
        }) {
            return Err(RegistryError::DuplicateServerIdentity);
        }
        if record.managed_admission
            && self.registry.active_records().any(|existing| {
                existing.managed_admission
                    && existing.registration_scope == record.registration_scope
            })
        {
            return Err(RegistryError::SameScopeManagedAdmissionCollision);
        }
        self.registry.records.push(record);
        validate_registry(&self.registry)
    }
}

fn validate_registry(registry: &RunnerRegistry) -> Result<(), RegistryError> {
    if registry.schema_version != RUNNER_REGISTRY_SCHEMA_VERSION {
        return Err(RegistryError::IncompatibleSchema {
            found: registry.schema_version,
            supported: RUNNER_REGISTRY_SCHEMA_VERSION,
        });
    }
    if !registry.is_valid() {
        return Err(RegistryError::InvalidRecord("registry failed validation"));
    }
    Ok(())
}

fn unique_keys(records: &[RunnerRecord]) -> bool {
    for (index, record) in records.iter().enumerate() {
        if records[..index]
            .iter()
            .any(|existing| existing.local_runner_key == record.local_runner_key)
        {
            return false;
        }
    }
    true
}

fn legacy_v01_key(admission: &AdmissionBinding, runner_home: &Path) -> String {
    format!(
        "v01-{}-{}",
        admission.runner_id,
        short_digest(&normalize_path_key(runner_home))
    )
}

fn short_digest(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn absolute_path_like(path: &Path) -> bool {
    let raw = path.to_string_lossy();
    !raw.is_empty()
        && !raw.contains('\0')
        && (path.is_absolute() || raw.chars().nth(1) == Some(':') || raw.starts_with('/'))
}

fn valid_scope_component(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.contains(['\0', '\r', '\n', '/', '\\', ' '])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExactLocalRunnerBinding, ReservedLabelOwnership, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
    };
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn identity() -> OpaqueIdentityReference {
        OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap()
    }

    fn scope_a() -> RegistrationScope {
        RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: "repo-a".to_owned(),
        }
    }

    fn scope_b() -> RegistrationScope {
        RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: "repo-b".to_owned(),
        }
    }

    fn v01_binding() -> InstalledRuntimeBinding {
        let scope = scope_a();
        InstalledRuntimeBinding::new(
            AdmissionBinding::new(
                scope.clone(),
                42,
                "fixture-runner",
                CredentialReference::new("windows-credential-manager", "fixture-reference")
                    .unwrap(),
                Some(ReservedLabelOwnership::for_runner(scope, 42)),
            )
            .unwrap(),
            ExactLocalRunnerBinding::new(
                PathBuf::from(r"C:\fixture\runner"),
                PathBuf::from(r"C:\fixture\work"),
                identity(),
            )
            .unwrap(),
            vec!["fixture.exe".to_owned()],
        )
        .unwrap()
    }

    fn discovered(
        key: &str,
        scope: RegistrationScope,
        runner_id: u64,
        home: &str,
        backend: RunnerBackendKind,
        service_name: Option<&str>,
    ) -> DiscoveredRunner {
        DiscoveredRunner {
            local_discovery_key: key.to_owned(),
            backend,
            disposition: DiscoveryDisposition::ObservedOnly,
            refusal: None,
            identity: crate::DiscoveredRunnerIdentity {
                registration_scope: Some(scope),
                runner_id: Some(runner_id),
                expected_runner_name: Some(format!("fixture-{runner_id}")),
                runner_home: PathBuf::from(home),
                work_root: Some(PathBuf::from(home).join("_work")),
                execution_identity_ref: Some(identity()),
            },
            service: service_name.map(|name| crate::DiscoveredServiceBinding {
                service_name: name.to_owned(),
                display_name: None,
                image_path: None,
                state: crate::DiscoveredLifecycle::Stopped,
            }),
            session: None,
            observed_lifecycle: crate::DiscoveredLifecycle::Stopped,
            same_scope_collision: false,
            capabilities: crate::DiscoveryCapabilities {
                service_control_contract: matches!(backend, RunnerBackendKind::WindowsService),
                user_session_control_contract: matches!(backend, RunnerBackendKind::UserSession),
                enrollment_eligible: true,
            },
        }
    }

    #[test]
    fn migrate_v01_binding_round_trips_without_credential_material() {
        let mut editor = RunnerRegistryEditor::empty();
        let record = editor
            .migrate_v01_binding(&v01_binding(), Some("fixture.scm.generic"))
            .unwrap()
            .clone();
        assert_eq!(record.enrollment_state, EnrollmentState::Enrolled);
        assert!(record.desired_enabled);
        assert!(record.managed_admission);
        assert_eq!(
            record.credential_ref.as_ref().unwrap().key,
            "fixture-reference"
        );

        let encoded = serde_json::to_string(editor.registry()).unwrap();
        assert!(encoded.contains("fixture-reference"));
        assert!(!encoded.to_ascii_lowercase().contains("bearer"));
        assert!(!encoded.contains("ghp_"));

        let rebuilt = editor.export_v01_binding().unwrap();
        assert_eq!(rebuilt.runner_id, 42);
        assert_eq!(rebuilt.runner_name, "fixture-runner");
        assert!(rebuilt.has_valid_ownership());
    }

    #[test]
    fn enroll_two_distinct_scopes_and_toggle_intent_independently() {
        let mut editor = RunnerRegistryEditor::empty();
        editor
            .enroll_from_discovery(
                &discovered(
                    "key-a",
                    scope_a(),
                    1,
                    r"C:\fixture\runners\a",
                    RunnerBackendKind::WindowsService,
                    Some("actions.runner.a"),
                ),
                None,
                true,
            )
            .unwrap();
        editor
            .enroll_from_discovery(
                &discovered(
                    "key-b",
                    scope_b(),
                    2,
                    r"C:\fixture\runners\b",
                    RunnerBackendKind::UserSession,
                    None,
                ),
                None,
                true,
            )
            .unwrap();

        editor.set_desired_enabled("key-a", false).unwrap();
        assert!(!editor.registry().get("key-a").unwrap().desired_enabled);
        assert!(editor.registry().get("key-b").unwrap().desired_enabled);
    }

    #[test]
    fn same_scope_managed_admission_collision_is_refused() {
        let mut editor = RunnerRegistryEditor::empty();
        editor
            .enroll_from_discovery(
                &discovered(
                    "key-a",
                    scope_a(),
                    1,
                    r"C:\fixture\runners\a",
                    RunnerBackendKind::WindowsService,
                    Some("actions.runner.a"),
                ),
                None,
                true,
            )
            .unwrap();
        let collided = editor.enroll_from_discovery(
            &discovered(
                "key-c",
                scope_a(),
                3,
                r"C:\fixture\runners\c",
                RunnerBackendKind::WindowsService,
                Some("actions.runner.c"),
            ),
            None,
            true,
        );
        assert!(matches!(
            collided,
            Err(RegistryError::SameScopeManagedAdmissionCollision)
        ));
    }

    #[test]
    fn duplicate_work_root_across_distinct_homes_is_refused() {
        let mut editor = RunnerRegistryEditor::empty();
        let mut cand_a = discovered(
            "key-a",
            scope_a(),
            1,
            r"C:\fixture\runners\a",
            RunnerBackendKind::UserSession,
            None,
        );
        cand_a.identity.work_root = Some(PathBuf::from(r"C:\fixture\shared_work"));
        editor.enroll_from_discovery(&cand_a, None, false).unwrap();

        let mut cand_b = discovered(
            "key-b",
            scope_b(),
            2,
            r"C:\fixture\runners\b",
            RunnerBackendKind::UserSession,
            None,
        );
        cand_b.identity.work_root = Some(PathBuf::from(r"C:\fixture\shared_work"));
        let err = editor.enroll_from_discovery(&cand_b, None, false);
        assert!(matches!(err, Err(RegistryError::DuplicateWorkRoot)));
    }

    #[test]
    fn forget_does_not_remove_record_or_claim_unregister() {
        let mut editor = RunnerRegistryEditor::empty();
        editor
            .enroll_from_discovery(
                &discovered(
                    "key-a",
                    scope_a(),
                    1,
                    r"C:\fixture\runners\a",
                    RunnerBackendKind::UserSession,
                    None,
                ),
                None,
                false,
            )
            .unwrap();
        let forgotten = editor.forget("key-a").unwrap();
        assert_eq!(forgotten.enrollment_state, EnrollmentState::Forgotten);
        assert!(!forgotten.desired_enabled);
        assert!(!forgotten.managed_admission);
        assert_eq!(editor.registry().records.len(), 1);
    }

    #[test]
    fn file_registry_store_round_trips_atomically_in_sandbox() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("runnermesh-registry-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("runner-registry.json");
        let store = FileRegistryStore::new(&path);

        let mut editor = RunnerRegistryEditor::empty();
        editor.migrate_v01_binding(&v01_binding(), None).unwrap();
        store.save(editor.registry()).unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.schema_version, RUNNER_REGISTRY_SCHEMA_VERSION);

        let mut broken = loaded.clone();
        broken.schema_version = RUNNER_REGISTRY_SCHEMA_VERSION + 1;
        fs::write(&path, serde_json::to_vec_pretty(&broken).unwrap()).unwrap();
        assert!(matches!(
            store.load(),
            Err(RegistryError::IncompatibleSchema { .. })
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_discovery_candidate_cannot_enroll() {
        let mut candidate = discovered(
            "key-bad",
            scope_a(),
            9,
            r"C:\fixture\runners\bad",
            RunnerBackendKind::UserSession,
            None,
        );
        candidate.capabilities.enrollment_eligible = false;
        let mut editor = RunnerRegistryEditor::empty();
        assert!(matches!(
            editor.enroll_from_discovery(&candidate, None, false),
            Err(RegistryError::NotEnrollmentEligible)
        ));
    }

    #[test]
    fn corrupt_json_and_temp_residue_do_not_silently_diverge_intent() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("runnermesh-registry-fail-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("runner-registry.json");
        let store = FileRegistryStore::new(&path);

        let mut editor = RunnerRegistryEditor::empty();
        editor
            .enroll_from_discovery(
                &discovered(
                    "key-a",
                    scope_a(),
                    1,
                    r"C:\fixture\runners\a",
                    RunnerBackendKind::UserSession,
                    None,
                ),
                None,
                false,
            )
            .unwrap();
        editor.set_desired_enabled("key-a", false).unwrap();
        store.save(editor.registry()).unwrap();

        // Simulate crash residue beside the durable file; load must still see
        // the last committed intent and must not invent enabled=true.
        fs::write(root.join("runner-registry.json.tmp"), b"{broken").unwrap();
        let reloaded = store.load().unwrap().unwrap();
        assert!(!reloaded.get("key-a").unwrap().desired_enabled);

        // Corrupt durable bytes must fail closed rather than returning empty.
        fs::write(&path, b"{not-json").unwrap();
        assert!(matches!(store.load(), Err(RegistryError::Json(_))));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v01_migrate_and_export_preserves_enabled_intent_round_trip() {
        let mut editor = RunnerRegistryEditor::empty();
        editor.migrate_v01_binding(&v01_binding(), None).unwrap();
        let key = editor
            .registry()
            .legacy_v01_binding_key
            .clone()
            .expect("legacy key");
        editor.set_desired_enabled(&key, false).unwrap();
        let exported = editor.export_v01_binding().unwrap();
        assert_eq!(exported.runner_id, v01_binding().admission.runner_id);

        // Re-import into a fresh editor must not resurrect desired_enabled=true
        // for a forgotten legacy path; migration is idempotent on key presence.
        let mut again = RunnerRegistryEditor::empty();
        again.migrate_v01_binding(&v01_binding(), None).unwrap();
        // Fresh migration creates a new enabled record because the previous
        // disable lived only in the first editor document — prove explicit save
        // is required to retain intent across process boundaries.
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("runnermesh-registry-mig-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("runner-registry.json");
        let store = FileRegistryStore::new(&path);
        store.save(editor.registry()).unwrap();
        let persisted = store.load().unwrap().unwrap();
        assert!(!persisted.get(&key).unwrap().desired_enabled);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_and_file_stores_report_write_failures_without_intent_drift() {
        let mut editor = RunnerRegistryEditor::empty();
        editor
            .enroll_from_discovery(
                &discovered(
                    "key-a",
                    scope_a(),
                    1,
                    r"C:\fixture\runners\a",
                    RunnerBackendKind::UserSession,
                    None,
                ),
                None,
                false,
            )
            .unwrap();
        let memory = MemoryRegistryStore::default();
        memory.save(editor.registry()).unwrap();
        editor.set_desired_enabled("key-a", false).unwrap();
        // Memory store not yet updated — intentional divergence must be visible
        // by comparing editor vs store rather than silently coalescing.
        let stale = memory.persisted().unwrap();
        assert!(stale.get("key-a").unwrap().desired_enabled);
        assert!(!editor.registry().get("key-a").unwrap().desired_enabled);
        memory.save(editor.registry()).unwrap();
        assert!(
            !memory
                .persisted()
                .unwrap()
                .get("key-a")
                .unwrap()
                .desired_enabled
        );

        // Write beneath a file-as-parent fails closed (no silent success).
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("runnermesh-registry-io-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let blocker = root.join("not-a-dir");
        fs::write(&blocker, b"file").unwrap();
        let missing = FileRegistryStore::new(blocker.join("registry.json"));
        let failed = missing.save(editor.registry());
        assert!(matches!(failed, Err(RegistryError::Io(_))));
        fs::remove_dir_all(root).unwrap();
    }
}
