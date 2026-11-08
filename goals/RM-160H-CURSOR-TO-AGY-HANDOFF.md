# Cursor → AGY handoff (160H Goal Train)

`PRODUCTION_MUTATION=false` · SOURCE_AND_SANDBOX_ONLY · privacy-safe

Cursor Implementer session ending by Owner request (Cursor quota). Continue with AGY CLI on the same worktree. Do **not** reopen path/worktree choice.

## Authoritative coordinates

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T02-V01-CLOSEOUT-PREP
PRIOR_TRAIN=RM-160H-T01-FIELD-BASELINE
WORKTREE_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=https://github.com/jerry-devlab/runnermesh/pull/65
REMOTE_TIP=5271d5b3d4705c2d9b0be58716ce52d273e37db7
LAST_CODE_PASS_ALL=90740eef1250a2ce0ec5aba39b9738ba0ad15dcd
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
```

`5271d5b` is checkpoint/docs on top of code tip `90740ee` (`PASS_ALL`). Confirm hosted CI on `5271d5b` before merging anything; Draft #65 must not merge into frozen v0.1 RC without Owner.

## Do not touch

```text
feat/lrm-09-reconcile-semantic-fixes
runnermesh-lrm-product / productization tip worktree overwrite
reset --hard / clean -fd / force checkout / delete other worktrees
```

Untracked `goals/RM-160H-DELIVERY-PROGRAM-001/` equals main (PR #64); do not re-commit unless it diverges.

## Host truth

```text
HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED
HUX1_NONEMPTY_LOCAL_DEV_HOST=EMPTY_OK
HUX1_ZENBOOKDUO=PENDING
```

Use `runnermesh-hux-sandbox operator-card-hux1` / `discover-readonly` only on ZenBookDuo. Never promote development-host empty to ZenBookDuo PASS.

## GT-01 source disposition (near-complete)

| Package | Disposition |
| --- | --- |
| P02 discovery identity / enrollment | PASS_SOURCE |
| P04-05 FakeGithub matrix (lag/stale/permission/link-loss/race/scope-drift/racing-assignment) | PASS_SOURCE |
| P05 multi-runner precedence / A/B / restart refuse / stale PID | PASS_SOURCE |
| P06 partial-batch / Tray i18n / CLI privacy / packaging sandbox | PASS_SOURCE |
| P03 synthetic≠live; stop/start readback Interrupted ≠ restore PASS | PASS_SOURCE |
| P01 G15R reverify `fdd9fe6b…` + run `42452639359` | PASS_READ_ONLY |
| P04 ADR 0006 | DRAFT_NOT_ACCEPTED |

Key local files:

- `goals/RM-160H-T01-CHECKPOINT.md`
- `goals/RM-160H-T02-CHECKPOINT.md`
- `goals/RM-160H-T01-CANDIDATE-GAP-MAP.md`
- `goals/RM-160H-P01-H2-OWNER-PREFLIGHT.md`
- `goals/RM-160H-P01-G17-RELEASE-PREP.md`
- `goals/RM-160H-P04-SELECTOR-DECISION-PREP.md`
- `goals/RM-160H-AGY-RESUME-PROMPT.md` ← paste into AGY

## Push / CI notes

- `git push` HTTPS may fail; prefer GitHub Git Data API (blob → tree → commit → ref PATCH).
- Always LF-normalize; reject empty blob `e69de29…`; trim trailing blank lines (public_audit whitespace).
- rustfmt: `1.98.1-x86_64-pc-windows-msvc`.

## Owner gates (still blocked)

```text
HUX1_ZENBOOKDUO
H2 (fresh token + independent security review)
HUX2_LIVE / TRUSTED_SCM
G-ADR006
H3 / publication
```

```text
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY
```

No real SCM start/stop/ChangeConfig, UAC, label/registration mutation, production install, H2 cutover, HUX-2/3 live, or release publish without separate Owner authorization.

## Next safe goals for AGY

1. Confirm CI on tip `5271d5b` (or settle next tip) — reuse PASS on `90740ee` code risk if docs-only delta empty for code gates.
2. GT-02: P01 Owner-packet polish (public-safe only); keep live WAITING_FOR_OWNER.
3. Residual unblocked P03/P04/P06 source only; no inventing HUX-1 nonempty.
4. Stop at Owner gates; write checkpoint every 60–90 minutes.

## Session end

```text
CURSOR_SESSION=STOPPED_BY_OWNER_QUOTA
HANDOFF_TO=AGY_CLI
IMPLEMENTER_WRITER=transfer_to_AGY_on_same_worktree
```
