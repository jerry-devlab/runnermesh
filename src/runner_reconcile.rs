//! Per-runner multi-instance admission reconciliation (LRM-06).
//!
//! Pure synthetic Observe -> Decide -> Reconcile over enrolled registry records.
//! Precedence (ADR 0005):
//! hard safety / ownership > Zen > runner desired_enabled=false >
//! explicit UserMode (incl. Force-CI) > Auto Lite.
//!
//! This module never mutates GitHub labels, SCM, or processes. Selector
//! mutations are represented as planned intents only.

use crate::{
    decide_admission, AdmissionLifecycleState, DiscoveredLifecycle, HardSafetyState, NodeState,
    ProbeSnapshot, ReasonCode, RegistrationScope, RunnerRecord, RunnerRegistry, UserMode,
    ZenOverride,
};

/// Observed facts for one enrolled runner. Reconstructed, never persisted as truth.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerRunnerObservation {
    pub local_runner_key: String,
    pub ownership_verified: bool,
    pub lifecycle: DiscoveredLifecycle,
    pub worker_present: bool,
    pub selector_present: Option<bool>,
    pub selector_observation_unknown: bool,
    /// True when workflows targeting this runner use only generic labels.
    pub legacy_generic_label_routing: bool,
    /// Another enrolled managed-admission runner shares this registration scope.
    pub same_scope_managed_collision: bool,
}

/// Desired capacity for one runner after policy + per-runner intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerRunnerDesired {
    Full,
    Drained,
    Offline,
}

/// Achieved claim only after independent matching evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerRunnerAchieved {
    Full,
    Drained,
    Offline,
    Transitional,
    Unknown,
    Refused,
}

/// Planned non-executing control intents for one reconcile tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerRunnerPlan {
    AdvertiseSelector,
    WithdrawSelector,
    HoldBusy,
    Refuse,
    Noop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerRunnerReconcileResult {
    pub local_runner_key: String,
    pub desired: PerRunnerDesired,
    pub observed_lifecycle: DiscoveredLifecycle,
    pub admission_lifecycle: AdmissionLifecycleState,
    pub achieved: PerRunnerAchieved,
    pub plan: PerRunnerPlan,
    pub reason_code: ReasonCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiRunnerReconcileReport {
    pub results: Vec<PerRunnerReconcileResult>,
}

/// Global policy inputs shared across runners for one tick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalPolicyInputs {
    pub user_mode: UserMode,
    pub zen: ZenOverride,
    pub hard_safety: HardSafetyState,
    pub probes: Vec<ProbeSnapshot>,
}

/// Reconciles each enrolled runner independently against the same global policy.
pub fn reconcile_multi_runners(
    registry: &RunnerRegistry,
    globals: &GlobalPolicyInputs,
    observations: &[PerRunnerObservation],
) -> MultiRunnerReconcileReport {
    let global_decision = decide_admission(
        globals.user_mode,
        globals.zen,
        globals.hard_safety,
        &globals.probes,
    );
    let mut results = Vec::new();
    for record in registry.active_records() {
        let observed = observations
            .iter()
            .find(|item| item.local_runner_key == record.local_runner_key);
        results.push(reconcile_one(record, &global_decision, observed));
    }
    // Isolation proof aid: sort by key for deterministic fixtures.
    results.sort_by(|left, right| left.local_runner_key.cmp(&right.local_runner_key));
    MultiRunnerReconcileReport { results }
}

fn reconcile_one(
    record: &RunnerRecord,
    global: &crate::AdmissionDecision,
    observed: Option<&PerRunnerObservation>,
) -> PerRunnerReconcileResult {
    let Some(observed) = observed else {
        return result(
            record,
            PerRunnerDesired::Drained,
            DiscoveredLifecycle::Unknown,
            AdmissionLifecycleState::Unknown,
            PerRunnerAchieved::Unknown,
            PerRunnerPlan::Refuse,
            "runners-observation-missing",
        );
    };

    if !observed.ownership_verified {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::Refused,
            PerRunnerAchieved::Refused,
            PerRunnerPlan::Refuse,
            "runners-ownership-denied",
        );
    }

    if observed.same_scope_managed_collision && record.managed_admission {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::Refused,
            PerRunnerAchieved::Refused,
            PerRunnerPlan::Refuse,
            "runners-same-scope-collision",
        );
    }

    if observed.legacy_generic_label_routing {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::Refused,
            PerRunnerAchieved::Refused,
            PerRunnerPlan::Refuse,
            "runners-unqualified-legacy-routing",
        );
    }

    if observed.selector_observation_unknown || observed.lifecycle == DiscoveredLifecycle::Unknown {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::Unknown,
            PerRunnerAchieved::Unknown,
            PerRunnerPlan::Refuse,
            "runners-unknown-evidence",
        );
    }

    // Manual per-runner disable outranks Force-CI / Auto / Idle.
    if !record.desired_enabled {
        return reconcile_disabled(record, observed);
    }

    if !global.allow_new_work {
        return reconcile_policy_drain(record, observed, global);
    }

    reconcile_enabled_full(record, observed)
}

