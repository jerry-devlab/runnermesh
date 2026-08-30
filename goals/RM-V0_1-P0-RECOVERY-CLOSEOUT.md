# P0 — Historical G11 Recovery-Only Closeout

Type: **Owner transaction / incident closeout, not product development**

## Mission

Return the workstation from the terminal historical G11 qualification experiment to the exact known-good baseline, then stop.

P0 exists to close old experimental state. It must not continue qualification, generate a new G11 transaction, change admission architecture, or merge product source.

## Required prestate

Fresh read-only evidence must identify the exact bound runner/service, historical terminal transaction, orphan process state, work-root/registration fingerprints, and unrelated runner processes.

No global `Runner.Listener`/`Runner.Worker` counting is authoritative; control is exact-runner-home/ancestry scoped.

## Allowed mutation

Only the minimum frozen recovery actions required to restore the original baseline, under explicit Owner authorization/UAC.

No registration change, runner-group/label change, broad work-root ownership change, global `safe.directory`, or unrelated runner mutation.

## Acceptance

```text
ORIGINAL_SERVICE=Running
SERVICE_BACKED_BOUND_LISTENER=1
BOUND_WORKER=0
HISTORICAL_ORPHAN_LISTENER=0
QUALIFICATION_WORKSPACE=CLEAN
SERVICE_CONFIG_UNCHANGED=true
SERVICE_SECURITY_UNCHANGED=true
REGISTRATION_UNCHANGED=true
RUNNER_HOME_UNCHANGED=true
WORK_ROOT_UNCHANGED=true
UNRELATED_RUNNER_MUTATED=false
```

## Exit

Emit a private recovery receipt and update only the public execution ledger state to `P0=ACCEPTED`/`known-good baseline restored` without publishing private host identifiers.

Stop after recovery verification. Next product work is governed independently by G11R-A.