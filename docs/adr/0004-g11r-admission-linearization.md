# ADR 0004: G11R Admission Linearization

- Status: **Proposed — Owner design-freeze and trust-authority decision required**
- Decision package date: 2026-08-30
- Product scope: RunnerMesh v0.1 admission/lifecycle control
- Real runner mutation: forbidden by this ADR

## Disposition

No local-only lifecycle mechanism for an already-configured persistent official
runner satisfies the frozen v0.1 withdrawal contract. The least disruptive
functional candidate is a unique server-side admission label combined with an
explicit two-phase withdrawal state machine. That candidate preserves the
persistent runner and active job, but it adds a GitHub write-authority and
secret-management boundary that the v0.1 freeze does not currently carry.

The stronger ephemeral/JIT alternative has explicit upstream one-job support,
but changes registration lifecycle and pulls the v0.9 broker/JIT product
boundary into v0.1.

This ADR therefore does **not** accept a production architecture. It records the
decision required from the Owner and prevents G11R-B from inventing a local
shutdown guarantee.

```text
G11R_A=BLOCKED_OWNER_ARCHITECTURE_DECISION
ADMISSION_ARCHITECTURE=PROPOSED_SERVER_SIDE_UNIQUE_ADMISSION_LABEL_PLUS_TWO_PHASE_WITHDRAWAL
WITHDRAWAL_STATE_MACHINE=DEFINED_CONDITIONALLY
LINEARIZATION_POINT=PROPOSED_LABEL_BARRIER_COMMIT
PRE_LINEARIZATION_RACE_POLICY=ASSIGNMENT_MAY_WIN_AND_MUST_FINISH
POST_LINEARIZATION_GUARANTEE=NO_NEW_SELECTOR_MATCH_CONDITIONAL_ON_GITHUB_ROUTING_CONTRACT
ACTIVE_JOB_POLICY=CONTINUE_WITHOUT_LOCAL_SHUTDOWN_SIGNAL
IDLE_WITHDRAWAL_POLICY=REMOVE_ADMISSION_LABEL_THEN_SETTLE_PRECOMMIT_RACE
RACING_JOB_POLICY=PRECOMMIT_ASSIGNMENT_RUNS_TO_COMPLETION
RESTART_RECONSTRUCTION_POLICY=RECONSTRUCT_FROM_SERVER_LABEL_PLUS_EXACT_LOCAL_EVIDENCE
REQUIRED_GITHUB_AUTHORITY=RUNNER_LABEL_WRITE_AT_REGISTRATION_SCOPE
REQUIRED_LOCAL_AUTHORITY=ORDINARY_USER_EXACT_BOUND_OBSERVATION_AND_POST_BARRIER_LIFECYCLE
REGISTRATION_LIFECYCLE=PERSISTENT_RUNNER_WITH_SERVER_LABEL_METADATA_MUTATION
UPSTREAM_COMPATIBILITY=SUPPORTED_APIS_WITH_SCHEDULER_COMMIT_DOCUMENTATION_GAP
DEPRECATION_RISK=LOW_FOR_LABEL_API_HIGH_FOR_RUN_ONCE_FALLBACK
DESIGN_FREEZE_CHANGE=REQUIRED
DESIGN_FREEZE_CHANGE_REQUIRED=true
```

## Product question

The question is not how to make historical PR #17 pass. It is:

> How does RunnerMesh truthfully withdraw contributed GitHub Actions capacity
> while preserving active normal work?

The answer needs one event that orders withdrawal against job assignment. A
policy request is intent, not proof that withdrawal has happened.

## Terms

- **Admission barrier**: a mechanism the GitHub scheduler uses when deciding
  whether this runner matches a job.
- **Barrier commit**: the observable event selected as the ordering boundary
  between assignments that may still win and assignments that are forbidden.
- **Race settlement**: completion or expiry of work assigned before the barrier
  commit. It is later than the barrier commit when an assignment was already in
  flight.
- **Withdraw requested**: policy denies future capacity, but the barrier has not
  committed.
- **Drain pending**: a pre-commit assignment or active Worker still needs to
  finish after withdrawal was requested.
- **Withdrawn**: the admission barrier has committed; new matching assignments
  are forbidden by the selected mechanism. An already-admitted job may still be
  finishing.
- **Offline**: withdrawn, race-settled, and no exact bound Listener is remotely
  available.

