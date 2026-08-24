use crate::{
    AdmissionDecision, HardSafetyState, NodeState, ProbeRuntimeState, ProbeSnapshot, ReasonCode,
    UserMode, ZenOverride,
};

/// Applies the frozen precedence order:
/// hard safety > Zen > explicit non-auto mode > conservative Auto Lite.
///
/// The policy consumes only normalized probe snapshots. Provider-specific Steam,
/// process, registry, and input details remain behind the probe boundary.
pub fn decide_admission(
    user_mode: UserMode,
    zen: ZenOverride,
    hard_safety: HardSafetyState,
    probes: &[ProbeSnapshot],
) -> AdmissionDecision {
    match hard_safety {
        HardSafetyState::Unsafe => return denied(NodeState::Drained, "hard-safety-unsafe", true),
        HardSafetyState::Unknown => return denied(NodeState::Drained, "hard-safety-unknown", true),
        HardSafetyState::Clear => {}
    }

    if zen == ZenOverride::Enabled {
        return denied(NodeState::Offline, "zen-enabled", true);
    }

    match user_mode {
        UserMode::Maintenance => denied(NodeState::Offline, "manual-maintenance", true),
        UserMode::Work => denied(NodeState::Drained, "manual-work", true),
        UserMode::Gaming => denied(NodeState::Drained, "manual-gaming", true),
        UserMode::Idle => allowed("manual-idle"),
        // Force CI bypasses ordinary activity evidence, never hard safety.
        UserMode::ForceCi => allowed("manual-force-ci"),
        UserMode::Auto => decide_auto_lite(probes),
    }
}

fn decide_auto_lite(probes: &[ProbeSnapshot]) -> AdmissionDecision {
    let enabled = probes
        .iter()
        .filter(|probe| probe.enabled)
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return denied(NodeState::Drained, "auto-no-enabled-probes", true);
    }
    if enabled
        .iter()
        .any(|probe| probe.runtime_state == ProbeRuntimeState::Active)
    {
        return denied(NodeState::Drained, "auto-probe-active", true);
    }
    if enabled
        .iter()
        .any(|probe| probe.runtime_state == ProbeRuntimeState::Unknown)
    {
        return denied(NodeState::Drained, "auto-probe-unknown", true);
    }

    let idle_evidence = enabled.iter().any(|probe| {
        probe.id.as_str() == "user-activity" && probe.runtime_state == ProbeRuntimeState::Inactive
    });
    let all_remaining_evidence_permits = enabled.iter().all(|probe| {
        matches!(
            probe.runtime_state,
            ProbeRuntimeState::Inactive | ProbeRuntimeState::Unavailable
        )
    });
    if idle_evidence && all_remaining_evidence_permits {
        allowed("auto-idle-permits")
    } else {
        denied(NodeState::Drained, "auto-awaiting-idle-evidence", true)
    }
}

fn allowed(reason: &'static str) -> AdmissionDecision {
    AdmissionDecision {
        allow_new_work: true,
        desired_node_state: NodeState::Full,
        reason_code: static_reason(reason),
        drain_requested: false,
    }
}

fn denied(node_state: NodeState, reason: &'static str, drain_requested: bool) -> AdmissionDecision {
    AdmissionDecision {
        allow_new_work: false,
        desired_node_state: node_state,
        reason_code: static_reason(reason),
        drain_requested,
    }
}

fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static policy reason codes must be valid")
}

#[cfg(test)]
mod tests {
    use super::decide_admission;
    use crate::{
        HardSafetyState, NodeState, ProbeHealth, ProbeId, ProbeRuntimeState, ProbeSnapshot,
        UserMode, ZenOverride,
    };

    #[test]
    fn precedence_and_auto_lite_matrix_is_conservative() {
        let cases = [
            Case {
                name: "hard safety precedes force ci",
                mode: UserMode::ForceCi,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Unsafe,
                probes: vec![probe("user-activity", true, ProbeRuntimeState::Inactive)],
                node_state: NodeState::Drained,
                allow: false,
                reason: "hard-safety-unsafe",
            },
            Case {
                name: "zen precedes manual idle",
                mode: UserMode::Idle,
                zen: ZenOverride::Enabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("user-activity", true, ProbeRuntimeState::Inactive)],
                node_state: NodeState::Offline,
                allow: false,
                reason: "zen-enabled",
            },
            Case {
                name: "manual work precedes activity probes",
                mode: UserMode::Work,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("user-activity", true, ProbeRuntimeState::Inactive)],
                node_state: NodeState::Drained,
                allow: false,
                reason: "manual-work",
            },
            Case {
                name: "force ci ignores ordinary active evidence",
                mode: UserMode::ForceCi,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("user-activity", true, ProbeRuntimeState::Active)],
                node_state: NodeState::Full,
                allow: true,
                reason: "manual-force-ci",
            },
            Case {
                name: "active probe drains auto",
                mode: UserMode::Auto,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("steam-game", true, ProbeRuntimeState::Active)],
                node_state: NodeState::Drained,
                allow: false,
                reason: "auto-probe-active",
            },
            Case {
                name: "unknown probe drains auto",
                mode: UserMode::Auto,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("user-activity", true, ProbeRuntimeState::Unknown)],
                node_state: NodeState::Drained,
                allow: false,
                reason: "auto-probe-unknown",
            },
            Case {
                name: "all useful probes disabled drains auto",
                mode: UserMode::Auto,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("user-activity", false, ProbeRuntimeState::Inactive)],
                node_state: NodeState::Drained,
                allow: false,
                reason: "auto-no-enabled-probes",
            },
            Case {
                name: "idle evidence and inactive activity permits full",
                mode: UserMode::Auto,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![
                    probe("user-activity", true, ProbeRuntimeState::Inactive),
                    probe("process-list", true, ProbeRuntimeState::Inactive),
                ],
                node_state: NodeState::Full,
                allow: true,
                reason: "auto-idle-permits",
            },
            Case {
                name: "unavailable steam is not inactive but can permit with idle evidence",
                mode: UserMode::Auto,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![
                    probe("user-activity", true, ProbeRuntimeState::Inactive),
                    probe("steam-game", true, ProbeRuntimeState::Unavailable),
                ],
                node_state: NodeState::Full,
                allow: true,
                reason: "auto-idle-permits",
            },
            Case {
                name: "unavailable user activity cannot prove idle",
                mode: UserMode::Auto,
                zen: ZenOverride::Disabled,
                safety: HardSafetyState::Clear,
                probes: vec![probe("user-activity", true, ProbeRuntimeState::Unavailable)],
                node_state: NodeState::Drained,
                allow: false,
                reason: "auto-awaiting-idle-evidence",
            },
        ];

        for case in cases {
            let decision = decide_admission(case.mode, case.zen, case.safety, &case.probes);
            assert_eq!(
                decision.desired_node_state, case.node_state,
                "{}",
                case.name
            );
            assert_eq!(decision.allow_new_work, case.allow, "{}", case.name);
            assert_eq!(decision.reason_code.as_str(), case.reason, "{}", case.name);
        }
    }

    struct Case {
        name: &'static str,
        mode: UserMode,
        zen: ZenOverride,
        safety: HardSafetyState,
        probes: Vec<ProbeSnapshot>,
        node_state: NodeState,
        allow: bool,
        reason: &'static str,
    }

    fn probe(id: &str, enabled: bool, runtime_state: ProbeRuntimeState) -> ProbeSnapshot {
        ProbeSnapshot {
            id: ProbeId::new(id).unwrap(),
            enabled,
            health: ProbeHealth::Healthy,
            runtime_state,
            reason_code: None,
        }
    }
}
