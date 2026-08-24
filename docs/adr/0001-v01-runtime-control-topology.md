# ADR 0001: v0.1 Runtime and Control Topology

- Status: Accepted
- Applies to: v0.1

## Decision

RunnerMesh v0.1 runs in the intended ordinary Windows user session. The runtime has one controlling Agent and two frontend surfaces: CLI and Windows tray.

```text
CLI ----\
         > Named Pipe -> Agent Core -> official Runner.Listener -> Runner.Worker -> GitHub
Tray ---/
```

The Agent owns `Observe -> Decide -> Reconcile`. Frontends consume `AgentSnapshot` and issue typed `AgentCommand` requests. They never directly own runner lifecycle or policy state.

Exactly one controlling Agent is allowed per user profile. Use a user-scoped single-instance guard. A future observer-only development profile must use separate IPC/data roots and have no runner-control authority.

The official GitHub runner remains the CI data plane and protocol implementation. RunnerMesh does not reimplement workflow parsing, queues, logs, checks, artifacts, or the runner protocol.

## Local IPC

Windows v0.1 uses a local Named Pipe contract. The security boundary is the intended user session. IPC must be reconnectable and versioned enough to reject incompatible commands safely.

## Agent state

Persist intent/configuration; reconstruct runtime observations. Agent restart must observe and, when ownership is safe, adopt an already-running listener instead of assuming parent-process ownership is required.

## UI choice

Tray is the daily UI and CLI is the automation/diagnostic UI. No TUI or full GUI settings window is part of v0.1.

## Consequences

- Normal operation does not require an elevated Agent.
- Service/headless execution remains a separate optional future backend.
- The same runtime contracts can later support a TUI without moving authority out of the Agent.
