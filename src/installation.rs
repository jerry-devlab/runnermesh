//! Explicit-root, sandboxable versioned installation and user-session autostart.
//!
//! This module never discovers a production location.  A caller must supply a
//! root and tests use a caller-owned temporary directory plus a file-backed
//! autostart backend.  The installed runtime is copied into immutable slots;
//! no activation path can point back to a source checkout or build output.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INSTALLATION_SCHEMA_VERSION: u32 = 1;
const LEDGER_FILE: &str = ".runnermesh-installation.json";
const CURRENT_FILE: &str = "current.json";
const VERSIONS_DIR: &str = "versions";
const BIN_DIR: &str = "bin";
const CONFIG_DIR: &str = "config";
const STATE_DIR: &str = "state";
const LOGS_DIR: &str = "logs";
const STABLE_AGENT_ENTRY: &str = "runnermesh-agent.exe";
const RUNTIME_BINDING_FILE: &str = "runtime-binding.json";
const AGENT_CONFIG_FILE: &str = "agent.json";
const INSTALL_TRANSACTION_FILE: &str = "installation-transaction.json";
const INSTALL_TRANSACTION_SCHEMA_VERSION: u32 = 1;
const UNINSTALL_TRANSACTION_FILE: &str = "uninstall-transaction.json";
const UNINSTALL_RECEIPT_FILE: &str = "uninstall-receipt.json";
const UNINSTALL_TRANSACTION_SCHEMA_VERSION: u32 = 1;
const AUTOSTART_OWNER: &str = "runnermesh-v01";
const AUTOSTART_VALUE: &str = "RunnerMesh";
static SANDBOX_AUTOSTART_LOCK: Mutex<()> = Mutex::new(());

/// A caller-provided install root. Constructing this value has no side effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Installation {
    root: PathBuf,
}

