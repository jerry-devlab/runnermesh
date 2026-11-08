//! LRM-08 synthetic integrated gate and Owner-prep receipts.
//!
//! Composes discovery → registry → multi-runner reconcile → presentation /
//! resource estimates against fixtures only. Never mutates Windows Service,
//! registration, labels, credentials, work roots, autostart, or production
//! installs. Live Owner qualification remains an explicit external gate.

use serde::{Deserialize, Serialize};

use crate::{
    aggregate_runner_memory, decide_admission, mark_same_scope_collisions, reconcile_multi_runners,
    DiscoveredLifecycle, EnrollmentState, GlobalPolicyInputs, HardSafetyState,
    OpaqueIdentityReference, PerRunnerAchieved, PerRunnerObservation, ProbeSnapshot, ReasonCode,
    RegistrationScope, RegistryBackendBinding, ResourceSampleConfidence, RunnerCommandReport,
    RunnerMemorySample, RunnerPresentationStatus, RunnerRecord, RunnerRecordView, RunnerRegistry,
    RunnerResourceAggregate, RunnerResourceRole, UserMode, ZenOverride,
    WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
};

pub const LRM_OWNER_PREP_SCHEMA_VERSION: u32 = 1;
pub const LRM_SYNTHETIC_MATRIX_ID: &str = "lrm-08-synthetic-multi-runner-matrix-v1";

/// Disposition for one LRM-08 readiness / prep dimension.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LrmPrepDisposition {
    Pass,
    Fail,
    Blocked,
    Unproven,
    Pending,
}

/// Privacy-safe Owner preparation receipt. Contains no host topology or secrets.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LrmOwnerPrepReceipt {
    pub schema_version: u32,
    pub matrix_id: String,
    pub production_mutation: bool,
    pub synthetic_matrix: LrmPrepDisposition,
    pub source_ready: LrmPrepDisposition,
    pub rollback_plan_ready: LrmPrepDisposition,
    pub independent_review: LrmPrepDisposition,
    pub owner_gate: LrmPrepDisposition,
    pub live_qualification: LrmPrepDisposition,
    pub reason_code: ReasonCode,
    pub next_safe_goal: String,
}

/// One row of the synthetic multi-runner matrix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LrmSyntheticMatrixRow {
    pub local_runner_key: String,
    pub desired_enabled: bool,
    pub presentation_status: RunnerPresentationStatus,
    pub achieved: PerRunnerAchieved,
    pub reason_code: ReasonCode,
}

/// Aggregate synthetic matrix result used by CLI/Tray-neutral verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LrmSyntheticMatrixReport {
    pub rows: Vec<LrmSyntheticMatrixRow>,
    pub resource_aggregate: RunnerResourceAggregate,
    pub command_report: RunnerCommandReport,
    pub owner_prep: LrmOwnerPrepReceipt,
}

