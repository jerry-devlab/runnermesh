//! Crate-external integration tests for R4 trusted Owner authorization boundary.
//!
//! Validates the complete P03 9-dimension cryptographic binding and refusal matrix
//! from the perspective of an external caller importing the `runnermesh` library.
//!
//! Enforces:
//! - UnissuedOwnerAuthorizationVerifier never opens a live gate (AuthorizationNotIssued).
//! - Callers cannot self-select tokens without the authority secret (AccessDenied).
//! - All 9 P03 dimensions are verified (service, image, execution identity, host,
//!   commit, rights, transaction, validity window, single-use nonce).
//! - Replays with same nonce, swapped nonce, and reconstructed verifier fail closed.
//! - Sensitive owner material is redacted from Debug representations.
//! - Defense-in-depth: Live Windows SCM backend writes remain sealed (GateClosed)
//!   even when presented with a TrustedLive execution gate.

use runnermesh::{
    LiveScmExecutionGate, OwnerAuthorizationIssuer, OwnerAuthorizationScope, ScmApiError,
    ScmControlBackend, ServiceStartMode, TokenValidatingOwnerVerifier,
    TrustedOwnerAuthorizationRequest, UnissuedOwnerAuthorizationVerifier, WindowsScmBackend,
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
    assert!(matches!(result, Err(ScmApiError::AuthorizationNotIssued)));
}

#[test]
fn external_caller_cannot_self_select_or_forge_voucher() {
    let authority_secret = b"owner-root-secret-offline-hardware-key-material";
    let scope = test_scope();
    let verifier = TokenValidatingOwnerVerifier::from_authority(authority_secret.to_vec(), scope);

    // Caller invents their own token and attempts to authorize actions.runner.auth-target
    let forged_request = TrustedOwnerAuthorizationRequest {
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
        nonce: "forged-external-nonce".to_string(),
        owner_material: b"self-selected-caller-token".to_vec(),
    };

    let result = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &forged_request);
    assert!(matches!(result, Err(ScmApiError::AccessDenied)));
}

#[test]
fn p03_all_nine_dimensions_refusal_matrix() {
    let authority_secret = b"owner-root-secret-offline-hardware-key-material";
    let scope = test_scope();
    let issuer = OwnerAuthorizationIssuer::new(authority_secret.to_vec());
    let verifier =
        TokenValidatingOwnerVerifier::from_authority(authority_secret.to_vec(), scope.clone());

    let (valid_req, _) = issuer.issue_for_scope(&scope, 3600, "valid-single-use-nonce-1");

    // 1. Empty material refuses with AuthorizationNotIssued
    let mut req = valid_req.clone();
    req.owner_material = Vec::new();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AuthorizationNotIssued)
    ));

    // 2. Tampered service name (wildcard) refuses with NameDrift
    let mut req = valid_req.clone();
    req.service_name = "actions.runner.*".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::NameDrift)
    ));

    // 3. Service name drift refuses with NameDrift
    let mut req = valid_req.clone();
    req.service_name = "actions.runner.foreign".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::NameDrift)
    ));

    // 4. Image token drift refuses with IdentityMismatch
    let mut req = valid_req.clone();
    req.image_token = "foreign-image-token".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::IdentityMismatch)
    ));

    // 5. Execution identity token drift refuses with IdentityMismatch
    let mut req = valid_req.clone();
    req.execution_identity_token = "foreign-execution-identity".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::IdentityMismatch)
    ));

    // 6. Target host token drift refuses with AccessDenied
    let mut req = valid_req.clone();
    req.target_host_token = "foreign-host-token".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AccessDenied)
    ));

    // 7. Candidate commit drift refuses with AccessDenied
    let mut req = valid_req.clone();
    req.candidate_commit = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AccessDenied)
    ));

    // 8. Transaction ID drift refuses with AccessDenied
    let mut req = valid_req.clone();
    req.transaction_id = "tx-tampered-id".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AccessDenied)
    ));

    // 9. Excessive permission request refuses with AccessDenied
    let mut req = valid_req.clone();
    req.allow_change_config = true; // Scope only authorizes start and stop
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AccessDenied)
    ));

    // 10. Empty nonce refuses with AccessDenied
    let mut req = valid_req.clone();
    req.nonce = "  ".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AccessDenied)
    ));

    // 11. Inverted validity window refuses with AccessDenied
    let mut req = valid_req.clone();
    req.expires_at_epoch_seconds = req.not_before_epoch_seconds - 10;
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &req),
        Err(ScmApiError::AccessDenied)
    ));

    // 12. Valid request passes exactly once and opens TrustedLive
    let gate = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &valid_req)
        .expect("Valid authorization must pass");
    assert!(gate.is_trusted_live());
    assert!(!gate.is_synthetic_test_only());
    assert!(gate.allows_mutation());

    // 13. Replay with identical nonce fails closed across shared ledger
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &valid_req),
        Err(ScmApiError::AccessDenied)
    ));

    // 14. Replay with swapped nonce fails closed (digest mismatch and/or consumption)
    let mut replay_swapped = valid_req.clone();
    replay_swapped.nonce = "swapped-nonce-attempt".to_string();
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &replay_swapped),
        Err(ScmApiError::AccessDenied)
    ));

    // 15. Replay against a fresh reconstructed verifier fails closed via global shared ledger
    let verifier2 = TokenValidatingOwnerVerifier::from_authority(authority_secret.to_vec(), scope);
    assert!(matches!(
        LiveScmExecutionGate::from_trusted_owner_authorization(&verifier2, &valid_req),
        Err(ScmApiError::AccessDenied)
    ));
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
fn defense_in_depth_trusted_live_gate_leaves_windows_backend_sealed() {
    let authority_secret = b"owner-root-secret-offline-hardware-key-material";
    let scope = test_scope();
    let issuer = OwnerAuthorizationIssuer::new(authority_secret.to_vec());
    let verifier =
        TokenValidatingOwnerVerifier::from_authority(authority_secret.to_vec(), scope.clone());

    let (valid_req, _) = issuer.issue_for_scope(&scope, 3600, "defense-in-depth-nonce");
    let gate = LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &valid_req)
        .expect("Valid authorization must pass");
    assert!(gate.is_trusted_live());

    // PRODUCTION_MUTATION=false: Windows SCM adapter writes unconditionally refuse
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
