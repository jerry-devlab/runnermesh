# G03 — Agent Core

## Mission

Create the in-process Agent Core around `Observe -> Decide -> Reconcile` using synthetic backends.

## Deliver

- authoritative Agent state ownership;
- synthetic observer/reconciler traits or equivalent boundaries;
- ConfigManager/state-store model;
- schema version and atomic persisted intent;
- persistent-vs-reconstructable state split;
- deterministic snapshot publication;
- no OS-specific runner mutation.

## Non-goals

No real Named Pipe, tray, Windows process control, real probes, production paths, autostart, or installer.

## Risk vector

Ordinary code + persistent-format design exercised only in temp/sandbox roots.

## Gates

Focused state/reconcile tests, atomic write/interruption tests in temp roots, one hosted CI.

## Exit

Agent Core can process synthetic commands/observations and reconcile desired state deterministically without external mutation.

Next: G04.
