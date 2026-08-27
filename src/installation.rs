//! Sandboxed, ownership-safe versioned installation and user-session autostart.
//!
//! This module operates only on an explicit caller-provided root. It never
//! discovers, selects, or mutates a workstation's production installation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::{self, Read},
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

/// An explicit sandbox layout; production callers must opt in separately at a
/// future human gate.
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

    /// Installs a payload into a new immutable slot. Repeating an identical
    /// install is idempotent; a different payload under the same version is a
    /// refusal, never an overwrite.
    pub fn install(&self, version: &str, payload: &Path) -> Result<InstallReceipt, InstallError> {
        validate_version(version)?;
        self.assert_payload_isolated(payload)?;
        let mut state = self.open_or_initialize()?;
        self.verify_state(&state)?;

        let manifest = manifest_for_payload(payload)?;
        if manifest.files.is_empty() {
            return Err(InstallError::InvalidPayload("payload is empty".to_owned()));
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
            .root
            .join(STATE_DIR)
            .join(format!(".install-{version}-{}", unique_suffix()));
        fs::create_dir_all(&staging).map_err(InstallError::io)?;
        copy_payload(payload, &staging, &manifest)?;
        verify_manifest(&staging, &manifest)?;
        fs::rename(&staging, &destination).map_err(InstallError::io)?;

        state.versions.insert(version.to_owned(), manifest);
        let activate = state.active_version.is_none();
        if activate {
            state.active_version = Some(version.to_owned());
        }
        self.write_state_and_activation(&state)?;

        Ok(InstallReceipt {
            version: version.to_owned(),
            idempotent: false,
            activated: activate,
        })
    }

    /// Changes only the explicit activation indirection. Version slots are
    /// never rewritten.
    pub fn select_active(&self, version: &str) -> Result<(), InstallError> {
        let mut state = self.open_existing()?;
        self.verify_state(&state)?;
        if !state.versions.contains_key(version) {
            return Err(InstallError::UnknownVersion(version.to_owned()));
        }
        state.active_version = Some(version.to_owned());
        self.write_state_and_activation(&state)
    }

    pub fn state(&self) -> Result<InstallationState, InstallError> {
        let state = self.open_existing()?;
        self.verify_state(&state)?;
        Ok(state)
    }

    pub fn active_agent_path(&self) -> Result<PathBuf, InstallError> {
        let state = self.state()?;
        let version = state.active_version.ok_or(InstallError::NoActiveVersion)?;
        let agent = self
            .versions_dir()
            .join(version)
            .join("runnermesh-agent.exe");
        if !agent.is_file() {
            return Err(InstallError::OwnershipDrift(agent));
        }
        Ok(agent)
    }

    /// Installs a user-session autostart reference to the stable entry only.
    /// The supplied backend is normally a sandbox backend during qualification.
    pub fn install_autostart(&self, backend: &impl AutostartBackend) -> Result<bool, InstallError> {
        let _ = self.active_agent_path()?;
        let desired = self.expected_autostart_entry();
        match backend.read().map_err(InstallError::io)? {
            None => {
                backend.write(&desired).map_err(InstallError::io)?;
                Ok(false)
            }
            Some(current) if current == desired => Ok(true),
            Some(_) => Err(InstallError::AutostartDrift),
        }
    }

    pub fn remove_autostart(&self, backend: &impl AutostartBackend) -> Result<bool, InstallError> {
        let desired = self.expected_autostart_entry();
        match backend.read().map_err(InstallError::io)? {
            None => Ok(false),
            Some(current) if current == desired => {
                backend.remove().map_err(InstallError::io)?;
                Ok(true)
            }
            Some(_) => Err(InstallError::AutostartDrift),
        }
    }

    /// Removes only ledger-verified product content. Unknown files are never
    /// removed and cause the root to be retained.
    pub fn uninstall(
        &self,
        backend: &impl AutostartBackend,
    ) -> Result<UninstallReceipt, InstallError> {
        let state = self.open_existing()?;
        self.verify_state(&state)?;
        let _ = self.remove_autostart(backend)?;

        let versions = self.versions_dir();
        if versions.exists() {
            fs::remove_dir_all(&versions).map_err(InstallError::io)?;
        }
        let bin = self.root.join(BIN_DIR);
        if bin.exists() {
            fs::remove_dir_all(&bin).map_err(InstallError::io)?;
        }
        let current = self.root.join(CURRENT_FILE);
        if current.exists() {
            fs::remove_file(&current).map_err(InstallError::io)?;
        }
        let ledger = self.root.join(LEDGER_FILE);
        if ledger.exists() {
            fs::remove_file(&ledger).map_err(InstallError::io)?;
        }

        for directory in [CONFIG_DIR, STATE_DIR, LOGS_DIR] {
            let path = self.root.join(directory);
            if is_empty_dir(&path)? {
                fs::remove_dir(&path).map_err(InstallError::io)?;
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

    fn expected_autostart_entry(&self) -> AutostartEntry {
        AutostartEntry {
            owner: AUTOSTART_OWNER.to_owned(),
            command: format!("\"{}\"", self.stable_agent_entry().display()),
        }
    }

    fn assert_payload_isolated(&self, payload: &Path) -> Result<(), InstallError> {
        if !payload.is_dir() {
            return Err(InstallError::InvalidPayload(format!(
                "payload is not a directory: {}",
                payload.display()
            )));
        }
        let payload = fs::canonicalize(payload).map_err(InstallError::io)?;
        if self.root.exists() {
            let root = fs::canonicalize(&self.root).map_err(InstallError::io)?;
            if payload.starts_with(&root) || root.starts_with(&payload) {
                return Err(InstallError::SourceRuntimeOverlap);
            }
        }
        Ok(())
    }

    fn open_or_initialize(&self) -> Result<InstallationState, InstallError> {
        if self.root.exists() && !self.root.is_dir() {
            return Err(InstallError::ForeignContent(self.root.clone()));
        }
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
            fs::create_dir_all(self.root.join(directory)).map_err(InstallError::io)?;
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
        let ledger = self.root.join(LEDGER_FILE);
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
            if !path.is_dir() {
                return Err(InstallError::OwnershipDrift(path));
            }
        }
        for (version, manifest) in &state.versions {
            let slot = self.versions_dir().join(version);
            verify_manifest(&slot, manifest)?;
        }

        let expected_versions = state.versions.keys().cloned().collect::<BTreeSet<_>>();
        let actual_versions = directory_names(&self.versions_dir())?;
        if actual_versions != expected_versions {
            return Err(InstallError::OwnershipDrift(self.versions_dir()));
        }

        match &state.active_version {
            None => {
                if self.root.join(CURRENT_FILE).exists() || self.stable_agent_entry().exists() {
                    return Err(InstallError::OwnershipDrift(self.root.clone()));
                }
            }
            Some(version) => {
                if !state.versions.contains_key(version) {
                    return Err(InstallError::DamagedMetadata(
                        "active version is absent from ledger".to_owned(),
                    ));
                }
                let current = fs::read(self.root.join(CURRENT_FILE)).map_err(InstallError::io)?;
                let current = serde_json::from_slice::<ActiveVersion>(&current)
                    .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
                if current.version != *version {
                    return Err(InstallError::OwnershipDrift(self.root.join(CURRENT_FILE)));
                }
                let expected = stable_entry_contents(version);
                let actual =
                    fs::read_to_string(self.stable_agent_entry()).map_err(InstallError::io)?;
                if actual != expected {
                    return Err(InstallError::OwnershipDrift(self.stable_agent_entry()));
                }
                if !self
                    .versions_dir()
                    .join(version)
                    .join("runnermesh-agent.exe")
                    .is_file()
                {
                    return Err(InstallError::OwnershipDrift(
                        self.versions_dir()
                            .join(version)
                            .join("runnermesh-agent.exe"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn write_state_and_activation(&self, state: &InstallationState) -> Result<(), InstallError> {
        let version = state
            .active_version
            .as_deref()
            .ok_or(InstallError::NoActiveVersion)?;
        let candidate = self
            .versions_dir()
            .join(version)
            .join("runnermesh-agent.exe");
        if !candidate.is_file() {
            return Err(InstallError::InvalidPayload(
                "payload must contain runnermesh-agent.exe".to_owned(),
            ));
        }
        atomic_write(
            &self.root.join(CURRENT_FILE),
            &serde_json::to_vec_pretty(&ActiveVersion {
                version: version.to_owned(),
            })
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?,
        )?;
        atomic_write(
            &self.stable_agent_entry(),
            stable_entry_contents(version).as_bytes(),
        )?;
        self.write_state(state)
    }

    fn write_state(&self, state: &InstallationState) -> Result<(), InstallError> {
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| InstallError::DamagedMetadata(error.to_string()))?;
        atomic_write(&self.root.join(LEDGER_FILE), &bytes)
    }
}

/// The complete ownership ledger. It intentionally records only immutable
/// slots and activation selection; config/state/log user data stay separate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstallationState {
    pub schema_version: u32,
    pub active_version: Option<String>,
    pub versions: BTreeMap<String, VersionManifest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VersionManifest {
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
pub struct AutostartEntry {
    pub owner: String,
    pub command: String,
}

/// A small backend interface allows all qualification to remain sandboxed.
pub trait AutostartBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>>;
    fn write(&self, entry: &AutostartEntry) -> io::Result<()>;
    fn remove(&self) -> io::Result<()>;
}

/// File-backed stand-in for the Windows user-session Run-key backend used by
/// deterministic sandbox fixtures. Its file is caller-owned and explicit.
#[derive(Clone, Debug)]
pub struct SandboxAutostartBackend {
    path: PathBuf,
}

impl SandboxAutostartBackend {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AutostartBackend for SandboxAutostartBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&self, entry: &AutostartEntry) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(entry)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        atomic_write_io(&self.path, &bytes)
    }

    fn remove(&self) -> io::Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// Windows user-session Run-key backend. It is deliberately inert unless a
/// caller explicitly invokes it; all F1 fixtures use [`SandboxAutostartBackend`].
#[cfg(windows)]
#[derive(Clone, Debug)]
pub struct WindowsRunKeyAutostartBackend {
    value_name: String,
}

#[cfg(windows)]
impl WindowsRunKeyAutostartBackend {
    pub fn new(value_name: impl Into<String>) -> Self {
        Self {
            value_name: value_name.into(),
        }
    }

    fn run_key(&self) -> io::Result<winreg::RegKey> {
        use winreg::{enums::*, RegKey};

        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            KEY_READ | KEY_WRITE,
        )
    }
}

#[cfg(windows)]
impl Default for WindowsRunKeyAutostartBackend {
    fn default() -> Self {
        Self::new("RunnerMesh")
    }
}

#[cfg(windows)]
impl AutostartBackend for WindowsRunKeyAutostartBackend {
    fn read(&self) -> io::Result<Option<AutostartEntry>> {
        match self.run_key()?.get_value::<String, _>(&self.value_name) {
            Ok(command) => Ok(Some(AutostartEntry {
                owner: AUTOSTART_OWNER.to_owned(),
                command,
            })),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&self, entry: &AutostartEntry) -> io::Result<()> {
        if entry.owner != AUTOSTART_OWNER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "autostart owner does not match RunnerMesh",
            ));
        }
        self.run_key()?.set_value(&self.value_name, &entry.command)
    }

    fn remove(&self) -> io::Result<()> {
        match self.run_key()?.delete_value(&self.value_name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug)]
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
                write!(formatter, "installation metadata is damaged: {error}")
            }
            Self::InvalidPayload(error) => write!(formatter, "invalid payload: {error}"),
            Self::InvalidVersion(version) => write!(formatter, "invalid version: {version}"),
            Self::SourceRuntimeOverlap => {
                formatter.write_str("source and installed runtime overlap")
            }
            Self::ForeignContent(path) => {
                write!(formatter, "foreign content at {}", path.display())
            }
            Self::OwnershipDrift(path) => {
                write!(formatter, "owned content drift at {}", path.display())
            }
            Self::VersionConflict(version) => write!(formatter, "version slot conflict: {version}"),
            Self::UnknownVersion(version) => write!(formatter, "unknown version: {version}"),
            Self::NoActiveVersion => formatter.write_str("no active version"),
            Self::AutostartDrift => {
                formatter.write_str("autostart entry is not owned by RunnerMesh")
            }
        }
    }
}

impl std::error::Error for InstallError {}

fn validate_version(version: &str) -> Result<(), InstallError> {
    if version.is_empty()
        || version.len() > 128
        || version.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+'))
        })
    {
        return Err(InstallError::InvalidVersion(version.to_owned()));
    }
    Ok(())
}

