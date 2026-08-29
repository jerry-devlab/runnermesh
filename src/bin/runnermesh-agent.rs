#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let mut development_root = None;
    let mut runner_home = None;
    let mut work_root = None;
    let mut g11_qualified_executor = false;
    let mut process_probe_names = Vec::new();

    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--development-root" => development_root = arguments.next().map(Into::into),
            "--runner-home" => runner_home = arguments.next().map(Into::into),
            "--work-root" => work_root = arguments.next().map(Into::into),
            "--g11-qualified-executor" => g11_qualified_executor = true,
            "--process-probe-executable" => {
                let Some(name) = arguments.next() else {
                    std::process::exit(2);
                };
                process_probe_names.push(name.to_string_lossy().into_owned());
            }
            _ => std::process::exit(2),
        }
    }

    let Some(development_root) = development_root else {
        std::process::exit(2);
    };
    let result = if g11_qualified_executor {
        match (runner_home, work_root) {
            (Some(runner_home), Some(work_root)) => {
                runnermesh::agent_runtime::run_g11_qualified_agent(
                    development_root,
                    runner_home,
                    work_root,
                    process_probe_names,
                )
            }
            _ => Err("--g11-qualified-executor requires --runner-home and --work-root".to_owned()),
        }
    } else {
        runnermesh::agent_runtime::run_development_agent(
            development_root,
            runner_home,
            process_probe_names,
        )
    };
    if result.is_err() {
        std::process::exit(2);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("runnermesh-agent is only available on Windows");
    std::process::exit(2);
}