impl Installation {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.root.join(VERSIONS_DIR)
    }

    pub fn stable_agent_entry(&self) -> PathBuf {
        self.root.join(BIN_DIR).join(STABLE_AGENT_ENTRY)
    }

    pub(crate) fn state_dir(&self) -> PathBuf {
        self.root.join(STATE_DIR)
    }

    pub(crate) fn agent_state_dir(&self) -> PathBuf {
        self.state_dir().join("agent")
    }

    pub(crate) fn agent_config_path(&self) -> PathBuf {
        self.root.join(CONFIG_DIR).join(AGENT_CONFIG_FILE)
    }

    pub fn runtime_binding_path(&self) -> PathBuf {
        self.root.join(CONFIG_DIR).join(RUNTIME_BINDING_FILE)
    }

    /// Copy a verified payload into an immutable slot. Repeating identical
    /// bytes is idempotent; a version collision with different bytes refuses.
    pub fn install(&self, version: &str, payload: &Path) -> Result<InstallReceipt, InstallError> {
        self.install_expected(version, payload, None)
    }

    pub(crate) fn install_with_payload_sha256(
        &self,
        version: &str,
        payload: &Path,
        expected_payload_sha256: &str,
    ) -> Result<InstallReceipt, InstallError> {
        self.install_expected(version, payload, Some(expected_payload_sha256))
    }

    fn install_expected(
        &self,
        version: &str,
        payload: &Path,
        expected_payload_sha256: Option<&str>,
    ) -> Result<InstallReceipt, InstallError> {
        let ancestor_guards = guard_existing_directories(&self.root)?;
        ancestor_guards.verify()?;
        validate_version(version)?;
        self.assert_runtime_root_safe()?;
        self.assert_payload_safe(payload)?;
        let mut state = self.open_or_initialize()?;
        let owned_guards = self.guard_owned_directories()?;
        owned_guards.verify()?;
        self.verify_state(&state)?;
        let manifest = manifest_for_payload(payload)?;
        let payload_sha256 = manifest_payload_sha256(&manifest);
        if let Some(expected) = expected_payload_sha256 {
            if payload_sha256 != expected {
                return Err(InstallError::PayloadChecksumMismatch {
                    expected: expected.to_owned(),
                    actual: payload_sha256,
                });
            }
        }
        if !manifest.files.contains_key("runnermesh-agent.exe") {
            return Err(InstallError::InvalidPayload(
                "payload must contain runnermesh-agent.exe".to_owned(),
            ));
        }

        if let Some(existing) = state.versions.get(version) {
            if existing == &manifest {
                return Ok(InstallReceipt {
                    version: version.to_owned(),
                    idempotent: true,
                    activated: state.active_version.as_deref() == Some(version),
                });
            }
            return Err(InstallError::VersionConflict(version.to_owned()));
        }

        let destination = self.versions_dir().join(version);
        if destination.exists() {
            return Err(InstallError::ForeignContent(destination));
        }
        let staging_name = format!(".install-{version}-{}", unique_suffix());
        let staging = self.state_dir().join(&staging_name);
        let previous = state.clone();
        state.versions.insert(version.to_owned(), manifest.clone());
        let activated = state.active_version.is_none();
        if activated {
            state.active_version = Some(version.to_owned());
        }
        let mut transaction = InstallationTransaction {
            schema_version: INSTALL_TRANSACTION_SCHEMA_VERSION,
            previous,
            desired: state,
            staging_name: Some(staging_name),
            phase: InstallationTransactionPhase::Intent,
        };
        self.begin_transaction(&transaction)?;
        let result = (|| {
            fs::create_dir(&staging).map_err(InstallError::io)?;
            copy_payload(payload, &staging, &manifest)?;
            verify_manifest(&staging, &manifest)?;
            transaction.phase = InstallationTransactionPhase::SlotReady;
            self.write_transaction(&transaction)?;
            owned_guards.verify()?;
            fs::rename(&staging, &destination).map_err(InstallError::io)?;
            transaction.phase = InstallationTransactionPhase::SlotCommitted;
            self.write_transaction(&transaction)?;
            self.finish_transaction(&mut transaction)
        })();
        if let Err(error) = result {
            if let Err(recovery) = self.recover_pending_transaction() {
                return Err(InstallError::RecoveryFailed(format!(
                    "operation={error}; recovery={recovery}"
                )));
            }
            return Err(error);
        }
        Ok(InstallReceipt {
            version: version.to_owned(),
            idempotent: false,
            activated,
        })
    }

    /// Selects a pre-existing immutable slot. It never modifies a slot.
    pub fn select_active(&self, version: &str) -> Result<(), InstallError> {
        let owned_guards = self.guard_owned_directories()?;
        owned_guards.verify()?;
        let mut state = self.open_existing()?;
        self.verify_state(&state)?;
        if !state.versions.contains_key(version) {
            return Err(InstallError::UnknownVersion(version.to_owned()));
        }
        if state.active_version.as_deref() == Some(version) {
            return Ok(());
        }
        let previous = state.clone();
        state.active_version = Some(version.to_owned());
        let mut transaction = InstallationTransaction {
            schema_version: INSTALL_TRANSACTION_SCHEMA_VERSION,
            previous,
            desired: state,
            staging_name: None,
            phase: InstallationTransactionPhase::Intent,
        };
        self.begin_transaction(&transaction)?;
        if let Err(error) = self.finish_transaction(&mut transaction) {
            if let Err(recovery) = self.recover_pending_transaction() {
                return Err(InstallError::RecoveryFailed(format!(
                    "operation={error}; recovery={recovery}"
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn state(&self) -> Result<InstallationState, InstallError> {
        let owned_guards = self.guard_owned_directories()?;
        owned_guards.verify()?;
        let state = self.open_existing()?;
        self.verify_state(&state)?;
        Ok(state)
    }

    pub fn active_agent_path(&self) -> Result<PathBuf, InstallError> {
        let state = self.state()?;
        let version = state.active_version.ok_or(InstallError::NoActiveVersion)?;
        let path = self
            .versions_dir()
            .join(version)
            .join("runnermesh-agent.exe");
        if !path.is_file() || is_reparse_point(&path)? {
            return Err(InstallError::OwnershipDrift(path));
        }
        Ok(path)
    }

    /// Reads the optional non-secret runtime binding from persistent config.
    /// Absence is a valid unconfigured state; malformed or unsafe content
    /// fails closed.
    pub fn runtime_binding(&self) -> Result<Option<crate::InstalledRuntimeBinding>, InstallError> {
        let _ = self.state()?;
        let path = self.runtime_binding_path();
        let Some(bytes) = read_optional_owned_file(&path)? else {
            return Ok(None);
        };
        if bytes.len() > 128 * 1024 {
            return Err(InstallError::DamagedRuntimeBinding);
        }
        let binding = serde_json::from_slice::<crate::InstalledRuntimeBinding>(&bytes)
            .map_err(|_| InstallError::DamagedRuntimeBinding)?;
        if !binding.is_valid() {
            return Err(InstallError::DamagedRuntimeBinding);
        }
        Ok(Some(binding))
    }

    /// Establishes the exact installed-runtime binding once. Repeating the
    /// same bytes is idempotent; a different existing binding is an explicit
    /// trust drift and is never overwritten implicitly.
    pub fn configure_runtime_binding(
        &self,
        binding: &crate::InstalledRuntimeBinding,
    ) -> Result<bool, InstallError> {
        if !binding.is_valid() {
            return Err(InstallError::DamagedRuntimeBinding);
        }
        let _ = self.state()?;
        let guards = self.guard_owned_directories()?;
        let path = self.runtime_binding_path();
        let bytes =
            serde_json::to_vec_pretty(binding).map_err(|_| InstallError::DamagedRuntimeBinding)?;
        match read_optional_owned_file(&path)? {
            Some(existing) if existing == bytes => return Ok(false),
            Some(_) => return Err(InstallError::RuntimeBindingDrift),
            None => {}
        }
        guards.verify()?;
        atomic_write(&path, &bytes).map_err(InstallError::io)?;
        guards.verify()?;
        if read_optional_owned_file(&path)?.as_deref() != Some(bytes.as_slice()) {
            return Err(InstallError::RuntimeBindingDrift);
        }
        Ok(true)
    }

    pub(crate) fn installed_payload_sha256(
        &self,
        version: &str,
    ) -> Result<Option<String>, InstallError> {
        let state = self.state()?;
        Ok(state.versions.get(version).map(manifest_payload_sha256))
    }

    /// Enables exactly the RunnerMesh named autostart value, and only when it
    /// references the stable installed activation entry.
    pub fn enable_autostart(&self, backend: &impl AutostartBackend) -> Result<bool, InstallError> {
        let _ = self.active_agent_path()?;
        let expected = self.expected_autostart_entry();
        match backend.install_exact(&expected).map_err(InstallError::io)? {
            AutostartChange::Changed => Ok(true),
            AutostartChange::Unchanged => Ok(false),
            AutostartChange::Drift => Err(InstallError::AutostartDrift),
        }
    }

    /// Disables only the exact owned RunnerMesh value. A foreign value refuses.
    pub fn disable_autostart(&self, backend: &impl AutostartBackend) -> Result<bool, InstallError> {
        let expected = self.expected_autostart_entry();
        match backend.remove_exact(&expected).map_err(InstallError::io)? {
            AutostartChange::Changed => Ok(true),
            AutostartChange::Unchanged => Ok(false),
            AutostartChange::Drift => Err(InstallError::AutostartDrift),
        }
    }

    /// Removes only ledger-verified runtime content. Config, state, logs and
    /// all unrelated root content remain untouched.
    pub fn uninstall(
        &self,
        backend: &impl AutostartBackend,
    ) -> Result<UninstallReceipt, InstallError> {
        if !self.root.exists() {
            return Ok(UninstallReceipt {
                removed_versions: 0,
                root_removed: true,
                foreign_content_preserved: false,
            });
        }
        if is_empty_dir(&self.root)? {
            fs::remove_dir(&self.root).map_err(InstallError::io)?;
            return Ok(UninstallReceipt {
                removed_versions: 0,
                root_removed: true,
                foreign_content_preserved: false,
            });
        }
        let existing_transaction = self.read_uninstall_transaction()?;
        if existing_transaction.is_none() {
            if let Some(receipt) = self.read_uninstall_receipt()? {
                return Ok(UninstallReceipt {
                    removed_versions: receipt.removed_versions,
                    root_removed: false,
                    foreign_content_preserved: true,
                });
            }
        }
        let mut transaction = {
            let _root_guards = guard_existing_directories(&self.state_dir())?;
            match existing_transaction {
                Some(transaction) => transaction,
                None => {
                    let state = self.open_existing()?;
                    self.verify_state(&state)?;
                    let transaction = UninstallTransaction {
                        schema_version: UNINSTALL_TRANSACTION_SCHEMA_VERSION,
                        transaction_id: format!("uninstall-{:032x}", unique_suffix()),
                        state,
                        phase: UninstallPhase::Intent,
                    };
                    durable_create_new(
                        &self.uninstall_transaction_path(),
                        &serde_json::to_vec_pretty(&transaction)
                            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
                    )
                    .map_err(InstallError::io)?;
                    transaction
                }
            }
        };
        self.resume_uninstall(backend, &mut transaction)
    }

    pub(crate) fn expected_autostart_entry(&self) -> AutostartEntry {
        AutostartEntry {
            owner: AUTOSTART_OWNER.to_owned(),
            command: format!("\"{}\"", self.stable_agent_entry().display()),
        }
    }

    fn assert_runtime_root_safe(&self) -> Result<(), InstallError> {
        let root = validate_explicit_path(&self.root)?;
        let source = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).map_err(InstallError::io)?;
        let build = source.join("target");
        if root.starts_with(&source) || root.starts_with(&build) {
            return Err(InstallError::SourceRuntimeOverlap);
        }
        if self.root.exists() && (!self.root.is_dir() || is_reparse_point(&self.root)?) {
            return Err(InstallError::ForeignContent(self.root.clone()));
        }
        Ok(())
    }

    fn assert_payload_safe(&self, payload: &Path) -> Result<(), InstallError> {
        if !payload.is_dir() || is_reparse_point(payload)? {
            return Err(InstallError::InvalidPayload(format!(
                "payload is not an ordinary directory: {}",
                payload.display()
            )));
        }
        let root = validate_explicit_path(&self.root)?;
        let payload = validate_explicit_path(payload)?;
        if payload.starts_with(&root) || root.starts_with(&payload) {
            return Err(InstallError::SourceRuntimeOverlap);
        }
        Ok(())
    }

    fn open_or_initialize(&self) -> Result<InstallationState, InstallError> {
        self.assert_runtime_root_safe()?;
        if !self.root.exists() {
            let parent = self
                .root
                .parent()
                .ok_or_else(|| InstallError::OwnershipDrift(self.root.clone()))?;
            let parent_guards = guard_existing_directories(parent)?;
            parent_guards.verify()?;
            if !parent.is_dir() || is_reparse_point(parent)? {
                return Err(InstallError::OwnershipDrift(parent.to_path_buf()));
            }
            let leaf = self
                .root
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| InstallError::OwnershipDrift(self.root.clone()))?;
            let staging = parent.join(format!(".{leaf}.runnermesh-init-{:032x}", unique_suffix()));
            fs::create_dir(&staging)
                .map_err(|error| InstallError::Io(format!("create init staging: {error}")))?;
            let result = (|| {
                for directory in [VERSIONS_DIR, BIN_DIR, CONFIG_DIR, STATE_DIR, LOGS_DIR] {
                    fs::create_dir(staging.join(directory)).map_err(|error| {
                        InstallError::Io(format!("create init directory {directory}: {error}"))
                    })?;
                }
                let state = initial_installation_state();
                atomic_write(
                    &staging.join(LEDGER_FILE),
                    &serde_json::to_vec_pretty(&state)
                        .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
                )
                .map_err(|error| InstallError::Io(format!("write init ledger: {error}")))?;
                parent_guards.verify()?;
                fs::rename(&staging, &self.root)
                    .map_err(|error| InstallError::Io(format!("commit init root: {error}")))?;
                Ok::<_, InstallError>(state)
            })();
            if result.is_err() && staging.exists() && !is_reparse_point(&staging)? {
                let _ = fs::remove_dir_all(&staging);
            }
            return result;
        }
        let ledger = self.root.join(LEDGER_FILE);
        if ledger.exists() {
            return self.open_existing();
        }
        if fs::read_dir(&self.root)
            .map_err(InstallError::io)?
            .next()
            .is_some()
        {
            return Err(InstallError::ForeignContent(self.root.clone()));
        }
        Err(InstallError::ForeignContent(self.root.clone()))
    }

    fn open_existing(&self) -> Result<InstallationState, InstallError> {
        self.assert_runtime_root_safe()?;
        if self.read_uninstall_transaction()?.is_some() {
            return Err(InstallError::UninstallInProgress);
        }
        self.recover_pending_transaction()?;
        self.read_state_file()
    }

    fn guard_owned_directories(&self) -> Result<ExistingDirectoryGuards, InstallError> {
        let mut guards = guard_existing_directories(&self.root)?;
        for directory in [VERSIONS_DIR, BIN_DIR, CONFIG_DIR, STATE_DIR, LOGS_DIR] {
            guards.extend(guard_existing_directories(&self.root.join(directory))?);
        }
        Ok(guards)
    }

    fn uninstall_transaction_path(&self) -> PathBuf {
        self.state_dir().join(UNINSTALL_TRANSACTION_FILE)
    }

    fn uninstall_receipt_path(&self) -> PathBuf {
        self.state_dir().join(UNINSTALL_RECEIPT_FILE)
    }

    fn read_uninstall_transaction(&self) -> Result<Option<UninstallTransaction>, InstallError> {
        let path = self.uninstall_transaction_path();
        let Some(bytes) = read_optional_owned_file(&path)? else {
            return Ok(None);
        };
        let transaction = serde_json::from_slice::<UninstallTransaction>(&bytes)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        validate_uninstall_transaction(&transaction)?;
        Ok(Some(transaction))
    }

    fn read_uninstall_receipt(&self) -> Result<Option<DurableUninstallReceipt>, InstallError> {
        let path = self.uninstall_receipt_path();
        let Some(bytes) = read_optional_owned_file(&path)? else {
            return Ok(None);
        };
        let receipt = serde_json::from_slice::<DurableUninstallReceipt>(&bytes)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        if receipt.schema_version != UNINSTALL_TRANSACTION_SCHEMA_VERSION
            || self.root.join(LEDGER_FILE).exists()
            || self.root.join(CURRENT_FILE).exists()
            || self.versions_dir().exists()
            || self.root.join(BIN_DIR).exists()
        {
            return Err(InstallError::DamagedMetadata(
                "invalid uninstall completion receipt".to_owned(),
            ));
        }
        Ok(Some(receipt))
    }

    fn write_uninstall_transaction(
        &self,
        transaction: &UninstallTransaction,
    ) -> Result<(), InstallError> {
        validate_uninstall_transaction(transaction)?;
        atomic_write(
            &self.uninstall_transaction_path(),
            &serde_json::to_vec_pretty(transaction)
                .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
        )
        .map_err(InstallError::io)
    }

    fn resume_uninstall(
        &self,
        backend: &impl AutostartBackend,
        transaction: &mut UninstallTransaction,
    ) -> Result<UninstallReceipt, InstallError> {
        self.resume_uninstall_with_hook(backend, transaction, || Ok(()))
    }

    fn resume_uninstall_with_hook(
        &self,
        backend: &impl AutostartBackend,
        transaction: &mut UninstallTransaction,
        after_root_detached: impl FnOnce() -> Result<(), InstallError>,
    ) -> Result<UninstallReceipt, InstallError> {
        let root_guards = guard_existing_directories(&self.state_dir())?;
        root_guards.verify()?;
        validate_uninstall_transaction(transaction)?;
        if transaction.phase == UninstallPhase::Intent {
            let _ = self.disable_autostart(backend)?;
            transaction.phase = UninstallPhase::AutostartRemoved;
            self.write_uninstall_transaction(transaction)?;
        }
        if transaction.phase == UninstallPhase::AutostartRemoved {
            root_guards.verify()?;
            self.quarantine_uninstall_content(transaction)?;
            transaction.phase = UninstallPhase::Quarantined;
            self.write_uninstall_transaction(transaction)?;
        }
        if transaction.phase == UninstallPhase::Quarantined {
            root_guards.verify()?;
            self.cleanup_uninstall_quarantine(transaction)?;
            transaction.phase = UninstallPhase::Cleaned;
            self.write_uninstall_transaction(transaction)?;
        }

        let removed_versions = transaction.state.versions.len();
        atomic_write(
            &self.uninstall_receipt_path(),
            &serde_json::to_vec_pretty(&DurableUninstallReceipt {
                schema_version: UNINSTALL_TRANSACTION_SCHEMA_VERSION,
                removed_versions,
            })
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
        )
        .map_err(InstallError::io)?;
        remove_owned_file(&self.uninstall_transaction_path())?;
        #[cfg(windows)]
        drop(root_guards);
        for directory in [CONFIG_DIR, LOGS_DIR] {
            let directory = self.root.join(directory);
            if directory.exists() && is_empty_dir(&directory)? {
                fs::remove_dir(&directory).map_err(InstallError::io)?;
            }
        }

        let state_entries = fs::read_dir(self.state_dir())
            .map_err(InstallError::io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(InstallError::io)?;
        let only_receipt = state_entries.len() == 1
            && state_entries[0].file_name().to_string_lossy() == UNINSTALL_RECEIPT_FILE;
        let root_entries = fs::read_dir(&self.root)
            .map_err(InstallError::io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(InstallError::io)?;
        let only_state =
            root_entries.len() == 1 && root_entries[0].file_name().to_string_lossy() == STATE_DIR;
        let root_removed = only_receipt && only_state;
        if root_removed {
            let parent = self
                .root
                .parent()
                .ok_or_else(|| InstallError::OwnershipDrift(self.root.clone()))?;
            let leaf = self
                .root
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| InstallError::OwnershipDrift(self.root.clone()))?;
            let detached = parent.join(format!(
                ".{leaf}.runnermesh-uninstall-{}",
                transaction.transaction_id
            ));
            if detached.exists() {
                return Err(InstallError::ForeignContent(detached));
            }
            fs::rename(&self.root, &detached).map_err(InstallError::io)?;
            after_root_detached()?;
            if !detached.is_dir() || is_reparse_point(&detached)? {
                return Err(InstallError::OwnershipDrift(detached));
            }
            fs::remove_dir_all(&detached).map_err(InstallError::io)?;
        }
        Ok(UninstallReceipt {
            removed_versions,
            root_removed,
            foreign_content_preserved: !root_removed,
        })
    }

    fn quarantine_uninstall_content(
        &self,
        transaction: &UninstallTransaction,
    ) -> Result<(), InstallError> {
        let quarantine = uninstall_quarantine_paths(self, transaction);
        quarantine_versions(
            &self.versions_dir(),
            &quarantine.versions,
            &transaction.state,
        )?;
        quarantine_bin(
            &self.root.join(BIN_DIR),
            &quarantine.bin,
            &transaction.state,
        )?;
        if let Some((current, _)) = activation_artifacts(&transaction.state)? {
            quarantine_file(&self.root.join(CURRENT_FILE), &quarantine.current, &current)?;
        } else if self.root.join(CURRENT_FILE).exists() || quarantine.current.exists() {
            return Err(InstallError::OwnershipDrift(self.root.join(CURRENT_FILE)));
        }
        quarantine_file(
            &self.root.join(LEDGER_FILE),
            &quarantine.ledger,
            &serde_json::to_vec_pretty(&transaction.state)
                .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
        )
    }

    fn cleanup_uninstall_quarantine(
        &self,
        transaction: &UninstallTransaction,
    ) -> Result<(), InstallError> {
        let quarantine = uninstall_quarantine_paths(self, transaction);
        for directory in [quarantine.versions, quarantine.bin] {
            if directory.exists() {
                if !directory.is_dir() || is_reparse_point(&directory)? {
                    return Err(InstallError::OwnershipDrift(directory));
                }
                fs::remove_dir_all(&directory).map_err(InstallError::io)?;
            }
        }
        for file in [quarantine.current, quarantine.ledger] {
            remove_owned_file(&file)?;
        }
        Ok(())
    }

    fn read_state_file(&self) -> Result<InstallationState, InstallError> {
        let ledger = self.root.join(LEDGER_FILE);
        if is_reparse_point(&ledger)? {
            return Err(InstallError::OwnershipDrift(ledger));
        }
        let bytes = fs::read(&ledger).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                InstallError::MissingLedger
            } else {
                InstallError::io(error)
            }
        })?;
        let state = serde_json::from_slice::<InstallationState>(&bytes)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        validate_installation_state_contract(&state)?;
        Ok(state)
    }

    fn transaction_path(&self) -> PathBuf {
        self.state_dir().join(INSTALL_TRANSACTION_FILE)
    }

    fn read_transaction(&self) -> Result<Option<InstallationTransaction>, InstallError> {
        let path = self.transaction_path();
        if is_reparse_point(&path)? {
            return Err(InstallError::OwnershipDrift(path));
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(InstallError::io(error)),
        };
        let transaction = serde_json::from_slice::<InstallationTransaction>(&bytes)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        if transaction.schema_version != INSTALL_TRANSACTION_SCHEMA_VERSION
            || transaction.desired.active_version.is_none()
        {
            return Err(InstallError::DamagedMetadata(
                "invalid installation transaction".to_owned(),
            ));
        }
        validate_installation_state_contract(&transaction.previous)?;
        validate_installation_state_contract(&transaction.desired)?;
        Ok(Some(transaction))
    }

    fn begin_transaction(&self, transaction: &InstallationTransaction) -> Result<(), InstallError> {
        if self.read_transaction()?.is_some() {
            return Err(InstallError::UnreconciledTransaction);
        }
        let bytes = serde_json::to_vec_pretty(transaction)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        durable_create_new(&self.transaction_path(), &bytes).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                InstallError::UnreconciledTransaction
            } else {
                InstallError::io(error)
            }
        })
    }

    fn write_transaction(&self, transaction: &InstallationTransaction) -> Result<(), InstallError> {
        let bytes = serde_json::to_vec_pretty(transaction)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        atomic_write(&self.transaction_path(), &bytes).map_err(InstallError::io)
    }

    fn remove_transaction(&self) -> Result<(), InstallError> {
        remove_owned_file(&self.transaction_path())
    }

    fn finish_transaction(
        &self,
        transaction: &mut InstallationTransaction,
    ) -> Result<(), InstallError> {
        self.ensure_activation_artifacts_known(transaction)?;
        self.write_state(&transaction.desired)?;
        self.write_activation(&transaction.desired)?;
        transaction.phase = InstallationTransactionPhase::MetadataCommitted;
        self.write_transaction(transaction)?;
        self.verify_state(&transaction.desired)?;
        self.remove_transaction()
    }

    fn ensure_activation_artifacts_known(
        &self,
        transaction: &InstallationTransaction,
    ) -> Result<(), InstallError> {
        let previous = activation_artifacts(&transaction.previous)?;
        let desired = activation_artifacts(&transaction.desired)?;
        let path = self.root.join(CURRENT_FILE);
        {
            let previous_bytes = previous.as_ref().map(|value| value.0.as_slice());
            let desired_bytes = desired.as_ref().map(|value| value.0.as_slice());
            if is_reparse_point(&path)? {
                return Err(InstallError::OwnershipDrift(path.clone()));
            }
            let observed = match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(InstallError::io(error)),
            };
            let known = match observed.as_deref() {
                Some(bytes) => previous_bytes == Some(bytes) || desired_bytes == Some(bytes),
                None => previous_bytes.is_none() || desired_bytes.is_none(),
            };
            if !known {
                return Err(InstallError::OwnershipDrift(path));
            }
        }
        let path = self.stable_agent_entry();
        if is_reparse_point(&path)? {
            return Err(InstallError::OwnershipDrift(path));
        }
        let observed = match fs::read(&path) {
            Ok(bytes) => Some(sha256_bytes(&bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(InstallError::io(error)),
        };
        let known = match observed.as_deref() {
            Some(digest) => {
                previous.as_ref().map(|value| value.1.as_str()) == Some(digest)
                    || desired.as_ref().map(|value| value.1.as_str()) == Some(digest)
            }
            None => previous.is_none() || desired.is_none(),
        };
        if !known {
            return Err(InstallError::OwnershipDrift(path));
        }
        Ok(())
    }

    fn recover_pending_transaction(&self) -> Result<(), InstallError> {
        let Some(mut transaction) = self.read_transaction()? else {
            return Ok(());
        };
        let current = self.read_state_file()?;
        if current != transaction.previous && current != transaction.desired {
            return Err(InstallError::DamagedMetadata(
                "installation ledger differs from both transaction states".to_owned(),
            ));
        }

        let added_versions = transaction
            .desired
            .versions
            .keys()
            .filter(|version| !transaction.previous.versions.contains_key(*version))
            .cloned()
            .collect::<Vec<_>>();
        let removed_versions = transaction
            .previous
            .versions
            .keys()
            .filter(|version| !transaction.desired.versions.contains_key(*version))
            .count();

        match &transaction.staging_name {
            Some(staging_name) => {
                let staging_relative = safe_relative(staging_name)?;
                if staging_relative.components().count() != 1
                    || !staging_name.starts_with(".install-")
                    || added_versions.len() != 1
                    || removed_versions != 0
                {
                    return Err(InstallError::DamagedMetadata(
                        "invalid installation slot transaction".to_owned(),
                    ));
                }
                let version = &added_versions[0];
                let manifest = transaction.desired.versions.get(version).ok_or_else(|| {
                    InstallError::DamagedMetadata("missing desired manifest".to_owned())
                })?;
                let staging = self.state_dir().join(staging_relative);
                let destination = self.versions_dir().join(version);
                let staging_exists = staging.exists();
                let destination_exists = destination.exists();
                if staging_exists && destination_exists {
                    return Err(InstallError::OwnershipDrift(destination));
                }
                if destination_exists {
                    verify_manifest(&destination, manifest)?;
                } else if transaction.phase == InstallationTransactionPhase::Intent {
                    if current != transaction.previous {
                        return Err(InstallError::DamagedMetadata(
                            "intent ledger advanced without a committed slot".to_owned(),
                        ));
                    }
                    if staging_exists {
                        if !staging.is_dir() || is_reparse_point(&staging)? {
                            return Err(InstallError::OwnershipDrift(staging));
                        }
                        fs::remove_dir_all(&staging).map_err(InstallError::io)?;
                    }
                    self.verify_state(&transaction.previous)?;
                    return self.remove_transaction();
                } else if transaction.phase == InstallationTransactionPhase::SlotReady
                    && staging_exists
                {
                    verify_manifest(&staging, manifest)?;
                    fs::rename(&staging, &destination).map_err(InstallError::io)?;
                    transaction.phase = InstallationTransactionPhase::SlotCommitted;
                    self.write_transaction(&transaction)?;
                } else {
                    return Err(InstallError::OwnershipDrift(destination));
                }
            }
            None => {
                if !added_versions.is_empty()
                    || removed_versions != 0
                    || transaction.previous.versions != transaction.desired.versions
                    || transaction.previous.active_version == transaction.desired.active_version
                {
                    return Err(InstallError::DamagedMetadata(
                        "invalid activation-only transaction".to_owned(),
                    ));
                }
            }
        }

        for (version, manifest) in &transaction.desired.versions {
            validate_version(version)?;
            verify_manifest(&self.versions_dir().join(version), manifest)?;
        }
        let expected = transaction
            .desired
            .versions
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        if directory_names(&self.versions_dir())? != expected {
            return Err(InstallError::OwnershipDrift(self.versions_dir()));
        }
        self.finish_transaction(&mut transaction)
    }

    fn verify_state(&self, state: &InstallationState) -> Result<(), InstallError> {
        for directory in [VERSIONS_DIR, BIN_DIR, CONFIG_DIR, STATE_DIR, LOGS_DIR] {
            let path = self.root.join(directory);
            if !path.is_dir() || is_reparse_point(&path)? {
                return Err(InstallError::OwnershipDrift(path));
            }
        }
        for (version, manifest) in &state.versions {
            validate_version(version)?;
            verify_manifest(&self.versions_dir().join(version), manifest)?;
        }
        let expected = state.versions.keys().cloned().collect::<BTreeSet<_>>();
        if directory_names(&self.versions_dir())? != expected {
            return Err(InstallError::OwnershipDrift(self.versions_dir()));
        }
        let bin_expected = match &state.active_version {
            Some(_) => BTreeSet::from([STABLE_AGENT_ENTRY.to_owned()]),
            None => BTreeSet::new(),
        };
        if file_names(&self.root.join(BIN_DIR))? != bin_expected {
            return Err(InstallError::OwnershipDrift(self.root.join(BIN_DIR)));
        }
        match &state.active_version {
            None if self.root.join(CURRENT_FILE).exists() => {
                Err(InstallError::OwnershipDrift(self.root.join(CURRENT_FILE)))
            }
            None => Ok(()),
            Some(version) => {
                if !state.versions.contains_key(version) {
                    return Err(InstallError::DamagedMetadata(
                        "active version is absent from ledger".to_owned(),
                    ));
                }
                let current_path = self.root.join(CURRENT_FILE);
                if is_reparse_point(&current_path)? {
                    return Err(InstallError::OwnershipDrift(current_path));
                }
                let current: ActiveVersion =
                    serde_json::from_slice(&fs::read(&current_path).map_err(InstallError::io)?)
                        .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
                if current.version != *version {
                    return Err(InstallError::OwnershipDrift(current_path));
                }
                let stable = self.stable_agent_entry();
                if !stable.is_file()
                    || is_reparse_point(&stable)?
                    || sha256_file(&stable).map_err(InstallError::io)?
                        != active_agent_digest(state, version)?
                {
                    return Err(InstallError::OwnershipDrift(stable));
                }
                self.active_agent_path_without_state(version).map(|_| ())
            }
        }
    }

    fn active_agent_path_without_state(&self, version: &str) -> Result<PathBuf, InstallError> {
        let path = self
            .versions_dir()
            .join(version)
            .join("runnermesh-agent.exe");
        if !path.is_file() || is_reparse_point(&path)? {
            return Err(InstallError::OwnershipDrift(path));
        }
        Ok(path)
    }

    fn write_activation(&self, state: &InstallationState) -> Result<(), InstallError> {
        let version = state
            .active_version
            .as_deref()
            .ok_or(InstallError::NoActiveVersion)?;
        let _ = self.active_agent_path_without_state(version)?;
        atomic_write(
            &self.root.join(CURRENT_FILE),
            &serde_json::to_vec_pretty(&ActiveVersion {
                version: version.to_owned(),
            })
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
        )
        .map_err(InstallError::io)?;
        let active_agent = self.active_agent_path_without_state(version)?;
        let bytes = fs::read(active_agent).map_err(InstallError::io)?;
        atomic_write(&self.stable_agent_entry(), &bytes).map_err(InstallError::io)
    }

    fn write_state(&self, state: &InstallationState) -> Result<(), InstallError> {
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        atomic_write(&self.root.join(LEDGER_FILE), &bytes).map_err(InstallError::io)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationState {
    pub schema_version: u32,
    pub active_version: Option<String>,
    pub versions: BTreeMap<String, VersionManifest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VersionManifest {
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveVersion {
    version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallReceipt {
    pub version: String,
    pub idempotent: bool,
    pub activated: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum InstallationTransactionPhase {
    Intent,
    SlotReady,
    SlotCommitted,
    MetadataCommitted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct InstallationTransaction {
    schema_version: u32,
    previous: InstallationState,
    desired: InstallationState,
    staging_name: Option<String>,
    phase: InstallationTransactionPhase,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum UninstallPhase {
    Intent,
    AutostartRemoved,
    Quarantined,
    Cleaned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct UninstallTransaction {
    schema_version: u32,
    transaction_id: String,
    state: InstallationState,
    phase: UninstallPhase,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableUninstallReceipt {
    schema_version: u32,
    removed_versions: usize,
}

struct UninstallQuarantinePaths {
    versions: PathBuf,
    bin: PathBuf,
    current: PathBuf,
    ledger: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UninstallReceipt {
    pub removed_versions: usize,
    pub root_removed: bool,
    pub foreign_content_preserved: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutostartEntry {
    pub owner: String,
    pub command: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutostartChange {
    Changed,
    Unchanged,
    Drift,
}

/// A backend owns only the RunnerMesh value it addresses; it cannot enumerate
/// or rewrite unrelated startup values.
pub trait AutostartBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>>;
    fn install_exact(&self, entry: &AutostartEntry) -> io::Result<AutostartChange>;
    fn remove_exact(&self, expected: &AutostartEntry) -> io::Result<AutostartChange>;
}

/// JSON-backed named-value store used solely for deterministic sandbox tests.
#[derive(Clone, Debug)]
pub struct SandboxAutostartBackend {
    path: PathBuf,
    value_name: String,
}

impl SandboxAutostartBackend {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::for_value(path, AUTOSTART_VALUE)
    }

    /// This is useful for fixtures proving that unrelated entries are retained.
    pub fn for_value(path: impl Into<PathBuf>, value_name: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value_name: value_name.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> io::Result<BTreeMap<String, AutostartEntry>> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error),
        }
    }

    fn store(&self, entries: &BTreeMap<String, AutostartEntry>) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(entries)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        atomic_write(&self.path, &bytes)
    }
}

impl AutostartBackend for SandboxAutostartBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>> {
        Ok(self.load()?.remove(&self.value_name))
    }

    fn install_exact(&self, entry: &AutostartEntry) -> io::Result<AutostartChange> {
        if entry.owner != AUTOSTART_OWNER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "autostart owner is not RunnerMesh",
            ));
        }
        let _guard = SANDBOX_AUTOSTART_LOCK
            .lock()
            .map_err(|_| io::Error::other("autostart lock poisoned"))?;
        let mut entries = self.load()?;
        match entries.get(&self.value_name) {
            Some(actual) if actual == entry => return Ok(AutostartChange::Unchanged),
            Some(_) => return Ok(AutostartChange::Drift),
            None => {}
        }
        entries.insert(self.value_name.clone(), entry.clone());
        self.store(&entries)?;
        Ok(AutostartChange::Changed)
    }

    fn remove_exact(&self, expected: &AutostartEntry) -> io::Result<AutostartChange> {
        let _guard = SANDBOX_AUTOSTART_LOCK
            .lock()
            .map_err(|_| io::Error::other("autostart lock poisoned"))?;
        let mut entries = self.load()?;
        match entries.get(&self.value_name) {
            None => return Ok(AutostartChange::Unchanged),
            Some(actual) if actual != expected => return Ok(AutostartChange::Drift),
            Some(_) => {}
        }
        entries.remove(&self.value_name);
        self.store(&entries)?;
        Ok(AutostartChange::Changed)
    }
}

/// Production-capable Windows user-session backend. The caller supplies the
/// exact current-user Startup directory; this type never discovers or mutates
/// any broader startup scope. The exact VBS entry launches the GUI-subsystem
/// stable Agent without a visible console. Creation is create-new only, and
/// removal verifies bytes and deletes the same locked file handle so concurrent
/// replacement cannot turn an owned delete into a foreign delete.
#[cfg(windows)]
#[derive(Clone, Debug)]
pub struct WindowsUserStartupBackend {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

#[cfg(windows)]
impl WindowsUserStartupBackend {
    /// Resolve the operating system's Startup known folder for the current
    /// logon token. This is read-only; no entry is created until
    /// `Installation::enable_autostart` calls the conditional backend method.
    pub fn for_current_user() -> io::Result<Self> {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
        use windows_sys::Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{FOLDERID_Startup, SHGetKnownFolderPath},
        };

        let mut raw = ptr::null_mut();
        // SAFETY: `raw` is a valid out pointer. A null token requests the
        // current user's known folder; the returned allocation is freed below.
        let result =
            unsafe { SHGetKnownFolderPath(&FOLDERID_Startup, 0, ptr::null_mut(), &mut raw) };
        if result < 0 || raw.is_null() {
            return Err(io::Error::other(format!(
                "current-user Startup resolution failed: HRESULT 0x{:08x}",
                result as u32
            )));
        }
        let mut length = 0_usize;
        // SAFETY: a successful `SHGetKnownFolderPath` returns a nul-terminated
        // UTF-16 string. We scan to the terminator before copying.
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
        }
        // SAFETY: the slice is bounded by the terminator found above.
        let directory = unsafe { OsString::from_wide(std::slice::from_raw_parts(raw, length)) };
        // SAFETY: `raw` is the allocation returned by `SHGetKnownFolderPath`.
        unsafe { CoTaskMemFree(raw.cast()) };
        let backend = Self::new(PathBuf::from(directory));
        backend.validate_path()?;
        Ok(backend)
    }

    pub fn new(startup_directory: impl Into<PathBuf>) -> Self {
        Self {
            path: startup_directory.into().join("RunnerMesh.vbs"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn validate_path(&self) -> io::Result<()> {
        validate_explicit_path(&self.path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "startup path has no parent")
        })?;
        if !parent.is_dir() || is_reparse_point(parent).map_err(io::Error::other)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "startup directory is not an ordinary directory",
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
impl AutostartBackend for WindowsUserStartupBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("autostart lock poisoned"))?;
        self.read_unlocked()
    }

    fn install_exact(&self, entry: &AutostartEntry) -> io::Result<AutostartChange> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("autostart lock poisoned"))?;
        self.validate_path()?;
        let directory_guards = guard_existing_directories(&self.path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        directory_guards
            .verify()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let bytes = windows_startup_entry_bytes(entry)?;
        match self.read_unlocked() {
            Ok(Some(actual)) if actual == *entry => return Ok(AutostartChange::Unchanged),
            Ok(Some(_)) => return Ok(AutostartChange::Drift),
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                return Ok(AutostartChange::Drift);
            }
            Err(error) => return Err(error),
        }
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "startup path has no parent")
        })?;
        let staging = parent.join(format!(".RunnerMesh.{:032x}.tmp", unique_suffix()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)?;
        let write_result = file.write_all(&bytes).and_then(|_| file.sync_all());
        drop(file);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&staging);
            return Err(error);
        }
        directory_guards
            .verify()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        match fs::rename(&staging, &self.path) {
            Ok(()) => match self.read_unlocked()? {
                Some(actual) if actual == *entry => Ok(AutostartChange::Changed),
                _ => Ok(AutostartChange::Drift),
            },
            Err(error) => {
                let _ = fs::remove_file(&staging);
                match self.read_unlocked() {
                    Ok(Some(actual)) if actual == *entry => Ok(AutostartChange::Unchanged),
                    Ok(Some(_)) => Ok(AutostartChange::Drift),
                    Ok(None) => Err(error),
                    Err(read_error) if read_error.kind() == io::ErrorKind::InvalidData => {
                        Ok(AutostartChange::Drift)
                    }
                    Err(read_error) => Err(read_error),
                }
            }
        }
    }

    fn remove_exact(&self, expected: &AutostartEntry) -> io::Result<AutostartChange> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("autostart lock poisoned"))?;
        self.validate_path()?;
        let directory_guards = guard_existing_directories(&self.path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        directory_guards
            .verify()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        delete_windows_startup_file_if_exact(&self.path, &windows_startup_entry_bytes(expected)?)
    }
}

