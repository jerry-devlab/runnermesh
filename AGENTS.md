# AGENTS.md — RunnerMesh development governance

RunnerMesh uses evidence-first, risk-based development governance inspired by the proven focused-Goal/Fast-Lane pattern. `dev_governance_files/QUALITY_GATES.md` is authoritative for gate selection; `FAST_LANE.md` is the compact execution reference; `AUTONOMOUS_TRAINS.md` governs long unattended runs.

## 1. Writer model

- Ordinary development has exactly one active Implementer writer per branch/worktree.
- Architect, supervisor, reviewer, and auditor roles are read-oriented unless a Goal explicitly transfers write authority.
- Never run competing writers against the same branch/worktree.
- Preserve foreign/local work; never reset or clean it merely to make a Goal proceed.
- Git writes belong to the active Implementer for that Goal.

## 2. Goal contract

Every implementation Goal declares:

- authoritative repository and starting `main`/predecessor;
- allowed subsystem/files;
- acceptance criteria;
- changed-risk vector and selected gates;
- explicit non-goals;
- production-mutation authority;
- stop conditions;
- receipt fields and next safe Goal.

Do not fold nearby cleanup or later-version scope into the Goal.

## 3. Branch/merge discipline

Preferred flow:

1. synchronize authoritative `main` or admitted predecessor;
2. require a clean/owned worktree;
3. create one focused branch for independently revertible work;
4. implement with focused tests;
5. settle one candidate head;
6. push/open or update the focused PR;
7. run only gates selected by `QUALITY_GATES.md`;
8. reuse accepted evidence when the relevant risk diff is empty;
9. merge intentionally after required gates pass;
10. verify remote `main` after merge;
11. emit a concise receipt.

Do not manufacture extra commits solely for governance checkpoints.

## 4. RunnerMesh risk vector

Before validation classify:

```text
CODE_CHANGED
TRAY_PRESENTATION_CHANGED
PROBE_OR_POLICY_CHANGED
RUNNER_CONTROL_CHANGED
USER_PERSISTENT_CONFIG_CHANGED
INSTALL_ACTIVATION_CHANGED
SECURITY_PRIVACY_CHANGED
RELEASE_BOUNDARY
```

Validation is risk-surface based. One invariant/failure family normally gets one representative proof.

## 5. Exact-head and evidence reuse

Fresh candidate evidence binds to:

```text
EXPECTED_HEAD == checked_out_head == evidence_head
```

Reuse accepted prior evidence only when the relevant risk diff is empty and record:

```text
<GATE>=REUSED
<GATE>_REUSED_FROM=<sha>
<GATE>_RISK_DIFF=EMPTY
```

A docs-only closeout does not invalidate unchanged tray, probe, runner-control, config, install, or dogfood evidence.

## 6. Evidence dispositions

Use only:

- `PASS` — proved;
- `FAIL` — disproved;
- `BLOCKED` — external/trust/precondition boundary prevented execution;
- `UNPROVEN` — insufficient evidence;
- `REUSED` — prior accepted evidence remains valid;
- `N/A` — outside the changed-risk vector.

Never convert `UNPROVEN` into success.

## 7. Frozen v0.1 product contract

`docs/v0.1-design-freeze.md` is the detailed v0.1 authority. Preserve:

- Windows single-workstation first-usable product;
- ordinary user-session Agent as default Workstation Mode;
- Agent + Tray + CLI with local Named Pipe IPC;
- Agent Core as sole operational authority;
- `Observe -> Decide -> Reconcile`;
- one controlling Agent per user profile;
- `UserMode`/`NodeState` contracts from G01;
- Zen as an override, not another mode;
- normalized probe evidence boundary;
- User Activity, Steam Game, and Process List probes;
- conservative Auto Lite;
- manual/Zen/hard-safety precedence;
- typed GitHub Actions link state;
- no TUI/full GUI/Web UI in v0.1;
- official GitHub runner reuse rather than protocol reimplementation;
- one execution identity / one active owned work root;
- graceful drain without normal destructive Worker termination;
- user-level production-safe install/update/rollback.

Do not pull v0.2 resource enforcement, v0.3 rich automatic intelligence, v0.4 mesh placement, or later backends into v0.1 without an explicit ADR.

## 8. Frontend and localization boundaries

CLI, Tray, and any future TUI consume the same `AgentSnapshot` and issue typed `AgentCommand`s. UI code never owns runner or policy state.

