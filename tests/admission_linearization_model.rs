#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    OfflineUnavailable,
    Withdrawn,
    FullAvailable,
    Listening,
    AssignmentPending,
    Busy,
    WithdrawRequested,
    DrainPending,
    Reconnecting,
    Unknown,
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReconstructionEvidence {
    Offline,
    Withdrawn,
    Listening,
    AssignmentPending,
    Busy,
    WithdrawRequested,
    DrainPendingBeforeBarrier,
    DrainPendingAfterBarrier,
    Unknown,
    Drift,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    PolicyRequestsFull,
    ReconnectBegins,
    ListenerBecomesRemotelyAvailable,
    PolicyRequestsWithdrawal,
    AssignmentBeforeBarrier,
    AdmissionBarrierCommits,
    AssignmentAfterBarrier,
    WorkerBegins,
    WorkerCompletes,
    ListenerExits,
    AgentRestarts(ReconstructionEvidence),
    RunnerDisappears,
    ConnectivityBecomesUncertain,
    BindingDriftAppears,
    UnrelatedRunnerAppears,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdmissionModel {
    state: LifecycleState,
    withdraw_intent: bool,
    barrier_committed: bool,
    worker_active: bool,
    admitted_jobs: u32,
    refused_post_barrier_assignments: u32,
    unrelated_runner_control_actions: u32,
}

impl AdmissionModel {
    fn offline() -> Self {
        Self {
            state: LifecycleState::OfflineUnavailable,
            withdraw_intent: true,
            barrier_committed: true,
            worker_active: false,
            admitted_jobs: 0,
            refused_post_barrier_assignments: 0,
            unrelated_runner_control_actions: 0,
        }
    }

    fn apply(&mut self, event: Event) {
        match event {
            Event::PolicyRequestsFull => {
                if !matches!(
                    self.state,
                    LifecycleState::Unknown | LifecycleState::Refused
                ) {
                    self.withdraw_intent = false;
                    self.barrier_committed = false;
                    self.state = LifecycleState::FullAvailable;
                }
            }
            Event::ReconnectBegins => {
                if self.state == LifecycleState::FullAvailable {
                    self.state = LifecycleState::Reconnecting;
                }
            }
            Event::ListenerBecomesRemotelyAvailable => {
                if self.state == LifecycleState::Reconnecting {
                    self.state = LifecycleState::Listening;
                }
            }
            Event::PolicyRequestsWithdrawal => {
                self.withdraw_intent = true;
                self.state = match self.state {
                    LifecycleState::Busy | LifecycleState::AssignmentPending => {
                        LifecycleState::DrainPending
                    }
                    LifecycleState::Listening
                    | LifecycleState::FullAvailable
                    | LifecycleState::Reconnecting => LifecycleState::WithdrawRequested,
                    current => current,
                };
            }
            Event::AssignmentBeforeBarrier => {
                if !self.barrier_committed
                    && matches!(
                        self.state,
                        LifecycleState::Listening | LifecycleState::WithdrawRequested
                    )
                {
                    self.admitted_jobs += 1;
                    self.state = LifecycleState::AssignmentPending;
                }
            }
            Event::AdmissionBarrierCommits => {
                if matches!(
                    self.state,
                    LifecycleState::Unknown | LifecycleState::Refused
                ) {
                    return;
                }
                self.barrier_committed = true;
                self.state = if self.worker_active
                    || matches!(
                        self.state,
                        LifecycleState::Busy | LifecycleState::AssignmentPending
                    ) {
                    LifecycleState::DrainPending
                } else {
                    LifecycleState::Withdrawn
                };
            }
            Event::AssignmentAfterBarrier => {
                if self.barrier_committed {
                    self.refused_post_barrier_assignments += 1;
                    self.state = LifecycleState::Refused;
                } else {
                    self.admitted_jobs += 1;
                    self.state = LifecycleState::AssignmentPending;
                }
            }
            Event::WorkerBegins => {
                if matches!(
                    self.state,
                    LifecycleState::AssignmentPending | LifecycleState::DrainPending
                ) {
                    self.worker_active = true;
                    self.state = if self.withdraw_intent {
                        LifecycleState::DrainPending
                    } else {
                        LifecycleState::Busy
                    };
                }
            }
            Event::WorkerCompletes => {
                self.worker_active = false;
                self.state = if self.withdraw_intent {
                    if self.barrier_committed {
                        LifecycleState::Withdrawn
                    } else {
                        LifecycleState::WithdrawRequested
                    }
                } else {
                    LifecycleState::Listening
                };
            }
            Event::ListenerExits => {
                self.state = if self.barrier_committed && !self.worker_active {
                    LifecycleState::OfflineUnavailable
                } else {
                    LifecycleState::Unknown
                };
            }
            Event::AgentRestarts(evidence) => {
                self.worker_active = matches!(
                    evidence,
                    ReconstructionEvidence::Busy
                        | ReconstructionEvidence::DrainPendingBeforeBarrier
                        | ReconstructionEvidence::DrainPendingAfterBarrier
                );
                self.state = match evidence {
                    ReconstructionEvidence::Offline => {
                        self.withdraw_intent = true;
                        self.barrier_committed = true;
                        LifecycleState::OfflineUnavailable
                    }
                    ReconstructionEvidence::Withdrawn => {
                        self.withdraw_intent = true;
                        self.barrier_committed = true;
                        LifecycleState::Withdrawn
                    }
                    ReconstructionEvidence::Listening => {
                        self.withdraw_intent = false;
                        self.barrier_committed = false;
                        LifecycleState::Listening
                    }
                    ReconstructionEvidence::AssignmentPending => {
                        self.withdraw_intent = false;
                        self.barrier_committed = false;
                        LifecycleState::AssignmentPending
                    }
                    ReconstructionEvidence::Busy => {
                        self.withdraw_intent = false;
                        self.barrier_committed = false;
                        LifecycleState::Busy
                    }
                    ReconstructionEvidence::WithdrawRequested => {
                        self.withdraw_intent = true;
                        self.barrier_committed = false;
                        LifecycleState::WithdrawRequested
                    }
                    ReconstructionEvidence::DrainPendingBeforeBarrier => {
                        self.withdraw_intent = true;
                        self.barrier_committed = false;
                        LifecycleState::DrainPending
                    }
                    ReconstructionEvidence::DrainPendingAfterBarrier => {
                        self.withdraw_intent = true;
                        self.barrier_committed = true;
                        LifecycleState::DrainPending
                    }
                    ReconstructionEvidence::Unknown => LifecycleState::Unknown,
                    ReconstructionEvidence::Drift => LifecycleState::Refused,
                };
            }
            Event::RunnerDisappears | Event::ConnectivityBecomesUncertain => {
                self.worker_active = false;
                self.state = LifecycleState::Unknown;
            }
            Event::BindingDriftAppears => {
                self.worker_active = false;
                self.state = LifecycleState::Refused;
            }
            Event::UnrelatedRunnerAppears => {
                // Exact ownership is unchanged. No control action is issued.
            }
        }
    }

    fn achieved_full(self) -> bool {
        self.state == LifecycleState::Listening && !self.withdraw_intent && !self.barrier_committed
    }

    fn achieved_withdrawn(self) -> bool {
        self.barrier_committed
            && matches!(
                self.state,
                LifecycleState::Withdrawn
                    | LifecycleState::DrainPending
                    | LifecycleState::OfflineUnavailable
            )
    }
}

fn listening_model() -> AdmissionModel {
    let mut model = AdmissionModel::offline();
    model.apply(Event::PolicyRequestsFull);
    model.apply(Event::ReconnectBegins);
    model.apply(Event::ListenerBecomesRemotelyAvailable);
    assert!(model.achieved_full());
    model
}

#[test]
fn idle_withdrawal_is_not_achieved_until_the_barrier_commits() {
    let mut model = listening_model();

    model.apply(Event::PolicyRequestsWithdrawal);
    assert_eq!(model.state, LifecycleState::WithdrawRequested);
    assert!(!model.achieved_withdrawn());

    model.apply(Event::AdmissionBarrierCommits);
    assert_eq!(model.state, LifecycleState::Withdrawn);
    assert!(model.achieved_withdrawn());
}

#[test]
fn assignment_before_the_barrier_wins_and_finishes_normally() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsWithdrawal);
    model.apply(Event::AssignmentBeforeBarrier);
    assert_eq!(model.state, LifecycleState::AssignmentPending);

    model.apply(Event::AdmissionBarrierCommits);
    model.apply(Event::WorkerBegins);
    assert_eq!(model.state, LifecycleState::DrainPending);
    assert!(model.worker_active);

    model.apply(Event::WorkerCompletes);
    assert_eq!(model.state, LifecycleState::Withdrawn);
    assert!(!model.worker_active);
    assert_eq!(model.admitted_jobs, 1);
}

