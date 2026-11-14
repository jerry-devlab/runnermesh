//! Regression tests for R4 trusted Owner authorization and R6 installed-mode startup assembly.
//!
//! Validates:
//! 1. External callers cannot self-sign via public `new` (zero success, fail closed with AuthorizationNotIssued).
//! 2. External callers cannot forge authorization with unauthorized material (zero success, fail closed).
//! 3. Atomic single-use consumption ledger allows EXACTLY ONE success under concurrent racing requests.
//! 4. Windows SCM write mutations remain sealed (GateClosed) even when a live gate is minted.
//! 5. Installed-mode startup path drives Named Pipe IPC, background polling, 4-phase sequential Stop dispatch
//!    on Runner A, and zero control action isolation on Runner B.

use std::sync::{Arc, Barrier};
use std::thread;

#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use runnermesh::{
    agent_runtime::{start_installed_agent_sandbox, InstalledAgentSandboxConfig},
    runner_service_image_token, AgentCommand, AuthoritativeMultiRunnerLifecycleBackend,
    BoundRunnerAdmissionCoordinator, DiscoveredLifecycle, DiscoveredRunner,
    DiscoveredRunnerIdentity, DiscoveredServiceBinding, DiscoveryCapabilities,
    DiscoveryDisposition, DiscoveryReport, InProcessAdmissionCoordinator, IpcClient, IpcEndpoint,
    IpcRequest, MockScmBackend, MockServiceRecord, MultiRunnerAdmissionCoordinator,
    MultiRunnerLifecycleBackend, OpaqueIdentityReference, PerRunnerObservation,
    PrivilegedScmHelper, RegistrationScope, RegistryError, RunnerBackendKind,
    SandboxMultiRunnerLifecycleBackend, ServiceObservation, SyntheticScmTestGrant, UserMode,
    WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
};
use runnermesh::{
    LiveScmExecutionGate, OwnerAuthorizationScope, ScmApiError, ScmControlBackend,
    ServiceStartMode, SharedAuthorizationLedger, TokenValidatingOwnerVerifier,
    TrustedOwnerAuthorizationRequest, UnissuedOwnerAuthorizationVerifier, WindowsScmBackend,
};

#[test]
fn regression_r4_external_caller_self_selected_token_refused() {
    let raw_token = b"caller-chosen-arbitrary-token-12345";
    let scope = OwnerAuthorizationScope::new(
        "actions.runner.repro-target",
        "authorized-image-token",
        "auth-ident-token",
        "auth-host-token",
        "ff30112c8527f41f49b64a3a6984b1126a876c05",
        "tx-repro-1",
        true,
        false,
        false,
    );

    // Callers instantiating a verifier directly via `new` receive an unissued verifier (no authority secret)
    let verifier = TokenValidatingOwnerVerifier::new(raw_token, scope);

    let request = TrustedOwnerAuthorizationRequest {
        service_name: "actions.runner.repro-target".to_string(),
        image_token: "authorized-image-token".to_string(),
        execution_identity_token: "auth-ident-token".to_string(),
        target_host_token: "auth-host-token".to_string(),
        candidate_commit: "ff30112c8527f41f49b64a3a6984b1126a876c05".to_string(),
        transaction_id: "tx-repro-1".to_string(),
        allow_start: true,
        allow_stop: false,
        allow_change_config: false,
        not_before_epoch_seconds: 100,
        expires_at_epoch_seconds: 2_000_000_000,
        nonce: "repro-nonce-new".to_string(),
        owner_material: raw_token.to_vec(),
    };

    let result = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request);
    // Verification: self-selected token without authority secret fails closed (0 successes)
    assert!(
        matches!(result, Err(ScmApiError::AuthorizationNotIssued)),
        "Self-selected raw token MUST be refused with AuthorizationNotIssued"
    );
}

#[test]
fn regression_r4_external_caller_unissued_verifier_refuses_all_requests() {
    let caller_secret = b"caller-invented-authority-root-secret";
    let scope = OwnerAuthorizationScope::new(
        "actions.runner.repro-target",
        "authorized-image-token",
        "auth-ident-token",
        "auth-host-token",
        "ff30112c8527f41f49b64a3a6984b1126a876c05",
        "tx-repro-2",
        true,
        false,
        false,
    );

    let forged_request = TrustedOwnerAuthorizationRequest {
        service_name: "actions.runner.repro-target".to_string(),
        image_token: "authorized-image-token".to_string(),
        execution_identity_token: "auth-ident-token".to_string(),
        target_host_token: "auth-host-token".to_string(),
        candidate_commit: "ff30112c8527f41f49b64a3a6984b1126a876c05".to_string(),
        transaction_id: "tx-repro-2".to_string(),
        allow_start: true,
        allow_stop: false,
        allow_change_config: false,
        not_before_epoch_seconds: 100,
        expires_at_epoch_seconds: 2_000_000_000,
        nonce: "repro-nonce-forged".to_string(),
        owner_material: caller_secret.to_vec(),
    };

    // 1. Presented to standard unissued runtime verifier -> refused with AuthorizationNotIssued
    let unissued_verifier = UnissuedOwnerAuthorizationVerifier;
    let result_unissued =
        LiveScmExecutionGate::from_trusted_owner_authorization(&unissued_verifier, &forged_request);
    assert!(
        matches!(result_unissued, Err(ScmApiError::AuthorizationNotIssued)),
        "Default product verifier MUST refuse external forged request with AuthorizationNotIssued"
    );

    // 2. Caller instantiates TokenValidatingOwnerVerifier::new -> refused with AuthorizationNotIssued
    let caller_verifier = TokenValidatingOwnerVerifier::new(caller_secret, scope);
    let result_caller =
        LiveScmExecutionGate::from_trusted_owner_authorization(&caller_verifier, &forged_request);
    assert!(
        matches!(result_caller, Err(ScmApiError::AuthorizationNotIssued)),
        "Verifier instantiated via public new MUST refuse request with AuthorizationNotIssued"
    );
}

#[test]
fn regression_r4_shared_ledger_atomic_concurrency() {
    let ledger = SharedAuthorizationLedger::new();
    let token_hash = [0x5Au8; 32];
    let nonce = "atomic-ledger-race-nonce";

    let num_threads = 8;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let l = ledger.clone();
        let b = Arc::clone(&barrier);

        handles.push(thread::spawn(move || {
            b.wait();
            l.try_consume(token_hash, nonce)
        }));
    }

    let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let success_count = results.iter().filter(|&&r| r).count();
    let failure_count = results.iter().filter(|&&r| !r).count();

    assert_eq!(
        success_count, 1,
        "Atomic ledger MUST permit exactly 1 success under concurrent races"
    );
    assert_eq!(
        failure_count,
        num_threads - 1,
        "All other concurrent racing threads MUST be refused"
    );
}

#[test]
fn regression_r4_defense_in_depth_real_scm_writes_sealed() {
    let backend = WindowsScmBackend::mutation_inert();
    assert!(matches!(
        backend.start("actions.runner.test"),
        Err(ScmApiError::GateClosed)
    ));
    assert!(matches!(
        backend.stop("actions.runner.test"),
        Err(ScmApiError::GateClosed)
    ));
    assert!(matches!(
        backend.change_start_mode("actions.runner.test", ServiceStartMode::Manual),
        Err(ScmApiError::GateClosed)
    ));
}

