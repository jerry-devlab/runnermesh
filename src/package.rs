//! Explicit-input Windows x64 package construction, verification and doctor.
//!
//! This is an artifact helper only. It neither discovers an installation nor
//! uploads, publishes, activates, or starts a runtime.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, OpenOptions},
    io::{self, Cursor, Read, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    installation::{
        guard_existing_directories, is_reparse_point, sha256_bytes, sha256_file,
        validate_explicit_path,
    },
    AutostartBackend, Installation, UpdateCoordinator, UpdatePhase,
};

pub const PACKAGE_SCHEMA_VERSION: u32 = 1;
pub const WINDOWS_X64_TARGET: &str = "x86_64-pc-windows-msvc";
const MANIFEST_NAME: &str = "PACKAGE-MANIFEST.json";
const CHECKSUMS_NAME: &str = "SHA256SUMS";
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 64 * 1024 * 1024;
const RUNTIME_NAMES: [&str; 3] = [
    "runnermesh.exe",
    "runnermesh-agent.exe",
    "runnermesh-agent.manifest",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageProvenance {
    pub version: String,
    pub commit: String,
    pub channel: String,
    pub target: String,
}

/// All package source paths are explicit file inputs; directories are never
/// scanned and no installed runtime is discovered implicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInput {
    pub provenance: PackageProvenance,
    pub cli_binary: PathBuf,
    pub agent_binary: PathBuf,
    pub agent_manifest: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub provenance: PackageProvenance,
    /// SHA-256 for each runtime package member, keyed by archive name.
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageReceipt {
    pub archive: PathBuf,
    pub archive_sha256: String,
    pub manifest: PackageManifest,
}

struct VerifiedArchive {
    manifest: PackageManifest,
    contents: BTreeMap<String, Vec<u8>>,
    archive_sha256: String,
}

pub struct PackageVerifier;

impl PackageVerifier {
    /// Creates one immutable archive. A pre-existing archive name is foreign
    /// output and is never overwritten.
    pub fn create(
        input: &PackageInput,
        output_directory: &Path,
    ) -> Result<PackageReceipt, PackageError> {
        let output_guards = guard_existing_directories(output_directory)
            .map_err(|_| PackageError::ForeignOutput(output_directory.to_path_buf()))?;
        output_guards
            .verify()
            .map_err(|_| PackageError::ForeignOutput(output_directory.to_path_buf()))?;
        validate_provenance(&input.provenance)?;
        let mut contents = BTreeMap::new();
        contents.insert(
            "runnermesh.exe".to_owned(),
            read_required_file(&input.cli_binary)?,
        );
        contents.insert(
            "runnermesh-agent.exe".to_owned(),
            read_required_file(&input.agent_binary)?,
        );
        contents.insert(
            "runnermesh-agent.manifest".to_owned(),
            read_required_file(&input.agent_manifest)?,
        );
        let files = contents
            .iter()
            .map(|(name, bytes)| (name.clone(), sha256_bytes(bytes)))
            .collect();
        let manifest = PackageManifest {
            schema_version: PACKAGE_SCHEMA_VERSION,
            provenance: input.provenance.clone(),
            files,
        };
        contents.insert(
            MANIFEST_NAME.to_owned(),
            serde_json::to_vec_pretty(&manifest)
                .map_err(|error| PackageError::MalformedManifest(error.to_string()))?,
        );
        contents.insert(
            CHECKSUMS_NAME.to_owned(),
            checksum_file(&contents).into_bytes(),
        );

        validate_explicit_path(output_directory)
            .map_err(|_| PackageError::ForeignOutput(output_directory.to_path_buf()))?;
        if output_directory.exists()
            && (!output_directory.is_dir()
                || is_reparse_point(output_directory)
                    .map_err(|error| PackageError::MalformedManifest(error.to_string()))?)
        {
            return Err(PackageError::ForeignOutput(output_directory.to_path_buf()));
        }
        if !output_directory.exists() {
            fs::create_dir(output_directory).map_err(PackageError::io)?;
        }
        let created_output_guards = guard_existing_directories(output_directory)
            .map_err(|_| PackageError::ForeignOutput(output_directory.to_path_buf()))?;
        created_output_guards
            .verify()
            .map_err(|_| PackageError::ForeignOutput(output_directory.to_path_buf()))?;
        validate_explicit_path(output_directory)
            .map_err(|_| PackageError::ForeignOutput(output_directory.to_path_buf()))?;
        let archive = output_directory.join(format!(
            "runnermesh-{}-windows-x64.zip",
            input.provenance.version
        ));
        if archive.exists() {
            return Err(PackageError::ForeignOutput(archive));
        }
        write_zip(&archive, &contents)?;
        let verified = read_verified_archive(&archive)?;
        if verified.manifest != manifest {
            return Err(PackageError::MalformedManifest(
                "post-write manifest changed".to_owned(),
            ));
        }
        Ok(PackageReceipt {
            archive,
            archive_sha256: verified.archive_sha256,
            manifest,
        })
    }

    /// Validates archive member names, duplicate entries, exact content set,
    /// package checksums and schema/provenance semantics.
    pub fn verify(archive: &Path) -> Result<PackageManifest, PackageError> {
        Self::verify_with_hash(archive).map(|(manifest, _)| manifest)
    }

    /// Returns the manifest and SHA-256 derived from the same in-memory bytes.
    /// Callers that publish or record a digest must use this method rather than
    /// reopening a previously verified path.
    pub fn verify_with_hash(archive: &Path) -> Result<(PackageManifest, String), PackageError> {
        read_verified_archive(archive).map(|verified| (verified.manifest, verified.archive_sha256))
    }

    fn verify_contents(
        contents: &BTreeMap<String, Vec<u8>>,
    ) -> Result<PackageManifest, PackageError> {
        let names = contents.keys().cloned().collect::<BTreeSet<_>>();
        let expected = expected_package_names();
        if names != expected {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        let manifest: PackageManifest = serde_json::from_slice(
            contents
                .get(MANIFEST_NAME)
                .ok_or(PackageError::UnexpectedArchiveContents)?,
        )
        .map_err(|error| PackageError::MalformedManifest(error.to_string()))?;
        if manifest.schema_version != PACKAGE_SCHEMA_VERSION {
            return Err(PackageError::MalformedManifest(
                "unsupported package schema".to_owned(),
            ));
        }
        validate_provenance(&manifest.provenance)?;
        let expected_runtime = RUNTIME_NAMES.into_iter().map(str::to_owned).collect();
        if manifest.files.keys().cloned().collect::<BTreeSet<_>>() != expected_runtime {
            return Err(PackageError::MalformedManifest(
                "content manifest must cover the exact runtime payload".to_owned(),
            ));
        }
        for (name, expected_hash) in &manifest.files {
            validate_hash(expected_hash)?;
            let Some(actual) = contents.get(name) else {
                return Err(PackageError::UnexpectedArchiveContents);
            };
            if sha256_bytes(actual) != *expected_hash {
                return Err(PackageError::ChecksumMismatch(name.clone()));
            }
        }
        let sums = std::str::from_utf8(
            contents
                .get(CHECKSUMS_NAME)
                .ok_or(PackageError::UnexpectedArchiveContents)?,
        )
        .map_err(|error| PackageError::MalformedManifest(error.to_string()))?;
        verify_checksum_file(sums, contents)?;
        Ok(manifest)
    }

    pub fn verify_expected(
        archive: &Path,
        expected: &PackageProvenance,
    ) -> Result<PackageManifest, PackageError> {
        let manifest = Self::verify(archive)?;
        if &manifest.provenance != expected {
            return Err(PackageError::ProvenanceMismatch {
                expected: Box::new(expected.clone()),
                actual: Box::new(manifest.provenance),
            });
        }
        Ok(manifest)
    }

    pub fn archive_sha256(archive: &Path) -> Result<String, PackageError> {
        validate_explicit_path(archive)
            .map_err(|_| PackageError::ForeignOutput(archive.to_path_buf()))?;
        if is_reparse_point(archive)
            .map_err(|error| PackageError::MalformedManifest(error.to_string()))?
        {
            return Err(PackageError::ForeignOutput(archive.to_path_buf()));
        }
        sha256_file(archive).map_err(PackageError::io)
    }

    /// Writes only verified runtime members into an explicit, empty sandbox
    /// directory. Metadata remains in the archive and does not become runtime.
    pub fn extract_runtime(
        archive: &Path,
        destination: &Path,
    ) -> Result<PackageManifest, PackageError> {
        let destination_guards = guard_existing_directories(destination)
            .map_err(|_| PackageError::ForeignOutput(destination.to_path_buf()))?;
        destination_guards
            .verify()
            .map_err(|_| PackageError::ForeignOutput(destination.to_path_buf()))?;
        let verified = read_verified_archive(archive)?;
        validate_explicit_path(destination)
            .map_err(|_| PackageError::ForeignOutput(destination.to_path_buf()))?;
        if destination.exists() {
            return Err(PackageError::ForeignOutput(destination.to_path_buf()));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| PackageError::ForeignOutput(destination.to_path_buf()))?;
        validate_explicit_path(parent)
            .map_err(|_| PackageError::ForeignOutput(parent.to_path_buf()))?;
        if !parent.is_dir()
            || is_reparse_point(parent)
                .map_err(|error| PackageError::MalformedManifest(error.to_string()))?
        {
            return Err(PackageError::ForeignOutput(parent.to_path_buf()));
        }
        let staging = parent.join(format!(
            ".runnermesh-extract-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir(&staging).map_err(PackageError::io)?;
        let result = (|| {
            for name in RUNTIME_NAMES {
                let path = staging.join(name);
                if path.parent() != Some(staging.as_path()) {
                    return Err(PackageError::UnexpectedArchiveContents);
                }
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(PackageError::io)?;
                file.write_all(
                    verified
                        .contents
                        .get(name)
                        .ok_or(PackageError::UnexpectedArchiveContents)?,
                )
                .map_err(PackageError::io)?;
                file.sync_all().map_err(PackageError::io)?;
            }
            destination_guards
                .verify()
                .map_err(|_| PackageError::ForeignOutput(destination.to_path_buf()))?;
            fs::rename(&staging, destination).map_err(PackageError::io)
        })();
        if result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result?;
        Ok(verified.manifest)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductizationCheckStatus {
    Pass,
    Fail,
    Unknown,
    NotConfigured,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProductizationCheck {
    pub id: &'static str,
    pub status: ProductizationCheckStatus,
}

/// Evidence reports only the productization observations this component really
/// performs. It intentionally reports no runner, IPC, probe or scheduler fact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackageDoctorReport {
    pub checks: Vec<ProductizationCheck>,
}

pub struct PackageDoctor;

impl PackageDoctor {
    pub fn inspect(
        archive: &Path,
        installation: &Installation,
        updates: &UpdateCoordinator,
        autostart: &impl AutostartBackend,
    ) -> PackageDoctorReport {
        let package = status_from(PackageVerifier::verify(archive));
        let state = installation.state();
        let installed = match &state {
            Ok(_) => ProductizationCheckStatus::Pass,
            Err(_) => ProductizationCheckStatus::Fail,
        };
        let active = match &state {
            Ok(state)
                if state.active_version.is_some() && installation.active_agent_path().is_ok() =>
            {
                ProductizationCheckStatus::Pass
            }
            Ok(_) => ProductizationCheckStatus::Fail,
            Err(_) => ProductizationCheckStatus::Unknown,
        };
        let autostart_status = match (&state, autostart.read()) {
            (Err(_), _) => ProductizationCheckStatus::Unknown,
            (Ok(_), Ok(None)) => ProductizationCheckStatus::NotConfigured,
            (Ok(_), Ok(Some(entry))) if entry == installation.expected_autostart_entry() => {
                ProductizationCheckStatus::Pass
            }
            (Ok(_), Ok(Some(_))) | (Ok(_), Err(_)) => ProductizationCheckStatus::Fail,
        };
        let transaction = match updates.transaction() {
            Ok(None) => ProductizationCheckStatus::NotConfigured,
            Ok(Some(value))
                if matches!(
                    value.phase,
                    UpdatePhase::Committed
                        | UpdatePhase::RolledBack
                        | UpdatePhase::RecoveredRollback
                ) =>
            {
                ProductizationCheckStatus::Pass
            }
            Ok(Some(_)) => ProductizationCheckStatus::Unknown,
            Err(_) => ProductizationCheckStatus::Fail,
        };
        PackageDoctorReport {
            checks: vec![
                check("package-validity", package),
                check("installed-state-validity", installed),
                check("active-version-validity", active),
                check("autostart-ownership", autostart_status),
                check("update-transaction-state", transaction),
            ],
        }
    }
}

fn status_from<T>(result: Result<T, PackageError>) -> ProductizationCheckStatus {
    if result.is_ok() {
        ProductizationCheckStatus::Pass
    } else {
        ProductizationCheckStatus::Fail
    }
}

fn check(id: &'static str, status: ProductizationCheckStatus) -> ProductizationCheck {
    ProductizationCheck { id, status }
}

#[derive(Debug, Eq, PartialEq)]
pub enum PackageError {
    Io(String),
    InvalidProvenance(String),
    MalformedManifest(String),
    ChecksumMismatch(String),
    ForeignOutput(PathBuf),
    UnexpectedArchiveContents,
    ProvenanceMismatch {
        expected: Box<PackageProvenance>,
        actual: Box<PackageProvenance>,
    },
}

impl PackageError {
    fn io(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "package I/O failed: {error}"),
            Self::InvalidProvenance(error) => {
                write!(formatter, "invalid package provenance: {error}")
            }
            Self::MalformedManifest(error) => {
                write!(formatter, "malformed package manifest: {error}")
            }
            Self::ChecksumMismatch(name) => write!(formatter, "checksum mismatch for {name}"),
            Self::ForeignOutput(path) => write!(
                formatter,
                "refusing existing or foreign output {}",
                path.display()
            ),
            Self::UnexpectedArchiveContents => {
                formatter.write_str("unexpected, duplicate or unsafe archive contents")
            }
            Self::ProvenanceMismatch { .. } => formatter.write_str("package provenance mismatch"),
        }
    }
}

impl std::error::Error for PackageError {}

fn read_required_file(path: &Path) -> Result<Vec<u8>, PackageError> {
    validate_explicit_path(path).map_err(|_| PackageError::ForeignOutput(path.to_path_buf()))?;
    let metadata = fs::symlink_metadata(path).map_err(PackageError::io)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(path)
            .map_err(|error| PackageError::MalformedManifest(error.to_string()))?
    {
        return Err(PackageError::ForeignOutput(path.to_path_buf()));
    }
    fs::read(path).map_err(PackageError::io)
}

fn validate_provenance(provenance: &PackageProvenance) -> Result<(), PackageError> {
    validate_token("version", &provenance.version, 128)?;
    validate_token("channel", &provenance.channel, 64)?;
    if provenance.commit.len() != 40
        || !provenance
            .commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(PackageError::InvalidProvenance(
            "commit must be the exact lowercase 40-hex Git identity".to_owned(),
        ));
    }
    if provenance.target != WINDOWS_X64_TARGET {
        return Err(PackageError::InvalidProvenance(
            "target must be x86_64-pc-windows-msvc".to_owned(),
        ));
    }
    Ok(())
}

fn validate_token(name: &str, value: &str, maximum: usize) -> Result<(), PackageError> {
    if value.is_empty()
        || value.len() > maximum
        || value.starts_with('.')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(PackageError::InvalidProvenance(format!("invalid {name}")));
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), PackageError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(PackageError::MalformedManifest(
            "invalid SHA-256 digest".to_owned(),
        ));
    }
    Ok(())
}

fn expected_package_names() -> BTreeSet<String> {
    [MANIFEST_NAME, CHECKSUMS_NAME]
        .into_iter()
        .chain(RUNTIME_NAMES)
        .map(str::to_owned)
        .collect()
}

/// Checksums cover every meaningful package member except the checksum file
/// itself, which cannot hash itself without a cycle.
fn checksum_file(contents: &BTreeMap<String, Vec<u8>>) -> String {
    contents
        .iter()
        .map(|(name, bytes)| format!("{}  {name}", sha256_bytes(bytes)))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn verify_checksum_file(
    sums: &str,
    contents: &BTreeMap<String, Vec<u8>>,
) -> Result<(), PackageError> {
    let mut observed = BTreeMap::new();
    for line in sums.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or_else(|| PackageError::MalformedManifest("invalid SHA256SUMS line".to_owned()))?;
        validate_hash(hash)?;
        if observed.insert(name.to_owned(), hash.to_owned()).is_some() {
            return Err(PackageError::MalformedManifest(
                "duplicate SHA256SUMS entry".to_owned(),
            ));
        }
    }
    let expected = [MANIFEST_NAME]
        .into_iter()
        .chain(RUNTIME_NAMES)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if observed.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(PackageError::MalformedManifest(
            "SHA256SUMS coverage differs from package".to_owned(),
        ));
    }
    for (name, expected) in observed {
        let actual = contents
            .get(&name)
            .ok_or(PackageError::UnexpectedArchiveContents)?;
        if sha256_bytes(actual) != expected {
            return Err(PackageError::ChecksumMismatch(name));
        }
    }
    Ok(())
}

