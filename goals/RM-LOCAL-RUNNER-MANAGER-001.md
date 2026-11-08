# RM-LOCAL-RUNNER-MANAGER-001 — Local multi-instance Runner control

Status: **PLANNED / source preparation permitted; real Runner/Service mutations NOT authorized**

Owner intent (2026-11-08): on a Windows workstation hosting multiple existing GitHub Actions self-hosted runners, allow the user to select which exact instances are enabled or stopped, reduce idle Listener/Service overhead, and preserve normal jobs and registrations.

## Position in the Program

- **Do not amend or block frozen v0.1**: the [remaining v0.1 Master Goal](RM-V0_1-FINAL-AUTONOMOUS-CLOSEOUT.md), [v0.1 execution ledger](RM-V0_1-EXECUTION-STATUS.md), and [v0.1 contract](../docs/v0.1-design-freeze.md) remain authoritative.
- Deliver this as the **first post-v0.1 workstream (v0.2-A)**. The original v0.2 resource-policy stage becomes v0.2-B, without renumbering later planned versions.
- Source-only design, discovery, typed contracts, and synthetic tests can be prepared in an isolated branch before v0.1 publication. Do not merge incompatible runtime behavior or start live qualification ahead of accepted v0.1 release/explicit Owner decision.
- Read [proposed ADR 0005](../docs/adr/0005-local-multi-runner-lifecycle.md) and [full delivery plan](RM-END-TO-END-DELIVERY-PLAN.md) first. ADR 0004 still governs v0.1 single-runner admission.

## Acceptance-level product requirements

- [ ] List all *discovered* official local runner candidates read-only, with distinct `ObservedOnly` / `Enrolled` / `Refused` statuses; never pretend discovery is exhaustive.
- [ ] Enroll only exact verified, explicitly approved Runner identities (registration scope + numeric server ID, home, work root, execution identity, backend, service/session binding).
- [ ] Persist individual desired enabled/disabled switches and restore intent across Agent restart, Windows restart/suspend, updates, and mode/Zen transitions.
- [ ] Run one authoritative per-user Agent with N per-runner records; the CLI and Tray present the same authoritative snapshot.
- [ ] Enable or disable one exact runner without changing any other runner's service, label, binding, or work root; support bounded `disable all` with per-item receipts.
- [ ] Disable prevents *new managed jobs* through a qualified exact selector when available; busy and racing jobs finish naturally; only then is a narrow graceful stop attempted.
- [ ] No destructive active-Worker stop; no false claim that generic-label legacy workflows have safe drain; blocked state is explicit.
- [ ] Support Windows Service and owned user-session backends with separate capabilities; unsupported scheduled-task/foreign-session runners remain observation-only.
- [ ] Restore user intent on reboot without automatic startup of manually disabled runners, even under Force-CI/Auto or when Zen is cleared.
- [ ] Show sampled per-instance Listener/Worker memory consumption and aggregate samples as estimates, explicitly timestamped; do not promise zero idle overhead or exact savings.
- [ ] Chinese/English Tray submenu and CLI help, stable nonlocalized JSON/reason codes, concise Doctor diagnostics.
- [ ] Real live proof and baseline restoration occur only at the exact approved Owner gate after source/sandbox acceptance.

## Implementation work packages (one active writer per worktree)

The below are bounded engineering estimates, **not completed work**. Hours are active development/review/verification estimates, excluding Owner wait, real-machine service change windows, and mandatory dogfood elapsed time. Reuse accepted unchanged-risk evidence.

