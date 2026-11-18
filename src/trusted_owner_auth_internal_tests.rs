//! Internal trusted Owner authorization tests for P03 boundary.
//!
//! Gated by `#[cfg(test)]`. Contains tests that use fake credentials,
//! fake authority providers, and internal test constructors.

use std::sync::Arc;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use crate::agent_runtime::{start_installed_agent_sandbox, InstalledAgentSandboxConfig};
#[cfg(windows)]
use crate::runner_service_backend::{ServiceObservation, ServiceStartMode};
use crate::windows_scm_executor::test_support::{
    create_test_verifier_with_provider_and_clock, FakeCredentialManagerAuthorityProvider,
    OwnerAuthorizationIssuer,
};
#[cfg(windows)]
use crate::windows_scm_executor::{
    HostExecutionContext, MockScmBackend, MockServiceRecord, PrivilegedScmAction,
    PrivilegedScmHelper, PrivilegedScmRequest, ScmControlBackend,
};
use crate::windows_scm_executor::{
    LiveScmExecutionGate, MockClock, OwnerAuthoritySecretProvider, OwnerAuthorizationScope,
    ScmActionKind, ScmApiError, DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
    DEFAULT_OWNER_AUTHORITY_CREDENTIAL_TARGET,
};
#[cfg(windows)]
use crate::{
    AgentCommand, AgentResponse, AuthoritativeMultiRunnerLifecycleBackend, DiscoveredLifecycle,
    EnrollmentState, GatedUserSessionProcessTransport, IpcClient, IpcEndpoint, IpcRequest,
    IpcResponseBody, MultiRunnerLifecycleBackend, RegistryBackendBinding, RunnerRecord,
    SandboxMultiRunnerLifecycleBackend, StandardSessionCoordinator, UserSessionProcessTransport,
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
    let backend = crate::WindowsScmBackend::mutation_inert().with_clock(clock.clone());
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
    let context = HostExecutionContext::new("host-token-abcde", &running_commit).unwrap();
    let helper = PrivilegedScmHelper::new(mock_backend.clone(), LiveScmExecutionGate::CLOSED)
        .with_context(context)
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
    let context = HostExecutionContext::new("host-token-abcde", &running_commit).unwrap();
    let helper = PrivilegedScmHelper::new(mock_backend.clone(), LiveScmExecutionGate::CLOSED)
        .with_context(context)
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

#[test]
#[cfg(windows)]
fn p03_session_gate_start_only_cannot_stop_and_stop_only_cannot_start() {
    // Gap 2 Verification:
    // Session gate retains allow_start and allow_stop; start and stop are checked separately.
    // Proves: stop-only cannot start, start-only cannot stop, and NO OR condition opens the door.
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-p03-session-gates-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let service_name = "actions.runner.p03-session-gate";
    let mock_backend = MockScmBackend::new();
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let clock = Arc::new(MockClock::new(start_time));
    let running_commit = env!("RUNNERMESH_BUILD_COMMIT").to_string();
    let context = HostExecutionContext::new("host-token-abcde", &running_commit).unwrap();
    let helper = PrivilegedScmHelper::new(mock_backend.clone(), LiveScmExecutionGate::CLOSED)
        .with_context(context)
        .with_clock(clock.clone());

    let session_transport = Arc::new(GatedUserSessionProcessTransport::closed());
    let coordinator = Arc::new(StandardSessionCoordinator::new(session_transport.clone()));

    let backend = Arc::new(
        AuthoritativeMultiRunnerLifecycleBackend::new(helper).with_session_coordinator(coordinator),
    );
    let ipc_endpoint =
        IpcEndpoint::for_test_scope(format!("p03-session-gate-pipe-{}", std::process::id()));

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: temp_dir.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(backend),
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
    }

    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));
    let issuer = OwnerAuthorizationIssuer::with_clock(secret, clock.clone());

    let fake_record = RunnerRecord {
        local_runner_key: service_name.to_string(),
        registration_scope: crate::RegistrationScope::Repository {
            owner: "o".to_string(),
            repository: "r".to_string(),
        },
        runner_id: 1,
        expected_runner_name: "test-runner".to_string(),
        runner_home: temp_dir.clone(),
        work_root: temp_dir.clone(),
        execution_identity_ref: crate::OpaqueIdentityReference::new("sid", "identity-token-67890")
            .unwrap(),
        backend: RegistryBackendBinding::UserSession,
        desired_enabled: true,
        enrollment_state: EnrollmentState::Enrolled,
        credential_ref: None,
        managed_admission: false,
    };

    // Case A: Start-only authorization (allow_start = true, allow_stop = false)
    let start_only_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "host-token-abcde",
        &running_commit,
        "tx-start-only-001",
        true,  // allow_start
        false, // allow_stop
        false,
    );
    let (start_only_req, _) = issuer.issue_for_scope(
        &start_only_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-start-only-001",
    );
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: start_only_req,
        }))
        .expect("AuthorizeOwnerMutation over IPC must succeed");
    let IpcResponseBody::Success(boxed_resp) = resp.body else {
        panic!("expected success");
    };
    let AgentResponse::OwnerAuthorization(rep) = *boxed_resp else {
        panic!("expected OwnerAuthorization");
    };
    assert!(rep.authorized);

    let session_gate = session_transport.current_gate();
    assert!(session_gate.allow_start());
    assert!(!session_gate.allow_stop());

    // Transport test: stop must fail with "session.transport-refused: session.stop-not-authorized"
    let stop_res = session_transport.request_graceful_stop(&fake_record);
    assert_eq!(
        stop_res.unwrap_err(),
        "session.transport-refused: session.stop-not-authorized",
        "start-only authorization must NOT permit session stop"
    );

    // Case B: Stop-only authorization (allow_start = false, allow_stop = true)
    let stop_only_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "host-token-abcde",
        &running_commit,
        "tx-stop-only-002",
        false, // allow_start
        true,  // allow_stop
        false,
    );
    let (stop_only_req, _) = issuer.issue_for_scope(
        &stop_only_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-stop-only-002",
    );
    let resp2 = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: stop_only_req,
        }))
        .expect("AuthorizeOwnerMutation over IPC must succeed");
    let IpcResponseBody::Success(boxed_resp2) = resp2.body else {
        panic!("expected success");
    };
    let AgentResponse::OwnerAuthorization(rep2) = *boxed_resp2 else {
        panic!("expected OwnerAuthorization");
    };
    assert!(rep2.authorized);

    let session_gate2 = session_transport.current_gate();
    assert!(!session_gate2.allow_start());
    assert!(session_gate2.allow_stop());

    // Transport test: start must fail with "session.transport-refused: session.start-not-authorized"
    let start_res = session_transport.launch_listener(&fake_record);
    assert_eq!(
        start_res.unwrap_err(),
        "session.transport-refused: session.start-not-authorized",
        "stop-only authorization must NOT permit session start"
    );

    let _ = runtime.shutdown();
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
#[cfg(windows)]
fn p03_execution_context_empty_or_unspecified_rejected_at_assembly_and_ipc() {
    // Gap 3 Verification:
    // Empty or "unspecified" host and commit must be rejected fail-closed.
    let running_commit = env!("RUNNERMESH_BUILD_COMMIT").to_string();

    // 1. HostExecutionContext construction rejects empty or "unspecified"
    assert!(HostExecutionContext::new("", &running_commit).is_err());
    assert!(HostExecutionContext::new("   ", &running_commit).is_err());
    assert!(HostExecutionContext::new("unspecified", &running_commit).is_err());
    assert!(HostExecutionContext::new("UNSPECIFIED", &running_commit).is_err());
    assert!(HostExecutionContext::new("host-1", "").is_err());
    assert!(HostExecutionContext::new("host-1", "   ").is_err());
    assert!(HostExecutionContext::new("host-1", "unspecified").is_err());
    assert!(HostExecutionContext::new("host-1", "UNSPECIFIED").is_err());
    assert!(HostExecutionContext::new("host-1", &running_commit).is_ok());

    // 2. Over IPC: unspecified or empty host/commit in authorization request are rejected
    let temp_dir = std::env::temp_dir().join(format!(
        "rm-p03-unspec-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let service_name = "actions.runner.p03-unspec";
    let mock_backend = MockScmBackend::new();
    let clock = Arc::new(MockClock::new(400_000));
    let context = HostExecutionContext::new("host-token-abcde", &running_commit).unwrap();
    let helper = PrivilegedScmHelper::new(mock_backend.clone(), LiveScmExecutionGate::CLOSED)
        .with_context(context)
        .with_clock(clock.clone());

    let backend = Arc::new(SandboxMultiRunnerLifecycleBackend::new(helper));
    let ipc_endpoint =
        IpcEndpoint::for_test_scope(format!("p03-unspec-pipe-{}", std::process::id()));

    let sandbox_config = InstalledAgentSandboxConfig {
        install_root: temp_dir.clone(),
        ipc_endpoint: ipc_endpoint.clone(),
        multi_runner_backend: Some(backend),
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
    }

    let ipc_client = IpcClient::new(ipc_endpoint, Duration::from_secs(3));
    let issuer = OwnerAuthorizationIssuer::with_clock(secret, clock);

    // Request with empty host token
    let empty_host_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "",
        &running_commit,
        "tx-unspec-001",
        true,
        true,
        false,
    );
    let (empty_host_req, _) = issuer.issue_for_scope(
        &empty_host_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-unspec-001",
    );
    let resp = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: empty_host_req,
        }))
        .unwrap();
    let IpcResponseBody::Success(boxed_resp) = resp.body else {
        panic!("expected success");
    };
    let AgentResponse::OwnerAuthorization(rep) = *boxed_resp else {
        panic!("expected OwnerAuthorization");
    };
    assert!(!rep.authorized, "empty host token must be rejected");
    assert_eq!(
        rep.reason_code.as_ref().map(|r| r.as_str()),
        Some("owner-auth-target-host-mismatch")
    );

    // Request with "unspecified" host token
    let unspec_host_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "unspecified",
        &running_commit,
        "tx-unspec-002",
        true,
        true,
        false,
    );
    let (unspec_host_req, _) = issuer.issue_for_scope(
        &unspec_host_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-unspec-002",
    );
    let resp2 = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: unspec_host_req,
        }))
        .unwrap();
    let IpcResponseBody::Success(boxed_resp2) = resp2.body else {
        panic!("expected success");
    };
    let AgentResponse::OwnerAuthorization(rep2) = *boxed_resp2 else {
        panic!("expected OwnerAuthorization");
    };
    assert!(!rep2.authorized, "unspecified host token must be rejected");
    assert_eq!(
        rep2.reason_code.as_ref().map(|r| r.as_str()),
        Some("owner-auth-target-host-mismatch")
    );

    // Request with empty candidate commit
    let empty_commit_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "host-token-abcde",
        "",
        "tx-unspec-003",
        true,
        true,
        false,
    );
    let (empty_commit_req, _) = issuer.issue_for_scope(
        &empty_commit_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-unspec-003",
    );
    let resp3 = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: empty_commit_req,
        }))
        .unwrap();
    let IpcResponseBody::Success(boxed_resp3) = resp3.body else {
        panic!("expected success");
    };
    let AgentResponse::OwnerAuthorization(rep3) = *boxed_resp3 else {
        panic!("expected OwnerAuthorization");
    };
    assert!(!rep3.authorized, "empty commit must be rejected");
    assert_eq!(
        rep3.reason_code.as_ref().map(|r| r.as_str()),
        Some("owner-auth-candidate-commit-mismatch")
    );

    // Request with "unspecified" candidate commit
    let unspec_commit_scope = OwnerAuthorizationScope::new(
        service_name,
        "image-token-12345",
        "identity-token-67890",
        "host-token-abcde",
        "unspecified",
        "tx-unspec-004",
        true,
        true,
        false,
    );
    let (unspec_commit_req, _) = issuer.issue_for_scope(
        &unspec_commit_scope,
        DEFAULT_AUTHORIZATION_VALIDITY_SECONDS,
        "nonce-unspec-004",
    );
    let resp4 = ipc_client
        .call(IpcRequest::new(AgentCommand::AuthorizeOwnerMutation {
            request: unspec_commit_req,
        }))
        .unwrap();
    let IpcResponseBody::Success(boxed_resp4) = resp4.body else {
        panic!("expected success");
    };
    let AgentResponse::OwnerAuthorization(rep4) = *boxed_resp4 else {
        panic!("expected OwnerAuthorization");
    };
    assert!(!rep4.authorized, "unspecified commit must be rejected");
    assert_eq!(
        rep4.reason_code.as_ref().map(|r| r.as_str()),
        Some("owner-auth-candidate-commit-mismatch")
    );

    let _ = runtime.shutdown();
    let _ = std::fs::remove_dir_all(temp_dir);
}