fn manifest_for_payload(payload: &Path) -> Result<VersionManifest, InstallError> {
    let mut files = BTreeMap::new();
    collect_manifest(payload, payload, &mut files)?;
    Ok(VersionManifest { files })
}

fn collect_manifest(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), InstallError> {
    for entry in fs::read_dir(current).map_err(InstallError::io)? {
        let entry = entry.map_err(InstallError::io)?;
        let file_type = entry.file_type().map_err(InstallError::io)?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(InstallError::InvalidPayload(format!(
                "symbolic links are not accepted: {}",
                path.display()
            )));
        }
        if file_type.is_dir() {
            collect_manifest(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| InstallError::InvalidPayload(error.to_string()))?;
            let key = relative_key(relative)?;
            files.insert(key, sha256_file(&path)?);
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
    source: &Path,
    destination: &Path,
    manifest: &VersionManifest,
) -> Result<(), InstallError> {
    for relative in manifest.files.keys() {
        let relative_path = Path::new(relative);
        let source = source.join(relative_path);
        let destination = destination.join(relative_path);
        let parent = destination
            .parent()
            .ok_or_else(|| InstallError::InvalidPayload(relative.clone()))?;
        fs::create_dir_all(parent).map_err(InstallError::io)?;
        fs::copy(source, destination).map_err(InstallError::io)?;
    }
    Ok(())
}

fn verify_manifest(root: &Path, manifest: &VersionManifest) -> Result<(), InstallError> {
    let actual = manifest_for_payload(root)?;
    if &actual != manifest {
        return Err(InstallError::OwnershipDrift(root.to_path_buf()));
    }
    Ok(())
}

fn directory_names(path: &Path) -> Result<BTreeSet<String>, InstallError> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(path).map_err(InstallError::io)? {
        let entry = entry.map_err(InstallError::io)?;
        if !entry.file_type().map_err(InstallError::io)?.is_dir() {
            return Err(InstallError::OwnershipDrift(path.to_path_buf()));
        }
        names.insert(
            entry
                .file_name()
                .to_str()
                .ok_or_else(|| InstallError::OwnershipDrift(path.to_path_buf()))?
                .to_owned(),
        );
    }
    Ok(names)
}

