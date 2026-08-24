# G05 — CLI Control

## Mission

Provide the first user-visible control/diagnostic surface over Agent contracts.

## Deliver

At minimum:

```text
runnermesh status
runnermesh status --json
runnermesh doctor
runnermesh doctor --json
runnermesh mode <auto|work|gaming|idle|maintenance|force-ci>
runnermesh zen on|off
runnermesh probe list
runnermesh probe enable|disable <id>
runnermesh runner status
runnermesh version
```

Commands must be truthful about unimplemented later behavior. Stable JSON is presentation-independent.

## Non-goals

No tray, real probes, or real runner control.

## Risk vector

Ordinary code / machine-contract surface.

## Gates

Parser/command/JSON contract tests + hosted CI.

## Exit

CLI drives the same `AgentCommand` authority and renders `AgentSnapshot` without owning state.

Next: G06.
