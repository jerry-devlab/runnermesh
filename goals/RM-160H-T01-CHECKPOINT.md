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
development-host zero-runner observation to ZenBookDuo PASS.

160H ROADMAP/CHECKLIST/CURSOR-EXECUTION already live on `main` via PR #64.

## P02 progress (this train)

- Path-boundary process attribution under runner home (reject sibling prefix).
- Require Listener/Worker under `bin\` layout when matching by name.
- Align `QueryServiceConfigW` buffer for Service discovery facts.
- Parse `lpBinaryPathName` quotes/args correctly (fail closed on empty/unclosed).
- Service-account opaque identity hashing; empty start account fails closed.
- Incomplete process / partial service enumeration coverage (no false-empty).
- Discover candidate views + Session Busy/Unknown adopt refusals (earlier).
- Gap map: `goals/RM-160H-T01-CANDIDATE-GAP-MAP.md`

## GT-02 prep started (no live gates)

- `goals/RM-160H-P01-G17-RELEASE-PREP.md` (WAITING_FOR_OWNER)
- P01 H2 preflight points at G17 checklist
- P03 trusted SCM auth design already on tip
- `goals/RM-160H-P04-SELECTOR-DECISION-PREP.md` (ADR 0006 still Draft)

## Remaining / next

```text
COMPLETED_GOALS=P00-partial(PENDING ZenBookDuo); P02-01..05_SOURCE_PASS; P02-06_PENDING_ZenBookDuo; P01_G17_prep_doc; P03_design_doc; P04_decision_prep_doc
CANDIDATE_PR=65
REMOTE_HEAD=see_branch_tip
NOTE=git HTTPS to github.com:443 flaky; tip advanced via GitHub Git Data API with LF-normalized blobs
NEXT_SAFE_GOAL=GT-02 P04-05 FakeGithub Busy/Unknown matrix source OR remaining P01 packet polish; no live gates
BUDGET_USED≈estimate_6.0h
BUDGET_LEFT≈10.0h_of_GT01
CI_STATUS=awaiting_checks_after_privacy_fix
WIP_STATUS=clean_except_untracked_goals/RM-160H-DELIVERY-PROGRAM-001_identical_to_main
OWNER_GATES=HUX1_ZENBOOKDUO; H2; HUX2_LIVE; TRUSTED_SCM; INDEPENDENT_SECURITY_REVIEW; G-ADR006
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline; prefer gh Git Data API if git HTTPS fails; never embed absolute workstation roots in public checkpoints; do not touch feat/lrm-09 or overwrite productization tip worktree
```
