//! Crate-external integration tests for R4 trusted Owner authorization boundary.
//!
//! Validates the complete P03 authorization boundary and negative matrix
//! from the perspective of an external caller importing the `runnermesh` library.
//!
//! In ordinary builds and for external crate consumers:
//! - Authority roots are UNISSUED in code (no hardcoded test roots exist).
//! - Public `UnissuedOwnerAuthorizationVerifier` unconditionally fails closed (`AuthorizationNotIssued`).
//! - Public `TokenValidatingOwnerVerifier::new(...)` constructs an unissued verifier (no authority secret),
//!   which unconditionally fails closed (`AuthorizationNotIssued`).
//! - External callers have ZERO public methods or constructors to supply authority secrets or mint `TrustedLive`.
//! - Forged, self-selected, or tampered vouchers are rejected fail-closed.
//! - Sensitive owner material is redacted from Debug representations (`[REDACTED]`).
//! - Defense-in-depth: Live Windows SCM backend writes remain sealed (`GateClosed`).

use std::path::PathBuf;
use std::sync::Arc;

use runnermesh::{
    capture_restore_plan, AchievedServiceControl, DiscoveredLifecycle, DiscoveredRunner,
    DiscoveredRunnerIdentity, DiscoveredServiceBinding, DiscoveryCapabilities,
    DiscoveryDisposition, FakeCredentialManagerAuthorityProvider, FakeGithubAdmissionBackend,
    LiveScmExecutionGate, MockClock, MockScmBackend, MockServiceRecord, OpaqueIdentityReference,
    OwnerAuthoritySecretProvider, OwnerAuthorizationIssuer, OwnerAuthorizationScope,
    RegistrationScope, RegistryError, RunnerBackendKind, RunnerRegistryEditor, ScmActionKind,
    ScmApiError, ScmControlBackend, ScmControlExecutor, ScmTransactionStage, ServiceObservation,
    ServiceStartMode, SyntheticScmTestGrant, TokenValidatingOwnerVerifier,
    TrustedOwnerAuthorizationRequest, UnissuedOwnerAuthorizationVerifier,
    WindowsCredentialManagerAuthorityProvider, WindowsScmBackend,
    DEFAULT_AUTHORIZATION_VALIDITY_SECONDS, DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET,
};

fn test_scope() -> OwnerAuthorizationScope {
    OwnerAuthorizationScope::new(
        "actions.runner.auth-target",
        "authorized-image-token-digest-12345",
        "authorized-execution-identity-digest-67890",
        "authorized-target-host-token-abcde",
        "028c7efba9ebd3d3925371b356a187eba428041d",
        "tx-auth-qualification-001",
        true,
        true,
        false,
    )
}

#[test]
fn unissued_verifier_refuses_all_external_requests() {
    let verifier = UnissuedOwnerAuthorizationVerifier;
    let request = TrustedOwnerAuthorizationRequest {
        service_name: "actions.runner.auth-target".to_string(),
        image_token: "authorized-image-token-digest-12345".to_string(),
        execution_identity_token: "authorized-execution-identity-digest-67890".to_string(),
        target_host_token: "authorized-target-host-token-abcde".to_string(),
        candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_string(),
        allow_start: true,
        allow_stop: true,
        allow_change_config: false,
        transaction_id: "tx-auth-qualification-001".to_string(),
        not_before_epoch_seconds: 0,
        expires_at_epoch_seconds: u64::MAX,
        nonce: "unissued-external-nonce".to_string(),
        owner_material: b"any-caller-material".to_vec(),
    };

    let result = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request);
    assert!(
        matches!(result, Err(ScmApiError::AuthorizationNotIssued)),
        "Unissued verifier must unconditionally return AuthorizationNotIssued"
    );
}

#[test]
fn external_caller_token_validating_verifier_refuses_without_authority() {
    let raw_token = b"caller-arbitrary-raw-token";
    let scope = test_scope();
    let verifier = TokenValidatingOwnerVerifier::new(raw_token, scope.clone());

    let request = TrustedOwnerAuthorizationRequest {
        service_name: scope.service_name,
        image_token: scope.image_token,
        execution_identity_token: scope.execution_identity_token,
        target_host_token: scope.target_host_token,
        candidate_commit: scope.candidate_commit,
        transaction_id: scope.transaction_id,
        allow_start: scope.allow_start,
        allow_stop: scope.allow_stop,
        allow_change_config: scope.allow_change_config,
        not_before_epoch_seconds: 0,
        expires_at_epoch_seconds: u64::MAX,
        nonce: "raw-token-nonce".to_string(),
        owner_material: raw_token.to_vec(),
    };

    let result = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request);
    assert!(
        matches!(result, Err(ScmApiError::AuthorizationNotIssued)),
        "TokenValidatingOwnerVerifier created via public new must return AuthorizationNotIssued"
    );
}