#[cfg(windows)]
impl WindowsUserStartupBackend {
    fn read_unlocked(&self) -> io::Result<Option<AutostartEntry>> {
        self.validate_path()?;
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if bytes.len() > 32 * 1024 || is_reparse_point(&self.path).map_err(io::Error::other)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "startup entry is foreign or unsafe",
            ));
        }
        parse_windows_startup_entry(&bytes).map(Some)
    }
}

#[cfg(windows)]
fn windows_startup_entry_bytes(entry: &AutostartEntry) -> io::Result<Vec<u8>> {
    if entry.owner != AUTOSTART_OWNER
        || entry.command.is_empty()
        || entry.command.contains(['\r', '\n'])
        || entry.command.len() > 16 * 1024
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid RunnerMesh startup entry",
        ));
    }
    let command = entry.command.replace('"', "\"\"");
    Ok(format!(
        "' RunnerMesh owner={AUTOSTART_OWNER}\r\nCreateObject(\"WScript.Shell\").Run \"{command}\", 0, False\r\n"
    )
    .into_bytes())
}

#[cfg(windows)]
fn parse_windows_startup_entry(bytes: &[u8]) -> io::Result<AutostartEntry> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "startup entry is not UTF-8"))?;
    let prefix =
        format!("' RunnerMesh owner={AUTOSTART_OWNER}\r\nCreateObject(\"WScript.Shell\").Run \"");
    let encoded = text
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix("\", 0, False\r\n"))
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "startup entry is foreign"))?;
    let entry = AutostartEntry {
        owner: AUTOSTART_OWNER.to_owned(),
        command: encoded.replace("\"\"", "\""),
    };
    if windows_startup_entry_bytes(&entry)? != bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "startup entry is not canonical",
        ));
    }
    Ok(entry)
}

