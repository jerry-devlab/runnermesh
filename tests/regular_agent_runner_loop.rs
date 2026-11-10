//! Integration test for R6 regular Agent execution chain driving the sandbox backend.
//!
//! Validates:
//! - Multi-runner discovery, enrollment, and IPC intent modifications.
//! - Multi-runner scheduling driven directly by `core.observe_decide_reconcile()`.
//! - Strict 4-phase stop dispatch guard on Runner A:
//!   Phase 1: Selector present -> 0 Stop calls.
//!   Phase 2: Selector unknown -> 0 Stop calls.
//!   Phase 3: Worker busy -> 0 Stop calls.
//!   Phase 4: Confirmed absent + Worker idle + plan allows stop -> exactly 1 Stop call.
//! - Zero control actions dispatched for Runner B throughout all phases.
//! - Manual intent durability across restart/reloading from storage.
//! - Same-scope dual managed admission collision refusal (ADR 0004).

use std::{cell::RefCell, fs, sync::Arc};

use runnermesh::{
    runner_service_image_token, AdmissionControlSnapshot, AdmissionDecision, AgentCommand,
    AgentConfig, AgentCore, AgentObservation, AgentObserver, AgentReconciler, AgentResponse,
    BuildProvenance, ConfigStore, ConfigStoreError, DesiredAdmissionState, DiscoveredLifecycle,
    DiscoveredRunner, DiscoveredRunnerIdentity, DiscoveredServiceBinding, DiscoveryCapabilities,
    DiscoveryDisposition, DiscoveryReport, LiveScmExecutionGate, MockScmBackend, MockServiceRecord,
    OpaqueIdentityReference, PerRunnerObservation, PrivilegedScmHelper, RegistrationScope,
    RegistryError, RunnerBackendKind, SandboxMultiRunnerLifecycleBackend, ServiceObservation,
    ServiceStartMode, SyntheticScmTestGrant, UserMode, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
};

struct TestObserver;
impl AgentObserver for TestObserver {
    fn observe(&mut self) -> Result<AgentObservation, String> {
        Ok(AgentObservation::unobserved())
    }
}

struct TestReconciler;
impl AgentReconciler for TestReconciler {
    fn reconcile(
        &mut self,
        _decision: &AdmissionDecision,
        _observation: &AgentObservation,
    ) -> Result<AdmissionControlSnapshot, String> {
        Ok(AdmissionControlSnapshot::not_configured(
            DesiredAdmissionState::Drained,
        ))
    }
}

#[derive(Default)]
struct MemoryStore(RefCell<Option<AgentConfig>>);
impl ConfigStore for MemoryStore {
    fn load(&self) -> Result<Option<AgentConfig>, ConfigStoreError> {
        Ok(self.0.borrow().clone())
    }
    fn save(&self, config: &AgentConfig) -> Result<(), ConfigStoreError> {
        *self.0.borrow_mut() = Some(config.clone());
        Ok(())
    }
}

fn build_info() -> BuildProvenance {
    BuildProvenance {
        version: "0.1.0".to_string(),
        commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_string(),
        channel: "rc".to_string(),
        target: "x86_64-pc-windows-msvc".to_string(),
    }
}

