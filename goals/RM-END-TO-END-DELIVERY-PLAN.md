# RunnerMesh end-to-end product delivery plan (v0.1 -> v1.0)

- Status: **PLANNED portfolio roadmap and implementation checklist**, not an execution receipt or grant of production authority
- Planning baseline: 2026-11-08, authoritative `main` `6a6543162b519f213549fc1f57aaadc19acdc36c`
- Scope: single-host v0.1 closeout; [Local Runner Manager](RM-LOCAL-RUNNER-MANAGER-001.md); Windows resource policy; rich admission; first multi-node Mesh; dynamic capacity; qualification; isolated execution; Linux; outbound broker/scale sets; v1.0 GA
- Truth: [v0.1 status ledger](RM-V0_1-EXECUTION-STATUS.md) governs v0.1 acceptance; [v0.1 Master Goal](RM-V0_1-FINAL-AUTONOMOUS-CLOSEOUT.md) governs its remaining run; [public roadmap](../docs/roadmap.md) summarizes versions
- Explicitly **no new authorized Goal Train for implementation** is created by this planning PR; each leaf still requires accepted scope, starting SHA, and the applicable gates

## 1. Delivery strategy and priority

**Ship order:** finish v0.1 exactly as frozen; immediately deliver the high-value per-runner local switch workflow as **v0.2-A**; complete Windows resource enforcement as **v0.2-B**; then v0.3 -> v0.4 -> v0.5 -> v0.6 -> v0.7 -> v0.8 -> v0.9 -> v1.0.

**Safe concurrency:** source-only LRM-00/01 may be prepared on a separate branch before v0.1 release. A prepared feature PR may be Draft; do **not** mutate a real Service/runner or merge an incompatible runtime/config path into v0.1's frozen release candidate. Compatible docs and pure test fixtures do not prove runtime compatibility.

**Optimization:** prioritize useful operating capability over speculative frameworks, whole-project reruns, extra CI, premature dashboards, and generic broker architecture. Reuse the official GitHub Actions protocol/queue. Reuse accepted evidence on empty risk diff. Stop when a real Owner Gate arises; do not quietly translate an Owner gate into an automated action.

## 2. Current reality / carryover

- Accepted v0.1 source: foundation G01-G10, G11R-A/B, H1 proof, and G12-G15 productization, subject to the actual execution ledger.
- `main` includes PR #49's narrowly scoped official `.runner` BOM compatibility fix, but the public v0.1 ledger **still** marks G15R `REQUALIFICATION_REQUIRED`, H2 `FAIL_POST_MUTATION` with restoration `PASS`, and the old RC as superseded. Do not infer that merged source + green CI equals a freshly qualified immutable RC or H2 authorization.
- The first H2 authorization was consumed. All subsequent real cutovers require a **new** exact Owner plan and token. Never replay private transaction data or expose private runner identities.
- Historical draft PRs #14-#16 are selective source references, not live merge candidates.
- v0.1 must stop after approved stable publication; starting v0.2 is a **separate** development route, not implicit permission from H3.

## 3. Macro milestones, dependencies and exit signals

| Stage / deliverable | Dependency | Target proof / release gate | Active estimate |
|---|---|---|---:|
| **C0: v0.1 closeout** | authoritative main and [F3-F5](RM-V0_1-ROADMAP.md) | fresh G15R RC + H2 PASS/restore + >=24h real dogfood + G17 + H3 Owner publication | **18–40 h** |
| **C1: v0.2-A Local Runner Manager** | v0.1 compatibility; selector/authority decisions | N independent exact local runners; true per-instance enable/disable, safe stop, persistent intent, CLI/Tray, live Owner proof | **75–140 h** |
| **C2: v0.2-B Windows resource policy** | C1; process ownership proof | Job Objects/EcoQoS/policy envelopes, foreground protection, measured tests | **35–65 h** |
| **C3: v0.3 rich Auto** | C2 policy controls | activity/GPU/AC signals, hysteresis, explainable safety and deterministic decisions | **24–46 h** |
| **C4: v0.4 First Mesh** | C1/C3; architecture and data-plane review | two or more opted-in nodes, presence/capability, safe placement preferences and recovery | **50–95 h** |
| **C5: v0.5 dynamic capacity** | C4 | bounded supply changes, lifecycle reconciliation and failure recovery | **36–65 h** |
| **C6: v0.6 machine qualification** | C4/C5 | capability proof, eligibility matrix, invalidation and repeatability | **30–50 h** |
| **C7: v0.7 isolated execution** | C6; security decision | verified disposable backend, trust split, no untrusted job on persistent host by default | **55–105 h** |
| **C8: v0.8 Linux backend** | stable typed contracts C4/C6 | systemd/cgroup semantics with honest capability gaps, Linux dogfood | **45–85 h** |
| **C9: v0.9 broker + scale sets** | C4/C5/C6/C8; security review | outbound-only control, lease/recovery, GitHub-native JIT/scale-set adapter with opt-in | **65–125 h** |
| **C10: v1.0 GA** | C0-C9 release-quality exits | security/usability/ops documentation, version migration, multi-platform release/rollback, sustained fleet dogfood | **40–75 h** |
| **TOTAL** | sequential critical contracts, some parallel source packages | indicative **473–891 active hours**; elapsed Owner waits and dogfood windows extra | **473–891 h** |

