//! Immutable package verification and narrowly bounded user-install operator helper.

use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use runnermesh::{
    AgentCommand, AgentResponse, AgentTransport, Installation, InstalledRuntimeBinding, IpcRequest,
    IpcResponseBody, LocalAgentTransport, PackageInput, PackageProvenance, PackageVerifier,
    IPC_PROTOCOL_VERSION, WINDOWS_X64_TARGET,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[cfg(windows)]
use runnermesh::WindowsUserStartupBackend;

fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("runnermesh-package: {error}");
            process::exit(2);
        }
    }
}

fn run(arguments: &[OsString]) -> Result<Value, String> {
    match arguments {
        [command] if command == "identity" => Ok(json!({
            "result": "identity",
            "provenance": operator_provenance(),
        })),
        [command, archive] if command == "verify" => {
            PackageVerifier::verify_with_hash(&PathBuf::from(archive))
                .map(|(manifest, archive_sha256)| {
                    json!({
                        "result": "verified",
                        "archive_sha256": archive_sha256,
                        "provenance": manifest.provenance,
                    })
                })
                .map_err(|error| error.to_string())
        }
        [command, archive, destination] if command == "extract" => {
            let destination = PathBuf::from(destination);
            PackageVerifier::extract_runtime(&PathBuf::from(archive), &destination)
                .map(|manifest| {
                    json!({
                        "result": "extracted",
                        "provenance": manifest.provenance,
                    })
                })
                .map_err(|error| error.to_string())
        }
        [command, archive, expected_hash, staging, install_root] if command == "install" => {
            install_accepted_archive(archive, expected_hash, staging, install_root)
        }
        [command, install_root, binding_file, expected_hash] if command == "bind" => {
            bind_runtime(install_root, binding_file, expected_hash)
        }
        [command, action, install_root] if command == "autostart" => {
            configure_autostart(action, install_root)
        }
        [command] if command == "stop-after-drain" => request_stop_after_drain(),
        [command, install_root] if command == "uninstall" => uninstall(install_root),
        [command, output, version, commit, channel, cli, agent, manifest]
            if command == "create" =>
        {
            PackageVerifier::create(
                &PackageInput {
                    provenance: PackageProvenance {
                        version: token(version, "version")?,
                        commit: token(commit, "commit")?,
                        channel: token(channel, "channel")?,
                        target: WINDOWS_X64_TARGET.to_owned(),
                    },
                    cli_binary: PathBuf::from(cli),
                    agent_binary: PathBuf::from(agent),
                    agent_manifest: PathBuf::from(manifest),
                },
                &PathBuf::from(output),
            )
            .map(|receipt| {
                json!({
                    "result": "created",
                    "archive": receipt.archive,
                    "archive_sha256": receipt.archive_sha256,
                    "provenance": receipt.manifest.provenance,
                })
            })
            .map_err(|error| error.to_string())
        }
        _ => Err(usage().to_owned()),
    }
}

fn install_accepted_archive(
    archive: &OsStr,
    expected_hash: &OsStr,
    staging: &OsStr,
    install_root: &OsStr,
) -> Result<Value, String> {
    let expected_hash = exact_sha256(expected_hash)?;
    let provenance = accepted_operator_provenance()?;
    let staging = PathBuf::from(staging);
    let manifest = PackageVerifier::extract_runtime_expected(
        &PathBuf::from(archive),
        &staging,
        &provenance,
        &expected_hash,
    )
    .map_err(|error| error.to_string())?;
    let installation = Installation::new(PathBuf::from(install_root));
    let receipt = installation
        .install(&manifest.provenance.version, &staging)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "result": "installed",
        "archive_sha256": expected_hash,
        "provenance": manifest.provenance,
        "version": receipt.version,
        "idempotent": receipt.idempotent,
        "activated": receipt.activated,
    }))
}

fn bind_runtime(
    install_root: &OsStr,
    binding_file: &OsStr,
    expected_hash: &OsStr,
) -> Result<Value, String> {
    let expected_hash = exact_sha256(expected_hash)?;
    let binding_path = PathBuf::from(binding_file);
    require_ordinary_file(&binding_path)?;
    let bytes = fs::read(&binding_path).map_err(|error| error.to_string())?;
    if bytes.len() > 128 * 1024 {
        return Err("runtime binding exceeds the bounded input size".to_owned());
    }
    let actual_hash = sha256_bytes(&bytes);
    if actual_hash != expected_hash {
        return Err("runtime binding SHA-256 differs from the authorized input".to_owned());
    }
    let binding = serde_json::from_slice::<InstalledRuntimeBinding>(&bytes)
        .map_err(|_| "runtime binding is malformed".to_owned())?;
    let _ = accepted_operator_provenance()?;
    let changed = Installation::new(PathBuf::from(install_root))
        .configure_runtime_binding(&binding)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "result": "bound",
        "binding_sha256": actual_hash,
        "changed": changed,
    }))
}

