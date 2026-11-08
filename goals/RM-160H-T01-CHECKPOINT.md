# RM-160H-T01-FIELD-BASELINE checkpoint

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T01-FIELD-BASELINE
ACTIVE_BUDGET=16h
WORKTREE_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
PRODUCTION_MUTATION=false
```

## P00 host / evidence boundary

```text
HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED
HUX0_PRODUCTION_MUTATION=false
HUX1_NONEMPTY_LOCAL_DEV_HOST=EMPTY_OK
HUX1_ZENBOOKDUO=PENDING
HUX1_COLLECTION=goals/RM-HUX-1-ZENBOOKDUO-READONLY-CHECKLIST.md
```

ZenBookDuo was not reachable from this Cursor session. Do not promote the
development-host zero-runner observation to ZenBookDuo PASS. Operator card for
real host RO discovery remains the existing HUX-1 checklist privacy-safe slot.

160H ROADMAP/CHECKLIST/CURSOR-EXECUTION already live on `main` via PR #64.
Local copies matched main hash; not re-committed on this tip.

## P02 progress (this train)

- Path-boundary process attribution under runner home (reject sibling prefix).
- Require Listener/Worker under `bin\` layout when matching by name.
- Align `QueryServiceConfigW` buffer for Service discovery facts.
- Unit coverage for sibling/prefix rejection and bin-layout suffix.

## Remaining / next

```text
COMPLETED_GOALS=P00-partial(PENDING ZenBookDuo); P02-path-boundary+session-adopt+discover-views; P01-preflight-doc
CANDIDATE_PR=65
# HEAD bound by hosted CI after push; avoid self-embedding this file's commit.
NEXT_SAFE_GOAL=GT-02 P01 remaining Owner packet polish OR P03 trusted-auth design docs; no live gates
BUDGET_USED≈estimate_3.5h
BUDGET_LEFT≈12.5h_of_GT01
CI_STATUS=awaiting_after_discover_views_push
OWNER_GATES=HUX1_ZENBOOKDUO; H2; HUX2_LIVE; TRUSTED_SCM
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline; do not touch feat/lrm-09 or overwrite productization tip worktree
```
