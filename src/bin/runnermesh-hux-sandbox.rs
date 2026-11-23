//! HUX development helpers: sandbox prepare and privacy-safe read-only discovery.
//!
//! Usage:
//!   runnermesh-hux-sandbox prepare <absolute-sandbox-root>
//!   runnermesh-hux-sandbox discover-readonly
//!   runnermesh-hux-sandbox operator-card-hux1
//!
//! Never mutates production installs, Services, labels, or credentials.

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
    };
    match command.as_str() {
        "operator-card-hux1" => {
            if args.next().is_some() {
                usage();
            }
            print!("{}", runnermesh::hux1_zenbookduo_operator_card());
        }
        "prepare" => {
            let Some(root) = args.next() else {
                usage();
            };
            if args.next().is_some() {
                usage();
            }
            let path = std::path::PathBuf::from(&root);
            if !path.is_absolute() {
                eprintln!("sandbox root must be an absolute path");
                std::process::exit(2);
            }
            match runnermesh::prepare_two_runner_sandbox(&path) {
                Ok(layout) => {
                    println!("HUX-0 sandbox ready");
                    println!("root={}", layout.root.display());
                    println!("registry={}", layout.registry_json.display());
                    println!("acceptance={}", layout.acceptance_md.display());
                    println!(
                        "next: runnermesh-agent.exe --development-root {}",
                        layout.root.display()
                    );
                }
                Err(error) => {
                    eprintln!("runnermesh-hux-sandbox: {error}");
                    std::process::exit(2);
                }
            }
        }
        "discover-readonly" => {
            if args.next().is_some() {
                usage();
            }
            let report = runnermesh::discover_local_runners_read_only(Vec::new());
            let summary = runnermesh::PrivacySafeDiscoverySummary::from_report(&report);
            println!("HUX-1 read-only discovery summary");
            println!("PRODUCTION_MUTATION=false");
            println!("candidate_count={}", summary.candidate_count);
            println!("observed_only_count={}", summary.observed_only_count);
            println!("refused_count={}", summary.refused_count);
            println!(
                "identity_complete_count={}",
                summary.identity_complete_count
            );
            println!("service_enumeration={:?}", summary.service_enumeration);
            println!("session_enumeration={:?}", summary.session_enumeration);
            println!("process_observation={:?}", summary.process_observation);
            println!("incomplete={}", summary.incomplete);
            if report.is_false_empty_success() {
                eprintln!("error: false empty success is forbidden");
                std::process::exit(3);
            }
        }
        _ => usage(),
    }
}

fn usage() -> ! {
    eprintln!("usage: runnermesh-hux-sandbox prepare <absolute-sandbox-root>");
    eprintln!("       runnermesh-hux-sandbox discover-readonly");
    eprintln!("       runnermesh-hux-sandbox operator-card-hux1");
    std::process::exit(2);
}
