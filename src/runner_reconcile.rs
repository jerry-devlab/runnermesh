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
}