#[test]
fn regular_agent_loop_drives_multi_runner_stop_guard_and_leaves_b_isolated() {
    let temp_dir = std::env::temp_dir().join(format!(
        "runnermesh-r6-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let home_a = temp_dir.join("runner-a");
    let home_b = temp_dir.join("runner-b");
    fs::create_dir_all(home_a.join("bin")).unwrap();
    fs::create_dir_all(home_b.join("bin")).unwrap();
    fs::write(home_a.join("bin").join("RunnerService.exe"), b"stub-a").unwrap();
    fs::write(home_b.join("bin").join("RunnerService.exe"), b"stub-b").unwrap();

    let identity = OpaqueIdentityReference::new(
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        "1111111111111111111111111111111111111111111111111111111111111111",
    )
    .unwrap();

    let image_token_a = runner_service_image_token(&home_a);
    let image_token_b = runner_service_image_token(&home_b);

    // Setup mock SCM backend
    let mock_scm = MockScmBackend::new();
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.a".to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: image_token_a.clone(),
        execution_identity_token: identity.key.clone(),
    });
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.b".to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: true, // Runner B is actively busy with a job
        image_token: image_token_b.clone(),
        execution_identity_token: identity.key.clone(),
    });

    // Synthetic gate authorized for actions.runner.a Stop
    let gate_a = LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
        "actions.runner.a",
        &image_token_a,
        false,
        true,
        false,
    ))
    .unwrap();
    let helper_a = PrivilegedScmHelper::new(mock_scm.clone(), gate_a);
    let lifecycle_backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper_a));

    // Shared registry file store to test persistence across restart
    let registry_file = temp_dir.join("runners-registry.json");
    let registry_store = runnermesh::FileRegistryStore::new(registry_file.clone());

    let mut core = AgentCore::with_registry_store(
        TestObserver,
        TestReconciler,
        MemoryStore::default(),
        build_info(),
        Box::new(registry_store),
    )
    .unwrap();

    // Attach the multi-runner lifecycle backend to the regular AgentCore
    core.set_multi_runner_backend(lifecycle_backend.clone());

    // 1. Discover runners A and B
    core.set_runner_discovery(DiscoveryReport {
        schema_version: runnermesh::RUNNER_DISCOVERY_SCHEMA_VERSION,
        read_only: true,
        exhaustive: false,
        service_enumeration: runnermesh::DiscoveryEnumerationStatus::Ok,
        session_enumeration: runnermesh::DiscoveryEnumerationStatus::Ok,
        process_observation: runnermesh::DiscoveryEnumerationStatus::Ok,
        candidates: vec![
            DiscoveredRunner {
                local_discovery_key: "runner-a".to_string(),
                disposition: DiscoveryDisposition::ObservedOnly,
                refusal: None,
                backend: RunnerBackendKind::WindowsService,
                identity: DiscoveredRunnerIdentity {
                    registration_scope: Some(RegistrationScope::Repository {
                        owner: "test-org".to_string(),
                        repository: "repo-a".to_string(),
                    }),
                    runner_id: Some(201),
                    expected_runner_name: Some("runner-a".to_string()),
                    runner_home: home_a.clone(),
                    work_root: Some(home_a.join("_work")),
                    execution_identity_ref: Some(identity.clone()),
                },
                service: Some(DiscoveredServiceBinding {
                    service_name: "actions.runner.a".to_string(),
                    display_name: Some("Runner A Service".to_string()),
                    image_path: Some(home_a.join("bin").join("RunnerService.exe")),
                    state: DiscoveredLifecycle::Listening,
                }),
                session: None,
                capabilities: DiscoveryCapabilities {
                    service_control_contract: true,
                    user_session_control_contract: false,
                    enrollment_eligible: true,
                },
                same_scope_collision: false,
                observed_lifecycle: DiscoveredLifecycle::Listening,
            },
            DiscoveredRunner {
                local_discovery_key: "runner-b".to_string(),
                disposition: DiscoveryDisposition::ObservedOnly,
                refusal: None,
                backend: RunnerBackendKind::WindowsService,
                identity: DiscoveredRunnerIdentity {
                    registration_scope: Some(RegistrationScope::Repository {
                        owner: "test-org".to_string(),
                        repository: "repo-b".to_string(),
                    }),
                    runner_id: Some(202),
                    expected_runner_name: Some("runner-b".to_string()),
                    runner_home: home_b.clone(),
                    work_root: Some(home_b.join("_work")),
                    execution_identity_ref: Some(identity.clone()),
                },
                service: Some(DiscoveredServiceBinding {
                    service_name: "actions.runner.b".to_string(),
                    display_name: Some("Runner B Service".to_string()),
                    image_path: Some(home_b.join("bin").join("RunnerService.exe")),
                    state: DiscoveredLifecycle::Listening,
                }),
                session: None,
                capabilities: DiscoveryCapabilities {
                    service_control_contract: true,
                    user_session_control_contract: false,
                    enrollment_eligible: true,
                },
                same_scope_collision: false,
                observed_lifecycle: DiscoveredLifecycle::Listening,
            },
        ],
    });

    // 2. Enroll both runners via regular Agent command entrypoint
    for key in ["runner-a", "runner-b"] {
        let resp = core
            .handle_command(AgentCommand::RunnersEnroll {
                local_runner_key: key.to_string(),
                prepare_only: false,
            })
            .unwrap();
        assert!(matches!(resp, AgentResponse::Runners(_)));
    }

    // Set baseline mode to Idle via Agent command
    core.handle_command(AgentCommand::SetMode {
        mode: UserMode::Idle,
    })
    .unwrap();

    // 3. Disable Runner A via regular Agent command entrypoint
    let disable_resp = core
        .handle_command(AgentCommand::RunnersDisable {
            local_runner_key: "runner-a".to_string(),
            when_idle: true,
            prepare_only: false,
        })
        .unwrap();
    assert!(matches!(disable_resp, AgentResponse::Runners(_)));
    assert!(
        !core
            .runner_registry()
            .get("runner-a")
            .unwrap()
            .desired_enabled
    );
    assert!(
        core.runner_registry()
            .get("runner-b")
            .unwrap()
            .desired_enabled
    );

    // Initial check: SCM backend has 0 calls for both services
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.a").is_empty());
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // -------------------------------------------------------------------------
    // Phase 1: Runner A selector is still present on GitHub Actions
    // -------------------------------------------------------------------------
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(true), // Selector still present
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
        PerRunnerObservation {
            local_runner_key: "runner-b".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: true,
            selector_present: Some(true),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
    ]);

    // Regular Agent polling cycle
    core.observe_decide_reconcile().unwrap();

    // Verification: Runner A receives 0 Stop calls while selector is present
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // -------------------------------------------------------------------------
    // Phase 2: Runner A withdrawal status is unknown
    // -------------------------------------------------------------------------
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: None,
            selector_observation_unknown: true, // Readback unknown/failed
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
        PerRunnerObservation {
            local_runner_key: "runner-b".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: true,
            selector_present: Some(true),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
    ]);

    // Regular Agent polling cycle
    core.observe_decide_reconcile().unwrap();

    // Verification: Runner A receives 0 Stop calls while readback is unknown
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // -------------------------------------------------------------------------
    // Phase 3: Runner A Worker is busy executing a job
    // -------------------------------------------------------------------------
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Busy, // Worker is actively executing
            worker_present: true,
            selector_present: Some(false), // Selector was withdrawn, but job still running
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
        PerRunnerObservation {
            local_runner_key: "runner-b".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: true,
            selector_present: Some(true),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
    ]);

    // Regular Agent polling cycle
    core.observe_decide_reconcile().unwrap();

    // Verification: Runner A receives 0 Stop calls while Worker is busy
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // -------------------------------------------------------------------------
    // Phase 4: Independent readback confirms selector absent + Worker idle + plan allows stop
    // -------------------------------------------------------------------------
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false, // Worker finished job and became idle
            selector_present: Some(false), // Selector confirmed absent
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
        PerRunnerObservation {
            local_runner_key: "runner-b".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: true,
            selector_present: Some(true),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
    ]);

    // Regular Agent polling cycle
    core.observe_decide_reconcile().unwrap();

    // Verification: Runner A receives EXACTLY ONE Stop call!
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 1);
    assert_eq!(mock_scm.stop_calls(), 1);

    // Runner B receives ZERO control actions throughout the entire execution!
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert_eq!(mock_scm.service_start_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // -------------------------------------------------------------------------
    // 5. Restart durability test: Reload AgentCore from storage
    // -------------------------------------------------------------------------
    let reloaded_store = runnermesh::FileRegistryStore::new(registry_file);
    let reloaded_core = AgentCore::with_registry_store(
        TestObserver,
        TestReconciler,
        MemoryStore::default(),
        build_info(),
        Box::new(reloaded_store),
    )
    .unwrap();

    assert!(
        !reloaded_core
            .runner_registry()
            .get("runner-a")
            .unwrap()
            .desired_enabled,
        "Runner A must remain disabled across restart"
    );
    assert!(
        reloaded_core
            .runner_registry()
            .get("runner-b")
            .unwrap()
            .desired_enabled,
        "Runner B must remain enabled across restart"
    );

    // -------------------------------------------------------------------------
    // 6. ADR 0004 collision refusal test:
    // Attempting to enroll another runner with the same scope as Runner A is refused
    // -------------------------------------------------------------------------
    let collision_candidate = DiscoveredRunner {
        local_discovery_key: "runner-collision".to_string(),
        disposition: DiscoveryDisposition::ObservedOnly,
        refusal: None,
        backend: RunnerBackendKind::WindowsService,
        identity: DiscoveredRunnerIdentity {
            registration_scope: Some(RegistrationScope::Repository {
                owner: "test-org".to_string(),
                repository: "repo-a".to_string(), // Collides with Runner A's managed scope!
            }),
            runner_id: Some(999),
            expected_runner_name: Some("runner-collision".to_string()),
            runner_home: temp_dir.join("runner-collision"),
            work_root: Some(temp_dir.join("runner-collision").join("_work")),
            execution_identity_ref: Some(identity),
        },
        service: Some(DiscoveredServiceBinding {
            service_name: "actions.runner.collision".to_string(),
            display_name: Some("Collision Runner".to_string()),
            image_path: None,
            state: DiscoveredLifecycle::Listening,
        }),
        session: None,
        capabilities: DiscoveryCapabilities {
            service_control_contract: true,
            user_session_control_contract: false,
            enrollment_eligible: true,
        },
        same_scope_collision: true,
        observed_lifecycle: DiscoveredLifecycle::Listening,
    };

    let mut reg = reloaded_core.runner_registry().clone();
    if let Some(rec) = reg
        .records
        .iter_mut()
        .find(|r| r.local_runner_key == "runner-a")
    {
        rec.managed_admission = true;
    }
    let mut editor = runnermesh::RunnerRegistryEditor::new(reg).unwrap();
    let collision_result = editor.enroll_from_discovery(&collision_candidate, None, true);
    assert!(matches!(
        collision_result,
        Err(RegistryError::SameScopeManagedAdmissionCollision)
    ));

    // Cleanup temp dir
    let _ = fs::remove_dir_all(temp_dir);
}