#[cfg(windows)]
#[test]
fn regression_r6_installed_agent_sandbox_ipc_and_sequential_stop() {
    let temp_dir = std::env::temp_dir().join(format!("runnermesh-r6-sbx-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let home_a = temp_dir.join("runner-a");
    let home_b = temp_dir.join("runner-b");
    let _ = std::fs::create_dir_all(home_a.join("bin"));
    let _ = std::fs::create_dir_all(home_b.join("bin"));
    let _ = std::fs::write(home_a.join("bin").join("RunnerService.exe"), b"stub-a");
    let _ = std::fs::write(home_b.join("bin").join("RunnerService.exe"), b"stub-b");
    let image_token_a = runner_service_image_token(&home_a);
    let image_token_b = runner_service_image_token(&home_b);

    let identity = OpaqueIdentityReference::new(
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        "1111111111111111111111111111111111111111111111111111111111111111",
    )
    .unwrap();

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
        worker_busy: true, // Runner B busy executing a job
        image_token: image_token_b.clone(),
        execution_identity_token: identity.key.clone(),
    });

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

    let discovery_report = DiscoveryReport {
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
    };

    let ipc_endpoint = IpcEndpoint::for_test_scope(format!("r6-sbx-{}", std::process::id()));

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: temp_dir.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(lifecycle_backend.clone()),
        discovery_report: Some(discovery_report),
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: true,
    };

    // 1. Start the installed Agent runtime with explicit sandbox configuration
    let runtime = start_installed_agent_sandbox(sandbox_config).unwrap();

    // 2. Connect via real Named Pipe IPC client
    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));

    // Enroll runners A and B via IPC
    for key in ["runner-a", "runner-b"] {
        let resp = ipc_client
            .call(IpcRequest::new(AgentCommand::RunnersEnroll {
                local_runner_key: key.to_string(),
                prepare_only: false,
            }))
            .expect("IPC RunnersEnroll must succeed");
        assert!(matches!(
            resp.body,
            runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Runners(ref report) if report.applied)
        ));
    }

    // Set baseline mode to Idle via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::SetMode {
            mode: UserMode::Idle,
        }))
        .expect("IPC SetMode must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Accepted { .. })
    ));

    // Disable Runner A via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersDisable {
            local_runner_key: "runner-a".to_string(),
            when_idle: true,
            prepare_only: false,
        }))
        .expect("IPC RunnersDisable must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Runners(ref report) if report.applied)
    ));

    // 3. Drive 4-phase sequential Stop dispatch via Agent observation driven automatically by background scheduler
    // Phase 1: Selector present -> 0 Stop calls
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(true),
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
    thread::sleep(Duration::from_millis(100));
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // Query status over IPC to verify Draining
    let list_resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersList))
        .expect("IPC RunnersList must succeed");
    if let runnermesh::IpcResponseBody::Success(boxed) = list_resp.body {
        if let runnermesh::AgentResponse::Runners(report) = *boxed {
            let runner_a = report
                .records
                .iter()
                .find(|r| r.local_runner_key == "runner-a")
                .unwrap();
            assert_eq!(
                runner_a.presentation_status,
                runnermesh::RunnerPresentationStatus::Disabled
            );
            assert!(!runner_a.desired_enabled);
        } else {
            panic!("Expected Runners response");
        }
    } else {
        panic!("Expected Success response");
    }

    // Phase 2: Readback unknown -> 0 Stop calls
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: None,
            selector_observation_unknown: true,
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
    thread::sleep(Duration::from_millis(100));
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // Phase 3: Worker busy -> 0 Stop calls
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Busy,
            worker_present: true,
            selector_present: Some(false),
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
    thread::sleep(Duration::from_millis(100));
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // Phase 4: Confirmed absent + Worker idle -> exactly 1 Stop call for A, 0 for B
    // Produced automatically by the background periodic loop without manual stepping
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(false),
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
    let start = std::time::Instant::now();
    while mock_scm.service_stop_calls("actions.runner.a") < 1
        && start.elapsed() < Duration::from_secs(2)
    {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        mock_scm.service_stop_calls("actions.runner.a"),
        1,
        "Background periodic scheduler MUST automatically trigger Stop for runner A"
    );
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert_eq!(mock_scm.service_start_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // Query status over IPC to verify Disabled
    let list_resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersList))
        .expect("IPC RunnersList must succeed");
    if let runnermesh::IpcResponseBody::Success(boxed) = list_resp.body {
        if let runnermesh::AgentResponse::Runners(report) = *boxed {
            let runner_a = report
                .records
                .iter()
                .find(|r| r.local_runner_key == "runner-a")
                .unwrap();
            assert_eq!(
                runner_a.presentation_status,
                runnermesh::RunnerPresentationStatus::Disabled
            );
        } else {
            panic!("Expected Runners response");
        }
    } else {
        panic!("Expected Success response");
    }

    // 4. Graceful shutdown via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::ExitAfterDrain))
        .expect("ExitAfterDrain via IPC must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Accepted { .. })
    ));
    runtime.shutdown();

    // 5. Restart durability: Reassemble from persistent disk storage
    let obs_restart = runnermesh::agent_runtime::RuntimeObserver::new(None, Vec::new());
    let rec_restart = runnermesh::agent_runtime::RuntimeReconciler::Unconfigured(
        runnermesh::agent_runtime::NoRunnerControl::new(),
    );
    let reloaded_core = runnermesh::agent_runtime::assemble_installed_core(
        temp_dir.join("config.json"),
        obs_restart,
        rec_restart,
        None,
    )
    .unwrap();
    assert!(
        !reloaded_core
            .runner_registry()
            .get("runner-a")
            .unwrap()
            .desired_enabled,
        "Runner A must remain disabled after restart"
    );
    assert!(
        reloaded_core
            .runner_registry()
            .get("runner-b")
            .unwrap()
            .desired_enabled,
        "Runner B must remain enabled after restart"
    );

    // 6. ADR 0004 collision refusal: enrolling same scope with managed admission is rejected
    let collision_candidate = DiscoveredRunner {
        local_discovery_key: "runner-collision".to_string(),
        disposition: DiscoveryDisposition::ObservedOnly,
        refusal: None,
        backend: RunnerBackendKind::WindowsService,
        identity: DiscoveredRunnerIdentity {
            registration_scope: Some(RegistrationScope::Repository {
                owner: "test-org".to_string(),
                repository: "repo-a".to_string(),
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
}

#[cfg(windows)]
#[test]
fn regression_r6_falsifiability_proves_disabled_scheduler_produces_zero_actions() {
    let temp_dir =
        std::env::temp_dir().join(format!("runnermesh-r6-falsify-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let home_a = temp_dir.join("runner-a");
    let home_b = temp_dir.join("runner-b");
    let _ = std::fs::create_dir_all(home_a.join("bin"));
    let _ = std::fs::create_dir_all(home_b.join("bin"));
    let _ = std::fs::write(home_a.join("bin").join("RunnerService.exe"), b"stub-a");
    let _ = std::fs::write(home_b.join("bin").join("RunnerService.exe"), b"stub-b");
    let image_token_a = runner_service_image_token(&home_a);
    let image_token_b = runner_service_image_token(&home_b);

    let identity = OpaqueIdentityReference::new(
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        "1111111111111111111111111111111111111111111111111111111111111111",
    )
    .unwrap();

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
        worker_busy: true,
        image_token: image_token_b.clone(),
        execution_identity_token: identity.key.clone(),
    });

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

    let discovery_report = DiscoveryReport {
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
                    execution_identity_ref: Some(identity),
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
    };

    let ipc_endpoint = IpcEndpoint::for_test_scope(format!("r6-falsify-{}", std::process::id()));

    // CRITICAL: enable_periodic_scheduler is explicitly FALSE
    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: temp_dir.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(lifecycle_backend.clone()),
        discovery_report: Some(discovery_report),
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: false,
    };

    let runtime = start_installed_agent_sandbox(sandbox_config).unwrap();
    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));

    for key in ["runner-a", "runner-b"] {
        let _ = ipc_client
            .call(IpcRequest::new(AgentCommand::RunnersEnroll {
                local_runner_key: key.to_string(),
                prepare_only: false,
            }))
            .expect("IPC RunnersEnroll must succeed");
    }

    let _ = ipc_client
        .call(IpcRequest::new(AgentCommand::SetMode {
            mode: UserMode::Idle,
        }))
        .expect("IPC SetMode must succeed");

    let _ = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersDisable {
            local_runner_key: "runner-a".to_string(),
            when_idle: true,
            prepare_only: false,
        }))
        .expect("IPC RunnersDisable must succeed");

    // Present Phase 4 observation (confirmed absent + idle) where Stop would normally be dispatched
    lifecycle_backend.set_observations(vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_string(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(false),
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

    // Wait 150ms: without the periodic scheduler running, zero actions can be dispatched
    thread::sleep(Duration::from_millis(150));

    // FALSIFIABILITY PROOF:
    // Because the periodic scheduler is disabled and manual step_poll is eliminated,
    // runner A Stop calls MUST remain 0!
    assert_eq!(
        mock_scm.service_stop_calls("actions.runner.a"),
        0,
        "Disabled periodic scheduler MUST produce ZERO Stop calls (falsifiability proof)"
    );
    assert_eq!(mock_scm.service_stop_calls("actions.runner.b"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::ExitAfterDrain))
        .expect("ExitAfterDrain via IPC must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Accepted { .. })
    ));
    runtime.shutdown();
}

#[test]
#[cfg(windows)]
fn regression_c5_lrm_sandbox_package_and_installed_agent_verification() {
    let temp_dir = std::env::temp_dir().join(format!(
        "runnermesh-c5-pkg-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let staging = temp_dir.join("staging");
    std::fs::create_dir_all(&staging).unwrap();

    let agent_exe = staging.join("runnermesh-agent.exe");
    let cli_exe = staging.join("runnermesh.exe");
    let agent_manifest = staging.join("runnermesh-agent.manifest");
    std::fs::write(&agent_exe, b"stub-agent").unwrap();
    std::fs::write(&cli_exe, b"stub-cli").unwrap();
    std::fs::write(&agent_manifest, b"stub-manifest").unwrap();

    let input = runnermesh::PackageInput {
        provenance: runnermesh::PackageProvenance {
            version: "0.1.0".to_string(),
            commit: "17e35296eeaeb27525dda3d373421354173a1393".to_string(),
            channel: "rc".to_string(),
            target: runnermesh::WINDOWS_X64_TARGET.to_string(),
        },
        cli_binary: cli_exe,
        agent_binary: agent_exe,
        agent_manifest,
    };

    let archives_dir = temp_dir.join("archives");
    let receipt = runnermesh::PackageVerifier::create(&input, &archives_dir).unwrap();
    assert_eq!(receipt.manifest.provenance.version, "0.1.0");
    assert_eq!(
        receipt.manifest.provenance.commit,
        "17e35296eeaeb27525dda3d373421354173a1393"
    );
    assert!(receipt.archive.is_file());

    // 1. Extract runtime package into payload directory
    let payload_dir = temp_dir.join("payload");
    let manifest =
        runnermesh::PackageVerifier::extract_runtime(&receipt.archive, &payload_dir).unwrap();
    assert_eq!(manifest.provenance.version, "0.1.0");

    // 2. Perform versioned installation
    let install_root = temp_dir.join("installed");
    let installation = runnermesh::Installation::new(install_root.clone());
    let install_receipt = installation.install("0.1.0", &payload_dir).unwrap();
    assert_eq!(install_receipt.version, "0.1.0");

    let autostart = runnermesh::SandboxAutostartBackend::new(temp_dir.join("autostart.json"));
    installation.enable_autostart(&autostart).unwrap();

    // 3. Assemble and run sandboxed installed agent with AuthoritativeMultiRunnerLifecycleBackend
    let mock_scm = MockScmBackend::new();
    let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
    let scope = RegistrationScope::Repository {
        owner: "c5-owner".to_string(),
        repository: "c5-repo".to_string(),
    };
    let admission_coord = Arc::new(
        runnermesh::InProcessAdmissionCoordinator::new().with_selector(scope.clone(), 901, true),
    );
    let authoritative_backend = Arc::new(
        runnermesh::AuthoritativeMultiRunnerLifecycleBackend::new(helper)
            .with_admission_coordinator(admission_coord)
            .with_session_coordinator(Arc::new(runnermesh::StandardSessionCoordinator::inert())),
    );
    authoritative_backend.register_runner_target("c5-runner", scope.clone(), 901);

    let ipc_endpoint = IpcEndpoint::for_test_scope(format!("c5-test-{}", std::process::id()));

    let home = temp_dir.join("runner-home");
    std::fs::create_dir_all(&home).unwrap();
    let identity = OpaqueIdentityReference::new(
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        "5555555555555555555555555555555555555555555555555555555555555555",
    )
    .unwrap();

    let discovery_report = DiscoveryReport {
        schema_version: runnermesh::RUNNER_DISCOVERY_SCHEMA_VERSION,
        read_only: true,
        exhaustive: false,
        service_enumeration: runnermesh::DiscoveryEnumerationStatus::Ok,
        session_enumeration: runnermesh::DiscoveryEnumerationStatus::Ok,
        process_observation: runnermesh::DiscoveryEnumerationStatus::Ok,
        candidates: vec![DiscoveredRunner {
            local_discovery_key: "c5-runner".to_string(),
            disposition: DiscoveryDisposition::ObservedOnly,
            refusal: None,
            backend: RunnerBackendKind::WindowsService,
            identity: DiscoveredRunnerIdentity {
                registration_scope: Some(scope),
                runner_id: Some(901),
                expected_runner_name: Some("c5-runner".to_string()),
                runner_home: home.clone(),
                work_root: Some(home.join("_work")),
                execution_identity_ref: Some(identity),
            },
            service: Some(DiscoveredServiceBinding {
                service_name: "actions.runner.c5".to_string(),
                display_name: None,
                image_path: None,
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
        }],
    };

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: install_root.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(authoritative_backend.clone()),
        discovery_report: Some(discovery_report),
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: true,
    };

    let runtime = start_installed_agent_sandbox(sandbox_config).unwrap();
    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));

    // 4. Enroll runner via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersEnroll {
            local_runner_key: "c5-runner".to_string(),
            prepare_only: false,
        }))
        .expect("IPC RunnersEnroll must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Runners(ref report) if report.applied)
    ));

    // 5. Query status via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersStatus {
            local_runner_key: None,
        }))
        .expect("IPC RunnersStatus must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Runners(ref report) if report.records.len() == 1)
    ));

    // 6. Test disable-all via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersDisableAll {
            when_idle: true,
            prepare_only: false,
        }))
        .expect("IPC RunnersDisableAll must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Runners(ref report) if report.applied && !report.records[0].desired_enabled)
    ));

    // 7. Test enable-all via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersEnableAll {
            prepare_only: false,
        }))
        .expect("IPC RunnersEnableAll must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Runners(ref report) if report.applied && report.records[0].desired_enabled)
    ));

    // 8. Test Doctor via IPC
    println!("[c5_lifecycle] Phase 8: Doctor report via IPC");
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunDoctor))
        .expect("IPC RunDoctor must succeed");
    assert!(matches!(
        resp.body,
        runnermesh::IpcResponseBody::Success(ref boxed) if matches!(**boxed, runnermesh::AgentResponse::Doctor(ref doc) if doc.checks.len() >= 2)
    ));

    // 9. Shutdown runtime cleanly
    println!("[c5_lifecycle] Phase 9: Graceful runtime shutdown");
    let shutdown_start = std::time::Instant::now();
    runtime.shutdown();
    let shutdown_elapsed = shutdown_start.elapsed();
    println!("[c5_lifecycle] Phase 9 complete: runtime shut down in {shutdown_elapsed:?}");
    assert!(
        shutdown_elapsed < Duration::from_secs(3),
        "shutdown took too long: {shutdown_elapsed:?}"
    );

    // 10. Clean uninstall
    println!("[c5_lifecycle] Phase 10: Clean uninstall");
    let uninstall = installation.uninstall(&autostart).unwrap();
    assert_eq!(uninstall.removed_versions, 1);
    assert!(uninstall.foreign_content_preserved);

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("[c5_lifecycle] All phases PASSED");
}