The range is a **planning estimate**, not measured developer hours, a promise of uninterrupted agent execution, or a cost/token forecast. It includes ordinary focused implementation, review, fixtures, integration and release preparation; excludes new major requirements, upstream platform surprises, host outages, and elapsed waits. Re-estimate after C0 and first C1 pilot. A second read-only reviewer reduces defect risk but does not make hours directly additive or cut the critical path by 50%.

**First visible user-value gate:** C0+C1 = **93–180 active hours**, plus v0.1's >=24-hour ordinary-use dogfood elapsed interval and Owner cutover availability.

## 4. Stage-by-stage work breakdown

### C0 — Close v0.1 without scope expansion

- [ ] **C0-01** Refresh `main`, protected PR status, and v0.1 execution ledger; distinguish PR #49 source fix from accepted RC evidence.
- [ ] **C0-02** Requalify **one** exact immutable G15R RC on current accepted source; package/hash/provenance/sandbox install/Agent-Tray-CLI-IPC/BOM regression/update-rollback; accept only selected risk gates.
- [ ] **C0-03** Re-check security and readbacks on that exact RC; `H2_RC_READY=true` only with independent accepted evidence.
- [ ] **C0-04** Show new H2 plan, exact immutable artifact, rollback, and Owner gate; do not touch real runner or service before authorization.
- [ ] **C0-05** Qualify real cutover; H2 success and restoration tracked independently; on failure restore before another step.
- [ ] **C0-06** Minimum >=24-hour ordinary-use dogfood; do not shorten or manufacture timestamps.
- [ ] **C0-07** G17 freeze, clean release notes, provenance/hash/readback; separate H3 publication authorization; stop at v0.1.0.
- **Exit:** `G15R=PASS; H2=PASS; DOGFOOD>=24h PASS; G17=PASS; H3=PASS; v0.1.0 RELEASED`. If not, report accurate `WAITING_FOR_OWNER` / `BLOCKED` / `UNPROVEN`.

### C1 — RM-LOCAL-RUNNER-MANAGER-001 (v0.2-A)

Detailed owner, files, cases and estimates: [Goal specification](RM-LOCAL-RUNNER-MANAGER-001.md); design: [ADR 0005](../docs/adr/0005-local-multi-runner-lifecycle.md).

- [ ] **LRM-00** Finalize selector collision strategy and scoped authority/privilege contract; preserve v0.1 frozen ADR 0004.
- [ ] **LRM-01** Read-only exact local discovery (SCM/User Session); unsupported modes observation-only; do not scan secrets.
- [ ] **LRM-02** Versioned registry, exact enrollment and safe v0.1 binding migration; durable per-runner intent and rollback.
- [ ] **LRM-03** Typed AgentSnapshot/AgentCommand/IPC/CLI with per-runner actions and stable JSON.
- [ ] **LRM-04** Real Service backend **behind Owner gate**: exact SCM binding, narrow start/stop/startup settings, receipts, restoration.
- [ ] **LRM-05** Owned User Session backend, correct execution identity/work root, no foreign process adoption.
- [ ] **LRM-06** Independent policy/Zen/per-runner ordering, owned-label readback, busy/racing jobs, legacy generic-label refusal and partial failure.
- [ ] **LRM-07** Tray submenu, CN/EN, state clarity, resource samples, `disable all` with each result visible.
- [ ] **LRM-08** Windows synthetic matrix, focused independent privilege/security review, exact real Owner qualification/restore, versioned release/dogfood.
- **Exit:** two or more exact enrolled instances can be independently switched with one Agent; no cross-runner mutation, no destructive active-job termination, reboot preserves disabled state; legacy unsafe states visibly blocked.

