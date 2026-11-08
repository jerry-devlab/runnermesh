# HUX-2 Owner Gate — Independent test Runner Service control

Status: **PREPARE_ONLY** · `PRODUCTION_MUTATION=false` until explicit Owner authorization

This plan does **not** authorize live Service mutation. Synthetic qualification
(`hux2_synthetic`) is a separate evidence class and never promotes to LIVE PASS.

## Purpose

Qualify exact Windows Service start/stop for **one disposable test Runner** that
is not the production dogfood identity, after productization source acceptance
and independent security review.

## Preconditions (all must PASS)

1. Productization Draft candidate hosted CI PASS.
2. Independent security review of Service control surface PASS (not Cursor self-review).
3. `QUALIFICATION_SYNTHETIC=PASS` and `RESTORE_SYNTHETIC=PASS` on the same HEAD.
4. Exact test Runner identity recorded privately by Owner (not in public repo):
   - SCM service name
   - runner home / work root
   - registration scope + server runner ID
   - start mode + running state
5. No active Worker on the test Runner (`Busy` → abort).
6. `LiveScmExecutionGate` remains CLOSED until a sealed
   `OwnerScmAuthorizationProof` for the exact service name + image token +
   rights is issued for the window. Public bool construction cannot open the gate.

## Authorized mutation scope (when Owner token granted)

- Exactly one named Windows Service matching the captured identity.
- Narrow rights only: query/start/stop/change-config for that service name.
- Optional admission selector withdraw/restore on that exact runner only.
- Duration: 15–120 minutes Owner Transaction window.

## Operator sequence (authorized window only)

1. Capture prestate (`ServicePrestateCapture`).
2. Verify exact identity (name, image, ownership).
3. Withdraw managed admission; confirm selector absent (or BLOCKED).
4. Confirm Worker idle.
5. Stop request → independent stop readback.
6. Start request → independent start readback.
7. Restore admission if withdrawn.
8. Restore start mode / running state from prestate.
9. Emit QUALIFICATION + RESTORE separately.

## Forbidden

- Wildcard service queries that stop unrelated services
- Production dogfood Runner mutation without a separate token
- Credential file reads/writes
- Registration/unregister
- Label changes outside the reserved admission selector owned by the test runner
- Destructive Worker termination / taskkill
- Opening the live gate in CI or unattended trains

## Required evidence fields

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
QUALIFICATION_SYNTHETIC=<PASS|FAIL>
RESTORE_SYNTHETIC=<PASS|FAIL>
LIVE_QUALIFICATION=<PASS|FAIL|BLOCKED|NOT_AUTHORIZED>
INDEPENDENT_SECURITY_REVIEW=<PASS|PENDING>
PRESTATE_CAPTURED=true
SERVICE_NAME_HASH=<privacy-safe>
START_MODE_BEFORE=...
STATE_BEFORE=...
STATE_AFTER=...
START_MODE_AFTER=...
WORKER_BUSY_DURING=false
ADMISSION_WITHDRAWN=<true|false|n/a>
ADMISSION_RESTORED=<true|false|n/a>
PRODUCTION_MUTATION=true   # only inside the authorized window
OWNER_TOKEN=<id>
```

## Restore contract

1. Capture prestate before any mutation.
2. On success or failure, attempt restore of start mode and running/stopped state.
3. `RESTORE=FAIL` latches a blocker even if qualification steps passed.
4. Public receipts contain only hashed/privacy-safe identifiers.

## Exit criteria

- PASS: stop→start or drain→stop→start on the test Runner with restore PASS.
- BLOCKED: missing independent review, busy Worker, identity drift, or Owner abort.
- Never convert UNPROVEN or SYNTHETIC into LIVE PASS.
