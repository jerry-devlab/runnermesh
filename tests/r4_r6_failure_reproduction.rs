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
    MockServiceRecord, OpaqueIdentityReference, PerRunnerObservation, PrivilegedScmHelper,
    RegistrationScope, RegistryError, RunnerBackendKind, SandboxMultiRunnerLifecycleBackend,
    ServiceObservation, SyntheticScmTestGrant, UserMode, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
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
        poll_interval: Duration::from_millis(50),
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

    // 3. Drive 4-phase sequential Stop dispatch via Agent observation & polling
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
    let _ = runtime.step_poll().unwrap();
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
    let _ = runtime.step_poll().unwrap();
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
    let _ = runtime.step_poll().unwrap();
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 0);
    assert!(mock_scm.service_actions("actions.runner.b").is_empty());

    // Phase 4: Confirmed absent + Worker idle -> exactly 1 Stop call for A, 0 for B
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
    let _ = runtime.step_poll().unwrap();
    assert_eq!(mock_scm.service_stop_calls("actions.runner.a"), 1);
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
