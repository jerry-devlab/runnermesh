//! HUX-0 development sandbox helpers for Agent + Tray + CLI human experience.
//!
//! Creates an isolated development root with two synthetic enrolled runners and
//! a privacy-safe discover seed. Never mutates production installs, Services,
//! labels, or credentials.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    AgentConfig, DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity,
    DiscoveredSessionBinding, DiscoveryCapabilities, DiscoveryDisposition,
    DiscoveryEnumerationStatus, DiscoveryReport, EnrollmentState, FileRegistryStore,
    OpaqueIdentityReference, RegistrationScope, RegistryStore, RunnerBackendKind,
    RunnerRegistryEditor, RUNNER_REGISTRY_SCHEMA_VERSION,
};

/// Relative paths written under a development sandbox root.
pub struct HuxSandboxLayout {
    pub root: PathBuf,
    pub config_json: PathBuf,
    pub registry_json: PathBuf,
    pub discovery_seed_json: PathBuf,
    pub acceptance_md: PathBuf,
}

impl HuxSandboxLayout {
    pub fn under(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_json: root.join("config.json"),
            registry_json: root.join("runners-registry.json"),
            discovery_seed_json: root.join("discovery-seed.json"),
            acceptance_md: root.join("HUX-0-ACCEPTANCE.md"),
            root,
        }
    }
}

