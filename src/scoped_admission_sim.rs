//! Pure simulation of same-scope admission selector compatibility.
//!
//! Models ADR 0004/0005 rules without touching GitHub, workflows, or Services.
//! The namespaced selector form is a **proposed** migration shape only; it is
//! not an approved production contract and never mutates remote state.

use serde::{Deserialize, Serialize};

use crate::{RegistrationScope, RESERVED_ADMISSION_LABEL};

/// Proposed (unapproved) per-runner selector namespace prefix.
pub const PROPOSED_SCOPED_SELECTOR_PREFIX: &str = "runnermesh-admit-";

/// One simulated enrolled runner's admission binding intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedAdmissionBinding {
    pub local_runner_key: String,
    pub scope: RegistrationScope,
    pub managed_admission: bool,
    /// Currently claimed selector, if any. v0.1 owners use `runnermesh-admit`.
    pub claimed_selector: Option<String>,
}

/// Outcome of a dry-run compatibility check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorCompatibility {
    /// Distinct scopes or unmanaged runners — no collision under v0.1 rules.
    Compatible,
    /// Same scope + managed admission claiming the reserved label more than once.
    BlockedSameScopeCollision { colliding_keys: Vec<String> },
    /// Incomplete claim — managed admission without an owned selector.
    RefusedMissingSelector { local_runner_key: String },
}

/// Dry-run migration plan. Never applied by this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedSelectorMigrationPlan {
    pub local_runner_key: String,
    pub from_selector: Option<String>,
    pub proposed_selector: String,
    pub requires_owner_gate: bool,
    pub mutates_workflows: bool,
    pub approved: bool,
}

/// Evaluates whether a set of bindings can coexist under frozen v0.1 ownership.
pub fn evaluate_v01_selector_compatibility(
    bindings: &[SimulatedAdmissionBinding],
) -> Vec<SelectorCompatibility> {
    let mut outcomes = Vec::new();
    for binding in bindings {
        if !binding.managed_admission {
            outcomes.push(SelectorCompatibility::Compatible);
            continue;
        }
        if binding.claimed_selector.as_deref() != Some(RESERVED_ADMISSION_LABEL) {
            outcomes.push(SelectorCompatibility::RefusedMissingSelector {
                local_runner_key: binding.local_runner_key.clone(),
            });
            continue;
        }
        let colliding: Vec<String> = bindings
            .iter()
            .filter(|other| {
                other.local_runner_key != binding.local_runner_key
                    && other.managed_admission
                    && other.scope == binding.scope
                    && other.claimed_selector.as_deref() == Some(RESERVED_ADMISSION_LABEL)
            })
            .map(|other| other.local_runner_key.clone())
            .collect();
        if colliding.is_empty() {
            outcomes.push(SelectorCompatibility::Compatible);
        } else {
            let mut keys = colliding;
            keys.push(binding.local_runner_key.clone());
            keys.sort();
            keys.dedup();
            outcomes.push(SelectorCompatibility::BlockedSameScopeCollision {
                colliding_keys: keys,
            });
        }
    }
    outcomes
}

/// Builds an unapproved dry-run migration plan for a namespaced selector.
///
/// The proposed name is deterministic from the local key for simulation only.
/// It does not authorize label writes, workflow edits, or Owner cutover.
pub fn propose_scoped_selector_migration(
    binding: &SimulatedAdmissionBinding,
) -> ScopedSelectorMigrationPlan {
    let sanitized: String = binding
        .local_runner_key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    ScopedSelectorMigrationPlan {
        local_runner_key: binding.local_runner_key.clone(),
        from_selector: binding.claimed_selector.clone(),
        proposed_selector: format!("{PROPOSED_SCOPED_SELECTOR_PREFIX}{sanitized}"),
        requires_owner_gate: true,
        mutates_workflows: true,
        approved: false,
    }
}

