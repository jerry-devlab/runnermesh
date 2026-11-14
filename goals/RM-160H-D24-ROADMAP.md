# RM-160H D24 roadmap — next 24h Owner-assist (no invented features)

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
PHASE=RM-160H-D24-OWNER-ASSIST
BUDGET=24h
CANDIDATE_PR=65
REMOTE_TIP=c3f939611b4028cf565ec1d441112467d0187e07
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
HOSTED_CI_RUN=42655292685
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true_until_exact_owner_gate
PRIOR=R0=PASS · R1_LOCAL=EMPTY_OK · R1_ZENBOOKDUO=PENDING_NOT_RUN
```

## Intent

Fill the next ≤24h with **Owner-assist packets** that unblock live gates when
authorized. Do **not** invent P02–P08 features, Mesh/Linux/enforcement, or claim
`INDEPENDENT_SECURITY_REVIEW=PASS` from AGY self-review.

## Day trains (ordered)

| ID | Cap | Work | Exit |
|---|---:|---|---|
| **D24-0** Coord sync | 1h | Tip/CI/`PROGRESS-BOARD`/`PHASE-R` coords → `c3f9396` + `42655292685`; public_audit | docs commit on #65 |
| **D24-R2** Security review assist | 8h | Exact-HEAD risk/diff map for G15R **and** #65 tip; reviewer intake checklist; **PASS only if external reviewer signs** | `R2=ASSIST_READY` or `R2=PASS` |
| **D24-R5** ADR006 Owner brief | 3h | 1-page decision brief from selector prep + draft ADR; options ACCEPTED/REJECTED/DEFER | `R5_BRIEF=READY` |
| **D24-HUX2** Live prep refresh | 4h | Refresh HUX-2 readiness + Owner service gate plan fields to tip; no SCM write | `HUX2_PREP=READY` |
| **D24-H2** G15R preflight refresh | 3h | Refresh H2/G17 public preflight to G15R_RC; never use #65 as RC | `H2_PREP=READY` |
| **D24-R1b** ZenBookDuo standby | 2h | Operator-card + runbook only; execute discover **only if Owner confirms physical Duo host** | `PENDING_NOT_RUN` or Duo disposition |
| **D24-HOLD** | rem. | If Owner issues R3/R4/R5/R7 mid-day → **switch to that single gate**; else stop | `WAITING_FOR_OWNER` |

## Hard stops

- No SCM Start/Stop/ChangeConfig, UAC, registration, label writes, production install.
- No H2 cutover / HUX-2/3 live without exact `OWNER_GATE=R3|R4|R6` text.
- No treating FakeGithub / Mock SCM / local EMPTY_OK as live PASS.
- No overwrite of `feat/lrm-09-*` or productization worktree; preserve untracked `goals/RM-160H-DELIVERY-PROGRAM-001/`.

## Dependency

```text
D24-0
  ├─► D24-R2 (priority — unlocks R3/R4)
  ├─► D24-R5 (paper; parallel OK)
  ├─► D24-HUX2 + D24-H2 (prep only; parallel OK)
  └─► D24-R1b (execute only on physical ZenBookDuo)
        └─► D24-HOLD / Owner-issued live gate
```

## Related

- Checklist: `goals/RM-160H-D24-CHECKLIST.md`
- Progress: `goals/RM-160H-PROGRESS-BOARD.md`
- Phase-R: `goals/RM-160H-REMAINING-ROADMAP.md`, `goals/RM-160H-REMAINING-CHECKLIST.md`
