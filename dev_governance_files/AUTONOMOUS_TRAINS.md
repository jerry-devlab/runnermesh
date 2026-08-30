# Autonomous Development Trains

RunnerMesh supports long unattended development while retaining focused Goals and hard human gates.

The authoritative remaining v0.1 sequence is `goals/RM-V0_1-ROADMAP.md`; current state is `goals/RM-V0_1-EXECUTION-STATUS.md`.

## Unit of work

A Train is orchestration across Goals, **not** one giant branch.

Each ordinary Goal performs:

```text
sync authoritative main
-> focused branch
-> implementation/research
-> focused iteration tests
-> settle candidate
-> required risk gates
-> PR
-> hosted CI / ordinary self-repair
-> merge
-> verify remote main
-> durable ledger/receipt update
-> next Goal
```

Remote Git `main`, PR state, hosted CI, and the public execution ledger are durable checkpoints. Do not invent a parallel custom lock/slot system for normal source development.

After a passing exact-head PR merges, verify remote `main` immediately.  Work on
the next safe Goal may overlap the post-merge main CI run, but the next PR must
not merge until that prior main run is healthy.  Latch a post-merge failure and
stop the merge pipeline until it is understood or resolved.

## Writer model

Exactly one Implementer writes a branch/worktree. Preserve foreign work. Architect/reviewer/auditor roles are read-oriented unless write authority is explicitly transferred.

## Two classes of execution

### Autonomous Train

Use for research, design, source implementation, tests, sandbox/integration qualification, docs, PRs, hosted CI, packaging rehearsal, and deterministic self-repair.

Normal duration is **6-12 hours**. A train may run for **up to 24 hours** when the plan contains enough independent source/design work to make that useful. Do not manufacture work merely to consume the timebox.

### Owner Transaction

Use only for real privilege/trust/production boundaries. Owner transactions should be completely prepared before they begin, normally take 15-120 minutes, and perform a bounded mutation with automatic restore/rollback where practical.

Owner transactions do not perform architecture discovery or open-ended source development.

## Self-repair

For ordinary deterministic failures, up to three materially distinct repair cycles per failure family are allowed. Eligible examples: compile, tests, rustfmt, clippy, deterministic IPC/lifecycle fixtures, docs links, hosted-CI mechanical defects, package/sandbox failures.

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

Only explicit Owner authorization for that prepared transaction permits continuation.

Do not place UAC, service mutation, GitHub Organization authority expansion, subjective Owner approval, and open-ended implementation into one nominally unattended Goal.

## Prepare everything first

Real-host qualification/cutover follows:

```text
source candidate frozen
routing/workflows ready
rollback/recovery ready
host prestate verified
all readiness fields PASS
-> one Owner gate
-> bounded transaction
-> automatic restore/rollback attempt
-> durable receipt
```

Do not mutate the real host and then discover the next routing/workflow prerequisite.

## Production protection

Ordinary development always assumes `PRODUCTION_MUTATION=false`.

Source branches, local Cargo builds/tests, hosted CI, and PR merges must not stop, overwrite, reconfigure, or replace an installed stable runtime. Arbitrary worktree binaries are never production dogfood; use immutable authorized RC/release artifacts.

## v0.1 roadmap-v2 trains

- **Accepted foundation:** G01-G10 + G06R + G10R.
- **P0:** historical G11 recovery-only closeout — Owner transaction; no qualification continuation.
- **GOV1:** execution ledger/docs plus recommended main protection.
- **G11R-A:** admission-linearization architecture — autonomous.
- **G11R-B:** lifecycle implementation — autonomous.
- **G11R-C:** qualification readiness — autonomous; must stop before H1 mutation.
- **H1:** one-shot real qualification — Owner transaction with automatic restore attempt.
- **Train C2:** G12-G15 productization rewrite/salvage — autonomous in sandbox/install fixtures.
- **G15R:** integrated pre-H2 RC — autonomous, no production activation.
- **H2/G16-A:** real workstation cutover — Owner transaction.
- **H2/G16-B:** sustained ordinary-use dogfood — minimum 24 hours before G17.
- **G17:** RC closeout — autonomous; no stable publication.
- **H3/G18:** public v0.1.0 publication — Owner transaction.

Historical V3/V4/V4R/V4S qualification variants are retained evidence, not the pattern for future work. Aim for one accepted architecture, one readiness gate, and one H1 transaction family.

## Execution ledger

Every future agent reads `goals/RM-V0_1-EXECUTION-STATUS.md` before acting and updates only decision-relevant rows after accepted merges or material blocker changes. Keep all private host identifiers out of the public ledger.

A train may stop earlier on a blocker, ambiguous scope, security issue, exhausted repair budget, or true Owner gate.
