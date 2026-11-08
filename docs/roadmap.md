# RunnerMesh product roadmap — v0.1 through v1.0

> **Planning roadmap, not a claim that future features are implemented.** For the detailed version-by-version implementation checklist, dependencies, estimates, and Cursor/AGY handoff, use [the end-to-end delivery plan](../goals/RM-END-TO-END-DELIVERY-PLAN.md). Current v0.1 execution truth remains in [the execution ledger](../goals/RM-V0_1-EXECUTION-STATUS.md) and [accepted Roadmap v3](../goals/RM-V0_1-ROADMAP.md).

Planning update: **2026-11-08**. This update adds the Owner-requested [Local Runner Manager](../goals/RM-LOCAL-RUNNER-MANAGER-001.md) as the first post-v0.1 feature; it does **not** amend the frozen [v0.1 design](v0.1-design-freeze.md) or approve real host mutation. The detailed design proposal is [ADR 0005](adr/0005-local-multi-runner-lifecycle.md).

## Why these stages

RunnerMesh contributes workstation capacity to GitHub Actions while giving priority to the human owner. It does not replace GitHub's workflow scheduler, Runner protocol, CI logs, or artifacts. A workstation being online does not make it available for CI. The releases progress in four capability layers:

1. **Correct local control:** first one existing official Runner (v0.1), then independently enabled/stopped N local Runner instances (v0.2-A).
2. **Safe local resource coexistence:** Windows enforcement (v0.2-B), richer activity policy (v0.3).
3. **Distributed opted-in supply:** first Mesh (v0.4), dynamic capacity (v0.5), capability qualification (v0.6).
4. **Extended trust/platform/integration:** isolated execution (v0.7), Linux (v0.8), outbound broker/scale sets (v0.9), sustained v1.0 GA qualification.

Never use the word `FULL`, `DRAINED`, `THROTTLED`, or `Stopped` as a substitute for the exact achieved evidence specified by the active version. Managed admission requires a qualified exact GitHub selector; generic-label jobs are outside the v0.1 label-withdrawal guarantee.

## P0 and v0.1 — Single Windows workstation, one exact Runner

**Status: pre-v0.1 / release hardening.**

Accepted source includes the Agent Core, Named Pipe IPC, CLI, native Tray and CN/EN/theme preferences, probes and Auto Lite, exact-runner observation, dynamic native-label admission and two-phase withdrawal, H1 real qualification, and G12-G15 autostart/immutable-install/update/rollback/packaging foundations. The original G11 design was superseded by ADR 0004; the exact reserved selector is `runnermesh-admit`.

The public ledger continues to show a real H2 cutover failure with successful restoration, a superseded G15R RC, and **requalification required**. The narrow BOM compatibility fix was merged as PR #49 on the planned baseline, but that is not a fresh integrated RC or H2 PASS.

Remaining authoritative sequence (F3-F5, not a reopened architecture effort):

- [ ] Requalify one immutable integrated G15R RC and bind its exact HEAD/checksum/provenance.
- [ ] Perform an explicitly Owner-authorized H2 real cutover and independently prove restoration when needed.
- [ ] Complete at least 24 hours of normal workstation dogfood.
- [ ] Complete G17 RC/release closeout; publish `v0.1.0` at an explicit H3 Owner gate.
- [ ] Stop the existing v0.1 Master Goal after publication; post-v0.1 work requires a new scoped implementation Goal.

No multi-runner service management, general Windows Service helper, real Job Object resource enforcement, generic broker, or full GUI is included in v0.1.

## v0.2 — Local Runner Manager + Windows resource policy

### v0.2-A — RM-LOCAL-RUNNER-MANAGER-001 (highest-priority new user value)

**Intent:** a single workstation may contain multiple already-registered official GitHub self-hosted runners. Give the owner explicit independent per-runner enabled/disabled state, safe drain then stop, persistent startup policy, and lower idle Listener overhead. **Not yet implemented.**

- [ ] Exact read-only inventory and explicit enrollment for service-backed and owned user-session runners; no opportunistic adoption of foreign instances.
- [ ] Versioned local registry and per-runner desired/observed/achieved state without changing the v0.1 release contract.
- [ ] Per-instance CLI/Tray toggles, `disable all` with per-item receipts, CN/EN state and Doctor explanations.
- [ ] Exact Windows Service and user-session lifecycle adapters with strict identity/work-root checks and bounded, reversible privileged operations.
- [ ] Job-safe withdrawal for qualified managed selectors; legacy generic-label workflow limitation visibly blocked or resolved through separately authorized migration.
- [ ] v0.1 label-ownership compatibility: same-scope collisions must fail closed until a reviewed per-runner selector namespace/migration decision is accepted.
- [ ] Persist disablement across restart, regardless of Auto/Force-CI/Zen exit; do not unregister or kill an active Worker.
- [ ] Synthetic multi-runner tests, selected real Owner-gated qualification, restoration and dogfood.

Source-only discovery/contract work may be prepared in parallel with v0.1 closeout **in an isolated Draft PR**. No live Service/GitHub mutation is implied.

### v0.2-B — Native Windows resource coexistence

