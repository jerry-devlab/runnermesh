# G11R-B — Lifecycle Implementation

Type: **Autonomous source-development train**

Expected duration: **6-12 hours**

Real runner mutation: **forbidden**

## Mission

Implement the admission/lifecycle architecture accepted by G11R-A and prove it with deterministic synthetic/integration fixtures before any real-host qualification.

## Required implementation surfaces

- explicit desired/observed lifecycle transition model;
- exact runner-home and process ownership;
- listening/busy/withdraw-requested-or-drain-pending/withdrawn/reconnect behavior;
- active-job preservation;
- idle withdrawal behavior from the accepted ADR;
- race handling at the selected linearization point;
- restart/reconnect/reconstruction;
- Agent restart during listening/busy/withdrawal;
- same-name unrelated runner isolation;
- registration, executable, and work-root drift refusal;
- one execution identity / one active work root;
- source/runtime isolation.

## PR #17 salvage rule

PR #17 is not presumed mergeable as the product solution. Salvage useful work when it matches the accepted ADR, including:

- exact process scoping;
- no CTRL+C/CTRL+BREAK Busy drain;
- safe-wait reconstruction;
- bounded executor seams and lifecycle fixtures.

Refactor or supersede the PR if its `RUN_ONCE_JOB_LEASE` assumption is not the chosen architecture. Do not preserve obsolete code merely to retain history.

## Required fixtures

At minimum cover:

- idle admission -> withdrawal;
- job racing with withdrawal;
- busy -> withdrawal;
- active job completes naturally;
- no new job after the accepted linearization point;
- reconnect after withdrawal;
- restart while listening;
- restart while busy;
- restart while withdrawal is pending;
- unrelated same-name runner;
- ambiguous process-path evidence -> refuse;
- registration drift -> refuse;
- work-root drift -> refuse;
- controller interruption -> safe reconstruction.

## Validation

Settle one candidate and run the normal code gates plus risk-selected lifecycle/concurrency tests. Public CI remains hosted.

## Implemented candidate contract

The focused G11R-B candidate implements:

- typed `observe_admission_selector`, `advertise_capacity`, and
  `withdraw_capacity` operations with no generic administration surface;
- exact organization/repository, runner ID/name, reserved-label, ownership,
  and opaque credential-reference binding;
- add-one/remove-one REST requests plus mandatory observation/readback through
  an injected transport boundary that is not enabled in the product runtime;
- explicit desired, selector, lifecycle, exact identity, label ownership,
  active-Worker, reason-code, and bounded-retry snapshot fields;
- Agent `Observe -> Decide -> Reconcile` integration and distinct desired and
  achieved node state in JSON, CLI, and Tray presentation;
- no-signal active-job completion, racing-assignment `DrainPending`, restart
  reconstruction, and truthful blocked/refused/unknown outcomes;
- exact executable-path observation using the native process snapshot, with
  same-name processes from unrelated runner homes excluded; and
- synthetic REST, auth/rate/error, drift, lifecycle/race, restart, presentation,
  privacy, and source-regression tests.

Runtime admission control remains explicitly `NotConfigured` until a later
Owner-authorized OS-backed credential provider and transport are configured.
No test or runtime path in this Goal sends a real GitHub request.

## Hard boundaries

No UAC, Windows Service mutation, real runner registration mutation, Organization runner setting changes, trusted real job dispatch, destructive work-root mutation, or installed production runtime mutation.

## Exit

```text
G11R_CODE_READY=true
SYNTHETIC_LIFECYCLE_TESTS=PASS
LIFECYCLE_RACE_FIXTURES=PASS
UNRELATED_RUNNER_ISOLATION=PASS
DRIFT_REFUSAL=PASS
NO_TASKLIST_REGRESSION=PASS
REAL_HOST_MUTATION=false
```

Next: G11R-C.
