# G10R — Pre-H1 Integrated Readiness

## Mission

Prove the non-mutating components required to enter H1 operate together in an
isolated ordinary-user development runtime.

## Deliver

- real Agent, Named Pipe, CLI, and native tray integration;
- read-only host/probe and official-runner observation evidence;
- a concrete Windows user-session supervisor preparation adapter, with
  sandbox-only child-process lifecycle tests;
- Agent restart, single-authority, and IPC reconnect evidence;
- a truthful development-runtime doctor for Agent/tray/IPC/probes/host/runner
  observation/supervisor/work-root readiness: runtime ready does not mean
  graceful drain is qualified.

## G10R implementation boundary

The development runtime keeps `NoRunnerControl` as its reconciler. Its normal
loop refreshes `Observe -> Decide -> Reconcile` from reconstructable
read-only evidence, while all tray mutations remain on the UI thread.

`WindowsUserSessionSupervisorAdapter` validates an explicitly selected runner
home and prepares typed user-session start/drain/stop/reconnect/adoption
operations without executing any of them. A future H1-qualified executor must
still supply the exact real-runner lifecycle mechanism only after identity and
work-root checks pass. The G10R child-process test is limited to a harmless
sandbox process and is not connected to an official runner.

The development `doctor` reports pre-H1 component readiness and explicitly
keeps `real-runner-drain` as a warning with an H1-required reason code. A
passing pre-H1 result therefore proves integration readiness, never a claim
that graceful drain, real lifecycle control, or work-root mutation was
qualified.

## Non-goals

No real official-runner lifecycle mutation, registration change, work-root
mutation, Service mutation, installed-runtime activation, autostart activation,
or Organization configuration.

## Risk vector

Ordinary code + tray/presentation + probe/read-only observation + synthetic
supervisor control + sandbox persistent configuration.

## Gates

Exact-head Rust suite; isolated real Agent/Pipe/CLI/tray smoke; read-only host
and runner-observer proof; sandbox lifecycle/recovery tests; public hosted
Windows and Ubuntu CI; privacy audit.

## Exit

`PRE_H1_RUNTIME_READY=true` while
`REAL_RUNNER_DRAIN_QUALIFIED=false`. Stop for H1 / G11 Owner authorization.
