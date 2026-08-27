//! Content-checked Windows x64 release-candidate packages and diagnostics.
//!
//! These helpers operate on explicit files and caller-owned sandbox roots.
//! They do not select a production installation, activate autostart, launch
//! an Agent, or publish a release.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    AutostartBackend, AutostartEntry, DoctorCheck, DoctorReport, DoctorStatus, Installation,
    ReasonCode, UpdateCoordinator, UpdatePhase,
};

const PACKAGE_SCHEMA_VERSION: u32 = 1;
const PACKAGE_TARGET: &str = "x86_64-pc-windows-msvc";
const PACKAGE_FILES: [&str; 5] = [
    "PACKAGE-MANIFEST.json",
    "SHA256SUMS",
    "runnermesh-agent.exe",
    "runnermesh-agent.manifest",
    "runnermesh.exe",
];
const AUTOSTART_OWNER: &str = "runnermesh-v01";

/// Fixed release candidate identity, kept separate from mutable installation
/// state and from the source checkout.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackageProvenance {
    pub version: String,
    pub commit: String,
    pub channel: String,
    pub target: String,
}

/// Inputs copied into a candidate archive. All inputs must be explicit files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInput {
    pub provenance: PackageProvenance,
    pub cli_binary: PathBuf,
    pub agent_binary: PathBuf,
    pub agent_manifest: PathBuf,
}

/// The archive's non-cyclic content manifest. `files` covers exactly the two
/// binaries and manifest; `SHA256SUMS` then also covers this manifest file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub provenance: PackageProvenance,
    pub files: BTreeMap<String, String>,
    pub common_controls_v6: bool,
    pub per_monitor_v2: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageReceipt {
    pub archive: PathBuf,
    pub archive_sha256: String,
    pub manifest: PackageManifest,
}

/// Signals gathered by an actual caller before rendering the package-aware
/// doctor report. The package code never invents IPC, probe, runner, or work
/// root facts on its own.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDoctorInputs {
    pub agent: DoctorStatus,
    pub ipc: DoctorStatus,
    pub probes: DoctorStatus,
    pub runner_discovery: DoctorStatus,
    pub work_root_safety: DoctorStatus,
    pub source_tree: PathBuf,
}

pub struct PackageVerifier;

