# ADR 0005 (Proposed): Local multi-instance Runner lifecycle management

- Status: **Proposed for post-v0.1 implementation; not a v0.1 amendment**
- Planning date: 2026-11-08
- Program Goal: [RM-LOCAL-RUNNER-MANAGER-001](../../goals/RM-LOCAL-RUNNER-MANAGER-001.md)
- Implementation authority: **none** granted by this planning document
- Baseline: [ADR 0004](0004-g11r-admission-linearization.md), [v0.1 freeze](../v0.1-design-freeze.md), and [RunnerMesh architecture](../architecture.md)

## Problem and architectural decision

A single Windows workstation can already host several independent official GitHub Actions runners: repository-scoped and organization-scoped registrations, Windows Services, and user-session listeners. An idle Listener/Service continues to consume some resources. The owner needs to **choose exactly which existing runners are allowed to run**, persist the choice, and stop unused listeners without unregistering or harming active jobs.

RunnerMesh v0.1 intentionally binds one exact official runner and controls CI eligibility through one reserved GitHub label. It does **not** implement a multi-instance registry or a general Windows Service manager. This ADR proposes a local-only extension **before cross-host Mesh** while keeping v0.1 release and evidence frozen.

**Decision proposal:** add a versioned local `RunnerRegistry` beneath one authoritative per-user Agent, with exact identity-bound per-runner desired state, an explicit enrollment boundary, separate Service and user-session backends, safe drain/stop reconciliation, and shared AgentSnapshot/IPC/CLI/Tray presentation. Do not create a new CI scheduler or a generic host configuration manager.

## Architecture

```text
CLI / Tray -- typed commands --> one Agent authority (per user profile)
                                  |
                                  +--> global policy / hard safety / Zen
                                  +--> local RunnerRegistry (explicit enrollment)
                                  |      +-- runner A: service backend / exact SCM name
                                  |      +-- runner B: service backend / exact SCM name
                                  |      +-- runner C: owned user-session backend
                                  +--> per-runner Observe -> Decide -> Reconcile
                                         +-- scoped GitHub label admission
                                         +-- exact local Listener/Worker observation
                                         +-- durable desired state / transaction receipts
```

The local registry is **not** the cross-host v0.4 node inventory. The same workstation is one Node and contains zero or more enrolled Runner instances. Preserve the invariant `one execution identity -> one active owned work root` for each Runner; do not conflate an Agent control profile with a Runner's execution identity.

## Minimum versioned contracts

Each persisted non-secret `RunnerRecord` is a distinct object containing:

- locally generated stable `local_runner_key` (not a process ID);
- GitHub registration scope and **exact server runner ID + expected name**;
- canonical runner-home and work-root paths, and a non-secret execution-identity reference;
- backend kind: `windows-service` or `user-session`; exact Windows SCM service name or owned session launch contract;
- selector and selector-ownership metadata, if managed admission is configured;
- `desired_enabled: boolean`, `enrollment_state`, and an optional opaque credential reference;
- policy/owner provenance and schema version, without storing credential values.

Observations are reconstructed, **not persisted as truth**: local phase, active bound Worker, SCM state/start type, remote runner online/busy, selector presence, identity/ownership evidence, and resource sample. Model separately:

- `desired_enabled`: user intent, persisted;
- `desired_admission`: policy-derived eligibility;
- `desired_lifecycle`: stay running / drain and stop;
- `observed_lifecycle`: `Stopped`, `Listening`, `Busy`, `DrainPending`, `Unknown`, or `Refused`;
- `achieved_status`: only asserted after independent matching local/remote observations.

Do not represent UI checkbox acknowledgement, SCM stop-request acknowledgement, or label API response as achieved `Stopped`/safe `DRAINED`. `Unknown` is neither `Stopped` nor `SafeToStop`.

### Enrollment / discovery

- Discovery is **read-only**: scan documented exact Windows SCM Runner service identities and explicitly supplied runner homes; treat scheduled-task installations as `ObservedOnly` until a separate scoped backend is designed. Avoid broad recursive filesystem search and never read or expose runner credential files.
- Default discovered entries to `ObservedOnly`; no automatic adoption, registration, service reconfiguration, or startup changes.
- Enrollment requires a one-to-one evidence match among local home, exact service/process, work root, execution identity, server ID, scope, and capability. Duplicate homes, service aliases, foreign sessions, identity drift, or missing evidence result in `Refused`.
- A local display name or repository label is **not** mutation authority. `forget` removes only a verified owned registry record and never unregisters an official Runner.
- Never display or commit private runner names/IDs, full host topology, credentials, or private paths in the public repository.

### Independent toggles and policy precedence

```text
hard safety / ownership denial
  > global Zen or maintenance
  > runner desired_enabled = false
  > explicit host UserMode (including force-ci)
  > Auto Lite / future rich probes
```

**An explicitly disabled runner stays disabled across restart, user mode changes, Zen exit, Force-CI, and Agent updates.** `Enable` means eligible for normal policy; it does not bypass global Zen/hard safety. `Disable` persists its intent immediately, then initiates safe withdrawal/stop; `DisableAll` is a composition of exact scoped per-runner operations with per-item receipts and partial-failure visibility, not a wildcard OS command.

Desired operations: `list`, `discover`, `inspect`, `enroll`, `enable`, `disable`, `reconcile`, `forget`, and `doctor`. UI and CLI only issue typed `AgentCommand` values; the Agent owns policy and state. Show `Enabled / Disabled / Draining / Busy / Stopped / Unknown / Refused` distinctly. Never show a generic success for partial batch application.

### Safe disable / drain / stop

