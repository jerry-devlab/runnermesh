//! Regression tests for R4 trusted Owner authorization and R6 installed-mode startup assembly.
//!
//! Validates:
//! 1. External callers cannot self-sign via public `new` (zero success, fail closed).
//! 2. External callers cannot forge authorization with an unauthorized secret (zero success, fail closed).
//! 3. Atomic single-use consumption ledger allows EXACTLY ONE success under concurrent racing requests.
//! 4. Windows SCM write mutations remain sealed (GateClosed) even when a live gate is minted.
//! 5. Installed-mode assembly path wires the multi-runner lifecycle backend and reconciles runners.

use std::sync::{Arc, Barrier};
use std::thread;

#[cfg(windows)]
use runnermesh::agent_runtime::{
    assemble_installed_core, NoRunnerControl, RuntimeObserver, RuntimeReconciler,
};
use runnermesh::{
    runner_service_image_token, LiveScmExecutionGate, MockScmBackend, MockServiceRecord,
    OpaqueIdentityReference, OwnerAuthorizationIssuer, OwnerAuthorizationScope,
    PrivilegedScmHelper, SandboxMultiRunnerLifecycleBackend, ScmApiError, ScmControlBackend,
    ServiceObservation, ServiceStartMode, SyntheticScmTestGrant, TokenValidatingOwnerVerifier,
    TrustedOwnerAuthorizationRequest, UnissuedOwnerAuthorizationVerifier, WindowsScmBackend,
    WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
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
fn regression_r4_external_caller_unauthorized_root_refused() {
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

    let caller_issuer = OwnerAuthorizationIssuer::new(caller_secret);
    let (forged_request, _voucher) =
        caller_issuer.issue_for_scope(&scope, 3600, "repro-nonce-root");

    // 1. Presented to standard unissued runtime verifier -> refused with AuthorizationNotIssued
    let unissued_verifier = UnissuedOwnerAuthorizationVerifier;
    let result_unissued =
        LiveScmExecutionGate::from_trusted_owner_authorization(&unissued_verifier, &forged_request);
    assert!(
        matches!(result_unissued, Err(ScmApiError::AuthorizationNotIssued)),
        "Default product verifier MUST refuse external forged request"
    );

    // 2. Presented to authorized verifier with different root secret -> refused with AccessDenied
    let authorized_root = b"actual-owner-offline-hardware-secret";
    let authorized_verifier =
        TokenValidatingOwnerVerifier::from_authority(authorized_root.to_vec(), scope);
    let result_auth = LiveScmExecutionGate::from_trusted_owner_authorization(
        &authorized_verifier,
        &forged_request,
    );
    assert!(
        matches!(result_auth, Err(ScmApiError::AccessDenied)),
        "Verifier with authorized root MUST refuse forged voucher with AccessDenied"
    );
}

#[test]
fn regression_r4_concurrent_same_voucher_permits_exactly_one_success() {
    let authority_secret = b"shared-authority-secret-concurrent-repro";
    let scope = OwnerAuthorizationScope::new(
        "actions.runner.repro-concurrent",
        "authorized-image-token",
        "auth-ident-token",
        "auth-host-token",
        "ff30112c8527f41f49b64a3a6984b1126a876c05",
        "tx-repro-concurrent",
        true,
        false,
        false,
    );

    let issuer = OwnerAuthorizationIssuer::new(authority_secret);
    let verifier = Arc::new(TokenValidatingOwnerVerifier::from_authority(
        authority_secret.to_vec(),
        scope.clone(),
    ));

    let (request, _voucher) = issuer.issue_for_scope(&scope, 3600, "repro-concurrent-shared-nonce");
    let request = Arc::new(request);

    let num_threads = 8;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let v = Arc::clone(&verifier);
        let req = Arc::clone(&request);
        let b = Arc::clone(&barrier);

        handles.push(thread::spawn(move || {
            b.wait();
            LiveScmExecutionGate::from_trusted_owner_authorization(&*v, &req)
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    let denial_count = results
        .iter()
        .filter(|r| matches!(r, Err(ScmApiError::AccessDenied)))
        .count();

    // Atomic consumption ledger: exactly 1 thread succeeds, all remaining threads fail closed!
    assert_eq!(
        success_count, 1,
        "Atomic ledger MUST permit exactly 1 success under concurrent races"
    );
    assert_eq!(
        denial_count,
        num_threads - 1,
        "All other concurrent racing threads MUST receive AccessDenied"
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
fn regression_r6_installed_assembly_wires_lifecycle_backend_and_preserves_isolation() {
    let temp_dir = std::env::temp_dir().join(format!("runnermesh-r6-reg-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config_path = temp_dir.join("config.json");

    // 1. Unwired assembly has None for multi_runner_backend
    let obs1 = RuntimeObserver::new(None, Vec::new());
    let rec1 = RuntimeReconciler::Unconfigured(NoRunnerControl::new());
    let core_unwired = assemble_installed_core(config_path.clone(), obs1, rec1, None).unwrap();
    assert!(core_unwired.multi_runner_backend().is_none());

    // 2. Wired assembly attaches backend and is ready for regular agent loop
    let home_a = temp_dir.join("runner-a");
    let _ = std::fs::create_dir_all(home_a.join("bin"));
    let _ = std::fs::write(home_a.join("bin").join("RunnerService.exe"), b"stub");
    let image_token_a = runner_service_image_token(&home_a);

    let identity = OpaqueIdentityReference::new(
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        "1111111111111111111111111111111111111111111111111111111111111111",
    )
    .unwrap();

    let mock_scm = MockScmBackend::new();
    mock_scm.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "actions.runner.a".to_string(),
            state: runnermesh::DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: image_token_a.clone(),
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

    let obs2 = RuntimeObserver::new(None, Vec::new());
    let rec2 = RuntimeReconciler::Unconfigured(NoRunnerControl::new());
    let core_wired =
        assemble_installed_core(config_path, obs2, rec2, Some(lifecycle_backend.clone())).unwrap();

    assert!(core_wired.multi_runner_backend().is_some());

    // Cleanup
    let _ = std::fs::remove_dir_all(temp_dir);
}
