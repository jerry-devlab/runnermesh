# G11R-A — Admission Linearization Architecture

Type: **Autonomous architecture/research train**

Real runner mutation: **forbidden**

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
ADMISSION_ARCHITECTURE=<mechanism>
WITHDRAWAL_STATE_MACHINE=<defined>
LINEARIZATION_POINT=<defined event>
ACTIVE_JOB_POLICY=<defined>
IDLE_WITHDRAWAL_POLICY=<defined>
RACING_JOB_POLICY=<defined>
REQUIRED_GITHUB_AUTHORITY=<defined>
REQUIRED_LOCAL_AUTHORITY=<defined>
DESIGN_FREEZE_CHANGE=<NONE|ADR_REQUIRED>
```

Also provide a migration/salvage assessment for PR #17.

## Hard boundaries

No service mutation, runner registration mutation, runner labels/groups mutation, trusted workflow dispatch, work-root cleanup, or production runtime activation.

## Exit

`PASS` only when the architecture is selected and the product semantic is explicit enough that G11R-B can implement deterministic fixtures without inventing behavior.

Next: G11R-B.