- [ ] Exact owned Worker process-tree binding and Windows Job Object capability model.
- [ ] Bounded CPU/memory native policy profiles, EcoQoS where supported, foreground-friendly control.
- [ ] Recovery after restart/suspend, unattachable/unsupported backend reporting, explanation and measured overhead.
- [ ] One scoped authorized real-workload verification; foreground safety and non-destructive Worker behavior remain invariants.

`THROTTLED` receives an enforced meaning only when a backend actually proves it. v0.2-A can be demonstrated and Owner-dogfooded as an isolated feature candidate, but publish a stable v0.2 release only after the explicitly chosen v0.2 release gate has accepted its intended scope.

## v0.3 — Rich automatic admission

- [ ] Normalized activity/foreground, GPU, battery/AC and other relevant latency evidence where supported.
- [ ] Confidence/freshness, hysteresis and cooldown to prevent policy flapping.
- [ ] Deterministic Unknown/Unavailable/Suspended semantics and mode/Zen/per-runner precedence tests.
- [ ] Profile explanations, manual overrides, and normal-use workstation dogfood.

## v0.4 — First distributed Mesh

- [ ] Multi-node opted-in inventory, trustworthy node identity/presence and capability declaration.
- [ ] Outbound-first node connectivity; separate Node identity from local Runner instance identity.
- [ ] Shared eligible capacity with hard requirements, soft preferences, safe fallback and expiration of stale observations.
- [ ] Node disconnect/reconnect/sleep/restart/identity-drift recovery.
- [ ] Real two-node trusted GitHub-native workflow qualification; never replace GitHub's scheduler.

## v0.5 — Dynamic capacity

- [ ] Versioned supply declarations, bounded contribution budgets and policy-driven dynamic changes.
- [ ] Demand/capability hints without hijacking GitHub job routing.
- [ ] Recoverable reconciliation under concurrent changes, lease expiry and node loss.
- [ ] Conservative testing of busy/idle/drain/foreground transitions and rate/capacity caps.

## v0.6 — Machine qualification

- [ ] Repeatable host/platform/toolchain capability evidence with provenance and freshness.
- [ ] Declared workflow requirements mapped to supported Runner/node capabilities.
- [ ] Eligibility checks, invalidate-on-drift semantics and fail-closed unknown results.
- [ ] Synthetic multi-node matrix and scoped real host checks.

## v0.7 — Isolated or disposable execution

- [ ] Explicit trust model for untrusted workloads and a qualified supported isolation backend.
- [ ] Ephemeral identities/storage/network, reproducible cleanup and interruption recovery.
- [ ] Credential and host-escape threat review; independent focused security acceptance.
- [ ] Authorized isolated pilot; personal persistent hosts never automatically accept untrusted public-fork code.

## v0.8 — Linux backend

- [ ] systemd-based local lifecycle and work-root ownership appropriate to the execution identity.
- [ ] cgroup v2 and other supported native resource-policy capabilities; truthful platform differences.
- [ ] Shared typed policy/Agent interfaces, portable configuration/CLI and packaging/rollback.
- [ ] Linux trusted dogfood and Windows regression gates.

## v0.9 — Outbound broker and scale-set integration

- [ ] Authenticated outbound-only capacity/policy/capability/lifecycle control plane with scoped privileges.
- [ ] Node/session leases, reconnect/partition/replay handling, recovery and cost/capacity caps.
- [ ] Opt-in GitHub-native JIT/ephemeral registration and supported scale-set/ARC integration.
- [ ] Distinct separation of persistent personal-workstation vs ephemeral execution trust.
- [ ] Independent authorization/security review and bounded multi-node acceptance.

## v1.0 — Workstation-aware CI Mesh GA

- [ ] Versioned stable public CLI/config/IPC and migration/rollback contracts.
- [ ] Complete first-run/operator docs, actionable Doctor, i18n and accessible daily controls.
- [ ] Measured idle overhead, resource enforcement and foreground responsiveness with reproducible methodology.
- [ ] Representative Windows + Linux, multi-node + broker failover, real trusted workflow qualification.
- [ ] Independent trust/privacy/security review and release/provenance verification.
- [ ] Sustained fleet dogfood, no known release-blocking P0/P1 defects, Owner-approved `v1.0.0` publication.

**Definition of complete:** v1.0 meets the explicitly supported platform/feature matrix and the evidence above; it is not a claim of general-purpose scheduling, universal Windows/Linux feature parity, or arbitrary untrusted-workload safety.

## Engineering governance and estimates

Use [AGENTS.md](../AGENTS.md), [QUALITY_GATES](../dev_governance_files/QUALITY_GATES.md), and [full delivery plan](../goals/RM-END-TO-END-DELIVERY-PLAN.md). One active Implementer owns each branch; Cursor may write, AGY reviews a settled candidate read-only; no simultaneous two-writer edits. Real service, runner registration, label/secret, production install/autostart, and release actions require explicit prepared Owner gates. Risk-selected tests and one settled candidate hosted CI replace routine repeated full-suite audits.

The indicative **total** for v0.1 remaining -> v1.0 is **473–891 active engineering hours** plus real dogfood elapsed time and Owner waits; the immediate v0.1 + Local Runner Manager corridor is **93–180 active hours**. Estimates are not acceptance evidence and should be recalibrated after the first live local-runner pilot.