fn reconcile_disabled(
    record: &RunnerRecord,
    observed: &PerRunnerObservation,
) -> PerRunnerReconcileResult {
    let busy = observed.worker_present || observed.lifecycle == DiscoveredLifecycle::Busy;
    let selector_absent = observed.selector_present == Some(false);
    let selector_present = observed.selector_present == Some(true);

    // Busy with selector still present: withdraw admission now; never defer
    // withdrawal solely because a Worker is held.
    if busy && selector_present {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::WithdrawRequested,
            PerRunnerAchieved::Transitional,
            PerRunnerPlan::WithdrawSelector,
            "runners-disable-busy-withdraw",
        );
    }
    // Busy with selector already absent: preserve Worker, do not claim drained.
    if busy {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::DrainPending,
            PerRunnerAchieved::Transitional,
            PerRunnerPlan::HoldBusy,
            "runners-disable-busy-hold",
        );
    }
    // ADR 0004: achieved DRAINED = selector absent + no Worker + consistent
    // evidence. Listener may still be Listening; stop is not required.
    if selector_absent {
        return result(
            record,
            PerRunnerDesired::Drained,
            observed.lifecycle,
            AdmissionLifecycleState::Drained,
            PerRunnerAchieved::Drained,
            PerRunnerPlan::Noop,
            "runners-disabled-drained",
        );
    }
    result(
        record,
        PerRunnerDesired::Drained,
        observed.lifecycle,
        AdmissionLifecycleState::WithdrawRequested,
        PerRunnerAchieved::Transitional,
        PerRunnerPlan::WithdrawSelector,
        "runners-disable-withdraw",
    )
}

fn reconcile_policy_drain(
    record: &RunnerRecord,
    observed: &PerRunnerObservation,
    global: &crate::AdmissionDecision,
) -> PerRunnerReconcileResult {
    let desired = match global.desired_node_state {
        NodeState::Offline => PerRunnerDesired::Offline,
        _ => PerRunnerDesired::Drained,
    };
    let busy = observed.worker_present || observed.lifecycle == DiscoveredLifecycle::Busy;
    let selector_absent = observed.selector_present == Some(false);
    let selector_present = observed.selector_present == Some(true);

    if busy && selector_present {
        return result(
            record,
            desired,
            observed.lifecycle,
            AdmissionLifecycleState::WithdrawRequested,
            PerRunnerAchieved::Transitional,
            PerRunnerPlan::WithdrawSelector,
            global.reason_code.as_str(),
        );
    }
    if busy {
        return result(
            record,
            desired,
            observed.lifecycle,
            AdmissionLifecycleState::DrainPending,
            PerRunnerAchieved::Transitional,
            PerRunnerPlan::HoldBusy,
            global.reason_code.as_str(),
        );
    }
    if selector_absent {
        // Work/Gaming drain: DRAINED does not require Listener stopped (ADR 0004).
        // Zen/Offline: Offline achieved still requires Listener Stopped evidence.
        if desired == PerRunnerDesired::Offline {
            if observed.lifecycle == DiscoveredLifecycle::Stopped {
                return result(
                    record,
                    desired,
                    observed.lifecycle,
                    AdmissionLifecycleState::Drained,
                    PerRunnerAchieved::Offline,
                    PerRunnerPlan::Noop,
                    global.reason_code.as_str(),
                );
            }
            return result(
                record,
                desired,
                observed.lifecycle,
                AdmissionLifecycleState::DrainPending,
                PerRunnerAchieved::Transitional,
                PerRunnerPlan::Noop,
                "runners-listener-stop-unproven",
            );
        }
        return result(
            record,
            desired,
            observed.lifecycle,
            AdmissionLifecycleState::Drained,
            PerRunnerAchieved::Drained,
            PerRunnerPlan::Noop,
            global.reason_code.as_str(),
        );
    }
    result(
        record,
        desired,
        observed.lifecycle,
        AdmissionLifecycleState::WithdrawRequested,
        PerRunnerAchieved::Transitional,
        PerRunnerPlan::WithdrawSelector,
        global.reason_code.as_str(),
    )
}