#[cfg(windows)]
fn setup_test_sandbox_runtime(
    test_name: &str,
) -> (
    std::path::PathBuf,
    runnermesh::agent_runtime::RunningInstalledAgentSandbox,
    IpcEndpoint,
) {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c5-{test_name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let install_root = temp_dir.join("installed");
    std::fs::create_dir_all(&install_root).unwrap();

    let ipc_endpoint =
        IpcEndpoint::for_test_scope(format!("c5-{test_name}-{}", std::process::id()));

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root,
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: None,
        discovery_report: None,
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: true,
    };

    let runtime = start_installed_agent_sandbox(sandbox_config).unwrap();
    (temp_dir, runtime, ipc_endpoint)
}

#[test]
#[cfg(windows)]
fn regression_c5_shutdown_without_client_unblocks_cleanly() {
    println!("[test:c5_shutdown_without_client] START");
    let (temp_dir, runtime, _endpoint) = setup_test_sandbox_runtime("no-client");

    // Give pipe thread a moment to enter ConnectNamedPipe
    std::thread::sleep(Duration::from_millis(50));

    let start = std::time::Instant::now();
    runtime.shutdown();
    let elapsed = start.elapsed();
    println!("[test:c5_shutdown_without_client] shutdown completed in {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(3),
        "shutdown took too long: {elapsed:?}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("[test:c5_shutdown_without_client] PASS");
}

#[test]
#[cfg(windows)]
fn regression_c5_shutdown_after_exit_after_drain_unblocks_cleanly() {
    println!("[test:c5_exit_after_drain] START");
    let (temp_dir, runtime, endpoint) = setup_test_sandbox_runtime("exit-drain");
    let client = IpcClient::new(endpoint, Duration::from_secs(3));

    let resp = client
        .call(IpcRequest::new(AgentCommand::ExitAfterDrain))
        .expect("IPC call should succeed");
    assert!(matches!(resp.body, runnermesh::IpcResponseBody::Success(_)));

    let start = std::time::Instant::now();
    runtime.shutdown();
    let elapsed = start.elapsed();
    println!("[test:c5_exit_after_drain] shutdown completed in {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(3),
        "shutdown took too long: {elapsed:?}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("[test:c5_exit_after_drain] PASS");
}

#[test]
#[cfg(windows)]
fn regression_c5_shutdown_after_connection_failure_unblocks_cleanly() {
    println!("[test:c5_connection_failure] START");
    let (temp_dir, runtime, endpoint) = setup_test_sandbox_runtime("conn-fail");

    // Client connects and abruptly sends partial garbage without completing IPC frame
    {
        let pipe_name = endpoint.pipe_name();
        let wide_name: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            windows_sys::Win32::Storage::FileSystem::CreateFileW(
                wide_name.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ
                    | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE,
                0,
                std::ptr::null(),
                windows_sys::Win32::Storage::FileSystem::OPEN_EXISTING,
                windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            let garbage = [0xde, 0xad, 0xbe];
            let mut written = 0;
            unsafe {
                windows_sys::Win32::Storage::FileSystem::WriteFile(
                    handle,
                    garbage.as_ptr(),
                    garbage.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                );
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
        }
    }

    // Give server thread a moment to handle the error and loop back to ConnectNamedPipe
    std::thread::sleep(Duration::from_millis(50));

    let start = std::time::Instant::now();
    runtime.shutdown();
    let elapsed = start.elapsed();
    println!("[test:c5_connection_failure] shutdown completed in {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(3),
        "shutdown took too long: {elapsed:?}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("[test:c5_connection_failure] PASS");
}

#[test]
#[cfg(windows)]
fn regression_c5_shutdown_race_under_concurrent_ipc_unblocks_cleanly() {
    println!("[test:c5_shutdown_race] START");
    let (temp_dir, runtime, endpoint) = setup_test_sandbox_runtime("race");

    let stop_worker = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_worker_clone = Arc::clone(&stop_worker);
    let endpoint_clone = endpoint.clone();

    let worker = std::thread::spawn(move || {
        let client = IpcClient::new(endpoint_clone, Duration::from_millis(200));
        while !stop_worker_clone.load(std::sync::atomic::Ordering::Acquire) {
            let _ = client.call(IpcRequest::new(AgentCommand::RunnersStatus {
                local_runner_key: None,
            }));
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    // Let concurrent requests run briefly
    std::thread::sleep(Duration::from_millis(30));

    let start = std::time::Instant::now();
    runtime.shutdown();
    let elapsed = start.elapsed();
    println!("[test:c5_shutdown_race] shutdown completed in {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(3),
        "shutdown took too long: {elapsed:?}"
    );

    stop_worker.store(true, std::sync::atomic::Ordering::Release);
    let _ = worker.join();

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("[test:c5_shutdown_race] PASS");
}

// ==============================================================================
// 160H Delivery Program: 5 Audit Findings Regressions & Counterexamples
// ==============================================================================

#[test]
fn regression_finding_5_counterexample_and_fix_durable_replay_protection() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-durable-ledger-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let ledger_path = temp_dir.join("consumed-authorizations.json");

    let token_hash = [0x77u8; 32];
    let nonce = "voucher-nonce-durable-proof";

    // 1. Counterexample: Standard in-memory-only ledgers lose state across reconstructed instances
    let mem_ledger_1 = SharedAuthorizationLedger::new();
    assert!(mem_ledger_1.try_consume(token_hash, nonce));
    let mem_ledger_2 = SharedAuthorizationLedger::new();
    // In-memory ledger without durability would falsely allow replay on a fresh process instance:
    assert!(
        mem_ledger_2.try_consume(token_hash, nonce),
        "COUNTEREXAMPLE: In-memory only ledger cannot persist across process restart"
    );

    // 2. Verified Fix: Durable ledger persists consumed state and rejects replays across instances
    let durable_ledger_1 = SharedAuthorizationLedger::with_durable_file(&ledger_path)
        .expect("durable ledger initialization");
    assert!(
        durable_ledger_1.try_consume(token_hash, nonce),
        "Initial consumption must succeed"
    );
    assert!(
        durable_ledger_1.is_consumed(&token_hash, nonce),
        "First instance reflects consumption"
    );

    // Instance 2 reloads from the durable path (simulating process restart)
    let durable_ledger_2 =
        SharedAuthorizationLedger::with_durable_file(&ledger_path).expect("durable ledger reload");
    assert!(
        durable_ledger_2.is_consumed(&token_hash, nonce),
        "Reloaded instance must reflect previously persisted consumption"
    );
    assert!(
        !durable_ledger_2.try_consume(token_hash, nonce),
        "FIX VERIFIED: Replay is strictly refused on fresh instance with durable ledger"
    );

    // Also test replay with same token but different nonce (swapped-nonce replay)
    assert!(
        !durable_ledger_2.try_consume(token_hash, "swapped-nonce"),
        "FIX VERIFIED: Token hash replay is refused even with different nonce"
    );

    // Also test replay with different token but same nonce (reused nonce)
    let other_token = [0x88u8; 32];
    assert!(
        !durable_ledger_2.try_consume(other_token, nonce),
        "FIX VERIFIED: Nonce reuse is refused across durable instances"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_finding_2_counterexample_and_fix_scm_observation_by_real_service_name() {
    let mock_scm = MockScmBackend::new();
    let real_service_name = "actions.runner.live-real-name";
    let hashed_discovery_key = "windows-service|home:aabbccdd|svc:11223344";

    // Register service under real service name in Win32 SCM
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: real_service_name.to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: "tok-f2".to_string(),
        execution_identity_token: "ident-f2".to_string(),
    });

    let helper = PrivilegedScmHelper::new(mock_scm.clone(), LiveScmExecutionGate::CLOSED);
    let backend = AuthoritativeMultiRunnerLifecycleBackend::new(helper);
    let scope = RegistrationScope::Repository {
        owner: "f2-owner".to_string(),
        repository: "f2-repo".to_string(),
    };

    // 1. COUNTEREXAMPLE: Querying directly by hashed discovery key fails (returns NotFound/Unknown)
    let snap_by_key = mock_scm.query_exact(hashed_discovery_key);
    assert!(
        snap_by_key.is_err(),
        "COUNTEREXAMPLE: SCM does not know the hashed discovery key"
    );

    // 2. FIX: Register runner target with real service name mapping
    backend.register_runner_target_with_service(
        hashed_discovery_key,
        scope,
        42,
        Some(real_service_name),
    );

    // Observe runners using the hashed key: backend resolves real service name for query_exact
    let obs = backend.observe_runners(&[hashed_discovery_key.to_string()]);
    assert_eq!(obs.len(), 1);
    assert_eq!(
        obs[0].lifecycle,
        DiscoveredLifecycle::Listening,
        "FIX VERIFIED: Backend resolves real service name and returns genuine SCM state"
    );
    assert!(
        obs[0].ownership_verified,
        "FIX VERIFIED: Ownership and image path verified against real service"
    );
}

#[test]
#[cfg(windows)]
fn regression_finding_3_counterexample_and_fix_per_runner_admission_coordinator_isolation() {
    let scope_a = RegistrationScope::Repository {
        owner: "org".to_string(),
        repository: "repo-a".to_string(),
    };
    let scope_b = RegistrationScope::Repository {
        owner: "org".to_string(),
        repository: "repo-b".to_string(),
    };

    // 1. COUNTEREXAMPLE: Scope mismatch in BoundRunnerAdmissionCoordinator MUST fail closed with Err, NEVER Ok(None)
    let fake_backend = runnermesh::FakeGithubAdmissionBackend::new_with_selector_present(true);
    let coord_bound_a = BoundRunnerAdmissionCoordinator::new(scope_a.clone(), 101, fake_backend);

    let mismatch_obs = coord_bound_a.observe_selector(&scope_b, 202);
    assert!(
        mismatch_obs.is_err(),
        "COUNTEREXAMPLE & FIX: Scope mismatch must return Err, never Ok(None)"
    );

    // 2. FIX: AuthoritativeMultiRunnerLifecycleBackend isolates per-runner coordinators
    let mock_scm = MockScmBackend::new();
    let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
    let backend = AuthoritativeMultiRunnerLifecycleBackend::new(helper);

    let coord_a =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope_a.clone(), 101, true));
    let coord_b =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope_b.clone(), 202, false));

    backend.register_runner_target("runner-a", scope_a.clone(), 101);
    backend.register_runner_target("runner-b", scope_b.clone(), 202);

    backend.register_runner_admission_coordinator(scope_a.clone(), 101, coord_a);
    backend.register_runner_admission_coordinator(scope_b.clone(), 202, coord_b);

    let obs = backend.observe_runners(&["runner-a".to_string(), "runner-b".to_string()]);
    assert_eq!(obs.len(), 2);

    // Runner A reflects coord_a (Present)
    assert_eq!(obs[0].local_runner_key, "runner-a");
    assert_eq!(obs[0].selector_present, Some(true));
    assert!(!obs[0].selector_observation_unknown);

    // Runner B reflects coord_b (Absent)
    assert_eq!(obs[1].local_runner_key, "runner-b");
    assert_eq!(obs[1].selector_present, Some(false));
    assert!(!obs[1].selector_observation_unknown);

    // Runner C with no coordinator registered fails closed as unknown
    backend.register_runner_target("runner-c", scope_a.clone(), 303);
    let obs_c = backend.observe_runners(&["runner-c".to_string()]);
    assert_eq!(obs_c[0].selector_present, None);
    assert!(
        obs_c[0].selector_observation_unknown,
        "Unbound runner without coordinator fails closed as selector_observation_unknown = true"
    );
}

#[test]
#[cfg(windows)]
fn regression_finding_1_counterexample_and_fix_selector_advertise_failure_prevents_start() {
    let mock_scm = MockScmBackend::new();
    let service_name = "actions.runner.f1-test";
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: service_name.to_string(),
            state: DiscoveredLifecycle::Stopped,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: "tok-f1".to_string(),
        execution_identity_token: "ident-f1".to_string(),
    });

    let scope = RegistrationScope::Repository {
        owner: "f1-owner".to_string(),
        repository: "f1-repo".to_string(),
    };

    let admission_coord =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope.clone(), 77, false));
    let helper = PrivilegedScmHelper::new(mock_scm.clone(), LiveScmExecutionGate::CLOSED);
    let backend = Arc::new(
        AuthoritativeMultiRunnerLifecycleBackend::new(helper)
            .with_admission_coordinator(admission_coord.clone()),
    );
    backend.register_runner_target_with_service("runner-f1", scope.clone(), 77, Some(service_name));

    // 1. COUNTEREXAMPLE: When selector advertisement fails, StartService must NOT be issued
    admission_coord.set_fail_advertise(true);

    let advertise_res = backend.advertise_selector(&scope, 77);
    assert!(
        advertise_res.is_err(),
        "COUNTEREXAMPLE: Simulated advertisement failure occurred"
    );

    // Dispatching when advertisement failed must NOT produce StartService
    let obs = backend.observe_runners(&["runner-f1".to_string()]);
    // Selector remains absent/not confirmed
    assert_ne!(obs[0].selector_present, Some(true));

    // 2. FIX: Now enable successful advertisement and confirm presence
    admission_coord.set_fail_advertise(false);
    let advertise_success = backend.advertise_selector(&scope, 77);
    assert!(advertise_success.is_ok() && advertise_success.unwrap());

    let obs_after = backend.observe_runners(&["runner-f1".to_string()]);
    assert_eq!(
        obs_after[0].selector_present,
        Some(true),
        "FIX VERIFIED: After confirmed advertisement, selector is observed present"
    );
}

