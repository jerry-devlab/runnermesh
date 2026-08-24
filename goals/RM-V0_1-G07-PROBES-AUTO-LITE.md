# G07 — Probes and Auto Lite

## Mission

Implement normalized local activity evidence and the conservative v0.1 automatic admission policy.

## Deliver

- Activity/Workload Probe abstraction;
- User Activity Probe;
- Steam Game Probe detecting actual running Steam App, not Steam client presence;
- configurable Process List Probe;
- `Active` / `Inactive` / `Unknown` / `Unavailable` / `Suspended` semantics;
- per-probe enablement and health;
- Auto Lite policy and explainable reason codes;
- precedence: hard safety > Zen > explicit non-auto mode > Auto Lite.

The initial Steam Windows backend may read `HKCU\Software\Valve\Steam\RunningAppID`; keep it behind the probe boundary.

## Non-goals

No GPU/game database, broad foreground classification, real runner mutation, or v0.3 hysteresis engine.

## Risk vector

Probe/policy + ordinary code; live host access read-only only.

## Gates

Table-driven policy matrix, probe fixtures, Unknown/Unavailable failure tests, optional focused live read-only Windows probe proof, hosted CI.

## Exit

Auto Lite never admits on ambiguous safety evidence and manual/Zen precedence is deterministic.

Next: G08.
