# Architecture

RunnerMesh is a capacity orchestration layer for interactive workstations. It contributes supply to a CI platform; it does not replace that platform's workflow scheduler or runner protocol.

The detailed accepted v0.1 contract is in [`v0.1-design-freeze.md`](v0.1-design-freeze.md).

## Boundaries

GitHub Actions remains responsible for workflow parsing, demand queues, job dependencies, runner assignment mechanics, logs, checks, artifacts, and the job protocol. RunnerMesh manages availability, admission, and lifecycle of contributed execution capacity.

RunnerMesh does not proxy source code, CI logs, or artifacts. Future control-plane traffic is limited to capacity, policy, capability, and lifecycle metadata.

## Conceptual layers

1. **CI demand** — GitHub Actions owns demand and queue semantics.
2. **Placement** — RunnerMesh determines eligible contributed capacity/capabilities.
3. **Admission** — a workstation decides whether it should accept new CI work now.
4. **Resource policy** — native OS mechanisms decide how admitted work coexists with foreground use.

v0.1 implements the single-node admission/lifecycle slice. Multi-node placement remains later work.

## v0.1 runtime topology

```text
ordinary Windows user session

CLI -----\
          >-- Named Pipe --> RunnerMesh Agent --> official Runner.Listener --> Runner.Worker --> GitHub
Tray ----/
```

The Agent Core is the sole authority. CLI and Tray render `AgentSnapshot` and issue typed `AgentCommand`s. The Agent follows `Observe -> Decide -> Reconcile`.

Exactly one controlling Agent exists per user profile. A future observer-only development profile must be isolated from production IPC/data and lack control authority.

## Human-first operation

The workstation owner has priority. RunnerMesh fails open for workstation use and fail-closed for new CI admission when evidence is uncertain.

Stable modes are `auto`, `work`, `gaming`, `idle`, `maintenance`, and `force-ci`; stable node states are `FULL`, `THROTTLED`, `DRAINED`, and `OFFLINE`.

v0.1 adds Zen as a persistent override above `UserMode`, normalized Activity Probes, and conservative Auto Lite. Manual policy/Zen/hard safety precedence is defined in ADR 0002.

## Probe boundary

Policy consumes normalized probe evidence, not provider-specific implementation types. v0.1 probes are User Activity, Steam Game, and configurable Process List. `Unknown`/`Unavailable` are explicit and cannot silently become `Inactive`.

## Capacity and lifecycle

Supervision covers local runner observation, launch/listening/busy phase, graceful drain, stop/restart/reconnect, safe adoption after Agent restart, and work-root ownership.

`DRAINED` means no new capacity is admitted while eligible active work may still be finishing. It is not synonymous with `RunnerPhase::Stopped`.

## Work-root contract

Each execution identity owns one active work root. The same identity can reuse its own root; different identities must not share one while active. RunnerMesh must not solve ownership conflicts by globally weakening Git safe-directory protections.

## Connection model

v0.1 exposes a typed GitHub Actions link state rather than a generic boolean. Process existence is not sufficient proof of remote connectivity; ambiguous evidence reports `Unknown`. Future broker connectivity is a separate connection kind.

## Presentation

Windows Tray is the v0.1 daily UI and CLI is the automation/diagnostic interface. Simplified Chinese/English and system/light/dark preferences affect presentation only; machine contracts remain stable and non-localized. No TUI/full GUI/Web UI is in v0.1.

## Persistence and recovery

Persist user intent/configuration and durable install/update receipts. Reconstruct transient process, host, link, and derived-state observations after restart. Agent restart may safely adopt an existing listener after verifying identity/home/work-root authority.

## Privileged operations

Privileged host operations are narrowly scoped, transactional, batched, reversible where practical, durably receipted, and reconcilable after control-plane interruption. A non-elevated control plane must not infer failure solely from loss of synchronous helper completion.

## Release/runtime isolation

Permanent invariant:

```text
SOURCE != BUILD != RELEASE != INSTALLED RUNTIME != ACTIVE VERSION
```

Production installation uses immutable version slots and an explicit activation indirection. Mutable worktrees and Cargo `target/` output are not deployment sources. See ADR 0003.
