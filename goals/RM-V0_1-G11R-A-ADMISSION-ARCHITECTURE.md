# G11R-A — Admission Linearization Architecture

Type: **Autonomous architecture/research train**

Real runner mutation: **forbidden**

Status: **Accepted — ADR 0004**

## Mission

Choose and formally define the v0.1 capacity-withdrawal mechanism before another real-host qualification attempt.

The Goal is not to make historical G11/PR #17 pass. The Goal is to prove that the selected mechanism can satisfy the product contract truthfully.

## Required product semantic

RunnerMesh remains human-first. When policy requests Work/Gaming/Zen/drain, new CI capacity must be withdrawn according to a precisely defined transition while an eligible active normal job is allowed to finish rather than being destructively killed.

Separate these cases explicitly:

- `Busy -> Drain`: active-job preservation and eventual withdrawal;
- `Listening -> Drain`: idle admission withdrawal and its race/linearization boundary.

## Required option study

Compare at least:

1. persistent official runner + local Listener lifecycle control;
2. `run.cmd --once` job leases;
3. GitHub server-side labels / runner groups as admission control;
4. ephemeral / JIT runner leases;
5. a clarified two-phase withdrawal semantic if it preserves the frozen human-first contract without claiming impossible instantaneous revocation.

For each option record:

- exact linearization point;
- no-new-job guarantee after that point;
- active-job survival;
- behavior if a job races with withdrawal;
- GitHub API/token/Organization authority;
- registration lifecycle mutation;
- local privilege/UAC needs;
- restart/reconstruction behavior;
- upstream support/deprecation risk;
- compatibility with an already-configured official runner;
- implementation/operational complexity;
- whether the v0.1 design freeze changes.

## Historical evidence to reuse

- CTRL+C/CTRL+BREAK are not accepted Busy-drain mechanisms;
- Busy drain must not normally signal/kill the active Worker;
- PR #17 exact runner-home process scoping and safe-wait reconstruction are useful evidence;
- `--once` is a candidate mechanism, not a preselected architecture;
- historical qualification variants are evidence, not a template for continued transaction proliferation.

## Required deliverables

Create one accepted ADR that defines:

```text
ADMISSION_ARCHITECTURE=GITHUB_NATIVE_DYNAMIC_ADMISSION_LABEL
WITHDRAWAL_PROTOCOL=TWO_PHASE
DESIRED_STATE_VS_ACHIEVED_STATE=EXPLICIT
SERVER_CONTROL_POINT=RESERVED_LABEL_REMOVAL_AND_READBACK
SCHEDULER_LINEARIZABILITY=NOT_CLAIMED_WITHOUT_UPSTREAM_GUARANTEE
ACTIVE_JOB_POLICY=COMPLETE_NATURALLY
RACING_JOB_POLICY=CONSERVATIVE_IN_FLIGHT_ASSIGNMENT_MAY_COMPLETE
REQUIRED_GITHUB_AUTHORITY=MINIMAL_RESERVED_LABEL_MUTATION_AUTHORITY
DESIGN_FREEZE_CHANGE=TRUST_BOUNDARY_EXPANSION_PLUS_SEMANTIC_CLARIFICATION
SEMANTIC_WEAKENING=FALSE
```

Also provide a migration/salvage assessment for PR #17.

## Hard boundaries

No service mutation, runner registration mutation, runner labels/groups mutation, trusted workflow dispatch, work-root cleanup, or production runtime activation.

## Exit

`PASS` only when the architecture is selected and the product semantic is explicit enough that G11R-B can implement deterministic fixtures without inventing behavior.

Next: G11R-B.
