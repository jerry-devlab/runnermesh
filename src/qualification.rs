//! Mechanism-neutral readiness and transaction contracts for a future H1.
//!
//! This module performs no host, runner, GitHub, service, registration, or
//! work-root I/O. It makes the fail-closed gate and restore/result separation
//! executable before mechanism-specific adapters are authorized.

use serde::{Deserialize, Serialize};

pub const QUALIFICATION_READINESS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceState {
    Pass,
    Fail,
    Unknown,
}

impl EvidenceState {
    fn is_pass(self) -> bool {
        self == Self::Pass
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadinessCheck {
    SchemaVersion,
    SourceReady,
    HostPrestateReady,
    RoutingReady,
    TrustedWorkflowReady,
    RollbackReady,
    RecoveryReady,
    SelectorUnique,
    OwnerGateReady,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationReadinessEvidence {
    pub schema_version: u32,
    pub source_ready: EvidenceState,
    pub host_prestate_ready: EvidenceState,
    pub routing_ready: EvidenceState,
    pub trusted_workflow_ready: EvidenceState,
    pub rollback_ready: EvidenceState,
    pub recovery_ready: EvidenceState,
    pub selector_unique: EvidenceState,
    pub owner_gate_ready: EvidenceState,
}

impl QualificationReadinessEvidence {
    #[cfg(test)]
    fn all_pass() -> Self {
        Self {
            schema_version: QUALIFICATION_READINESS_SCHEMA_VERSION,
            source_ready: EvidenceState::Pass,
            host_prestate_ready: EvidenceState::Pass,
            routing_ready: EvidenceState::Pass,
            trusted_workflow_ready: EvidenceState::Pass,
            rollback_ready: EvidenceState::Pass,
            recovery_ready: EvidenceState::Pass,
            selector_unique: EvidenceState::Pass,
            owner_gate_ready: EvidenceState::Pass,
        }
    }

    fn checks(self) -> [(ReadinessCheck, EvidenceState); 8] {
        [
            (ReadinessCheck::SourceReady, self.source_ready),
            (ReadinessCheck::HostPrestateReady, self.host_prestate_ready),
            (ReadinessCheck::RoutingReady, self.routing_ready),
            (
                ReadinessCheck::TrustedWorkflowReady,
                self.trusted_workflow_ready,
            ),
            (ReadinessCheck::RollbackReady, self.rollback_ready),
            (ReadinessCheck::RecoveryReady, self.recovery_ready),
            (ReadinessCheck::SelectorUnique, self.selector_unique),
            (ReadinessCheck::OwnerGateReady, self.owner_gate_ready),
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadinessDisposition {
    ReadyForOwnerGate,
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadinessBlocker {
    pub check: ReadinessCheck,
    pub state: EvidenceState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualificationReadinessReceipt {
    pub schema_version: u32,
    pub disposition: ReadinessDisposition,
    pub evidence: QualificationReadinessEvidence,
    pub blockers: Vec<ReadinessBlocker>,
    pub h1_mutation_allowed: bool,
}

/// Pure readiness entrypoint. Any `FAIL`, `UNKNOWN`, or schema mismatch denies
/// H1 mutation.
pub fn qualify_readiness(
    evidence: QualificationReadinessEvidence,
) -> QualificationReadinessReceipt {
    let mut blockers = Vec::new();

    if evidence.schema_version != QUALIFICATION_READINESS_SCHEMA_VERSION {
        blockers.push(ReadinessBlocker {
            check: ReadinessCheck::SchemaVersion,
            state: EvidenceState::Unknown,
        });
    }

    blockers.extend(
        evidence
            .checks()
            .into_iter()
            .filter(|(_, state)| !state.is_pass())
            .map(|(check, state)| ReadinessBlocker { check, state }),
    );

    let h1_mutation_allowed = blockers.is_empty();
    QualificationReadinessReceipt {
        schema_version: QUALIFICATION_READINESS_SCHEMA_VERSION,
        disposition: if h1_mutation_allowed {
            ReadinessDisposition::ReadyForOwnerGate
        } else {
            ReadinessDisposition::Blocked
        },
        evidence,
        blockers,
        h1_mutation_allowed,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QualificationDisposition {
    Pass,
    Fail,
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RestoreDisposition {
    Pass,
    Fail,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum H1TransactionPhase {
    Prepared,
    HostMutationStarted,
    WorkflowRunning,
    RestorePending,
    Restoring,
    Complete,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H1TransactionEvent {
    PreMutationFailure,
    HostMutationBegins,
    WorkflowDispatched,
    WorkflowNeverDispatched,
    WorkflowPassed,
    WorkflowFailed,
    JobTimedOut,
    ControllerLost,
    AgentLost,
    BeginAutomaticRestore,
    RestorePassed,
    RestoreFailed,
    RestoreInterrupted,
    UnrelatedRunnerAppears,
    OwnershipBecomesAmbiguous,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct H1TransactionReceipt {
    pub qualification: QualificationDisposition,
    pub restore: RestoreDisposition,
    pub host_mutation_started: bool,
    pub automatic_restore_attempted: bool,
    pub emergency_owner_recovery_required: bool,
    pub unrelated_runner_control_actions: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct H1TransactionModel {
    phase: H1TransactionPhase,
    qualification: Option<QualificationDisposition>,
    restore: Option<RestoreDisposition>,
    host_mutation_started: bool,
    automatic_restore_attempted: bool,
    unrelated_runner_control_actions: u32,
}

impl Default for H1TransactionModel {
    fn default() -> Self {
        Self::new()
    }
}

impl H1TransactionModel {
    pub fn new() -> Self {
        Self {
            phase: H1TransactionPhase::Prepared,
            qualification: None,
            restore: None,
            host_mutation_started: false,
            automatic_restore_attempted: false,
            unrelated_runner_control_actions: 0,
        }
    }

    pub fn phase(self) -> H1TransactionPhase {
        self.phase
    }

    pub fn apply(&mut self, event: H1TransactionEvent) -> Result<(), &'static str> {
        match event {
            H1TransactionEvent::PreMutationFailure
                if self.phase == H1TransactionPhase::Prepared =>
            {
                self.complete_without_mutation(QualificationDisposition::Blocked);
            }
            H1TransactionEvent::HostMutationBegins
                if self.phase == H1TransactionPhase::Prepared =>
            {
                self.host_mutation_started = true;
                self.phase = H1TransactionPhase::HostMutationStarted;
            }
            H1TransactionEvent::WorkflowDispatched
                if self.phase == H1TransactionPhase::HostMutationStarted =>
            {
                self.phase = H1TransactionPhase::WorkflowRunning;
            }
            H1TransactionEvent::WorkflowNeverDispatched
                if self.phase == H1TransactionPhase::HostMutationStarted =>
            {
                self.request_restore(QualificationDisposition::Blocked);
            }
            H1TransactionEvent::WorkflowPassed
                if self.phase == H1TransactionPhase::WorkflowRunning =>
            {
                self.request_restore(QualificationDisposition::Pass);
            }
            H1TransactionEvent::WorkflowFailed
                if self.phase == H1TransactionPhase::WorkflowRunning =>
            {
                self.request_restore(QualificationDisposition::Fail);
            }
            H1TransactionEvent::JobTimedOut
                if matches!(
                    self.phase,
                    H1TransactionPhase::HostMutationStarted | H1TransactionPhase::WorkflowRunning
                ) =>
            {
                self.request_restore(QualificationDisposition::Blocked);
            }
            H1TransactionEvent::ControllerLost | H1TransactionEvent::AgentLost => {
                if self.host_mutation_started {
                    if !matches!(
                        self.phase,
                        H1TransactionPhase::Complete | H1TransactionPhase::RecoveryRequired
                    ) {
                        self.request_restore(QualificationDisposition::Blocked);
                    }
                } else if self.phase == H1TransactionPhase::Prepared {
                    self.complete_without_mutation(QualificationDisposition::Blocked);
                } else {
                    return Err("loss event is invalid in the current phase");
                }
            }
            H1TransactionEvent::BeginAutomaticRestore
                if self.phase == H1TransactionPhase::RestorePending =>
            {
                self.automatic_restore_attempted = true;
                self.phase = H1TransactionPhase::Restoring;
            }
            H1TransactionEvent::RestorePassed if self.phase == H1TransactionPhase::Restoring => {
                self.restore = Some(RestoreDisposition::Pass);
                self.phase = H1TransactionPhase::Complete;
            }
            H1TransactionEvent::RestoreFailed | H1TransactionEvent::RestoreInterrupted
                if self.phase == H1TransactionPhase::Restoring =>
            {
                self.restore = Some(RestoreDisposition::Fail);
                self.phase = H1TransactionPhase::RecoveryRequired;
            }
            H1TransactionEvent::UnrelatedRunnerAppears => {
                // Exact target ownership is unchanged and no action is issued.
            }
            H1TransactionEvent::OwnershipBecomesAmbiguous => {
                if self.host_mutation_started {
                    self.qualification
                        .get_or_insert(QualificationDisposition::Blocked);
                    self.restore = Some(RestoreDisposition::Fail);
                    self.phase = H1TransactionPhase::RecoveryRequired;
                } else if self.phase == H1TransactionPhase::Prepared {
                    self.complete_without_mutation(QualificationDisposition::Blocked);
                } else {
                    return Err("ownership ambiguity is invalid in the current phase");
                }
            }
            _ => return Err("event is invalid in the current phase"),
        }

        Ok(())
    }

    pub fn receipt(self) -> Option<H1TransactionReceipt> {
        if !matches!(
            self.phase,
            H1TransactionPhase::Complete | H1TransactionPhase::RecoveryRequired
        ) {
            return None;
        }

        Some(H1TransactionReceipt {
            qualification: self.qualification?,
            restore: self.restore?,
            host_mutation_started: self.host_mutation_started,
            automatic_restore_attempted: self.automatic_restore_attempted,
            emergency_owner_recovery_required: self.phase == H1TransactionPhase::RecoveryRequired,
            unrelated_runner_control_actions: self.unrelated_runner_control_actions,
        })
    }

    fn request_restore(&mut self, qualification: QualificationDisposition) {
        self.qualification.get_or_insert(qualification);
        self.phase = H1TransactionPhase::RestorePending;
    }

    fn complete_without_mutation(&mut self, qualification: QualificationDisposition) {
        self.qualification = Some(qualification);
        self.restore = Some(RestoreDisposition::Pass);
        self.phase = H1TransactionPhase::Complete;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finish_restore(model: &mut H1TransactionModel, restore_passes: bool) {
        model
            .apply(H1TransactionEvent::BeginAutomaticRestore)
            .unwrap();
        model
            .apply(if restore_passes {
                H1TransactionEvent::RestorePassed
            } else {
                H1TransactionEvent::RestoreFailed
            })
            .unwrap();
    }

    fn mutated_model() -> H1TransactionModel {
        let mut model = H1TransactionModel::new();
        model.apply(H1TransactionEvent::HostMutationBegins).unwrap();
        model
    }

    #[test]
    fn readiness_all_pass_is_the_only_owner_gate_admission() {
        let receipt = qualify_readiness(QualificationReadinessEvidence::all_pass());
        assert_eq!(receipt.disposition, ReadinessDisposition::ReadyForOwnerGate);
        assert!(receipt.h1_mutation_allowed);
        assert!(receipt.blockers.is_empty());
    }

    #[test]
    fn every_false_or_unknown_readiness_field_blocks_h1() {
        let setters: [fn(&mut QualificationReadinessEvidence, EvidenceState); 8] = [
            |evidence, state| evidence.source_ready = state,
            |evidence, state| evidence.host_prestate_ready = state,
            |evidence, state| evidence.routing_ready = state,
            |evidence, state| evidence.trusted_workflow_ready = state,
            |evidence, state| evidence.rollback_ready = state,
            |evidence, state| evidence.recovery_ready = state,
            |evidence, state| evidence.selector_unique = state,
            |evidence, state| evidence.owner_gate_ready = state,
        ];

        for setter in setters {
            for state in [EvidenceState::Fail, EvidenceState::Unknown] {
                let mut evidence = QualificationReadinessEvidence::all_pass();
                setter(&mut evidence, state);
                let receipt = qualify_readiness(evidence);
                assert_eq!(receipt.disposition, ReadinessDisposition::Blocked);
                assert!(!receipt.h1_mutation_allowed);
                assert_eq!(receipt.blockers.len(), 1);
                assert_eq!(receipt.blockers[0].state, state);
            }
        }
    }

    #[test]
    fn unknown_schema_blocks_without_erasing_field_evidence() {
        let mut evidence = QualificationReadinessEvidence::all_pass();
        evidence.schema_version += 1;
        let receipt = qualify_readiness(evidence);
        assert_eq!(receipt.disposition, ReadinessDisposition::Blocked);
        assert!(!receipt.h1_mutation_allowed);
        assert_eq!(receipt.evidence.schema_version, 2);
    }

    #[test]
    fn readiness_json_uses_stable_machine_keys_and_values() {
        let json =
            serde_json::to_value(qualify_readiness(QualificationReadinessEvidence::all_pass()))
                .unwrap();

        assert_eq!(json["disposition"], "READY_FOR_OWNER_GATE");
        assert_eq!(json["evidence"]["source_ready"], "PASS");
        assert_eq!(json["evidence"]["owner_gate_ready"], "PASS");
        assert_eq!(json["h1_mutation_allowed"], true);
    }

    #[test]
    fn readiness_input_rejects_unknown_fields() {
        let mut json = serde_json::to_value(QualificationReadinessEvidence::all_pass()).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("invented_ready".to_owned(), serde_json::Value::Bool(true));

        assert!(serde_json::from_value::<QualificationReadinessEvidence>(json).is_err());
    }

    #[test]
    fn failure_before_mutation_is_blocked_with_verified_unchanged_restore() {
        let mut model = H1TransactionModel::new();
        model.apply(H1TransactionEvent::PreMutationFailure).unwrap();
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
        assert!(!receipt.host_mutation_started);
        assert!(!receipt.automatic_restore_attempted);
    }

    #[test]
    fn workflow_never_dispatching_still_attempts_restore() {
        let mut model = mutated_model();
        model
            .apply(H1TransactionEvent::WorkflowNeverDispatched)
            .unwrap();
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
        assert!(receipt.automatic_restore_attempted);
    }

    #[test]
    fn workflow_failure_and_restore_have_independent_results() {
        let mut model = mutated_model();
        model.apply(H1TransactionEvent::WorkflowDispatched).unwrap();
        model.apply(H1TransactionEvent::WorkflowFailed).unwrap();
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Fail);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
    }

    #[test]
    fn successful_qualification_still_restores_before_completion() {
        let mut model = mutated_model();
        model.apply(H1TransactionEvent::WorkflowDispatched).unwrap();
        model.apply(H1TransactionEvent::WorkflowPassed).unwrap();
        assert!(model.receipt().is_none());
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Pass);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
    }

    #[test]
    fn job_timeout_attempts_restore_and_reports_blocked() {
        let mut model = mutated_model();
        model.apply(H1TransactionEvent::WorkflowDispatched).unwrap();
        model.apply(H1TransactionEvent::JobTimedOut).unwrap();
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
    }

    #[test]
    fn controller_or_agent_loss_after_mutation_enters_restore_path() {
        for loss in [
            H1TransactionEvent::ControllerLost,
            H1TransactionEvent::AgentLost,
        ] {
            let mut model = mutated_model();
            model.apply(loss).unwrap();
            assert_eq!(model.phase(), H1TransactionPhase::RestorePending);
            finish_restore(&mut model, true);
            let receipt = model.receipt().unwrap();
            assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
            assert_eq!(receipt.restore, RestoreDisposition::Pass);
        }
    }

    #[test]
    fn restore_interruption_requires_emergency_owner_recovery() {
        let mut model = mutated_model();
        model.apply(H1TransactionEvent::WorkflowDispatched).unwrap();
        model.apply(H1TransactionEvent::WorkflowFailed).unwrap();
        model
            .apply(H1TransactionEvent::BeginAutomaticRestore)
            .unwrap();
        model.apply(H1TransactionEvent::RestoreInterrupted).unwrap();
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Fail);
        assert_eq!(receipt.restore, RestoreDisposition::Fail);
        assert!(receipt.emergency_owner_recovery_required);
    }

    #[test]
    fn restore_failure_does_not_erase_a_successful_qualification() {
        let mut model = mutated_model();
        model.apply(H1TransactionEvent::WorkflowDispatched).unwrap();
        model.apply(H1TransactionEvent::WorkflowPassed).unwrap();
        finish_restore(&mut model, false);
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Pass);
        assert_eq!(receipt.restore, RestoreDisposition::Fail);
        assert!(receipt.emergency_owner_recovery_required);
    }

    #[test]
    fn invalid_transition_refuses_without_advancing_phase() {
        let mut model = H1TransactionModel::new();
        assert!(model.apply(H1TransactionEvent::WorkflowDispatched).is_err());
        assert_eq!(model.phase(), H1TransactionPhase::Prepared);
        assert!(model.receipt().is_none());
    }

    #[test]
    fn unrelated_runner_presence_never_creates_a_control_action() {
        let mut model = mutated_model();
        model
            .apply(H1TransactionEvent::UnrelatedRunnerAppears)
            .unwrap();
        model
            .apply(H1TransactionEvent::WorkflowNeverDispatched)
            .unwrap();
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.unrelated_runner_control_actions, 0);
    }

    #[test]
    fn ambiguous_ownership_refuses_before_mutation() {
        let mut model = H1TransactionModel::new();
        model
            .apply(H1TransactionEvent::OwnershipBecomesAmbiguous)
            .unwrap();
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
        assert!(!receipt.host_mutation_started);
    }

    #[test]
    fn ambiguous_ownership_after_mutation_requires_owner_recovery() {
        let mut model = mutated_model();
        model
            .apply(H1TransactionEvent::OwnershipBecomesAmbiguous)
            .unwrap();
        let receipt = model.receipt().unwrap();

        assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
        assert_eq!(receipt.restore, RestoreDisposition::Fail);
        assert!(receipt.emergency_owner_recovery_required);
    }
}
