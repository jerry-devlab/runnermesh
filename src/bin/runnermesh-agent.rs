#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let mut development_root = None;
    let mut runner_home = None;
    let mut process_probe_names = Vec::new();

    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--development-root" => development_root = arguments.next().map(Into::into),
            "--runner-home" => runner_home = arguments.next().map(Into::into),
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
    if runnermesh::agent_runtime::run_development_agent(
        development_root,
        runner_home,
        process_probe_names,
    )
    .is_err()
    {
        std::process::exit(2);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("runnermesh-agent is only available on Windows");
    std::process::exit(2);
}