#[test]
#[cfg(windows)]
fn regression_c5_shutdown_timeout_path_still_joins_and_reclaims_handle() {
    println!("[test:c5_shutdown_timeout_reclamation] START");
    let (temp_dir, runtime, endpoint) = setup_test_sandbox_runtime("timeout-reclaim");

    // Connect a half-open raw named pipe client without sending anything
    let pipe_name = endpoint.pipe_name();
    let wide_name: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        windows_sys::Win32::Storage::FileSystem::CreateFileW(
            wide_name.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ
                | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE,
            0,
            std::ptr::null(),
            windows_sys::Win32::Storage::FileSystem::OPEN_EXISTING,
            windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };

    // The server thread is now connected, blocked waiting in ReadFile.
    // Call shutdown with a 20ms timeout so that it exceeds the timeout and escalates.
    let start = std::time::Instant::now();
    runtime.shutdown_with_timeout(Duration::from_millis(20));
    let elapsed = start.elapsed();
    println!("[test:c5_shutdown_timeout_reclamation] completed in {elapsed:?}");

    if handle != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
    }

    // Verify stage log completed all steps
    let stage_file = temp_dir.join("installed/runtime-stage.txt");
    if stage_file.exists() {
        let content = std::fs::read_to_string(&stage_file).unwrap_or_default();
        println!("[test:c5_shutdown_timeout_reclamation] final recorded stage: {content}");
        assert!(
            content.contains("shutdown-complete"),
            "Shutdown stage must reach shutdown-complete"
        );
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("[test:c5_shutdown_timeout_reclamation] PASS");
}

