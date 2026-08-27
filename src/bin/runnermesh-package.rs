use std::{env, path::PathBuf, process};

use runnermesh::{PackageInput, PackageProvenance, PackageVerifier};

fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [command, archive] if command == "verify" => {
            PackageVerifier::verify(&PathBuf::from(archive)).map(|manifest| {
                println!(
                    "verified {} {} {} {}",
                    manifest.provenance.version,
                    manifest.provenance.commit,
                    manifest.provenance.channel,
                    manifest.provenance.target
                );
            })
        }
        [output, version, commit, channel, cli, agent, manifest] => PackageVerifier::create(
            &PackageInput {
                provenance: PackageProvenance {
                    version: version.to_string_lossy().into_owned(),
                    commit: commit.to_string_lossy().into_owned(),
                    channel: channel.to_string_lossy().into_owned(),
                    target: "x86_64-pc-windows-msvc".to_owned(),
                },
                cli_binary: PathBuf::from(cli),
                agent_binary: PathBuf::from(agent),
                agent_manifest: PathBuf::from(manifest),
            },
            &PathBuf::from(output),
        )
        .map(|receipt| println!("{} {}", receipt.archive.display(), receipt.archive_sha256)),
        _ => {
            eprintln!(
                "usage: runnermesh-package <output-dir> <version> <commit> <channel> <runnermesh.exe> <runnermesh-agent.exe> <agent.manifest> | verify <archive>"
            );
            process::exit(2);
        }
    };
    if let Err(error) = result {
        eprintln!("runnermesh-package: {error}");
        process::exit(2);
    }
}