### C2 — Windows resource policy (v0.2-B)

- [ ] **RES-01** Exact Runner.Listener / Runner.Worker process-tree identity and immutable ownership proof; resource capability model.
- [ ] **RES-02** Windows Job Objects or other OS-supported containment with strict attach/escape semantics and recovery; never claim ownership of foreign processes.
- [ ] **RES-03** Bounded CPU/memory/power profiles, EcoQoS where supported; compatibility detection; `Unsupported` is honest.
- [ ] **RES-04** Foreground-friendly controller; manual and hard-safety precedence; throttle decisions are separate from admission/stop.
- [ ] **RES-05** Test against synthetic and one authorized real job; prove no active job cancellation, bounded impact, and recovery after Agent restart.
- [ ] **RES-06** Benchmark cold/idle Agent overhead, idle Listener memory, policy latency, foreground responsiveness; report platform/version/test methodology rather than fabricated numbers.
- **Exit:** `THROTTLED` has an observed enforced resource meaning on supported Windows backend; unsupported capability never masquerades as enforced.

### C3 — Rich automatic admission (v0.3)

- [ ] **AUTO-01** Normalized foreground activity and latency-sensitive workload evidence.
- [ ] **AUTO-02** GPU/battery/AC/thermal observations where supported; timestamp, freshness and reliability levels.
- [ ] **AUTO-03** Profile policy matrix for Auto Lite successor with hysteresis, debounce/cooldown, and human override.
- [ ] **AUTO-04** Unknown/Unavailable/Suspended/freshness tests; fail closed for CI admission.
- [ ] **AUTO-05** Explanation/reason codes in CLI/Tray; owner-level control of probes and retention.
- [ ] **AUTO-06** Focused read-only workstation profiling and dogfood, no surprise background job start.
- **Exit:** repeatable mode transitions without flapping; manual disable still beats `force-ci` and Auto.

### C4 — First Mesh (v0.4)

- [ ] **MESH-01** Separate `NodeIdentity` from `RunnerIdentity`; define machine-local registry vs cross-host inventory.
- [ ] **MESH-02** Choose/prove an outbound node presence transport and explicit trust bootstrap (ADR). No inbound workstation port dependency.
- [ ] **MESH-03** Capability declarations/versioned snapshots with freshness/expiry and offline semantics.
- [ ] **MESH-04** Hard requirements and soft preferences mapped to GitHub-native placement/admission, not a replacement scheduler.
- [ ] **MESH-05** Fleet view (read-only first): 2+ nodes, runner capacity, global/manual node intent, offline and stale data.
- [ ] **MESH-06** Node-loss/rejoin/duplicate identity/sleep/wake recovery, key rotation and isolated-machine failure fixtures.
- [ ] **MESH-07** Authorized 2-host trusted workflow pilot, independent readbacks, no foreign Runner registration changes.
- **Exit:** two opted-in workstations safely contribute and withdraw capacity; node disappearance does not confer zombie eligibility or destroy an active job.

### C5 — Dynamic capacity (v0.5)

- [ ] **CAP-01** Capacity declaration and demand hint contracts; separate GitHub actual scheduling from RunnerMesh's contribution decisions.
- [ ] **CAP-02** Per-node and per-runner contribution budgets with soft placement and deterministic preference evaluation.
- [ ] **CAP-03** Capacity reconciliation with lease/expiry/duplicate event handling; no unlimited autoscale loops.
- [ ] **CAP-04** Busy-to-drain, demand surge, machine foreground takeover, and partial-node outage scenarios.
- [ ] **CAP-05** Operator reason-codes, limits, rollback and measured responsiveness.
- **Exit:** changing fleet capacity does not exceed opted-in budgets or turn an uncertain runner into `FULL`.

### C6 — Machine qualification (v0.6)

- [ ] **QUAL-01** Versioned capability schema and evidence source contract (OS, architecture, toolchain, hardware, permissions).
- [ ] **QUAL-02** Read-only qualification probes with expiry and error provenance; no arbitrary injected command execution.
- [ ] **QUAL-03** Eligibility matching against declared workflow requirements and hard safety/trust boundaries.
- [ ] **QUAL-04** Qualification drift, invalidation, rollback and operator override boundaries.
- [ ] **QUAL-05** Synthetic multi-platform matrix plus authorized representative machines.
- **Exit:** a node is eligible only with sufficiently fresh, qualifying evidence.

