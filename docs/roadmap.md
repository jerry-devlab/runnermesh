# Roadmap

This roadmap describes intended stages. Roadmap items are plans, not claims of current implementation. The detailed v0.1 implementation sequence is in [`../goals/RM-V0_1-ROADMAP.md`](../goals/RM-V0_1-ROADMAP.md).

## P0 — Execution bootstrap

Design validation was completed before this public product repository began. Private dogfood evidence and infrastructure details are intentionally not published here.

## v0.1 — First usable Windows admission controller

The foundational `NodeState` and `UserMode` domain contracts are complete. The full v0.1 design is frozen in [`v0.1-design-freeze.md`](v0.1-design-freeze.md); operational behavior remains under implementation.

v0.1 targets a single interactive Windows workstation and an already-configured official GitHub Actions self-hosted runner. Planned first-usable capabilities include:

- ordinary user-session persistent Agent;
- Windows Tray + CLI + local Named Pipe IPC;
- explicit modes and persistent Zen override;
- User Activity, Steam Game, and configurable Process List probes;
- conservative Auto Lite;
- stable status/doctor/version and JSON contracts;
- typed runner phase and GitHub Actions link state;
- runner observation, supervision, restart/reconnect/adoption, and graceful drain;
- CPU/memory/idle/session observation;
- system/light/dark and Simplified-Chinese/English UI preferences;
- user-session autostart and crash/restart reconciliation;
- user-level immutable versioned installation;
- staged update, durable activation receipts, health-checked rollback;
- Windows x64 GitHub Release artifacts and SHA-256 checksums.

`THROTTLED` remains part of the stable state vocabulary but does not claim resource enforcement in v0.1.

## v0.2 — Windows resource policy

- process-tree ownership and Job Objects;
- policy profiles;
- foreground-friendly execution;
- bounded CPU/memory/native controls;
- EcoQoS/power-throttling where appropriate;
- backend capability reporting;
- recovery/explainability for resource enforcement.

## v0.3 — Rich automatic admission

- richer user/application activity intelligence;
- GPU/foreground/latency-sensitive workload signals;
- battery/AC policy;
- workload classification;
- hysteresis/cooldown;
- deterministic policy refinement;
- manual-override precedence preserved.

## v0.4 — First Mesh

- multi-node inventory and presence;
- capability declaration;
- shared pool;
- hard requirements / soft preferences;
- fallback;
- node disappearance/restart handling.

## v0.5 — Dynamic capacity

Planned dynamic capacity control.

## v0.6 — Machine qualification

Planned machine qualification and capability validation.

## v0.7 — Isolated execution

Planned isolated/disposable execution for appropriate workloads.

## v0.8 — Linux backend

Planned Linux resource-policy backend.

## v0.9 — Broker and scale-set integration

Planned outbound broker, JIT runner provisioning, and scale-set integration.

## v1.0 — Workstation-aware CI mesh

Planned mature workstation-aware CI mesh.
