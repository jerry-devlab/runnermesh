# RM-160H remaining roadmap — Owner gates & program closeout

Status: **ACTIVE AFTER SOURCE CLOSEOUT**. Supplements (does not replace)
`goals/RM-160H-DELIVERY-PROGRAM-001/ROADMAP.md`.

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
PHASE=RM-160H-PHASE-R-OWNER-CLOSEOUT
SOURCE_AND_SANDBOX_CLOSEOUT=PASS
LIVE_DELIVERY=WAITING_FOR_OWNER
CANDIDATE_PR=65
BRANCH=feat/160h-gt01-field-baseline
WORKTREE_LABEL=runnermesh-160h
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
REMOTE_TIP=6e6990b6b0c500a3c5b9dd10f0b5c8ee324e1236
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
HOSTED_CI_RUN=42646514218
R0=PASS
PRODUCTION_MUTATION=false_until_exact_owner_gate
```

## 0. What is already done

GT-01 … GT-07 source/sandbox trains closed. Packages P02–P06 and P08 are
`PASS_SOURCE`. P07 qualification packets are `PREPARED`. P01 public preflight is
`READY_FOR_OWNER`. P09 reserve may stay unused.

Do **not** reopen completed source leaves to fill hours. Do **not** invent Mesh,
Linux, Broker, enforcement Job Objects, or v1.0 GA inside this remaining phase.

## 1. Two deliverable tracks (keep separate)

| Track | Goal | Authoritative artifact |
|---|---|---|
| **v0.1 C0** | H2 + dogfood + G17 + H3 on immutable G15R RC | `G15R_RC` only — never PR #65 tip |
| **v0.2-A LRM** | HUX-2/HUX-3 + optional merge of Draft #65 + later RC/publish | PR #65 / Owner-chosen tip |

Crossing tracks (e.g. cutting over H2 with #65 tip) is forbidden.

## 2. Remaining Goal Trains (Owner-gated)

Budget below is **planning ceiling for Owner-session + Implementer assist**, not
a mandate to spend. Prefer short Owner transactions (15–120 min) per AGENTS.md.

| Train | Cap | Focus | Stop before |
|---|---:|---|---|
| **R0** Packet polish | 4h | Refresh public readiness docs to exact tip; privacy audit; merge-readiness notes | Any live mutation |
| **R1** G-RO HUX-1 ZenBookDuo | 4h | Read-only discover on ZenBookDuo; privacy-safe counts | Registration/install/SCM write |
| **R2** Independent security review | 8h | External reviewer on exact HEAD; Implementer only supplies diff map | Treat self-review as PASS |
| **R3** H2 v0.1 cutover | 6h active + 24h dogfood elapsed | G15R RC only; new token; RESTORE separate | Using #65 tip as RC |
| **R4** HUX-2 live | 6h | Disposable Service; Stop→readback→Start→restore | Dogfood runner; unlabeled Busy |
| **R5** G-ADR006 decision | 4h | Owner accept/reject/revise scoped selector ADR | Live same-scope dual managed without decision |
| **R6** HUX-3 live | 6h | Two exact runners; A disable ≠ mutate B | Cross-scope label drift |
| **R7** PR #65 merge + main CI | 2h | Owner accept merge; verify remote main CI | Auto-publish release |
| **R8** H3 / HUX-4 publish | 4h | Separate v0.1.0 and/or v0.2-A publication approvals | Combined silent release |
| **R9** Contingency | 4h | Only documented P0/P1 from live evidence | Speculative features |
| **TOTAL remaining ceiling** | **~48h** | Owner + assist; unused OK | |

## 3. Dependency DAG

```text
R0 (always safe, bounded)
  ├─► R1 G-RO ZenBookDuo          (independent)
  ├─► R2 Independent review       (required before R3 and/or R4)
  │     ├─► R3 H2 (G15R track)
  │     │     └─► dogfood ≥24h elapsed ▷ R8 v0.1 H3
  │     └─► R4 HUX-2
  │           └─► R5 G-ADR006 (may parallel R4 prep; required before multi-scope live admits)
  │                 └─► R6 HUX-3
  │                       └─► R7 merge #65 (Owner may merge earlier for source only — separate from live PASS)
  └─► R8 / R9 as authorized
```

R7 may occur before live HUX-2 **only** if Owner explicitly accepts merging Draft
source without claiming LIVE_DELIVERY=PASS.

## 4. Exit criteria

```text
PROGRAM_SOURCE_CLOSEOUT=PASS          # already
PROGRAM_LIVE_DELIVERY=PASS            # only when applicable R1–R6/R8 actually PASS
PROGRAM_LIVE_DELIVERY=BLOCKED_BY_OWNER  # honest if gates never issued
```

Never convert `WAITING_FOR_OWNER` / `PENDING` into PASS.

## 5. Related files

- Checklist: `goals/RM-160H-REMAINING-CHECKLIST.md`
- Prompt: `goals/RM-160H-AGY-RESUME-PROMPT.md`
- HUX-2: `goals/RM-HUX-2-READINESS-PACKAGE.md`, `goals/RM-HUX-2-OWNER-SERVICE-GATE-PLAN.md`
- H2/G17: `goals/RM-160H-P01-H2-OWNER-PREFLIGHT.md`, `goals/RM-160H-P01-G17-RELEASE-PREP.md`
- P04 decision: `goals/RM-160H-P04-SELECTOR-DECISION-PREP.md`
- Program DAG: `goals/RM-160H-DELIVERY-PROGRAM-001/ROADMAP.md`