fn reconcile_enabled_full(
    record: &RunnerRecord,
    observed: &PerRunnerObservation,
) -> PerRunnerReconcileResult {
    let busy = observed.lifecycle == DiscoveredLifecycle::Busy || observed.worker_present;
    if busy {
        // Full requires independent admission evidence (selector present).
        if observed.selector_present != Some(true) {
            return result(
                record,
                PerRunnerDesired::Full,
                observed.lifecycle,
                AdmissionLifecycleState::Busy,
                PerRunnerAchieved::Transitional,
                PerRunnerPlan::Noop,
                "runners-busy-selector-unproven",
            );
        }
        return result(
            record,
            PerRunnerDesired::Full,
            observed.lifecycle,
            AdmissionLifecycleState::Busy,
            PerRunnerAchieved::Full,
            PerRunnerPlan::Noop,
            "runners-busy-full",
        );
    }
    if observed.selector_present == Some(true)
        && observed.lifecycle == DiscoveredLifecycle::Listening
    {
        return result(
            record,
            PerRunnerDesired::Full,
            observed.lifecycle,
            AdmissionLifecycleState::Listening,
            PerRunnerAchieved::Full,
            PerRunnerPlan::Noop,
            "runners-listening-full",
        );
    }
    result(
        record,
        PerRunnerDesired::Full,
        observed.lifecycle,
        AdmissionLifecycleState::Advertising,
        PerRunnerAchieved::Transitional,
        PerRunnerPlan::AdvertiseSelector,
        "runners-advertise",
    )
}

fn result(
    record: &RunnerRecord,
    desired: PerRunnerDesired,
    observed_lifecycle: DiscoveredLifecycle,
    admission_lifecycle: AdmissionLifecycleState,
    achieved: PerRunnerAchieved,
    plan: PerRunnerPlan,
    reason: &str,
) -> PerRunnerReconcileResult {
    PerRunnerReconcileResult {
        local_runner_key: record.local_runner_key.clone(),
        desired,
        observed_lifecycle,
        admission_lifecycle,
        achieved,
        plan,
        reason_code: ReasonCode::new(reason.to_owned()).expect("static runners reason"),
    }
}

/// Detects managed-admission same-scope collisions among active enrolled records.
pub fn mark_same_scope_collisions(
    registry: &RunnerRegistry,
    observations: &mut [PerRunnerObservation],
) {
    for observation in observations.iter_mut() {
        let Some(record) = registry.get(&observation.local_runner_key) else {
            continue;
        };
        if !record.managed_admission || !record.is_active_enrollment() {
            observation.same_scope_managed_collision = false;
            continue;
        }
        observation.same_scope_managed_collision = registry.active_records().any(|other| {
            other.local_runner_key != record.local_runner_key
                && other.managed_admission
                && scopes_equal(&other.registration_scope, &record.registration_scope)
        });
    }
}