1. Validate exact enrollment, process/work-root ownership, backend capability, and credentials. Persist desired disablement with a durable receipt.
2. If this runner has a **qualified admission selector** used by all its intended workflows, withdraw *only that exact runner's owned selector*, read it back, and classify racing assignments as `DrainPending`. Re-observe Worker and process identity.
3. If an exact Worker is busy or identity/routing evidence is ambiguous, **do not signal or kill it**. Keep `DrainPending` / `WithdrawalBlocked` / `Refused`; permit the job to finish naturally.
4. Once the scoped route is withdrawn and an exact idle Listener is positively verified, request a narrowly scoped graceful stop through the selected backend; re-read SCM/process/remote state before claiming `Stopped`.
5. Stop when the known proof is insufficient. A GitHub label readback is not a globally linearizable assignment barrier; an observed racing assignment must defer shutdown. This is a conservative operational procedure, **not** a zero-race guarantee from GitHub.
6. For legacy workflows that only use generic labels or for which routing completeness cannot be proven, display **`UNQUALIFIED_LEGACY_ROUTING`**. Do not claim safe no-new-job drain. Offer an explicit workflow migration plan and an Owner-controlled maintenance-window operation; never silently treat a local idle check as proof no new job will arrive.

A stopped official persistent runner remains registered and is expected to show Offline on the GitHub side. Stop/disable does not delete registration, work roots, logs, settings, or secret material.

### GitHub admission selector compatibility

ADR 0004 reserves `runnermesh-admit` with **single-identity ownership per registration scope**. Multiple enrolled runners in one GitHub scope cannot all claim that same selector under the frozen v0.1 contract. A v0.2 implementation must:

- preserve v0.1 binding and its reserved label for one legacy owner;
- allow different, collision-free repository scopes without weakening ownership;
- **block** same-scope managed-admission enrollment if selectors collide until a scoped-selector extension is separately decided, implemented, and qualified;
- investigate per-runner selectors (for example a stable `runnermesh-admit-<runner-key>`) with exact ownership records and workflow migration, but **do not assume this proposal is already approved or implemented**;
- never silently rewrite workflow `runs-on`, runner groups, registrations, or unrelated labels.

Any final scoped-selector ADR must specify stable naming, uniqueness, GitHub permission scope, migration/rollback, selector ownership, same-scope test matrix, and an Owner-controlled real qualification. It applies only beyond v0.1.

### Windows execution boundaries

- **Service backend:** exact SCM service name and verified service definition, start mode, executable/image, and owning identity; use scoped SCM operations, never `Get-Service | Stop-Service` wildcards, `taskkill`, or generic process kill. Startup type changes require a transactional before/after record and restoration plan. A normal user Agent must not silently elevate itself; use only a bounded approved privilege mechanism.
- **User-session backend:** operate only on an explicitly owned, exact home, listener, session, and work root; no cross-session adoption by name. PowerShell availability is verified under the actual intended execution identity.
- **Other modes:** scheduled tasks, container/VM runners, arbitrary third-party Runner wrappers, and enterprise managed fleets are initially observation-only or out of scope.
- **Startup/recovery:** persist disabled intent, but treat prior service state and running PIDs as stale after restart; reconcile after fresh ownership observation. A service set to Automatic by another controller is configuration drift, not license to silently change an unrelated service.
- **Control plane:** the Agent remains small and can stay alive while every Listener is stopped; a production runtime is immutable and separate from a mutable development worktree.

### UI and resource evidence

First delivery: per-runner CLI plus a compact Tray submenu with individual toggles, a `Disable all` confirmation, state/reason details, and CN/EN text. No v0.1 full desktop GUI is implied. Resource display is read-only, best-effort per exact process tree with sample time and attribution limits; it is **not** a precise measured resource savings claim. The future v0.2 native resource-policy backend is a separate Goal.

## Safety and test obligations

- Registry schema migration v1(single binding) -> v2(records) is explicit, reversible when safe, and does not invent multiple old records; rollback to v0.1 preserves the original usable binding.
- One Runner's disable/failed transaction must not mutate another Runner's service, label, work root, identity, or persisted intent.
- No repeat start after disabled intent; no dual Agent authority; no cross-identity shared active work root.
- Unit/model tests: disconnected GitHub, duplicate name, duplicate home, same-scope selector collision, stale PID, unrecognized service, user-session mismatch, busy Worker, racing assignment, crash between mutation/readback, and interrupted persistence.
- Integration tests: fake SCM and synthetic user-session lifecycle, typed IPC and tray/CLI parity, install/update rollback migration, service restart, suspend/resume, and partial batch failure.
- Real qualification needs separate **Owner Gate** for SCM changes, label/workflow migration, credentials, and production host mutation. Verify both safe operation and original-state restoration independently.
- Public repository contains only synthetic identities and fixtures; no private dogfood evidence.
- Gates: `R` runner control, `S` persistent config, `V` tray, `X` trust/privilege, and `I` if installed-runtime migration is changed; reuse unchanged tests per `QUALITY_GATES.md`.

## Deferred alternatives and constraints

- Do not run N full Agent installations for N runners: that would duplicate user UI/IPC and risk ownership drift. Use one Agent with N scoped records.
- Do not force all users to convert existing Windows Services into user-session runners.
- Do not solve legacy routing by cancelling active jobs or deleting registrations.
- Do not build a bespoke scheduler, generic Windows service manager, local broker, or remote dashboard for this Goal.
- Cross-host discovery/placement belongs to v0.4, not this local module.

## Acceptance / supersession

This ADR is an **implementation proposal**, not evidence that any per-runner service control currently works. Before coding a service/label mutation backend, finalize its exact authority and qualified selector decision, with a focused amendment if implementation evidence forces a change. The frozen v0.1 contract and ADR 0004 remain normative for v0.1.
