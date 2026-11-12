# RM-160H remaining checklist — Owner closeout

Mark dispositions with evidence. Do not check a live box without Owner evidence.

```text
PHASE=RM-160H-PHASE-R-OWNER-CLOSEOUT
GOAL_START_MAIN=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
REMOTE_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
PR_BASE=feat/lrm-productization
PR_BASE_HEAD=8cd996acfee23e5f2006e7801139c70d0d67551d
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
SOURCE_CLOSEOUT=PASS
R0=PASS
R1_LOCAL=EMPTY_OK
R1_ZENBOOKDUO=PENDING_NOT_RUN
CANDIDATE_PR=65
PACKAGE_BUILD_COMMIT=2703a1e57bd3ced0c350ba8266c11ad08851abdb
CODE_ANCHOR=2703a1e57bd3ced0c350ba8266c11ad08851abdb
PR_HEAD=2703a1e57bd3ced0c350ba8266c11ad08851abdb
PR_CI=43161254344 (PASS_ALL 6/6 jobs)
```

## R0 — packet polish (safe, no live mutation)

- [x] R0-01 Re-fetch PR #65 tip, CI, and confirm `SOURCE_CLOSEOUT` coordinates match remote.
- [x] R0-02 Refresh public readiness docs to tip without absolute paths / secrets (`public_audit` clean).
- [x] R0-03 Confirm G15R RC still resolves; do not rebuild.
- [x] R0-04 Write/update `goals/RM-160H-PHASE-R-CHECKPOINT.md` with NEXT Owner gate.
- [x] **EXIT** `R0=PASS` · `PRODUCTION_MUTATION=false`

## R1 — G-RO / HUX-1 ZenBookDuo (Owner or Owner-supervised)

- [x] R1-01 Confirm host is ZenBookDuo (not development EMPTY_OK host).
  - Observed host is `ZenbookS14` (`ASUS Zenbook S 14 UX5406SA_UX5406SA`), which is the local development workstation, not physical dual-screen `ZenBookDuo`.
- [x] R1-02 Run `runnermesh-hux-sandbox discover-readonly` (and/or `operator-card-hux1`).
  - Executed on host `ZenbookS14`; operator card verified and available.
- [x] R1-03 Record only privacy-safe counts: candidate/observed/refused/identity_complete + enumeration statuses + incomplete.
  - `candidate_count=0`, `observed_only_count=0`, `refused_count=0`, `identity_complete_count=0`, `service_enumeration=Ok`, `session_enumeration=Ok`, `process_observation=Ok`, `incomplete=false`.
- [x] R1-04 Disposition one of: `PASS_NONEMPTY` | `EMPTY_OK` | `INCOMPLETE` | `PENDING_NOT_RUN`.
  - Local development host (`ZenbookS14`): `EMPTY_OK`.
  - Physical ZenBookDuo: `PENDING_NOT_RUN` (governance forbids substituting development host result for ZenBookDuo).
- [x] **EXIT** `HUX1_ZENBOOKDUO=PENDING_NOT_RUN` (`LOCAL_DEV_HOST=EMPTY_OK`) · never paste paths/IDs/tokens publicly

## R2 — independent security review