### C7 — Isolated / disposable execution (v0.7)

- [ ] **ISO-01** Threat model review: trusted persistent host vs untrusted public PR execution; explicitly keep untrusted workflows off a personal persistent host by default.
- [ ] **ISO-02** Choose a supported isolation backend (VM/container/disposable worker) and scope host/sandbox authority in ADR.
- [ ] **ISO-03** Ephemeral identity/storage/network policy, cleanup and artifact boundaries without replacing GitHub's job protocol.
- [ ] **ISO-04** Recover and garbage-collect after interrupted run; deny reuse of contaminated or unverifiable state.
- [ ] **ISO-05** Isolation escape / credential exposure / stale worker tests and focused independent security review.
- [ ] **ISO-06** Approved real isolated execution pilot; default fail closed if isolation unavailable.
- **Exit:** evidence supports the exact workload trust class; no blanket claim that all untrusted code is now safe.

### C8 — Linux backend (v0.8)

- [ ] **LNX-01** Map Windows semantic contracts to systemd-user/headless process and identity model; no fake cross-platform equivalence.
- [ ] **LNX-02** cgroup v2 delegated resource limits and supported optional capabilities; fail/unknown on missing delegation.
- [ ] **LNX-03** Local Linux official Runner observation, safe drain/stop, work-root permissions, user/service distinction.
- [ ] **LNX-04** Tray alternatives/CLI and localization with shared typed Agent; no fabricated Windows UI support.
- [ ] **LNX-05** Cross-OS registry/IPC/persistence/version migration tests and install/rollback.
- [ ] **LNX-06** At least one opt-in Linux live trusted qualification and ordinary-use dogfood, keeping Windows regressions green.
- **Exit:** Linux honestly reports capability support and matches safety/invariants of Windows within documented limits.

### C9 — Outbound broker and scale-set integration (v0.9)

- [ ] **BROKER-01** Threat/data-boundary ADR: broker only carries authenticated capacity, policy, capability and lifecycle metadata.
- [ ] **BROKER-02** Outbound node sessions, identity/auth rotation, leases, bounded queues, replay and restart recovery.
- [ ] **BROKER-03** Multi-node network partition, credential compromise, stale lease and offline-state fixtures.
- [ ] **BROKER-04** Opt-in GitHub-native JIT/ephemeral runner registration adapter, distinct from persistent v0.1/v0.2 lifecycle.
- [ ] **BROKER-05** Integrate compatible GitHub scale-set/ARC capability through documented APIs rather than replacing runner protocol.
- [ ] **BROKER-06** Capacity ceilings, cost/overload guardrails, clear operator controls, no hidden privilege escalation.
- [ ] **BROKER-07** Independently review trust/registration/security and run a bounded fleet pilot with explicit Owner authority.
- **Exit:** outbound-only broker survives partition/reconnect and preserves trusted vs ephemeral scope separation.

### C10 — v1.0 general availability

- [ ] **GA-01** Freeze versioned public API/CLI/config/reason-code contracts and record migrations from v0.1 through v1.0.
- [ ] **GA-02** Complete operator experience: first-run setup, exact enrollment, status, Doctor, Tray/CLI, CN/EN, accessible error/help, documented rollback.
- [ ] **GA-03** Platform/version capability matrix; real multi-node Windows + Linux acceptance including no inbound workstation dependency.
- [ ] **GA-04** Long-lived fleet dogfood and failure-injection: restart, sleep/resume, broker loss, Worker busy, upgrade/rollback, selector drift, race handling.
- [ ] **GA-05** Independent focused security/privacy audit, secrets handling, untrusted-workload restrictions, public artifact scan.
- [ ] **GA-06** Reproducible releases/signing or provenance path, exact hashes, SBOM where appropriate, release notes, supported upgrade/rollback paths, runbooks.
- [ ] **GA-07** Document measured CPU/memory overhead, foreground responsiveness, workload eligibility and resource enforcement with reproducible fixtures, confidence and limitations.
- [ ] **GA-08** Zero known release-blocking P0/P1 issues, complete required Owner gates, stable published `v1.0.0` only after explicit Owner publication approval.
- **Exit:** a maintainable, installable, independently qualified workstation-aware CI mesh with truthful capability limits, not an assertion that every optional backend is universally supported.

## 5. Critical path and optional accelerators

**Critical path:** accepted v0.1 immutable RC -> authorized H2 -> >=24h dogfood -> H3 -> LRM exact identity/selector contract -> per-runner safe lifecycle -> Windows resource policy -> trusted two-node proof -> capacity/qualification -> isolation/Linux/broker gates -> v1.0 acceptance.

