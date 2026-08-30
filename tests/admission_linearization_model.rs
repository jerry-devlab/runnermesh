const RESERVED_LABEL: &str = "runnermesh-admit";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesiredCapacity {
    Full,
    Drained,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectorObservation {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    Full,
    Advertising,
    ReAdvertising,
    Listening,
    AssignmentPending,
    Busy,
    WithdrawRequested,
    Withdrawing,
    WithdrawalBlocked,
    DrainPending,
    Drained,
    Unknown,
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reason {
    ApiUnavailable,
    AuthenticationFailed,
    RateLimited,
    ConnectivityUnknown,
    RunnerUnavailable,
    RegistrationDrift,
    RunnerIdentityDrift,
    ReservedLabelOwnershipDrift,
    SelectorCollision,
    LocalEvidenceInconsistent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReconstructionEvidence {
    desired: DesiredCapacity,
    selector: SelectorObservation,
    worker_active: bool,
    assignment_in_flight: bool,
    local_consistent: bool,
    runner_identity_matches: bool,
    registration_matches: bool,
    reserved_label_owned: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    PolicyRequestsFull,
    PolicyRequestsDrained,
    LabelAddRequested,
    LabelAddAcknowledged,
    LabelPresenceReadback,
    LabelRemoveRequested,
    LabelRemoveAcknowledged,
    LabelAbsenceReadback,
    JobAssignmentObserved,
    WorkerStarts,
    WorkerCompletes,
    ApiUnavailable,
    ApiAuthFailure,
    ApiRateLimit,
    ConnectivityUnknown,
    AgentRestarts(ReconstructionEvidence),
    RunnerDisappears,
    RegistrationDrift,
    RunnerIdentityDrift,
    ReservedLabelOwnershipDrift,
    SelectorCollision,
    LocalEvidenceInconsistent,
    UnrelatedLabelObserved,
    UnrelatedSameNameProcessObserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdmissionModel {
    state: LifecycleState,
    desired: DesiredCapacity,
    selector: SelectorObservation,
    worker_active: bool,
    assignment_in_flight: bool,
    local_consistent: bool,
    runner_identity_matches: bool,
    registration_matches: bool,
    reserved_label_owned: bool,
    reason: Option<Reason>,
    reserved_label_adds: u32,
    reserved_label_removes: u32,
    unrelated_label_mutations: u32,
    destructive_worker_actions: u32,
    unrelated_process_actions: u32,
    last_mutated_label: Option<&'static str>,
}

impl AdmissionModel {
    fn drained() -> Self {
        Self {
            state: LifecycleState::Drained,
            desired: DesiredCapacity::Drained,
            selector: SelectorObservation::Absent,
            worker_active: false,
            assignment_in_flight: false,
            local_consistent: true,
            runner_identity_matches: true,
            registration_matches: true,
            reserved_label_owned: true,
            reason: None,
            reserved_label_adds: 0,
            reserved_label_removes: 0,
            unrelated_label_mutations: 0,
            destructive_worker_actions: 0,
            unrelated_process_actions: 0,
            last_mutated_label: None,
        }
    }

    fn apply(&mut self, event: Event) {
        match event {
            Event::PolicyRequestsFull => {
                if self.state != LifecycleState::Refused {
                    self.desired = DesiredCapacity::Full;
                    self.reason = None;
                    self.state = LifecycleState::Full;
                }
            }
            Event::PolicyRequestsDrained => {
                if self.state != LifecycleState::Refused {
                    self.desired = DesiredCapacity::Drained;
                    self.reason = None;
                    self.state = LifecycleState::WithdrawRequested;
                }
            }
            Event::LabelAddRequested => {
                if self.mutation_is_allowed() && self.desired == DesiredCapacity::Full {
                    self.reserved_label_adds += 1;
                    self.last_mutated_label = Some(RESERVED_LABEL);
                    self.state = LifecycleState::ReAdvertising;
                }
            }
            Event::LabelAddAcknowledged => {
                if self.state == LifecycleState::ReAdvertising {
                    self.state = LifecycleState::Advertising;
                }
            }
            Event::LabelPresenceReadback => {
                self.selector = SelectorObservation::Present;
                if self.desired == DesiredCapacity::Full && self.binding_is_consistent() {
                    self.reason = None;
                    self.state = if self.worker_active {
                        LifecycleState::Busy
                    } else {
                        LifecycleState::Listening
                    };
                } else if self.desired == DesiredCapacity::Drained {
                    self.state = LifecycleState::Withdrawing;
                }
            }
            Event::LabelRemoveRequested => {
                if self.mutation_is_allowed() && self.desired == DesiredCapacity::Drained {
                    self.reserved_label_removes += 1;
                    self.last_mutated_label = Some(RESERVED_LABEL);
                    self.state = LifecycleState::Withdrawing;
                }
            }
            Event::LabelRemoveAcknowledged => {
                if self.state == LifecycleState::Withdrawing {
                    // The acknowledgement is not selector readback and cannot
                    // establish achieved withdrawal.
                }
            }
            Event::LabelAbsenceReadback => {
                self.selector = SelectorObservation::Absent;
                self.reason = None;
                self.state = if self.worker_active || self.assignment_in_flight {
                    LifecycleState::DrainPending
                } else if self.desired == DesiredCapacity::Drained && self.binding_is_consistent() {
                    LifecycleState::Drained
                } else {
                    LifecycleState::Advertising
                };
            }
            Event::JobAssignmentObserved => {
                self.assignment_in_flight = true;
                self.state = if self.desired == DesiredCapacity::Drained {
                    LifecycleState::DrainPending
                } else {
                    LifecycleState::AssignmentPending
                };
            }
            Event::WorkerStarts => {
                self.assignment_in_flight = false;
                self.worker_active = true;
                self.state = if self.desired == DesiredCapacity::Drained {
                    LifecycleState::DrainPending
                } else if self.selector == SelectorObservation::Present {
                    LifecycleState::Busy
                } else {
                    LifecycleState::Unknown
                };
            }
            Event::WorkerCompletes => {
                self.assignment_in_flight = false;
                self.worker_active = false;
                self.state = match (self.desired, self.selector) {
                    (DesiredCapacity::Drained, SelectorObservation::Absent)
                        if self.binding_is_consistent() =>
                    {
                        LifecycleState::Drained
                    }
                    (DesiredCapacity::Drained, SelectorObservation::Absent) => {
                        LifecycleState::Unknown
                    }
                    (DesiredCapacity::Drained, SelectorObservation::Present) => {
                        LifecycleState::WithdrawRequested
                    }
                    (DesiredCapacity::Drained, SelectorObservation::Unknown) => {
                        LifecycleState::Unknown
                    }
                    (DesiredCapacity::Full, SelectorObservation::Present)
                        if self.binding_is_consistent() =>
                    {
                        LifecycleState::Listening
                    }
                    (DesiredCapacity::Full, _) => LifecycleState::Advertising,
                };
            }
            Event::ApiUnavailable => self.block(Reason::ApiUnavailable),
            Event::ApiAuthFailure => self.block(Reason::AuthenticationFailed),
            Event::ApiRateLimit => self.block(Reason::RateLimited),
            Event::ConnectivityUnknown => {
                self.selector = SelectorObservation::Unknown;
                self.block(Reason::ConnectivityUnknown);
            }
            Event::AgentRestarts(evidence) => self.reconstruct(evidence),
            Event::RunnerDisappears => {
                self.selector = SelectorObservation::Unknown;
                self.block(Reason::RunnerUnavailable);
            }
            Event::RegistrationDrift => self.refuse(Reason::RegistrationDrift),
            Event::RunnerIdentityDrift => self.refuse(Reason::RunnerIdentityDrift),
            Event::ReservedLabelOwnershipDrift => self.refuse(Reason::ReservedLabelOwnershipDrift),
            Event::SelectorCollision => self.refuse(Reason::SelectorCollision),
            Event::LocalEvidenceInconsistent => {
                self.local_consistent = false;
                self.selector = SelectorObservation::Unknown;
                self.state = LifecycleState::Unknown;
                self.reason = Some(Reason::LocalEvidenceInconsistent);
            }
            Event::UnrelatedLabelObserved | Event::UnrelatedSameNameProcessObserved => {
                // Observation grants no authority and causes no control action.
            }
        }
    }

    fn reconstruct(&mut self, evidence: ReconstructionEvidence) {
        self.desired = evidence.desired;
        self.selector = evidence.selector;
        self.worker_active = evidence.worker_active;
        self.assignment_in_flight = evidence.assignment_in_flight;
        self.local_consistent = evidence.local_consistent;
        self.runner_identity_matches = evidence.runner_identity_matches;
        self.registration_matches = evidence.registration_matches;
        self.reserved_label_owned = evidence.reserved_label_owned;
        self.reason = None;

        if !self.runner_identity_matches {
            self.refuse(Reason::RunnerIdentityDrift);
        } else if !self.registration_matches {
            self.refuse(Reason::RegistrationDrift);
        } else if !self.reserved_label_owned {
            self.refuse(Reason::ReservedLabelOwnershipDrift);
        } else if !self.local_consistent {
            self.state = LifecycleState::Unknown;
            self.reason = Some(Reason::LocalEvidenceInconsistent);
        } else {
            self.state = match (self.desired, self.selector) {
                (DesiredCapacity::Full, SelectorObservation::Present) => {
                    if self.worker_active {
                        LifecycleState::Busy
                    } else {
                        LifecycleState::Listening
                    }
                }
                (DesiredCapacity::Full, SelectorObservation::Absent) => {
                    LifecycleState::ReAdvertising
                }
                (DesiredCapacity::Full, SelectorObservation::Unknown) => LifecycleState::Unknown,
                (DesiredCapacity::Drained, SelectorObservation::Present) => {
                    LifecycleState::WithdrawRequested
                }
                (DesiredCapacity::Drained, SelectorObservation::Absent) => {
                    if self.worker_active || self.assignment_in_flight {
                        LifecycleState::DrainPending
                    } else {
                        LifecycleState::Drained
                    }
                }
                (DesiredCapacity::Drained, SelectorObservation::Unknown) => LifecycleState::Unknown,
            };
        }
    }

    fn block(&mut self, reason: Reason) {
        self.reason = Some(reason);
        self.state = if self.desired == DesiredCapacity::Drained {
            LifecycleState::WithdrawalBlocked
        } else {
            LifecycleState::Unknown
        };
    }

    fn refuse(&mut self, reason: Reason) {
        match reason {
            Reason::RunnerIdentityDrift => self.runner_identity_matches = false,
            Reason::RegistrationDrift => self.registration_matches = false,
            Reason::ReservedLabelOwnershipDrift | Reason::SelectorCollision => {
                self.reserved_label_owned = false
            }
            _ => {}
        }
        self.reason = Some(reason);
        self.state = LifecycleState::Refused;
    }

    fn binding_is_consistent(self) -> bool {
        self.local_consistent
            && self.runner_identity_matches
            && self.registration_matches
            && self.reserved_label_owned
    }

    fn mutation_is_allowed(self) -> bool {
        self.state != LifecycleState::Refused && self.binding_is_consistent()
    }

    fn achieved_full(self) -> bool {
        self.desired == DesiredCapacity::Full
            && self.selector == SelectorObservation::Present
            && self.binding_is_consistent()
            && matches!(self.state, LifecycleState::Listening | LifecycleState::Busy)
    }

    fn achieved_drained(self) -> bool {
        self.desired == DesiredCapacity::Drained
            && self.selector == SelectorObservation::Absent
            && !self.worker_active
            && !self.assignment_in_flight
            && self.binding_is_consistent()
            && self.state == LifecycleState::Drained
    }
}

fn listening_model() -> AdmissionModel {
    let mut model = AdmissionModel::drained();
    model.apply(Event::PolicyRequestsFull);
    assert_eq!(model.state, LifecycleState::Full);
    model.apply(Event::LabelAddRequested);
    assert_eq!(model.state, LifecycleState::ReAdvertising);
    model.apply(Event::LabelAddAcknowledged);
    assert_eq!(model.state, LifecycleState::Advertising);
    assert!(!model.achieved_full());
    model.apply(Event::LabelPresenceReadback);
    assert!(model.achieved_full());
    model
}

fn busy_model() -> AdmissionModel {
    let mut model = listening_model();
    model.apply(Event::JobAssignmentObserved);
    model.apply(Event::WorkerStarts);
    assert_eq!(model.state, LifecycleState::Busy);
    model
}

#[test]
fn p1_normal_withdrawal_never_kills_an_active_worker() {
    let mut model = busy_model();
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    model.apply(Event::LabelRemoveAcknowledged);
    model.apply(Event::LabelAbsenceReadback);

    assert_eq!(model.state, LifecycleState::DrainPending);
    assert!(model.worker_active);
    assert_eq!(model.destructive_worker_actions, 0);

    model.apply(Event::WorkerCompletes);
    assert!(model.achieved_drained());
}

#[test]
fn p2_drained_is_never_reported_while_selector_is_present() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    model.apply(Event::LabelRemoveAcknowledged);
    model.apply(Event::LabelPresenceReadback);

    assert_eq!(model.selector, SelectorObservation::Present);
    assert_eq!(model.state, LifecycleState::Withdrawing);
    assert!(!model.achieved_drained());
}

#[test]
fn p3_drained_is_never_reported_while_exact_worker_is_active() {
    let mut model = busy_model();
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    model.apply(Event::LabelAbsenceReadback);

    assert!(model.worker_active);
    assert_eq!(model.state, LifecycleState::DrainPending);
    assert!(!model.achieved_drained());
}

#[test]
fn p4_mutation_and_readback_uncertainty_never_become_success() {
    for event in [
        Event::ApiUnavailable,
        Event::ApiAuthFailure,
        Event::ApiRateLimit,
        Event::ConnectivityUnknown,
        Event::RunnerDisappears,
        Event::LocalEvidenceInconsistent,
    ] {
        let mut model = listening_model();
        model.apply(Event::PolicyRequestsDrained);
        model.apply(Event::LabelRemoveRequested);
        model.apply(event);

        assert!(matches!(
            model.state,
            LifecycleState::WithdrawalBlocked | LifecycleState::Unknown
        ));
        assert!(!model.achieved_drained());
    }
}

#[test]
fn p5_racing_assignment_is_visible_and_may_complete() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    model.apply(Event::LabelRemoveAcknowledged);
    model.apply(Event::LabelAbsenceReadback);
    assert!(model.achieved_drained());

    model.apply(Event::JobAssignmentObserved);
    assert_eq!(model.state, LifecycleState::DrainPending);
    assert!(model.assignment_in_flight);
    assert!(!model.achieved_drained());

    model.apply(Event::WorkerStarts);
    assert!(model.worker_active);
    assert_eq!(model.destructive_worker_actions, 0);
    model.apply(Event::WorkerCompletes);
    assert!(model.achieved_drained());
}

#[test]
fn p6_remove_acknowledgement_alone_does_not_end_active_work() {
    let mut model = busy_model();
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    model.apply(Event::LabelRemoveAcknowledged);

    assert_eq!(model.selector, SelectorObservation::Present);
    assert!(model.worker_active);
    assert_eq!(model.state, LifecycleState::Withdrawing);
    assert!(!model.achieved_drained());
}

#[test]
fn p7_restart_reconstructs_desired_remote_and_local_state_separately() {
    let mut model = listening_model();
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    model.apply(Event::LabelRemoveAcknowledged);

    model.apply(Event::AgentRestarts(ReconstructionEvidence {
        desired: DesiredCapacity::Drained,
        selector: SelectorObservation::Present,
        worker_active: false,
        assignment_in_flight: false,
        local_consistent: true,
        runner_identity_matches: true,
        registration_matches: true,
        reserved_label_owned: true,
    }));
    assert_eq!(model.state, LifecycleState::WithdrawRequested);
    assert!(!model.achieved_drained());

    model.apply(Event::AgentRestarts(ReconstructionEvidence {
        desired: DesiredCapacity::Drained,
        selector: SelectorObservation::Absent,
        worker_active: true,
        assignment_in_flight: false,
        local_consistent: true,
        runner_identity_matches: true,
        registration_matches: true,
        reserved_label_owned: true,
    }));
    assert_eq!(model.state, LifecycleState::DrainPending);
    assert!(!model.achieved_drained());

    model.apply(Event::AgentRestarts(ReconstructionEvidence {
        desired: DesiredCapacity::Drained,
        selector: SelectorObservation::Unknown,
        worker_active: false,
        assignment_in_flight: false,
        local_consistent: true,
        runner_identity_matches: true,
        registration_matches: true,
        reserved_label_owned: true,
    }));
    assert_eq!(model.state, LifecycleState::Unknown);
    assert!(!model.achieved_drained());
}

#[test]
fn p8_runner_identity_and_registration_drift_refuse_mutation() {
    for drift in [Event::RunnerIdentityDrift, Event::RegistrationDrift] {
        let mut model = listening_model();
        model.apply(Event::PolicyRequestsDrained);
        model.apply(drift);
        let removals_before = model.reserved_label_removes;
        model.apply(Event::LabelRemoveRequested);

        assert_eq!(model.state, LifecycleState::Refused);
        assert_eq!(model.reserved_label_removes, removals_before);
    }
}

#[test]
fn p9_reserved_label_ownership_drift_refuses_correction() {
    for drift in [Event::ReservedLabelOwnershipDrift, Event::SelectorCollision] {
        let mut model = listening_model();
        model.apply(Event::PolicyRequestsDrained);
        model.apply(drift);
        model.apply(Event::LabelRemoveRequested);

        assert_eq!(model.state, LifecycleState::Refused);
        assert_eq!(model.reserved_label_removes, 0);
    }
}

#[test]
fn p10_only_the_reserved_label_can_be_mutated() {
    let mut model = AdmissionModel::drained();
    model.apply(Event::UnrelatedLabelObserved);
    model.apply(Event::PolicyRequestsFull);
    model.apply(Event::LabelAddRequested);
    assert_eq!(model.last_mutated_label, Some(RESERVED_LABEL));
    assert_eq!(model.reserved_label_adds, 1);
    assert_eq!(model.unrelated_label_mutations, 0);

    model.apply(Event::LabelAddAcknowledged);
    model.apply(Event::LabelPresenceReadback);
    model.apply(Event::PolicyRequestsDrained);
    model.apply(Event::LabelRemoveRequested);
    assert_eq!(model.last_mutated_label, Some(RESERVED_LABEL));
    assert_eq!(model.reserved_label_removes, 1);
    assert_eq!(model.unrelated_label_mutations, 0);
}

#[test]
fn p11_unrelated_same_name_processes_remain_outside_authority() {
    let mut model = listening_model();
    let before = model;
    model.apply(Event::UnrelatedSameNameProcessObserved);

    assert_eq!(model.state, before.state);
    assert_eq!(model.worker_active, before.worker_active);
    assert_eq!(model.unrelated_process_actions, 0);
}

#[test]
fn p12_full_readvertisement_requires_presence_readback() {
    let mut model = AdmissionModel::drained();
    model.apply(Event::PolicyRequestsFull);
    assert_eq!(model.state, LifecycleState::Full);
    assert!(!model.achieved_full());

    model.apply(Event::LabelAddRequested);
    assert_eq!(model.state, LifecycleState::ReAdvertising);
    assert!(!model.achieved_full());

    model.apply(Event::LabelAddAcknowledged);
    assert_eq!(model.state, LifecycleState::Advertising);
    assert!(!model.achieved_full());

    model.apply(Event::LabelPresenceReadback);
    assert_eq!(model.state, LifecycleState::Listening);
    assert!(model.achieved_full());
}

#[test]
fn restart_refuses_each_exact_binding_drift_family() {
    let cases = [
        (
            ReconstructionEvidence {
                desired: DesiredCapacity::Drained,
                selector: SelectorObservation::Absent,
                worker_active: false,
                assignment_in_flight: false,
                local_consistent: true,
                runner_identity_matches: false,
                registration_matches: true,
                reserved_label_owned: true,
            },
            Reason::RunnerIdentityDrift,
        ),
        (
            ReconstructionEvidence {
                desired: DesiredCapacity::Drained,
                selector: SelectorObservation::Absent,
                worker_active: false,
                assignment_in_flight: false,
                local_consistent: true,
                runner_identity_matches: true,
                registration_matches: false,
                reserved_label_owned: true,
            },
            Reason::RegistrationDrift,
        ),
        (
            ReconstructionEvidence {
                desired: DesiredCapacity::Drained,
                selector: SelectorObservation::Absent,
                worker_active: false,
                assignment_in_flight: false,
                local_consistent: true,
                runner_identity_matches: true,
                registration_matches: true,
                reserved_label_owned: false,
            },
            Reason::ReservedLabelOwnershipDrift,
        ),
    ];

    for (evidence, reason) in cases {
        let mut model = listening_model();
        model.apply(Event::AgentRestarts(evidence));
        assert_eq!(model.state, LifecycleState::Refused);
        assert_eq!(model.reason, Some(reason));
    }
}
