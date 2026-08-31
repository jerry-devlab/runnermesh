# Roadmap

This roadmap describes intended product stages. Roadmap items are plans, not claims of current implementation. The authoritative v0.1 execution sequence is in [`../goals/RM-V0_1-ROADMAP.md`](../goals/RM-V0_1-ROADMAP.md), with current state in [`../goals/RM-V0_1-EXECUTION-STATUS.md`](../goals/RM-V0_1-EXECUTION-STATUS.md).

## P0 — Execution bootstrap

Design validation was completed before this public product repository began. Private dogfood evidence and infrastructure details are intentionally not published here.

## v0.1 — First usable Windows admission controller

The stable domain/runtime foundation, Agent Core, IPC, CLI, native tray, probes, conservative Auto Lite, host observation, runner observation, and pre-H1 supervisor/runtime foundation are implemented. Real admission/lifecycle semantics, productization, real cutover, sustained dogfood, and release closeout remain under active development.

The original post-G10R G11 qualification path was superseded after the
2026-08-30 architecture audit. Roadmap v3 adopts the accepted G11R architecture,
separates source preparation from Owner-gated live work, and keeps H1 plus
baseline restoration as the merge/acceptance gate for productization.

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

1. governance truth and parallel source preparation policy;
2. P0 supervised baseline restoration;
3. H1 live adapters and readiness;
4. parallel G12-G15 source preparation, with merge held;
5. H1 one-shot real qualification and baseline restoration;
6. G12-G15 acceptance plus one integrated G15R RC;
7. H2/G16 cutover and sustained dogfood;
8. G17 closeout and H3/G18 publication.

G11R-A selected exact-runner GitHub-native admission through the reserved
`runnermesh-admit` label and two-phase withdrawal. `run.cmd --once`, runner
groups, and ephemeral/JIT registration are not v0.1 alternatives to reopen.

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