#[cfg(windows)]
fn delete_windows_startup_file_if_exact(
    path: &Path,
    expected: &[u8],
) -> io::Result<AutostartChange> {
    use std::{
        os::windows::{
            ffi::OsStrExt,
            fs::MetadataExt,
            io::{AsRawHandle, FromRawHandle},
        },
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::{
            CreateFileW, FileDispositionInfo, SetFileInformationByHandle, DELETE,
            FILE_ATTRIBUTE_NORMAL, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING,
        },
    };

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: the path is nul-terminated and all pointer arguments remain valid
    // for the call. The returned handle is immediately owned by `File` below.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_GENERIC_READ | DELETE,
            FILE_SHARE_READ,
            ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(2 | 3)) {
            return Ok(AutostartChange::Unchanged);
        }
        return Err(error);
    }
    // SAFETY: `handle` is valid and uniquely transferred into `File`.
    let mut file = unsafe { File::from_raw_handle(handle) };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Ok(AutostartChange::Drift);
    }
    let mut bytes = Vec::new();
    (&mut file).take(32 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes != expected {
        return Ok(AutostartChange::Drift);
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the handle belongs to `file`; the fixed-size disposition buffer
    // is valid for this synchronous call. Deletion applies to this exact handle.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            &disposition as *const _ as *const _,
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    drop(file);
    Ok(AutostartChange::Changed)
}

#[derive(Debug, Eq, PartialEq)]
pub enum InstallError {
    Io(String),
    MissingLedger,
    DamagedMetadata(String),
    InvalidPayload(String),
    InvalidVersion(String),
    SourceRuntimeOverlap,
    ForeignContent(PathBuf),
    OwnershipDrift(PathBuf),
    VersionConflict(String),
    UnknownVersion(String),
    NoActiveVersion,
    AutostartDrift,
    UnreconciledTransaction,
    UninstallInProgress,
    PayloadChecksumMismatch { expected: String, actual: String },
    DamagedRuntimeBinding,
    RuntimeBindingDrift,
    RecoveryFailed(String),
}

impl InstallError {
    fn io(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "installation I/O failed: {error}"),
            Self::MissingLedger => formatter.write_str("installation ownership ledger is absent"),
            Self::DamagedMetadata(error) => {
                write!(formatter, "installation ledger is damaged: {error}")
            }
            Self::InvalidPayload(error) => write!(formatter, "invalid payload: {error}"),
            Self::InvalidVersion(version) => write!(formatter, "invalid version: {version}"),
            Self::SourceRuntimeOverlap => {
                formatter.write_str("source/build and installed runtime overlap")
            }
            Self::ForeignContent(path) => {
                write!(formatter, "foreign content at {}", path.display())
            }
            Self::OwnershipDrift(path) => {
                write!(formatter, "ownership drift at {}", path.display())
            }
            Self::VersionConflict(version) => {
                write!(formatter, "immutable slot conflict for {version}")
            }
            Self::UnknownVersion(version) => {
                write!(formatter, "unknown installed version {version}")
            }
            Self::NoActiveVersion => formatter.write_str("no active installed version"),
            Self::AutostartDrift => formatter.write_str("autostart entry is foreign or changed"),
            Self::UnreconciledTransaction => {
                formatter.write_str("installation transaction requires reconciliation")
            }
            Self::UninstallInProgress => {
                formatter.write_str("uninstall transaction requires reconciliation")
            }
            Self::PayloadChecksumMismatch { expected, actual } => write!(
                formatter,
                "payload checksum mismatch: expected {expected}, got {actual}"
            ),
            Self::DamagedRuntimeBinding => {
                formatter.write_str("installed runtime binding is damaged or invalid")
            }
            Self::RuntimeBindingDrift => {
                formatter.write_str("installed runtime binding differs from the accepted binding")
            }
            Self::RecoveryFailed(error) => {
                write!(formatter, "installation recovery failed: {error}")
            }
        }
    }
}

