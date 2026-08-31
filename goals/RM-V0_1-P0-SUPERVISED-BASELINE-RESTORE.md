# RM-V0_1-P0-SUPERVISED-BASELINE-RESTORE-001

Status: **PREPARING — future Owner transaction**

Type: **supervised maintenance / historical incident closeout, not product
development**

## Mission

Restore the one historical G11 experiment to its exact known-good baseline,
independently verify that baseline, and stop.

This Goal does not execute H1, continue qualification, change admission
architecture, mutate runner registration/labels/access, or merge product
source. Historical R1/R2/R3 transaction artifacts remain private evidence and
are not templates for another transaction family.

## Authority and start

- Repository authority: current accepted RunnerMesh roadmap and execution
  ledger.
- Host authority: none until a fresh read-only preflight passes and the Owner is
  present with explicit authorization for this exact maintenance attempt.
- Production mutation: limited to the minimum exact recovery actions declared
  in the private Owner runbook after preflight; otherwise false.
- Reuse: no prior nonce, handoff, transaction ID, preflight, or authorization is
  reusable.

## Required sequence

```text
fresh read-only exact-scope preflight
-> PREPARED
-> WAITING_FOR_OWNER
-> fresh Owner authorization
-> minimum exact recovery action
-> independent postverification
-> durable privacy-safe result
-> stop
```

The public Goal deliberately does not embed private host identifiers or exact
private commands.

## Read-only preflight

Verify exact target identity, image/home binding, service configuration,
historical orphan identity, absence of active bound work, registration and work
root fingerprints, and unrelated-runner isolation. Any missing, ambiguous, or
drifted evidence is `BLOCKED_PRECONDITION` and permits no mutation.

Global process-name counts are not authority.

## Allowed mutation

Only after explicit Owner authorization:

1. perform the minimum exact action needed to remove the frozen historical
   orphan state;
2. restore only the exact target service to its known-good baseline;
3. perform no additional qualification or setup.

No registration, label, group, repository/Organization access, work-root,
global Git, unrelated runner, product install, autostart, or H1 mutation is in
scope.

## Independent postverification

Privacy-safe acceptance fields are:

```text
EXACT_SERVICE_BASELINE=PASS
SERVICE_BACKED_BOUND_LISTENER=PASS
BOUND_WORKER_ABSENT=PASS
HISTORICAL_ORPHAN_ABSENT=PASS
QUALIFICATION_WORKSPACE_CLEAN=PASS
SERVICE_CONFIG_UNCHANGED=PASS
SERVICE_SECURITY_UNCHANGED=PASS
REGISTRATION_UNCHANGED=PASS
RUNNER_HOME_UNCHANGED=PASS
WORK_ROOT_UNCHANGED=PASS
UNRELATED_RUNNER_MUTATED=false
```

The postverification must be independent of the mutation helper's success
claim. Launcher success, helper exit, or a global process count is insufficient.

## Dispositions

- `WAITING_FOR_OWNER` is resumable control flow, not a defect.
- `OWNER_CANCELED` ends only that authorization attempt and is not an
  implementation failure.
- `FAIL_PRE_MUTATION` proves no external mutation started.
- `FAIL_POST_MUTATION` requires bounded restoration and independent
  reconciliation before any further work.
- `PASS` requires every postverification field above.

## Stop conditions

Stop without mutation on identity/ownership drift, unexpected active work,
unreadable exact evidence, preflight mismatch, absent Owner authorization, or
scope expansion. Stop after postverification regardless of PASS or failure; do
not continue into H1.

## Receipt

The private receipt may bind exact sanitized evidence. The public ledger records
only the disposition, whether mutation started, whether baseline restoration
passed, and the next safe prerequisite. It never contains private identifiers,
commands, or raw host evidence.
