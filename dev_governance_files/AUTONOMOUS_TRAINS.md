# Autonomous Development Trains

RunnerMesh supports long unattended development while retaining focused Goals and hard human gates.

## Unit of work

A Train is orchestration across Goals, **not** one giant branch.

Each Goal performs:

```text
sync authoritative main
-> focused branch
-> implementation
-> focused iteration tests
-> settle candidate
-> required risk gates
-> PR
-> hosted CI / ordinary self-repair
-> merge
-> verify remote main
-> durable receipt
-> next Goal
```

Remote Git `main`, PR state, and CI state are durable checkpoints. Do not invent a parallel custom lock/slot system for normal source development.

## Writer model

Exactly one Implementer writes a branch/worktree. Preserve foreign work. Architect/reviewer/auditor roles are read-oriented unless write authority is explicitly transferred.

## Self-repair

For ordinary deterministic failures, up to three materially distinct repair cycles per failure family are allowed. Eligible examples: compile, tests, rustfmt, clippy, deterministic IPC/fixture tests, docs links, hosted-CI mechanical defects.

After budget exhaustion: stop, record the failure fingerprint, and do not spin indefinitely.

## Human-gate stop conditions

Unattended trains stop before:

- UAC/elevation request;
- real Windows Service mutation;
- real official-runner registration mutation;
- destructive real work-root mutation;
- GitHub Organization runner-access/security changes;
- new secret/trust authority;
- production autostart activation;
- installed stable RunnerMesh mutation;
- real production cutover;
- public release publication;
- destructive active-job termination;
- ambiguous ownership/security state.

Only explicit Owner authorization for that gate permits continuation.

## Production protection

Ordinary development always assumes `PRODUCTION_MUTATION=false`.

Source branches, local Cargo builds/tests, hosted CI, and PR merges must not stop, overwrite, reconfigure, or replace an installed stable runtime. Arbitrary worktree binaries are never production dogfood; use immutable authorized RC/release artifacts.

## v0.1 trains

- **Train A:** G02-G07 — fully autonomous.
- **Train B1:** G08-G10 — autonomous; real production runner remains read-only/non-mutated.
- **H1 / G11:** real official-runner lifecycle and graceful-drain qualification — mandatory Owner gate.
- **Train C:** G12-G15 — autonomous in sandbox/install fixtures after H1 passes.
- **H2 / G16:** real workstation cutover/dogfood — mandatory Owner gate.
- **G17:** RC closeout — can run automatically after successful G16 evidence, but must not publish stable release.
- **H3 / G18:** public v0.1.0 publication — mandatory Owner gate.

A train may stop earlier on a blocker, ambiguous scope, security issue, or exhausted repair budget.
