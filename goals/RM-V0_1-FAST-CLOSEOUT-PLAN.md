# RunnerMesh v0.1 Fast Closeout Plan

Status: **Proposed execution overlay for Roadmap v3**

This document does not create Roadmap v4 and does not change the frozen v0.1 product contract. It compresses the remaining Roadmap v3 work into five execution blocks by reusing accepted evidence, batching compatible acceptance work, and eliminating repeat ceremony.

## Objective

Target the shortest credible path from current pre-H1 state to `v0.1.0` while preserving the real safety gates:

```text
TARGET_ACTIVE_ENGINEERING=10-15h
HARD_WALL_CLOCK_GATE=>=24h ordinary-use dogfood
```

The estimate is a planning target, not a release criterion. A newly discovered product P0/P1 defect may extend it.

## Codex execution rule

Owner preference for remaining v0.1 development:

```text
CODEX_MODE=YOLO
DEFAULT_LAUNCH=codex --yolo -m gpt-5.6-sol
```

`--yolo` removes routine local approval friction. It is **not** authority to cross a Human/Owner gate, mutate production outside the active Goal, bypass protected `main`, touch unrelated runners/worktrees/services, or publish a release early. Goal scope and explicit Owner authorization remain authoritative.

## What is intentionally deferred

Unless a release-blocking defect proves otherwise, defer beyond v0.1:

- richer `auditor_preflight.py` failure observability beyond what is needed for the next accepted run;
- Python App Execution Alias repair;
- local WSL portability bootstrap;
- generalized stable/volatile identity framework;
- generalized Codex sandbox ACL cleanup tooling;
- extended 48-72h dogfood beyond the mandatory 24h minimum;
- new orchestration, audit, recovery, or CI platforms.

Do not remove already-prepared G12-G15 product features merely to save time. Their source already exists in PR #33; the optimization is to batch acceptance, not rewrite the scope.

## Fast closeout blocks

| Block | Goal | Target active time | Exit condition |
|---|---|---:|---|
| F1 | `RM-V0_1-F1-P0-H1-OWNER-CORRIDOR` | 2-4h | P0 PASS, H1 `QUALIFICATION=PASS`, H1 `RESTORE=PASS` |
| F2 | `RM-V0_1-F2-PRODUCTIZATION-ACCEPTANCE` | 1-2h | PR #33-equivalent G12-G15 source accepted on protected main |
| F3 | `RM-V0_1-F3-RC-RELEASE-HARDENING` | 3-5h | one immutable G15R RC; explicit Named Pipe DACL debt closed |
| F4 | `RM-V0_1-F4-H2-CUTOVER-DOGFOOD` | 1-2h + >=24h wall clock | successful cutover and minimum sustained dogfood PASS |
| F5 | `RM-V0_1-F5-RELEASE-CLOSEOUT` | 1.5-3h | G17 PASS and Owner-published `v0.1.0` verified |

## Process compression rules

1. **One accepted risk delta, one validation family.** Reuse accepted unchanged-risk evidence; do not restart comprehensive audits.
2. **P0 retry budget is one fresh bounded transaction.** If the historical target is again blocked by non-product infrastructure after the current material evidence change, stop archaeology. An explicit Owner decision may retire that historical qualification target and establish a clean official runner for H1 instead; this escape hatch is never automatic.
3. **P0 and H1 may share one Owner session, not one authorization.** P0 must independently PASS first. Then a new H1 phase/gate may begin without restarting the whole Codex context.
4. **G12-G15 accept as one coherent productization PR when possible.** Do not create four ceremonial acceptance PRs if the prepared source remains reviewable and current.
5. **G15R is integration evidence, not a second implementation project.** Reuse G12-G15 unit/ownership evidence and focus on the package -> install -> runtime -> update -> rollback -> uninstall chain.
6. **Named Pipe explicit DACL closes inside F3.** Do not create a standalone Goal/PR unless the change unexpectedly becomes large or independent.
7. **Use the 24h dogfood window productively.** Release notes, docs, artifact naming, checksum/provenance preparation, and H3 commands may be prepared in parallel; stable publication remains forbidden until dogfood passes.
8. **G17 and H3 may share one final release session.** G17 acceptance/freeze must complete before the explicit H3 Owner publication gate.
9. **Generic review cannot relatch an accepted Owner policy decision without new technical evidence.** Follow Blocker Policy v2.
10. **No work is manufactured to consume a timebox.** If an execution block reaches its true acceptance state early, stop.

## Critical-path state

Current source posture entering this plan:

```text
CORE_RUNTIME=ACCEPTED
G11R_SOURCE=ACCEPTED
H1_LIVE_ADAPTER_SOURCE=ACCEPTED_SOURCE
BLOCKER_POLICY_V2=ACCEPTED
P0_CURRENT_INVARIANT_MODEL=ACCEPTED
H1_OWNER_BUNDLE_SOURCE_PREPARED=true
G12_G15_SOURCE_PREPARED=true   # PR #33, held behind H1 gate
```

The remaining critical path is therefore:

```text
F1 P0 + H1
  -> F2 G12-G15 acceptance
  -> F3 G15R + release hardening
  -> F4 H2 + >=24h dogfood
  -> F5 G17 + H3
  -> v0.1.0
```

## Stop / escalation policy

A block is allowed to expand the schedule only when it is one of:

- a fresh product P0/P1 defect;
- a real security/trust ambiguity required by the active risk policy;
- failed post-mutation restoration;
- failed H1/H2 live acceptance;
- release artifact/provenance failure.

Routine tool friction, stale historical evidence, Owner wait/cancellation, and unchanged audit disagreement are classified and bounded under Blocker Policy v2 rather than turned into new project phases.

## Goal files

Execute the following in order:

1. `goals/RM-V0_1-F1-P0-H1-OWNER-CORRIDOR.md`
2. `goals/RM-V0_1-F2-PRODUCTIZATION-ACCEPTANCE.md`
3. `goals/RM-V0_1-F3-RC-RELEASE-HARDENING.md`
4. `goals/RM-V0_1-F4-H2-CUTOVER-DOGFOOD.md`
5. `goals/RM-V0_1-F5-RELEASE-CLOSEOUT.md`

Where this overlay conflicts with a frozen product invariant, security policy, or an explicit Owner gate in Roadmap v3 / `QUALITY_GATES.md`, the stricter accepted authority wins.