- [x] R2-01 Choose exact HEAD(s): Track 2 (LRM) Candidate `2703a1e57bd3ced0c350ba8266c11ad08851abdb` on PR #65; Track 1 `G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab` for H2 (separate track).
- [x] R2-02 Provide privacy-safe diff/risk map to external reviewer (Surfaces A-D documented in `RM-160H-R2-SECURITY-REVIEW-PACKET.md`).
- [x] R2-03 Record reviewer identity class + bound SHA: Admitted independent process (`jerry-auditor`) preflight passed (`STOP_REASON=NONE`). Formal review executed by Codex `jerry-auditor` on exact settled HEAD `2703a1e57bd3ced0c350ba8266c11ad08851abdb` (CI Run `43161254344`: 6/6 PASS). Diagnostic review verified Surface A HMAC-SHA256 and dispatch-scope findings addressed. Disposition: `BLOCKED` on audit environment admission failure (conda/Python unavailable in subshell, TLS UnknownIssuer on child admission). `BLOCKER_LATCHED=true`.
- [ ] **EXIT** `INDEPENDENT_SECURITY_REVIEW_LRM=BLOCKED` · `BLOCKER_LATCHED=true` · Track 1 G15R remains `PENDING` (never use PR #65 tip as RC)

## R3 — H2 v0.1 (G15R track only)

- [ ] R3-01 Preconditions: R2 PASS for G15R HEAD; new Owner token; private packet complete.
- [ ] R3-02 Execute bounded cutover on **G15R_RC only** per H2 preflight.
- [ ] R3-03 Record `QUALIFICATION` and `RESTORE` separately.
- [ ] R3-04 Start ≥24h ordinary-use dogfood clock only after H2+RESTORE PASS; no invented hours.
- [ ] R3-05 G17 evidence package after dogfood.
- [ ] **EXIT** H2/RESTORE/DOGFOOD/G17 each honest PASS or WAITING — never use PR #65 tip as RC

## R4 — HUX-2 live (disposable Service)

- [ ] R4-01 Preconditions: R2 PASS for candidate HEAD; readiness package fields complete (`goals/RM-HUX-2-READINESS-PACKAGE.md` updated); no Busy Worker.
- [ ] R4-02 Follow `RM-HUX-2-OWNER-SERVICE-GATE-PLAN.md` exactly.
- [ ] R4-03 Stop → readback → Start → restore; `RESTORE` separate disposition.
- [ ] R4-04 Never touch production dogfood runner.
- [ ] **EXIT** `HUX2_LIVE=PASS` + `RESTORE=PASS|FAIL` or `BLOCKED_BY_OWNER`

## R5 — G-ADR006 Owner architecture decision

- [ ] R5-01 Review `RM-160H-P04-SELECTOR-DECISION-PREP.md` + draft ADR 0006.
- [ ] R5-02 Owner disposition: `ACCEPTED` | `REJECTED` | `DEFER` with written rationale.
- [ ] R5-03 Until ACCEPTED: same-scope dual managed enrollment remains REFUSED; no live label writes for scoped scheme.
- [ ] **EXIT** `G-ADR006=<disposition>`

## R6 — HUX-3 live (two runners)

- [ ] R6-01 Preconditions: HUX2_LIVE PASS; routing decision compatible with R5; two exact enrolled runners (`goals/RM-HUX-3-READINESS-PACKAGE.md` prepared).
- [ ] R6-02 Disable A; prove B desired/Worker/service/selector/registry unchanged; B can finish legitimate CI.
- [ ] R6-03 Restart preserves manual intent; no cross-scope drift.
- [ ] **EXIT** `HUX3_LIVE=PASS` or `BLOCKED_BY_OWNER`

## R7 — PR #65 merge (source acceptance ≠ live PASS)

- [ ] R7-01 Owner explicit accept of Draft #65 scope.
- [ ] R7-02 Merge; verify remote `main` CI healthy before further merges.
- [ ] R7-03 Ledger update: decision-relevant only; no private host IDs.
- [ ] **EXIT** `PR65_MERGED=true` · still does not imply HUX2/H3/H2 PASS

## R8 — publication (H3 / HUX-4)

- [ ] R8-01 Separate approval for `v0.1.0` (G15R track) if pursued.
- [ ] R8-02 Separate approval for v0.2-A RC/publish if pursued.
- [ ] R8-03 Provenance digests bound to immutable artifacts, not worktree `target/`.
- [ ] **EXIT** published only when authorized; else `H3=WAITING_FOR_OWNER`

## R9 — contingency

- [ ] R9-01 Only documented P0/P1 from live evidence.
- [ ] R9-02 Unused reserve remains unused.

## Standing Owner gate board

```text
HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED
HUX1_LOCAL_DEV_HOST=EMPTY_OK
HUX1_ZENBOOKDUO=PENDING_NOT_RUN
H2=WAITING_FOR_OWNER
HUX2_LIVE=WAITING_FOR_OWNER
HUX3_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW_LRM=BLOCKED
INDEPENDENT_SECURITY_REVIEW_G15R=PENDING
PR65_MERGED=false
PRODUCTION_MUTATION=false
BLOCKER_LATCHED=true
```

## Receipt fields (Owner transaction)

```text
DISPOSITION=
PHASE_GATE=R0|R1|R2|R3|R4|R5|R6|R7|R8|R9
CANDIDATE_HEAD=
G15R_RC_OR_N/A=
QUALIFICATION=
RESTORE=
PRODUCTION_MUTATION=
OWNER_ACTION=
NEXT_SAFE_GOAL=
```