#[cfg(windows)]
fn configure_autostart(action: &OsStr, install_root: &OsStr) -> Result<Value, String> {
    let _ = accepted_operator_provenance()?;
    let installation = Installation::new(PathBuf::from(install_root));
    let backend =
        WindowsUserStartupBackend::for_current_user().map_err(|error| error.to_string())?;
    let changed = if action == "enable" {
        installation.enable_autostart(&backend)
    } else if action == "disable" {
        installation.disable_autostart(&backend)
    } else {
        return Err(usage().to_owned());
    }
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "result": "autostart",
        "enabled": action == "enable",
        "changed": changed,
    }))
}

#[cfg(not(windows))]
fn configure_autostart(_action: &OsStr, _install_root: &OsStr) -> Result<Value, String> {
    Err("current-user autostart is supported only on Windows".to_owned())
}

fn request_stop_after_drain() -> Result<Value, String> {
    let _ = accepted_operator_provenance()?;
    let response = LocalAgentTransport::new(Duration::from_secs(10))
        .call(IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: 1,
            command: AgentCommand::ExitAfterDrain,
        })
        .map_err(|error| error.to_string())?;
    match response.body {
        IpcResponseBody::Success(response)
            if matches!(*response, AgentResponse::Accepted { .. }) =>
        {
            Ok(json!({ "result": "stop-after-drain-accepted" }))
        }
        IpcResponseBody::Failure(error) => {
            Err(format!("Agent rejected stop-after-drain: {:?}", error.code))
        }
        _ => Err("Agent returned an unexpected stop-after-drain response".to_owned()),
    }
}

#[cfg(windows)]
fn uninstall(install_root: &OsStr) -> Result<Value, String> {
    let _ = accepted_operator_provenance()?;
    let installation = Installation::new(PathBuf::from(install_root));
    let backend =
        WindowsUserStartupBackend::for_current_user().map_err(|error| error.to_string())?;
    let receipt = installation
        .uninstall(&backend)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "result": "uninstalled",
        "removed_versions": receipt.removed_versions,
        "root_removed": receipt.root_removed,
        "foreign_content_preserved": receipt.foreign_content_preserved,
    }))
}

#[cfg(not(windows))]
fn uninstall(_install_root: &OsStr) -> Result<Value, String> {
    Err("current-user uninstall is supported only on Windows".to_owned())
}

fn operator_provenance() -> PackageProvenance {
    PackageProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: env!("RUNNERMESH_BUILD_COMMIT").to_owned(),
        channel: env!("RUNNERMESH_BUILD_CHANNEL").to_owned(),
        target: env!("RUNNERMESH_BUILD_TARGET").to_owned(),
    }
}

fn accepted_operator_provenance() -> Result<PackageProvenance, String> {
    let provenance = operator_provenance();
    if provenance.commit.len() != 40
        || !provenance
            .commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || provenance.target != WINDOWS_X64_TARGET
    {
        return Err("operator helper is not an immutable Windows x64 candidate build".to_owned());
    }
    Ok(provenance)
}

fn exact_sha256(value: &OsStr) -> Result<String, String> {
    let value = token(value, "SHA-256")?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected SHA-256 must be exact lowercase 64-hex".to_owned());
    }
    Ok(value)
}

fn token(value: &OsStr, name: &str) -> Result<String, String> {
    value
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{name} must be Unicode"))
}

fn require_ordinary_file(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("runtime binding must be an absolute file path".to_owned());
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("runtime binding must be an ordinary file".to_owned());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("runtime binding cannot be reparse-backed".to_owned());
        }
    }
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn usage() -> &'static str {
    "usage: runnermesh-package identity | create <absolute-output-dir> <version> <exact-40-hex-commit> <channel> <absolute-runnermesh.exe> <absolute-runnermesh-agent.exe> <absolute-runnermesh-agent.manifest> | verify <absolute-archive> | extract <absolute-archive> <new-absolute-sandbox-directory> | install <absolute-archive> <accepted-archive-sha256> <new-absolute-staging-directory> <absolute-install-root> | bind <absolute-install-root> <absolute-binding-json> <accepted-binding-sha256> | autostart <enable|disable> <absolute-install-root> | stop-after-drain | uninstall <absolute-install-root>"
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs};

    use super::{run, sha256_bytes};

    #[test]
    fn identity_is_non_mutating_and_reports_compiled_provenance() {
        let value = run(&[OsString::from("identity")]).unwrap();
        assert_eq!(value["result"], "identity");
        assert!(value["provenance"]["version"].is_string());
    }

    #[test]
    fn binding_hash_mismatch_refuses_before_installation_mutation() {
        let root = std::env::temp_dir().join(format!(
            "runnermesh-package-binding-refusal-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let binding = root.join("binding.json");
        fs::write(&binding, b"{}").unwrap();
        let actual = sha256_bytes(b"{}");
        let wrong = "f".repeat(64);
        assert_ne!(actual, wrong);
        assert!(run(&[
            OsString::from("bind"),
            root.join("installed").into_os_string(),
            binding.into_os_string(),
            OsString::from(wrong),
        ])
        .unwrap_err()
        .contains("SHA-256 differs"));
        assert!(!root.join("installed").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
