# HUX-2 Owner Gate — Independent test Runner Service control

Status: **PREPARE_ONLY** · `PRODUCTION_MUTATION=false` until explicit Owner authorization

This plan does **not** authorize live Service mutation. It defines the evidence
and restore contract required before any future Owner transaction.

## Purpose

Qualify exact Windows Service start/stop for **one disposable test Runner** that
is not the production dogfood identity, after HUX-0/HUX-1 source acceptance.

## Preconditions (all must PASS)

1. Draft LRM/HUX source candidate hosted CI PASS.
2. Independent security review of Service control surface PASS (not Cursor self-review).
3. Exact test Runner identity recorded privately by Owner (not in public repo):
   - SCM service name
   - runner home / work root
   - registration scope + server runner ID
   - start mode + running state
4. No active Worker on the test Runner (`Busy` → abort).
5. Rollback/restore rehearsal completed in synthetic tests.

## Authorized mutation scope (when Owner token granted)

- Exactly one named Windows Service matching the captured identity.
- Narrow rights only: query/start/stop/change-config for that service name.
- Optional admission selector withdraw/restore on that exact runner only.
- Duration: 15–120 minutes Owner Transaction window.

## Forbidden

- Wildcard service queries that stop unrelated services
- Production dogfood Runner mutation without a separate token
- Credential file reads/writes
- Registration/unregister
- Label changes outside the reserved admission selector owned by the test runner
- Destructive Worker termination

## Required evidence fields

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
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
INDEPENDENT_REVIEW=PASS
```

## Restore contract

1. Capture prestate before any mutation (`ServicePrestateCapture`).
2. On success or failure, attempt restore of start mode and running/stopped state.
3. `RESTORE=FAIL` latches a blocker even if qualification steps passed.
4. Public receipts contain only hashed/privacy-safe identifiers.

## Exit criteria

- PASS: stop→start or drain→stop→start on the test Runner with restore PASS.
- BLOCKED: missing independent review, busy Worker, identity drift, or Owner abort.
- Never convert UNPROVEN restore into PASS.