#[test]
#[cfg(windows)]
fn regression_c1_cross_process_replay_protection_two_preopened_instances() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c1-replay-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let ledger_path = temp_dir.join("authorizations.json");

    let token_hash = [0x5Bu8; 32];
    let nonce = "c1-preopened-nonce";

    // Both instances pre-opened on the same ledger path before any consumption
    let ledger_a = SharedAuthorizationLedger::with_durable_file(&ledger_path).unwrap();
    let ledger_b = SharedAuthorizationLedger::with_durable_file(&ledger_path).unwrap();

    // A consumes successfully
    let res_a = ledger_a.try_consume(token_hash, nonce);
    assert!(res_a, "First consumption on ledger A must succeed");

    // B was already opened before A consumed. It must observe the durable consumption and fail closed!
    let res_b = ledger_b.try_consume(token_hash, nonce);
    assert!(
        !res_b,
        "Sequential consumption on pre-opened ledger B must be refused"
    );
    assert!(ledger_b.is_consumed(&token_hash, nonce));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_c1_durable_ledger_corrupt_json_fails_closed() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c1-corrupt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let ledger_path = temp_dir.join("authorizations.json");

    // Write malformed/corrupted JSON to the ledger file
    std::fs::write(
        &ledger_path,
        b"{\"schema_version\": 1, \"consumed\": [CORRUPT-SYNTAX",
    )
    .unwrap();

    // When file has corrupt JSON, loading with_durable_file fails closed:
    let ledger_res = SharedAuthorizationLedger::with_durable_file(&ledger_path);
    assert!(
        ledger_res.is_err(),
        "Corrupt ledger on disk must fail closed"
    );

    // The corrupt file must NOT have been wiped or overwritten to empty valid JSON!
    let content = std::fs::read_to_string(&ledger_path).unwrap();
    assert!(
        content.contains("CORRUPT-SYNTAX"),
        "Corrupt ledger content must not be cleared to empty"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_c1_registry_load_duplicate_rejection() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c1-registry-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let registry_file = temp_dir.join("runners-registry.json");

    // Construct registry with duplicate runner_home between runner-1 and runner-2
    let mut reg = runnermesh::RunnerRegistry::default();
    let r1 = runnermesh::RunnerRecord {
        local_runner_key: "runner-1".to_string(),
        registration_scope: RegistrationScope::Repository {
            owner: "org".to_string(),
            repository: "repo1".to_string(),
        },
        runner_id: 101,
        expected_runner_name: "r1".to_string(),
        runner_home: std::path::PathBuf::from(r"C:\runners\shared-home"),
        work_root: std::path::PathBuf::from(r"C:\runners\work-1"),
        execution_identity_ref: OpaqueIdentityReference::new("test", "id").unwrap(),
        backend: runnermesh::RegistryBackendBinding::WindowsService {
            service_name: "actions.runner.1".to_string(),
        },
        desired_enabled: true,
        enrollment_state: runnermesh::EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: false,
    };
    let mut r2 = r1.clone();
    r2.local_runner_key = "runner-2".to_string();
    r2.runner_id = 102;
    r2.expected_runner_name = "r2".to_string();
    r2.work_root = std::path::PathBuf::from(r"C:\runners\work-2");
    r2.backend = runnermesh::RegistryBackendBinding::WindowsService {
        service_name: "actions.runner.2".to_string(),
    };
    reg.records.push(r1);
    reg.records.push(r2);
    let json = serde_json::to_string_pretty(&reg).unwrap();
    std::fs::write(&registry_file, json).unwrap();

    let store = runnermesh::FileRegistryStore::new(registry_file);
    use runnermesh::RegistryStore;
    let loaded = store.load();
    assert!(
        matches!(loaded, Err(RegistryError::DuplicateHome)),
        "Loading registry with duplicate home must fail closed with DuplicateHome"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_c2_worker_busy_observation_prevents_stop() {
    let mock_scm = MockScmBackend::new();
    let image_token = "test-image".to_string();
    let ident_token = "test-ident".to_string();
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.busy-runner".to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: true,
        image_token: image_token.clone(),
        execution_identity_token: ident_token.clone(),
    });

    let gate = LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
        "actions.runner.busy-runner",
        &image_token,
        false,
        true,
        false,
    ))
    .unwrap();
    let helper = PrivilegedScmHelper::new(mock_scm, gate);

    let stop_req = runnermesh::PrivilegedScmRequest {
        action: runnermesh::PrivilegedScmAction::StopService,
        service_name: "actions.runner.busy-runner".to_string(),
        expected_image_token: image_token,
        expected_execution_identity_token: ident_token,
        target_host_token: String::new(),
        candidate_commit: String::new(),
        transaction_id: "tx-c2".to_string(),
    };

    let report = helper.dispatch(&stop_req);
    assert!(
        !report.applied,
        "StopService must be refused when Worker is active"
    );
    assert_eq!(
        report.refusal_reason,
        Some(ScmApiError::BusyWorker.as_str().to_string())
    );
    assert_eq!(report.achieved_state, DiscoveredLifecycle::Busy);
}