fn write_zip(path: &Path, contents: &BTreeMap<String, Vec<u8>>) -> Result<(), PackageError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(PackageError::io)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, bytes) in contents {
        writer
            .start_file(name, options)
            .map_err(|error| PackageError::MalformedManifest(error.to_string()))?;
        writer.write_all(bytes).map_err(PackageError::io)?;
    }
    writer
        .finish()
        .map_err(|error| PackageError::MalformedManifest(error.to_string()))?;
    Ok(())
}

fn read_verified_archive(archive_path: &Path) -> Result<VerifiedArchive, PackageError> {
    let _archive_guards = guard_existing_directories(archive_path)
        .map_err(|_| PackageError::ForeignOutput(archive_path.to_path_buf()))?;
    validate_explicit_path(archive_path)
        .map_err(|_| PackageError::ForeignOutput(archive_path.to_path_buf()))?;
    let metadata = fs::symlink_metadata(archive_path).map_err(PackageError::io)?;
    if !metadata.is_file()
        || metadata.len() > MAX_PACKAGE_BYTES
        || is_reparse_point(archive_path)
            .map_err(|error| PackageError::MalformedManifest(error.to_string()))?
    {
        return Err(PackageError::ForeignOutput(archive_path.to_path_buf()));
    }
    let bytes = fs::read(archive_path).map_err(PackageError::io)?;
    let archive_sha256 = sha256_bytes(&bytes);
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| PackageError::MalformedManifest(error.to_string()))?;
    let mut contents = BTreeMap::new();
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| PackageError::MalformedManifest(error.to_string()))?;
        let name = entry.name().to_owned();
        let path = Path::new(&name);
        if name.contains('\\')
            || path.components().count() != 1
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            || entry.is_dir()
            || contents.contains_key(&name)
        {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        if entry.size() > MAX_MEMBER_BYTES {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(MAX_MEMBER_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(PackageError::io)?;
        if bytes.len() as u64 > MAX_MEMBER_BYTES {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        total_bytes = total_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(PackageError::UnexpectedArchiveContents)?;
        if total_bytes > MAX_PACKAGE_BYTES {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        contents.insert(name, bytes);
    }
    let manifest = PackageVerifier::verify_contents(&contents)?;
    Ok(VerifiedArchive {
        manifest,
        contents,
        archive_sha256,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        io::{Seek, Write},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::{PackageError, RUNTIME_NAMES, WINDOWS_X64_TARGET};
    use crate::{
        payload_sha256, HealthObservation, Installation, PackageDoctor, PackageInput,
        PackageProvenance, PackageVerifier, ProductizationCheckStatus, SafePointObservation,
        SandboxAutostartBackend, UpdateCoordinator, UpdateOutcome, UpdateRequest,
    };

    fn root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "runnermesh-package-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn candidate(root: &Path, version: &str, marker: &str) -> PackageInput {
        let source = root.join(format!("source-{version}-{marker}"));
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("runnermesh.exe"), format!("cli-{marker}")).unwrap();
        fs::write(
            source.join("runnermesh-agent.exe"),
            format!("agent-{marker}"),
        )
        .unwrap();
        fs::write(
            source.join("runnermesh-agent.manifest"),
            format!("manifest-{marker}"),
        )
        .unwrap();
        PackageInput {
            provenance: PackageProvenance {
                version: version.to_owned(),
                commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
                channel: "sandbox".to_owned(),
                target: WINDOWS_X64_TARGET.to_owned(),
            },
            cli_binary: source.join("runnermesh.exe"),
            agent_binary: source.join("runnermesh-agent.exe"),
            agent_manifest: source.join("runnermesh-agent.manifest"),
        }
    }

    fn archive_entries(archive: &Path) -> BTreeMap<String, Vec<u8>> {
        super::read_verified_archive(archive).unwrap().contents
    }

    #[test]
    fn creates_verifies_and_binds_provenance() {
        let root = root("create");
        let input = candidate(&root, "0.1.0-rc.1", "one");
        let receipt = PackageVerifier::create(&input, &root.join("archives")).unwrap();
        assert_eq!(
            PackageVerifier::verify(&receipt.archive).unwrap(),
            receipt.manifest
        );
        assert_eq!(
            PackageVerifier::archive_sha256(&receipt.archive).unwrap(),
            receipt.archive_sha256
        );
        let (verified_manifest, verified_hash) =
            PackageVerifier::verify_with_hash(&receipt.archive).unwrap();
        assert_eq!(verified_manifest, receipt.manifest);
        assert_eq!(verified_hash, receipt.archive_sha256);
        let mut wrong = input.provenance.clone();
        wrong.channel = "other".to_owned();
        assert!(matches!(
            PackageVerifier::verify_expected(&receipt.archive, &wrong),
            Err(PackageError::ProvenanceMismatch { .. })
        ));

        let mut abbreviated = candidate(&root, "0.1.0", "short-commit");
        abbreviated.provenance.commit = "0123456".to_owned();
        assert!(matches!(
            PackageVerifier::create(&abbreviated, &root.join("short-commit-output")),
            Err(PackageError::InvalidProvenance(_))
        ));
        let mut uppercase = candidate(&root, "0.1.0", "uppercase-commit");
        uppercase.provenance.commit = "0123456789ABCDEF0123456789ABCDEF01234567".to_owned();
        assert!(matches!(
            PackageVerifier::create(&uppercase, &root.join("uppercase-commit-output")),
            Err(PackageError::InvalidProvenance(_))
        ));
    }

    #[test]
    fn rejects_tamper_extra_traversal_and_duplicate_entries() {
        let root = root("bad-archive");
        let receipt =
            PackageVerifier::create(&candidate(&root, "0.1.0", "one"), &root.join("archives"))
                .unwrap();
        let mut tampered = archive_entries(&receipt.archive);
        tampered.insert("runnermesh.exe".to_owned(), b"tampered".to_vec());
        let tampered_path = root.join("tampered.zip");
        super::write_zip(&tampered_path, &tampered).unwrap();
        assert!(matches!(
            PackageVerifier::verify(&tampered_path),
            Err(PackageError::ChecksumMismatch(_))
        ));

        let mut extra = archive_entries(&receipt.archive);
        extra.insert("extra.exe".to_owned(), b"no".to_vec());
        let extra_path = root.join("extra.zip");
        super::write_zip(&extra_path, &extra).unwrap();
        assert_eq!(
            PackageVerifier::verify(&extra_path),
            Err(PackageError::UnexpectedArchiveContents)
        );

        let traversal = root.join("traversal.zip");
        let mut zip = ZipWriter::new(fs::File::create(&traversal).unwrap());
        zip.start_file("../escape.exe", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert_eq!(
            PackageVerifier::verify(&traversal),
            Err(PackageError::UnexpectedArchiveContents)
        );

        let duplicate = root.join("duplicate.zip");
        write_duplicate_stored_zip(&duplicate, "runnermesh.exe", b"duplicate");
        assert_eq!(
            PackageVerifier::verify(&duplicate),
            Err(PackageError::UnexpectedArchiveContents)
        );

        let existing_empty = root.join("existing-empty");
        fs::create_dir(&existing_empty).unwrap();
        assert!(matches!(
            PackageVerifier::extract_runtime(&receipt.archive, &existing_empty),
            Err(PackageError::ForeignOutput(_))
        ));
        assert!(matches!(
            PackageVerifier::extract_runtime(
                &receipt.archive,
                &root.join("parent").join("..").join("escaped")
            ),
            Err(PackageError::ForeignOutput(_))
        ));
    }

    #[test]
    fn package_to_sandbox_install_update_rollback_doctor_and_uninstall() {
        let root = root("cross");
        let first = PackageVerifier::create(
            &candidate(&root, "0.1.0", "one"),
            &root.join("archives-one"),
        )
        .unwrap();
        let first_payload = root.join("payload-one");
        PackageVerifier::extract_runtime(&first.archive, &first_payload).unwrap();
        let installation = Installation::new(root.join("installed"));
        installation.install("0.1.0", &first_payload).unwrap();
        let autostart = SandboxAutostartBackend::new(root.join("autostart.json"));
        installation.enable_autostart(&autostart).unwrap();

        let second = PackageVerifier::create(
            &candidate(&root, "0.2.0", "two"),
            &root.join("archives-two"),
        )
        .unwrap();
        let second_payload = root.join("payload-two");
        PackageVerifier::extract_runtime(&second.archive, &second_payload).unwrap();
        let updates = UpdateCoordinator::new(installation.clone());
        updates
            .stage(&UpdateRequest::new(
                "0.2.0",
                &second_payload,
                payload_sha256(&second_payload).unwrap(),
                true,
            ))
            .unwrap();
        assert_eq!(
            updates
                .activate(SafePointObservation::Idle, HealthObservation::Healthy)
                .unwrap()
                .outcome,
            UpdateOutcome::Committed
        );

        let third = PackageVerifier::create(
            &candidate(&root, "0.3.0", "three"),
            &root.join("archives-three"),
        )
        .unwrap();
        let third_payload = root.join("payload-three");
        PackageVerifier::extract_runtime(&third.archive, &third_payload).unwrap();
        updates
            .stage(&UpdateRequest::new(
                "0.3.0",
                &third_payload,
                payload_sha256(&third_payload).unwrap(),
                true,
            ))
            .unwrap();
        assert_eq!(
            updates
                .activate(
                    SafePointObservation::Idle,
                    HealthObservation::Unhealthy("fixture".to_owned())
                )
                .unwrap()
                .outcome,
            UpdateOutcome::RolledBack
        );

        let report = PackageDoctor::inspect(&second.archive, &installation, &updates, &autostart);
        assert!(report
            .checks
            .iter()
            .all(|check| check.status == ProductizationCheckStatus::Pass));
        updates.clear_terminal_journal().unwrap();
        let uninstall = installation.uninstall(&autostart).unwrap();
        assert!(uninstall.root_removed);
    }

    #[test]
    fn malformed_manifest_and_missing_file_are_rejected() {
        let root = root("manifest");
        let receipt =
            PackageVerifier::create(&candidate(&root, "0.1.0", "one"), &root.join("archives"))
                .unwrap();
        let mut malformed = archive_entries(&receipt.archive);
        malformed.insert("PACKAGE-MANIFEST.json".to_owned(), b"{}".to_vec());
        let path = root.join("malformed.zip");
        super::write_zip(&path, &malformed).unwrap();
        assert!(matches!(
            PackageVerifier::verify(&path),
            Err(PackageError::MalformedManifest(_))
        ));

        let mut missing = archive_entries(&receipt.archive);
        missing.remove(RUNTIME_NAMES[0]);
        let path = root.join("missing.zip");
        super::write_zip(&path, &missing).unwrap();
        assert_eq!(
            PackageVerifier::verify(&path),
            Err(PackageError::UnexpectedArchiveContents)
        );
    }

    fn write_duplicate_stored_zip(path: &Path, name: &str, bytes: &[u8]) {
        let mut file = fs::File::create(path).unwrap();
        let mut local_offsets = Vec::new();
        for _ in 0..2 {
            local_offsets.push(file.stream_position().unwrap() as u32);
            write_u32(&mut file, 0x0403_4b50);
            write_u16(&mut file, 20);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u32(&mut file, crc32(bytes));
            write_u32(&mut file, bytes.len() as u32);
            write_u32(&mut file, bytes.len() as u32);
            write_u16(&mut file, name.len() as u16);
            write_u16(&mut file, 0);
            file.write_all(name.as_bytes()).unwrap();
            file.write_all(bytes).unwrap();
        }
        let central_offset = file.stream_position().unwrap() as u32;
        for offset in local_offsets {
            write_u32(&mut file, 0x0201_4b50);
            write_u16(&mut file, 20);
            write_u16(&mut file, 20);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u32(&mut file, crc32(bytes));
            write_u32(&mut file, bytes.len() as u32);
            write_u32(&mut file, bytes.len() as u32);
            write_u16(&mut file, name.len() as u16);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u16(&mut file, 0);
            write_u32(&mut file, 0);
            write_u32(&mut file, offset);
            file.write_all(name.as_bytes()).unwrap();
        }
        let central_end = file.stream_position().unwrap() as u32;
        write_u32(&mut file, 0x0605_4b50);
        write_u16(&mut file, 0);
        write_u16(&mut file, 0);
        write_u16(&mut file, 2);
        write_u16(&mut file, 2);
        write_u32(&mut file, central_end - central_offset);
        write_u32(&mut file, central_offset);
        write_u16(&mut file, 0);
    }

    fn write_u16(file: &mut fs::File, value: u16) {
        file.write_all(&value.to_le_bytes()).unwrap();
    }

    fn write_u32(file: &mut fs::File, value: u32) {
        file.write_all(&value.to_le_bytes()).unwrap();
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut value = 0xffff_ffff_u32;
        for byte in bytes {
            value ^= u32::from(*byte);
            for _ in 0..8 {
                value = if value & 1 != 0 {
                    (value >> 1) ^ 0xedb8_8320
                } else {
                    value >> 1
                };
            }
        }
        !value
    }
}
