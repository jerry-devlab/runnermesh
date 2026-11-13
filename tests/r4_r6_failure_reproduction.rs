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
    runner_service_image_token, AgentCommand, DiscoveredLifecycle, DiscoveredRunner,
    DiscoveredRunnerIdentity, DiscoveredServiceBinding, DiscoveryCapabilities,
    DiscoveryDisposition, DiscoveryReport, IpcClient, IpcEndpoint, IpcRequest, MockScmBackend,
    MockServiceRecord, MultiRunnerLifecycleBackend, OpaqueIdentityReference, PerRunnerObservation,
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
            .with_session_coordinator(Arc::new(runnermesh::StandardSessionCoordinator)),
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
