# G09 — Supervisor Core

## Mission

Implement runner supervision semantics behind deterministic fake/synthetic process backends.

## Deliver

Desired start/stop, drain intent, restart, adoption, process-tree ownership, no-op/idempotence behavior, and safe refusal on ownership ambiguity.

## Non-goals

Do not mutate the real production runner, registration, work root, service, or Organization settings.

## Risk vector

Runner-control code in synthetic boundary only.

## Gates

Lifecycle fixture families for stopped/listening/busy/drain/restart/adoption/ambiguous ownership + hosted CI.

## Exit

Supervisor semantics are deterministic and ready for real-runner qualification, but no real control claim is made.

Next: G10.