Localization affects presentation only. Never localize JSON keys, stable enum values, reason codes, config keys, IPC command IDs, or stable menu/action IDs. Never implement behavior by comparing visible strings.

Tray/menu mutation belongs to its UI/event-loop thread.

## 9. Probe boundaries

Policy consumes normalized `ProbeSnapshot` evidence, not Steam/process/provider-specific types. `Unknown`/`Unavailable` must not masquerade as `Inactive`. Disabled configuration is distinct from runtime suspension/state.

A heuristic or platform observation must not be presented as more authoritative than its evidence supports.

## 10. Official runner and host safety

RunnerMesh does not reimplement GitHub Actions workflow parsing, demand queues, job protocol, logs, checks, or artifacts.

Real runner registration, real service state, real work-root ownership, and Organization runner access are trust boundaries. Unattended source-development Goals do not mutate them unless a specific Human Gate explicitly authorizes it.

Cross-identity active work-root reuse is forbidden. Do not globally weaken Git `safe.directory` to bypass ownership.

## 11. Production-runtime isolation

Permanent invariant:

```text
SOURCE != BUILD != RELEASE != INSTALLED RUNTIME != ACTIVE VERSION
```

- mutable worktree binaries are never production deployment;
- `target/` is development output only;
- autostart never points into a source tree;
- normal source development defaults to `PRODUCTION_MUTATION=false`;
- stable/canary payloads may coexist but only one Agent has control authority;
- dogfood immutable authorized RC/release artifacts, not arbitrary worktree builds.

## 12. Persistent/destructive writes

Setup, autostart, install, uninstall, migration, update, rollback, and remediation code must prove exact ownership before overwrite/delete, preserve unrelated content, refuse ambiguous drift, and provide recovery where practical.

Privileged or activation transactions are narrow, transactional, durably receipted, and reconciled after interruption. Loss of synchronous helper completion is not proof that no mutation occurred.

## 13. Public trust/privacy

This repository is public. Never place private dogfood identifiers, credentials, personal infrastructure paths, private topology, private workflow IDs, or secrets in public code, docs, examples, fixtures, or receipts.

Public PR CI remains GitHub-hosted. Persistent personal workstations must not execute arbitrary untrusted public-fork code by default.

## 14. Autonomous train stop gates

Unattended trains stop before:

- UAC/elevation;
- real Windows Service mutation;
- real runner registration mutation;
- destructive real work-root mutation;
- Organization runner-access/security changes;
- new secret/trust authority;
- production autostart activation;
- installed stable-runtime mutation;
- production cutover;
- public release publication;
- destructive active-job termination;
- ambiguous ownership/security state.

Ordinary deterministic failure families receive at most three materially distinct repair cycles before stopping with a blocker fingerprint.

## 15. Audit/blocker discipline

Dedicated auditors are reserved for changed destructive/persistent external writes, security/privacy, high-risk concurrency/ownership, ambiguous defects, production cutover, release/publication, or explicit Implementer request.

Do not chain auditors over unchanged evidence.

Once an unchanged blocker is proven, record:

```text
BLOCKER_LATCHED=true
```

Re-evaluate only after relevant source, evidence, trust state, Owner action, or external prerequisite changes.

## 16. Completion receipts

Receipts contain only active decision-relevant gates. Typical fields:

```text
DISPOSITION=<PASS|FAIL|BLOCKED|UNPROVEN>
GOAL_ID=<id>
START_MAIN=<sha>
CANDIDATE_HEAD=<sha>
PR=<number-or-url>
PR_MERGED=<true|false>
FINAL_MAIN=<sha-or-N/A>
RISK_VECTOR=<active dimensions>
CODE_CI=<PASS|REUSED|N/A>
TRAY_PRESENTATION=<PASS|REUSED|N/A>
PROBE_POLICY=<PASS|REUSED|N/A>
RUNNER_CONTROL=<PASS|REUSED|N/A>
PERSISTENT_CONFIG_SAFETY=<PASS|REUSED|N/A>
INSTALL_ACTIVATION_SAFETY=<PASS|REUSED|N/A>
SECURITY_PRIVACY=<PASS|REUSED|N/A>
RELEASE_GATE=<PASS|REUSED|N/A>
PRODUCTION_MUTATION=<true|false>
BLOCKER_LATCHED=<true|false>
OWNER_ACTION=<none-or-specific>
NEXT_RECOMMENDED_GOAL=<id-or-none>
```

Do not require irrelevant fields for ceremony.