| Leaf Goal | Focus / likely files | Deliverable and primary acceptance | Estimate |
|---|---|---|---:|
| **LRM-00** Architecture/authority freeze | ADR 0005, registry and selector decision; `docs/` | Resolve same-scope selectors and legacy-label risk before any mutating backend; versioned types and threat review agreed | 4–7 h |
| **LRM-01** Read-only inventory | `src/runner_observer.rs`, new registry/discovery adapter | Typed candidates; synthetic SCM/home fixtures; duplicates/foreign sessions/unsupported modes refused | 7–12 h |
| **LRM-02** Registry/persistence | `src/runtime_binding.rs`, `src/agent.rs`, new registry module | Versioned exact identity; atomic per-record intent; v0.1 migration and rollback fixtures; no credential leakage | 9–17 h |
| **LRM-03** Commands/CLI/IPC | `src/model.rs`, `src/ipc.rs`, `src/cli.rs`, `src/agent.rs` | `list/discover/enroll/enable/disable/status/reconcile/forget` contract; stable JSON; no side effects from read-only queries | 7–12 h |
| **LRM-04** Windows Service backend | new scoped SCM adapter and supervisor | Exact service ownership; startup-mode repair/rollback; busy/unknown refuse; no broad elevation or wildcard stop | 12–23 h |
| **LRM-05** User-session backend | `src/windows_supervisor.rs`, session evidence | Owned home/session/work-root launch/adopt/graceful-stop; no cross-identity root reuse | 7–13 h |
| **LRM-06** Multi-admission reconciliation | `src/admission.rs`, `src/agent_runtime.rs` | Per-runner policy and selector lifecycle, legacy blocked states, same-scope collision refusal, crash/readback/race fixtures | 10–21 h |
| **LRM-07** Tray/status/resource evidence | `src/tray.rs`, status/doctor/i18n | Checked instance submenu, partial-failure state, reversible toggles, CN/EN parity, sampled memory/time; no UI-owned authority | 7–13 h |
| **LRM-08** Integrated gate + Owner cutover prep | tests, docs, package/update seams, qualification receipts | Windows synthetic end-to-end; independent scoped security review; trusted H1-like live plan, explicit Owner authorization, restoration proof | 12–22 h |

**Indicative sum:** **75–140 active hours**. Actual versioned deliverables may split or combine adjacent leaves if the risk diff remains bounded. The full Program budget is maintained in [RM-END-TO-END-DELIVERY-PLAN](RM-END-TO-END-DELIVERY-PLAN.md); keep numerical ranges consistent at each revision.

## Dependency and merge graph

```text
v0.1 existing Master Goal ────────> release gate ──┐
                                                    ├─> LRM-04/05/06 integrated control ─> LRM-07 ─> LRM-08
LRM-00 ─> LRM-01 ─> LRM-02 ─> LRM-03 ─────────────┘
```

- **Safe early lane:** LRM-00/01 and synthetic LRM-02 may begin on an isolated branch without an installed RunnerMesh, real GitHub token, Service edits, or other host modifications. A Draft PR is allowed; feature integration/merge is gated by v0.1 release compatibility and authoritative Owner decision.
- **Selector decision gate:** LRM-06 cannot advertise same-scope multiple runners until a reviewed selector-namespace/migration ADR is accepted. Maintaining only read-only discovery across colliding scopes is always safe.
- **Authority gate:** Real Service mutation, runner label changes, startup change, credentials, registration, or private workflow migration require an exact Owner plan + authorization. No global admin privileges are silently assumed.
- **Production gate:** immutable versioned install and exact checked artifact; never activate `target/` or worktree builds for live dogfood.
- **Release gate:** v0.2-A must have independent successful real cutover plus restoration evidence and no critical regressions; a feature branch is not a release.

## Contract examples (design sketch; not existing CLI)

```text
runnermesh runners list --json
runnermesh runners discover --read-only
runnermesh runners inspect <local-key>
runnermesh runners enroll <local-key> --prepare
runnermesh runners enable <local-key>
runnermesh runners disable <local-key> --when-idle
runnermesh runners disable --all --when-idle
runnermesh runners reconcile <local-key>
runnermesh runners forget <local-key> --prepare
```

These commands are **planned**, not implemented. CLI implementation must first expose a safe dry-run or prepare-only path for mutating actions. A `StopNow` or `kill-worker` shortcut is not part of this Goal.

## Tests / evidence matrix

