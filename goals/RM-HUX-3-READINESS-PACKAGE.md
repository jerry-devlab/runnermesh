# HUX-3 Formal Multi-Runner Qualification Readiness Package

`PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001` · `TRAIN_ID=RM-160H-T06-UX-INSTALL-HUX3` · `PRODUCTION_MUTATION=false`

This package prepares Owner-gated live Multi-Runner qualification (HUX-3 / R6).
It does **not** authorize live label mutation, live service mutation, or production cutover.
Implementer self-review is not independent acceptance.

```text
PACKAGE_BUILD_COMMIT=3952b1b83f922701f8d4984585ced02b132c997e
CODE_ANCHOR=3952b1b83f922701f8d4984585ced02b132c997e
PREDECESSOR_DOCS_PR_HEAD=9757405c93cb6fb7d42cf7f7a79e61dbfffc3dbd
PREDECESSOR_DOCS_PR_CI=42788141367 (PASS_ALL 6/6 jobs)
SANDBOX_TWO_RUNNER_PASS=PASS        # prepare_two_runner_sandbox layout + tests
AGENT_CORE_MULTI_RUNNER=PASS        # single authority multi-runner matrix
AGENT_ENTRYPOINT_ISOLATION=PASS     # agent_entrypoint_runner_a_disable_preserves_runner_b_intent_and_active_execution
REGISTRY_PERSISTENCE=PASS           # assert_registry_intent_round_trip
ADR0004_COLLISION_GUARD=PASS        # same_scope_collision refused
ADR0006_STATUS=DRAFT_NOT_ACCEPTED   # awaiting Owner R5 decision
R4_PREREQUISITE=WAITING_FOR_OWNER   # HUX-2 live SCM gate must pass first
LIVE_NOT_AUTHORIZED=true
```

## 1. Multi-Runner Scope & Invariants Under Governance

### Invariant 1: Single Controlling Agent Authority
A single Agent process owns all local runners on the workstation. Neither UI (Tray/CLI)
nor runner services own policy, desired state, or admission authority.

### Invariant 2: Independent Runner Intent Isolation
Toggling or disabling runner A MUST NOT mutate runner B:
- Runner B desired state, worker lifecycle, Windows service, admission selector, and registry remain completely untouched.
- Runner B can continue executing and finishing legitimate CI jobs while runner A is in drain/disabled state.

### Invariant 3: Admission Routing Under R5 (ADR 0006)
- **Current Accepted Authority (ADR 0004)**: Uses the single reserved label `runnermesh-admit`.
  Dual managed enrollment targeting the exact same GitHub registration scope is strictly refused as `same_scope_collision`.
  Multi-runner operation is valid across:
  1. Distinct GitHub scopes (e.g., Repo 1 + Repo 2, or Org 1 + Org 2); or
  2. Exactly one managed runner (`runnermesh-admit`) plus unmanaged auxiliary runners.
- **Draft Proposal (ADR 0006)**: Scoped selectors `runnermesh-admit-<stable-local-key>`.
  Remains `DRAFT_NOT_ACCEPTED` until Owner explicitly dispositions ADR 0006 (`ACCEPTED` | `REJECTED` | `DEFER`).

### Invariant 4: Non-Destructive Intent & State Persistence
- Disabling a runner requests graceful drain (`DrainThenStop`), never destructive worker termination (`kill` / `taskkill`).
- Agent restart preserves persisted disabled intent in the registry store (`assert_registry_intent_round_trip`).

## 2. Source & Sandbox Verification Evidence

All source and sandbox contracts for multi-runner execution are verified and green:

1. **Sandbox Layout Isolation (`src/hux_sandbox.rs`)**:
   - `prepare_two_runner_sandbox` constructs two independent runner roots (`runner-a`, `runner-b`) with distinct `.runner` credentials, service configurations, and work directories.
   - Tested in `hux_sandbox::tests` and cargo test suite.
2. **AgentCore Multi-Runner Reconciler (`src/agent.rs`)**:
   - `agent_core_single_authority_reconciles_multi_runners_with_complete_precedence_matrix` verifies multi-runner precedence, collision refusal, and state observation.
3. **Intent Round-Trip Persistence (`src/lib.rs`, `src/agent.rs`)**:
   - Verified that disabled intent round-trips through disk storage and survives Agent crash/restart.
4. **Collision Refusal (`src/admission.rs`, `tests/fake_github_admission.rs`)**:
   - `same_scope_collision` is surfaced and blocks enrollment when two runners share the same GitHub scope under ADR 0004.
5. **Human Visual Confirmation (HUX-0 / PR #63)**:
   - Owner accepted the development Tray presentation showing distinct runner intent labels and independent toggling.
6. **Regular Agent Entrypoint Isolation (`src/agent.rs`)**:
   - `agent_entrypoint_runner_a_disable_preserves_runner_b_intent_and_active_execution` proves from the regular Agent entrypoint (`AgentCommand::RunnersDisable`) that disabling Runner A preserves Runner B desired `Full`, keeps active worker execution uninterrupted (`plan: Noop`), persists across Agent reload, and rejects same-scope collisions fail-closed.

## 3. Operator Sequence for Live HUX-3 Qualification (When Authorized)

When prerequisites are met and Owner opens the live qualification window:

```text
Step 1: Capture prestate for both Runner A and Runner B (status, service state, active job, selector).
Step 2: Confirm both runners are healthy and idle (Worker not busy).
Step 3: Toggle Runner A to Disabled via Tray or CLI.
Step 4: Verify Runner A begins graceful drain (withdrawing selector; wait idle; stop service).
Step 5: Concurrently verify Runner B state is 100% untouched.
Step 6: Trigger a legitimate CI job targeting Runner B; verify Runner B accepts and finishes the job successfully.
Step 7: Restart the Agent process.
Step 8: Verify Runner A remains Disabled (persisted intent) while Runner B remains Active.
Step 9: Restore Runner A to Enabled; verify both runners achieve Active/Listening state.
Step 10: Emit separate QUALIFICATION and RESTORE receipts.
```

## 4. Non-Bypassable Blockers Before Live Qualification

Live multi-runner qualification CANNOT proceed until:

1. **R4 (HUX-2 live SCM gate) PASS**: Live Windows Service control for a single runner must be qualified first.
2. **R5 (ADR 0006) Owner Disposition**: Owner must decide `ACCEPTED`, `REJECTED`, or `DEFER` on `goals/RM-160H-R5-ADR006-OWNER-BRIEF.md`.
3. **Two Exact Live Test Runners**: Workstation must have two real registered test runners (recorded in private Owner identity sheet).
4. **Owner Transaction Token**: Sealed trusted authorization window (15–120 minutes) with automatic restore.

## 5. Exit Dispositions

```text
PASS     Independent isolation proven: A disabled -> B completes CI; restart preserves intent; RESTORE=PASS
BLOCKED  R4 not passed, ADR 0006 undecided, missing second runner, busy worker, or Owner abort
FAIL     Toggling A mutates B, cross-scope label drift, or destructive worker kill attempted
```
