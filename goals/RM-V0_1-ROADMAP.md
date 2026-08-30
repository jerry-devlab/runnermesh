# RunnerMesh v0.1 Implementation Roadmap v2

Status: **Accepted implementation sequence**

This roadmap supersedes the original post-G10R execution sequence from G11 onward. G01-G10, G06R, and G10R remain accepted. The product contract in `docs/v0.1-design-freeze.md` remains authoritative unless an explicit ADR changes it.

The reset follows the 2026-08-30 product/architecture audit. The audit concluded that the original G11 qualification path had drifted toward a qualification framework, that `Busy -> Drain` and `Listening -> Drain` must be separated, and that real-host mutation must not begin before routing, workflow, recovery, and rollback prerequisites are ready.

## Product invariant carried forward

RunnerMesh v0.1 remains a Windows, single-workstation, human-first GitHub Actions admission/lifecycle controller for an already-configured official self-hosted runner.

Key invariants remain:

- human activity has priority over CI;
- uncertainty fails closed for new CI admission;
- active normal jobs are not destructively terminated for ordinary Work/Gaming/Zen/drain transitions;
- GitHub Actions owns workflow scheduling and the job protocol;
- RunnerMesh manages contributed capacity, admission, and lifecycle;
- one execution identity owns one active work root;
- `SOURCE != BUILD != RELEASE != INSTALLED RUNTIME != ACTIVE VERSION`;
- production-style mutations occur only behind explicit Owner gates.

## Accepted foundation

The following implementation is accepted and remains the baseline:

- G01 Domain foundation;
- G02 Runtime contracts;
- G03 Agent Core;
- G04 Local Named Pipe IPC;
- G05 CLI control;
- G06 Tray presentation contracts;
- G07 Probes + Auto Lite;
- G08 Runner observer;
- G09 Supervisor core;
- G10 Host observation + recovery model;
- G06R Native tray + persistent ordinary-user Agent runtime;
- G10R Pre-H1 integration readiness;
- PR #18 Windows native process snapshot hotfix removing runtime `tasklist` polling.

The historical G11 qualification path is superseded. PR #17 is retained as research/salvage material until G11R-B decides whether to refactor or replace it.

## P0 — Recovery-only closeout

Purpose: close the terminal historical G11 experiment and return the host to a known-good baseline.

This is not a product-development Goal and must not continue qualification after recovery.

Acceptance:

- original service running;
- service-backed bound Listener present;
- bound Worker absent;
- historical orphan Listener absent;
- qualification workspace clean;
- service config/security, registration, runner home, and work root unchanged;
- unrelated runners untouched.

Human gate: one narrowly scoped recovery authorization/UAC when required.

## GOV1 — Governance and durable execution state

Before long unattended writers are treated as routine:

- create and maintain `goals/RM-V0_1-EXECUTION-STATUS.md`;
- refresh stale public project-status documentation;
- require future agents to read the roadmap and execution ledger before acting;
- recommend repository enforcement for PR-only main updates, hosted CI, force-push protection, and branch-deletion protection.

GitHub Organization/repository setting changes remain an Owner action; source/docs work is autonomous.

## G11R-A — Admission Linearization Architecture

Autonomous architecture/research train. No real runner mutation.

Mission: define the exact capacity-withdrawal semantic and its linearization point before another real qualification attempt.

The analysis must compare at least:

1. persistent official runner + local Listener lifecycle control;
2. `run.cmd --once` job leases;
3. GitHub server-side labels/runner groups as an admission barrier;
4. ephemeral/JIT runner leases;
5. an explicit clarification of the v0.1 withdrawal transition if the existing product semantic can be preserved without claiming impossible instantaneous revocation.

For every option evaluate human-first correctness, no-new-job boundary, active-job survival, GitHub authority, registration mutation, local privilege, upstream support/deprecation risk, restart recovery, implementation complexity, and fit with the frozen v0.1 contract.

Do not select a mechanism merely because existing code already implements it.

Exit requires an accepted ADR with:

```text
ADMISSION_ARCHITECTURE=ACCEPTED
LINEARIZATION_POINT=DEFINED
ACTIVE_JOB_POLICY=DEFINED
IDLE_WITHDRAWAL_POLICY=DEFINED
REQUIRED_GITHUB_AUTHORITY=DEFINED
REQUIRED_LOCAL_AUTHORITY=DEFINED
```

If the winning design weakens the frozen product semantic or adds materially new server/JIT authority, update the design freeze through an explicit ADR before G11R-B.

## G11R-B — Lifecycle Implementation

Autonomous source-development train, normally 6-12 hours. No real production runner mutation.

Implement the accepted G11R-A lifecycle architecture, including:

- FULL/listening/busy/withdraw-requested-or-drain-pending/withdrawn/reconnect semantics;
- exact runner-home/process ownership;
- active-job preservation;
- idle withdrawal behavior defined by the accepted ADR;
- restart/reconnect/reconstruction;
- unrelated same-name runner isolation;
- registration and work-root drift refusal;
- one-identity/one-work-root enforcement;
- source/runtime separation.

PR #17 may be salvaged, refactored, or superseded. Its useful evidence includes exact process scoping, rejection of CTRL+C/CTRL+BREAK for Busy drain, safe-wait reconstruction, and run-once experiments. `RUN_ONCE_JOB_LEASE` is not preselected as the final product architecture.

Exit:

```text
G11R_CODE_READY=true
SYNTHETIC_LIFECYCLE_TESTS=PASS
REAL_HOST_MUTATION=false
```

## G11R-C — Qualification Readiness

Autonomous readiness train. Prepare everything before mutating the real host.

Required readiness surfaces:

- exact trusted private qualification workflow;
- exact runner selector/routing strategy;
- primary/no-admission/reconnect/failure witnesses;
- source candidate frozen;
- host prestate verifier;
- rollback and automatic restore plan;
- crash/timeout recovery semantics;
- one-shot transaction generator;
- privacy-safe durable receipts.

No real host mutation may begin unless all are true:

```text
SOURCE_READY=true
HOST_PRESTATE_READY=true
ROUTING_READY=true
TRUSTED_WORKFLOW_READY=true
ROLLBACK_READY=true
RECOVERY_READY=true
SELECTOR_UNIQUE=true
OWNER_GATE_READY=true
```

Principle: **prepare everything first; mutate the real host last.**

## Human Gate H1 — One-shot real qualification

One prepared transaction proves the accepted admission/lifecycle architecture on the real trusted runner.

Owner interaction should be one explicit authorization/UAC wherever feasible. The transaction performs bounded qualification and attempts automatic restoration for PASS, FAIL, BLOCKED, timeout, or controller loss.

Final receipt always separates:

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
```

Only `RESTORE=FAIL` should require emergency Owner recovery.

Train C2 begins only after H1 qualification PASS and restored baseline PASS.

## Train C2 — Productization rewrite/salvage

Existing draft PRs #14-#16 are implementation assets, not mandatory merge candidates. Extract useful code onto the accepted G11R baseline.

### G12 — User Autostart

- user-session login start;
- stable activation entry only;
- source-tree paths forbidden;
- duplicate Agent authority prevented;
- safe enable/disable/remove semantics.

### G13 — Versioned Installation

- immutable version slots;
- stable activation indirection;
- config/state/log separation;
- ownership-safe install/uninstall;
- foreign content preserved;
- doctor aware of installed/active state.

### G14 — Update + Rollback

- stage -> verify -> compatibility -> durable READY_TO_ACTIVATE -> safe-point -> activate -> health-check -> commit;
- rollback/reconciliation after interruption;
- active CI jobs are not killed by RunnerMesh update.

### G15 — Packaging + Doctor

- Windows x64 package;
- provenance and SHA-256;
- package verification;
- install/update/rollback sandbox dry runs;
- hardened doctor;
- hosted public CI only;
- public privacy audit.

Old PR #14 should be split conceptually into G12 and G13. PRs #15 and #16 are salvaged after G11R; they are not merged unchanged merely because they are already written.

## G15R — Pre-H2 integrated RC

Autonomous integration train. Build exactly one authoritative-main RC and exercise it entirely in sandbox/development roots before H2.

Prove package -> install -> Agent/Tray/CLI -> policy/probes -> lifecycle integration -> update -> rollback -> uninstall. Recheck the Windows background-process regression:

```text
TASKLIST_RUNTIME_CALLS=0
RUNNERMESH_TASKLIST_CHILD_COUNT=0
VISIBLE_CONSOLE_FLASH=false
```

Exit:

```text
H2_RC_HEAD=<authoritative-main>
H2_RC_SHA256=<sha256>
H2_RC_READY=true
```

Only this immutable RC may enter H2.

## Human Gate H2 / G16 — Real cutover + sustained dogfood

### H2-A One-shot cutover

Install and activate the immutable authorized RC, preserve rollback, and prove real tray/CLI/Auto Lite/probes/modes/Zen/lifecycle/autostart/restart/source-runtime isolation/update rollback/uninstall-recovery semantics.

### H2-B Sustained dogfood

Minimum release gate: **24 hours** of ordinary workstation use after successful cutover. Prefer 48-72 hours when practical.

Observe and later audit durable evidence for crashes, wrong admission, stale UI, mode transition failures, resource anomalies, console flash, stale listeners, reconnect failures, autostart, suspend/resume, and user interference.

G17 may begin only after the minimum dogfood window passes without a release-blocking lifecycle fault.

## G17 — RC closeout

Autonomous closeout train:

- freeze the exact candidate;
- reconcile the execution/evidence ledger;
- refresh README/architecture/install/update/rollback documentation;
- prepare release notes and known limitations;
- run hosted exact-head CI, privacy/security review, package/checksum/provenance checks;
- bind the sustained-dogfood receipt;
- do not publish stable release.

## Human Gate H3 / G18 — v0.1.0 publication

After explicit Owner authorization:

- tag `v0.1.0`;
- publish Windows x64 artifact, checksums, release notes;
- verify public provenance and downloadability;
- stop after release verification;
- do not automatically start v0.2.

## Autonomous work model

Use two classes of work:

### Autonomous Train

Normally 6-12 hours, and up to 24 hours when the plan contains enough independent source/design work. It may research, design, implement, test, sandbox, open/update PRs, use hosted CI, self-repair deterministic failures, and merge ordinary admitted source/docs changes.

It must stop before true privilege/trust/production boundaries.

### Owner Transaction

Normally 15-120 minutes. It starts only after readiness is complete and performs a bounded privileged/trust/production mutation with automatic restoration/rollback where practical. Owner transactions do not perform architecture discovery.

Do not combine UAC, real service mutation, GitHub Organization authority expansion, subjective visual approval, and open-ended source design into a nominally unattended train.

## Goal discipline

Every Goal:

1. reads this roadmap and `RM-V0_1-EXECUTION-STATUS.md` first;
2. starts from authoritative main or an admitted predecessor;
3. preserves foreign/local work;
4. uses one focused writer per branch/worktree;
5. declares changed-risk vector and non-goals;
6. reuses accepted unchanged-risk evidence explicitly;
7. stops at true human gates rather than inventing micro-gates;
8. verifies remote main after merge;
9. updates the execution ledger with privacy-safe durable state;
10. emits a concise receipt.

The historical V3/V4/V4R/V4S qualification variants are evidence, not a template for continued transaction proliferation. One accepted architecture, one readiness gate, and one H1 transaction family is the target.