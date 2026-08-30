# Roadmap

This roadmap describes intended product stages. Roadmap items are plans, not claims of current implementation. The authoritative v0.1 execution sequence is in [`../goals/RM-V0_1-ROADMAP.md`](../goals/RM-V0_1-ROADMAP.md), with current state in [`../goals/RM-V0_1-EXECUTION-STATUS.md`](../goals/RM-V0_1-EXECUTION-STATUS.md).

## P0 — Execution bootstrap

Design validation was completed before this public product repository began. Private dogfood evidence and infrastructure details are intentionally not published here.

## v0.1 — First usable Windows admission controller

The stable domain/runtime foundation, Agent Core, IPC, CLI, native tray, probes, conservative Auto Lite, host observation, runner observation, and pre-H1 supervisor/runtime foundation are implemented. Real admission/lifecycle semantics, productization, real cutover, sustained dogfood, and release closeout remain under active development.

The original post-G10R G11 qualification path was superseded after the 2026-08-30 architecture audit. Roadmap v2 separates admission architecture, lifecycle implementation, qualification readiness, one-shot real qualification, productization, pre-H2 RC integration, real cutover, and sustained dogfood.

v0.1 still targets a single interactive Windows workstation and an official GitHub Actions self-hosted runner. First-usable capabilities include:

- ordinary user-session persistent Agent;
- Windows Tray + CLI + local Named Pipe IPC;
- explicit modes and persistent Zen override;
- User Activity, Steam Game, and configurable Process List probes;
- conservative Auto Lite;
- stable status/doctor/version and JSON contracts;
- typed runner phase and GitHub Actions link state;
- runner observation, admission/lifecycle control, restart/reconnect/reconstruction, and graceful active-job-safe withdrawal;
- CPU/memory/idle/session observation;
- system/light/dark and Simplified-Chinese/English UI preferences;
- user-session autostart and crash/restart reconciliation;
- user-level immutable versioned installation;
- staged update, durable activation receipts, health-checked rollback;
- Windows x64 GitHub Release artifacts and SHA-256 checksums;
- one-shot trusted real qualification before productization admission;
- production-style RC cutover followed by at least 24 hours of sustained ordinary-use dogfood before release closeout.

`THROTTLED` remains part of the stable state vocabulary but does not claim resource enforcement in v0.1.

### v0.1 execution phases

1. close historical G11 experimental state with recovery-only semantics;
2. G11R-A admission-linearization architecture;
3. G11R-B lifecycle implementation;
4. G11R-C qualification readiness;
5. H1 one-shot real qualification with automatic restore attempt;
6. G12-G15 productization rewrite/salvage;
7. G15R integrated pre-H2 RC;
8. H2/G16 real cutover + sustained dogfood;
9. G17 RC closeout;
10. H3/G18 v0.1.0 publication.

The exact mechanism for capacity withdrawal is selected by G11R-A. `run.cmd --once`, persistent local lifecycle control, server-side labels/groups, and ephemeral/JIT leases are implementation options to evaluate—not product commitments by themselves.

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