fn relative_key(path: &Path) -> Result<String, InstallError> {
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(InstallError::InvalidPayload(path.display().to_string()));
    }
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| InstallError::InvalidPayload(path.display().to_string()))
}

fn sha256_file(path: &Path) -> Result<String, InstallError> {
    let mut file = fs::File::open(path).map_err(InstallError::io)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(InstallError::io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

fn stable_entry_contents(version: &str) -> String {
    format!("@echo off\r\n\"%~dp0..\\{VERSIONS_DIR}\\{version}\\runnermesh-agent.exe\" %*\r\n")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    atomic_write_io(path, bytes).map_err(InstallError::io)
}

fn atomic_write_io(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runnermesh"),
        unique_suffix()
    ));
    fs::write(&temporary, bytes)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_first_error) if path.exists() => {
            fs::remove_file(path)?;
            fs::rename(temporary, path)
        }
        Err(error) => Err(error),
    }
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn is_empty_dir(path: &Path) -> Result<bool, InstallError> {
    if !path.exists() {
        return Ok(false);
    }
    Ok(fs::read_dir(path)
        .map_err(InstallError::io)?
        .next()
        .is_none())
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

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "runnermesh-installation-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn payload(root: &Path, marker: &str) -> PathBuf {
        let payload = root.join("payload");
        fs::create_dir_all(&payload).unwrap();
        fs::write(
            payload.join("runnermesh-agent.exe"),
            format!("agent-{marker}"),
        )
        .unwrap();
        fs::write(payload.join("runnermesh.exe"), format!("cli-{marker}")).unwrap();
        payload
    }

    #[test]
    fn consolidated_install_autostart_and_ownership_fixture_family() {
        let root = temp_root("consolidated");
        let source_one = payload(&root, "one");
        let installation = Installation::new(root.join("installed"));
        let backend = SandboxAutostartBackend::new(root.join("autostart.json"));

        let clean = installation.install("0.1.0", &source_one).unwrap();
        assert!(clean.activated);
        assert!(!clean.idempotent);
        assert!(!installation.install_autostart(&backend).unwrap());
        assert!(installation.install_autostart(&backend).unwrap());

        let reinstall = installation.install("0.1.0", &source_one).unwrap();
        assert!(reinstall.idempotent);

        let source_two = payload(&temp_root("second-payload"), "two");
        let second = installation.install("0.1.1", &source_two).unwrap();
        assert!(!second.activated);
        installation.select_active("0.1.1").unwrap();
        assert!(installation
            .active_agent_path()
            .unwrap()
            .ends_with("versions/0.1.1/runnermesh-agent.exe"));

        fs::remove_dir_all(&source_two).unwrap();
        assert!(installation.active_agent_path().unwrap().is_file());
        assert!(installation.remove_autostart(&backend).unwrap());
        assert!(!installation.remove_autostart(&backend).unwrap());

        let receipt = installation.uninstall(&backend).unwrap();
        assert_eq!(receipt.removed_versions, 2);
        assert!(receipt.root_removed);
    }

    #[test]
    fn foreign_content_and_drift_are_refused_without_overwrite() {
        let root = temp_root("foreign");
        let foreign = root.join("foreign-install");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("keep.txt"), "foreign").unwrap();
        assert!(matches!(
            Installation::new(&foreign).install("0.1.0", &payload(&root, "one")),
            Err(InstallError::ForeignContent(_))
        ));

        let installation = Installation::new(root.join("installed"));
        let payload = payload(&root, "owned");
        installation.install("0.1.0", &payload).unwrap();
        let agent = installation.active_agent_path().unwrap();
        fs::write(&agent, "tampered").unwrap();
        assert!(matches!(
            installation.select_active("0.1.0"),
            Err(InstallError::OwnershipDrift(_))
        ));
        assert!(matches!(
            installation.uninstall(&SandboxAutostartBackend::new(root.join("auto.json"))),
            Err(InstallError::OwnershipDrift(_))
        ));
    }

    #[test]
    fn damaged_metadata_and_foreign_autostart_are_refused() {
        let root = temp_root("metadata");
        let installation = Installation::new(root.join("installed"));
        let owned_payload = payload(&root, "one");
        installation.install("0.1.0", &owned_payload).unwrap();
        fs::write(
            installation.root().join(".runnermesh-installation.json"),
            "{bad json",
        )
        .unwrap();
        assert!(matches!(
            installation.state(),
            Err(InstallError::DamagedMetadata(_))
        ));

        let root = temp_root("autostart");
        let installation = Installation::new(root.join("installed"));
        installation
            .install("0.1.0", &payload(&root, "one"))
            .unwrap();
        let backend = SandboxAutostartBackend::new(root.join("autostart.json"));
        backend
            .write(&AutostartEntry {
                owner: "foreign".to_owned(),
                command: "foreign.exe".to_owned(),
            })
            .unwrap();
        assert!(matches!(
            installation.install_autostart(&backend),
            Err(InstallError::AutostartDrift)
        ));
        assert!(matches!(
            installation.remove_autostart(&backend),
            Err(InstallError::AutostartDrift)
        ));
    }

    #[test]
    fn uninstall_preserves_unrelated_root_content() {
        let root = temp_root("preserve");
        let installation = Installation::new(root.join("installed"));
        installation
            .install("0.1.0", &payload(&root, "one"))
            .unwrap();
        fs::write(installation.root().join("user-note.txt"), "keep").unwrap();
        let receipt = installation
            .uninstall(&SandboxAutostartBackend::new(root.join("autostart.json")))
            .unwrap();
        assert!(!receipt.root_removed);
        assert!(receipt.foreign_content_preserved);
        assert!(installation.root().join("user-note.txt").is_file());
    }
}