impl std::error::Error for InstallError {}

fn initial_installation_state() -> InstallationState {
    InstallationState {
        schema_version: INSTALLATION_SCHEMA_VERSION,
        active_version: None,
        versions: BTreeMap::new(),
    }
}

fn validate_installation_state_contract(state: &InstallationState) -> Result<(), InstallError> {
    if state.schema_version != INSTALLATION_SCHEMA_VERSION {
        return Err(InstallError::DamagedMetadata(
            "unsupported installation schema".to_owned(),
        ));
    }
    if state.versions.is_empty() != state.active_version.is_none() {
        return Err(InstallError::DamagedMetadata(
            "installation active-version contract is inconsistent".to_owned(),
        ));
    }
    for (version, manifest) in &state.versions {
        validate_version(version)?;
        if manifest.files.is_empty() || !manifest.files.contains_key("runnermesh-agent.exe") {
            return Err(InstallError::DamagedMetadata(
                "installation manifest is incomplete".to_owned(),
            ));
        }
        for (path, digest) in &manifest.files {
            safe_relative(path)?;
            if !is_lower_hex_digest(digest, 64) {
                return Err(InstallError::DamagedMetadata(
                    "installation manifest digest is invalid".to_owned(),
                ));
            }
        }
    }
    if let Some(active) = &state.active_version {
        validate_version(active)?;
        if !state.versions.contains_key(active) {
            return Err(InstallError::DamagedMetadata(
                "active version is absent from installation state".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_uninstall_transaction(transaction: &UninstallTransaction) -> Result<(), InstallError> {
    if transaction.schema_version != UNINSTALL_TRANSACTION_SCHEMA_VERSION
        || transaction.transaction_id.len() != 42
        || !transaction.transaction_id.starts_with("uninstall-")
        || !transaction.transaction_id[10..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(InstallError::DamagedMetadata(
            "invalid uninstall transaction contract".to_owned(),
        ));
    }
    validate_installation_state_contract(&transaction.state)?;
    for (version, manifest) in &transaction.state.versions {
        validate_version(version)?;
        if manifest.files.is_empty()
            || !manifest
                .files
                .values()
                .all(|digest| is_lower_hex_digest(digest, 64))
        {
            return Err(InstallError::DamagedMetadata(
                "invalid uninstall manifest".to_owned(),
            ));
        }
    }
    if let Some(active) = transaction.state.active_version.as_deref() {
        if !transaction.state.versions.contains_key(active) {
            return Err(InstallError::DamagedMetadata(
                "uninstall active version is absent".to_owned(),
            ));
        }
    }
    Ok(())
}

fn is_lower_hex_digest(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn read_optional_owned_file(path: &Path) -> Result<Option<Vec<u8>>, InstallError> {
    if is_reparse_point(path)? {
        return Err(InstallError::OwnershipDrift(path.to_path_buf()));
    }
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(InstallError::io(error)),
    }
}

fn uninstall_quarantine_paths(
    installation: &Installation,
    transaction: &UninstallTransaction,
) -> UninstallQuarantinePaths {
    let prefix = format!(".{}", transaction.transaction_id);
    UninstallQuarantinePaths {
        versions: installation.state_dir().join(format!("{prefix}-versions")),
        bin: installation.state_dir().join(format!("{prefix}-bin")),
        current: installation
            .state_dir()
            .join(format!("{prefix}-current.json")),
        ledger: installation
            .state_dir()
            .join(format!("{prefix}-ledger.json")),
    }
}

fn quarantine_versions(
    source: &Path,
    destination: &Path,
    state: &InstallationState,
) -> Result<(), InstallError> {
    match (source.exists(), destination.exists()) {
        (true, true) => return Err(InstallError::OwnershipDrift(destination.to_path_buf())),
        (false, false) => {
            return Err(InstallError::OwnershipDrift(source.to_path_buf()));
        }
        (true, false) => {
            verify_versions_root(source, state)?;
            fs::rename(source, destination).map_err(InstallError::io)?;
        }
        (false, true) => {}
    }
    verify_versions_root(destination, state)
}

fn verify_versions_root(root: &Path, state: &InstallationState) -> Result<(), InstallError> {
    if !root.is_dir() || is_reparse_point(root)? {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    for (version, manifest) in &state.versions {
        verify_manifest(&root.join(version), manifest)?;
    }
    let expected = state.versions.keys().cloned().collect::<BTreeSet<_>>();
    if directory_names(root)? != expected {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    Ok(())
}

fn quarantine_bin(
    source: &Path,
    destination: &Path,
    state: &InstallationState,
) -> Result<(), InstallError> {
    match (source.exists(), destination.exists()) {
        (true, true) => return Err(InstallError::OwnershipDrift(destination.to_path_buf())),
        (false, false) => return Err(InstallError::OwnershipDrift(source.to_path_buf())),
        (true, false) => {
            verify_bin_root(source, state)?;
            fs::rename(source, destination).map_err(InstallError::io)?;
        }
        (false, true) => {}
    }
    verify_bin_root(destination, state)
}

fn verify_bin_root(root: &Path, state: &InstallationState) -> Result<(), InstallError> {
    if !root.is_dir() || is_reparse_point(root)? {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    let expected_files = if state.active_version.is_some() {
        BTreeSet::from([STABLE_AGENT_ENTRY.to_owned()])
    } else {
        BTreeSet::new()
    };
    if file_names(root)? != expected_files {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    if let Some(active) = state.active_version.as_deref() {
        let stable = root.join(STABLE_AGENT_ENTRY);
        if !stable.is_file()
            || is_reparse_point(&stable)?
            || sha256_file(&stable).map_err(InstallError::io)?
                != active_agent_digest(state, active)?
        {
            return Err(InstallError::OwnershipDrift(stable));
        }
    }
    Ok(())
}

fn quarantine_file(source: &Path, destination: &Path, expected: &[u8]) -> Result<(), InstallError> {
    match (source.exists(), destination.exists()) {
        (true, true) => return Err(InstallError::OwnershipDrift(destination.to_path_buf())),
        (false, false) => return Err(InstallError::OwnershipDrift(source.to_path_buf())),
        (true, false) => {
            if read_optional_owned_file(source)?.as_deref() != Some(expected) {
                return Err(InstallError::OwnershipDrift(source.to_path_buf()));
            }
            fs::rename(source, destination).map_err(InstallError::io)?;
        }
        (false, true) => {}
    }
    if read_optional_owned_file(destination)?.as_deref() != Some(expected) {
        return Err(InstallError::OwnershipDrift(destination.to_path_buf()));
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<(), InstallError> {
    if version.is_empty()
        || version.len() > 128
        || version.starts_with('.')
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(InstallError::InvalidVersion(version.to_owned()));
    }
    Ok(())
}

fn manifest_for_payload(payload: &Path) -> Result<VersionManifest, InstallError> {
    let mut files = BTreeMap::new();
    collect_manifest(payload, payload, &mut files)?;
    if files.is_empty() {
        return Err(InstallError::InvalidPayload("payload is empty".to_owned()));
    }
    Ok(VersionManifest { files })
}

pub(crate) fn manifest_payload_sha256(manifest: &VersionManifest) -> String {
    let mut hasher = Sha256::new();
    for (path, digest) in &manifest.files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(digest.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn collect_manifest(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), InstallError> {
    for entry in fs::read_dir(current).map_err(InstallError::io)? {
        let entry = entry.map_err(InstallError::io)?;
        let path = entry.path();
        if is_reparse_point(&path)? {
            return Err(InstallError::InvalidPayload(format!(
                "reparse point is not allowed: {}",
                path.display()
            )));
        }
        if path.is_dir() {
            collect_manifest(root, &path, files)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| InstallError::InvalidPayload(error.to_string()))?;
            let key = relative_key(relative)?;
            if files
                .insert(key.clone(), sha256_file(&path).map_err(InstallError::io)?)
                .is_some()
            {
                return Err(InstallError::InvalidPayload(format!(
                    "duplicate payload path {key}"
                )));
            }
        } else {
            return Err(InstallError::InvalidPayload(format!(
                "unsupported payload entry: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn copy_payload(
    source_root: &Path,
    destination_root: &Path,
    manifest: &VersionManifest,
) -> Result<(), InstallError> {
    for relative in manifest.files.keys() {
        let relative = safe_relative(relative)?;
        let source = source_root.join(&relative);
        let destination = destination_root.join(&relative);
        if is_reparse_point(&source)? || !source.is_file() {
            return Err(InstallError::InvalidPayload(source.display().to_string()));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| InstallError::InvalidPayload("payload file has no parent".to_owned()))?;
        fs::create_dir_all(parent).map_err(InstallError::io)?;
        fs::copy(source, destination).map_err(InstallError::io)?;
    }
    Ok(())
}

fn verify_manifest(root: &Path, manifest: &VersionManifest) -> Result<(), InstallError> {
    if !root.is_dir() || is_reparse_point(root)? {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    let observed = manifest_for_payload(root)?;
    if &observed != manifest {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    Ok(())
}

fn directory_names(path: &Path) -> Result<BTreeSet<String>, InstallError> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(path).map_err(InstallError::io)? {
        let entry = entry.map_err(InstallError::io)?;
        let candidate = entry.path();
        if !candidate.is_dir() || is_reparse_point(&candidate)? {
            return Err(InstallError::OwnershipDrift(path.to_path_buf()));
        }
        names.insert(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

fn file_names(path: &Path) -> Result<BTreeSet<String>, InstallError> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(path).map_err(InstallError::io)? {
        let entry = entry.map_err(InstallError::io)?;
        let candidate = entry.path();
        if !candidate.is_file() || is_reparse_point(&candidate)? {
            return Err(InstallError::OwnershipDrift(path.to_path_buf()));
        }
        names.insert(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

fn safe_relative(value: &str) -> Result<PathBuf, InstallError> {
    let path = Path::new(value);
    if path.components().any(|component| {
        !matches!(component, Component::Normal(_))
            || matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
    }) {
        return Err(InstallError::InvalidPayload(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

fn relative_key(path: &Path) -> Result<String, InstallError> {
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(InstallError::InvalidPayload(path.display().to_string()));
    }
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| InstallError::InvalidPayload(path.display().to_string()))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn active_agent_digest(state: &InstallationState, version: &str) -> Result<String, InstallError> {
    state
        .versions
        .get(version)
        .and_then(|manifest| manifest.files.get("runnermesh-agent.exe"))
        .cloned()
        .ok_or_else(|| {
            InstallError::DamagedMetadata("active Agent is absent from manifest".to_owned())
        })
}

fn activation_artifacts(
    state: &InstallationState,
) -> Result<Option<(Vec<u8>, String)>, InstallError> {
    let Some(version) = state.active_version.as_deref() else {
        return Ok(None);
    };
    validate_version(version)?;
    if !state.versions.contains_key(version) {
        return Err(InstallError::DamagedMetadata(
            "active version is absent from ledger".to_owned(),
        ));
    }
    let current = serde_json::to_vec_pretty(&ActiveVersion {
        version: version.to_owned(),
    })
    .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
    Ok(Some((current, active_agent_digest(state, version)?)))
}

/// Persist a complete replacement without deleting the destination first.
/// Windows uses a write-through replace operation because `rename` cannot
/// safely replace an existing destination there.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let directory_guards = guard_existing_directories(path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent)?;
    if path.exists()
        && is_reparse_point(path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing reparse destination",
        ));
    }
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runnermesh"),
        unique_suffix()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        directory_guards
            .verify()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        replace_file(&temporary, path)
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn durable_create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let directory_guards = guard_existing_directories(path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent)?;
    directory_guards
        .verify()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let temporary = validate_explicit_path(temporary)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let destination = validate_explicit_path(destination)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let temporary: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both buffers are nul-terminated and alive for this call.
    if unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

pub(crate) fn is_reparse_point(path: &Path) -> Result<bool, InstallError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Ok(true);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                Ok(metadata.file_attributes() & 0x400 != 0)
            }
            #[cfg(not(windows))]
            Ok(false)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(InstallError::io(error)),
    }
}

pub(crate) struct ExistingDirectoryGuards {
    #[cfg(windows)]
    handles: Vec<GuardedDirectory>,
}

impl ExistingDirectoryGuards {
    fn empty() -> Self {
        Self {
            #[cfg(windows)]
            handles: Vec::new(),
        }
    }

    fn extend(&mut self, other: Self) {
        #[cfg(windows)]
        self.handles.extend(other.handles);
        #[cfg(not(windows))]
        let _ = other;
    }

    pub(crate) fn verify(&self) -> Result<(), InstallError> {
        #[cfg(windows)]
        for guarded in &self.handles {
            let observed = open_guarded_directory(&guarded.path)?;
            if guarded.identity != observed.identity {
                return Err(InstallError::OwnershipDrift(guarded.path.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
struct GuardedDirectory {
    path: PathBuf,
    identity: (u32, u32, u32),
    _file: File,
}

/// Retain an identity handle for every currently existing directory in a path.
/// Mutation boundaries re-open and compare those identities so a renamed or
/// junction-replaced ancestor fails closed. This narrow primitive is used only
/// by install/package ownership operations.
pub(crate) fn guard_existing_directories(
    path: &Path,
) -> Result<ExistingDirectoryGuards, InstallError> {
    let normalized = validate_explicit_path(path)?;
    #[cfg(windows)]
    {
        let mut guards = ExistingDirectoryGuards::empty();
        let mut directories = normalized
            .ancestors()
            .filter(|candidate| candidate.is_dir())
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        directories.reverse();
        directories.dedup();
        for directory in directories {
            guards.handles.push(open_guarded_directory(&directory)?);
        }
        Ok(guards)
    }
    #[cfg(not(windows))]
    {
        let _ = normalized;
        Ok(ExistingDirectoryGuards::empty())
    }
}

#[cfg(windows)]
fn open_guarded_directory(path: &Path) -> Result<GuardedDirectory, InstallError> {
    use std::os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    };
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::{
            CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
    };

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: `wide` is a live nul-terminated UTF-16 path. Omitting
    // FILE_SHARE_DELETE denies ordinary delete-sharing while the retained
    // handle also supplies a stable identity for explicit boundary readback.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(InstallError::io(io::Error::last_os_error()));
    }
    // SAFETY: the valid handle is transferred exactly once into `File`.
    let file = unsafe { File::from_raw_handle(handle) };
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    // SAFETY: `information` is a correctly sized writable output buffer.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(InstallError::io(io::Error::last_os_error()));
    }
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(InstallError::OwnershipDrift(path.to_path_buf()));
    }
    Ok(GuardedDirectory {
        path: path.to_path_buf(),
        identity: (
            information.dwVolumeSerialNumber,
            information.nFileIndexHigh,
            information.nFileIndexLow,
        ),
        _file: file,
    })
}

pub(crate) fn validate_explicit_path(path: &Path) -> Result<PathBuf, InstallError> {
    if !path.is_absolute()
        || path.components().any(|component| match component {
            Component::CurDir | Component::ParentDir => true,
            Component::Normal(value) => {
                let value = value.to_string_lossy();
                value.contains(':') || value.ends_with('.') || value.ends_with(' ')
            }
            Component::Prefix(_) | Component::RootDir => false,
        })
    {
        return Err(InstallError::OwnershipDrift(path.to_path_buf()));
    }

    let existing = path
        .ancestors()
        .find(|candidate| fs::symlink_metadata(candidate).is_ok())
        .ok_or_else(|| InstallError::OwnershipDrift(path.to_path_buf()))?;
    for ancestor in existing.ancestors() {
        if is_reparse_point(ancestor)? {
            return Err(InstallError::OwnershipDrift(ancestor.to_path_buf()));
        }
    }
    let canonical = fs::canonicalize(existing).map_err(InstallError::io)?;
    let suffix = path
        .strip_prefix(existing)
        .map_err(|_| InstallError::OwnershipDrift(path.to_path_buf()))?;
    if suffix.as_os_str().is_empty() {
        Ok(canonical)
    } else {
        Ok(canonical.join(suffix))
    }
}

fn remove_owned_file(path: &Path) -> Result<(), InstallError> {
    if path.exists() {
        if !path.is_file() || is_reparse_point(path)? {
            return Err(InstallError::OwnershipDrift(path.to_path_buf()));
        }
        fs::remove_file(path).map_err(InstallError::io)?;
    }
    Ok(())
}

fn is_empty_dir(path: &Path) -> Result<bool, InstallError> {
    Ok(path.is_dir()
        && fs::read_dir(path)
            .map_err(InstallError::io)?
            .next()
            .is_none())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(windows)]
    use super::{guard_existing_directories, WindowsUserStartupBackend};
    use super::{
        manifest_for_payload, AutostartBackend, AutostartChange, AutostartEntry, InstallError,
        Installation, InstallationTransaction, InstallationTransactionPhase,
        SandboxAutostartBackend, UninstallPhase, UninstallTransaction,
        INSTALL_TRANSACTION_SCHEMA_VERSION, UNINSTALL_TRANSACTION_SCHEMA_VERSION,
    };

    fn root(name: &str) -> PathBuf {
        let value = std::env::temp_dir().join(format!(
            "runnermesh-installation-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&value).unwrap();
        value
    }

    fn payload(root: &Path, name: &str) -> PathBuf {
        let payload = root.join(format!("payload-{name}"));
        fs::create_dir_all(&payload).unwrap();
        fs::write(payload.join("runnermesh.exe"), format!("cli-{name}")).unwrap();
        fs::write(
            payload.join("runnermesh-agent.exe"),
            format!("agent-{name}"),
        )
        .unwrap();
        payload
    }

    fn runtime_binding(root: &Path) -> crate::InstalledRuntimeBinding {
        let scope = crate::RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: "fixture-repository".to_owned(),
        };
        crate::InstalledRuntimeBinding::new(
            crate::AdmissionBinding::new(
                scope.clone(),
                42,
                "fixture-runner",
                crate::CredentialReference::new("windows-credential-manager", "fixture-reference")
                    .unwrap(),
                Some(crate::ReservedLabelOwnership::for_runner(scope, 42)),
            )
            .unwrap(),
            crate::ExactLocalRunnerBinding::new(
                root.join("runner"),
                root.join("work"),
                crate::OpaqueIdentityReference::new("windows-user", "fixture-reference").unwrap(),
            )
            .unwrap(),
            vec!["fixture.exe".to_owned()],
        )
        .unwrap()
    }

    #[test]
    fn immutable_slots_and_stable_autostart_are_sandboxed() {
        let root = root("stable");
        let install = Installation::new(root.join("installed"));
        let payload = payload(&root, "one");
        assert!(install.install("0.1.0", &payload).unwrap().activated);
        assert!(install.install("0.1.0", &payload).unwrap().idempotent);
        let backend = SandboxAutostartBackend::new(root.join("autostart.json"));
        assert!(install.enable_autostart(&backend).unwrap());
        assert!(!install.enable_autostart(&backend).unwrap());
        assert_eq!(
            backend.read().unwrap().unwrap().command,
            format!("\"{}\"", install.stable_agent_entry().display())
        );
        assert!(!backend
            .read()
            .unwrap()
            .unwrap()
            .command
            .contains("target\\debug"));
        assert_eq!(
            fs::read(install.stable_agent_entry()).unwrap(),
            fs::read(payload.join("runnermesh-agent.exe")).unwrap()
        );
        assert_eq!(
            install
                .stable_agent_entry()
                .extension()
                .and_then(|value| value.to_str()),
            Some("exe")
        );
    }

    #[test]
    fn runtime_binding_is_idempotent_and_drift_safe_outside_immutable_slots() {
        let root = root("runtime-binding");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        let binding = runtime_binding(&root);
        assert!(install.configure_runtime_binding(&binding).unwrap());
        assert!(!install.configure_runtime_binding(&binding).unwrap());
        assert!(install.runtime_binding().unwrap().unwrap() == binding);

        let mut drift = binding;
        drift.process_probe_executables = vec!["other.exe".to_owned()];
        assert_eq!(
            install.configure_runtime_binding(&drift),
            Err(InstallError::RuntimeBindingDrift)
        );

        let receipt = install
            .uninstall(&SandboxAutostartBackend::new(root.join("autostart.json")))
            .unwrap();
        assert!(receipt.foreign_content_preserved);
        assert!(install.runtime_binding_path().is_file());
    }

    #[test]
    fn conflicts_corruption_and_foreign_roots_refuse() {
        let root = root("refusal");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        assert!(matches!(
            install.install("0.1.0", &payload(&root, "two")),
            Err(InstallError::VersionConflict(_))
        ));
        fs::write(install.root().join(".runnermesh-installation.json"), b"{").unwrap();
        assert!(matches!(
            install.state(),
            Err(InstallError::DamagedMetadata(_))
        ));

        let foreign = root.join("foreign");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("unrelated.txt"), b"keep").unwrap();
        assert!(matches!(
            Installation::new(&foreign).install("0.1.0", &payload(&root, "three")),
            Err(InstallError::ForeignContent(_))
        ));
    }

    #[test]
    fn autostart_drift_and_unrelated_entries_are_preserved() {
        let root = root("autostart");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        let backend = SandboxAutostartBackend::new(root.join("startup.json"));
        let unrelated = SandboxAutostartBackend::for_value(root.join("startup.json"), "OtherApp");
        unrelated
            .install_exact(&AutostartEntry {
                owner: "other".to_owned(),
                command: "other.exe".to_owned(),
            })
            .unwrap_err();
        fs::write(
            root.join("startup.json"),
            r#"{"OtherApp":{"owner":"other","command":"other.exe"}}"#,
        )
        .unwrap();
        install.enable_autostart(&backend).unwrap();
        assert!(install.disable_autostart(&backend).unwrap());
        assert!(
            std::str::from_utf8(&fs::read(root.join("startup.json")).unwrap())
                .unwrap()
                .contains("OtherApp")
        );
        backend
            .install_exact(&AutostartEntry {
                owner: "runnermesh-v01".to_owned(),
                command: "foreign.exe".to_owned(),
            })
            .map(|change| assert_eq!(change, AutostartChange::Changed))
            .unwrap();
        assert_eq!(
            install.enable_autostart(&backend),
            Err(InstallError::AutostartDrift)
        );
    }

    #[test]
    fn uninstall_preserves_unrelated_content_and_source_root_refuses() {
        let root = root("uninstall");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        fs::write(install.root().join("keep.txt"), b"unrelated").unwrap();
        let result = install
            .uninstall(&SandboxAutostartBackend::new(root.join("auto.json")))
            .unwrap();
        assert!(result.foreign_content_preserved);
        assert!(install.root().join("keep.txt").is_file());
        assert!(matches!(
            Installation::new(env!("CARGO_MANIFEST_DIR")).install("0.1.0", &payload(&root, "two")),
            Err(InstallError::SourceRuntimeOverlap)
        ));
    }

    #[test]
    fn interrupted_initialization_debris_does_not_strand_the_exact_root() {
        let root = root("initialization-interruption");
        let installation_root = root.join("installed");
        let debris = root.join(".installed.runnermesh-init-interrupted");
        fs::create_dir(&debris).unwrap();
        fs::write(debris.join("partial"), b"partial").unwrap();
        let install = Installation::new(&installation_root);
        assert_eq!(
            install.open_or_initialize().unwrap(),
            super::initial_installation_state()
        );
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        assert_eq!(
            install.state().unwrap().active_version.as_deref(),
            Some("0.1.0")
        );
        assert!(debris.is_dir());
    }

    #[test]
    fn interrupted_uninstall_resumes_from_verified_quarantine() {
        let root = root("uninstall-interruption");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        let backend = SandboxAutostartBackend::new(root.join("autostart.json"));
        install.enable_autostart(&backend).unwrap();
        let mut transaction = UninstallTransaction {
            schema_version: UNINSTALL_TRANSACTION_SCHEMA_VERSION,
            transaction_id: "uninstall-00000000000000000000000000000001".to_owned(),
            state: install.state().unwrap(),
            phase: UninstallPhase::Intent,
        };
        install.write_uninstall_transaction(&transaction).unwrap();
        install.disable_autostart(&backend).unwrap();
        transaction.phase = UninstallPhase::AutostartRemoved;
        install.write_uninstall_transaction(&transaction).unwrap();
        install.quarantine_uninstall_content(&transaction).unwrap();

        let receipt = install.uninstall(&backend).unwrap();
        assert_eq!(receipt.removed_versions, 1);
        assert!(receipt.root_removed);
        assert!(!install.root().exists());
        assert_eq!(backend.read().unwrap(), None);
    }

    #[test]
    fn empty_install_and_final_detach_window_remain_recoverable() {
        let empty_root = root("uninstall-empty");
        let empty = Installation::new(empty_root.join("installed"));
        assert_eq!(
            empty.open_or_initialize().unwrap(),
            super::initial_installation_state()
        );
        let empty_receipt = empty
            .uninstall(&SandboxAutostartBackend::new(
                empty_root.join("autostart.json"),
            ))
            .unwrap();
        assert_eq!(empty_receipt.removed_versions, 0);
        assert!(empty_receipt.root_removed);
        assert!(!empty.root().exists());

        let root = root("uninstall-final-detach");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        let backend = SandboxAutostartBackend::new(root.join("autostart.json"));
        let mut transaction = UninstallTransaction {
            schema_version: UNINSTALL_TRANSACTION_SCHEMA_VERSION,
            transaction_id: "uninstall-00000000000000000000000000000002".to_owned(),
            state: install.state().unwrap(),
            phase: UninstallPhase::Intent,
        };
        install.write_uninstall_transaction(&transaction).unwrap();
        assert!(matches!(
            install.resume_uninstall_with_hook(&backend, &mut transaction, || Err(
                InstallError::Io("injected after root detach".to_owned())
            )),
            Err(InstallError::Io(_))
        ));
        assert!(!install.root().exists());
        assert!(install.uninstall(&backend).unwrap().root_removed);
    }

    #[test]
    fn pending_activation_and_partial_slot_are_deterministically_recovered() {
        let root = root("transaction-recovery");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "old")).unwrap();
        install.install("0.2.0", &payload(&root, "new")).unwrap();

        let previous = install.state().unwrap();
        let mut desired = previous.clone();
        desired.active_version = Some("0.2.0".to_owned());
        let activation = InstallationTransaction {
            schema_version: INSTALL_TRANSACTION_SCHEMA_VERSION,
            previous: previous.clone(),
            desired: desired.clone(),
            staging_name: None,
            phase: InstallationTransactionPhase::Intent,
        };
        install.begin_transaction(&activation).unwrap();
        install.write_state(&desired).unwrap();
        assert_eq!(
            install.state().unwrap().active_version.as_deref(),
            Some("0.2.0")
        );
        assert!(!install.transaction_path().exists());

        let previous = install.state().unwrap();
        let next_payload = payload(&root, "third");
        let mut desired = previous.clone();
        desired.versions.insert(
            "0.3.0".to_owned(),
            manifest_for_payload(&next_payload).unwrap(),
        );
        let staging_name = ".install-0.3.0-fixture".to_owned();
        let slot = InstallationTransaction {
            schema_version: INSTALL_TRANSACTION_SCHEMA_VERSION,
            previous: previous.clone(),
            desired,
            staging_name: Some(staging_name.clone()),
            phase: InstallationTransactionPhase::Intent,
        };
        install.begin_transaction(&slot).unwrap();
        let staging = install.state_dir().join(staging_name);
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("partial"), b"partial").unwrap();
        assert_eq!(install.state().unwrap(), previous);
        assert!(!staging.exists());
        assert!(!install.transaction_path().exists());
    }

    #[test]
    fn relative_parent_alias_and_second_digest_drift_are_refused() {
        let root = root("path-and-digest");
        let source = payload(&root, "source");
        assert!(matches!(
            Installation::new(PathBuf::from("relative-installed")).install("0.1.0", &source),
            Err(InstallError::OwnershipDrift(_))
        ));
        assert!(matches!(
            Installation::new(root.join("parent").join("..").join("escaped"))
                .install("0.1.0", &source),
            Err(InstallError::OwnershipDrift(_))
        ));

        let install = Installation::new(root.join("installed"));
        let expected = super::manifest_payload_sha256(&manifest_for_payload(&source).unwrap());
        fs::write(
            source.join("runnermesh-agent.exe"),
            b"changed-after-first-digest",
        )
        .unwrap();
        assert!(matches!(
            install.install_with_payload_sha256("0.1.0", &source, &expected),
            Err(InstallError::PayloadChecksumMismatch { .. })
        ));
        assert!(!install.versions_dir().join("0.1.0").exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_user_startup_backend_is_create_new_handle_bound_and_drift_safe() {
        let root = root("windows-startup");
        let install = Installation::new(root.join("installed"));
        install.install("0.1.0", &payload(&root, "one")).unwrap();
        let startup = root.join("Startup");
        fs::create_dir(&startup).unwrap();
        fs::write(startup.join("OtherApp.cmd"), b"keep").unwrap();
        let backend = WindowsUserStartupBackend::new(&startup);

        let interrupted_staging = startup.join(".RunnerMesh.interrupted.tmp");
        fs::write(&interrupted_staging, b"partial").unwrap();

        assert!(install.enable_autostart(&backend).unwrap());
        assert!(!install.enable_autostart(&backend).unwrap());
        assert_eq!(
            backend.read().unwrap().unwrap(),
            install.expected_autostart_entry()
        );
        let startup_source = fs::read_to_string(backend.path()).unwrap();
        assert!(startup_source.contains("WScript.Shell"));
        assert!(!startup_source.to_ascii_lowercase().contains("cmd.exe"));
        assert_eq!(
            backend.path().extension().and_then(|value| value.to_str()),
            Some("vbs")
        );
        assert!(install.disable_autostart(&backend).unwrap());
        assert!(startup.join("OtherApp.cmd").is_file());
        assert_eq!(fs::read(&interrupted_staging).unwrap(), b"partial");
        assert!(!backend.path().exists());

        assert!(install.enable_autostart(&backend).unwrap());
        fs::write(backend.path(), b"foreign").unwrap();
        assert_eq!(
            install.disable_autostart(&backend),
            Err(InstallError::AutostartDrift)
        );
        assert_eq!(fs::read(backend.path()).unwrap(), b"foreign");
    }

    #[cfg(windows)]
    #[test]
    fn guarded_directory_identity_drift_is_detected_before_mutation() {
        let root = root("directory-guard");
        let protected = root.join("protected");
        fs::create_dir(&protected).unwrap();
        let guard = guard_existing_directories(&protected.join("future")).unwrap();
        let moved = root.join("moved");
        fs::rename(&protected, &moved).unwrap();
        assert!(matches!(guard.verify(), Err(InstallError::Io(_))));
    }
}