Allowed overlap, without shared writer state:

- C0 dogfood wall time: draft LRM pure contracts/read-only discovery and later release docs in isolated branch.
- C1 implementation: AGY read-only review of settled Cursor diff; never simultaneous writes to Cursor's worktree.
- C3 signals and C4 inventory can have separate implementations only after shared typed contracts are frozen and branches have distinct file ownership.
- Linux backend investigation and isolation threat modelling can begin early as documents, but cannot count as real acceptance.

Avoid premature coupling among FleetSplice, coordination tools, personal infrastructure, generic remote desktop, private service orchestrators, or new CI scheduler. RunnerMesh must deliver standalone value.

## 6. Team roles and review discipline

| Role | Responsibilities | Explicit limit |
|---|---|---|
| **Owner / architect** | Prioritize goals, settle contract and scope, approve service/trust/production/release gates, review receipts | Architecture discussion alone does not authorize host mutation |
| **Cursor Desktop** | **Sole branch/worktree Implementer**; source, synthetic fixtures, focused tests, candidate PR, bounded repairs | No unapproved real service/runner/secret/installed-runtime mutation |
| **AGY** | Independent read-only design/code/evidence review; file/line findings and risk-specific audits on settled SHA | No simultaneous write to Cursor branch; no automatic merge or hidden remediation |
| **GitHub Actions** | Hosted settled-head candidate checks, docs Fast Gate, Windows/Ubuntu code CI as selected | CI PASS does not prove H2/Owner live qualifications |

Each leaf should contain the minimum contract: Goal ID, authoritative main SHA, allowed files, prerequisites, risk vector, acceptance evidence, non-goals, Owner gates, stop conditions, and concise receipt. Gate selection is owned by [QUALITY_GATES](../dev_governance_files/QUALITY_GATES.md). A risk-diff-empty gate can be `REUSED`; no routine full-history auditor or duplicated CI suite.

After one accepted PR merges, read back remote `main` and wait for the required risk-appropriate main health before the **next** PR merge. A source-only blocked lane remains Draft, not forced into main.

## 7. Initial Cursor sprint (safe, immediately executable)

**GOAL_ID=`RM-LOCAL-RUNNER-MANAGER-001/LRM-01-DISCOVERY-SOURCE-ONLY`**

- [ ] Clone/check local worktree under a clearly owned development path; confirm toolchain and authoritative main; never reset unrelated changes.
- [ ] Read `AGENTS.md`, `QUALITY_GATES.md`, v0.1 ledger/freeze, ADRs 0004/0005, LRM Goal and this plan.
- [ ] Produce a short design reconciliation of current `src/runner_observer.rs`, `src/windows_supervisor.rs`, `src/runtime_binding.rs`, `src/agent_runtime.rs`.
- [ ] Implement pure `DiscoveredRunner`/backend type, read-only interface and injected SCM/session fakes (synthetic fixtures) **without adding any Service control** or changing v0.1 on-disk binding.
- [ ] Focused tests: 0/1/N inventory, exact identity collision, unknown/foreign session refusal, two runners same scope, no secret/worker access.
- [ ] Run targeted `cargo test` and `cargo fmt --check`, selected docs/code fast gate; do not run redundant hosted CI until candidate settled.
- [ ] Ask AGY to review exact settled candidate read-only for identity/safety/privacy; Cursor alone fixes concrete findings.
- [ ] Push one bounded **Draft PR**; mark `PRODUCTION_MUTATION=false`, `REAL_RUNNER_TEST=N/A`; leave v0.1 protected release and real services untouched.
- [ ] Summarize diff, checks, candidate SHA, AGY findings, remaining selector authority question and next safe Goal.

**Do not proceed to LRM-04 Service mutation, LRM-06 GitHub label mutation, host startup change, credential setup, or any production cutover without an explicit prepared Owner transaction.** The first sprint is valuable even if v0.1 dogfood is pending.

## 8. Program done definition

`RUNNERMESH_COMPLETE` refers only to the **agreed v1.0 scope** above, not every imaginable CI provider or remote orchestration system. Count a stage DONE only when: its contract is implemented, relevant deterministic fixtures pass, risk-selected hosted CI and independent review are complete, real Owner qualifications are accepted where required, docs/receipts are updated, and the release artifact is signed off. No unchecked item is silently interpreted as PASS.