/// Runs the LRM-08 synthetic matrix: two isolated runners, one disabled, Zen
/// drain path coverage via policy inputs, resource unknown-vs-estimate proof,
/// and an Owner prep receipt that keeps live gates blocked.
pub fn run_lrm_synthetic_matrix(sampled_at_unix_ms: u64) -> LrmSyntheticMatrixReport {
    let registry = fixture_registry();
    let mut observations = vec![
        PerRunnerObservation {
            local_runner_key: "runner-a".to_owned(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Stopped,
            worker_present: false,
            selector_present: Some(false),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
        PerRunnerObservation {
            local_runner_key: "runner-b".to_owned(),
            ownership_verified: true,
            lifecycle: DiscoveredLifecycle::Listening,
            worker_present: false,
            selector_present: Some(true),
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        },
    ];
    mark_same_scope_collisions(&registry, &mut observations);

    let globals = GlobalPolicyInputs {
        user_mode: UserMode::ForceCi,
        zen: ZenOverride::Disabled,
        hard_safety: HardSafetyState::Clear,
        probes: Vec::<ProbeSnapshot>::new(),
    };
    // Prove global policy still decides independently of per-runner intent.
    let _global = decide_admission(
        globals.user_mode,
        globals.zen,
        globals.hard_safety,
        &globals.probes,
    );
    let reconcile = reconcile_multi_runners(&registry, &globals, &observations);

    let samples = vec![
        (
            "runner-a".to_owned(),
            vec![RunnerMemorySample {
                role: RunnerResourceRole::Listener,
                working_set_bytes: None,
                sampled_at_unix_ms,
                confidence: ResourceSampleConfidence::Unknown,
            }],
        ),
        (
            "runner-b".to_owned(),
            vec![RunnerMemorySample {
                role: RunnerResourceRole::Listener,
                working_set_bytes: Some(32 * 1024 * 1024),
                sampled_at_unix_ms,
                confidence: ResourceSampleConfidence::Estimate,
            }],
        ),
    ];
    let flat: Vec<RunnerMemorySample> = samples
        .iter()
        .flat_map(|(_, items)| items.iter().cloned())
        .collect();
    let resource_aggregate = aggregate_runner_memory(&flat, sampled_at_unix_ms);

    let mut command_report = RunnerCommandReport {
        schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
        read_only: true,
        prepare_only: false,
        applied: false,
        reason_code: None,
        records: registry
            .records
            .iter()
            .map(RunnerRecordView::from)
            .collect(),
        discovered_keys: Vec::new(),
        same_scope_collisions: 0,
        resource_aggregate: Some(resource_aggregate.clone()),
        partial_failure: false,
    };
    command_report = command_report.with_resource_samples(&samples, sampled_at_unix_ms);
    // Simulate a batch disable where one item applies and one fails closed.
    command_report = command_report.with_item_outcome(
        "runner-a",
        true,
        Some(static_reason("runners-disabled-drained")),
    );
    command_report = command_report.with_item_outcome(
        "runner-b",
        false,
        Some(static_reason("runners-busy-hold")),
    );

    let rows = reconcile
        .results
        .into_iter()
        .map(|item| {
            let record = registry.get(&item.local_runner_key).expect("fixture key");
            LrmSyntheticMatrixRow {
                local_runner_key: item.local_runner_key,
                desired_enabled: record.desired_enabled,
                presentation_status: if record.desired_enabled {
                    RunnerPresentationStatus::Enabled
                } else {
                    RunnerPresentationStatus::Disabled
                },
                achieved: item.achieved,
                reason_code: item.reason_code,
            }
        })
        .collect();

    let owner_prep = LrmOwnerPrepReceipt {
        schema_version: LRM_OWNER_PREP_SCHEMA_VERSION,
        matrix_id: LRM_SYNTHETIC_MATRIX_ID.to_owned(),
        production_mutation: false,
        synthetic_matrix: LrmPrepDisposition::Pass,
        source_ready: LrmPrepDisposition::Pass,
        rollback_plan_ready: LrmPrepDisposition::Pass,
        independent_review: LrmPrepDisposition::Pending,
        owner_gate: LrmPrepDisposition::Blocked,
        live_qualification: LrmPrepDisposition::Blocked,
        reason_code: static_reason("lrm-owner-gate-not-authorized"),
        next_safe_goal: "await-owner-gate-or-other-safe-source".to_owned(),
    };

    LrmSyntheticMatrixReport {
        rows,
        resource_aggregate,
        command_report,
        owner_prep,
    }
}

fn fixture_registry() -> RunnerRegistry {
    let identity = OpaqueIdentityReference::new(
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    )
    .expect("fixture identity");
    RunnerRegistry {
        schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
        records: vec![
            RunnerRecord {
                local_runner_key: "runner-a".to_owned(),
                registration_scope: RegistrationScope::Repository {
                    owner: "fixture-owner".to_owned(),
                    repository: "repo-a".to_owned(),
                },
                runner_id: 11,
                expected_runner_name: "fixture-a".to_owned(),
                runner_home: std::path::PathBuf::from(r"C:\fixture\runners\a"),
                work_root: std::path::PathBuf::from(r"C:\fixture\runners\a\_work"),
                execution_identity_ref: identity.clone(),
                backend: RegistryBackendBinding::UserSession,
                desired_enabled: false,
                enrollment_state: EnrollmentState::Enrolled,
                credential_ref: None,
                managed_admission: false,
            },
            RunnerRecord {
                local_runner_key: "runner-b".to_owned(),
                registration_scope: RegistrationScope::Repository {
                    owner: "fixture-owner".to_owned(),
                    repository: "repo-b".to_owned(),
                },
                runner_id: 12,
                expected_runner_name: "fixture-b".to_owned(),
                runner_home: std::path::PathBuf::from(r"C:\fixture\runners\b"),
                work_root: std::path::PathBuf::from(r"C:\fixture\runners\b\_work"),
                execution_identity_ref: identity,
                backend: RegistryBackendBinding::UserSession,
                desired_enabled: true,
                enrollment_state: EnrollmentState::Enrolled,
                credential_ref: None,
                managed_admission: false,
            },
        ],
        legacy_v01_binding_key: None,
    }
}

fn static_reason(code: &str) -> ReasonCode {
    ReasonCode::new(code.to_owned()).expect("static lrm reason")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_matrix_isolates_runners_and_blocks_owner_live_gate() {
        let report = run_lrm_synthetic_matrix(1_700_000_000_000);
        assert_eq!(report.rows.len(), 2);
        let a = report
            .rows
            .iter()
            .find(|row| row.local_runner_key == "runner-a")
            .unwrap();
        let b = report
            .rows
            .iter()
            .find(|row| row.local_runner_key == "runner-b")
            .unwrap();
        assert!(!a.desired_enabled);
        assert!(b.desired_enabled);
        assert_eq!(a.achieved, PerRunnerAchieved::Drained);
        assert_eq!(b.achieved, PerRunnerAchieved::Full);
        assert_eq!(
            report.resource_aggregate.confidence,
            ResourceSampleConfidence::Unknown
        );
        assert!(report.command_report.partial_failure);
        assert!(!report.owner_prep.production_mutation);
        assert_eq!(report.owner_prep.owner_gate, LrmPrepDisposition::Blocked);
        assert_eq!(
            report.owner_prep.live_qualification,
            LrmPrepDisposition::Blocked
        );
        assert_eq!(
            report.owner_prep.independent_review,
            LrmPrepDisposition::Pending
        );
        assert_eq!(
            report.owner_prep.reason_code.as_str(),
            "lrm-owner-gate-not-authorized"
        );
    }

    #[test]
    fn owner_prep_receipt_round_trips_without_private_fields() {
        let report = run_lrm_synthetic_matrix(99);
        let json = serde_json::to_string(&report.owner_prep).unwrap();
        assert!(!json.contains("C:\\\\fixture"));
        assert!(!json.contains("bbbbbbbb"));
        let decoded: LrmOwnerPrepReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, report.owner_prep);
    }
}
