# RunnerMesh v0.1 Execution Status

Status: **Authoritative privacy-safe execution ledger**

This file is the first status source for future RunnerMesh v0.1 agents. It
records public repository state only. Do not place private host paths,
usernames, runner IDs, private workflow IDs, credentials, or private topology
here.

Last roadmap reset: 2026-08-31 (Roadmap v3).

## State vocabulary

- `ACCEPTED` — merged into authoritative `main` with required evidence.
- `SUPERSEDED` — historical work is retained as evidence but is no longer the
  active path.
- `SALVAGE` — useful draft implementation exists but is not accepted as-is.
- `PREPARING` — bounded source/preflight/readiness work is in progress.
- `PREPARED` — all non-Owner prerequisites are complete and freshly verifiable.
- `WAITING_FOR_OWNER` — a prepared/resumable lane awaits fresh Owner presence or
  authorization; this is not a defect or terminal blocker.
- `OWNER_CANCELED` — one Owner authorization attempt ended; this is not an
  implementation failure and does not authorize reuse of its transaction data.
- `BLOCKED_PRECONDITION` — a required verified precondition is false or unknown.
- `BLOCKED_EXTERNAL` — an external trust/service/platform dependency prevents
  progress.
- `FAIL_PRE_MUTATION` — execution failed before external mutation began.
- `FAIL_POST_MUTATION` — execution failed after mutation began; restoration
  remains an independent mandatory outcome.
- `PASS` — the bounded result and required verification passed.
- `TODO` — not yet implemented or qualified.

Evidence-scope flags are not top-level result dispositions:

- `AUDIT_ADMISSION_FAILED` — the independent review process could not execute
  its harmless read-only child; this is infrastructure evidence, not an
  artifact failure.
- `DIAGNOSTIC_ONLY` — useful evidence from a normal read-only diagnostic
  process that is not independent acceptance.
- `INDEPENDENT_ACCEPTANCE_PENDING` — the active risk policy still requires a
  fresh independent result before the affected external authority can be used.

## Current ledger