fn scopes_equal(left: &RegistrationScope, right: &RegistrationScope) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EnrollmentState, OpaqueIdentityReference, RegistryBackendBinding, RunnerRecord,
        RunnerRegistry, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
    };
    use std::path::PathBuf;

    fn identity() -> OpaqueIdentityReference {
        OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap()
    }

    fn scope(repo: &str) -> RegistrationScope {
        RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: repo.to_owned(),
        }
    }

    fn record(key: &str, repo: &str, runner_id: u64, enabled: bool, managed: bool) -> RunnerRecord {
        RunnerRecord {
            local_runner_key: key.to_owned(),
            registration_scope: scope(repo),
            runner_id,
            expected_runner_name: format!("fixture-{runner_id}"),
            runner_home: PathBuf::from(format!(r"C:\fixture\runners\{key}")),
            work_root: PathBuf::from(format!(r"C:\fixture\runners\{key}\_work")),
            execution_identity_ref: identity(),
            backend: RegistryBackendBinding::UserSession,
            desired_enabled: enabled,
            enrollment_state: EnrollmentState::Enrolled,
            credential_ref: None,
            managed_admission: managed,
        }
    }

    fn build_registry(records: Vec<RunnerRecord>) -> RunnerRegistry {
        RunnerRegistry {
            schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
            records,
            legacy_v01_binding_key: None,
        }
    }

    fn obs(
        key: &str,
        lifecycle: DiscoveredLifecycle,
        worker: bool,
        selector: Option<bool>,
    ) -> PerRunnerObservation {
        PerRunnerObservation {
            local_runner_key: key.to_owned(),
            ownership_verified: true,
            lifecycle,
            worker_present: worker,
            selector_present: selector,
            selector_observation_unknown: false,
            legacy_generic_label_routing: false,
            same_scope_managed_collision: false,
        }
    }

    fn globals(mode: UserMode, zen: ZenOverride) -> GlobalPolicyInputs {
        GlobalPolicyInputs {
            user_mode: mode,
            zen,
            hard_safety: HardSafetyState::Clear,
            probes: Vec::new(),
        }
    }

    fn globals_with_safety(
        mode: UserMode,
        zen: ZenOverride,
        hard_safety: HardSafetyState,
    ) -> GlobalPolicyInputs {
        GlobalPolicyInputs {
            user_mode: mode,
            zen,
            hard_safety,
            probes: Vec::new(),
        }
    }

    #[test]
    fn manual_disable_outranks_force_ci_and_withdraws_busy_with_selector() {
        let registry = build_registry(vec![
            record("a", "repo-a", 1, false, false),
            record("b", "repo-b", 2, true, false),
        ]);
        let observations = vec![
            obs("a", DiscoveredLifecycle::Busy, true, Some(true)),
            obs("b", DiscoveredLifecycle::Listening, false, Some(true)),
        ];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Disabled),
            &observations,
        );
        let a = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "a")
            .unwrap();
        let b = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "b")
            .unwrap();
        assert_eq!(a.plan, PerRunnerPlan::WithdrawSelector);
        assert_eq!(a.desired, PerRunnerDesired::Drained);
        assert_eq!(a.achieved, PerRunnerAchieved::Transitional);
        assert_eq!(a.reason_code.as_str(), "runners-disable-busy-withdraw");
        assert_eq!(b.desired, PerRunnerDesired::Full);
        assert_eq!(b.achieved, PerRunnerAchieved::Full);
        assert_eq!(b.plan, PerRunnerPlan::Noop);
    }

    #[test]
    fn enabled_busy_without_selector_does_not_claim_full() {
        let registry = build_registry(vec![record("a", "repo-a", 1, true, false)]);
        let observations = vec![obs("a", DiscoveredLifecycle::Busy, true, Some(false))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Disabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.desired, PerRunnerDesired::Full);
        assert_eq!(a.achieved, PerRunnerAchieved::Transitional);
        assert_eq!(a.reason_code.as_str(), "runners-busy-selector-unproven");
        assert_ne!(a.achieved, PerRunnerAchieved::Full);
    }

    #[test]
    fn zen_drains_enabled_runners_without_mutating_disabled_intent() {
        let registry = build_registry(vec![record("a", "repo-a", 1, true, false)]);
        let observations = vec![obs("a", DiscoveredLifecycle::Listening, false, Some(true))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Enabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.desired, PerRunnerDesired::Offline);
        assert_eq!(a.plan, PerRunnerPlan::WithdrawSelector);
        // Registry intent untouched by reconcile.
        assert!(registry.get("a").unwrap().desired_enabled);
    }

    #[test]
    fn zen_offline_with_selector_absent_does_not_claim_listener_stopped() {
        let registry = build_registry(vec![record("a", "repo-a", 1, true, false)]);
        let observations = vec![obs("a", DiscoveredLifecycle::Listening, false, Some(false))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Enabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.desired, PerRunnerDesired::Offline);
        assert_eq!(a.achieved, PerRunnerAchieved::Transitional);
        assert_eq!(a.reason_code.as_str(), "runners-listener-stop-unproven");
        assert_ne!(a.achieved, PerRunnerAchieved::Offline);
        assert_ne!(a.achieved, PerRunnerAchieved::Drained);
    }

    #[test]
    fn policy_drained_allows_listening_listener_when_selector_absent() {
        // ADR 0004 matrix: selector absent + no Worker => DRAINED even if Listening.
        let registry = build_registry(vec![record("a", "repo-a", 1, true, false)]);
        let observations = vec![obs("a", DiscoveredLifecycle::Listening, false, Some(false))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Work, ZenOverride::Disabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.desired, PerRunnerDesired::Drained);
        assert_eq!(a.achieved, PerRunnerAchieved::Drained);
        assert_eq!(a.admission_lifecycle, AdmissionLifecycleState::Drained);
        assert_eq!(a.plan, PerRunnerPlan::Noop);
    }

    #[test]
    fn disabled_drained_allows_listening_listener_when_selector_absent() {
        let registry = build_registry(vec![record("a", "repo-a", 1, false, false)]);
        let observations = vec![obs("a", DiscoveredLifecycle::Listening, false, Some(false))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Disabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.achieved, PerRunnerAchieved::Drained);
        assert_eq!(a.reason_code.as_str(), "runners-disabled-drained");
    }

    #[test]
    fn zen_offline_requires_stopped_listener_for_offline_achieved() {
        let registry = build_registry(vec![record("a", "repo-a", 1, true, false)]);
        let observations = vec![obs("a", DiscoveredLifecycle::Stopped, false, Some(false))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Enabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.desired, PerRunnerDesired::Offline);
        assert_eq!(a.achieved, PerRunnerAchieved::Offline);
    }

    #[test]
    fn selector_readback_uncertainty_refuses_one_runner_without_mutating_sibling() {
        let registry = build_registry(vec![
            record("uncertain", "repo-a", 1, true, false),
            record("stable", "repo-b", 2, true, false),
        ]);
        let mut uncertain = obs("uncertain", DiscoveredLifecycle::Listening, false, None);
        uncertain.selector_observation_unknown = true;
        let stable = obs("stable", DiscoveredLifecycle::Listening, false, Some(true));
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Disabled),
            &[uncertain, stable],
        );
        let uncertain = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "uncertain")
            .unwrap();
        let stable = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "stable")
            .unwrap();
        assert_eq!(uncertain.plan, PerRunnerPlan::Refuse);
        assert!(matches!(
            uncertain.achieved,
            PerRunnerAchieved::Unknown | PerRunnerAchieved::Refused
        ));
        assert_ne!(uncertain.achieved, PerRunnerAchieved::Full);
        assert_eq!(stable.achieved, PerRunnerAchieved::Full);
        assert_eq!(stable.plan, PerRunnerPlan::Noop);
        assert!(registry.get("stable").unwrap().desired_enabled);
    }

    #[test]
    fn unknown_and_legacy_routing_refuse_without_achieved_success() {
        let registry = build_registry(vec![
            record("unknown", "repo-u", 3, true, false),
            record("legacy", "repo-l", 4, true, false),
        ]);
        let mut unknown = obs("unknown", DiscoveredLifecycle::Unknown, false, None);
        unknown.selector_observation_unknown = true;
        let mut legacy = obs("legacy", DiscoveredLifecycle::Listening, false, Some(true));
        legacy.legacy_generic_label_routing = true;
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Disabled),
            &[unknown, legacy],
        );
        assert!(report.results.iter().all(|item| {
            matches!(
                item.achieved,
                PerRunnerAchieved::Unknown | PerRunnerAchieved::Refused
            ) && item.plan == PerRunnerPlan::Refuse
        }));
    }

    #[test]
    fn same_scope_managed_collision_refuses_both_without_cross_mutation() {
        let registry = build_registry(vec![
            record("a", "shared", 1, true, true),
            record("b", "shared", 2, true, true),
        ]);
        let mut observations = vec![
            obs("a", DiscoveredLifecycle::Listening, false, Some(true)),
            obs("b", DiscoveredLifecycle::Listening, false, Some(true)),
        ];
        mark_same_scope_collisions(&registry, &mut observations);
        assert!(observations
            .iter()
            .all(|item| item.same_scope_managed_collision));
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Disabled),
            &observations,
        );
        assert!(report.results.iter().all(|item| {
            item.achieved == PerRunnerAchieved::Refused
                && item.reason_code.as_str() == "runners-same-scope-collision"
        }));
    }

    #[test]
    fn racing_busy_after_selector_absent_holds_without_destructive_claim() {
        let registry = build_registry(vec![record("a", "repo-a", 1, false, false)]);
        // Selector already absent, but a racing Worker is still present.
        let observations = vec![obs("a", DiscoveredLifecycle::Busy, true, Some(false))];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Disabled),
            &observations,
        );
        let a = &report.results[0];
        assert_eq!(a.plan, PerRunnerPlan::HoldBusy);
        assert_eq!(a.achieved, PerRunnerAchieved::Transitional);
        assert_eq!(a.admission_lifecycle, AdmissionLifecycleState::DrainPending);
        assert_ne!(a.achieved, PerRunnerAchieved::Drained);
        assert_ne!(a.achieved, PerRunnerAchieved::Offline);
    }

    #[test]
    fn zen_drain_keeps_busy_and_listening_runners_independent() {
        let registry = build_registry(vec![
            record("busy", "repo-a", 1, true, false),
            record("idle", "repo-b", 2, true, false),
        ]);
        let observations = vec![
            obs("busy", DiscoveredLifecycle::Busy, true, Some(true)),
            obs("idle", DiscoveredLifecycle::Listening, false, Some(true)),
        ];
        // Zen outranks Force-CI: both desire Offline/Drain, but Busy never claims
        // drained while a Worker is held.
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::ForceCi, ZenOverride::Enabled),
            &observations,
        );
        let busy = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "busy")
            .unwrap();
        let idle = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "idle")
            .unwrap();
        assert_eq!(busy.plan, PerRunnerPlan::WithdrawSelector);
        assert_eq!(busy.achieved, PerRunnerAchieved::Transitional);
        assert_ne!(busy.achieved, PerRunnerAchieved::Drained);
        assert_ne!(busy.achieved, PerRunnerAchieved::Offline);
        assert_eq!(idle.plan, PerRunnerPlan::WithdrawSelector);
        assert_eq!(idle.achieved, PerRunnerAchieved::Transitional);
        // Plans are independent intents — neither runner mutates the other.
        assert_eq!(busy.local_runner_key, "busy");
        assert_eq!(idle.local_runner_key, "idle");
    }

    #[test]
    fn hard_safety_outranks_force_ci_without_mutating_disabled_intent() {
        let registry = build_registry(vec![
            record("enabled", "repo-a", 1, true, false),
            record("disabled", "repo-b", 2, false, false),
        ]);
        let observations = vec![
            obs("enabled", DiscoveredLifecycle::Listening, false, Some(true)),
            obs(
                "disabled",
                DiscoveredLifecycle::Listening,
                false,
                Some(true),
            ),
        ];
        let report = reconcile_multi_runners(
            &registry,
            &globals_with_safety(
                UserMode::ForceCi,
                ZenOverride::Disabled,
                HardSafetyState::Unsafe,
            ),
            &observations,
        );
        let enabled = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "enabled")
            .unwrap();
        let disabled = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "disabled")
            .unwrap();
        // Force-CI is ignored while hard safety is Unsafe: enabled runner drains.
        assert_eq!(enabled.desired, PerRunnerDesired::Drained);
        assert_eq!(enabled.plan, PerRunnerPlan::WithdrawSelector);
        assert_ne!(enabled.desired, PerRunnerDesired::Full);
        // Explicit disable still outranks for its own desired state, and the
        // registry intent is never rewritten by reconcile.
        assert_eq!(disabled.desired, PerRunnerDesired::Drained);
        assert!(!registry.get("disabled").unwrap().desired_enabled);
        assert!(registry.get("enabled").unwrap().desired_enabled);
    }

    #[test]
    fn two_runners_remain_isolated_when_only_one_is_disabled() {
        let mut registry = build_registry(vec![
            record("a", "repo-a", 1, true, false),
            record("b", "repo-b", 2, true, false),
        ]);
        registry.get_mut("a").unwrap().desired_enabled = false;
        let observations = vec![
            obs("a", DiscoveredLifecycle::Stopped, false, Some(false)),
            obs("b", DiscoveredLifecycle::Listening, false, Some(true)),
        ];
        let report = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Disabled),
            &observations,
        );
        let a = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "a")
            .unwrap();
        let b = report
            .results
            .iter()
            .find(|item| item.local_runner_key == "b")
            .unwrap();
        assert_eq!(a.desired, PerRunnerDesired::Drained);
        assert_eq!(b.desired, PerRunnerDesired::Full);
        assert_eq!(a.achieved, PerRunnerAchieved::Drained);
        assert_eq!(b.achieved, PerRunnerAchieved::Full);
        assert_eq!(a.admission_lifecycle, AdmissionLifecycleState::Drained);
        assert_eq!(b.admission_lifecycle, AdmissionLifecycleState::Listening);
        assert_eq!(a.reason_code.as_str(), "runners-disabled-drained");
        assert_eq!(b.reason_code.as_str(), "runners-listening-full");
        assert_eq!(a.plan, PerRunnerPlan::Noop);
        assert_eq!(b.plan, PerRunnerPlan::Noop);
        // Registry intent for b stays enabled; a remains independently disabled.
        assert!(!registry.get("a").unwrap().desired_enabled);
        assert!(registry.get("b").unwrap().desired_enabled);
    }

    #[test]
    fn selector_removal_alone_is_not_job_assignment_barrier_and_achieved_drained_requires_no_worker(
    ) {
        let registry = build_registry(vec![record("runner-1", "repo-a", 1, true, false)]);

        // Case 1: Work policy drain requested, selector is already absent from GitHub,
        // but an in-flight / racing job is executing (worker_present = true).
        // Selector removal alone is NOT a linearizable barrier; achieved MUST remain Transitional.
        let obs_busy = vec![obs(
            "runner-1",
            DiscoveredLifecycle::Busy,
            true,
            Some(false),
        )];
        let report_busy = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Work, ZenOverride::Disabled),
            &obs_busy,
        );
        let r1_busy = &report_busy.results[0];
        assert_eq!(r1_busy.desired, PerRunnerDesired::Drained);
        assert_eq!(
            r1_busy.admission_lifecycle,
            AdmissionLifecycleState::DrainPending
        );
        assert_eq!(r1_busy.achieved, PerRunnerAchieved::Transitional);
        assert_ne!(r1_busy.achieved, PerRunnerAchieved::Drained);
        assert_eq!(r1_busy.plan, PerRunnerPlan::HoldBusy);

        // Case 2: Racing job completes (worker_present = false), selector absent.
        // Now achieved DRAINED is reached.
        let obs_drained = vec![obs(
            "runner-1",
            DiscoveredLifecycle::Listening,
            false,
            Some(false),
        )];
        let report_drained = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Work, ZenOverride::Disabled),
            &obs_drained,
        );
        let r1_drained = &report_drained.results[0];
        assert_eq!(r1_drained.desired, PerRunnerDesired::Drained);
        assert_eq!(
            r1_drained.admission_lifecycle,
            AdmissionLifecycleState::Drained
        );
        assert_eq!(r1_drained.achieved, PerRunnerAchieved::Drained);
        assert_eq!(r1_drained.plan, PerRunnerPlan::Noop);

        // Case 3: DRAINED does NOT require Listener to be Stopped (ADR 0004 Work mode).
        assert_eq!(
            r1_drained.observed_lifecycle,
            DiscoveredLifecycle::Listening
        );

        // Case 4: Zen/Offline policy requires Listener Stopped before achieved OFFLINE is asserted.
        let report_offline_listening = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Enabled),
            &obs_drained,
        );
        let r1_zen_listening = &report_offline_listening.results[0];
        assert_eq!(r1_zen_listening.desired, PerRunnerDesired::Offline);
        assert_eq!(r1_zen_listening.achieved, PerRunnerAchieved::Transitional);
        assert_eq!(
            r1_zen_listening.reason_code.as_str(),
            "runners-listener-stop-unproven"
        );

        let obs_stopped = vec![obs(
            "runner-1",
            DiscoveredLifecycle::Stopped,
            false,
            Some(false),
        )];
        let report_offline_stopped = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Enabled),
            &obs_stopped,
        );
        let r1_zen_stopped = &report_offline_stopped.results[0];
        assert_eq!(r1_zen_stopped.desired, PerRunnerDesired::Offline);
        assert_eq!(r1_zen_stopped.achieved, PerRunnerAchieved::Offline);
    }

    #[test]
    fn service_vs_session_multi_backend_reconcile_and_no_auto_reenable() {
        let mut rec_svc = record("svc-runner", "repo-a", 10, true, false);
        rec_svc.backend = RegistryBackendBinding::WindowsService {
            service_name: "actions.runner.svc".to_owned(),
        };
        rec_svc.desired_enabled = false; // explicitly disabled

        let mut rec_sess = record("sess-runner", "repo-b", 20, true, false);
        rec_sess.backend = RegistryBackendBinding::UserSession;
        rec_sess.desired_enabled = true; // explicitly enabled

        let registry = build_registry(vec![rec_svc, rec_sess]);

        // Multiple ticks simulated
        for tick in 1..=3 {
            let observations = vec![
                obs("svc-runner", DiscoveredLifecycle::Busy, true, Some(false)),
                obs(
                    "sess-runner",
                    DiscoveredLifecycle::Listening,
                    false,
                    Some(true),
                ),
            ];
            let report = reconcile_multi_runners(
                &registry,
                &globals(UserMode::Idle, ZenOverride::Disabled),
                &observations,
            );
            let svc_res = report
                .results
                .iter()
                .find(|r| r.local_runner_key == "svc-runner")
                .unwrap();
            let sess_res = report
                .results
                .iter()
                .find(|r| r.local_runner_key == "sess-runner")
                .unwrap();

            // Service runner: explicitly disabled, worker busy -> HoldBusy, never kills Worker
            assert_eq!(svc_res.desired, PerRunnerDesired::Drained);
            assert_eq!(svc_res.plan, PerRunnerPlan::HoldBusy);
            assert_eq!(svc_res.achieved, PerRunnerAchieved::Transitional);

            // Session runner: enabled, Idle policy -> Full
            assert_eq!(sess_res.desired, PerRunnerDesired::Full);
            assert_eq!(sess_res.achieved, PerRunnerAchieved::Full);
            assert_eq!(sess_res.plan, PerRunnerPlan::Noop);

            // Registry intent is strictly preserved: never automatically re-enabled
            assert!(
                !registry.get("svc-runner").unwrap().desired_enabled,
                "tick {tick} must never re-enable disabled runner"
            );
            assert!(
                registry.get("sess-runner").unwrap().desired_enabled,
                "tick {tick} session runner remains enabled"
            );
        }
    }

    #[test]
    fn resilience_and_failure_matrix_stale_pid_uncertain_readback_and_zero_worker_termination() {
        let registry = build_registry(vec![
            record("runner-svc", "repo-a", 1, true, false),
            record("runner-sess", "repo-b", 2, true, false),
        ]);

        // Scenario 1: Stale PID / dead process recovery.
        // Even if a stale PID was once associated with runner-svc, when fresh observation reports
        // Stopped with no worker and selector absent under Drained policy (Work mode),
        // it achieves Drained immediately without hang or inventing active workers.
        let obs_stale_recovered = vec![
            obs(
                "runner-svc",
                DiscoveredLifecycle::Stopped,
                false,
                Some(false),
            ),
            obs(
                "runner-sess",
                DiscoveredLifecycle::Listening,
                false,
                Some(true),
            ),
        ];
        let rep1 = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Work, ZenOverride::Disabled),
            &obs_stale_recovered,
        );
        let svc1 = rep1
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        let sess1 = rep1
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-sess")
            .unwrap();
        assert_eq!(svc1.desired, PerRunnerDesired::Drained);
        assert_eq!(svc1.achieved, PerRunnerAchieved::Drained);
        assert_eq!(svc1.plan, PerRunnerPlan::Noop);
        // Under Work mode, runner-sess also drains cleanly because policy demands drain
        assert_eq!(sess1.desired, PerRunnerDesired::Drained);
        assert_eq!(sess1.plan, PerRunnerPlan::WithdrawSelector);

        // Scenario 2: Remote link loss / readback uncertainty fail-closed.
        // Under Idle (policy wants Full), if selector observation is unknown or None,
        // achieved must NEVER be Full.
        let mut obs_uncertain = obs("runner-svc", DiscoveredLifecycle::Listening, false, None);
        obs_uncertain.selector_observation_unknown = true;
        let obs_healthy = obs(
            "runner-sess",
            DiscoveredLifecycle::Listening,
            false,
            Some(true),
        );
        let rep2 = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Idle, ZenOverride::Disabled),
            &[obs_uncertain, obs_healthy],
        );
        let svc2 = rep2
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        let sess2 = rep2
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-sess")
            .unwrap();
        assert_eq!(svc2.desired, PerRunnerDesired::Drained);
        assert_eq!(svc2.plan, PerRunnerPlan::Refuse);
        assert_eq!(svc2.achieved, PerRunnerAchieved::Unknown);
        assert_ne!(svc2.achieved, PerRunnerAchieved::Full);
        // Healthy sibling remains Full with no cross-talk
        assert_eq!(sess2.desired, PerRunnerDesired::Full);
        assert_eq!(sess2.achieved, PerRunnerAchieved::Full);
        assert_eq!(sess2.plan, PerRunnerPlan::Noop);

        // Scenario 3: Active worker non-termination invariant under withdrawal.
        // When drain is requested (Work mode) and runner is busy, plan must be HoldBusy,
        // and achieved state must remain Transitional (never claiming Drained prematurely).
        let obs_busy = vec![
            obs("runner-svc", DiscoveredLifecycle::Busy, true, Some(false)),
            obs(
                "runner-sess",
                DiscoveredLifecycle::Listening,
                false,
                Some(false),
            ),
        ];
        let rep3 = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Work, ZenOverride::Disabled),
            &obs_busy,
        );
        let svc3 = rep3
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        assert_eq!(svc3.desired, PerRunnerDesired::Drained);
        assert_eq!(svc3.plan, PerRunnerPlan::HoldBusy);
        assert_eq!(svc3.achieved, PerRunnerAchieved::Transitional);
        assert_ne!(svc3.achieved, PerRunnerAchieved::Drained);

        // Scenario 4: Worker finishes on next tick -> transitions cleanly to Drained.
        let obs_busy_done = vec![
            obs(
                "runner-svc",
                DiscoveredLifecycle::Listening,
                false,
                Some(false),
            ),
            obs(
                "runner-sess",
                DiscoveredLifecycle::Listening,
                false,
                Some(false),
            ),
        ];
        let rep4 = reconcile_multi_runners(
            &registry,
            &globals(UserMode::Work, ZenOverride::Disabled),
            &obs_busy_done,
        );
        let svc4 = rep4
            .results
            .iter()
            .find(|r| r.local_runner_key == "runner-svc")
            .unwrap();
        assert_eq!(svc4.desired, PerRunnerDesired::Drained);
        assert_eq!(svc4.achieved, PerRunnerAchieved::Drained);
        assert_eq!(svc4.plan, PerRunnerPlan::Noop);
    }
}