impl PackageVerifier {
    /// Builds an immutable zip from explicit candidate bytes. Existing output
    /// files are refused to prevent a silent artifact overwrite.
    pub fn create(
        input: &PackageInput,
        output_directory: &Path,
    ) -> Result<PackageReceipt, PackageError> {
        validate_provenance(&input.provenance)?;
        let cli = read_required_file(&input.cli_binary, "runnermesh.exe")?;
        let agent = read_required_file(&input.agent_binary, "runnermesh-agent.exe")?;
        let agent_manifest =
            read_required_file(&input.agent_manifest, "runnermesh-agent.manifest")?;
        let manifest_text = std::str::from_utf8(&agent_manifest)
            .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        let common_controls_v6 = manifest_text.contains("Microsoft.Windows.Common-Controls")
            && manifest_text.contains("version=\"6.0.0.0\"");
        let per_monitor_v2 = manifest_text.contains("PerMonitorV2");
        if !common_controls_v6 || !per_monitor_v2 {
            return Err(PackageError::RequiredManifestCapabilityMissing);
        }

        let mut payloads = BTreeMap::new();
        payloads.insert("runnermesh.exe".to_owned(), cli);
        payloads.insert("runnermesh-agent.exe".to_owned(), agent);
        payloads.insert("runnermesh-agent.manifest".to_owned(), agent_manifest);
        let files = payloads
            .iter()
            .map(|(name, bytes)| (name.clone(), sha256_bytes(bytes)))
            .collect();
        let manifest = PackageManifest {
            schema_version: PACKAGE_SCHEMA_VERSION,
            provenance: input.provenance.clone(),
            files,
            common_controls_v6,
            per_monitor_v2,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        payloads.insert("PACKAGE-MANIFEST.json".to_owned(), manifest_bytes);
        let sums = checksum_file(&payloads);
        payloads.insert("SHA256SUMS".to_owned(), sums.into_bytes());

        fs::create_dir_all(output_directory).map_err(PackageError::io)?;
        let archive = output_directory.join(format!(
            "runnermesh-{}-windows-x64.zip",
            input.provenance.version
        ));
        if archive.exists() {
            return Err(PackageError::ForeignOutput(archive));
        }
        write_zip(&archive, &payloads)?;
        let receipt = PackageReceipt {
            archive: archive.clone(),
            archive_sha256: sha256_file(&archive)?,
            manifest,
        };
        Self::verify(&archive)?;
        Ok(receipt)
    }

    /// Validates every archive member, the semantic package manifest, its
    /// checksums, and the required Windows UI manifest capabilities.
    pub fn verify(archive: &Path) -> Result<PackageManifest, PackageError> {
        let contents = read_zip_contents(archive)?;
        let names = contents.keys().cloned().collect::<BTreeSet<_>>();
        let expected = PACKAGE_FILES.into_iter().map(str::to_owned).collect();
        if names != expected {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        let manifest_bytes = contents
            .get("PACKAGE-MANIFEST.json")
            .ok_or(PackageError::UnexpectedArchiveContents)?;
        let manifest = serde_json::from_slice::<PackageManifest>(manifest_bytes)
            .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        if manifest.schema_version != PACKAGE_SCHEMA_VERSION {
            return Err(PackageError::InvalidManifest(
                "unsupported package schema".to_owned(),
            ));
        }
        validate_provenance(&manifest.provenance)?;
        let expected_payload_names = [
            "runnermesh.exe",
            "runnermesh-agent.exe",
            "runnermesh-agent.manifest",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        if manifest.files.keys().cloned().collect::<BTreeSet<_>>() != expected_payload_names {
            return Err(PackageError::InvalidManifest(
                "package manifest does not cover the exact payload set".to_owned(),
            ));
        }
        for (name, expected_hash) in &manifest.files {
            let actual = contents
                .get(name)
                .ok_or(PackageError::UnexpectedArchiveContents)?;
            if sha256_bytes(actual) != *expected_hash {
                return Err(PackageError::ChecksumMismatch(name.clone()));
            }
        }
        let sums = std::str::from_utf8(
            contents
                .get("SHA256SUMS")
                .ok_or(PackageError::UnexpectedArchiveContents)?,
        )
        .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        verify_checksum_file(sums, &contents)?;

        let agent_manifest = std::str::from_utf8(
            contents
                .get("runnermesh-agent.manifest")
                .ok_or(PackageError::UnexpectedArchiveContents)?,
        )
        .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        if !manifest.common_controls_v6
            || !agent_manifest.contains("Microsoft.Windows.Common-Controls")
            || !agent_manifest.contains("version=\"6.0.0.0\"")
            || !manifest.per_monitor_v2
            || !agent_manifest.contains("PerMonitorV2")
        {
            return Err(PackageError::RequiredManifestCapabilityMissing);
        }
        Ok(manifest)
    }

    /// Unpacks only a package that has first passed full archive verification.
    /// The destination must be absent or empty and no archive-controlled path
    /// traversal is accepted.
    pub fn unpack(archive: &Path, destination: &Path) -> Result<PackageManifest, PackageError> {
        let manifest = Self::verify(archive)?;
        if destination.exists()
            && fs::read_dir(destination)
                .map_err(PackageError::io)?
                .next()
                .is_some()
        {
            return Err(PackageError::ForeignOutput(destination.to_path_buf()));
        }
        fs::create_dir_all(destination).map_err(PackageError::io)?;
        let contents = read_zip_contents(archive)?;
        for (name, bytes) in contents {
            let path = destination.join(&name);
            if path.parent() != Some(destination) {
                return Err(PackageError::UnexpectedArchiveContents);
            }
            fs::write(path, bytes).map_err(PackageError::io)?;
        }
        Ok(manifest)
    }
}

pub struct PackageDoctor;

impl PackageDoctor {
    /// Produces package and install diagnostics from verified package bytes,
    /// explicit external observations, and caller-owned install metadata.
    pub fn inspect(
        archive: &Path,
        installation: &Installation,
        updates: &UpdateCoordinator,
        autostart: &impl AutostartBackend,
        inputs: &PackageDoctorInputs,
    ) -> Result<DoctorReport, PackageError> {
        let _manifest = PackageVerifier::verify(archive)?;
        let installation_state = installation.state().map_err(PackageError::installation)?;
        let active_version = installation_state
            .active_version
            .clone()
            .ok_or(PackageError::NoActiveVersion)?;
        let active_path = installation
            .active_agent_path()
            .map_err(PackageError::installation)?;
        let expected_autostart = AutostartEntry {
            owner: AUTOSTART_OWNER.to_owned(),
            command: format!("\"{}\"", installation.stable_agent_entry().display()),
        };
        let autostart_status = match autostart.read().map_err(PackageError::io)? {
            None => DoctorStatus::Pass,
            Some(actual) if actual == expected_autostart => DoctorStatus::Pass,
            Some(_) => DoctorStatus::Fail,
        };
        let transaction_status = match updates.transaction().map_err(PackageError::update)? {
            None => DoctorStatus::Pass,
            Some(transaction)
                if matches!(
                    transaction.phase,
                    UpdatePhase::Committed | UpdatePhase::RolledBack
                ) =>
            {
                DoctorStatus::Pass
            }
            Some(_) => DoctorStatus::Warn,
        };
        let source_runtime_isolated = path_isolated(&inputs.source_tree, installation.root())?;
        let check = |id, status| DoctorCheck {
            id: reason(id),
            status,
            reason_code: (status != DoctorStatus::Pass).then(|| reason("package-doctor-warning")),
        };

        Ok(DoctorReport {
            checks: vec![
                check("package-agent", inputs.agent),
                check("package-ipc", inputs.ipc),
                check("package-probes", inputs.probes),
                check("package-runner-discovery", inputs.runner_discovery),
                check("package-work-root-safety", inputs.work_root_safety),
                check("package-autostart", autostart_status),
                check(
                    "package-installed-versions",
                    if !installation_state.versions.is_empty() {
                        DoctorStatus::Pass
                    } else {
                        DoctorStatus::Fail
                    },
                ),
                check(
                    "package-active-version",
                    if active_path.ends_with(
                        Path::new("versions")
                            .join(&active_version)
                            .join("runnermesh-agent.exe"),
                    ) {
                        DoctorStatus::Pass
                    } else {
                        DoctorStatus::Fail
                    },
                ),
                check("package-transaction-state", transaction_status),
                check("package-provenance", DoctorStatus::Pass),
                check(
                    "package-source-runtime-isolation",
                    if source_runtime_isolated {
                        DoctorStatus::Pass
                    } else {
                        DoctorStatus::Fail
                    },
                ),
            ],
        })
    }
}

#[derive(Debug)]
pub enum PackageError {
    Io(String),
    Installation(String),
    Update(String),
    InvalidProvenance(String),
    InvalidManifest(String),
    RequiredManifestCapabilityMissing,
    ChecksumMismatch(String),
    ForeignOutput(PathBuf),
    UnexpectedArchiveContents,
    NoActiveVersion,
}

impl PackageError {
    fn io(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }

    fn installation(error: impl fmt::Display) -> Self {
        Self::Installation(error.to_string())
    }

    fn update(error: impl fmt::Display) -> Self {
        Self::Update(error.to_string())
    }
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "package I/O failed: {error}"),
            Self::Installation(error) => {
                write!(formatter, "package install inspection failed: {error}")
            }
            Self::Update(error) => write!(formatter, "package update inspection failed: {error}"),
            Self::InvalidProvenance(error) => {
                write!(formatter, "invalid package provenance: {error}")
            }
            Self::InvalidManifest(error) => write!(formatter, "invalid package manifest: {error}"),
            Self::RequiredManifestCapabilityMissing => {
                formatter.write_str("agent manifest lacks required Windows UI capabilities")
            }
            Self::ChecksumMismatch(name) => write!(formatter, "package checksum mismatch: {name}"),
            Self::ForeignOutput(path) => write!(
                formatter,
                "refusing to overwrite foreign output: {}",
                path.display()
            ),
            Self::UnexpectedArchiveContents => {
                formatter.write_str("package contents are not the exact expected set")
            }
            Self::NoActiveVersion => {
                formatter.write_str("package doctor requires an active installed version")
            }
        }
    }
}