/// Routing classification for a GitHub Actions workflow job's `runs-on` clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowRoutingClassification {
    /// Qualified: explicitly requires the owned admission selector.
    QualifiedOwnedSelector { selector: String },
    /// Unqualified legacy: requires generic labels (e.g. self-hosted) without the owned selector.
    /// A runner with this routing CANNOT prove safe no-new-job drain upon selector withdrawal!
    UnqualifiedLegacyRouting {
        generic_labels: Vec<String>,
        reason: String,
    },
    /// Ambiguous or missing `runs-on` clause (e.g. variable, matrix, or empty).
    AmbiguousOrMissingRunsOn { detail: String },
}

/// Audit result for one workflow job's routing contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRoutingAudit {
    pub job_id: String,
    pub runs_on_labels: Vec<String>,
    pub classification: WorkflowRoutingClassification,
    /// Whether withdrawing the owned selector provably stops new jobs from targeting this runner.
    pub safe_no_new_job_drain_provable: bool,
    pub migration_recommendation: Option<String>,
}

/// Audits a single workflow job's `runs-on` labels against an owned admission selector.
pub fn audit_workflow_job_routing(
    job_id: &str,
    runs_on_labels: &[String],
    owned_selector: &str,
) -> WorkflowRoutingAudit {
    if runs_on_labels.is_empty() {
        return WorkflowRoutingAudit {
            job_id: job_id.to_owned(),
            runs_on_labels: Vec::new(),
            classification: WorkflowRoutingClassification::AmbiguousOrMissingRunsOn {
                detail: "runs-on label list is empty".to_owned(),
            },
            safe_no_new_job_drain_provable: false,
            migration_recommendation: Some(format!(
                "explicitly declare runs-on requiring owned selector '{owned_selector}'"
            )),
        };
    }

    let has_owned = runs_on_labels
        .iter()
        .any(|l| l.eq_ignore_ascii_case(owned_selector));

    if has_owned {
        WorkflowRoutingAudit {
            job_id: job_id.to_owned(),
            runs_on_labels: runs_on_labels.to_vec(),
            classification: WorkflowRoutingClassification::QualifiedOwnedSelector {
                selector: owned_selector.to_owned(),
            },
            safe_no_new_job_drain_provable: true,
            migration_recommendation: None,
        }
    } else {
        WorkflowRoutingAudit {
            job_id: job_id.to_owned(),
            runs_on_labels: runs_on_labels.to_vec(),
            classification: WorkflowRoutingClassification::UnqualifiedLegacyRouting {
                generic_labels: runs_on_labels.to_vec(),
                reason: "generic labels without owned admission selector cannot guarantee no-new-job drain".to_owned(),
            },
            safe_no_new_job_drain_provable: false,
            migration_recommendation: Some(format!(
                "migrate workflow job to include exact owned selector '{owned_selector}' or schedule Owner maintenance window"
            )),
        }
    }
}

/// Strongly typed scoped admission mutation action.
///
/// Refuses replace-all or remove-all operations; only operates via exact add-one or remove-one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScopedAdmissionAction {
    AddExactLabel { label: String },
    RemoveExactLabel { label: String },
}

/// A planned scoped admission transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedAdmissionTransaction {
    pub runner_id: u64,
    pub action: ScopedAdmissionAction,
    pub expected_label: String,
    pub transaction_id: String,
    pub requires_owner_gate: bool,
}

/// Receipt for a scoped admission transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedAdmissionReceipt {
    pub runner_id: u64,
    pub transaction_id: String,
    pub applied: bool,
    pub readback_pass: bool,
    pub observed_labels: Vec<String>,
    pub refusal_reason: Option<String>,
}

/// Synthetic / simulation transport trait for scoped admission readback.
pub trait ScopedAdmissionTransport {
    fn query_runner_labels(&mut self, runner_id: u64) -> Result<Vec<String>, String>;
    fn add_runner_label(&mut self, runner_id: u64, label: &str) -> Result<(), String>;
    fn remove_runner_label(&mut self, runner_id: u64, label: &str) -> Result<(), String>;
}

