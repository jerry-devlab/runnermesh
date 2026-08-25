# RunnerMesh v0.1 Implementation Roadmap

Status: **Accepted implementation sequence**

G01 (domain foundation) is complete. The remaining v0.1 plan is split into autonomous trains separated by mandatory human gates.

## Train A — autonomous core product

- **G02 Runtime Contracts** — stable runtime models and JSON/IPC-facing contracts.
- **G03 Agent Core** — Observe/Decide/Reconcile skeleton, config/state ownership, synthetic backends.
- **G04 Local IPC** — Windows Named Pipe, reconnect, single-Agent authority.
- **G05 CLI Control** — status/doctor/mode/zen/probe/runner/version surfaces.
- **G06 Tray + i18n/theme** — primary daily UI using synthetic/Agent snapshots.
- **G07 Probes + Auto Lite** — User Activity, Steam Game, Process List, deterministic policy.

Train A may run unattended across separate branches/PRs. Stop only for blocker, trust/ownership ambiguity, or exhausted repair budget.

## Train B1 — autonomous runner/host foundations

- **G08 Runner Observer** — read-only official-runner discovery and phase/link evidence.
- **G09 Supervisor Core** — synthetic lifecycle/start-stop-drain/adoption abstractions; no real production runner mutation.
- **G10 Host + Recovery** — CPU/memory/idle/session observation, restart reconstruction, safe adoption logic.

Train B1 stops before G11.

## Pre-H1 readiness recovery

The initial G06 acceptance proved the presentation contracts, stable menu IDs,
and synthetic event-loop adapter. It did not prove a persistent Windows Agent
with a real notification-area backend. Those historical claims remain useful
and are recorded precisely as:

- **G06 presentation contract:** PASS;
- **G06 native tray runtime:** UNPROVEN before G06R.

- **G06R Native Tray + Persistent Agent Runtime** — real ordinary-user
  `runnermesh-agent`, local Named Pipe, and native Windows tray backend.
- **G10R Pre-H1 Integration** — non-mutating end-to-end readiness evidence for
  the development Agent, CLI, host/runner observers, recovery, and the
  supervisor adapter.

G06R and G10R are autonomous focused Goals. They do not qualify real runner
lifecycle control and stop before H1.

## Human Gate H1

- **G11 Real Runner Lifecycle + Graceful Drain** — explicit Owner authorization required. Qualify real official-runner start/listen/busy/drain/stop/reconnect/adoption/work-root behavior in a trusted lane, after G06R/G10R readiness passes.

If graceful drain cannot be proven, stop and revise implementation/ADR; do not fake PASS.

## Train C — autonomous productization

After G11 PASS:

- **G12 User Autostart** — user-session start-on-login backend; sandbox qualification only.
- **G13 Versioned Install** — immutable version slots, stable activation entry, config/state/log separation.
- **G14 Update + Rollback** — staging, verification, durable transactions, safe activation, rollback/reconciliation.
- **G15 Packaging + Doctor Hardening** — Windows x64 packaging, provenance/checksums, install/update dry-run, expanded doctor.

Train C runs in synthetic/sandbox install roots and stops before production activation.

## Human Gate H2

- **G16 Real Workstation Cutover + Dogfood** — explicit Owner authorization required. Install an immutable RC, reconcile any conflicting execution backend only as authorized, activate user-session Agent, verify tray/CLI/Auto Lite/Steam/process/Zen/modes/drain/autostart/crash-recovery/source-runtime isolation, and preserve rollback.

## RC and release

- **G17 RC Closeout** — freeze exact v0.1 candidate, package/checksums/release notes, exact-head CI, evidence ledger, privacy/security review. No stable publication.
- **G18 v0.1.0 Release** — explicit Owner authorization required. Publish Windows x64 artifact, checksums, release notes, and verify public provenance. Do not auto-start v0.2.

## Goal discipline

Every G02-G18 Goal:

1. starts from authoritative `main` or an admitted predecessor;
2. uses a focused branch/PR;
3. declares changed risk vector;
4. iterates with focused tests;
5. settles one candidate;
6. runs only required risk gates;
7. reuses accepted unchanged-risk evidence explicitly;
8. merges intentionally;
9. verifies remote main;
10. emits a concise durable receipt.

Ordinary source development always defaults to `PRODUCTION_MUTATION=false`.
