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
- `PROTOTYPE` — source-only framework or model exists in a draft, but its
  prerequisite Goal is not accepted and it is not product/readiness acceptance.
- `OWNER_ACTION_PENDING` — autonomous source/docs work is accepted, but a separate
  repository, trust, privilege, or production setting still requires the Owner.
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
| Roadmap v2 | ACCEPTED | `2621548d685fde4a9910b675192de39ee791649f` | #19 merged | authoritative remaining v0.1 sequence and historical G11 supersession | GOV1 source/docs closeout |
| Historical G11 qualification | SUPERSEDED | N/A | N/A | V3/V4/V4R/V4S retained as private evidence; do not proliferate variants | P0 recovery-only closeout + G11R-A |
| PR #17 bounded executor | SALVAGE / REDESIGN | `a8a028e472ff1271003ee161b7307c3e70818b40` | #17 draft | Busy drain/no-signal and exact scoping are useful; idle withdrawal remains unproven | G11R-A decision, then G11R-B |
| P0 Recovery-only closeout | BLOCKED / OWNER GATE | N/A | N/A | historical terminal experiment must return to known-good baseline; no qualification continuation | narrow Owner recovery transaction |
| GOV1 Governance reset | ACCEPTED / OWNER_ACTION_PENDING | PR #20 candidate | #20 | public ledger/roadmap/status references accepted; main protection recommendation documented | Owner applies machine-enforced main protection; autonomous work self-enforces PR-only flow |
| G11R-A Admission architecture | ACCEPTED | `91cf656fde0b365fb97197c1bef93991a4f44c6e` | #21 merged | ADR 0004 selects `runnermesh-admit` plus two-phase withdrawal; hosted Windows and Ubuntu exact-head CI passed; trust expansion accepted without semantic weakening | G11R-B |
| G11R-B Lifecycle implementation | READY | focused candidate on accepted G11R-A main | pending | typed exact-runner reserved-label control; desired/achieved states; no normal Worker signal; synthetic gates pass locally | exact-head hosted CI and focused merge |
| G11R-C Qualification readiness | PROTOTYPE / BLOCKED | `3f50af33b3e5b40d67ad82e7f39786f5e382d609` | #22 draft | mechanism-neutral prototype is salvage-only; no real adapters or private workflow | accepted G11R-B, then label-specific readiness adaptation |
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
2621548d685fde4a9910b675192de39ee791649f
```

Future agents must refresh remote state rather than assuming this SHA remains current.

## Governance status

```text
ROADMAP_V2=ACCEPTED
ROADMAP_V2_PR=19
ROADMAP_V2_PR_STATE=MERGED
GOV1_DOCS=ACCEPTED
MAIN_PROTECTION=OWNER_ACTION_PENDING
```

As verified on 2026-08-30, `main` had neither branch protection nor a repository
ruleset. Until the Owner applies the documented recommendation, autonomous work
must self-enforce focused branch -> PR -> exact-head hosted CI -> merge and must
never update `main` directly.

## Current admission decision

The Owner accepted GitHub-native dynamic admission through the unique reserved
label `runnermesh-admit` with two-phase withdrawal. Desired withdrawal starts at
accepted local policy intent. Achieved `DRAINED` requires selector absence, no
exact bound Worker, and consistent evidence. Mutation/readback is not described
as a globally linearizable scheduler barrier; a racing assignment is treated as
in-flight and may finish naturally.

```text
G11R_A=ACCEPTED
ADMISSION_ARCHITECTURE=GITHUB_NATIVE_DYNAMIC_ADMISSION_LABEL
RESERVED_ADMISSION_LABEL=runnermesh-admit
WITHDRAWAL_PROTOCOL=TWO_PHASE
SCHEDULER_LINEARIZABILITY=NOT_CLAIMED_WITHOUT_UPSTREAM_GUARANTEE
LOCAL_PERSISTENT_IDLE_WITHDRAWAL=UNPROVEN
RUN_ONCE_IDLE_WITHDRAWAL=UNPROVEN
DESIGN_FREEZE_CHANGE=TRUST_BOUNDARY_EXPANSION_PLUS_SEMANTIC_CLARIFICATION
SEMANTIC_WEAKENING=FALSE
G11R_B=CANDIDATE_LOCAL_GATES_PASS
G11R_C=PROTOTYPE_ONLY
H1=NOT_READY
```

PR #21 contains the accepted ADR, formal model, workflow contract, and PR #17
salvage map. Draft PR #22 remains only a mechanism-neutral readiness/restore
prototype until G11R-B is accepted. No source-development phase authorizes real
host, runner, registration, label/group, work-root, or H1 mutation.
## Update rule

After every accepted Goal/PR merge or material blocker change, update only decision-relevant rows. Do not rewrite historical accepted rows merely for formatting. Private evidence may be referenced generically (for example, `private H1 receipt`) but never copied into this public ledger.