/// Prepares a reproducible two-runner development sandbox.
pub fn prepare_two_runner_sandbox(root: impl AsRef<Path>) -> Result<HuxSandboxLayout, String> {
    let layout = HuxSandboxLayout::under(root.as_ref());
    fs::create_dir_all(&layout.root).map_err(|error| error.to_string())?;

    // Default Agent config (presentation + probe enablement only). Development
    // must not claim an installed production binding.
    let config = AgentConfig::default();
    fs::write(
        &layout.config_json,
        serde_json::to_vec_pretty(&config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let mut editor = RunnerRegistryEditor::empty();
    editor
        .enroll_from_discovery(
            &synthetic_candidate("sandbox-runner-a", "repo-a", 101),
            None,
            true,
        )
        .map_err(|error| format!("enroll a: {error:?}"))?;
    editor
        .enroll_from_discovery(
            &synthetic_candidate("sandbox-runner-b", "repo-b", 102),
            None,
            false,
        )
        .map_err(|error| format!("enroll b: {error:?}"))?;
    // Explicitly disable B to prove independent intent persistence in HUX-0.
    editor
        .set_desired_enabled("sandbox-runner-b", false)
        .map_err(|error| format!("disable b: {error:?}"))?;

    let store = FileRegistryStore::new(&layout.registry_json);
    store
        .save(editor.registry())
        .map_err(|error| format!("save registry: {error:?}"))?;

    let discovery = DiscoveryReport {
        schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
        read_only: true,
        exhaustive: false,
        service_enumeration: DiscoveryEnumerationStatus::Ok,
        session_enumeration: DiscoveryEnumerationStatus::Ok,
        process_observation: DiscoveryEnumerationStatus::Ok,
        candidates: vec![
            synthetic_candidate("sandbox-runner-a", "repo-a", 101),
            synthetic_candidate("sandbox-runner-b", "repo-b", 102),
        ],
    };
    let seed = serde_json::json!({
        "read_only": true,
        "candidate_count": discovery.candidates.len(),
        "keys": discovery
            .candidates
            .iter()
            .map(|candidate| candidate.local_discovery_key.clone())
            .collect::<Vec<_>>(),
        "note": "synthetic fixture keys only; no private host topology"
    });
    fs::write(
        &layout.discovery_seed_json,
        serde_json::to_vec_pretty(&seed).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let acceptance = format!(
        r#"# HUX-0 Acceptance — Development sandbox

PRODUCTION_MUTATION=false

## Setup

```text
cargo build -p runnermesh --bins
$root = "<absolute-sandbox-root>"
# After prepare_two_runner_sandbox(root):
target\debug\runnermesh-agent.exe --development-root $root
```

In another console (same user session):

```text
target\debug\runnermesh.exe runners list --json
target\debug\runnermesh.exe runners status
target\debug\runnermesh.exe runners enable sandbox-runner-b
target\debug\runnermesh.exe runners disable sandbox-runner-a --when-idle
target\debug\runnermesh.exe runners discover
```

## Required human checks

1. Tray submenu shows both runners with distinct intent labels (enabled/disabled).
2. CLI `runners status` matches Tray intent; never shows "stopped" solely because intent was saved.
3. Restart Agent with the same `--development-root`; disabled intent for `sandbox-runner-b` remains.
4. No production install, Service, label, or credential mutation occurs.

## Fixture

- registry schema: {schema}
- runners: sandbox-runner-a (desired_enabled=true), sandbox-runner-b (desired_enabled=false)
"#,
        schema = RUNNER_REGISTRY_SCHEMA_VERSION,
    );
    fs::write(&layout.acceptance_md, acceptance).map_err(|error| error.to_string())?;
    Ok(layout)
}

fn synthetic_candidate(key: &str, repo: &str, runner_id: u64) -> DiscoveredRunner {
    let identity = OpaqueIdentityReference::new(
        "windows-sid-sha256-v1",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    )
    .expect("fixture identity");
    DiscoveredRunner {
        local_discovery_key: key.to_owned(),
        backend: RunnerBackendKind::UserSession,
        disposition: DiscoveryDisposition::ObservedOnly,
        refusal: None,
        identity: DiscoveredRunnerIdentity {
            registration_scope: Some(RegistrationScope::Repository {
                owner: "fixture-owner".to_owned(),
                repository: repo.to_owned(),
            }),
            runner_id: Some(runner_id),
            expected_runner_name: Some(format!("fixture-{runner_id}")),
            runner_home: PathBuf::from(format!(r"C:\fixture\hux\{key}")),
            work_root: Some(PathBuf::from(format!(r"C:\fixture\hux\{key}\_work"))),
            execution_identity_ref: Some(identity),
        },
        service: None,
        session: Some(DiscoveredSessionBinding {
            current_user_owned: true,
            listener_present: false,
            worker_present: false,
        }),
        observed_lifecycle: DiscoveredLifecycle::Stopped,
        same_scope_collision: false,
        capabilities: DiscoveryCapabilities {
            service_control_contract: false,
            user_session_control_contract: true,
            enrollment_eligible: true,
        },
    }
}

/// Reloads the sandbox registry and asserts in-memory intent matches disk.
pub fn assert_registry_intent_round_trip(root: impl AsRef<Path>) -> Result<(), String> {
    let layout = HuxSandboxLayout::under(root.as_ref());
    let store = FileRegistryStore::new(&layout.registry_json);
    let loaded = store
        .load()
        .map_err(|error| format!("{error:?}"))?
        .ok_or_else(|| "registry missing".to_owned())?;
    let a = loaded
        .get("sandbox-runner-a")
        .ok_or_else(|| "missing a".to_owned())?;
    let b = loaded
        .get("sandbox-runner-b")
        .ok_or_else(|| "missing b".to_owned())?;
    if !a.desired_enabled {
        return Err("runner-a should be enabled".into());
    }
    if b.desired_enabled {
        return Err("runner-b should be disabled".into());
    }
    if a.enrollment_state != EnrollmentState::Enrolled
        || b.enrollment_state != EnrollmentState::Enrolled
    {
        return Err("both runners must remain enrolled".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn prepares_two_runner_sandbox_with_independent_intent() {
        let root = std::env::temp_dir().join(format!(
            "runnermesh-hux0-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let layout = prepare_two_runner_sandbox(&root).unwrap();
        assert!(layout.registry_json.is_file());
        assert!(layout.acceptance_md.is_file());
        assert_registry_intent_round_trip(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