#[test]
fn external_caller_cannot_mint_trusted_live_with_any_token_or_root() {
    let scope = test_scope();

    // Matrix of diverse attempts by an external caller:
    let test_materials: &[&[u8]] = &[
        b"caller-invented-secret",
        b"actual-owner-offline-hardware-secret",
        b"owner-root-secret-offline-hardware-key-material",
        b"trusted-authority-master-secret",
        b"",
        b"\0\0\0\0",
        &[0xFF; 64],
    ];

    for &material in test_materials {
        let verifier = TokenValidatingOwnerVerifier::new(material, scope.clone());
        let request = TrustedOwnerAuthorizationRequest {
            service_name: scope.service_name.clone(),
            image_token: scope.image_token.clone(),
            execution_identity_token: scope.execution_identity_token.clone(),
            target_host_token: scope.target_host_token.clone(),
            candidate_commit: scope.candidate_commit.clone(),
            transaction_id: scope.transaction_id.clone(),
            allow_start: scope.allow_start,
            allow_stop: scope.allow_stop,
            allow_change_config: scope.allow_change_config,
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: format!("attempt-nonce-{}", material.len()),
            owner_material: material.to_vec(),
        };

        let result = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request);
        assert!(
            matches!(
                result,
                Err(ScmApiError::AuthorizationNotIssued | ScmApiError::AccessDenied)
            ),
            "Every external attempt must fail closed (got {result:?})"
        );
    }
}

#[test]
fn sensitive_material_is_redacted_in_debug() {
    let request = TrustedOwnerAuthorizationRequest {
        service_name: "actions.runner.auth-target".to_string(),
        image_token: "authorized-image-token-digest-12345".to_string(),
        execution_identity_token: "authorized-execution-identity-digest-67890".to_string(),
        target_host_token: "authorized-target-host-token-abcde".to_string(),
        candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_string(),
        allow_start: true,
        allow_stop: true,
        allow_change_config: false,
        transaction_id: "tx-auth-qualification-001".to_string(),
        not_before_epoch_seconds: 1_000,
        expires_at_epoch_seconds: 2_000,
        nonce: "debug-nonce-123".to_string(),
        owner_material: b"super-secret-cryptographic-voucher-bytes".to_vec(),
    };

    let debug_output = format!("{request:?}");
    assert!(debug_output.contains("[REDACTED]"));
    assert!(!debug_output.contains("super-secret"));
}

#[test]
fn defense_in_depth_real_windows_scm_writes_remain_sealed() {
    let backend = WindowsScmBackend::mutation_inert();
    assert!(matches!(
        backend.start("actions.runner.auth-target"),
        Err(ScmApiError::GateClosed)
    ));
    assert!(matches!(
        backend.stop("actions.runner.auth-target"),
        Err(ScmApiError::GateClosed)
    ));
    assert!(matches!(
        backend.change_start_mode("actions.runner.auth-target", ServiceStartMode::Manual),
        Err(ScmApiError::GateClosed)
    ));
}

#[test]
fn windows_credential_manager_provider_fails_closed_when_credential_absent() {
    let provider = WindowsCredentialManagerAuthorityProvider;
    let res = provider.read_authority_secret("non-existent-test-credential-target-xyz123");
    assert!(matches!(res, Err(ScmApiError::AuthorizationNotIssued)));
}

#[test]
fn p03_fake_credential_manager_30min_temporal_window_and_single_use_nonce() {
    let fake_provider = FakeCredentialManagerAuthorityProvider::new();
    let absent_res = fake_provider.read_authority_secret(DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET);
    assert!(matches!(
        absent_res,
        Err(ScmApiError::AuthorizationNotIssued)
    ));

    let secret = b"offline-owner-qualification-key-32b-secret";
    fake_provider.set_secret(DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET, secret);

    let start_time: u64 = 50_000;
    let clock = Arc::new(MockClock::new(start_time));
    let scope = test_scope();

    let issuer = OwnerAuthorizationIssuer::with_clock(secret, clock.clone());
    let (req, _voucher) = issuer.issue_for_scope(
        &scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "p03-integration-nonce-001",
    );

    let verifier = TokenValidatingOwnerVerifier::from_credential_provider_and_clock(
        scope,
        DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET,
        &fake_provider,
        clock.clone(),
    )
    .expect("verifier construction with provider must succeed");

    // Mint trusted live gate
    let gate = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req)
        .expect("valid owner voucher must produce TrustedLive gate");
    assert!(gate.is_trusted_live());
    assert!(gate.allows_mutation());

    // Arm WindowsScmBackend with gate and mock clock
    let mut backend = WindowsScmBackend::mutation_inert().with_clock(clock.clone());
    backend.arm_with_gate(&gate).unwrap();
    assert!(backend.is_armed());

    // Pre-I/O check: permitted Start action succeeds
    assert!(backend
        .authorize_mutation("actions.runner.auth-target", ScmActionKind::Start)
        .is_ok());

    // Pre-I/O check: drifted service name fails closed
    assert_eq!(
        backend
            .authorize_mutation("actions.runner.drifted-name", ScmActionKind::Start)
            .unwrap_err(),
        ScmApiError::NameDrift
    );

    // Pre-I/O check: unpermitted action fails closed
    assert_eq!(
        backend
            .authorize_mutation("actions.runner.auth-target", ScmActionKind::ChangeConfig)
            .unwrap_err(),
        ScmApiError::GateClosed
    );

    // Advance clock past 30-min (1800s) validity window: t = 50_000 + 1800 + 1
    clock.set_time(start_time + DEFAULT_AUTHORIZATION_VALIDITY_SECONDS + 1);
    assert!(!backend.is_armed(), "backend must disarm when time expires");
    assert_eq!(
        backend
            .authorize_mutation("actions.runner.auth-target", ScmActionKind::Start)
            .unwrap_err(),
        ScmApiError::AccessDenied,
        "mutation must be closed/denied once expired"
    );

    // Single-use nonce: attempting to verify the exact same request again fails closed
    let replay_res = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req);
    assert_eq!(
        replay_res.unwrap_err(),
        ScmApiError::AccessDenied,
        "replayed nonce must fail closed with AccessDenied"
    );
}

