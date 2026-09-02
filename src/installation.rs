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
    time::{SystemTime, UNIX_EPOCH},
};

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
const STABLE_AGENT_ENTRY: &str = "runnermesh-agent.cmd";
const AUTOSTART_OWNER: &str = "runnermesh-v01";
const AUTOSTART_VALUE: &str = "RunnerMesh";

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

    /// Copy a verified payload into an immutable slot. Repeating identical
    /// bytes is idempotent; a version collision with different bytes refuses.
    pub fn install(&self, version: &str, payload: &Path) -> Result<InstallReceipt, InstallError> {
        validate_version(version)?;
        self.assert_runtime_root_safe()?;
        self.assert_payload_safe(payload)?;
        let mut state = self.open_or_initialize()?;
        self.verify_state(&state)?;
        let manifest = manifest_for_payload(payload)?;
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
        let staging = self
            .state_dir()
            .join(format!(".install-{version}-{}", unique_suffix()));
        fs::create_dir_all(&staging).map_err(InstallError::io)?;
        let copy_result = (|| {
            copy_payload(payload, &staging, &manifest)?;
            verify_manifest(&staging, &manifest)?;
            fs::rename(&staging, &destination).map_err(InstallError::io)
        })();
        if copy_result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        copy_result?;

        state.versions.insert(version.to_owned(), manifest);
        let activated = state.active_version.is_none();
        if activated {
            state.active_version = Some(version.to_owned());
            self.write_activation(&state)?;
        }
        self.write_state(&state)?;
        Ok(InstallReceipt {
            version: version.to_owned(),
            idempotent: false,
            activated,
        })
    }

    /// Selects a pre-existing immutable slot. It never modifies a slot.
    pub fn select_active(&self, version: &str) -> Result<(), InstallError> {
        let mut state = self.open_existing()?;
        self.verify_state(&state)?;
        if !state.versions.contains_key(version) {
            return Err(InstallError::UnknownVersion(version.to_owned()));
        }
        state.active_version = Some(version.to_owned());
        self.write_activation(&state)?;
        self.write_state(&state)
    }

    pub fn state(&self) -> Result<InstallationState, InstallError> {
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

    /// Enables exactly the RunnerMesh named autostart value, and only when it
    /// references the stable installed activation entry.
    pub fn enable_autostart(&self, backend: &impl AutostartBackend) -> Result<bool, InstallError> {
        let _ = self.active_agent_path()?;
        let expected = self.expected_autostart_entry();
        match backend.read().map_err(InstallError::io)? {
            None => {
                backend.write(&expected).map_err(InstallError::io)?;
                Ok(true)
            }
            Some(actual) if actual == expected => Ok(false),
            Some(_) => Err(InstallError::AutostartDrift),
        }
    }

    /// Disables only the exact owned RunnerMesh value. A foreign value refuses.
    pub fn disable_autostart(&self, backend: &impl AutostartBackend) -> Result<bool, InstallError> {
        let expected = self.expected_autostart_entry();
        match backend.read().map_err(InstallError::io)? {
            None => Ok(false),
            Some(actual) if actual == expected => {
                backend.remove().map_err(InstallError::io)?;
                Ok(true)
            }
            Some(_) => Err(InstallError::AutostartDrift),
        }
    }

    /// Removes only ledger-verified runtime content. Config, state, logs and
    /// all unrelated root content remain untouched.
    pub fn uninstall(
        &self,
        backend: &impl AutostartBackend,
    ) -> Result<UninstallReceipt, InstallError> {
        let state = self.open_existing()?;
        self.verify_state(&state)?;
        let _ = self.disable_autostart(backend)?;
        fs::remove_dir_all(self.versions_dir()).map_err(InstallError::io)?;
        fs::remove_dir_all(self.root.join(BIN_DIR)).map_err(InstallError::io)?;
        remove_owned_file(&self.root.join(CURRENT_FILE))?;
        remove_owned_file(&self.root.join(LEDGER_FILE))?;
        for directory in [CONFIG_DIR, STATE_DIR, LOGS_DIR] {
            let directory = self.root.join(directory);
            if is_empty_dir(&directory)? {
                fs::remove_dir(&directory).map_err(InstallError::io)?;
            }
        }
        let root_removed = is_empty_dir(&self.root)?;
        if root_removed {
            fs::remove_dir(&self.root).map_err(InstallError::io)?;
        }
        Ok(UninstallReceipt {
            removed_versions: state.versions.len(),
            root_removed,
            foreign_content_preserved: !root_removed,
        })
    }

    pub(crate) fn expected_autostart_entry(&self) -> AutostartEntry {
        AutostartEntry {
            owner: AUTOSTART_OWNER.to_owned(),
            command: format!("\"{}\"", self.stable_agent_entry().display()),
        }
    }

    fn assert_runtime_root_safe(&self) -> Result<(), InstallError> {
        let root = if self.root.exists() {
            fs::canonicalize(&self.root).map_err(InstallError::io)?
        } else {
            absolute_path(&self.root)?
        };
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
        if self.root.exists() {
            let root = fs::canonicalize(&self.root).map_err(InstallError::io)?;
            let payload = fs::canonicalize(payload).map_err(InstallError::io)?;
            if payload.starts_with(&root) || root.starts_with(&payload) {
                return Err(InstallError::SourceRuntimeOverlap);
            }
        }
        Ok(())
    }

    fn open_or_initialize(&self) -> Result<InstallationState, InstallError> {
        if !self.root.exists() {
            fs::create_dir_all(&self.root).map_err(InstallError::io)?;
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
        for directory in [VERSIONS_DIR, BIN_DIR, CONFIG_DIR, STATE_DIR, LOGS_DIR] {
            fs::create_dir(self.root.join(directory)).map_err(InstallError::io)?;
        }
        let state = InstallationState {
            schema_version: INSTALLATION_SCHEMA_VERSION,
            active_version: None,
            versions: BTreeMap::new(),
        };
        self.write_state(&state)?;
        Ok(state)
    }

    fn open_existing(&self) -> Result<InstallationState, InstallError> {
        self.assert_runtime_root_safe()?;
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
        if state.schema_version != INSTALLATION_SCHEMA_VERSION {
            return Err(InstallError::DamagedMetadata(
                "unsupported installation schema".to_owned(),
            ));
        }
        Ok(state)
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
                if fs::read_to_string(&stable).map_err(InstallError::io)?
                    != stable_entry_contents(version)
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
        atomic_write(
            &self.stable_agent_entry(),
            stable_entry_contents(version).as_bytes(),
        )
        .map_err(InstallError::io)
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

/// A backend owns only the RunnerMesh value it addresses; it cannot enumerate
/// or rewrite unrelated startup values.
pub trait AutostartBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>>;
    fn write(&self, entry: &AutostartEntry) -> io::Result<()>;
    fn remove(&self) -> io::Result<()>;
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

    fn write(&self, entry: &AutostartEntry) -> io::Result<()> {
        if entry.owner != AUTOSTART_OWNER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "autostart owner is not RunnerMesh",
            ));
        }
        let mut entries = self.load()?;
        entries.insert(self.value_name.clone(), entry.clone());
        self.store(&entries)
    }

    fn remove(&self) -> io::Result<()> {
        let mut entries = self.load()?;
        if entries.remove(&self.value_name).is_some() {
            self.store(&entries)?;
        }
        Ok(())
    }
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
        }
    }
}

impl std::error::Error for InstallError {}

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

fn stable_entry_contents(version: &str) -> String {
    format!("@echo off\r\n\"%~dp0..\\{VERSIONS_DIR}\\{version}\\runnermesh-agent.exe\" %*\r\n")
}

/// Persist a complete replacement without deleting the destination first.
/// Windows uses a write-through replace operation because `rename` cannot
/// safely replace an existing destination there.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
        replace_file(&temporary, path)
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

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

fn absolute_path(path: &Path) -> Result<PathBuf, InstallError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(InstallError::io)?
            .join(path))
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

    use super::{
        AutostartBackend, AutostartEntry, InstallError, Installation, SandboxAutostartBackend,
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
            .write(&AutostartEntry {
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
            .write(&AutostartEntry {
                owner: "runnermesh-v01".to_owned(),
                command: "foreign.exe".to_owned(),
            })
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
}
