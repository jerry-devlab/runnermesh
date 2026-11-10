# AGY/Cursor → Codex handoff (160H paused → Codex resumes)

`PRODUCTION_MUTATION=false` · Owner paused active development · Codex is next sole Implementer when work resumes.

Do **not** reopen path/worktree choice. Do **not** invent new P02–P08 feature leaves, Mesh, Linux, Broker, or enforcement Job Objects.

## Authoritative coordinates (git-verified)

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
PHASE=RM-160H-PHASE-R-OWNER-CLOSEOUT
LAST_TRAIN=D24-OWNER-ASSIST
LAST_DISPOSITION=D24_ASSIST_READY / WAITING_FOR_OWNER
WORKTREE_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=https://github.com/jerry-devlab/runnermesh/pull/65
REMOTE_TIP=ea0d9a36000d5c993c9e7c5d5c46450bdd1f87b8
HOSTED_CI_RUN=42743583126
CI_STATUS=PASS_ALL (6/6)
CODE_ANCHOR=b221eaf72336c5caac676b10405c06e8cdd90b06
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
PR_BASE=feat/lrm-productization
PR_BASE_HEAD=8cd996acfee23e5f2006e7801139c70d0d67551d (Draft #63)
SOURCE_AND_SANDBOX_CLOSEOUT=PASS
LIVE_DELIVERY=WAITING_FOR_OWNER
PROGRAM_COMPOSITE_PCT≈80
LIVE_OWNER_CLOSEOUT_PCT≈20
PRIOR_IMPLEMENTERS=Cursor then AGY
NEXT_IMPLEMENTER=Codex
```

These coordinates record the verified remote candidate before the Codex handoff commit. Always prefer `git rev-parse HEAD` over any receipt that truncates/mangles the full SHA.

## Do not touch

```text
feat/lrm-09-reconcile-semantic-fixes
runnermesh-lrm-product / productization tip worktree overwrite
reset --hard / clean -fd / force checkout / delete other worktrees
goals/RM-160H-DELIVERY-PROGRAM-001/   # untracked; preserve; do not re-commit unless diverged
```

## Progress truth

| Slice | % | Note |
|---|---:|---|
| SOURCE_AND_SANDBOX | 100% | GT-01…GT-07; P02–P06/P08 PASS_SOURCE; P07 PREPARED; P01 preflight READY |
| LIVE_OWNER_CLOSEOUT | ≈20% | R0 PASS; R1 local EMPTY_OK; R2 ASSIST_READY; R5 BRIEF_READY |
| PROGRAM_COMPOSITE | ≈80% | 0.75×source + 0.25×live |

### Phase-R board

```text
R0=PASS
R1_LOCAL_DEV_HOST=EMPTY_OK
R1_ZENBOOKDUO=PENDING_NOT_RUN
R2=ASSIST_READY (INDEPENDENT_SECURITY_REVIEW=PENDING)
R3=WAITING_FOR_OWNER (needs R2 on G15R)
R4=WAITING_FOR_OWNER (needs R2 on #65)
R5=BRIEF_READY (G-ADR006=DRAFT_NOT_ACCEPTED)
R6=WAITING_FOR_OWNER
R7=OPEN_DRAFT (#65 not merged)
R8=WAITING_FOR_OWNER
R9=UNUSED
```

### Standing Owner gate board

```text
HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED
HUX1_LOCAL_DEV_HOST=EMPTY_OK
HUX1_ZENBOOKDUO=PENDING_NOT_RUN
H2=WAITING_FOR_OWNER
HUX2_LIVE=WAITING_FOR_OWNER
HUX3_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
PR65_MERGED=false
PRODUCTION_MUTATION=false
```

## Two tracks (never cross)

| Track | Artifact | Use for |
|---|---|---|
| v0.1 C0 | **G15R_RC only** `fdd9fe6b…` | H2 / dogfood / G17 / H3 |
| v0.2-A LRM | PR #65 tip (now `ea0d9a3…`) / CODE_ANCHOR `b221eaf…` | HUX-2/3, merge, later RC |

H2 must **never** cut over on PR #65 tip.

## Must-read files (in order)

1. `AGENTS.md`
2. `goals/RM-160H-PROGRESS-BOARD.md`
3. `goals/RM-160H-REMAINING-ROADMAP.md`
4. `goals/RM-160H-REMAINING-CHECKLIST.md`
5. `goals/RM-160H-PHASE-R-CHECKPOINT.md`
6. `goals/RM-160H-D24-ROADMAP.md` + `goals/RM-160H-D24-CHECKLIST.md` (D24 complete)
7. `goals/RM-160H-R2-SECURITY-REVIEW-PACKET.md`
8. `goals/RM-160H-R5-ADR006-OWNER-BRIEF.md`
9. `goals/RM-HUX-2-READINESS-PACKAGE.md` + `goals/RM-HUX-2-OWNER-SERVICE-GATE-PLAN.md`
10. `goals/RM-160H-P01-H2-OWNER-PREFLIGHT.md` + `goals/RM-160H-P01-G17-RELEASE-PREP.md`
11. `docs/v0.1-design-freeze.md` (frozen product contract)

## What Codex should do on resume

1. `git fetch` + confirm tip/CI; sync any stale Checkpoint tip fields to `ea0d9a3…` / `42743583126` if still lagging.
2. **Default:** remain `WAITING_FOR_OWNER` — no live mutation, no new feature leaves.
3. When Owner names **exactly one** gate, execute only that gate:
   - `R2` — external review result recording (Codex self-review ≠ PASS)
   - `R5` — record Owner ACCEPTED|REJECTED|DEFER
   - `R1b` — ZenBookDuo discover-readonly only on physical UX8406
   - `R3` — H2 on G15R_RC only (requires R2); QUALIFICATION/RESTORE separate
   - `R4` — HUX-2 disposable Service (requires R2); never dogfood
   - `R6` / `R7` / `R8` — only with explicit Owner text
4. After each gate: receipt → update Progress Board + Checklist + Checkpoint → stop.
5. Public docs: LF, no trailing blanks, no absolute paths/secrets (`public_audit`).

PR #65 currently targets the productization branch. A merge into that base
does not update protected main. R7 requires an Owner-approved merge path.

## Push / CI notes

- Prefer HTTPS `git push`; if flaky, GitHub Git Data API (blob→tree→commit→ref PATCH).
- Reject empty blob `e69de29…`; rustfmt toolchain `1.98.1-x86_64-pc-windows-msvc`.
- Hosted CI is final candidate evidence; keep Draft #65 until Owner merge.

## Writer model

Exactly one Implementer writer on this worktree/branch. Cursor/AGY are paused. Codex owns git writes for this Goal until Owner reassigns.