#[test]
fn p03_restoration_readback_fails_closed_when_service_remains_listening() {
    let mock = MockScmBackend::new();
    let service_name = "actions.runner.restore-target";
    mock.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: service_name.to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: "expected-image-token".to_string(),
        execution_identity_token: "expected-identity-token".to_string(),
    });

    let grant = SyntheticScmTestGrant::new(service_name, "expected-image-token", true, true, true);
    let gate = LiveScmExecutionGate::from_synthetic_test_grant(&grant).unwrap();

    let executor = ScmControlExecutor::new(
        mock.clone(),
        service_name,
        "expected-image-token",
        "expected-identity-token",
        gate,
    );

    let prestate = executor.capture_prestate(false, true).unwrap();
    let mut plan = capture_restore_plan(&prestate);
    // Restoration requires service to be Stopped
    plan.restore_running = false;

    // Simulate SCM failure: next backend operation fails with AccessDenied
    mock.set_fail_next(ScmApiError::AccessDenied);

    let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
    let receipt = executor.restore(&plan, &mut admission);

    assert!(
        !receipt.restore_pass,
        "restore_pass must be false when service fails to stop"
    );
    assert_eq!(
        receipt.stage,
        ScmTransactionStage::RestoreFailed,
        "stage must be RestoreFailed"
    );
    assert_ne!(
        receipt.achieved,
        AchievedServiceControl::Stopped,
        "must never falsely report Stopped"
    );
}

fn test_discovered_runner(
    key: &str,
    scope: RegistrationScope,
    runner_id: u64,
    home: &str,
    service_name: &str,
) -> DiscoveredRunner {
    let identity = OpaqueIdentityReference::new(
        "windows-sid-sha256",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();

    DiscoveredRunner {
        local_discovery_key: key.to_owned(),
        backend: RunnerBackendKind::WindowsService,
        disposition: DiscoveryDisposition::ObservedOnly,
        refusal: None,
        identity: DiscoveredRunnerIdentity {
            registration_scope: Some(scope),
            runner_id: Some(runner_id),
            expected_runner_name: Some(format!("runner-{runner_id}")),
            runner_home: PathBuf::from(home),
            work_root: Some(PathBuf::from(home).join("_work")),
            execution_identity_ref: Some(identity),
        },
        service: Some(DiscoveredServiceBinding {
            service_name: service_name.to_owned(),
            display_name: None,
            image_path: None,
            state: DiscoveredLifecycle::Stopped,
        }),
        session: None,
        observed_lifecycle: DiscoveredLifecycle::Stopped,
        same_scope_collision: false,
        capabilities: DiscoveryCapabilities {
            service_control_contract: true,
            user_session_control_contract: false,
            enrollment_eligible: true,
        },
    }
}

#[test]
fn p03_adr0006_deferred_distinct_scopes_allowed_same_scope_collision_refused() {
    let mut editor = RunnerRegistryEditor::empty();
    let runner_a = test_discovered_runner(
        "r1",
        RegistrationScope::Repository {
            owner: "owner-org".to_string(),
            repository: "repo-alpha".to_string(),
        },
        101,
        r"C:\Runners\r1",
        "actions.runner.r1",
    );
    let runner_b = test_discovered_runner(
        "r2",
        RegistrationScope::Repository {
            owner: "owner-org".to_string(),
            repository: "repo-beta".to_string(),
        },
        102,
        r"C:\Runners\r2",
        "actions.runner.r2",
    );
    let runner_c_colliding = test_discovered_runner(
        "r3",
        RegistrationScope::Repository {
            owner: "owner-org".to_string(),
            repository: "repo-alpha".to_string(),
        },
        103,
        r"C:\Runners\r3",
        "actions.runner.r3",
    );

    assert!(editor.enroll_from_discovery(&runner_a, None, true).is_ok());
    assert!(
        editor.enroll_from_discovery(&runner_b, None, true).is_ok(),
        "distinct scopes must both succeed"
    );
    let res = editor.enroll_from_discovery(&runner_c_colliding, None, true);
    assert!(
        matches!(res, Err(RegistryError::SameScopeManagedAdmissionCollision)),
        "ADR-0006 DEFER: same-scope dual managed admission must be refused"
    );
}
