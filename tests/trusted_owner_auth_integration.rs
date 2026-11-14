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

use runnermesh::{
    LiveScmExecutionGate, OwnerAuthorizationScope, ScmApiError, ScmControlBackend,
    ServiceStartMode, TokenValidatingOwnerVerifier, TrustedOwnerAuthorizationRequest,
    UnissuedOwnerAuthorizationVerifier, WindowsScmBackend,
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
