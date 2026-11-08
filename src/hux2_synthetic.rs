//! Full synthetic HUX-2 qualification matrix (never live SCM).
//!
//! Reports QUALIFICATION_SYNTHETIC / RESTORE_SYNTHETIC separately from any live
//! disposition. LIVE_QUALIFICATION remains NOT_AUTHORIZED in this Train.

use crate::{
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle, LiveScmExecutionGate,
    MockScmBackend, MockServiceRecord, OwnerScmAuthorizationProof, ScmApiError, ScmControlExecutor,
    ScmTransactionStage, ServiceObservation, ServiceStartMode,
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

fn sealed_gate(
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
) -> LiveScmExecutionGate {
    LiveScmExecutionGate::from_owner_authorization(
        &OwnerScmAuthorizationProof::synthetic_for_tests(
            "fixture.scm.hux2",
            "hux2-image",
            allow_start,
            allow_stop,
            allow_change_config,
        ),
    )
    .expect("synthetic seal")
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

/// Runs the canonical synthetic lifecycle with an open mock gate.
pub fn run_hux2_synthetic_happy_path() -> Hux2SyntheticReceipt {
    let mock = MockScmBackend::new();
    mock.insert(fixture_record(
        "fixture.scm.hux2",
        DiscoveredLifecycle::Listening,
        ServiceStartMode::Automatic,
        false,
    ));
    let gate = sealed_gate(true, true, true);
    let executor = executor(mock.clone(), gate);

    // 1–2 prestate + identity
    let prestate = match executor.capture_prestate(false, false) {
        Ok(prestate) => prestate,
        Err(_) => {
            return Hux2SyntheticReceipt {
                qualification_synthetic: SyntheticDisposition::Fail,
                restore_synthetic: SyntheticDisposition::Fail,
                live_qualification: "NOT_AUTHORIZED",
                independent_security_review: "PENDING",
                notes: vec!["prestate-capture-failed"],
            };
        }
    };

    // 3 withdraw admission (managed)
    let withdraw = executor.advance(
        DesiredServiceLifecycle::DrainThenStop,
        false,
        false,
        true,
        &prestate,
    );
    if withdraw.stage != ScmTransactionStage::WithdrawingAdmission {
        return Hux2SyntheticReceipt {
            qualification_synthetic: SyntheticDisposition::Fail,
            restore_synthetic: SyntheticDisposition::Blocked,
            live_qualification: "NOT_AUTHORIZED",
            independent_security_review: "PENDING",
            notes: vec!["admission-withdraw-phase-missing"],
        };
    }

    // 4–6 idle + stop + readback
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
        return Hux2SyntheticReceipt {
            qualification_synthetic: SyntheticDisposition::Fail,
            restore_synthetic: SyntheticDisposition::Blocked,
            live_qualification: "NOT_AUTHORIZED",
            independent_security_review: "PENDING",
            notes: vec!["stop-readback-failed"],
        };
    }

    // 7–8 start + readback
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
        return Hux2SyntheticReceipt {
            qualification_synthetic: SyntheticDisposition::Fail,
            restore_synthetic: SyntheticDisposition::Blocked,
            live_qualification: "NOT_AUTHORIZED",
            independent_security_review: "PENDING",
            notes: vec!["start-readback-failed"],
        };
    }

    // 9–10 restore to original Automatic+Running with independent admission readback
    let plan = stop
        .restore
        .clone()
        .or(start.restore.clone())
        .expect("plan");
    let restored = executor.restore(&plan, plan.admission_selector_absent);
    let restore_ok = restored.restore_pass
        && restored.stage == ScmTransactionStage::Completed
        && restored.achieved == AchievedServiceControl::Running;

    Hux2SyntheticReceipt {
        qualification_synthetic: SyntheticDisposition::Pass,
        restore_synthetic: if restore_ok {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        },
        live_qualification: "NOT_AUTHORIZED",
        independent_security_review: "PENDING",
        notes: vec!["synthetic-happy-path"],
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
        let executor = executor(mock, sealed_gate(true, true, false));
        let disposition = match executor.observe_verified() {
            Err(ScmApiError::AccessDenied) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("access-denied", disposition));
    }

    // Busy worker sudden appearance blocks stop
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Busy,
            ServiceStartMode::Manual,
            true,
        ));
        let executor = executor(mock.clone(), sealed_gate(false, true, false));
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

    // Closed gate must not mutate even when phase wants stop
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

    // Incomplete observation flags must not be overwritten into trust
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

    // Restore must not PASS when change-config fails mid-restore
    {
        let mock = MockScmBackend::new();
        mock.insert(fixture_record(
            "fixture.scm.hux2",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Automatic,
            false,
        ));
        mock.set_fail_change(ScmApiError::Timeout);
        let executor = executor(mock, sealed_gate(true, true, true));
        let prestate = executor.capture_prestate(false, true).unwrap();
        let plan = crate::capture_restore_plan(&prestate);
        let receipt = executor.restore(&plan, true);
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