#[test]
fn ordinary_busy_withdrawal_has_no_worker_kill_transition() {
    let mut model = listening_model();
    model.apply(Event::AssignmentBeforeBarrier);
    model.apply(Event::WorkerBegins);
    assert_eq!(model.state, LifecycleState::Busy);

    model.apply(Event::PolicyRequestsWithdrawal);
    model.apply(Event::AdmissionBarrierCommits);
    assert_eq!(model.state, LifecycleState::DrainPending);
    assert!(model.worker_active);

    model.apply(Event::WorkerCompletes);
    assert_eq!(model.state, LifecycleState::Withdrawn);
}

#[test]
fn assignment_after_the_barrier_is_refused() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsWithdrawal);
    model.apply(Event::AdmissionBarrierCommits);
    let admitted_before = model.admitted_jobs;

    model.apply(Event::AssignmentAfterBarrier);
    assert_eq!(model.state, LifecycleState::Refused);
    assert_eq!(model.admitted_jobs, admitted_before);
    assert_eq!(model.refused_post_barrier_assignments, 1);
}

#[test]
fn a_late_barrier_response_cannot_clear_unknown_evidence() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsWithdrawal);
    model.apply(Event::ConnectivityBecomesUncertain);
    model.apply(Event::AdmissionBarrierCommits);

    assert_eq!(model.state, LifecycleState::Unknown);
    assert!(!model.achieved_withdrawn());
}

