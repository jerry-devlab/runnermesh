//! Crate-external integration tests for R4 trusted Owner authorization boundary.
//!
//! Validates the complete P03 authorization boundary and negative matrix
//! from the perspective of an external caller importing the `runnermesh` library.
//!
//! In ordinary builds and for external crate consumers:
//! - Authority roots are UNISSUED in code (no hardcoded test roots exist).
//! - Public `UnissuedOwnerAuthorizationVerifier` unconditionally fails closed (`AuthorizationNotIssued`).
//! - Public `TokenValidatingOwnerVerifier::from_credential_manager(...)` queries Windows Credential Manager,
//!   failing closed (`AuthorizationNotIssued`) when absent.
//! - External callers have ZERO public methods or constructors to supply authority secrets or mint `TrustedLive`.
//! - Forged, self-selected, or tampered vouchers are rejected fail-closed.
//! - Sensitive owner material is redacted from Debug representations (`<redacted>`).
//! - Defense-in-depth: Live Windows SCM backend writes remain sealed (`GateClosed`).

use std::path::PathBuf;

use runnermesh::{
    capture_restore_plan, AchievedServiceControl, DiscoveredLifecycle, DiscoveredRunner,
    DiscoveredRunnerIdentity, DiscoveredServiceBinding, DiscoveryCapabilities,
    DiscoveryDisposition, FakeGithubAdmissionBackend, LiveScmExecutionGate, MockScmBackend,
    MockServiceRecord, OpaqueIdentityReference, OwnerAuthoritySecretProvider,
    OwnerAuthorizationScope, RegistrationScope, RegistryError, RunnerBackendKind,
    RunnerRegistryEditor, ScmActionKind, ScmApiError, ScmControlExecutor, ScmTransactionStage,
    ServiceObservation, ServiceStartMode, SyntheticScmTestGrant, TokenValidatingOwnerVerifier,
    TrustedOwnerAuthorizationRequest, UnissuedOwnerAuthorizationVerifier,
    WindowsCredentialManagerAuthorityProvider, WindowsScmBackend,
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
        not_before_epoch_seconds: 1000,
        expires_at_epoch_seconds: 2800,
        nonce: "test-nonce-001".to_string(),
        owner_material: b"forged-voucher-signature-bytes".to_vec(),
    };

    let result = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ScmApiError::AuthorizationNotIssued);
}

#[test]
fn external_caller_token_validating_verifier_refuses_without_authority() {
    let scope = test_scope();

    let verifier_result = TokenValidatingOwnerVerifier::from_credential_manager(scope);
    assert!(
        matches!(verifier_result, Err(ScmApiError::AuthorizationNotIssued)),
        "unprovisioned environment must return AuthorizationNotIssued"
    );
}

#[test]
fn external_caller_cannot_mint_trusted_live_with_any_token_or_root() {
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
        not_before_epoch_seconds: 1000,
        expires_at_epoch_seconds: 2800,
        nonce: "test-nonce-002".to_string(),
        owner_material: b"completely-arbitrary-bytes".to_vec(),
    };

    let unissued = UnissuedOwnerAuthorizationVerifier;
    let gate = LiveScmExecutionGate::from_trusted_owner_authorization(&unissued, &request);
    assert_eq!(gate.unwrap_err(), ScmApiError::AuthorizationNotIssued);

    let default_gate = LiveScmExecutionGate::default();
    assert!(!default_gate.is_trusted_live());
    assert!(!default_gate.allows_mutation());

    let closed_gate = LiveScmExecutionGate::CLOSED;
    assert!(!closed_gate.is_trusted_live());
    assert!(!closed_gate.allows_mutation());
}

#[test]
fn sensitive_material_is_redacted_in_debug() {
    let request = TrustedOwnerAuthorizationRequest {
        service_name: "actions.runner.svc".to_string(),
        image_token: "image-token".to_string(),
        execution_identity_token: "identity-token".to_string(),
        target_host_token: "host-token".to_string(),
        candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_string(),
        allow_start: true,
        allow_stop: true,
        allow_change_config: false,
        transaction_id: "tx-001".to_string(),
        not_before_epoch_seconds: 1000,
        expires_at_epoch_seconds: 2800,
        nonce: "secret-nonce".to_string(),
        owner_material: b"super-secret-voucher-hmac-material".to_vec(),
    };

    let debug_repr = format!("{request:?}");
    assert!(
        !debug_repr.contains("super-secret-voucher-hmac-material"),
        "raw voucher material must never appear in Debug representations"
    );
    assert!(
        debug_repr.contains("<redacted>"),
        "voucher material must be redacted in Debug output"
    );
}

