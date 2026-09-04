//! Explicit-input package helper. It has no release, upload or install command.

use std::{env, path::PathBuf, process};

use runnermesh::{PackageInput, PackageProvenance, PackageVerifier, WINDOWS_X64_TARGET};

fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [command, archive] if command == "verify" => PackageVerifier::verify_with_hash(
            &PathBuf::from(archive),
        )
        .map(|(manifest, archive_sha256)| {
            serde_json::json!({
                "result": "verified",
                "archive_sha256": archive_sha256,
                "provenance": manifest.provenance,
            })
        }),
        [command, archive, destination] if command == "extract" => {
            let destination = PathBuf::from(destination);
            PackageVerifier::extract_runtime(&PathBuf::from(archive), &destination).map(
                |manifest| {
                    serde_json::json!({
                        "result": "extracted",
                        "destination": destination,
                        "provenance": manifest.provenance,
                    })
                },
            )
        }
        [command, output, version, commit, channel, cli, agent, manifest]
            if command == "create" =>
        {
            PackageVerifier::create(
                &PackageInput {
                    provenance: PackageProvenance {
                        version: version.to_string_lossy().into_owned(),
                        commit: commit.to_string_lossy().into_owned(),
                        channel: channel.to_string_lossy().into_owned(),
                        target: WINDOWS_X64_TARGET.to_owned(),
                    },
                    cli_binary: PathBuf::from(cli),
                    agent_binary: PathBuf::from(agent),
                    agent_manifest: PathBuf::from(manifest),
                },
                &PathBuf::from(output),
            )
            .map(|receipt| {
                serde_json::json!({
                    "result": "created",
                    "archive": receipt.archive,
                    "archive_sha256": receipt.archive_sha256,
                    "provenance": receipt.manifest.provenance,
                })
            })
        }
        _ => {
            eprintln!(
                "usage: runnermesh-package create <absolute-output-dir> <version> <exact-40-hex-commit> <channel> <absolute-runnermesh.exe> <absolute-runnermesh-agent.exe> <absolute-runnermesh-agent.manifest> | verify <absolute-archive> | extract <absolute-archive> <new-absolute-sandbox-directory>"
            );
            process::exit(2);
        }
    };
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("runnermesh-package: {error}");
            process::exit(2);
        }
    }
}