/// Plans an exact add-one scoped label transaction.
pub fn plan_scoped_label_add(
    runner_id: u64,
    label: &str,
    transaction_id: impl Into<String>,
    requires_owner_gate: bool,
) -> Result<ScopedAdmissionTransaction, String> {
    let clean = label.trim();
    if clean.is_empty() || clean.contains('*') || clean.contains('?') {
        return Err("invalid label: empty or contains wildcards".to_owned());
    }
    Ok(ScopedAdmissionTransaction {
        runner_id,
        action: ScopedAdmissionAction::AddExactLabel {
            label: clean.to_owned(),
        },
        expected_label: clean.to_owned(),
        transaction_id: transaction_id.into(),
        requires_owner_gate,
    })
}

/// Plans an exact remove-one scoped label transaction.
pub fn plan_scoped_label_remove(
    runner_id: u64,
    label: &str,
    transaction_id: impl Into<String>,
    requires_owner_gate: bool,
) -> Result<ScopedAdmissionTransaction, String> {
    let clean = label.trim();
    if clean.is_empty() || clean.contains('*') || clean.contains('?') {
        return Err("invalid label: empty or contains wildcards".to_owned());
    }
    Ok(ScopedAdmissionTransaction {
        runner_id,
        action: ScopedAdmissionAction::RemoveExactLabel {
            label: clean.to_owned(),
        },
        expected_label: clean.to_owned(),
        transaction_id: transaction_id.into(),
        requires_owner_gate,
    })
}