`DRAINED` is an operational admission state, not a process-name observation. A
node can be `DRAINED` while its exact Listener remains connected, provided the
server-side selector barrier makes it ineligible and any pre-commit assignment
is handled by the race policy. `OFFLINE` additionally requires absence of
remotely available capacity.

## Cases A-D

### Case A: Busy to withdrawal

1. Policy records withdrawal intent.
2. The server-side admission label is removed.
3. The current Worker continues without a local Listener/Worker shutdown
   signal.
4. After normal completion, RunnerMesh remains withdrawn and may optionally
   stop the Listener only after the pre-commit race is settled.

The active job wins because it was admitted before the barrier commit.

### Case B: Listening/idle to withdrawal

1. Policy moves `Listening -> WithdrawRequested`.
2. RunnerMesh removes the unique admission label through the typed GitHub port.
3. A successful response whose returned representation excludes the label is
   the proposed barrier commit.
4. RunnerMesh waits for any assignment that won before that response. GitHub's
   documented 60-second pickup/requeue bound informs qualification of this
   settlement window; it is not the barrier commit itself.
5. With no racing assignment, the node reports `Withdrawn`. Physical Listener
   shutdown is a later lifecycle operation.

The initial policy request alone never reports `DRAINED`.

### Case C: Assignment racing with withdrawal

- Assignment ordered before the barrier commit may be delivered and must be
  allowed to run normally.
- Assignment ordered after the barrier commit is forbidden because every
  admitted workflow requires the now-absent unique label.
- Unknown ordering, server read failure, or selector drift reports `Unknown` or
  `Refused`; it never reports achieved `DRAINED`.
- RunnerMesh must not use Ctrl+C, Ctrl+Break, service stop, or arbitrary process
  termination to settle the race.

### Case D: Agent restart during withdrawal

Persist only intent and the last durable barrier transaction identity. On
restart, reconstruct from:

1. exact registration scope and runner identity;
2. current server-side labels and runner `status`/`busy` evidence;
3. exact local runner-home, registration, executable, Listener, Worker, and
   work-root evidence;
4. the durable request/response record for any in-flight label mutation.

If those sources disagree or cannot be read, enter `Unknown` or `Refused`.
Never infer `FULL` or `DRAINED` from a remembered intent or process name.

## Proposed state machine

| State | Meaning | May advertise `FULL` | May advertise `DRAINED` |
|---|---|---:|---:|
| `OfflineUnavailable` | No proved remotely available exact capacity | no | only if the barrier is also proved |
| `Withdrawn` | Barrier committed; pre-commit work may still finish | no | yes |
| `FullAvailable` | Policy permits capacity; advertisement not yet proved | no | no |
| `Listening` | Exact bound runner is remotely available and selector is present | yes | no |
| `AssignmentPending` | A job assignment won before the barrier or is being acquired | no | no |
| `Busy` | Exact bound Worker is active under normal job ownership | no additional job | no unless barrier already committed |
| `WithdrawRequested` | Intent recorded; barrier not committed | no claim | no |
| `DrainPending` | Barrier requested/committed while assigned work remains | no | only after barrier commit |
| `Reconnecting` | Re-advertisement or session reconstruction in progress | no | no |
| `Unknown` | Connectivity/order/evidence cannot be established | no | no |
| `Refused` | Registration, selector, image, identity, or work-root drift | no | no |

### Transition table

| Current | Event | Next | Required effect |
|---|---|---|---|
| `OfflineUnavailable` / `Withdrawn` | policy requests FULL | `FullAvailable` | clear no server state yet |
| `FullAvailable` | reconnect begins | `Reconnecting` | start only exact bound capacity |
| `Reconnecting` | Listener and selector become remotely available | `Listening` | advertise only after proof |
| `Listening` | policy requests withdrawal | `WithdrawRequested` | durably record intent |
| `Busy` | policy requests withdrawal | `DrainPending` | do not signal Worker/Listener |
| `WithdrawRequested` | pre-commit assignment arrives | `AssignmentPending` | assignment wins the race |
| `WithdrawRequested` | barrier commit | `Withdrawn` | forbid later matching assignment |
| `AssignmentPending` / `Busy` | barrier commit | `DrainPending` | preserve assigned work |
| `AssignmentPending` | Worker begins | `DrainPending` | preserve Worker under withdrawal |
| `DrainPending` | Worker completes and barrier is committed | `Withdrawn` | no relaunch/re-advertisement |
| `DrainPending` | Worker completes without barrier proof | `WithdrawRequested` | do not invent withdrawal |
| any | post-commit assignment appears | `Refused` | contract/platform violation; no dispatch by RunnerMesh |
| any controlled state | connectivity becomes uncertain | `Unknown` | revoke achieved-state claims |
| any controlled state | registration/selector/image/work-root drift | `Refused` | issue no control action |
| any controlled state | unrelated runner appears | unchanged | issue no action toward it |
| any | Agent restarts | reconstructed state | use current server + exact local evidence |

