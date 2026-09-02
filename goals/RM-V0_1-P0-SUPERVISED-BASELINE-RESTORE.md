# RM-V0_1-P0-SUPERVISED-BASELINE-RESTORE-001

Status: **PREPARING — future Owner transaction**

Type: **supervised maintenance / historical incident closeout, not product
development**

## Mission

Reconcile the one historical G11 experiment to its exact known-good current
baseline, take only the action the fresh state requires (possibly none),
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
- Production mutation: limited to the minimum exact action selected by fresh
  current-state preflight and declared in the private Owner runbook; otherwise
  false. A healthy baseline requires no mutation.
- Reuse: no prior nonce, handoff, transaction ID, preflight, or authorization is
  reusable.

## Required sequence

```text
fresh read-only exact-scope preflight
-> select minimum action from current invariants
-> PREPARED
-> WAITING_FOR_OWNER
-> fresh Owner authorization
-> minimum exact action or no-op
-> independent postverification
-> durable privacy-safe result
-> stop
```

The public Goal deliberately does not embed private host identifiers or exact
private commands.

## Read-only preflight

Verify exact target identity, image/home binding, service configuration,
current historical-orphan count, absence of active bound work, registration and
work-root fingerprints, and unrelated-runner isolation. Missing or ambiguous
current evidence and durable identity drift are `BLOCKED_PRECONDITION` and
permit no mutation. A positively observed orphan count of zero is the desired
cleanup postcondition:

```text
ORPHAN_CLEANUP=ALREADY_SATISFIED
```

Do not recreate an orphan merely to satisfy an obsolete intervention path.

Global process-name counts are not authority.

Durable identity consists of runner scope/registration, runner home, service
identity/configuration/security, work root, execution identity, binary
fingerprints, and other ownership bindings. PID, parent PID, process creation
time, process handle, session, and a specific Listener instance are volatile.

```text
LIVE_PROCESS_EVIDENCE_TTL=SAME_OWNER_TRANSACTION_ONLY
```

Volatile evidence is reacquired and revalidated immediately before any process
mutation. Process turnover with unchanged durable identity requires fresh
reacquisition, not a permanent drift failure.

## Current-state action selection

| Fresh exact state | Prepared action |
|---|---|
| Durable bindings valid; service stopped; orphan 0; worker 0; ambiguity 0 | `START_SERVICE_ONLY` |
| Durable bindings valid; service stopped; exact orphan 1; worker 0; ambiguity 0; live process identity freshly reacquired | `TERMINATE_EXACT_ORPHAN_THEN_START_SERVICE` |
| Complete desired baseline already healthy | `NO_MUTATION_REQUIRED` |
| Orphan count greater than one or unknown, ambiguity, active Worker, durable drift, stale exact-orphan evidence, or any unexpected topology | `BLOCKED_PRECONDITION` |

Any unlisted topology fails closed. The public table is semantic only; exact
identities and commands remain private.

## Allowed mutation

Only after explicit Owner authorization, unless the selected branch is
`NO_MUTATION_REQUIRED`:

1. if and only if one exact orphan remains, reacquire and revalidate that live
   process instance immediately before terminating it;
2. restore only the exact target service to its known-good baseline when it is
   not already healthy;
3. perform no additional qualification or setup.

No registration, label, group, repository/Organization access, work-root,
global Git, unrelated runner, product install, autostart, or H1 mutation is in
scope.

## Independent postverification

Privacy-safe acceptance fields are:

```text
ORIGINAL_SERVICE=Running
SERVICE_BACKED_BOUND_LISTENER=1
TARGET_WORKER=0
HISTORICAL_ORPHAN_LISTENER=0
TARGET_AMBIGUITY=0
WORKSPACE=CLEAN
SERVICE_CONFIG_UNCHANGED=true
SERVICE_SECURITY_UNCHANGED=true
REGISTRATION_UNCHANGED=true
RUNNER_HOME_UNCHANGED=true
WORK_ROOT_UNCHANGED=true
UNRELATED_RUNNER_MUTATED=false
```

The postverification must be independent of the mutation helper's success
claim and is required after every branch, including `NO_MUTATION_REQUIRED`.
Launcher success, helper exit, or a global process count is insufficient.

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
