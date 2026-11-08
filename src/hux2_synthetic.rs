//! Full synthetic HUX-2 qualification matrix (never live SCM).
//!
//! Reports QUALIFICATION_SYNTHETIC / RESTORE_SYNTHETIC separately from any live
//! disposition. LIVE_QUALIFICATION remains NOT_AUTHORIZED in this Train.

use crate::{
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle, LiveScmExecutionGate,
    MockScmBackend, MockServiceRecord, ScmApiError, ScmControlExecutor, ScmTransactionStage,
    ServiceObservation, ServiceStartMode,
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

/// Runs the canonical synthetic lifecycle with an open mock gate.
pub fn run_hux2_synthetic_happy_path() -> Hux2SyntheticReceipt {
    let mock = MockScmBackend::new();
    mock.insert(MockServiceRecord {
        observation: ServiceObservation {
            service_name: "fixture.scm.hux2".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
        },
        worker_busy: false,
        expected_image_token: "hux2-image".to_owned(),
    });
    let gate = LiveScmExecutionGate {
        allow_start: true,
        allow_stop: true,
        allow_change_config: true,
    };
    let executor = ScmControlExecutor::new(
        mock.clone(),
        "fixture.scm.hux2",
        "hux2-image",
        true,
        gate,
    );

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

    // 9–10 restore to original Automatic+Running
    let plan = stop
        .restore
        .clone()
        .or(start.restore.clone())
        .expect("plan");
    let restored = executor.restore(&plan);
    let restore_ok = restored.stage == ScmTransactionStage::Completed
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
        mock.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.scm.hux2".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            expected_image_token: "hux2-image".to_owned(),
        });
        mock.set_fail_next(ScmApiError::AccessDenied);
        let executor = ScmControlExecutor::new(
            mock,
            "fixture.scm.hux2",
            "hux2-image",
            true,
            LiveScmExecutionGate {
                allow_stop: true,
                allow_start: true,
                allow_change_config: false,
            },
        );
        let disposition = match executor.observe_verified() {
            Err(ScmApiError::AccessDenied) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("access-denied", disposition));
    }

    // Busy worker sudden appearance blocks stop
    {
        let mock = MockScmBackend::new();
        mock.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.scm.hux2".to_owned(),
                state: DiscoveredLifecycle::Busy,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: true,
            expected_image_token: "hux2-image".to_owned(),
        });
        let executor = ScmControlExecutor::new(
            mock.clone(),
            "fixture.scm.hux2",
            "hux2-image",
            true,
            LiveScmExecutionGate {
                allow_stop: true,
                allow_start: false,
                allow_change_config: false,
            },
        );
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
        mock.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.scm.other".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            expected_image_token: "hux2-image".to_owned(),
        });
        let executor = ScmControlExecutor::new(
            mock,
            "fixture.scm.hux2",
            "hux2-image",
            true,
            LiveScmExecutionGate::CLOSED,
        );
        let disposition = match executor.observe_verified() {
            Err(ScmApiError::NotFound) => SyntheticDisposition::Pass,
            _ => SyntheticDisposition::Fail,
        };
        out.push(("service-name-drift", disposition));
    }

    // Closed gate must not mutate even when phase wants stop
    {
        let mock = MockScmBackend::new();
        mock.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.scm.hux2".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            expected_image_token: "hux2-image".to_owned(),
        });
        let executor = ScmControlExecutor::new(
            mock.clone(),
            "fixture.scm.hux2",
            "hux2-image",
            true,
            LiveScmExecutionGate::CLOSED,
        );
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
        {
            SyntheticDisposition::Pass
        } else {
            SyntheticDisposition::Fail
        };
        out.push(("closed-gate-no-mutation", disposition));
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
