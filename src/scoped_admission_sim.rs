//! Pure simulation of same-scope admission selector compatibility.
//!
//! Models ADR 0004/0005 rules without touching GitHub, workflows, or Services.
//! The namespaced selector form is a **proposed** migration shape only; it is
//! not an approved production contract and never mutates remote state.

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
}
