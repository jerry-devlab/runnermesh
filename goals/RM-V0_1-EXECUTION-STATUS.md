# RunnerMesh v0.1 Execution Status

Status: **Authoritative privacy-safe execution ledger**

This file is the first status source for future RunnerMesh v0.1 agents. It records public repository state only. Do not place private host paths, usernames, runner IDs, private workflow IDs, credentials, or private topology here.

Last roadmap reset: 2026-08-30.

## State vocabulary

- `ACCEPTED` — merged into authoritative `main` with required evidence.
- `SUPERSEDED` — historical work retained as evidence but no longer the active implementation path.
- `REDESIGN` — product goal remains; mechanism must be reselected/reworked.
- `SALVAGE` — useful draft implementation exists but is not accepted as-is.
- `READY` — prerequisites are satisfied for the next defined gate.
- `BLOCKED` — an external/trust/precondition boundary prevents progress.
- `TODO` — not yet implemented/qualified.

## Current ledger

| Goal / phase | State | Accepted head / candidate | PR | Durable evidence / note | Next prerequisite |
|---|---|---|---|---|---|
| G01 Domain foundation | ACCEPTED | historical main | #1 | NodeState/UserMode contracts | none |
| Design freeze / roadmap v1 | ACCEPTED historical | historical main | #2 | product contract remains authoritative except explicit ADR changes | roadmap v2 governs remaining execution |
| G02 Runtime contracts | ACCEPTED | historical main | #3 | runtime/control vocabulary | none |
| G03 Agent Core | ACCEPTED | historical main | #4 | Observe/Decide/Reconcile + persistent intent | none |
| G04 Local IPC | ACCEPTED | historical main | #5 | user-local Named Pipe + single Agent authority | none |
| G05 CLI | ACCEPTED | historical main | #6 | typed status/control/doctor/version surfaces | none |
| G06 Tray contracts | ACCEPTED | historical main | #7 | presentation contract | G06R later proved native runtime |
| G07 Probes + Auto Lite | ACCEPTED | historical main | #8 | User Activity / Steam / Process List + conservative policy | none |
| G08 Runner observer | ACCEPTED | historical main | #9 | read-only official runner observation | none |
| G09 Supervisor core | ACCEPTED | historical main | #10 | synthetic lifecycle abstractions | none |
| G10 Host + recovery model | ACCEPTED | historical main | #11 | host observation + reconstruction model | none |
| G06R Native tray/runtime | ACCEPTED | historical main | #12 | ordinary-user persistent Agent + native tray | none |
| G10R Pre-H1 integration | ACCEPTED | `2078c22bc9c8a2c409b651923ccca76ae3b2af45` | #13 | pre-H1 runtime readiness; no real runner control | none |
| Windows no-tasklist hotfix | ACCEPTED | `b6dfdf92dae4e9ba20a2a4abc4e6ee26a356ab1b` | #18 | native ToolHelp process snapshot; console-flash regression closed | none |
| Historical G11 qualification | SUPERSEDED | N/A | N/A | V3/V4/V4R/V4S retained as private evidence; do not proliferate variants | P0 recovery-only closeout + G11R-A |
| PR #17 bounded executor | SALVAGE / REDESIGN | `a8a028e472ff1271003ee161b7307c3e70818b40` | #17 draft | Busy drain/no-signal and exact scoping are useful; idle withdrawal remains unproven | G11R-A decision, then G11R-B |
| P0 Recovery-only closeout | BLOCKED / OWNER GATE | N/A | N/A | historical terminal experiment must return to known-good baseline; no qualification continuation | narrow Owner recovery transaction |
| GOV1 Governance reset | TODO | N/A | N/A | ledger/roadmap docs plus recommended main protection | docs first; GitHub settings remain Owner action |
| G11R-A Admission architecture | TODO | N/A | N/A | choose/define linearization point without preselecting `--once` | P0 may remain independent; no real-host mutation required for design |
| G11R-B Lifecycle implementation | TODO | N/A | N/A | salvage/refactor/supersede PR #17 on accepted ADR | G11R-A accepted ADR |
| G11R-C Qualification readiness | TODO | N/A | N/A | routing/workflows/recovery/readiness complete before host mutation | G11R-B source candidate |
| H1 One-shot qualification | TODO | N/A | N/A | one prepared real qualification with automatic restore attempt | G11R-C `OWNER_GATE_READY=true` |
| G12 Autostart | SALVAGE | draft implementation asset | #14 draft | old PR combines G12+G13; do not merge as-is | H1 PASS; extract clean G12 |
| G13 Versioned install | SALVAGE | draft implementation asset | #14 draft | immutable-slot/install concepts reusable | H1 PASS; extract clean G13 |
| G14 Update + rollback | SALVAGE | draft implementation asset | #15 draft | durable update/rollback implementation asset | accepted G13 baseline |
| G15 Packaging + doctor | SALVAGE | draft implementation asset | #16 draft | package/provenance/doctor implementation asset | accepted G14 baseline |
| G15R Integrated pre-H2 RC | TODO | N/A | N/A | exactly one authoritative-main sandbox-qualified RC | G12-G15 accepted |
| H2/G16-A Real cutover | TODO | N/A | N/A | immutable RC only | G15R `H2_RC_READY=true` + Owner gate |
| H2/G16-B Sustained dogfood | TODO | N/A | N/A | minimum 24h ordinary-use window | successful cutover |
| G17 RC closeout | TODO | N/A | N/A | exact candidate/evidence/docs/release-note closeout | sustained dogfood PASS |
| H3/G18 v0.1.0 release | TODO | N/A | N/A | stable publication requires Owner authorization | G17 PASS |

## Current public repository baseline

At the roadmap-v2 reset, authoritative `main` was:

```text
b6dfdf92dae4e9ba20a2a4abc4e6ee26a356ab1b
```

Future agents must refresh remote state rather than assuming this SHA remains current.

## Current architectural blocker

The remaining lifecycle design must define a truthful linearization point for capacity withdrawal. `Busy -> Drain` active-job preservation is distinct from `Listening -> Drain` idle admission withdrawal. A successful test of only the former is not sufficient evidence for the v0.1 product contract.

Do not optimize qualification infrastructure for a PASS before G11R-A chooses the product mechanism.

## Update rule

After every accepted Goal/PR merge or material blocker change, update only decision-relevant rows. Do not rewrite historical accepted rows merely for formatting. Private evidence may be referenced generically (for example, `private H1 receipt`) but never copied into this public ledger.