impl std::error::Error for PackageError {}

fn validate_provenance(provenance: &PackageProvenance) -> Result<(), PackageError> {
    if provenance.target != PACKAGE_TARGET {
        return Err(PackageError::InvalidProvenance(format!(
            "target must be {PACKAGE_TARGET}"
        )));
    }
    for (name, value) in [
        ("version", &provenance.version),
        ("commit", &provenance.commit),
        ("channel", &provenance.channel),
    ] {
        if value.is_empty()
            || value.len() > 128
            || value.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+'))
            })
        {
            return Err(PackageError::InvalidProvenance(name.to_owned()));
        }
    }
    Ok(())
}

fn read_required_file(path: &Path, expected: &str) -> Result<Vec<u8>, PackageError> {
    if path.file_name().and_then(|name| name.to_str()) != Some(expected) {
        return Err(PackageError::InvalidManifest(format!(
            "expected input file name {expected}"
        )));
    }
    let bytes = fs::read(path).map_err(PackageError::io)?;
    if bytes.is_empty() {
        return Err(PackageError::InvalidManifest(format!(
            "input is empty: {expected}"
        )));
    }
    Ok(bytes)
}

fn checksum_file(payloads: &BTreeMap<String, Vec<u8>>) -> String {
    payloads
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
            .ok_or_else(|| PackageError::InvalidManifest("invalid SHA256SUMS line".to_owned()))?;
        observed.insert(name.to_owned(), hash.to_owned());
    }
    let expected_names = [
        "PACKAGE-MANIFEST.json",
        "runnermesh.exe",
        "runnermesh-agent.exe",
        "runnermesh-agent.manifest",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    if observed.keys().cloned().collect::<BTreeSet<_>>() != expected_names {
        return Err(PackageError::InvalidManifest(
            "SHA256SUMS does not cover the exact content set".to_owned(),
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
    let file = File::create(path).map_err(PackageError::io)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, bytes) in contents {
        writer
            .start_file(name, options)
            .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        writer.write_all(bytes).map_err(PackageError::io)?;
    }
    writer
        .finish()
        .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
    Ok(())
}

fn read_zip_contents(archive_path: &Path) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
    let file = File::open(archive_path).map_err(PackageError::io)?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
    let mut contents = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
        let name = entry.name().to_owned();
        let candidate = Path::new(&name);
        if candidate.components().count() != 1
            || candidate
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            || contents.contains_key(&name)
        {
            return Err(PackageError::UnexpectedArchiveContents);
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(PackageError::io)?;
        contents.insert(name, bytes);
    }
    Ok(contents)
}

fn path_isolated(source: &Path, runtime: &Path) -> Result<bool, PackageError> {
    let source = fs::canonicalize(source).map_err(PackageError::io)?;
    let runtime = fs::canonicalize(runtime).map_err(PackageError::io)?;
    Ok(!source.starts_with(&runtime) && !runtime.starts_with(&source))
}

fn reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("package doctor IDs are stable reason codes")
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:X}", hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<String, PackageError> {
    let mut file = File::open(path).map_err(PackageError::io)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(PackageError::io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        payload_sha256, DoctorStatus, HealthObservation, Installation, PackageDoctor,
        PackageDoctorInputs, PackageError, PackageInput, PackageProvenance, PackageVerifier,
        SandboxAutostartBackend, UpdateCoordinator, UpdateOutcome, UpdateRequest,
    };

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "runnermesh-package-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_candidate(source: &Path, version: &str, marker: &str) -> PackageInput {
        fs::create_dir_all(source).unwrap();
        let cli = source.join("runnermesh.exe");
        let agent = source.join("runnermesh-agent.exe");
        let manifest = source.join("runnermesh-agent.manifest");
        fs::write(&cli, format!("fixture-cli-{marker}")).unwrap();
        fs::write(&agent, format!("fixture-agent-{marker}")).unwrap();
        fs::write(
            &manifest,
            r#"<assembly><dependency><assemblyIdentity name="Microsoft.Windows.Common-Controls" version="6.0.0.0" /></dependency><dpiAwareness>PerMonitorV2, PerMonitor</dpiAwareness></assembly>"#,
        )
        .unwrap();
        PackageInput {
            provenance: PackageProvenance {
                version: version.to_owned(),
                commit: "0123456789abcdef".to_owned(),
                channel: "rc-dry-run".to_owned(),
                target: "x86_64-pc-windows-msvc".to_owned(),
            },
            cli_binary: cli,
            agent_binary: agent,
            agent_manifest: manifest,
        }
    }

    fn unpack_candidate(root: &Path, version: &str, marker: &str) -> (PathBuf, PathBuf) {
        let source = root.join(format!("source-{version}"));
        let input = write_candidate(&source, version, marker);
        let receipt = PackageVerifier::create(&input, &root.join("archives")).unwrap();
        assert_eq!(
            PackageVerifier::verify(&receipt.archive).unwrap(),
            receipt.manifest
        );
        let unpacked = root.join(format!("unpacked-{version}"));
        PackageVerifier::unpack(&receipt.archive, &unpacked).unwrap();
        fs::remove_dir_all(source).unwrap();
        (receipt.archive, unpacked)
    }

    #[test]
    fn package_bytes_support_sandbox_install_update_rollback_doctor_and_uninstall() {
        let root = temp_root("workflow");
        let source_tree = root.join("source-tree");
        fs::create_dir_all(&source_tree).unwrap();
        let (v1_archive, v1_payload) = unpack_candidate(&root, "0.1.0-rc.1", "one");
        let installation = Installation::new(root.join("installed"));
        installation.install("0.1.0", &v1_payload).unwrap();
        let autostart = SandboxAutostartBackend::new(root.join("autostart.json"));
        assert!(!installation.install_autostart(&autostart).unwrap());

        let (v2_archive, v2_payload) = unpack_candidate(&root, "0.2.0-rc.1", "two");
        let updates = UpdateCoordinator::new(installation.clone());
        let v2 = UpdateRequest::new("0.2.0", &v2_payload, payload_sha256(&v2_payload).unwrap());
        updates.stage(&v2).unwrap();
        assert_eq!(
            updates
                .activate(false, HealthObservation::Healthy)
                .unwrap()
                .outcome,
            UpdateOutcome::Committed
        );

        let (_v3_archive, v3_payload) = unpack_candidate(&root, "0.3.0-rc.1", "three");
        let v3 = UpdateRequest::new("0.3.0", &v3_payload, payload_sha256(&v3_payload).unwrap());
        updates.stage(&v3).unwrap();
        assert_eq!(
            updates
                .activate(
                    false,
                    HealthObservation::Unhealthy("sandbox health failure".to_owned()),
                )
                .unwrap()
                .outcome,
            UpdateOutcome::RolledBack
        );
        assert_eq!(
            installation.state().unwrap().active_version.as_deref(),
            Some("0.2.0")
        );

        let doctor = PackageDoctor::inspect(
            &v2_archive,
            &installation,
            &updates,
            &autostart,
            &PackageDoctorInputs {
                agent: DoctorStatus::Pass,
                ipc: DoctorStatus::Pass,
                probes: DoctorStatus::Pass,
                runner_discovery: DoctorStatus::Pass,
                work_root_safety: DoctorStatus::Pass,
                source_tree: source_tree.clone(),
            },
        )
        .unwrap();
        let ids = doctor
            .checks
            .iter()
            .map(|check| check.id.as_str())
            .collect::<Vec<_>>();
        for required in [
            "package-agent",
            "package-ipc",
            "package-probes",
            "package-runner-discovery",
            "package-work-root-safety",
            "package-autostart",
            "package-installed-versions",
            "package-active-version",
            "package-transaction-state",
            "package-provenance",
            "package-source-runtime-isolation",
        ] {
            assert!(ids.contains(&required));
        }
        assert!(updates.clear_terminal_journal().unwrap());
        let uninstall = installation.uninstall(&autostart).unwrap();
        assert!(uninstall.root_removed);
        assert!(v1_archive.is_file());
        assert!(v2_archive.is_file());
    }

    #[test]
    fn package_refuses_foreign_output_and_bad_windows_manifest() {
        let root = temp_root("refusal");
        let source = root.join("source");
        let input = write_candidate(&source, "0.1.0-rc.1", "one");
        let first = PackageVerifier::create(&input, &root.join("archives")).unwrap();
        assert!(matches!(
            PackageVerifier::create(&input, &root.join("archives")),
            Err(PackageError::ForeignOutput(_))
        ));
        assert!(first.archive.is_file());

        fs::write(&input.agent_manifest, "<assembly />").unwrap();
        assert!(matches!(
            PackageVerifier::create(&input, &root.join("second-archives")),
            Err(PackageError::RequiredManifestCapabilityMissing)
        ));
    }

    #[test]
    fn package_doctor_refuses_source_runtime_overlap() {
        let root = temp_root("overlap");
        let (archive, payload) = unpack_candidate(&root, "0.1.0-rc.1", "one");
        let installation = Installation::new(root.join("installed"));
        installation.install("0.1.0", &payload).unwrap();
        let autostart = SandboxAutostartBackend::new(root.join("autostart.json"));
        let updates = UpdateCoordinator::new(installation.clone());
        let report = PackageDoctor::inspect(
            &archive,
            &installation,
            &updates,
            &autostart,
            &PackageDoctorInputs {
                agent: DoctorStatus::Pass,
                ipc: DoctorStatus::Pass,
                probes: DoctorStatus::Pass,
                runner_discovery: DoctorStatus::Pass,
                work_root_safety: DoctorStatus::Pass,
                source_tree: installation.root().to_path_buf(),
            },
        )
        .unwrap();
        assert_eq!(
            report
                .checks
                .iter()
                .find(|check| check.id.as_str() == "package-source-runtime-isolation")
                .unwrap()
                .status,
            DoctorStatus::Fail
        );
    }
}