The executable reference model is
[`tests/admission_linearization_model.rs`](../../tests/admission_linearization_model.rs).
It proves the conditional transition properties without calling GitHub or a
real runner.

## Safety properties

1. Ordinary withdrawal has no transition that kills or signals an active
   normal Worker.
2. After the barrier commit, a selector-matching assignment cannot be admitted
   by the model.
3. Before the barrier commit, a racing assignment is explicit and survives to
   normal completion.
4. Unknown connectivity, ordering, or ownership cannot yield `FULL`,
   `DRAINED`, or `OFFLINE`.
5. Same-name unrelated processes never create control authority.
6. Restart reconstructs observed truth and may refuse; it does not replay
   remembered process authority.

These are conditional source-model proofs. The post-commit guarantee still
depends on an Owner-approved GitHub routing contract and later trusted H1
qualification.

## Upstream audit

Audit snapshots:

| Surface | Version / commit | Finding |
|---|---|---|
| Locally targeted historical runner source | `actions/runner v2.336.0`, commit `98aabcd429c4e8402406c56ce2d26387fed3b9ce` | `--once` warning and shutdown behavior below |
| Latest release during this audit | `v2.337.0`, commit `397b032cbf865e9c3ddfab89d533ec19325e1273`, published 2026-08-26 | relevant source blobs unchanged from v2.336.0 |
| Current upstream `main` during this audit | `fb64b9b20d56951bf30c5b7333a128bc25c2d923` | relevant source blobs still unchanged |

Exact source evidence:

- [`Runner.cs` warns that `--once` will be deprecated and maps both `--once`
  and ephemeral configuration to one-job execution](https://github.com/actions/runner/blob/397b032cbf865e9c3ddfab89d533ec19325e1273/src/Runner.Listener/Runner.cs#L312-L331).
- [`Runner.cs` waits for the one job to finish and then exits](https://github.com/actions/runner/blob/397b032cbf865e9c3ddfab89d533ec19325e1273/src/Runner.Listener/Runner.cs#L568-L602).
- [Ctrl+C requests runner shutdown](https://github.com/actions/runner/blob/397b032cbf865e9c3ddfab89d533ec19325e1273/src/Runner.Listener/Runner.cs#L359-L389), and
  [`JobDispatcher::ShutdownAsync` cancels a running job](https://github.com/actions/runner/blob/397b032cbf865e9c3ddfab89d533ec19325e1273/src/Runner.Listener/JobDispatcher.cs#L206-L248).
- [Windows service stop sends Ctrl+C and kills the Listener if it has not
  exited within 30 seconds](https://github.com/actions/runner/blob/397b032cbf865e9c3ddfab89d533ec19325e1273/src/Runner.Service/Windows/RunnerService.cs#L170-L222).
- [The Listener sends `Online`/`Busy` status while polling for messages](https://github.com/actions/runner/blob/397b032cbf865e9c3ddfab89d533ec19325e1273/src/Runner.Listener/MessageListener.cs#L218-L300).

Official platform documentation:

- [GitHub routes to a matching online/idle runner and requeues an assigned job
  if it is not picked up within 60 seconds](https://docs.github.com/en/actions/reference/runners/self-hosted-runners#routing-precedence-for-self-hosted-runners).
- [GitHub explicitly warns that it cannot always guarantee no assignment to a
  persistent runner while it shuts down, and recommends ephemeral runners for
  autoscaling](https://docs.github.com/en/actions/reference/runners/self-hosted-runners#ephemeral-runners-for-autoscaling).
- [Ephemeral runners are automatically de-registered after one job](https://docs.github.com/en/actions/reference/runners/self-hosted-runners#ephemeral-runners-for-autoscaling).
- [Organization label mutation requires `Self-hosted runners` write permission;
  repository label mutation requires repository `Administration` write](https://docs.github.com/en/rest/actions/self-hosted-runners?apiVersion=2026-03-10).
- [Runner-group membership/access mutation requires Organization self-hosted
  runner write authority](https://docs.github.com/en/rest/actions/self-hosted-runner-groups?apiVersion=2026-03-10).
- [JIT configuration requires the same Organization runner-write or repository
  Administration-write boundary](https://docs.github.com/en/rest/actions/self-hosted-runners?apiVersion=2026-03-10#create-configuration-for-a-just-in-time-runner-for-an-organization).

GitHub documents label-based eligibility and the mutation APIs, but does not
document a scheduler concurrency guarantee that explicitly orders an API
response against an assignment already being routed. The proposed barrier
commit is therefore a product assumption that must be approved and qualified,
not an upstream claim quoted as stronger than its documentation.

## Option scoring

Score direction is consistent for every row: `3` is strongest correctness or
lowest authority/risk/complexity; `0` is a contract failure or material product
shift. Scores compare the complete v0.1 requirement, not isolated code reuse.

| Criterion | A Persistent local Listener | B `--once` lease | C Server label/group | D Ephemeral/JIT lease | E Two-phase clarification alone |
|---|---:|---:|---:|---:|---:|
| Human-first correctness | 0 | 1 | 2 | 3 | 1 |
| Exact linearization point | 0 | 1 | 2 | 3 | 0 |
| No-new-job guarantee | 0 | 1 | 2 | 3 | 0 |
| Explicit racing-job semantics | 0 | 1 | 3 | 3 | 3 |
| Active-job survival | 0 | 3 | 3 | 3 | 3 |
| Idle withdrawal behavior | 0 | 0 | 3 | 3 | 0 |
| Minimal GitHub API authority | 3 | 3 | 0 | 0 | 3 |
| Minimal token/Organization authority | 3 | 3 | 0 | 0 | 3 |
| Minimal registration mutation | 3 | 3 | 2 | 0 | 3 |
| Minimal local privilege/UAC | 1 | 3 | 3 | 3 | 3 |
| Restart/reconstruction | 2 | 2 | 2 | 2 | 2 |
| Current upstream support | 3 | 1 | 3 | 3 | 2 |
| Low deprecation risk | 3 | 0 | 3 | 3 | 3 |
| Low implementation complexity | 2 | 2 | 2 | 0 | 2 |
| Low operational complexity | 2 | 2 | 1 | 0 | 2 |
| Already-configured runner compatibility | 3 | 3 | 2 | 0 | 3 |
| Frozen v0.1 fit | 1 | 1 | 0 | 0 | 0 |
| Later v0.4/v0.9 fit | 1 | 1 | 2 | 3 | 2 |

### Option A: persistent runner with local Listener control

- **Failure**: an idle observation and a shutdown request are not ordered
  against assignment. If assignment wins, current upstream shutdown cancels the
  Worker. GitHub separately documents that persistent shutdown can still race
  with assignment.
- Service stop adds privilege and a 30-second kill fallback without fixing the
  admission race.
- Process exit could be an observational boundary, but there is no accepted way
  to reach it that preserves a just-started racing Worker.

### Option B: run-once job lease

- **Partial only**: after a Busy job, natural one-job completion provides a safe
  exit and no relaunch can preserve withdrawal.
- **Failure while idle**: the Listener must either accept one more job, wait
  indefinitely, or take the unsafe shutdown path. A policy request cannot
  promptly withdraw idle capacity.
- Upstream source explicitly warns of future `--once` deprecation.

### Option C: server-side label or runner-group admission

- A unique required label is narrower than changing runner-group access and
  preserves the persistent registration.
- Removing that label does not signal a running Worker and can handle idle
  withdrawal. Pre-response assignment is the explicit race winner.
- Every relevant workflow must require the admission label; generic selectors
  would bypass the barrier.
- It requires runner-label write authority at the registration scope. An
  Organization runner requires Organization `Self-hosted runners` write; a
  repository runner requires repository `Administration` write.
- The API response/scheduler ordering assumption needs Owner acceptance and H1
  qualification because the official docs do not explicitly specify it.

Runner-group mutation is not selected: it is broader, affects routing access,
requires Organization runner-write authority, and is harder to isolate to one
runner without a dedicated group.

### Option D: ephemeral or JIT runner lease

- Strongest upstream semantics when provisioning is demand-gated: issue at most
  one runner lease only while policy permits capacity; a lease issued before
  withdrawal may finish, and no later lease is issued.
- GitHub explicitly guarantees one job for ephemeral runners and automatically
  de-registers them after the job.
- It requires registration creation, server APIs, token authority, external log
  handling, cleanup, and a provisioning/controller category.
- It is directionally aligned with v0.9 scale-set/JIT work, not the frozen v0.1
  already-configured persistent-runner contract.

### Option E: two-phase semantic clarification

- Necessary vocabulary, not an admission mechanism.
- It truthfully distinguishes `WithdrawRequested` from achieved `Withdrawn` and
  makes racing work explicit.
- Alone it can remain pending forever and supplies no post-boundary guarantee.
  Combined with Option C or D it is valuable; combined only with A or B it does
  not repair idle withdrawal.
- Accepting indefinite pending/best-effort withdrawal would weaken the frozen
  human-first semantic and requires an explicit design-freeze decision.

## PR #17 salvage map

PR #17 exact head audited: `a8a028e472ff1271003ee161b7307c3e70818b40`.

| Component | Disposition | Reason |
|---|---|---|
| Exact runner-home and executable-image scoping | `KEEP_WITH_REFACTOR` | Preserve exact boundary matching; integrate with the accepted admission mechanism rather than the G11-only executor |
| Registration/image/work-root fingerprints | `KEEP_WITH_REFACTOR` | Preserve drift refusal, but distinguish expected official runner update from hostile/ambiguous drift; do not permanently pin one version |
| Bounded executor port/fake seam | `KEEP_WITH_REFACTOR` | Valuable deterministic boundary; rename around mechanism-neutral admission and lifecycle ports |
| CTRL+C / CTRL+BREAK rejection for ordinary Busy drain | `KEEP_AS_IS` | Current upstream proves shutdown cancels a Worker; no ordinary withdrawal signal is permitted |
| Busy no-signal behavior | `KEEP_AS_IS` | Active normal work must finish |
| `run.cmd --once` production launch assumption | `TEST_EVIDENCE_ONLY` | Useful proof for one-job completion, but incomplete idle semantics and future deprecation prevent selection |
| Safe-wait reconstruction | `KEEP_WITH_REFACTOR` | Exact wait-only behavior is safe under uncertainty; known-owned listeners must still support truthful adoption/reconstruction under ADR 0001 |
| Idle-withdrawal refusal | `SUPERSEDE` | Truthful as a PR17 limitation, but not a product solution; the selected barrier must implement idle withdrawal |
| Process ancestry/scoping | `KEEP_WITH_REFACTOR` | Exact image/home/service relation can support ownership; ancestry or process name alone never grants authority |
| G11-only CLI/executor activation surface | `DELETE_IN_REPLACEMENT` | Do not ship a mechanism-specific qualification switch as the v0.1 lifecycle architecture |
| PR17 ADR 0004 run-once decision | `SUPERSEDE` | It explicitly leaves idle withdrawal unproven and was never merged |
| Historical local/host qualification results | `TEST_EVIDENCE_ONLY` | Source/Busy evidence informs tests; it is not current architecture or host acceptance |

PR #17 must not be rebased or merged as the G11R-B implementation.

## Owner decision

Choose one of these product-contract changes before G11R-B:

1. **Authorize the proposed label barrier**: add a narrowly designed typed
   GitHub credential boundary, require the unique admission selector, accept the
   documented routing assumption for H1 qualification, and define secret
   provisioning/rotation/revocation outside ordinary process memory and logs.
2. **Move ephemeral/JIT admission into v0.1**: accept the broader registration,
   provisioning, log-retention, and scale-controller product shift.
3. **Weaken v0.1 withdrawal**: explicitly change the freeze so a local-only
   `WithdrawRequested` may remain pending or admit one more job. This is not the
   recommended human-first semantic and cannot be inferred from the current
   freeze.

Until that decision is made:

```text
G11R-B=NOT_AUTHORIZED
REAL_HOST_MUTATION=false
H1=NOT_READY
```