| State or edge | Required minimum test |
|---|---|
| Two runners on distinct scopes | Toggle A leaves B labels, service, work root, identity and state unchanged |
| Same scope + same frozen v0.1 selector | Refuse collision; no accidental label mutation |
| Valid busy Worker at disable time | Persist disabled intent, withdraw eligible admission, leave Worker intact, defer stop |
| Generic-label legacy workflow | Surface no-guarantee / migration requirement; do not assert achieved safe drain |
| Label removal acknowledged, readback unavailable | `WithdrawalBlocked`/unknown, not `Stopped` |
| Racing assignment after readback | Reclassify `DrainPending`, no destructive action |
| Service running / stopped / starting / external drift | Only exact verified service is eligible for control |
| Agent crash after intent, before SCM operation | Reconstruct from receipt + fresh observation; no duplicate/unsafe mutation |
| Reboot while runner disabled | Disabled intent persists; no unauthorized automatic re-enable |
| Owner session vs foreign identity / shared work root | Fail closed / refuse |
| Partial `disable all` | Per-item results and unaffected foreign processes |
| Original installation rollback | v0.1 single binding and immutable version remain recoverable |
| CLI / Tray / JSON / CN-EN | Same typed snapshot, stable keys, no localized machine values |
| Host resource samples | Stale/unattributable sample labeled unknown; no speculative savings |
| Real Owner qualification | Independent remote/local readbacks and mandatory restored baseline recorded separately |

## Quality gates / operational safety

Classify `CODE_CHANGED`, `RUNNER_CONTROL_CHANGED`, `USER_PERSISTENT_CONFIG_CHANGED`, `TRAY_PRESENTATION_CHANGED`, `SECURITY_PRIVACY_CHANGED`, and `INSTALL_ACTIVATION_CHANGED` **only where actually touched**. Apply `QUALITY_GATES.md` D/C/R/S/V/X/I criteria; run focused local tests first and one settled candidate hosted CI. Do not multiply identical audit loops or modify the production CI runner to test a synthetic backend.

Unattended train default: `PRODUCTION_MUTATION=false`. Never touch actual Services, labels, runner groups, registration, work roots, official active Worker, secret stores, production startup or installed runtime without a prepared Owner Gate. Include exact rollback/restore and verify it independently. No private host identifiers in public fixtures, PR comments, or receipts.

## Cursor + AGY execution handoff

1. **Cursor = sole Implementer/writer** for the currently owned branch. Start with LRM-00/01 after clean-worktree / authoritative-main / toolchain checks; follow `AGENTS.md` and the ledger. Iterate with focused local tests; do not launch a full repo audit.
2. **AGY = independent read-only review** on settled Cursor candidate, focusing on authority, selector collision, ownership, active-job safety, versioned config, and public privacy. Return severity, exact file/line, reproducible evidence, and `PASS/BLOCKED/UNPROVEN`; do not write Cursor's branch.
3. Cursor implements bounded fixes based on AGY findings; one final candidate gate; open or update focused PR. Do not merge live-mutating backend scope before the Owner authorization and accepted v0.1 compatibility gate.
4. Maintain explicit receipts: `GOAL_ID`, `START_MAIN`, `CANDIDATE_HEAD`, `RISK_VECTOR`, `LOCAL_GATE`, `CI_GATE`, `AGY_REVIEW`, `PRODUCTION_MUTATION=false`, `NEXT_GOAL`.

## Immediate safe first leaf

`LRM-01-DISCOVERY-SOURCE-ONLY`:

- inventory *read-only* of candidate service/session backends behind injected interfaces; no integration with real SCM mutation API;
- add synthetic fixtures for 0/1/N runners and same-scope collisions;
- expose pure typed `DiscoveredRunner`/refusal reasons, keeping v0.1 installed binding unchanged;
- run focused tests, `cargo fmt`, and risk-selected quality gate;
- prepare one **Draft** source PR only after settled self-review. AGY reviews the diff independently;
- stop before any production mutation, privileged host operation, registration, label mutation, service start/stop, or v0.1 product release merge.

No agent may infer permission for an Owner-gated action merely because the user allowed autonomous coding.
