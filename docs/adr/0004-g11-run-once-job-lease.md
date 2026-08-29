# ADR 0004: G11 Run-Once Job Lease

- Status: Accepted for the bounded G11 executor; real-host qualification remains Owner-gated.
- Applies to: the v0.1 G11 user-session executor only.

## Decision

```text
V0_1_G11_CAPACITY_MODEL=RUN_ONCE_JOB_LEASE
BUSY_DRAIN=WAIT_FOR_ACTIVE_RUN_ONCE_JOB
BUSY_DRAIN_SIGNAL=false
IDLE_LISTENER_SIGNAL_USED_IN_G11_DESIGN=false
```

For each `FULL` lease, the executor launches only the verified configured
`run.cmd --once` entrypoint in its exact runner home. The binding includes the
canonical runner home, exact `run.cmd` bytes, registration bytes, listener and
worker image bytes, and the exact `_work` root identity. Drift refuses before
launch, reconstruction, or process observation can become control.

When a run-once job is busy and policy requests `DRAINED`, RunnerMesh records
`DrainPending` and sends no console signal to the Listener or Worker. The job
finishes normally, the upstream run-once Listener exits, and a `DRAINED` target
does not relaunch it. If the target returns to `FULL` after exit, exactly one
new run-once lease may be launched.

## Upstream audit

The locally installed runner was `v2.336.0`. Its matching upstream source
accepts `run --once`, starts one job, prevents another dispatch after that
job has been received, and exits after completion. It emits a warning that
`--once` is planned for future deprecation. `--ephemeral` is not used: it is a
registration-lifecycle choice, not a local drain primitive.

The official cancellation path shuts down the runner. Together with the prior
trusted Busy experiment in which CTRL+BREAK cancelled the active job, neither
CTRL+C nor CTRL+BREAK is a G11 drain control.

## Listening-to-drain boundary

```text
IDLE_WITHDRAWAL_ATOMICITY=UNPROVEN
```

Once an idle Listener is waiting, the upstream runner exposes no proven local,
race-free operation that both prevents a new admission and avoids cancelling a
newly started Worker. The bounded G11 executor therefore refuses an
idle-withdrawal request rather than claiming that a signal is graceful. This
does not weaken the frozen product invariant; it keeps that future production
semantic outside the G11 Busy-drain acceptance path until separately proven.

## Restart reconstruction

On Agent restart, exact listener/worker image paths are read-only evidence.
An exact bound Listener is not adopted for control. RunnerMesh enters
`SafeWaitForExactBoundRunner`, sends no signal, and permits a replacement only
after the observed process exits naturally. Same-name processes under a
different runner home are ignored; unavailable or ambiguous image-path
evidence refuses.

## Future trusted qualification sequence

The real G11 path uses two natural run-once exits: the primary trusted job
proves Busy drain, then a separate reconnect witness job proves reconnection.
Only after both Listeners and Workers have exited naturally may the exact
qualification workspace be cleaned and the original service restored.

```text
BUSY_DRAIN_SIGNAL_USED=false
IDLE_LISTENER_SIGNAL_USED=false
```

No real runner, service, registration, work-root, or qualification workspace
mutation is authorized by this ADR.