#[test]
fn uncertainty_and_drift_never_report_achieved_full_or_withdrawn() {
    for event in [
        Event::ConnectivityBecomesUncertain,
        Event::RunnerDisappears,
        Event::BindingDriftAppears,
    ] {
        let mut model = listening_model();
        model.apply(event);
        assert!(!model.achieved_full());
        assert!(!model.achieved_withdrawn());
        assert!(matches!(
            model.state,
            LifecycleState::Unknown | LifecycleState::Refused
        ));
    }
}

#[test]
fn unrelated_runner_never_becomes_controlled() {
    let mut model = listening_model();
    let before = model;
    model.apply(Event::UnrelatedRunnerAppears);

    assert_eq!(model.state, before.state);
    assert_eq!(model.worker_active, before.worker_active);
    assert_eq!(model.unrelated_runner_control_actions, 0);
}

#[test]
fn restart_reconstructs_current_evidence_and_refuses_drift() {
    let cases = [
        (
            ReconstructionEvidence::Offline,
            LifecycleState::OfflineUnavailable,
        ),
        (ReconstructionEvidence::Withdrawn, LifecycleState::Withdrawn),
        (ReconstructionEvidence::Listening, LifecycleState::Listening),
        (
            ReconstructionEvidence::AssignmentPending,
            LifecycleState::AssignmentPending,
        ),
        (ReconstructionEvidence::Busy, LifecycleState::Busy),
        (
            ReconstructionEvidence::WithdrawRequested,
            LifecycleState::WithdrawRequested,
        ),
        (
            ReconstructionEvidence::DrainPendingBeforeBarrier,
            LifecycleState::DrainPending,
        ),
        (
            ReconstructionEvidence::DrainPendingAfterBarrier,
            LifecycleState::DrainPending,
        ),
        (ReconstructionEvidence::Unknown, LifecycleState::Unknown),
        (ReconstructionEvidence::Drift, LifecycleState::Refused),
    ];

    for (evidence, expected) in cases {
        let mut model = listening_model();
        model.apply(Event::AgentRestarts(evidence));
        assert_eq!(model.state, expected);
        if evidence == ReconstructionEvidence::DrainPendingBeforeBarrier {
            assert!(!model.achieved_withdrawn());
        }
        if evidence == ReconstructionEvidence::DrainPendingAfterBarrier {
            assert!(model.achieved_withdrawn());
        }
    }
}

#[test]
fn listener_exit_without_a_proved_barrier_is_unknown() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsWithdrawal);
    model.apply(Event::ListenerExits);

    assert_eq!(model.state, LifecycleState::Unknown);
    assert!(!model.achieved_withdrawn());
}

#[test]
fn listener_exit_after_barrier_and_race_settlement_is_offline() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsWithdrawal);
    model.apply(Event::AdmissionBarrierCommits);
    model.apply(Event::ListenerExits);

    assert_eq!(model.state, LifecycleState::OfflineUnavailable);
    assert!(model.achieved_withdrawn());
}
