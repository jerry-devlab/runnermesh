# RunnerMesh

RunnerMesh is a human-first CI capacity orchestration layer for interactive developer workstations.

Turn everyday developer workstations into an elastic CI pool without treating them like dedicated servers.

## Why RunnerMesh?

Teams often have capable workstations with intermittent spare capacity, but those machines have a human owner whose work must take priority. RunnerMesh is being designed to contribute that capacity conservatively while keeping GitHub Actions native.

## Product invariants

- **Human first.** Foreground use always has priority over CI.
- **Zero developer workflow change.** Existing CI workflows remain CI-platform workflows.
- **CI-platform native.** RunnerMesh manages capacity, not workflow semantics.
- **Fail open for the workstation; fail closed for CI admission.** Uncertainty must preserve the human's machine and reject new CI work.
- **No inbound workstation requirement.** Ordinary operation starts outbound connections.
- **Graceful drain.** A node can stop accepting new work before it becomes unavailable.
- **Capability-aware admission.** Hardware, operating system, and policy capabilities inform eligibility.
- **Native resource control first.** Backends use operating-system mechanisms where they fit.
- **Semantic portability, not fake equivalence.** Common policy intent does not imply identical controls on every platform.
- **Explainable decisions.** Admission and policy outcomes should be understandable to the human owner.
- **Manual policy wins.** A selected mode overrides automatic sensing.
- **No CI-data proxy.** Source, logs, and artifacts remain on the CI provider data plane.
- **Minimal authority.** Privileged host operations are a separate, narrowly scoped concern.
- **One execution identity, one owned work root.** Different execution identities must not actively share a work root.

## Project status

RunnerMesh is **pre-v0.1 / active development**. The accepted foundation already includes the domain/runtime contracts, Agent Core, local Named Pipe IPC, CLI, native Windows tray, User Activity/Steam/Process List probes, conservative Auto Lite, host observation, official-runner observation, supervisor foundations, persistent ordinary-user development runtime, and the Windows native process snapshot used by observation/probes.

The remaining v0.1 work is concentrated in real admission/lifecycle semantics, one-shot qualification, productized install/autostart/update/rollback/package flows, real workstation dogfood, and release closeout. The historical G11 qualification path has been superseded by the roadmap-v2 G11R architecture/implementation/readiness sequence; the project is not yet an installable stable product.

See [`goals/RM-V0_1-EXECUTION-STATUS.md`](goals/RM-V0_1-EXECUTION-STATUS.md) for the durable current execution ledger.

## Core concepts

RunnerMesh manages contributed CI **supply**, not a replacement workflow scheduler:

1. **CI demand** — GitHub Actions owns workflow demand, queue semantics, dependencies, assignment mechanics, logs, and checks.
2. **Placement** — RunnerMesh determines eligible contributed capacity and capabilities.
3. **Admission** — Each workstation decides whether it should accept new CI work now.
4. **Resource policy** — Native operating-system mechanisms determine how admitted work coexists with foreground use.

See [the architecture](docs/architecture.md) for the planned boundary in more detail.

## Node modes

Planned human-facing modes are `auto`, `work`, `gaming`, `idle`, `maintenance`, and `force-ci`. Manual policy wins over automatic sensing.

Machine-facing states are:

- `FULL` — normal eligible capacity is available.
- `THROTTLED` — only constrained capacity is available.
- `DRAINED` — no new work is admitted while existing work is allowed to finish.
- `OFFLINE` — the node is unavailable for admission.

`THROTTLED` is stable vocabulary but real resource enforcement is deferred beyond v0.1.

## Workload classes

- **Local preflight** may use a current, dirty local workspace and is not necessarily a RunnerMesh responsibility.
- **Canonical CI** validates immutable source identity visible to the CI provider.
- **Machine qualification** validates whether a contributed node meets declared expectations.
- **Heavy CI** is planned to use explicit capacity and policy constraints.
- **Untrusted CI** must use hosted, disposable, or appropriately isolated execution rather than a trusted persistent personal workstation by default.

## Capacity model

A node contributes only the capacity it can safely offer at the moment. v0.1 focuses on one Windows workstation and the admission/lifecycle slice; later mesh versions add multi-node placement and richer capability negotiation. GitHub Actions remains the authority for workflow scheduling and the official job protocol.

## Networking model

The official GitHub runner initiates outbound connectivity. Ordinary workstation operation does not require inbound public access, port forwarding, DDNS, or a VPN merely to participate in GitHub Actions. Future RunnerMesh broker sessions are also intended to be outbound-initiated. RunnerMesh control traffic will carry capacity, policy, and capability metadata—not source code, CI logs, or artifacts.

## Security model

Persistent personal workstations must not execute arbitrary untrusted public-fork code by default. Public repository CI uses GitHub-hosted runners. Trusted self-hosted execution is an explicit, separately configured boundary. See the [threat model](docs/threat-model.md).

## Initial scope

The first supported path is GitHub Actions, Windows interactive workstations, and the official GitHub self-hosted runner. The default execution model is an ordinary intended user session running RunnerMesh and the official runner; it does not require `NETWORK SERVICE` or another service identity. PowerShell 7 must be resolvable and functional for the selected execution identity, regardless of supported installation method.

RunnerMesh will not reimplement the GitHub Actions runner protocol. An execution identity may reuse its own work root, but separate identities must not share an active work root. Ownership conflicts must not be solved by globally weakening Git safe-directory protections.

## Installation / current availability

There is no stable released binary or published crate yet. Do not treat this repository as installable product software until the v0.1 release gates are complete.

## CLI direction

The current development CLI exposes typed status/control/diagnostic surfaces over Agent IPC. Contracts remain pre-v0.1 and may still change before the first stable release.

## Non-goals

RunnerMesh is not a GitHub Actions replacement, generic workflow engine, Kubernetes or Jenkins replacement, distributed compiler, remote-development system, source synchronization system, workstation configuration manager, VM manager, generic compute scheduler, deployment platform, coding-agent orchestrator, secrets manager, VPN/NAT traversal system, source/log/artifact proxy, monitoring dashboard, or generic cross-platform cgroup compatibility layer.

## Roadmap

The public multi-version direction is documented in [docs/roadmap.md](docs/roadmap.md). The authoritative remaining v0.1 implementation sequence is [`goals/RM-V0_1-ROADMAP.md`](goals/RM-V0_1-ROADMAP.md), with current status in [`goals/RM-V0_1-EXECUTION-STATUS.md`](goals/RM-V0_1-EXECUTION-STATUS.md).

## Development

With a standard Rust toolchain available, validate the repository with:

```powershell
cargo check
cargo test
```

Public contributions are validated with GitHub-hosted CI; contributors do not need to own a self-hosted runner. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

RunnerMesh is licensed under the [MIT License](LICENSE).
