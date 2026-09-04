#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let mut development_root = None;
    let mut installed_root = None;
    let mut runner_home = None;
    let mut process_probe_names = Vec::new();

    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--development-root" => development_root = arguments.next().map(Into::into),
            "--installed-root" => installed_root = arguments.next().map(Into::into),
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

    if development_root.is_none()
        && installed_root.is_none()
        && runner_home.is_none()
        && process_probe_names.is_empty()
    {
        installed_root = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().and_then(|bin| bin.parent()).map(Into::into));
    }

    let result = match (development_root, installed_root) {
        (Some(development_root), None) => runnermesh::agent_runtime::run_development_agent(
            development_root,
            runner_home,
            process_probe_names,
        ),
        (None, Some(installed_root)) if runner_home.is_none() && process_probe_names.is_empty() => {
            runnermesh::agent_runtime::run_installed_agent(installed_root)
        }
        _ => Err("exactly one Agent runtime profile is required".to_owned()),
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
