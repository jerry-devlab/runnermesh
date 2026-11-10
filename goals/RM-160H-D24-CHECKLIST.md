# RM-160H D24 checklist — next 24h Owner-assist

```text
PHASE=RM-160H-D24-OWNER-ASSIST
TIP=c3f939611b4028cf565ec1d441112467d0187e07
CI=42655292685
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
PRODUCTION_MUTATION=false
```

## D24-0 — coordinate sync

- [x] D24-0-01 Confirm `git rev-parse HEAD` == `c3f939611b4028cf565ec1d441112467d0187e07` (or newer Owner tip).
- [x] D24-0-02 Update Phase-R Checkpoint / Remaining ROADMAP+CHECKLIST / Progress Board / AGY prompt tips + CI `42655292685`.
- [x] D24-0-03 Refresh `goals/RM-HUX-2-READINESS-PACKAGE.md` candidate coords (no live auth claim).
- [x] D24-0-04 `public_audit` clean (LF, no trailing blanks, no absolute paths/secrets).
- [x] **EXIT** docs pushed on Draft #65 · CI green or in-flight

## D24-R2 — independent security review assist

- [x] D24-R2-01 Bind HEADs explicitly:
  - H2 track → `G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab`
  - HUX-2 track → PR #65 tip (`c3f939611b4028cf565ec1d441112467d0187e07`)
- [x] D24-R2-02 Publish privacy-safe **risk/diff map** (paths + risk vector + what is synthetic vs live-gated) in `goals/RM-160H-R2-SECURITY-REVIEW-PACKET.md`.
- [x] D24-R2-03 Reviewer intake: identity class field, bound SHA field, PASS/FAIL/BLOCKED template — **no secrets**.
- [ ] D24-R2-04 If Owner names external reviewer and returns signed result → record `INDEPENDENT_SECURITY_REVIEW=PASS|FAIL` bound to SHA.
- [x] D24-R2-05 Awaiting external reviewer result → `PENDING` + `R2=ASSIST_READY` (AGY self-review is not external PASS).
- [x] **EXIT** `R2=ASSIST_READY` · `INDEPENDENT_SECURITY_REVIEW=PENDING`

## D24-R5 — G-ADR006 Owner brief

- [x] D24-R5-01 Distill `RM-160H-P04-SELECTOR-DECISION-PREP.md` + draft ADR 0006 into ≤1 page Owner brief (`goals/RM-160H-R5-ADR006-OWNER-BRIEF.md`).
- [x] D24-R5-02 Offer exactly: `ACCEPTED` | `REJECTED` | `DEFER` with consequences for same-scope dual managed.
- [x] D24-R5-03 Do **not** write live labels or claim ADR accepted without Owner text.
- [x] **EXIT** `R5_BRIEF=READY` · `G-ADR006=DRAFT_NOT_ACCEPTED` (awaiting Owner decision)

## D24-HUX2 — HUX-2 prep refresh (no live)

- [x] D24-HUX2-01 Align readiness package + Owner service gate plan to tip.
- [x] D24-HUX2-02 Confirm synthetic matrix still cited as ≠ LIVE PASS.
- [x] D24-HUX2-03 List exact Owner token fields still private/unissued.
- [x] **EXIT** `HUX2_PREP=READY` · `HUX2_LIVE=WAITING_FOR_OWNER`

## D24-H2 — G15R H2/G17 prep refresh (no cutover)

- [x] D24-H2-01 Confirm G15R_RC resolves; do not rebuild; do not substitute #65 tip.
- [x] D24-H2-02 Refresh public H2 preflight + G17 prep pointers only.
- [x] D24-H2-03 Keep QUALIFICATION/RESTORE/DOGFOOD slots empty until Owner R3.
- [x] **EXIT** `H2_PREP=READY` · `H2=WAITING_FOR_OWNER`

## D24-R1b — ZenBookDuo standby / optional execute

- [x] D24-R1b-01 Keep `operator-card-hux1` verified.
- [ ] D24-R1b-02 **Only if Owner confirms physical ZenBookDuo (UX8406 dual-screen)** → run `discover-readonly`; record privacy-safe counts; disposition `PASS_NONEMPTY|EMPTY_OK|INCOMPLETE`.
- [x] D24-R1b-03 Duo hardware not confirmed in this turn → leave `HUX1_ZENBOOKDUO=PENDING_NOT_RUN` (do not use ZenbookS14 as substitute).
- [x] **EXIT** `HUX1_ZENBOOKDUO=PENDING_NOT_RUN`

## D24-HOLD / Owner interrupt

- [ ] If Owner issues `OWNER_GATE=R3|R4|R5|R6|R7|R8` mid-train → **stop D24 prep**, execute **only that gate**, then stop.
- [x] End of D24 assist preparation → `WAITING_FOR_OWNER` receipt; Progress Board updated.

## Standing board (must remain honest)

```text
HUX1_LOCAL_DEV_HOST=EMPTY_OK
HUX1_ZENBOOKDUO=PENDING_NOT_RUN
INDEPENDENT_SECURITY_REVIEW=PENDING
H2=WAITING_FOR_OWNER
HUX2_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
PRODUCTION_MUTATION=false
```
