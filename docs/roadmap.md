# Roadmap

This roadmap describes intended stages. Except for the design-validation note in P0, the items below are not implemented in this bootstrap.

## P0 — Execution bootstrap

Design validation was completed before this public product repository began. Private dogfood evidence and infrastructure details are intentionally not published here.

## v0.1 — Windows admission prototype

The foundational `NodeState` and `UserMode` domain contracts are complete. All operational v0.1 behavior below remains planned.

- status and doctor;
- explicit modes;
- `FULL`, `THROTTLED`, `DRAINED`, and `OFFLINE`;
- runner lifecycle observation;
- graceful drain;
- CPU, memory, and user-idle observation;
- stable JSON; and
- conservative failure behavior.

## v0.2 — Windows resource policy

- process-tree ownership;
- Job Objects;
- policy profiles;
- foreground-friendly execution;
- bounded native controls;
- backend capability reporting; and
- recovery and explainability.

## v0.3 — Automatic admission

- user activity;
- CPU and memory pressure;
- battery and AC state;
- workload classification;
- latency-sensitive workload detection;
- hysteresis and cooldowns;
- deterministic policy; and
- manual-override precedence.

## v0.4 — First Mesh

- node inventory;
- capability declaration;
- shared pool;
- hard requirements;
- soft preferences;
- fallback; and
- node disappearance and restart handling.

## v0.5 — Dynamic capacity

Planned dynamic capacity control.

## v0.6 — Machine qualification

Planned machine qualification and capability validation.

## v0.7 — Isolated execution

Planned isolated execution for appropriate workloads.

## v0.8 — Linux backend

Planned Linux resource-policy backend.

## v0.9 — Broker and scale-set integration

Planned outbound broker and scale-set integration.

## v1.0 — workstation-aware CI mesh

Planned workstation-aware CI mesh.