| Goal / phase | State | Accepted head / candidate | PR | Durable evidence / note | Next prerequisite |
|---|---|---|---|---|---|
| Final autonomous closeout Master Goal | WAITING_FOR_OWNER | containing protected-main merge | #37, #38, containing ledger reconciliation PR | Phase 0 authority accepted on healthy protected main; historical P0 retry remains exhausted; fresh official H1 path changes target authority only | exact-plan `AUTHORIZE_MASTER_FRESH_H1_TARGET` |
| G01-G10 + G06R + G10R | ACCEPTED | historical accepted main | #1-#13 | frozen domain/runtime/product foundation | none |
| Windows no-tasklist hotfix | ACCEPTED | `b6dfdf92dae4e9ba20a2a4abc4e6ee26a356ab1b` | #18 | native ToolHelp process snapshot; console-flash regression closed | none |
| Roadmap v2 | SUPERSEDED | `2621548d685fde4a9910b675192de39ee791649f` | #19 | retained historical reset; Roadmap v3 governs remaining execution | Roadmap v3 |
| Roadmap v3 governance truth | ACCEPTED | `af0c14e63f070409c7c31f92c986aca5214ac379` | #29 | accepted health-audit conclusions; parallel source lane plus H1 merge gate | follow the eight execution blocks |
| Historical G11 qualification | SUPERSEDED | N/A | N/A | V3/V4/V4R/V4S remain private evidence, not active transaction variants | P0 supervised restore only |
| PR #17 bounded executor | SUPERSEDED | `a8a028e472ff1271003ee161b7307c3e70818b40` | #17 closed | exact-scope/no-signal evidence preserved by ADR 0004 and accepted G11R; run-once path obsolete | none |
| P0 supervised baseline restore | BLOCKED_EXTERNAL | `9501376ad1cddd3f0a29c9350f603bb5e9b8a60f` | N/A | F1's one fresh retry stopped at independent Auditor child-process admission; no live target observation or host mutation started; `P0_HISTORICAL_TARGET_RETRY_EXHAUSTED=true` | explicit Owner decision on a fresh official H1 qualification target; never automatically register or replace a runner |
| Independent Auditor admission | PREPARED | containing protected-main merge | containing implementation PR | read-only/never profile plus one-child admission preflight; packaged-shell resolution is scoped to the child environment; admission is not audit acceptance | reuse for bounded independent reviews; grant only the exact evidence scope each audit requires |
| GOV1 governance reset | ACCEPTED | PR #20 accepted | #20 | public ledger and PR-only governance foundation | none |
| Fast Lane v2 CI/audit automation | ACCEPTED | `03d2a1c64ccbe0c113d5cd6acd4127cd208dda2f` | #26 | conservative classification, deterministic audit, stable `CI Gate`, hosted Windows/Ubuntu coverage | keep full post-merge code main CI |
| DVP1 developer velocity pack | ACCEPTED | containing protected-main merge | containing implementation PR | anti-sleep train wrapper; docs-main fast path; exact-head candidate, wait, merge, and main-health helpers; compact ledger/receipt policy | productization salvage preparation |
| G11R-A admission architecture | ACCEPTED | `91cf656fde0b365fb97197c1bef93991a4f44c6e` | #21 | ADR 0004 selects exact-runner `runnermesh-admit` and two-phase withdrawal | none |
| G11R-B lifecycle implementation | ACCEPTED | `0c76e10f67d563f2dadc4914b5eefaa29a73d858` | #24 | exact-scope REST seam, desired/achieved state, drift refusal, no normal Worker signal | H1 live adapter source |
| G11R-C qualification contracts | ACCEPTED_SOURCE | accepted label-specific package | #25 | eleven-gate fail-closed verifier, inert workflow template, one H1 transaction family, synthetic restore proof | live adapters and Owner prerequisites |
| H1 live adapters/readiness source | ACCEPTED_SOURCE | containing protected-main merge | #30 + containing H1 preparation correction PR | fixed-authority transport, strict read-only private-artifact binding verification, frozen candidate/transaction workflow identities, step-scoped server-valid runner-context assertion, workflow/routing/readiness collectors, and privacy-safe ACL reset round-trip proof; exact-head CI Gate, focused trust review, and post-merge full main CI required; tests remain synthetic-only | Owner configuration and fresh live readiness |
| H1 one-shot qualification | PREPARING | accepted fresh-target authority; containing workflow correction merge | containing focused correction PR | dedicated private repository-scoped user-session target established; initial workflow dispatch was rejected before any H1 run or selector mutation because the runner context was placed at job scope; baseline independently remained advertised/online/idle, and the historical P0 target remains untouched | merge correction, recollect live readiness, and request fresh exact-plan `AUTHORIZE_MASTER_H1_QUALIFICATION` |
| G12 autostart | SALVAGE / SOURCE_PREP_ALLOWED | draft asset | #14 draft | selective extraction allowed; acceptance/merge held | H1 qualification PASS + restore PASS + target authority ACCEPTED |
| G13 versioned install | SALVAGE / SOURCE_PREP_ALLOWED | draft asset | #14 draft | selective extraction allowed; acceptance/merge held | H1 qualification PASS + restore PASS + target authority ACCEPTED |
| G14 update + rollback | SALVAGE / SOURCE_PREP_ALLOWED | draft asset | #15 draft | selective extraction allowed; acceptance/merge held | accepted G13 after H1 |
| G15 packaging + doctor | SALVAGE / SOURCE_PREP_ALLOWED | draft asset | #16 draft | selective extraction allowed; acceptance/merge held | accepted G14 after H1 |
| G15R integrated pre-H2 RC | TODO | N/A | N/A | exactly one authoritative-main sandbox-qualified RC | G12-G15 accepted |
| H2/G16-A real cutover | TODO | N/A | N/A | immutable RC only | G15R `H2_RC_READY=true` + Owner gate |
| H2/G16-B sustained dogfood | TODO | N/A | N/A | minimum 24h ordinary-use window | successful cutover |
| G17 RC closeout | TODO | N/A | N/A | exact candidate/evidence/docs/release closeout | sustained dogfood PASS |
| H3/G18 v0.1.0 release | TODO | N/A | N/A | stable publication requires Owner authorization | G17 PASS |

## Current public repository baseline

Roadmap v3 began from authoritative `main`:

```text
490852e219c74a0312e955e3eabeae02737bfc08
```

Future agents must refresh remote state rather than assuming this SHA remains
current.

## Governance status

Verified against the active repository ruleset on 2026-08-31:

```text
ROADMAP_V3=ACCEPTED
MAIN_PROTECTION=ENFORCED
MAIN_RULESET=protect-main
REQUIRED_CHECK=CI Gate
MAIN_BYPASS=NONE
DIRECT_PUSH_TO_MAIN=BLOCKED_BY_PULL_REQUEST_RULE
FORCE_PUSH_TO_MAIN=BLOCKED
MAIN_DELETION=BLOCKED
STRICT_REQUIRED_STATUS_CHECKS_POLICY=false
POST_MERGE_MAIN_CODE_CI=FULL_WINDOWS_UBUNTU
POST_MERGE_MAIN_DOCS_CI=LIGHTWEIGHT
```