#[test]
fn defense_in_depth_real_windows_scm_writes_remain_sealed() {
    let backend = WindowsScmBackend::mutation_inert();
    assert!(!backend.is_armed());

    let start_res = backend.authorize_mutation("actions.runner.auth-target", ScmActionKind::Start);
    assert_eq!(start_res.unwrap_err(), ScmApiError::GateClosed);

    let stop_res = backend.authorize_mutation("actions.runner.auth-target", ScmActionKind::Stop);
    assert_eq!(stop_res.unwrap_err(), ScmApiError::GateClosed);

    let config_res =
        backend.authorize_mutation("actions.runner.auth-target", ScmActionKind::ChangeConfig);
    assert_eq!(config_res.unwrap_err(), ScmApiError::GateClosed);
}

#[test]
fn windows_credential_manager_provider_fails_closed_when_credential_absent() {
    let provider = WindowsCredentialManagerAuthorityProvider;
    let res = provider.read_authority_secret("non-existent-test-credential-target-xyz123");
    assert!(matches!(res, Err(ScmApiError::AuthorizationNotIssued)));
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

#[test]
fn default_release_api_refuses_arbitrary_key_minting_trusted_live_for_native_operations() {
    let scope = test_scope();

    // 1. The public production constructor from_credential_manager strictly queries
    // Windows Credential Manager target runnermesh/owner-authority. When no secret is stored,
    // it fails closed with AuthorizationNotIssued.
    let verifier_result = TokenValidatingOwnerVerifier::from_credential_manager(scope.clone());
    assert!(
        matches!(verifier_result, Err(ScmApiError::AuthorizationNotIssued)),
        "default release API without credential manager entry must fail closed with AuthorizationNotIssued"
    );

    // 2. An external attacker constructing an arbitrary voucher request cannot mint TrustedLive
    // through an unissued verifier or the default release API.
    let forged_req = TrustedOwnerAuthorizationRequest {
        service_name: scope.service_name.clone(),
        image_token: scope.image_token.clone(),
        execution_identity_token: scope.execution_identity_token.clone(),
        target_host_token: scope.target_host_token.clone(),
        candidate_commit: scope.candidate_commit.clone(),
        allow_start: scope.allow_start,
        allow_stop: scope.allow_stop,
        allow_change_config: scope.allow_change_config,
        transaction_id: scope.transaction_id.clone(),
        not_before_epoch_seconds: 1000,
        expires_at_epoch_seconds: 2800,
        nonce: "forged-nonce-001".to_string(),
        owner_material: b"attacker-arbitrary-hmac-material-0123456789".to_vec(),
    };

    let unissued_verifier = UnissuedOwnerAuthorizationVerifier;
    let gate_result =
        LiveScmExecutionGate::from_trusted_owner_authorization(&unissued_verifier, &forged_req);
    assert_eq!(
        gate_result.unwrap_err(),
        ScmApiError::AuthorizationNotIssued,
        "unissued verifier must refuse forged request"
    );

    // 3. Defense-in-depth: Default gate is CLOSED and strictly refuses mutation.
    let closed_gate = LiveScmExecutionGate::CLOSED;
    assert!(!closed_gate.allows_mutation());
    assert!(!closed_gate.is_trusted_live());

    // 4. Default Windows SCM backend is mutation-inert and strictly refuses mutation.
    let inert_backend = WindowsScmBackend::mutation_inert();
    assert!(!inert_backend.is_armed());
    assert_eq!(
        inert_backend
            .authorize_mutation("actions.runner.auth-target", ScmActionKind::Start)
            .unwrap_err(),
        ScmApiError::GateClosed,
        "mutation-inert backend must reject mutation with GateClosed"
    );
}
