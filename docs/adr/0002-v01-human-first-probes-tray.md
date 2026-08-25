# ADR 0002: v0.1 Human-First Policy, Probes, and Tray

- Status: Accepted
- Applies to: v0.1

## Decision

v0.1 is manual-first with a conservative Auto Lite. Policy consumes normalized probe evidence and preserves explicit human override semantics.

Precedence:

```text
hard safety > Zen > explicit non-auto UserMode > Auto Lite
```

Mode behavior:

- `maintenance` -> `OFFLINE`;
- `work` / `gaming` -> `DRAINED`;
- `idle` / `force-ci` -> `FULL` (`force-ci` does not bypass hard safety);
- `auto` -> Auto Lite.

## Zen

Zen is a persistent override, not another `UserMode`. It denies new admission, gracefully drains active work, stops contribution afterward, suspends nonessential probes without disabling their configuration, and keeps the minimal Agent/Tray/IPC resume shell alive.

## Probe boundary

Probe runtime states are `Active`, `Inactive`, `Unknown`, `Unavailable`, and `Suspended`. Disabled configuration is distinct from runtime state.

v0.1 includes:

- User Activity Probe;
- Steam Game Probe;
- configurable Process List Probe.

The Steam probe detects an actually running Steam App rather than Steam client presence. The first Windows implementation may read `HKCU\Software\Valve\Steam\RunningAppID`, but that source is an implementation detail and must not become a public compatibility promise.

## Auto Lite

Any relevant active probe drains capacity. Unknown safety-relevant evidence drains capacity. `FULL` requires the idle/away threshold and all relevant evidence permitting contribution. If all useful probes are disabled, Auto Lite resolves to `DRAINED`.

## Tray

Tray is the primary daily control surface. It shows build/version, Agent health, capacity/mode, runner phase, GitHub Actions link state, Zen, modes, per-probe enablement and runtime state, diagnostics, settings, update entry, and exit-after-drain.

Theme preferences: `system`, `light`, `dark`. `system` means the current-user
Windows effective application color mode, resolved at runtime from documented
`UISettings` color observation; the persisted preference remains `system`.

Language preferences: `system`, `zh-CN`, `en-US`. `system` means the current
user's preferred UI language, resolved at runtime from the documented user UI
language list; it is not the keyboard, region, or Windows installation locale.

Tray option descriptions are a v0.1 presentation feature. They use stable
semantic menu keys, are localized separately from menu labels, and are enabled
by default through the `Show option descriptions` UI preference. They never
alter command routing, policy, probes, or runner state.

Localization is presentation-only. Stable JSON, config, IPC, menu/action IDs, enum values, and reason codes are never localized. UI mutation stays on the tray/event-loop owner thread.

## Consequences

- v0.1 is useful without v0.3-level application intelligence.
- User-controlled process lists extend protection without a universal game database.
- A future TUI can render the same `AgentSnapshot` without becoming a second authority.