Full Windows and Ubuntu CI remains enabled for code changes on `main` because
the required check does not require an up-to-date base. Docs-only main pushes
use the lightweight Fast Gate plus stable `CI Gate`. Do not merge a next PR
until the prior risk-appropriate post-merge `main` run is healthy.

## Source lane and Owner lane

```text
EXECUTION_MODEL=PARALLEL_SOURCE_PREP_WITH_H1_MERGE_GATE
P0_PRODUCT_BLOCKER=false
H1_TARGET_AUTHORITY_BLOCKER=false
H1_TARGET_AUTHORITY_WAITING_FOR_OWNER=false
P0_SOURCE_DEVELOPMENT_BLOCKER=false
H1_SHOULD_BLOCK_SOURCE_PREPARATION=false
G12_G15_SOURCE_PREPARATION_ALLOWED=true
H1_ENTRY_AUTHORITY=P0_PASS_OR_OWNER_RETIRED_HISTORICAL_P0_PLUS_FRESH_TARGET_SELECTED
G12_G15_ACCEPTANCE_REQUIRES_H1_QUALIFICATION_PASS=true
G12_G15_ACCEPTANCE_REQUIRES_H1_RESTORE_PASS=true
G12_G15_ACCEPTANCE_REQUIRES_H1_TARGET_AUTHORITY_ACCEPTED=true
```

P0/H1 waiting does not automatically block safe source-only work. G12-G15 may
be selectively extracted, corrected, tested, and prepared as focused drafts.
They remain unaccepted and unmergeable as product milestones until H1
qualification, restoration, and target authority all pass.

## Current admission decision

```text
G11R_A=ACCEPTED
ADMISSION_ARCHITECTURE=GITHUB_NATIVE_DYNAMIC_ADMISSION_LABEL
RESERVED_ADMISSION_LABEL=runnermesh-admit
WITHDRAWAL_PROTOCOL=TWO_PHASE
SCHEDULER_LINEARIZABILITY=NOT_CLAIMED_WITHOUT_UPSTREAM_GUARANTEE
SEMANTIC_WEAKENING=false
G11R_B=ACCEPTED
G11R_C=ACCEPTED_SOURCE
H1_READINESS_VERIFIER=PASS_SYNTHETIC
H1_LIVE_ADAPTER_SOURCE_READY=true
H1_NON_MUTATING_READINESS=INVALIDATED_PENDING_WORKFLOW_CORRECTION
H1_OWNER_BUNDLE_SOURCE_PREPARED=true
MASTER_GOAL_PHASE=H1_WORKFLOW_CORRECTION
MASTER_GOAL_STATE=PREPARING
P0_HISTORICAL_TARGET_RETRY_EXHAUSTED=true
HISTORICAL_P0_V0_1_AUTHORITY=RETIRED_BY_OWNER
FRESH_OFFICIAL_H1_TARGET_SELECTED=true
H1_TARGET_AUTHORITY=ACCEPTED
H1_MUTATION_ALLOWED=false
LIVE_READINESS_EXECUTED=true
H1_LIVE_READINESS=INVALIDATED_BY_WORKFLOW_SERVICE_VALIDATION
H1_EXECUTED=false
```

Unknown identity, selector ownership, routing, workflow, credential, local
binding, rollback, recovery, or Owner evidence fails closed.

## Remaining security debt

```text
PRIVATE_EVIDENCE_ACL_HARDENING=REQUIRED_BEFORE_H1_LIVE_ARTIFACTS_OR_OWNER_GATE
ACTIONS_CHECKOUT_IMMUTABLE_PINNING=RESOLVED_AT_H1_SOURCE
ACTIONS_CHECKOUT_PIN=3d3c42e5aac5ba805825da76410c181273ba90b1
NAMED_PIPE_EXPLICIT_DACL=REQUIRED_BEFORE_RELEASE
```

The ACL change and real credential/workflow/runner configuration are separate
Owner actions. Public docs and fixtures contain no private identities.

## Update rule

After every accepted Goal/PR merge or material blocker change, update only
decision-relevant rows. `WAITING_FOR_OWNER` is resumable control state, not a
failure. Never reuse an old nonce, transaction, handoff, preflight, or Owner
authorization. After mutation, report qualification and restoration
independently.