/// Executes a scoped admission transaction against a synthetic transport with mandatory readback.
///
/// CRITICAL FAIL-CLOSED CONTRACT:
/// If the transport mutation succeeds but subsequent readback fails or reveals that the
/// label presence does NOT match expected outcome, `applied: true` may be recorded but
/// `readback_pass: false` is strictly enforced.
pub fn execute_synthetic_scoped_admission<T: ScopedAdmissionTransport>(
    tx: &ScopedAdmissionTransaction,
    transport: &mut T,
    authorized: bool,
) -> ScopedAdmissionReceipt {
    if tx.requires_owner_gate && !authorized {
        return ScopedAdmissionReceipt {
            runner_id: tx.runner_id,
            transaction_id: tx.transaction_id.clone(),
            applied: false,
            readback_pass: false,
            observed_labels: Vec::new(),
            refusal_reason: Some("unauthorized-live-mutation-blocked".to_owned()),
        };
    }

    let mutate_result = match &tx.action {
        ScopedAdmissionAction::AddExactLabel { label } => {
            transport.add_runner_label(tx.runner_id, label)
        }
        ScopedAdmissionAction::RemoveExactLabel { label } => {
            transport.remove_runner_label(tx.runner_id, label)
        }
    };

    if let Err(error) = mutate_result {
        return ScopedAdmissionReceipt {
            runner_id: tx.runner_id,
            transaction_id: tx.transaction_id.clone(),
            applied: false,
            readback_pass: false,
            observed_labels: Vec::new(),
            refusal_reason: Some(error),
        };
    }

    // Fresh readback from transport
    let observed = match transport.query_runner_labels(tx.runner_id) {
        Ok(labels) => labels,
        Err(error) => {
            return ScopedAdmissionReceipt {
                runner_id: tx.runner_id,
                transaction_id: tx.transaction_id.clone(),
                applied: true,
                readback_pass: false,
                observed_labels: Vec::new(),
                refusal_reason: Some(format!("readback-failed: {error}")),
            };
        }
    };

    let label_present = observed
        .iter()
        .any(|l| l.eq_ignore_ascii_case(&tx.expected_label));

    let expected_presence = matches!(tx.action, ScopedAdmissionAction::AddExactLabel { .. });

    if label_present != expected_presence {
        ScopedAdmissionReceipt {
            runner_id: tx.runner_id,
            transaction_id: tx.transaction_id.clone(),
            applied: true,
            readback_pass: false,
            observed_labels: observed,
            refusal_reason: Some("readback-presence-mismatch".to_owned()),
        }
    } else {
        ScopedAdmissionReceipt {
            runner_id: tx.runner_id,
            transaction_id: tx.transaction_id.clone(),
            applied: true,
            readback_pass: true,
            observed_labels: observed,
            refusal_reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_scope() -> RegistrationScope {
        RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: "fixture-repo".to_owned(),
        }
    }

    #[test]
    fn distinct_scopes_remain_compatible_under_v01_reserved_label() {
        let bindings = vec![
            SimulatedAdmissionBinding {
                local_runner_key: "a".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
            SimulatedAdmissionBinding {
                local_runner_key: "b".into(),
                scope: RegistrationScope::Organization {
                    organization: "fixture-org".into(),
                },
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
        ];
        let outcomes = evaluate_v01_selector_compatibility(&bindings);
        assert!(outcomes
            .iter()
            .all(|o| matches!(o, SelectorCompatibility::Compatible)));
    }

    #[test]
    fn same_scope_dual_managed_claim_is_blocked_without_mutation() {
        let bindings = vec![
            SimulatedAdmissionBinding {
                local_runner_key: "alpha".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
            SimulatedAdmissionBinding {
                local_runner_key: "beta".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
        ];
        let outcomes = evaluate_v01_selector_compatibility(&bindings);
        assert!(outcomes
            .iter()
            .all(|o| matches!(o, SelectorCompatibility::BlockedSameScopeCollision { .. })));
    }

    #[test]
    fn proposed_scoped_migration_is_unapproved_and_owner_gated() {
        let binding = SimulatedAdmissionBinding {
            local_runner_key: "runner/One".into(),
            scope: repo_scope(),
            managed_admission: true,
            claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
        };
        let plan = propose_scoped_selector_migration(&binding);
        assert_eq!(plan.proposed_selector, "runnermesh-admit-runner-one");
        assert!(plan.requires_owner_gate);
        assert!(plan.mutates_workflows);
        assert!(!plan.approved);
    }

    #[test]
    fn unmanaged_same_scope_runners_remain_compatible_without_reserved_claim() {
        let bindings = vec![
            SimulatedAdmissionBinding {
                local_runner_key: "a".into(),
                scope: repo_scope(),
                managed_admission: false,
                claimed_selector: None,
            },
            SimulatedAdmissionBinding {
                local_runner_key: "b".into(),
                scope: repo_scope(),
                managed_admission: false,
                claimed_selector: None,
            },
        ];
        let outcomes = evaluate_v01_selector_compatibility(&bindings);
        assert!(outcomes
            .iter()
            .all(|o| matches!(o, SelectorCompatibility::Compatible)));
    }

    #[test]
    fn proposed_namespaced_selectors_do_not_silently_unlock_v01_collision() {
        // Even if each runner *proposes* a distinct scoped selector, the frozen
        // v0.1 evaluator still blocks dual managed claims on the reserved label
        // until a separate ADR/Owner gate accepts the migration.
        let bindings = vec![
            SimulatedAdmissionBinding {
                local_runner_key: "alpha".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
            SimulatedAdmissionBinding {
                local_runner_key: "beta".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
        ];
        let plans: Vec<_> = bindings
            .iter()
            .map(propose_scoped_selector_migration)
            .collect();
        assert_ne!(plans[0].proposed_selector, plans[1].proposed_selector);
        assert!(plans.iter().all(|plan| !plan.approved));
        let outcomes = evaluate_v01_selector_compatibility(&bindings);
        assert!(outcomes
            .iter()
            .all(|o| matches!(o, SelectorCompatibility::BlockedSameScopeCollision { .. })));
    }

    #[test]
    fn mixed_managed_and_unmanaged_or_missing_selector_classifications() {
        let bindings = vec![
            SimulatedAdmissionBinding {
                local_runner_key: "valid-managed".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
            },
            SimulatedAdmissionBinding {
                local_runner_key: "missing-selector".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: None,
            },
            SimulatedAdmissionBinding {
                local_runner_key: "unrelated-label".into(),
                scope: repo_scope(),
                managed_admission: true,
                claimed_selector: Some("unrelated-label".to_owned()),
            },
            SimulatedAdmissionBinding {
                local_runner_key: "unmanaged".into(),
                scope: repo_scope(),
                managed_admission: false,
                claimed_selector: None,
            },
        ];
        let outcomes = evaluate_v01_selector_compatibility(&bindings);
        assert_eq!(outcomes[0], SelectorCompatibility::Compatible);
        assert_eq!(
            outcomes[1],
            SelectorCompatibility::RefusedMissingSelector {
                local_runner_key: "missing-selector".into()
            }
        );
        assert_eq!(
            outcomes[2],
            SelectorCompatibility::RefusedMissingSelector {
                local_runner_key: "unrelated-label".into()
            }
        );
        assert_eq!(outcomes[3], SelectorCompatibility::Compatible);
    }

    #[test]
    fn migration_sanitizer_and_adr_preservation() {
        let binding = SimulatedAdmissionBinding {
            local_runner_key: "my org/runner 01#special!".into(),
            scope: repo_scope(),
            managed_admission: true,
            claimed_selector: Some(RESERVED_ADMISSION_LABEL.to_owned()),
        };
        let plan = propose_scoped_selector_migration(&binding);
        assert_eq!(
            plan.proposed_selector,
            "runnermesh-admit-my-org-runner-01-special-"
        );
        assert!(plan.requires_owner_gate);
        assert!(plan.mutates_workflows);
        assert!(!plan.approved);
    }

    #[derive(Default)]
    struct MockAdmissionTransport {
        labels: std::collections::BTreeMap<u64, Vec<String>>,
        fail_next_mutate: Option<String>,
        fail_next_query: Option<String>,
    }

    impl ScopedAdmissionTransport for MockAdmissionTransport {
        fn query_runner_labels(&mut self, runner_id: u64) -> Result<Vec<String>, String> {
            if let Some(err) = self.fail_next_query.take() {
                return Err(err);
            }
            Ok(self.labels.get(&runner_id).cloned().unwrap_or_default())
        }

        fn add_runner_label(&mut self, runner_id: u64, label: &str) -> Result<(), String> {
            if let Some(err) = self.fail_next_mutate.take() {
                return Err(err);
            }
            let list = self.labels.entry(runner_id).or_default();
            if !list.iter().any(|l| l.eq_ignore_ascii_case(label)) {
                list.push(label.to_owned());
            }
            Ok(())
        }

        fn remove_runner_label(&mut self, runner_id: u64, label: &str) -> Result<(), String> {
            if let Some(err) = self.fail_next_mutate.take() {
                return Err(err);
            }
            let list = self.labels.entry(runner_id).or_default();
            list.retain(|l| !l.eq_ignore_ascii_case(label));
            Ok(())
        }
    }

    #[test]
    fn audit_workflow_routing_classifies_qualified_unqualified_and_missing() {
        // 1. Qualified job containing exact owned selector
        let qualified_labels = vec![
            "self-hosted".to_owned(),
            "windows".to_owned(),
            "x64".to_owned(),
            RESERVED_ADMISSION_LABEL.to_owned(),
        ];
        let audit_q =
            audit_workflow_job_routing("build", &qualified_labels, RESERVED_ADMISSION_LABEL);
        assert!(audit_q.safe_no_new_job_drain_provable);
        assert_eq!(
            audit_q.classification,
            WorkflowRoutingClassification::QualifiedOwnedSelector {
                selector: RESERVED_ADMISSION_LABEL.to_owned()
            }
        );
        assert_eq!(audit_q.migration_recommendation, None);

        // 2. Unqualified legacy job using generic labels without owned selector
        let legacy_labels = vec![
            "self-hosted".to_owned(),
            "windows".to_owned(),
            "x64".to_owned(),
        ];
        let audit_legacy =
            audit_workflow_job_routing("test", &legacy_labels, RESERVED_ADMISSION_LABEL);
        assert!(!audit_legacy.safe_no_new_job_drain_provable);
        assert!(matches!(
            audit_legacy.classification,
            WorkflowRoutingClassification::UnqualifiedLegacyRouting { .. }
        ));
        assert!(audit_legacy
            .migration_recommendation
            .as_deref()
            .unwrap()
            .contains("migrate workflow job"));

        // 3. Ambiguous empty labels
        let audit_empty = audit_workflow_job_routing("deploy", &[], RESERVED_ADMISSION_LABEL);
        assert!(!audit_empty.safe_no_new_job_drain_provable);
        assert!(matches!(
            audit_empty.classification,
            WorkflowRoutingClassification::AmbiguousOrMissingRunsOn { .. }
        ));
    }

    #[test]
    fn synthetic_scoped_admission_execution_enforces_readback_and_owner_gate() {
        let mut transport = MockAdmissionTransport::default();
        transport
            .labels
            .insert(42, vec!["self-hosted".to_owned(), "windows".to_owned()]);

        let plan_add = plan_scoped_label_add(42, "runnermesh-admit-42", "tx-1", true).unwrap();

        // 1. Unissued Owner gate blocks live mutation
        let receipt_unauthorized =
            execute_synthetic_scoped_admission(&plan_add, &mut transport, false);
        assert!(!receipt_unauthorized.applied);
        assert!(!receipt_unauthorized.readback_pass);
        assert_eq!(
            receipt_unauthorized.refusal_reason.as_deref(),
            Some("unauthorized-live-mutation-blocked")
        );

        // 2. Authorized mutation succeeds and verifies readback
        let receipt_add = execute_synthetic_scoped_admission(&plan_add, &mut transport, true);
        assert!(receipt_add.applied);
        assert!(receipt_add.readback_pass);
        assert!(receipt_add
            .observed_labels
            .contains(&"runnermesh-admit-42".to_owned()));

        // 3. Remove label when readback fails fails closed
        let plan_remove =
            plan_scoped_label_remove(42, "runnermesh-admit-42", "tx-2", true).unwrap();
        transport.fail_next_query = Some("simulated-network-timeout".to_owned());
        let receipt_rb_err = execute_synthetic_scoped_admission(&plan_remove, &mut transport, true);
        assert!(receipt_rb_err.applied);
        assert!(!receipt_rb_err.readback_pass);
        assert!(receipt_rb_err
            .refusal_reason
            .as_deref()
            .unwrap()
            .contains("readback-failed"));

        // 4. Remove label with clean readback completes
        let plan_remove_clean =
            plan_scoped_label_remove(42, "runnermesh-admit-42", "tx-3", true).unwrap();
        let receipt_remove =
            execute_synthetic_scoped_admission(&plan_remove_clean, &mut transport, true);
        assert!(receipt_remove.applied);
        assert!(receipt_remove.readback_pass);
        assert!(!receipt_remove
            .observed_labels
            .contains(&"runnermesh-admit-42".to_owned()));
    }

    #[test]
    fn plan_scoped_label_rejects_wildcards_and_empty_labels() {
        assert!(plan_scoped_label_add(10, "*", "tx-1", true).is_err());
        assert!(plan_scoped_label_add(10, "label*wild", "tx-1", true).is_err());
        assert!(plan_scoped_label_add(10, "   ", "tx-1", true).is_err());
        assert!(plan_scoped_label_remove(10, "?", "tx-1", true).is_err());
    }
}
