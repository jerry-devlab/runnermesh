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
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use runnermesh::agent_runtime::{start_installed_agent_sandbox, InstalledAgentSandboxConfig};
use runnermesh::test_support::{
    create_test_verifier_with_provider_and_clock, FakeCredentialManagerAuthorityProvider,
    OwnerAuthorizationIssuer,
};
use runnermesh::{
    capture_restore_plan, AchievedServiceControl, DiscoveredLifecycle, DiscoveredRunner,
    DiscoveredRunnerIdentity, DiscoveredServiceBinding, DiscoveryCapabilities,
    DiscoveryDisposition, FakeGithubAdmissionBackend, LiveScmExecutionGate, MockClock,
    MockScmBackend, MockServiceRecord, OpaqueIdentityReference, OwnerAuthoritySecretProvider,
    OwnerAuthorizationScope, RegistrationScope, RegistryError, RunnerBackendKind,
    RunnerRegistryEditor, ScmActionKind, ScmApiError, ScmControlBackend, ScmControlExecutor,
    ScmTransactionStage, ServiceObservation, ServiceStartMode, SyntheticScmTestGrant,
    TokenValidatingOwnerVerifier, TrustedOwnerAuthorizationRequest,
    UnissuedOwnerAuthorizationVerifier, WindowsCredentialManagerAuthorityProvider,
    WindowsScmBackend, DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
    DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET,
};
#[cfg(windows)]
use runnermesh::{
    AgentCommand, AgentResponse, IpcClient, IpcEndpoint, IpcRequest, IpcResponseBody,
    MultiRunnerLifecycleBackend, PrivilegedScmAction, PrivilegedScmHelper, PrivilegedScmRequest,
    SandboxMultiRunnerLifecycleBackend,
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

    let verifier = create_test_verifier_with_provider_and_clock(
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
    let backend = WindowsScmBackend::mutation_inert().with_clock(clock.clone());
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

    // 2. An attacker generating a forged voucher using an arbitrary key cannot mint TrustedLive
    // through an unissued verifier.
    let forged_secret = b"attacker-arbitrary-key-0123456789";
    let attacker_issuer = OwnerAuthorizationIssuer::new(forged_secret);
    let (forged_req, _) = attacker_issuer.issue_for_scope(
        &scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "forged-nonce-001",
    );

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

#[test]
#[cfg(windows)]
fn p03_end_to_end_controlled_execution_chain_via_ipc_with_fake_credentials_and_os_boundary() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-p03-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let service_name = "actions.runner.p03-target";
    let mock_backend = MockScmBackend::new();
    mock_backend.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: service_name.to_string(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        image_token: "image-token-12345".to_string(),
        execution_identity_token: "identity-token-67890".to_string(),
    });

    let start_time: u64 = 100_000;
    let clock = Arc::new(MockClock::new(start_time));
    let running_commit = env!("RUNNERMESH_BUILD_COMMIT").to_string();
    let helper = PrivilegedScmHelper::new(mock_backend.clone(), LiveScmExecutionGate::CLOSED)
        .with_host_and_commit("host-token-abcde", &running_commit)
        .with_clock(clock.clone());

    let backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper));
    let ipc_endpoint = IpcEndpoint::for_test_scope(format!("p03-pipe-{}", std::process::id()));

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: temp_dir.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(backend.clone()),
        discovery_report: None,
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: false,
    };

    let mut runtime = start_installed_agent_sandbox(sandbox_config).unwrap();

    let fake_provider = Arc::new(FakeCredentialManagerAuthorityProvider::new());
    let secret = b"p03-qualification-secret-key-32b";
    fake_provider.set_secret(DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET, secret);
    {
        let mut core = runtime.core().lock().unwrap();
        core.set_authority_provider_for_test(fake_provider, Some(clock.clone()));
        core.set_expected_target_host_token_for_test("host-token-abcde");
    }

    // 1. Initial prestate: execution arm is CLOSED and session gate is unpopulated
    assert!(
        !mock_backend.is_armed(),
        "mock backend must initially be unarmed"
    );
    assert!(
        backend.armed_session_gate().is_none(),
        "session gate must initially be empty"
    );

    // 2. Issue valid owner authorization request for the exact running environment
    let scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "host-token-abcde",
        &running_commit,
        "tx-p03-e2e-001",
        true,
        true,
        false,
    );
    let issuer = OwnerAuthorizationIssuer::with_clock(secret, clock.clone());
    let (auth_req, _) = issuer.issue_for_scope(
        &scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-p03-e2e-001",
    );

    // 3. Send AuthorizeOwnerMutation over real Named Pipe IPC
    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: auth_req,
        }))
        .expect("AuthorizeOwnerMutation over IPC must succeed");

    let IpcResponseBody::Success(boxed_resp) = resp.body else {
        panic!("expected success response from IPC");
    };
    let AgentResponse::OwnerAuthorization(report) = *boxed_resp else {
        panic!("expected OwnerAuthorization response");
    };
    assert!(report.authorized, "report must indicate authorized: true");
    assert_eq!(report.transaction_id, "tx-p03-e2e-001");
    assert_eq!(
        report.expires_at_epoch_seconds,
        start_time + DEFAULT_AUTHORIZATION_VALIDITY_SECONDS
    );

    // 4. Verification that execution arm and session door were genuinely armed
    assert!(
        mock_backend.is_armed(),
        "underlying SCM execution arm must be armed after valid authorization"
    );
    assert!(
        backend.armed_session_gate().is_some(),
        "session gate door must be armed"
    );
    let session_gate = backend.armed_session_gate().unwrap();
    assert!(
        session_gate.is_trusted_live(),
        "session gate door must be TrustedLive"
    );

    // 5. Action: Stop service via armed execution arm
    let stop_req = PrivilegedScmRequest {
        action: PrivilegedScmAction::StopService,
        service_name: service_name.to_string(),
        expected_image_token: "image-token-12345".to_string(),
        expected_execution_identity_token: "identity-token-67890".to_string(),
        target_host_token: "host-token-abcde".to_string(),
        candidate_commit: running_commit.clone(),
        transaction_id: "tx-p03-e2e-001".to_string(),
    };
    let report = backend.dispatch_scm(&stop_req);
    assert!(
        report.applied,
        "stop action must be successfully applied by armed execution arm"
    );
    assert_eq!(report.achieved_state, DiscoveredLifecycle::Stopped);

    // Readback: SCM confirms Stopped
    let readback = mock_backend.query_exact(service_name).unwrap();
    assert_eq!(
        readback.observation.state,
        DiscoveredLifecycle::Stopped,
        "readback must confirm service is Stopped"
    );

    // 6. Restoration: Restore service back to Listening
    let start_req = PrivilegedScmRequest {
        action: PrivilegedScmAction::StartService,
        service_name: service_name.to_string(),
        expected_image_token: "image-token-12345".to_string(),
        expected_execution_identity_token: "identity-token-67890".to_string(),
        target_host_token: "host-token-abcde".to_string(),
        candidate_commit: running_commit.clone(),
        transaction_id: "tx-p03-e2e-001".to_string(),
    };
    let restore_report = backend.dispatch_scm(&start_req);
    assert!(
        restore_report.applied,
        "start action must be successfully applied by armed execution arm"
    );
    assert_eq!(
        restore_report.achieved_state,
        DiscoveredLifecycle::Listening
    );

    // Readback: SCM confirms Listening
    let restore_readback = mock_backend.query_exact(service_name).unwrap();
    assert_eq!(
        restore_readback.observation.state,
        DiscoveredLifecycle::Listening,
        "readback must confirm service is restored to Listening"
    );

    // 7. Expiry Write Prohibition: advance clock past 1800s
    clock.set_time(start_time + DEFAULT_AUTHORIZATION_VALIDITY_SECONDS + 1);
    assert!(
        !mock_backend.is_armed(),
        "execution arm must disarm automatically after expiry"
    );

    let expired_report = backend.dispatch_scm(&stop_req);
    assert!(
        !expired_report.applied,
        "mutation action after expiry must NOT be applied"
    );
    assert_eq!(
        expired_report.refusal_reason.as_deref(),
        Some(ScmApiError::AccessDenied.as_str()),
        "expired mutation must fail with access-denied"
    );

    let _ = runtime.shutdown();
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
#[cfg(windows)]
fn p03_controlled_execution_chain_candidate_commit_and_host_token_drift_rejected() {
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-p03-drift-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let service_name = "actions.runner.p03-drift";
    let mock_backend = MockScmBackend::new();
    let clock = Arc::new(MockClock::new(200_000));
    let running_commit = env!("RUNNERMESH_BUILD_COMMIT").to_string();
    let helper = PrivilegedScmHelper::new(mock_backend.clone(), LiveScmExecutionGate::CLOSED)
        .with_host_and_commit("host-token-abcde", &running_commit)
        .with_clock(clock.clone());

    let backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper));
    let ipc_endpoint =
        IpcEndpoint::for_test_scope(format!("p03-drift-pipe-{}", std::process::id()));

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: temp_dir.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(backend.clone()),
        discovery_report: None,
        poll_interval: Duration::from_millis(25),
        enable_periodic_scheduler: false,
    };

    let mut runtime = start_installed_agent_sandbox(sandbox_config).unwrap();

    let fake_provider = Arc::new(FakeCredentialManagerAuthorityProvider::new());
    let secret = b"p03-qualification-secret-key-32b";
    fake_provider.set_secret(DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET, secret);
    {
        let mut core = runtime.core().lock().unwrap();
        core.set_authority_provider_for_test(fake_provider, Some(clock.clone()));
        core.set_expected_target_host_token_for_test("host-token-abcde");
    }

    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));
    let issuer = OwnerAuthorizationIssuer::with_clock(secret, clock.clone());

    // 1. Commit drift: candidate commit in request differs from running build provenance
    let drifted_commit_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "host-token-abcde",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "tx-drift-001",
        true,
        true,
        false,
    );
    let (drifted_commit_req, _) = issuer.issue_for_scope(
        &drifted_commit_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-drift-commit-001",
    );
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: drifted_commit_req,
        }))
        .expect("AuthorizeOwnerMutation call must succeed");

    let IpcResponseBody::Success(boxed_resp) = resp.body else {
        panic!("expected success IPC response");
    };
    let AgentResponse::OwnerAuthorization(report) = *boxed_resp else {
        panic!("expected OwnerAuthorization response");
    };
    assert!(!report.authorized, "drifted commit must NOT be authorized");
    assert_eq!(
        report.reason_code.as_ref().map(|r| r.as_str()),
        Some("owner-auth-candidate-commit-mismatch")
    );
    assert!(
        !mock_backend.is_armed(),
        "mock backend must remain unarmed on commit mismatch"
    );
    assert!(
        backend.armed_session_gate().is_none(),
        "session gate must remain empty"
    );

    // 2. Host token drift: host token in request differs from running environment host
    let drifted_host_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "drifted-host-token-99999",
        &running_commit,
        "tx-drift-002",
        true,
        true,
        false,
    );
    let (drifted_host_req, _) = issuer.issue_for_scope(
        &drifted_host_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-drift-host-002",
    );
    let resp2 = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: drifted_host_req,
        }))
        .expect("AuthorizeOwnerMutation call must succeed");

    let IpcResponseBody::Success(boxed_resp2) = resp2.body else {
        panic!("expected success IPC response");
    };
    let AgentResponse::OwnerAuthorization(report2) = *boxed_resp2 else {
        panic!("expected OwnerAuthorization response");
    };
    assert!(
        !report2.authorized,
        "drifted host token must NOT be authorized"
    );
    assert_eq!(
        report2.reason_code.as_ref().map(|r| r.as_str()),
        Some("owner-auth-target-host-mismatch")
    );
    assert!(
        !mock_backend.is_armed(),
        "mock backend must remain unarmed on host mismatch"
    );
    assert!(
        backend.armed_session_gate().is_none(),
        "session gate must remain empty"
    );

    let _ = runtime.shutdown();
    let _ = std::fs::remove_dir_all(temp_dir);
}