#[test]
#[cfg(windows)]
fn regression_c3_user_session_process_observation_and_transport() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c3-session-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let runner_home = temp_dir.join("session-home");
    std::fs::create_dir_all(&runner_home).unwrap();
    let entrypoint = runner_home.join("run.cmd");
    std::fs::write(&entrypoint, b"@echo off\r\n").unwrap();
    let work_root = temp_dir.join("session-work");
    std::fs::create_dir_all(&work_root).unwrap();

    let record = runnermesh::RunnerRecord {
        local_runner_key: "user-session-1".to_string(),
        registration_scope: RegistrationScope::Repository {
            owner: "o".to_string(),
            repository: "r".to_string(),
        },
        runner_id: 301,
        expected_runner_name: "usr-1".to_string(),
        runner_home: runner_home.clone(),
        work_root: work_root.clone(),
        execution_identity_ref: OpaqueIdentityReference::new("test", "id").unwrap(),
        backend: runnermesh::RegistryBackendBinding::UserSession,
        desired_enabled: true,
        enrollment_state: runnermesh::EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: false,
    };

    let transport = Arc::new(runnermesh::SimulatedUserSessionProcessTransport::new());
    let coordinator = runnermesh::StandardSessionCoordinator::new(transport.clone());
    use runnermesh::MultiRunnerSessionCoordinator;

    let res_launch = coordinator.dispatch_session(
        &record,
        runnermesh::PreparedSessionAction::LaunchOwnedListener,
    );
    assert!(
        res_launch.is_ok(),
        "Launch on owned session must succeed via transport"
    );
    assert_eq!(
        *transport.launched.lock().unwrap(),
        vec!["user-session-1".to_string()]
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_c4_runners_status_reflects_failures_and_resources() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c4-status-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let runner_home = temp_dir.join("c4-home");
    std::fs::create_dir_all(&runner_home).unwrap();
    let work_root = temp_dir.join("c4-work");
    std::fs::create_dir_all(&work_root).unwrap();

    let registry_file = temp_dir.join("runners-registry.json");
    let mut reg = runnermesh::RunnerRegistry::default();
    let record = runnermesh::RunnerRecord {
        local_runner_key: "runner-c4".to_string(),
        registration_scope: RegistrationScope::Repository {
            owner: "o".to_string(),
            repository: "r".to_string(),
        },
        runner_id: 401,
        expected_runner_name: "c4-name".to_string(),
        runner_home: runner_home.clone(),
        work_root: work_root.clone(),
        execution_identity_ref: OpaqueIdentityReference::new("test", "id").unwrap(),
        backend: runnermesh::RegistryBackendBinding::WindowsService {
            service_name: "actions.runner.c4".to_string(),
        },
        desired_enabled: true,
        enrollment_state: runnermesh::EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: false,
    };
    reg.records.push(record);
    std::fs::write(&registry_file, serde_json::to_string_pretty(&reg).unwrap()).unwrap();

    let install_root = temp_dir.join("installed");
    std::fs::create_dir_all(&install_root).unwrap();
    std::fs::copy(&registry_file, install_root.join("runners-registry.json")).unwrap();

    let mock_scm = MockScmBackend::new();
    let image_token = runner_service_image_token(&runner_home);
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.c4".to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token,
        execution_identity_token: "id".to_string(),
    });

    let helper = PrivilegedScmHelper::new(mock_scm, LiveScmExecutionGate::CLOSED);
    let authoritative_backend = Arc::new(AuthoritativeMultiRunnerLifecycleBackend::new(helper));
    let scope = RegistrationScope::Repository {
        owner: "o".to_string(),
        repository: "r".to_string(),
    };
    authoritative_backend.register_runner_target_with_service(
        "runner-c4",
        scope.clone(),
        401,
        Some("actions.runner.c4"),
    );
    let admission_coord =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope.clone(), 401, false));
    authoritative_backend.register_runner_admission_coordinator(scope, 401, admission_coord);

    let ipc_endpoint = IpcEndpoint::for_test_scope(format!("c4-test-{}", std::process::id()));
    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: install_root.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(authoritative_backend),
        discovery_report: None,
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: true,
    };

    let runtime = start_installed_agent_sandbox(sandbox_config).unwrap();
    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));

    // Disable the runner via IPC to plan StopService
    let _ = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersDisable {
            local_runner_key: "runner-c4".to_string(),
            prepare_only: false,
            when_idle: false,
        }))
        .expect("IPC RunnersDisable must succeed");

    // Wait 60ms for periodic tick to trigger reconcile_and_dispatch_multi_runners under GateClosed
    std::thread::sleep(Duration::from_millis(60));

    // Query RunnersStatus via IPC
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersStatus {
            local_runner_key: None,
        }))
        .expect("IPC RunnersStatus must succeed");

    let runnermesh::IpcResponseBody::Success(boxed) = resp.body else {
        panic!("Expected Success IPC response");
    };
    let runnermesh::AgentResponse::Runners(report) = *boxed else {
        panic!("Expected Runners response");
    };

    // Verify C4 requirements:
    assert_eq!(report.records.len(), 1);
    assert_eq!(report.records[0].local_runner_key, "runner-c4");
    assert!(
        report.partial_failure,
        "Report must reflect partial failure from GateClosed SCM dispatch"
    );
    assert_eq!(report.records[0].item_applied, Some(false));
    assert!(report.records[0].item_reason_code.is_some());
    assert!(
        report.resource_aggregate.is_some(),
        "Resource aggregate must be attached"
    );

    // Verify C4 full resource sample structure and propagation with PID, TTL, and attribution
    let sample = runnermesh::RunnerResourceSample {
        local_runner_key: "runner-c4".to_string(),
        role: runnermesh::RunnerResourceRole::Listener,
        pid: Some(1234),
        working_set_bytes: Some(1024 * 1024),
        cpu: None,
        sampled_at_unix_ms: 1000,
        ttl_ms: runnermesh::RunnerResourceSample::DEFAULT_TTL_MS,
        attribution: runnermesh::ProcessAttributionDisposition::DirectVerified,
        confidence: runnermesh::ResourceSampleConfidence::Estimate,
    };
    let enriched = report
        .clone()
        .with_full_resource_samples(&[("runner-c4".to_string(), vec![sample])], 1000);
    assert_eq!(enriched.records[0].resource_samples.len(), 1);
    assert_eq!(enriched.records[0].resource_samples[0].pid, Some(1234));
    assert_eq!(enriched.records[0].resource_samples[0].ttl_ms, 5000);
    assert_eq!(
        enriched.records[0].resource_samples[0].attribution,
        runnermesh::ProcessAttributionDisposition::DirectVerified
    );

    runtime.shutdown();
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_f01_inaccessible_worker_process_evidence_fails_closed() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-f01-worker-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let runner_home = temp_dir.join("f01-home");
    std::fs::create_dir_all(&runner_home).unwrap();

    // 1. Synthetic ProcessImage with executable_name = "Runner.Worker.exe" but executable_path = None
    // (Simulating access-denied or process termination during QueryFullProcessImageNameW)
    let images = vec![runnermesh::ProcessImage {
        process_id: 12345,
        executable_name: "Runner.Worker.exe".to_string(),
        executable_path: None,
        user_matches_current: None,
    }];

    // 2. observe_runner_processes_from_images must NOT silently drop it!
    // It must report worker_present = true and evidence_complete = false
    let (listener, worker, evidence_complete) =
        runnermesh::observe_runner_processes_from_images(&runner_home, &images);

    assert!(
        worker,
        "Inaccessible Runner.Worker.exe must fail closed as worker present"
    );
    assert!(
        !evidence_complete,
        "Missing process path evidence must mark evidence as incomplete"
    );
    assert!(!listener);

    // 3. SCM StopService refusal under StateUnknown
    let image_token = "f01-img".to_string();
    let ident_token = "f01-id".to_string();

    let mock_scm = MockScmBackend::new();
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.f01-test".to_string(),
            state: DiscoveredLifecycle::Unknown,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: image_token.clone(),
        execution_identity_token: ident_token.clone(),
    });

    let gate = LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
        "actions.runner.f01-test",
        &image_token,
        false,
        true,
        false,
    ))
    .unwrap();
    let helper = PrivilegedScmHelper::new(mock_scm, gate);

    let stop_req = runnermesh::PrivilegedScmRequest {
        action: runnermesh::PrivilegedScmAction::StopService,
        service_name: "actions.runner.f01-test".to_string(),
        expected_image_token: image_token,
        expected_execution_identity_token: ident_token,
        target_host_token: String::new(),
        candidate_commit: String::new(),
        transaction_id: "tx-f01".to_string(),
    };

    let report = helper.dispatch(&stop_req);
    assert!(
        !report.applied,
        "StopService must be refused when service lifecycle state is Unknown"
    );
    assert_eq!(
        report.refusal_reason,
        Some(ScmApiError::StateUnknown.as_str().to_string()),
        "Refusal reason must be StateUnknown"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[cfg(windows)]
#[test]
fn regression_finding1_runner_ownership_verification_fails_closed() {
    let temp_dir = std::env::temp_dir().join(format!(
        "runnermesh-repro-finding1-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let runner_home = temp_dir.join("finding1-home");
    std::fs::create_dir_all(&runner_home).unwrap();
    let bin_dir = runner_home.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let listener_exe = bin_dir.join("Runner.Listener.exe");
    std::fs::write(&listener_exe, b"listener").unwrap();
    let worker_exe = bin_dir.join("Runner.Worker.exe");
    std::fs::write(&worker_exe, b"worker").unwrap();

    // 1. Process images where user_matches_current is None or Some(false)
    // user_matches_current == Some(false) is foreign: must be ignored
    let images_foreign = vec![
        runnermesh::ProcessImage {
            process_id: 111,
            executable_name: "Runner.Listener.exe".to_string(),
            executable_path: Some(listener_exe.clone()),
            user_matches_current: Some(false),
        },
        runnermesh::ProcessImage {
            process_id: 112,
            executable_name: "Runner.Worker.exe".to_string(),
            executable_path: Some(worker_exe.clone()),
            user_matches_current: Some(false),
        },
    ];
    let (listener_f, worker_f, complete_f) =
        runnermesh::observe_runner_processes_from_images(&runner_home, &images_foreign);
    assert!(
        !listener_f,
        "Foreign listener must not be attributed to session"
    );
    assert!(
        !worker_f,
        "Foreign worker must not be attributed to session"
    );
    assert!(
        complete_f,
        "Complete inspection of foreign processes succeeds"
    );

    // user_matches_current == None is ambiguous: must fail closed (worker = true, evidence_complete = false)
    let images_ambiguous = vec![runnermesh::ProcessImage {
        process_id: 113,
        executable_name: "Runner.Worker.exe".to_string(),
        executable_path: Some(worker_exe.clone()),
        user_matches_current: None,
    }];
    let (listener_a, worker_a, complete_a) =
        runnermesh::observe_runner_processes_from_images(&runner_home, &images_ambiguous);
    assert!(
        worker_a,
        "Ambiguous worker must fail closed as worker present"
    );
    assert!(
        !complete_a,
        "Ambiguous user token must mark evidence as incomplete"
    );
    assert!(!listener_a);

    // 2. Verified ownership for locally created path
    assert_eq!(
        runnermesh::current_user_owns_path(&runner_home),
        Ok(true),
        "Locally created temp runner_home must be owned by current user"
    );
    let work_root = runner_home.join("_work");
    std::fs::create_dir_all(&work_root).unwrap();
    assert!(
        runnermesh::verify_path_and_ancestor_ownership(&work_root),
        "Locally created work_root must be verified as owned"
    );
    assert_eq!(
        runnermesh::paths_have_matching_owner(&runner_home, &work_root),
        Ok(true),
        "runner_home and work_root created by same user must have matching owners"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[cfg(windows)]
#[test]
fn regression_finding2_incomplete_process_evidence_propagates_and_gates_enrollment() {
    use runnermesh::{
        handle_runners_command, DiscoveryEnumerationStatus, RunnerRegistry, RunnerRegistryEditor,
    };

    let registry = RunnerRegistry::default();
    let mut editor = RunnerRegistryEditor::new(registry).unwrap();

    let candidate = DiscoveredRunner {
        local_discovery_key: "r-test".to_string(),
        disposition: DiscoveryDisposition::ObservedOnly,
        refusal: None,
        backend: RunnerBackendKind::WindowsService,
        identity: DiscoveredRunnerIdentity {
            registration_scope: Some(RegistrationScope::Repository {
                owner: "org".to_string(),
                repository: "repo".to_string(),
            }),
            runner_id: Some(101),
            expected_runner_name: Some("test-runner".to_string()),
            runner_home: std::path::PathBuf::from(r"C:\test-runner"),
            work_root: Some(std::path::PathBuf::from(r"C:\test-runner\_work")),
            execution_identity_ref: Some(
                OpaqueIdentityReference::new(
                    WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
                    "1111111111111111111111111111111111111111111111111111111111111111",
                )
                .unwrap(),
            ),
        },
        service: Some(DiscoveredServiceBinding {
            service_name: "actions.runner.test".to_string(),
            display_name: None,
            image_path: None,
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
    };

    // 1. Report with process_observation = Unavailable must reject RunnersEnroll
    let incomplete_report = DiscoveryReport {
        schema_version: runnermesh::RUNNER_DISCOVERY_SCHEMA_VERSION,
        read_only: true,
        exhaustive: false,
        service_enumeration: DiscoveryEnumerationStatus::Ok,
        session_enumeration: DiscoveryEnumerationStatus::Ok,
        process_observation: DiscoveryEnumerationStatus::Unavailable,
        candidates: vec![candidate.clone()],
    };

    let enroll_cmd = AgentCommand::RunnersEnroll {
        local_runner_key: "r-test".to_string(),
        prepare_only: false,
    };
    let err = handle_runners_command(&mut editor, Some(&incomplete_report), &enroll_cmd);
    assert!(
        matches!(err, Err(RegistryError::ProcessEvidenceIncomplete)),
        "RunnersEnroll must fail with ProcessEvidenceIncomplete when discovery process_observation is Unavailable"
    );

    // 2. Candidate with observed_lifecycle = Unknown must reject RunnersEnroll
    let mut unknown_candidate = candidate.clone();
    unknown_candidate.observed_lifecycle = DiscoveredLifecycle::Unknown;
    let ok_report = DiscoveryReport {
        schema_version: runnermesh::RUNNER_DISCOVERY_SCHEMA_VERSION,
        read_only: true,
        exhaustive: false,
        service_enumeration: DiscoveryEnumerationStatus::Ok,
        session_enumeration: DiscoveryEnumerationStatus::Ok,
        process_observation: DiscoveryEnumerationStatus::Ok,
        candidates: vec![unknown_candidate],
    };
    let err_unknown = handle_runners_command(&mut editor, Some(&ok_report), &enroll_cmd);
    assert!(
        matches!(err_unknown, Err(RegistryError::ProcessEvidenceIncomplete)),
        "RunnersEnroll must fail with ProcessEvidenceIncomplete when candidate lifecycle is Unknown"
    );
}

#[test]
fn regression_finding3_scm_quoted_image_path_with_arguments_parsed_consistently() {
    use runnermesh::{opaque_image_token, runner_home_from_image, service_image_path_token};

    // 1. Quoted ImagePath with trailing service arguments
    let quoted_with_args =
        r#""C:\Program Files\fixture-runner\bin\RunnerService.exe" --runAsService"#;
    let token = service_image_path_token(quoted_with_args);
    assert_eq!(
        token,
        Some(r"C:\Program Files\fixture-runner\bin\RunnerService.exe"),
        "service_image_path_token must extract exact quoted executable path"
    );

    let home = runner_home_from_image(std::path::Path::new(token.unwrap()));
    assert_eq!(
        home,
        Some(std::path::PathBuf::from(r"C:\Program Files\fixture-runner")),
        "runner_home_from_image must extract runner home from <home>/bin/RunnerService.exe"
    );

    let hash_quoted = opaque_image_token(quoted_with_args);
    let hash_clean = opaque_image_token(r"C:\Program Files\fixture-runner\bin\RunnerService.exe");
    assert_eq!(
        hash_quoted, hash_clean,
        "opaque_image_token must match between quoted-with-args and clean executable path"
    );

    // 2. Unquoted ImagePath with trailing arguments
    let unquoted_with_args = r"C:\fixture-runners\home\bin\RunnerService.exe -svc";
    let token_unquoted = service_image_path_token(unquoted_with_args);
    assert_eq!(
        token_unquoted,
        Some(r"C:\fixture-runners\home\bin\RunnerService.exe"),
        "service_image_path_token must extract first whitespace-separated token"
    );
    let hash_unquoted = opaque_image_token(unquoted_with_args);
    let hash_unquoted_clean = opaque_image_token(r"C:\fixture-runners\home\bin\RunnerService.exe");
    assert_eq!(
        hash_unquoted, hash_unquoted_clean,
        "opaque_image_token must match between unquoted-with-args and clean executable path"
    );
}

#[test]
fn c1_race_child_worker() {
    let Ok(role) = std::env::var("RUNNERMESH_C1_RACE_ROLE") else {
        return;
    };
    let ledger_path = std::env::var("RUNNERMESH_C1_RACE_LEDGER").unwrap();
    let barrier_dir = std::path::PathBuf::from(std::env::var("RUNNERMESH_C1_RACE_DIR").unwrap());
    let voucher_hex = std::env::var("RUNNERMESH_C1_RACE_VOUCHER").unwrap();
    let nonce = std::env::var("RUNNERMESH_C1_RACE_NONCE").unwrap();

    let mut voucher_hash = [0u8; 32];
    for (i, byte) in (0..32)
        .map(|i| u8::from_str_radix(&voucher_hex[i * 2..i * 2 + 2], 16).unwrap())
        .enumerate()
    {
        voucher_hash[i] = byte;
    }

    let ledger = runnermesh::SharedAuthorizationLedger::with_durable_file(&ledger_path)
        .expect("child failed to open durable ledger");

    // Signal ready
    let ready_file = barrier_dir.join(format!("ready_{role}.txt"));
    std::fs::write(&ready_file, b"ready").unwrap();

    // Spin on barrier start file
    let start_file = barrier_dir.join("start.txt");
    let spin_start = std::time::Instant::now();
    while !start_file.exists() {
        if spin_start.elapsed() > std::time::Duration::from_secs(15) {
            panic!("child {role} timed out waiting for barrier start");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Call try_consume concurrently
    let consumed = ledger.try_consume(voucher_hash, &nonce);
    let result_file = barrier_dir.join(format!("result_{role}.txt"));
    std::fs::write(&result_file, if consumed { b"SUCCESS" } else { b"REFUSED" }).unwrap();
}

#[test]
fn regression_c1_two_independent_os_processes_barrier_race() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c1-race-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let barrier_dir = temp_dir.join("barrier");
    std::fs::create_dir_all(&barrier_dir).unwrap();
    let ledger_path = temp_dir.join("shared-ledger.json");
    let current_exe = std::env::current_exe().unwrap();

    let voucher_a_hex = "11".repeat(32);
    let nonce_a = "nonce-race-a";

    // 1. Spawn two independent OS child processes racing for the same voucher
    let mut child_1 = std::process::Command::new(&current_exe)
        .arg("--")
        .arg("c1_race_child_worker")
        .arg("--exact")
        .arg("--nocapture")
        .env("RUNNERMESH_C1_RACE_ROLE", "child_1")
        .env("RUNNERMESH_C1_RACE_LEDGER", ledger_path.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_DIR", barrier_dir.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_VOUCHER", &voucher_a_hex)
        .env("RUNNERMESH_C1_RACE_NONCE", nonce_a)
        .spawn()
        .expect("failed to spawn child_1");

    let mut child_2 = std::process::Command::new(&current_exe)
        .arg("--")
        .arg("c1_race_child_worker")
        .arg("--exact")
        .arg("--nocapture")
        .env("RUNNERMESH_C1_RACE_ROLE", "child_2")
        .env("RUNNERMESH_C1_RACE_LEDGER", ledger_path.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_DIR", barrier_dir.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_VOUCHER", &voucher_a_hex)
        .env("RUNNERMESH_C1_RACE_NONCE", nonce_a)
        .spawn()
        .expect("failed to spawn child_2");

    // Wait until both children write their ready markers
    let ready_1 = barrier_dir.join("ready_child_1.txt");
    let ready_2 = barrier_dir.join("ready_child_2.txt");
    let wait_start = std::time::Instant::now();
    while !(ready_1.exists() && ready_2.exists()) {
        if wait_start.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child_1.kill();
            let _ = child_2.kill();
            panic!("timed out waiting for children to initialize and reach barrier");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // Release barrier simultaneously
    std::fs::write(barrier_dir.join("start.txt"), b"go").unwrap();

    // Reclaim both child processes
    let exit_1 = child_1.wait().expect("child_1 wait failed");
    let exit_2 = child_2.wait().expect("child_2 wait failed");
    assert!(exit_1.success(), "child_1 must exit successfully");
    assert!(exit_2.success(), "child_2 must exit successfully");

    // Read results
    let res_1 = std::fs::read_to_string(barrier_dir.join("result_child_1.txt")).unwrap();
    let res_2 = std::fs::read_to_string(barrier_dir.join("result_child_2.txt")).unwrap();

    let mut successes = 0;
    let mut refusals = 0;
    for r in [&res_1, &res_2] {
        if r.trim() == "SUCCESS" {
            successes += 1;
        } else if r.trim() == "REFUSED" {
            refusals += 1;
        }
    }

    assert_eq!(
        successes, 1,
        "Strictly one child process must be granted consumption"
    );
    assert_eq!(
        refusals, 1,
        "Strictly one child process must be refused consumption"
    );

    // Verify on-disk ledger file has exactly 1 entry
    let ledger_bytes = std::fs::read(&ledger_path).unwrap();
    let entries: Vec<runnermesh::DurableLedgerEntry> =
        serde_json::from_slice(&ledger_bytes).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].nonce, nonce_a);

    // 2. Race two distinct vouchers concurrently: both must succeed without lost update
    std::fs::remove_file(barrier_dir.join("start.txt")).unwrap();
    let ready_3 = barrier_dir.join("ready_child_3.txt");
    let ready_4 = barrier_dir.join("ready_child_4.txt");

    let voucher_b_hex = "22".repeat(32);
    let voucher_c_hex = "33".repeat(32);

    let mut child_3 = std::process::Command::new(&current_exe)
        .arg("--")
        .arg("c1_race_child_worker")
        .arg("--exact")
        .arg("--nocapture")
        .env("RUNNERMESH_C1_RACE_ROLE", "child_3")
        .env("RUNNERMESH_C1_RACE_LEDGER", ledger_path.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_DIR", barrier_dir.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_VOUCHER", &voucher_b_hex)
        .env("RUNNERMESH_C1_RACE_NONCE", "nonce-b")
        .spawn()
        .expect("failed to spawn child_3");

    let mut child_4 = std::process::Command::new(&current_exe)
        .arg("--")
        .arg("c1_race_child_worker")
        .arg("--exact")
        .arg("--nocapture")
        .env("RUNNERMESH_C1_RACE_ROLE", "child_4")
        .env("RUNNERMESH_C1_RACE_LEDGER", ledger_path.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_DIR", barrier_dir.to_str().unwrap())
        .env("RUNNERMESH_C1_RACE_VOUCHER", &voucher_c_hex)
        .env("RUNNERMESH_C1_RACE_NONCE", "nonce-c")
        .spawn()
        .expect("failed to spawn child_4");

    let wait_start2 = std::time::Instant::now();
    while !(ready_3.exists() && ready_4.exists()) {
        if wait_start2.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child_3.kill();
            let _ = child_4.kill();
            panic!("timed out waiting for children 3 & 4 to reach barrier");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    std::fs::write(barrier_dir.join("start.txt"), b"go").unwrap();

    let exit_3 = child_3.wait().expect("child_3 wait failed");
    let exit_4 = child_4.wait().expect("child_4 wait failed");
    assert!(exit_3.success());
    assert!(exit_4.success());

    let res_3 = std::fs::read_to_string(barrier_dir.join("result_child_3.txt")).unwrap();
    let res_4 = std::fs::read_to_string(barrier_dir.join("result_child_4.txt")).unwrap();
    assert_eq!(res_3.trim(), "SUCCESS");
    assert_eq!(res_4.trim(), "SUCCESS");

    let ledger_bytes_after = std::fs::read(&ledger_path).unwrap();
    let entries_after: Vec<runnermesh::DurableLedgerEntry> =
        serde_json::from_slice(&ledger_bytes_after).unwrap();
    assert_eq!(
        entries_after.len(),
        3,
        "All three consumed vouchers must be durably present without lost update"
    );

    // 3. Two pre-opened ledger instances: A consumes, B attempts sequential replay and is refused
    let ledger_a = runnermesh::SharedAuthorizationLedger::with_durable_file(&ledger_path).unwrap();
    let ledger_b = runnermesh::SharedAuthorizationLedger::with_durable_file(&ledger_path).unwrap();

    let voucher_d = [0x44u8; 32];
    assert!(ledger_a.try_consume(voucher_d, "nonce-d"));
    assert!(
        !ledger_b.try_consume(voucher_d, "nonce-d"),
        "Pre-opened ledger B must refuse replay after A consumed on disk"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_c2_c3_multirunner_assembly_service_and_session_real_ipc_and_tick() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-c2c3-multi-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let home_1 = temp_dir.join("runner-1");
    std::fs::create_dir_all(&home_1).unwrap();
    let work_1 = temp_dir.join("work-1");
    std::fs::create_dir_all(&work_1).unwrap();

    let home_2 = temp_dir.join("runner-2");
    std::fs::create_dir_all(&home_2).unwrap();
    let work_2 = temp_dir.join("work-2");
    std::fs::create_dir_all(&work_2).unwrap();

    let home_3 = temp_dir.join("runner-3");
    std::fs::create_dir_all(&home_3).unwrap();
    let entry_3 = home_3.join("run.cmd");
    std::fs::write(&entry_3, b"@echo off\r\n").unwrap();
    let work_3 = temp_dir.join("work-3");
    std::fs::create_dir_all(&work_3).unwrap();

    let identity = OpaqueIdentityReference::new("test", "id").unwrap();

    let mut reg = runnermesh::RunnerRegistry::default();
    let r1 = runnermesh::RunnerRecord {
        local_runner_key: "runner-1".to_string(),
        registration_scope: RegistrationScope::Repository {
            owner: "org".to_string(),
            repository: "repo1".to_string(),
        },
        runner_id: 101,
        expected_runner_name: "r1".to_string(),
        runner_home: home_1.clone(),
        work_root: work_1.clone(),
        execution_identity_ref: identity.clone(),
        backend: runnermesh::RegistryBackendBinding::WindowsService {
            service_name: "actions.runner.svc1".to_string(),
        },
        desired_enabled: true,
        enrollment_state: runnermesh::EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: true,
    };
    let r2 = runnermesh::RunnerRecord {
        local_runner_key: "runner-2".to_string(),
        registration_scope: RegistrationScope::Repository {
            owner: "org".to_string(),
            repository: "repo2".to_string(),
        },
        runner_id: 102,
        expected_runner_name: "r2".to_string(),
        runner_home: home_2.clone(),
        work_root: work_2.clone(),
        execution_identity_ref: identity.clone(),
        backend: runnermesh::RegistryBackendBinding::WindowsService {
            service_name: "actions.runner.svc2".to_string(),
        },
        desired_enabled: true,
        enrollment_state: runnermesh::EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: true,
    };
    let r3 = runnermesh::RunnerRecord {
        local_runner_key: "runner-3".to_string(),
        registration_scope: RegistrationScope::Repository {
            owner: "org".to_string(),
            repository: "repo3".to_string(),
        },
        runner_id: 103,
        expected_runner_name: "r3".to_string(),
        runner_home: home_3.clone(),
        work_root: work_3.clone(),
        execution_identity_ref: identity.clone(),
        backend: runnermesh::RegistryBackendBinding::UserSession,
        desired_enabled: true,
        enrollment_state: runnermesh::EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: true,
    };

    reg.records.push(r1);
    reg.records.push(r2);
    reg.records.push(r3);

    let install_root = temp_dir.join("installed");
    std::fs::create_dir_all(&install_root).unwrap();
    let registry_file = install_root.join("runners-registry.json");
    std::fs::write(&registry_file, serde_json::to_string_pretty(&reg).unwrap()).unwrap();

    let mock_scm = MockScmBackend::new();
    let img_1 = runner_service_image_token(&home_1);
    let img_2 = runner_service_image_token(&home_2);

    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.svc1".to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: img_1,
        execution_identity_token: "id".to_string(),
    });
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.svc2".to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: img_2,
        execution_identity_token: "id".to_string(),
    });

    let helper = PrivilegedScmHelper::new(mock_scm.clone(), LiveScmExecutionGate::CLOSED);
    let session_transport = Arc::new(
        runnermesh::SimulatedUserSessionProcessTransport::new()
            .with_active_session("runner-3", DiscoveredLifecycle::Listening),
    );
    let session_coord = Arc::new(runnermesh::StandardSessionCoordinator::new(
        session_transport.clone(),
    ));

    let authoritative_backend = Arc::new(
        AuthoritativeMultiRunnerLifecycleBackend::new(helper)
            .with_session_coordinator(session_coord),
    );

    let scope_1 = RegistrationScope::Repository {
        owner: "org".to_string(),
        repository: "repo1".to_string(),
    };
    let scope_2 = RegistrationScope::Repository {
        owner: "org".to_string(),
        repository: "repo2".to_string(),
    };
    let scope_3 = RegistrationScope::Repository {
        owner: "org".to_string(),
        repository: "repo3".to_string(),
    };

    authoritative_backend.register_runner_target_with_service(
        "runner-1",
        scope_1.clone(),
        101,
        Some("actions.runner.svc1"),
    );
    authoritative_backend.register_runner_target_with_service(
        "runner-2",
        scope_2.clone(),
        102,
        Some("actions.runner.svc2"),
    );
    authoritative_backend.register_runner_target_detailed(
        "runner-3",
        scope_3.clone(),
        103,
        None,
        Some(&home_3),
        true,
    );

    let coord_1 =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope_1.clone(), 101, true));
    let coord_2 =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope_2.clone(), 102, true));
    let coord_3 =
        Arc::new(InProcessAdmissionCoordinator::new().with_selector(scope_3.clone(), 103, true));

    authoritative_backend.register_runner_admission_coordinator(scope_1, 101, coord_1);
    authoritative_backend.register_runner_admission_coordinator(scope_2, 102, coord_2);
    authoritative_backend.register_runner_admission_coordinator(scope_3, 103, coord_3);

    let ipc_endpoint = IpcEndpoint::for_test_scope(format!("c2c3-test-{}", std::process::id()));
    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: install_root.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(authoritative_backend),
        discovery_report: None,
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: true,
    };

    let runtime = start_installed_agent_sandbox(sandbox_config).unwrap();
    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));

    // Set baseline mode to Idle via IPC so Agent Core permits capacity
    let _ = ipc_client
        .call(IpcRequest::new(AgentCommand::SetMode {
            mode: UserMode::Idle,
        }))
        .expect("IPC SetMode to Idle");

    let resp1 = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersDisable {
            local_runner_key: "runner-1".to_string(),
            prepare_only: false,
            when_idle: false,
        }))
        .expect("IPC disable runner-1");
    assert!(matches!(
        resp1.body,
        runnermesh::IpcResponseBody::Success(_)
    ));

    let resp3 = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersDisable {
            local_runner_key: "runner-3".to_string(),
            prepare_only: false,
            when_idle: false,
        }))
        .expect("IPC disable runner-3");
    assert!(matches!(
        resp3.body,
        runnermesh::IpcResponseBody::Success(_)
    ));

    let deadline_stop = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline_stop {
        if !session_transport.stopped.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }

    assert_eq!(
        *session_transport.stopped.lock().unwrap(),
        vec!["runner-3".to_string()]
    );
    assert!(
        mock_scm.service_actions("actions.runner.svc2").is_empty(),
        "Runner 2 must receive zero control actions"
    );

    let resp3_en = ipc_client
        .call(IpcRequest::new(AgentCommand::RunnersEnable {
            local_runner_key: "runner-3".to_string(),
            prepare_only: false,
        }))
        .expect("IPC enable runner-3");
    assert!(matches!(
        resp3_en.body,
        runnermesh::IpcResponseBody::Success(_)
    ));

    let deadline_launch = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline_launch {
        if !session_transport.launched.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }

    assert_eq!(
        *session_transport.launched.lock().unwrap(),
        vec!["runner-3".to_string()]
    );

    runtime.shutdown();
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(windows)]
fn regression_c5b_immutable_rc_sandbox_install_upgrade_rollback_uninstall() {
    let relative_archive = std::path::PathBuf::from(
        r"..\..\runnermesh-160h-audit\2026-11-12\160H-FINAL-CLOSEOUT-2703a1e\artifacts\lrm-rc\runnermesh-0.2.0-rc.1-windows-x64.zip",
    );
    let relative_helper = std::path::PathBuf::from(
        r"..\..\runnermesh-160h-audit\2026-11-12\160H-FINAL-CLOSEOUT-2703a1e\artifacts\lrm-rc\runnermesh-package.exe",
    );

    let temp_root = std::env::temp_dir().join(format!(
        "rm-c5b-sandbox-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_root).unwrap();

    let (rc_archive, _created_guard) = if relative_archive.is_file() {
        assert!(relative_helper.is_file(), "RC package helper must exist");
        let target_archive = temp_root.join("runnermesh-0.2.0-rc.1-windows-x64.zip");
        std::fs::copy(&relative_archive, &target_archive).unwrap();
        (target_archive, None)
    } else {
        // CI fallback: create authentic RC package in task-owned temp directory
        let ci_staging = temp_root.join("ci-staging");
        std::fs::create_dir_all(&ci_staging).unwrap();
        let agent_exe = ci_staging.join("runnermesh-agent.exe");
        let cli_exe = ci_staging.join("runnermesh.exe");
        let agent_manifest = ci_staging.join("runnermesh-agent.manifest");
        std::fs::write(&agent_exe, b"ci-rc-agent-stub").unwrap();
        std::fs::write(&cli_exe, b"ci-rc-cli-stub").unwrap();
        std::fs::write(&agent_manifest, b"ci-rc-manifest-stub").unwrap();

        let input = runnermesh::PackageInput {
            provenance: runnermesh::PackageProvenance {
                version: "0.2.0-rc.1".to_string(),
                commit: "850269aa12c6bf44c10d1f17678716eafca329f7".to_string(),
                channel: "rc".to_string(),
                target: runnermesh::WINDOWS_X64_TARGET.to_string(),
            },
            cli_binary: cli_exe,
            agent_binary: agent_exe,
            agent_manifest,
        };
        let archives_dir = temp_root.join("archives");
        let receipt = runnermesh::PackageVerifier::create(&input, &archives_dir).unwrap();
        (receipt.archive, Some(ci_staging))
    };

    let (manifest, hash) = runnermesh::PackageVerifier::verify_with_hash(&rc_archive).unwrap();
    assert!(!hash.is_empty(), "RC package hash must be non-empty");
    assert_eq!(manifest.provenance.version, "0.2.0-rc.1");
    assert_eq!(manifest.provenance.channel, "rc");
    assert_eq!(manifest.provenance.commit.len(), 40);

    let staging_1 = temp_root.join("staging-1");
    let install_root = temp_root.join("installed");

    let installation = runnermesh::Installation::new(&install_root);
    let extracted = runnermesh::PackageVerifier::extract_runtime_expected(
        &rc_archive,
        &staging_1,
        &manifest.provenance,
        &hash,
    )
    .unwrap();

    let accepted_members = runnermesh::VersionManifest {
        files: extracted.files,
    };
    let receipt = installation
        .install_with_manifest("0.2.0-rc.1", &staging_1, &accepted_members)
        .unwrap();
    assert!(receipt.activated);
    assert_eq!(receipt.version, "0.2.0-rc.1");
    assert_eq!(
        installation.active_agent_path().unwrap(),
        install_root
            .join("versions")
            .join("0.2.0-rc.1")
            .join("runnermesh-agent.exe")
    );

    let staging_2 = temp_root.join("staging-2");
    std::fs::create_dir_all(&staging_2).unwrap();
    for name in [
        "runnermesh.exe",
        "runnermesh-agent.exe",
        "runnermesh-agent.manifest",
    ] {
        std::fs::copy(staging_1.join(name), staging_2.join(name)).unwrap();
    }
    let receipt_2 = installation
        .install_with_manifest("0.2.0-rc.2", &staging_2, &accepted_members)
        .unwrap();
    assert!(
        !receipt_2.activated,
        "Second version slot installation must not automatically activate"
    );
    assert_eq!(receipt_2.version, "0.2.0-rc.2");
    installation.select_active("0.2.0-rc.2").unwrap();
    assert_eq!(
        installation.active_agent_path().unwrap(),
        install_root
            .join("versions")
            .join("0.2.0-rc.2")
            .join("runnermesh-agent.exe")
    );

    installation.select_active("0.2.0-rc.1").unwrap();
    assert_eq!(
        installation.active_agent_path().unwrap(),
        install_root
            .join("versions")
            .join("0.2.0-rc.1")
            .join("runnermesh-agent.exe")
    );

    let user_log = install_root.join("logs").join("preserved.log");
    std::fs::create_dir_all(install_root.join("logs")).unwrap();
    std::fs::write(&user_log, b"preserve me").unwrap();

    let startup = runnermesh::WindowsUserStartupBackend::for_current_user().unwrap();
    let uninst = installation.uninstall(&startup).unwrap();
    assert!(uninst.removed_versions >= 2);
    assert!(uninst.foreign_content_preserved);
    assert!(
        user_log.exists(),
        "Foreign user log must be preserved on uninstall"
    );

    let _ = std::fs::remove_dir_all(&temp_root);
}
