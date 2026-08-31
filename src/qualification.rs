//! Fail-closed readiness and transaction contracts for the future H1.
//!
//! This module performs no host, runner, GitHub, service, registration,
//! credential, work-root, or workflow I/O. It makes the accepted dynamic-label
//! architecture executable against synthetic evidence while keeping live
//! evidence and Owner authorization as separate future boundaries.

use serde::{Deserialize, Serialize};

use crate::RESERVED_ADMISSION_LABEL;

pub const H1_READINESS_SCHEMA_VERSION: u32 = 2;
pub const H1_TRANSACTION_SCHEMA_VERSION: u32 = 1;
pub const H1_TRANSACTION_FAMILY_ID: &str = "h1-github-native-admission-label-v1";

const H1_WORKFLOW_TEMPLATE: &str = include_str!("../docs/qualification/templates/h1-workflow.yml");

pub fn h1_workflow_template() -> &'static str {
    H1_WORKFLOW_TEMPLATE
}

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
pub enum EvidenceProvenance {
    Live,
    Synthetic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadinessCheck {
    SchemaVersion,
    SourceReady,
    HostPrestateReady,
    GithubAuthorityConfigured,
    ExactRunnerIdentityReady,
    ReservedSelectorReady,
    SelectorUnique,
    TrustedWorkflowReady,
    RoutingReady,
    RollbackReady,
    RecoveryReady,
    OwnerGateReady,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct H1ReadinessEvidence {
    pub schema_version: u32,
    pub provenance: EvidenceProvenance,
    pub source_ready: EvidenceState,
    pub host_prestate_ready: EvidenceState,
    pub github_authority_configured: EvidenceState,
    pub exact_runner_identity_ready: EvidenceState,
    pub reserved_selector_ready: EvidenceState,
    pub selector_unique: EvidenceState,
    pub trusted_workflow_ready: EvidenceState,
    pub routing_ready: EvidenceState,
    pub rollback_ready: EvidenceState,
    pub recovery_ready: EvidenceState,
    pub owner_gate_ready: EvidenceState,
}

impl H1ReadinessEvidence {
    pub fn unknown_live() -> Self {
        Self {
            schema_version: H1_READINESS_SCHEMA_VERSION,
            provenance: EvidenceProvenance::Live,
            source_ready: EvidenceState::Unknown,
            host_prestate_ready: EvidenceState::Unknown,
            github_authority_configured: EvidenceState::Unknown,
            exact_runner_identity_ready: EvidenceState::Unknown,
            reserved_selector_ready: EvidenceState::Unknown,
            selector_unique: EvidenceState::Unknown,
            trusted_workflow_ready: EvidenceState::Unknown,
            routing_ready: EvidenceState::Unknown,
            rollback_ready: EvidenceState::Unknown,
            recovery_ready: EvidenceState::Unknown,
            owner_gate_ready: EvidenceState::Unknown,
        }
    }

    fn checks(self) -> [(ReadinessCheck, EvidenceState); 11] {
        [
            (ReadinessCheck::SourceReady, self.source_ready),
            (ReadinessCheck::HostPrestateReady, self.host_prestate_ready),
            (
                ReadinessCheck::GithubAuthorityConfigured,
                self.github_authority_configured,
            ),
            (
                ReadinessCheck::ExactRunnerIdentityReady,
                self.exact_runner_identity_ready,
            ),
            (
                ReadinessCheck::ReservedSelectorReady,
                self.reserved_selector_ready,
            ),
            (ReadinessCheck::SelectorUnique, self.selector_unique),
            (
                ReadinessCheck::TrustedWorkflowReady,
                self.trusted_workflow_ready,
            ),
            (ReadinessCheck::RoutingReady, self.routing_ready),
            (ReadinessCheck::RollbackReady, self.rollback_ready),
            (ReadinessCheck::RecoveryReady, self.recovery_ready),
            (ReadinessCheck::OwnerGateReady, self.owner_gate_ready),
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadinessDisposition {
    ReadyForOwnerGate,
    PassSynthetic,
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadinessBlocker {
    pub check: ReadinessCheck,
    pub state: EvidenceState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct H1ReadinessReceipt {
    pub schema_version: u32,
    pub disposition: ReadinessDisposition,
    pub evidence: H1ReadinessEvidence,
    pub blockers: Vec<ReadinessBlocker>,
    pub h1_mutation_allowed: bool,
}

/// Pure readiness entrypoint. Synthetic evidence can prove the verifier but
/// can never authorize H1. Live evidence permits the future Owner transaction
/// only when every required field is positively proved under this schema.
pub fn verify_h1_readiness(evidence: H1ReadinessEvidence) -> H1ReadinessReceipt {
    let mut blockers = Vec::new();

    if evidence.schema_version != H1_READINESS_SCHEMA_VERSION {
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

    let all_checks_pass = blockers.is_empty();
    let h1_mutation_allowed = all_checks_pass && evidence.provenance == EvidenceProvenance::Live;
    let disposition = if !all_checks_pass {
        ReadinessDisposition::Blocked
    } else if evidence.provenance == EvidenceProvenance::Synthetic {
        ReadinessDisposition::PassSynthetic
    } else {
        ReadinessDisposition::ReadyForOwnerGate
    };

    H1ReadinessReceipt {
        schema_version: H1_READINESS_SCHEMA_VERSION,
        disposition,
        evidence,
        blockers,
        h1_mutation_allowed,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct H1WorkflowTemplateAssessment {
    pub workflow_dispatch_only: bool,
    pub reserved_selector_exact: bool,
    pub runtime_identity_asserted: bool,
    pub arbitrary_command_input_absent: bool,
    pub secret_context_absent: bool,
}

impl H1WorkflowTemplateAssessment {
    pub fn source_contract_ready(self) -> bool {
        self.workflow_dispatch_only
            && self.reserved_selector_exact
            && self.runtime_identity_asserted
            && self.arbitrary_command_input_absent
            && self.secret_context_absent
    }
}

/// Checks only the inert public template's source contract. It does not claim
/// that a trusted private workflow, repository variable, or route exists.
pub fn assess_h1_workflow_template() -> H1WorkflowTemplateAssessment {
    assess_h1_workflow_source(H1_WORKFLOW_TEMPLATE)
}

/// Deterministically assesses fetched or generated workflow source. Repository,
/// ref, and blob identity remain separate verification inputs.
pub fn assess_h1_workflow_source(source: &str) -> H1WorkflowTemplateAssessment {
    let lowercase = source.to_ascii_lowercase();
    let secret_context = ["secrets", "."].concat();
    let triggers = top_level_child_keys(source, "on", 2);
    let workflow_inputs = nested_child_keys(source, "workflow_dispatch", "inputs", 6);
    let selectors = list_values(source, "runs-on");
    H1WorkflowTemplateAssessment {
        workflow_dispatch_only: triggers == ["workflow_dispatch"],
        reserved_selector_exact: selectors
            == ["self-hosted", "Windows", "X64", RESERVED_ADMISSION_LABEL],
        runtime_identity_asserted: source.contains("H1_OBSERVED_RUNNER_NAME: ${{ runner.name }}")
            && source.contains("RUNNERMESH_EXPECTED_RUNNER_NAME")
            && source.contains("-cne $env:H1_OBSERVED_RUNNER_NAME"),
        arbitrary_command_input_absent: workflow_inputs
            == ["witness", "candidate_sha", "transaction_id"]
            && !lowercase.contains("inputs.command")
            && !lowercase.contains("inputs.script")
            && !lowercase.contains("run: ${{ inputs.")
            && !lowercase.contains("invoke-expression"),
        secret_context_absent: !lowercase.contains(&secret_context),
    }
}

fn top_level_child_keys<'a>(source: &'a str, root: &str, child_indent: usize) -> Vec<&'a str> {
    let lines = source.lines().collect::<Vec<_>>();
    let Some(root_index) = lines
        .iter()
        .position(|line| indentation(line) == 0 && line.trim() == format!("{root}:"))
    else {
        return Vec::new();
    };
    lines[root_index + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || indentation(line) > 0)
        .filter_map(|line| yaml_key_at_indent(line, child_indent))
        .collect()
}

fn nested_child_keys<'a>(
    source: &'a str,
    parent: &str,
    container: &str,
    child_indent: usize,
) -> Vec<&'a str> {
    let lines = source.lines().collect::<Vec<_>>();
    let Some(parent_index) = lines
        .iter()
        .position(|line| line.trim() == format!("{parent}:"))
    else {
        return Vec::new();
    };
    let parent_indent = indentation(lines[parent_index]);
    let Some(container_offset) = lines[parent_index + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || indentation(line) > parent_indent)
        .position(|line| line.trim() == format!("{container}:"))
    else {
        return Vec::new();
    };
    let container_index = parent_index + 1 + container_offset;
    let container_indent = indentation(lines[container_index]);
    lines[container_index + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || indentation(line) > container_indent)
        .filter_map(|line| yaml_key_at_indent(line, child_indent))
        .collect()
}

fn list_values<'a>(source: &'a str, key: &str) -> Vec<&'a str> {
    let lines = source.lines().collect::<Vec<_>>();
    let Some(key_index) = lines
        .iter()
        .position(|line| line.trim() == format!("{key}:"))
    else {
        return Vec::new();
    };
    let key_indent = indentation(lines[key_index]);
    lines[key_index + 1..]
        .iter()
        .take_while(|line| line.trim().is_empty() || indentation(line) > key_indent)
        .filter_map(|line| line.trim().strip_prefix("- ").map(str::trim))
        .collect()
}

fn yaml_key_at_indent(line: &str, expected_indent: usize) -> Option<&str> {
    (indentation(line) == expected_indent)
        .then(|| line.trim().strip_suffix(':'))
        .flatten()
}

fn indentation(line: &str) -> usize {
    line.len().saturating_sub(line.trim_start().len())
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
pub enum BaselineAdmissionState {
    Advertised,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct H1RestoreBaseline {
    pub admission: BaselineAdmissionState,
    pub local_runner_expected_online: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum H1TransactionPhase {
    Prepared,
    OwnerAuthorized,
    AdmissionControlEstablished,
    Advertised,
    PrimaryJobRunning,
    PrimaryJobCompleted,
    Withdrawing,
    SelectorAbsent,
    DrainPending,
    NoNewAdmissionWitnessed,
    Drained,
    ReAdvertising,
    ReAdvertised,
    RestorePending,
    Restoring,
    Complete,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H1TransactionEvent {
    PreOwnerGateBlocked,
    OwnerGateAccepted,
    AdmissionControlEstablished { mutation_performed: bool },
    AdvertisedCapacityQualified,
    PrimaryTrustedJobStarted,
    PrimaryTrustedJobPassed,
    PrimaryTrustedJobFailed,
    WithdrawalRequested,
    SelectorAbsenceObserved,
    NoNewAdmissionWitnessed { racing_assignment_observed: bool },
    NoNewAdmissionWitnessFailed,
    ActiveWorkerCompleted,
    AchievedDrainedObserved,
    ReadvertiseRequested,
    SelectorPresenceObserved,
    ReconnectWitnessPassed,
    RoutingUnavailable,
    ActiveJobTimedOut,
    SelectorObservationUnknown,
    ControllerLost,
    AgentLost,
    BeginAutomaticRestore,
    RestorePassed,
    RestoreFailed,
    RestoreInterrupted,
    UnrelatedRunnerObserved,
    OwnershipBecameAmbiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H1TransactionError {
    ReadinessNotLiveAndComplete,
    InvalidTransition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct H1TransactionReceipt {
    pub schema_version: u32,
    pub transaction_family: String,
    pub qualification: QualificationDisposition,
    pub restore: RestoreDisposition,
    pub baseline: H1RestoreBaseline,
    pub external_mutation_started: bool,
    pub automatic_restore_attempted: bool,
    pub emergency_owner_recovery_required: bool,
    pub unrelated_runner_control_actions: u32,
}

/// Durable, provider-neutral state for exactly one future H1 transaction
/// family. Exact runner/repository/workflow identifiers remain in the private
/// transaction envelope rather than this public state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct H1TransactionModel {
    schema_version: u32,
    transaction_family: String,
    phase: H1TransactionPhase,
    qualification: Option<QualificationDisposition>,
    restore: Option<RestoreDisposition>,
    baseline: H1RestoreBaseline,
    external_mutation_started: bool,
    automatic_restore_attempted: bool,
    unrelated_runner_control_actions: u32,
}

impl H1TransactionModel {
    pub fn prepare(
        readiness: &H1ReadinessReceipt,
        baseline: H1RestoreBaseline,
    ) -> Result<Self, H1TransactionError> {
        if readiness.disposition != ReadinessDisposition::ReadyForOwnerGate
            || !readiness.h1_mutation_allowed
        {
            return Err(H1TransactionError::ReadinessNotLiveAndComplete);
        }
        Ok(Self {
            schema_version: H1_TRANSACTION_SCHEMA_VERSION,
            transaction_family: H1_TRANSACTION_FAMILY_ID.to_owned(),
            phase: H1TransactionPhase::Prepared,
            qualification: None,
            restore: None,
            baseline,
            external_mutation_started: false,
            automatic_restore_attempted: false,
            unrelated_runner_control_actions: 0,
        })
    }

    pub fn phase(&self) -> H1TransactionPhase {
        self.phase
    }

    pub fn apply(&mut self, event: H1TransactionEvent) -> Result<(), H1TransactionError> {
        use H1TransactionEvent as Event;
        use H1TransactionPhase as Phase;

        match event {
            Event::PreOwnerGateBlocked if self.phase == Phase::Prepared => {
                self.complete_without_transaction(QualificationDisposition::Blocked);
            }
            Event::OwnerGateAccepted if self.phase == Phase::Prepared => {
                self.phase = Phase::OwnerAuthorized;
            }
            Event::AdmissionControlEstablished { mutation_performed }
                if self.phase == Phase::OwnerAuthorized =>
            {
                self.external_mutation_started |= mutation_performed;
                self.phase = Phase::AdmissionControlEstablished;
            }
            Event::AdvertisedCapacityQualified
                if self.phase == Phase::AdmissionControlEstablished =>
            {
                self.phase = Phase::Advertised;
            }
            Event::PrimaryTrustedJobStarted if self.phase == Phase::Advertised => {
                self.phase = Phase::PrimaryJobRunning;
            }
            Event::PrimaryTrustedJobPassed if self.phase == Phase::PrimaryJobRunning => {
                self.phase = Phase::PrimaryJobCompleted;
            }
            Event::WithdrawalRequested if self.phase == Phase::PrimaryJobCompleted => {
                self.external_mutation_started = true;
                self.phase = Phase::Withdrawing;
            }
            Event::SelectorAbsenceObserved if self.phase == Phase::Withdrawing => {
                self.phase = Phase::SelectorAbsent;
            }
            Event::NoNewAdmissionWitnessed {
                racing_assignment_observed,
            } if self.phase == Phase::SelectorAbsent => {
                self.phase = if racing_assignment_observed {
                    Phase::DrainPending
                } else {
                    Phase::NoNewAdmissionWitnessed
                };
            }
            Event::ActiveWorkerCompleted if self.phase == Phase::DrainPending => {
                self.phase = Phase::NoNewAdmissionWitnessed;
            }
            Event::AchievedDrainedObserved if self.phase == Phase::NoNewAdmissionWitnessed => {
                self.phase = Phase::Drained;
            }
            Event::ReadvertiseRequested if self.phase == Phase::Drained => {
                self.external_mutation_started = true;
                self.phase = Phase::ReAdvertising;
            }
            Event::SelectorPresenceObserved if self.phase == Phase::ReAdvertising => {
                self.phase = Phase::ReAdvertised;
            }
            Event::ReconnectWitnessPassed if self.phase == Phase::ReAdvertised => {
                self.request_restore(QualificationDisposition::Pass);
            }
            Event::PrimaryTrustedJobFailed if self.phase == Phase::PrimaryJobRunning => {
                self.request_restore(QualificationDisposition::Fail);
            }
            Event::NoNewAdmissionWitnessFailed
                if matches!(self.phase, Phase::SelectorAbsent | Phase::DrainPending) =>
            {
                self.request_restore(QualificationDisposition::Fail);
            }
            Event::RoutingUnavailable
            | Event::ActiveJobTimedOut
            | Event::SelectorObservationUnknown
            | Event::AgentLost
                if !self.is_terminal() && self.phase != Phase::Prepared =>
            {
                self.request_restore(QualificationDisposition::Blocked);
            }
            Event::ControllerLost if self.phase == Phase::Restoring => {
                self.restore = Some(RestoreDisposition::Fail);
                self.phase = Phase::RecoveryRequired;
            }
            Event::ControllerLost if !self.is_terminal() && self.phase != Phase::Prepared => {
                self.request_restore(QualificationDisposition::Blocked);
            }
            Event::BeginAutomaticRestore if self.phase == Phase::RestorePending => {
                self.automatic_restore_attempted = true;
                self.phase = Phase::Restoring;
            }
            Event::RestorePassed if self.phase == Phase::Restoring => {
                self.restore = Some(RestoreDisposition::Pass);
                self.phase = Phase::Complete;
            }
            Event::RestoreFailed | Event::RestoreInterrupted if self.phase == Phase::Restoring => {
                self.restore = Some(RestoreDisposition::Fail);
                self.phase = Phase::RecoveryRequired;
            }
            Event::UnrelatedRunnerObserved if !self.is_terminal() => {
                // Observation grants no authority and creates no control action.
            }
            Event::OwnershipBecameAmbiguous if self.phase == Phase::Prepared => {
                self.complete_without_transaction(QualificationDisposition::Blocked);
            }
            Event::OwnershipBecameAmbiguous if !self.is_terminal() => {
                self.qualification
                    .get_or_insert(QualificationDisposition::Blocked);
                self.restore = Some(RestoreDisposition::Fail);
                self.phase = Phase::RecoveryRequired;
            }
            _ => return Err(H1TransactionError::InvalidTransition),
        }
        Ok(())
    }

    pub fn receipt(&self) -> Option<H1TransactionReceipt> {
        if !self.is_terminal() {
            return None;
        }
        Some(H1TransactionReceipt {
            schema_version: H1_TRANSACTION_SCHEMA_VERSION,
            transaction_family: self.transaction_family.clone(),
            qualification: self.qualification?,
            restore: self.restore?,
            baseline: self.baseline,
            external_mutation_started: self.external_mutation_started,
            automatic_restore_attempted: self.automatic_restore_attempted,
            emergency_owner_recovery_required: self.phase == H1TransactionPhase::RecoveryRequired,
            unrelated_runner_control_actions: self.unrelated_runner_control_actions,
        })
    }

    fn request_restore(&mut self, qualification: QualificationDisposition) {
        self.qualification.get_or_insert(qualification);
        self.phase = H1TransactionPhase::RestorePending;
    }

    fn complete_without_transaction(&mut self, qualification: QualificationDisposition) {
        self.qualification = Some(qualification);
        self.restore = Some(RestoreDisposition::Pass);
        self.phase = H1TransactionPhase::Complete;
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.phase,
            H1TransactionPhase::Complete | H1TransactionPhase::RecoveryRequired
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_pass(provenance: EvidenceProvenance) -> H1ReadinessEvidence {
        H1ReadinessEvidence {
            schema_version: H1_READINESS_SCHEMA_VERSION,
            provenance,
            source_ready: EvidenceState::Pass,
            host_prestate_ready: EvidenceState::Pass,
            github_authority_configured: EvidenceState::Pass,
            exact_runner_identity_ready: EvidenceState::Pass,
            reserved_selector_ready: EvidenceState::Pass,
            selector_unique: EvidenceState::Pass,
            trusted_workflow_ready: EvidenceState::Pass,
            routing_ready: EvidenceState::Pass,
            rollback_ready: EvidenceState::Pass,
            recovery_ready: EvidenceState::Pass,
            owner_gate_ready: EvidenceState::Pass,
        }
    }

    fn live_ready() -> H1ReadinessReceipt {
        verify_h1_readiness(all_pass(EvidenceProvenance::Live))
    }

    fn baseline() -> H1RestoreBaseline {
        H1RestoreBaseline {
            admission: BaselineAdmissionState::Advertised,
            local_runner_expected_online: true,
        }
    }

    fn prepared() -> H1TransactionModel {
        H1TransactionModel::prepare(&live_ready(), baseline()).unwrap()
    }

    fn owner_authorized() -> H1TransactionModel {
        let mut model = prepared();
        model.apply(H1TransactionEvent::OwnerGateAccepted).unwrap();
        model
    }

    fn primary_running() -> H1TransactionModel {
        let mut model = owner_authorized();
        model
            .apply(H1TransactionEvent::AdmissionControlEstablished {
                mutation_performed: true,
            })
            .unwrap();
        model
            .apply(H1TransactionEvent::AdvertisedCapacityQualified)
            .unwrap();
        model
            .apply(H1TransactionEvent::PrimaryTrustedJobStarted)
            .unwrap();
        model
    }

    fn finish_restore(model: &mut H1TransactionModel, succeeds: bool) {
        model
            .apply(H1TransactionEvent::BeginAutomaticRestore)
            .unwrap();
        model
            .apply(if succeeds {
                H1TransactionEvent::RestorePassed
            } else {
                H1TransactionEvent::RestoreFailed
            })
            .unwrap();
    }

    #[test]
    fn synthetic_all_pass_proves_the_verifier_but_never_authorizes_h1() {
        let receipt = verify_h1_readiness(all_pass(EvidenceProvenance::Synthetic));
        assert_eq!(receipt.disposition, ReadinessDisposition::PassSynthetic);
        assert!(receipt.blockers.is_empty());
        assert!(!receipt.h1_mutation_allowed);
        assert_eq!(
            H1TransactionModel::prepare(&receipt, baseline()).unwrap_err(),
            H1TransactionError::ReadinessNotLiveAndComplete
        );
    }

    #[test]
    fn every_failed_or_unknown_live_gate_blocks_h1() {
        let setters: [fn(&mut H1ReadinessEvidence, EvidenceState); 11] = [
            |e, value| e.source_ready = value,
            |e, value| e.host_prestate_ready = value,
            |e, value| e.github_authority_configured = value,
            |e, value| e.exact_runner_identity_ready = value,
            |e, value| e.reserved_selector_ready = value,
            |e, value| e.selector_unique = value,
            |e, value| e.trusted_workflow_ready = value,
            |e, value| e.routing_ready = value,
            |e, value| e.rollback_ready = value,
            |e, value| e.recovery_ready = value,
            |e, value| e.owner_gate_ready = value,
        ];
        for setter in setters {
            for value in [EvidenceState::Fail, EvidenceState::Unknown] {
                let mut evidence = all_pass(EvidenceProvenance::Live);
                setter(&mut evidence, value);
                let receipt = verify_h1_readiness(evidence);
                assert_eq!(receipt.disposition, ReadinessDisposition::Blocked);
                assert!(!receipt.h1_mutation_allowed);
                assert_eq!(receipt.blockers.len(), 1);
                assert_eq!(receipt.blockers[0].state, value);
            }
        }
    }

    #[test]
    fn unknown_live_evidence_and_unknown_schema_fail_closed() {
        let receipt = verify_h1_readiness(H1ReadinessEvidence::unknown_live());
        assert_eq!(receipt.blockers.len(), 11);
        assert!(!receipt.h1_mutation_allowed);

        let mut evidence = all_pass(EvidenceProvenance::Live);
        evidence.schema_version += 1;
        let receipt = verify_h1_readiness(evidence);
        assert_eq!(receipt.blockers[0].check, ReadinessCheck::SchemaVersion);
        assert!(!receipt.h1_mutation_allowed);
    }

    #[test]
    fn readiness_json_uses_all_stable_required_keys() {
        let json =
            serde_json::to_value(verify_h1_readiness(all_pass(EvidenceProvenance::Synthetic)))
                .unwrap();
        assert_eq!(json["disposition"], "PASS_SYNTHETIC");
        assert_eq!(json["evidence"]["source_ready"], "PASS");
        assert_eq!(json["evidence"]["github_authority_configured"], "PASS");
        assert_eq!(json["evidence"]["exact_runner_identity_ready"], "PASS");
        assert_eq!(json["evidence"]["reserved_selector_ready"], "PASS");
        assert_eq!(json["evidence"]["owner_gate_ready"], "PASS");
        assert_eq!(json["h1_mutation_allowed"], false);
    }

    #[test]
    fn inert_workflow_template_has_the_label_identity_and_trigger_contract() {
        let assessment = assess_h1_workflow_template();
        assert!(assessment.source_contract_ready());
        assert!(H1_WORKFLOW_TEMPLATE.contains("workflow_dispatch"));
        assert!(!H1_WORKFLOW_TEMPLATE.contains("pull_request"));
    }

    #[test]
    fn workflow_assessment_refuses_extra_trigger_selector_or_command_input() {
        let extra_trigger =
            H1_WORKFLOW_TEMPLATE.replace("  workflow_dispatch:", "  push:\n  workflow_dispatch:");
        assert!(!assess_h1_workflow_source(&extra_trigger).workflow_dispatch_only);

        let extra_selector = H1_WORKFLOW_TEMPLATE.replace(
            "      - runnermesh-admit",
            "      - runnermesh-admit\n      - unrelated-selector",
        );
        assert!(!assess_h1_workflow_source(&extra_selector).reserved_selector_exact);

        let command_input = H1_WORKFLOW_TEMPLATE.replace(
            "      candidate_sha:",
            "      command:\n        required: true\n        type: string\n      candidate_sha:",
        );
        assert!(!assess_h1_workflow_source(&command_input).arbitrary_command_input_absent);
    }

    #[test]
    fn happy_path_is_one_complete_label_specific_transaction_family() {
        let mut model = primary_running();
        for event in [
            H1TransactionEvent::PrimaryTrustedJobPassed,
            H1TransactionEvent::WithdrawalRequested,
            H1TransactionEvent::SelectorAbsenceObserved,
            H1TransactionEvent::NoNewAdmissionWitnessed {
                racing_assignment_observed: false,
            },
            H1TransactionEvent::AchievedDrainedObserved,
            H1TransactionEvent::ReadvertiseRequested,
            H1TransactionEvent::SelectorPresenceObserved,
            H1TransactionEvent::ReconnectWitnessPassed,
        ] {
            model.apply(event).unwrap();
        }
        assert_eq!(model.phase(), H1TransactionPhase::RestorePending);
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();
        assert_eq!(receipt.transaction_family, H1_TRANSACTION_FAMILY_ID);
        assert_eq!(receipt.qualification, QualificationDisposition::Pass);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
        assert!(receipt.external_mutation_started);
        assert!(receipt.automatic_restore_attempted);
    }

    #[test]
    fn racing_assignment_remains_drain_pending_until_worker_completion() {
        let mut model = primary_running();
        for event in [
            H1TransactionEvent::PrimaryTrustedJobPassed,
            H1TransactionEvent::WithdrawalRequested,
            H1TransactionEvent::SelectorAbsenceObserved,
            H1TransactionEvent::NoNewAdmissionWitnessed {
                racing_assignment_observed: true,
            },
        ] {
            model.apply(event).unwrap();
        }
        assert_eq!(model.phase(), H1TransactionPhase::DrainPending);
        assert!(model
            .apply(H1TransactionEvent::AchievedDrainedObserved)
            .is_err());
        model
            .apply(H1TransactionEvent::ActiveWorkerCompleted)
            .unwrap();
        model
            .apply(H1TransactionEvent::AchievedDrainedObserved)
            .unwrap();
        assert_eq!(model.phase(), H1TransactionPhase::Drained);
    }

    #[test]
    fn failure_before_owner_gate_has_no_mutation_or_restore_action() {
        let mut model = prepared();
        model
            .apply(H1TransactionEvent::PreOwnerGateBlocked)
            .unwrap();
        let receipt = model.receipt().unwrap();
        assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
        assert!(!receipt.external_mutation_started);
        assert!(!receipt.automatic_restore_attempted);
    }

    #[test]
    fn workflow_failure_has_independent_result_and_automatic_restore() {
        let mut model = primary_running();
        model
            .apply(H1TransactionEvent::PrimaryTrustedJobFailed)
            .unwrap();
        assert_eq!(model.phase(), H1TransactionPhase::RestorePending);
        finish_restore(&mut model, true);
        let receipt = model.receipt().unwrap();
        assert_eq!(receipt.qualification, QualificationDisposition::Fail);
        assert_eq!(receipt.restore, RestoreDisposition::Pass);
        assert!(receipt.automatic_restore_attempted);
    }

    #[test]
    fn routing_timeout_and_agent_loss_each_enter_restore() {
        for event in [
            H1TransactionEvent::RoutingUnavailable,
            H1TransactionEvent::ActiveJobTimedOut,
            H1TransactionEvent::AgentLost,
            H1TransactionEvent::ControllerLost,
        ] {
            let mut model = primary_running();
            model.apply(event).unwrap();
            assert_eq!(model.phase(), H1TransactionPhase::RestorePending);
            finish_restore(&mut model, true);
            let receipt = model.receipt().unwrap();
            assert_eq!(receipt.qualification, QualificationDisposition::Blocked);
            assert_eq!(receipt.restore, RestoreDisposition::Pass);
        }
    }

    #[test]
    fn automatic_restore_failure_injection_requires_owner_recovery() {
        let mut model = primary_running();
        model
            .apply(H1TransactionEvent::PrimaryTrustedJobFailed)
            .unwrap();
        finish_restore(&mut model, false);
        let receipt = model.receipt().unwrap();
        assert_eq!(receipt.qualification, QualificationDisposition::Fail);
        assert_eq!(receipt.restore, RestoreDisposition::Fail);
        assert!(receipt.emergency_owner_recovery_required);
    }

    #[test]
    fn interrupted_restore_preserves_qualification_and_requires_recovery() {
        let mut model = primary_running();
        model
            .apply(H1TransactionEvent::PrimaryTrustedJobPassed)
            .unwrap();
        model.request_restore(QualificationDisposition::Pass);
        model
            .apply(H1TransactionEvent::BeginAutomaticRestore)
            .unwrap();
        model.apply(H1TransactionEvent::RestoreInterrupted).unwrap();
        let receipt = model.receipt().unwrap();
        assert_eq!(receipt.qualification, QualificationDisposition::Pass);
        assert_eq!(receipt.restore, RestoreDisposition::Fail);
        assert!(receipt.emergency_owner_recovery_required);
    }

    #[test]
    fn ambiguous_ownership_refuses_and_unrelated_runner_is_never_controlled() {
        let mut before = prepared();
        before
            .apply(H1TransactionEvent::UnrelatedRunnerObserved)
            .unwrap();
        before
            .apply(H1TransactionEvent::OwnershipBecameAmbiguous)
            .unwrap();
        let receipt = before.receipt().unwrap();
        assert_eq!(receipt.unrelated_runner_control_actions, 0);
        assert!(!receipt.external_mutation_started);

        let mut after = primary_running();
        after
            .apply(H1TransactionEvent::OwnershipBecameAmbiguous)
            .unwrap();
        let receipt = after.receipt().unwrap();
        assert_eq!(receipt.restore, RestoreDisposition::Fail);
        assert!(receipt.emergency_owner_recovery_required);
    }

    #[test]
    fn durable_state_round_trips_without_private_identity_fields() {
        let model = primary_running();
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains(H1_TRANSACTION_FAMILY_ID));
        assert!(!json.contains("runner_name"));
        assert!(!json.contains("repository"));
        let round_trip: H1TransactionModel = serde_json::from_str(&json).unwrap();
        assert_eq!(round_trip, model);
    }
}
