//! Full synthetic HUX-2 qualification matrix (never live SCM / never live GitHub).
//!
//! Evidence classes (do not cross-promote):
//! - SYNTHETIC_PASS — mock SCM + fake GitHub
//! - SOURCE_READY — Windows write paths exist but inert; trusted auth unissued
//! - INDEPENDENT_REVIEW_PENDING
//! - LIVE_NOT_AUTHORIZED

use crate::{
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle,
    FakeGithubAdmissionBackend, LiveScmExecutionGate, MockScmBackend, MockServiceRecord,
    ScmApiError, ScmControlExecutor, ScmTransactionStage, ServiceObservation, ServiceStartMode,
    SyntheticScmTestGrant, TrustedOwnerAuthorizationRequest, UnissuedOwnerAuthorizationVerifier,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticDisposition {
    Pass,
    Fail,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hux2SyntheticReceipt {
    pub qualification_synthetic: SyntheticDisposition,
    pub restore_synthetic: SyntheticDisposition,
    pub source_ready: bool,
    pub live_qualification: &'static str,
    pub independent_security_review: &'static str,
    pub notes: Vec<&'static str>,
}

fn fixture_record(
    name: &str,
    state: DiscoveredLifecycle,
    start_mode: ServiceStartMode,
    busy: bool,
) -> MockServiceRecord {
    MockServiceRecord {
        observation: ServiceObservation {
            service_name: name.to_owned(),
            state,
            start_mode,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: busy,
        image_token: "hux2-image".to_owned(),
        execution_identity_token: "hux2-identity".to_owned(),
    }
}

fn synthetic_gate(
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
) -> LiveScmExecutionGate {
    LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
        "fixture.scm.hux2",
        "hux2-image",
        allow_start,
        allow_stop,
        allow_change_config,
    ))
    .expect("synthetic grant")
}

fn executor(
    mock: MockScmBackend,
    gate: LiveScmExecutionGate,
) -> ScmControlExecutor<MockScmBackend> {
    ScmControlExecutor::new(
        mock,
        "fixture.scm.hux2",
        "hux2-image",
        "hux2-identity",
        gate,
    )
}

/// Canonical synthetic lifecycle with Mock SCM + Fake GitHub admission.
pub fn run_hux2_synthetic_happy_path() -> Hux2SyntheticReceipt {
    let mock = MockScmBackend::new();
    mock.insert(fixture_record(
        "fixture.scm.hux2",
        DiscoveredLifecycle::Listening,
        ServiceStartMode::Automatic,
        false,
    ));
    let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
    let gate = synthetic_gate(true, true, true);
    let executor = executor(mock.clone(), gate);

    let prestate = match executor.capture_prestate(false, false) {
        Ok(prestate) => prestate,
        Err(_) => {
            return fail_receipt(vec!["prestate-capture-failed"]);
        }
    };

    let withdraw = executor.advance(
        DesiredServiceLifecycle::DrainThenStop,
        false,
        false,
        true,
        &prestate,
    );
    if withdraw.stage != ScmTransactionStage::WithdrawingAdmission {
        return fail_receipt(vec!["admission-withdraw-phase-missing"]);
    }
    if admission.withdraw_capacity().is_err() {
        return fail_receipt(vec!["fake-github-withdraw-failed"]);
    }

    // Busy appears after withdraw — must wait, never stop.
    mock.set_worker_busy("fixture.scm.hux2", true);
    let busy = executor.advance(
        DesiredServiceLifecycle::DrainThenStop,
        true,
        true,
        true,
        &prestate,
    );
    if busy.stage != ScmTransactionStage::WaitingIdle || mock.stop_calls() != 0 {
        return fail_receipt(vec!["busy-wait-failed"]);
    }
    mock.set_worker_busy("fixture.scm.hux2", false);

    let stop = executor.advance(
        DesiredServiceLifecycle::DrainThenStop,
        true,
        false,
        true,
        &prestate,
    );
    if stop.stage != ScmTransactionStage::StopReadback
        || stop.achieved != AchievedServiceControl::Stopped
    {
        return fail_receipt(vec!["stop-readback-failed"]);
    }

    let start = executor.advance(
        DesiredServiceLifecycle::RemainRunning,
        true,
        false,
        true,
        &prestate,
    );
    if start.stage != ScmTransactionStage::StartReadback
        || start.achieved != AchievedServiceControl::Running
    {
        return fail_receipt(vec!["start-readback-failed"]);
    }

    // Restore admission to match original prestate (selector was present).
    if admission.restore_capacity().is_err() {
        return fail_receipt(vec!["fake-github-restore-failed"]);
    }

    let plan = stop
        .restore
        .clone()
        .or(start.restore.clone())
        .expect("plan");
    let restored = executor.restore(&plan, &mut admission);
    let restore_ok = restored.restore_pass
        && restored.stage == ScmTransactionStage::Completed
        && restored.achieved == AchievedServiceControl::Running
        && admission.withdraw_calls() == 1
        && admission.restore_calls() == 1;

    Hux2SyntheticReceipt {
        qualification_synthetic: SyntheticDisposition::Pass,
        restore_synthetic: if restore_ok {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        },
        source_ready: true,
        live_qualification: "NOT_AUTHORIZED",
        independent_security_review: "PENDING",
        notes: vec!["synthetic-happy-path-mock-scm-fake-github"],
    }
}

fn fail_receipt(notes: Vec<&'static str>) -> Hux2SyntheticReceipt {
    Hux2SyntheticReceipt {
        qualification_synthetic: SyntheticDisposition::Fail,
        restore_synthetic: SyntheticDisposition::Blocked,
        source_ready: true,
        live_qualification: "NOT_AUTHORIZED",
        independent_security_review: "PENDING",
        notes,
    }
}

/// Failure-injection matrix. Each case must fail closed without claiming live PASS.
pub fn run_hux2_synthetic_failure_matrix() -> Vec<(&'static str, SyntheticDisposition)> {
    let mut out = Vec::new();

    // Access denied
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));
        mock.set_fail_next(ScmApiError::AccessDenied);
        let executor = executor(mock, synthetic_gate(true, true, false));
        let disposition = match executor.observe_verified() {
            Err(ScmApiError::AccessDenied) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("access-denied", disposition));
    }

    // Busy worker blocks stop
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Busy,
            ServiceStartMode::Manual,
            true,
        ));
        let executor = executor(mock.clone(), synthetic_gate(false, true, false));
        let prestate = executor.capture_prestate(true, true).unwrap();
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            true,
            true,
            &prestate,
        );
        let disposition =
            if receipt.stage == ScmTransactionStage::WaitingIdle && mock.stop_calls() == 0 {
                SyntheticDisposition::Pass
            } else {
                SyntheticDisposition::Fail
            };
        out.push(("busy-worker-blocks-stop", disposition));
    }

    // Name drift
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.other",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock, LiveScmExecutionGate::CLOSED);
        let disposition = match executor.observe_verified() {
            Err(ScmApiError::NotFound) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("service-name-drift", disposition));
    }

    // Closed gate
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock.clone(), LiveScmExecutionGate::CLOSED);
        let prestate = executor.capture_prestate(false, true).unwrap();
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        let disposition = if receipt.reason.as_deref() == Some("scm.live-gate-closed")
            && mock.stop_calls() == 0
            && !receipt.restore_pass
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("closed-gate-no-mutation", disposition));
    }

    // Incomplete image flag
    {
        let mock = MockScmBackend::new();
        let mut record = fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        );
        record.observation.image_path_matches = false;
        mock.insert(record);
        let executor = executor(mock, LiveScmExecutionGate::CLOSED);
        let disposition = match executor.observe_verified() {
            Err(ScmApiError::IdentityMismatch) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("incomplete-image-flag-refused", disposition));
    }

    // Restore change-config fail
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Automatic,
            false,
        ));
        mock.set_fail_change(ScmApiError::Timeout);
        let executor = executor(mock, synthetic_gate(true, true, true));
        let prestate = executor.capture_prestate(false, true).unwrap();
        let plan = crate::capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = executor.restore(&plan, &mut admission);
        let disposition = if !receipt.restore_pass
            && receipt.stage == ScmTransactionStage::RestoreFailed
            && receipt.reason.as_deref() == Some("scm.timeout")
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("restore-change-config-fail", disposition));
    }

    // Identity race: image token changes mid-flight
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock.clone(), synthetic_gate(true, true, true));
        let prestate = executor.capture_prestate(false, true).unwrap();
        // Drift image after prestate
        let mut drifted = fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        );
        drifted.image_token = "drifted-image".to_owned();
        mock.insert(drifted);
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        let disposition = if receipt.reason.as_deref() == Some("scm.identity-mismatch")
            && mock.stop_calls() == 0
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("identity-race-mid-flight", disposition));
    }

    // Admission disconnect during restore verification
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Automatic,
            false,
        ));
        let executor = executor(mock, synthetic_gate(true, true, true));
        let prestate = executor.capture_prestate(false, false).unwrap();
        let plan = crate::capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_disconnected(true);
        let receipt = executor.restore(&plan, &mut admission);
        let disposition =
            if !receipt.restore_pass && receipt.reason.as_deref() == Some("scm.unavailable") {
                SyntheticDisposition::Pass
            } else {
                SyntheticDisposition::Fail
            };
        out.push(("admission-disconnect-on-restore", disposition));
    }

    // Duplicate stop command after already stopped is Observe, not second stop
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock.clone(), synthetic_gate(true, true, true));
        let prestate = executor.capture_prestate(false, true).unwrap();
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        let disposition =
            if receipt.stage == ScmTransactionStage::Completed && mock.stop_calls() == 0 {
                SyntheticDisposition::Pass
            } else {
                SyntheticDisposition::Fail
            };
        out.push(("duplicate-stop-is-observe", disposition));
    }

    // Trusted verifier never issues in this Train
    {
        let request = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.hux2".to_owned(),
            image_token: "hux2-image".to_owned(),
            execution_identity_token: "hux2-execution-identity".to_owned(),
            target_host_token: "hux2-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-hux2-synth".to_owned(),
            owner_material: b"attempt".to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "hux2-synth-nonce".to_owned(),
        };
        let disposition = match LiveScmExecutionGate::from_trusted_owner_authorization(
            &UnissuedOwnerAuthorizationVerifier,
            &request,
        ) {
            Err(ScmApiError::AuthorizationNotIssued) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("trusted-auth-unissued", disposition));
    }

    // Partial restore: admission restored but observer still mismatches plan
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Automatic,
            false,
        ));
        let executor = executor(mock, synthetic_gate(true, true, true));
        let prestate = executor.capture_prestate(false, true).unwrap();
        let plan = crate::capture_restore_plan(&prestate);
        // Plan expects absent=true; after "partial" restore selector is present again.
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        let receipt = executor.restore(&plan, &mut admission);
        let disposition = if !receipt.restore_pass
            && receipt.reason.as_deref() == Some("scm.admission-readback-mismatch")
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("partial-admission-restore-mismatch", disposition));
    }

    // Interrupted stop readback: stop accepted by SCM, but observation query fails
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock.clone(), synthetic_gate(false, true, false));
        let prestate = executor.capture_prestate(false, true).unwrap();
        mock.set_fail_after_queries(1, ScmApiError::Timeout);
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        let disposition = if receipt.stage == ScmTransactionStage::Interrupted
            && receipt.api_accepted
            && !receipt.restore_pass
            && receipt.achieved == AchievedServiceControl::Unknown
            && receipt.reason.as_deref() == Some("scm.timeout")
            && mock.stop_calls() == 1
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("interrupted-stop-readback-fail", disposition));
    }

    // Interrupted start readback: start accepted by SCM, but observation query fails
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock.clone(), synthetic_gate(true, false, false));
        let prestate = executor.capture_prestate(false, true).unwrap();
        mock.set_fail_after_queries(1, ScmApiError::Unavailable);
        let receipt = executor.advance(
            DesiredServiceLifecycle::RemainRunning,
            true,
            false,
            true,
            &prestate,
        );
        let disposition = if receipt.stage == ScmTransactionStage::Interrupted
            && receipt.api_accepted
            && !receipt.restore_pass
            && receipt.achieved == AchievedServiceControl::Unknown
            && receipt.reason.as_deref() == Some("scm.unavailable")
            && mock.start_calls() == 1
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("interrupted-start-readback-fail", disposition));
    }

    // Restore change-start-mode readback fails: change accepted but readback query times out
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Automatic,
            false,
        ));
        let executor = executor(mock.clone(), synthetic_gate(true, true, true));
        let prestate = executor.capture_prestate(false, true).unwrap();
        let plan = crate::capture_restore_plan(&prestate);
        mock.set_fail_after_queries(1, ScmApiError::Timeout);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = executor.restore(&plan, &mut admission);
        let disposition = if receipt.stage == ScmTransactionStage::RestoreFailed
            && receipt.api_accepted
            && !receipt.restore_pass
            && receipt.achieved == AchievedServiceControl::Unknown
            && receipt.reason.as_deref() == Some("scm.timeout")
            && mock.change_calls() == 1
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("restore-start-mode-readback-interrupted", disposition));
    }

    // Concurrent Agent state drift: prestate was Listening, but service became Busy before stop
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));
        let executor = executor(mock.clone(), synthetic_gate(false, true, false));
        let prestate = executor.capture_prestate(false, true).unwrap();
        mock.set_worker_busy("fixture.scm.hux2", true);
        let receipt = executor.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            true,
            true,
            &prestate,
        );
        let disposition = if receipt.stage == ScmTransactionStage::WaitingIdle
            && mock.stop_calls() == 0
            && !receipt.restore_pass
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("concurrent-agent-state-drift", disposition));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_synthetic_qualification_and_restore_pass() {
        let receipt = run_hux2_synthetic_happy_path();
        assert_eq!(receipt.qualification_synthetic, SyntheticDisposition::Pass);
        assert_eq!(receipt.restore_synthetic, SyntheticDisposition::Pass);
        assert!(receipt.source_ready);
        assert_eq!(receipt.live_qualification, "NOT_AUTHORIZED");
        assert_eq!(receipt.independent_security_review, "PENDING");
    }

    #[test]
    fn failure_matrix_cases_pass_closed() {
        for (name, disposition) in run_hux2_synthetic_failure_matrix() {
            assert_eq!(
                disposition,
                SyntheticDisposition::Pass,
                "case {name} must fail closed correctly"
            );
        }
    }
}
