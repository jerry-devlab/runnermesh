# G02 — Runtime Contracts

## Mission

Extend G01's `NodeState`/`UserMode` foundation with the stable runtime vocabulary needed by every frontend and later Agent implementation.

## Deliver

`RunnerPhase`, `AdmissionDecision`, `ZenOverride`, `AgentHealth`, `LinkState`/`LinkSnapshot`, `ProbeRuntimeState`/`ProbeSnapshot`, `AgentSnapshot`, `AgentCommand`/`AgentResponse`, and `UiPreferences`.

Lock stable JSON spelling and reason-code conventions. Keep presentation strings out of machine contracts.

## Non-goals

No Agent loop, IPC, OS probes, runner control, tray, persistent host mutation, or production install.

## Risk vector

`CODE_CHANGED=true`; all external/runtime mutation risks false.

## Gates

Focused serialization/table tests + one settled-head hosted CI + public privacy audit.

## Exit

All contracts compile, round-trip deterministically, reject invalid spellings where intended, and preserve the G01 enum contracts.

Next: G03.
