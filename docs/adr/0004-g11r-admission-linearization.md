# ADR 0004: GitHub-native dynamic admission label

- Status: **Accepted**
- Decision date: 2026-08-30
- Product scope: RunnerMesh v0.1 admission and lifecycle control
- Real runner or label mutation during G11R-A/B/C: **forbidden**

## Decision record

The Owner accepts one RunnerMesh-owned GitHub custom label as the v0.1
admission selector for an existing persistent official runner. Withdrawal is a
two-phase protocol: local policy intent starts withdrawal, then RunnerMesh
removes and reads back the reserved selector. The readback is an observed
server control point, not a claim that GitHub exposes a globally linearizable
scheduler barrier.

```text
G11R_A=ACCEPTED
ADMISSION_ARCHITECTURE=GITHUB_NATIVE_DYNAMIC_ADMISSION_LABEL
WITHDRAWAL_PROTOCOL=TWO_PHASE
DESIRED_STATE_VS_ACHIEVED_STATE=EXPLICIT
WITHDRAWAL_START=LOCAL_POLICY_INTENT_ACCEPTED
SERVER_CONTROL_POINT=RESERVED_LABEL_REMOVAL_AND_READBACK
SCHEDULER_LINEARIZABILITY=NOT_CLAIMED_WITHOUT_UPSTREAM_GUARANTEE
PRE_BARRIER_RACE_POLICY=CONSERVATIVE_IN_FLIGHT_ASSIGNMENT_MAY_COMPLETE
POST_SELECTOR_ABSENCE_POLICY=NO_NEW_RUNNERMESH_ELIGIBILITY_CLAIM
ACHIEVED_DRAINED_REQUIRES=SELECTOR_ABSENT_AND_NO_ACTIVE_BOUND_WORKER_AND_CONSISTENT_EVIDENCE
ACTIVE_JOB_POLICY=COMPLETE_NATURALLY
NORMAL_LOCAL_SIGNAL_POLICY=NONE
REQUIRED_GITHUB_AUTHORITY=MINIMAL_RESERVED_LABEL_MUTATION_AUTHORITY
REGISTRATION_LIFECYCLE=EXISTING_PERSISTENT_RUNNER_PRESERVED
JIT_EPHEMERAL=DEFERRED
PR17=SALVAGE_ONLY
DESIGN_FREEZE_CHANGE=TRUST_BOUNDARY_EXPANSION_PLUS_SEMANTIC_CLARIFICATION
SEMANTIC_WEAKENING=FALSE
RESERVED_ADMISSION_LABEL=runnermesh-admit
WORKFLOW_SELECTOR_CONTRACT=REQUIRED_FOR_RUNNERMESH_MANAGED_CAPACITY
```

The change expands the v0.1 trust boundary because managed admission now needs
narrow GitHub label-write authority. It does not weaken the human-first drain
semantic. It clarifies that requested/withdrawing are desired or transitional
states, while `DRAINED` is an achieved state supported by remote and local
evidence.

## Product semantics

Conceptually:

```text
FULL
  -> WITHDRAW_REQUESTED
  -> WITHDRAWING
  -> reserved selector confirmed absent
  -> settle any observed pre-boundary or in-flight assignment
  -> DRAINED
```

The boundaries are:

- accepting Work, Gaming, Zen, or an explicit drain request immediately records
  desired `DRAINED` and begins withdrawal;
- a label mutation acknowledgement is not achieved withdrawal;
- a subsequent observation of the exact bound runner must confirm that
  `runnermesh-admit` is absent;
- absence means a workflow that requires that selector no longer sees this
  runner as RunnerMesh-eligible under GitHub's documented cumulative label
  matching; it does not establish ordering for an assignment already being
  routed;
- an assignment observed around or after readback is conservatively treated as
  in-flight, remains visible as `DrainPending`, and may complete normally;
- `DRAINED` requires selector absence, no exact bound active Worker, and
  consistent exact-identity/local evidence;
- uncertainty, drift, or an API failure never becomes `FULL` or `DRAINED`.

If a late in-flight assignment becomes visible after an earlier `DRAINED`
observation, RunnerMesh immediately returns to `DrainPending` and allows that
work to complete. The product does not hide the race or reinterpret the API
response/readback as an upstream guarantee.

## Desired and achieved states

| State | Meaning | Achieved `FULL` | Achieved `DRAINED` |
|---|---|---:|---:|
| `Full` | policy desires advertised capacity | no | no |
| `Advertising` | reserved-label add requested or acknowledged, readback pending | no | no |
| `ReAdvertising` | re-advertising from a previously drained state | no | no |
| `Listening` | exact runner is locally consistent and selector presence is confirmed | yes | no |
| `Busy` | exact bound Worker is active while desired state is `FULL` | yes | no |
| `WithdrawRequested` | local withdrawal intent accepted | no | no |
| `Withdrawing` | reserved-label removal/readback is in progress | no | no |
| `WithdrawalBlocked` | withdrawal cannot advance because the API/credential/control evidence failed | no | no |
| `DrainPending` | selector is absent or removal is in progress, but an assignment/Worker remains | no | no |
| `Drained` | selector absent, no exact bound Worker, evidence consistent | no | yes |
| `Unknown` | remote/local evidence is unavailable or contradictory | no | no |
| `Refused` | runner identity, registration, ownership, or reserved-label binding drifted | no | no |

The reference model is
[`tests/admission_linearization_model.rs`](../../tests/admission_linearization_model.rs).
It keeps desired capacity, observed selector state, observed Worker state, and
local consistency separate.

## Two-phase withdrawal

### Busy withdrawal

1. Persist desired `DRAINED` and enter `WithdrawRequested`.
2. Request removal of only `runnermesh-admit` from the exact bound runner.
3. Do not send Ctrl+C, Ctrl+Break, service stop, or a termination request to the
   Listener or Worker.
4. Read the exact runner labels until absence is positively observed or a
   bounded failure policy reports `WithdrawalBlocked`.
5. Allow the active Worker to finish naturally.
6. Report `DRAINED` only after selector absence, Worker absence, and consistent
   local binding evidence are all observed.

### Idle withdrawal and racing assignment

1. Persist desired `DRAINED` and enter `WithdrawRequested`.
2. Remove only the reserved selector and read the exact runner labels.
3. If a racing assignment or Worker appears, represent it as `DrainPending` and
   allow completion.
4. Otherwise report `DRAINED` when the required composite evidence is present.

Listener shutdown is not the normal admission control point. It may remain a
separate exact-bound lifecycle or recovery operation, subject to its own
authority, but ordinary withdrawal neither depends on it nor uses it to settle
an active job.

### Re-advertisement

Desired `FULL` requests addition of only the reserved selector. Mutation
acknowledgement is insufficient: achieved `FULL` requires a positive readback
showing the selector present on the exact runner plus consistent local and
identity evidence. Unknown selector state remains `Advertising`, `Unknown`, or
`Refused`; it never becomes `FULL`.

## Reserved admission selector

The public v0.1 reserved name is exactly lowercase `runnermesh-admit`.

Namespace audit:

- GitHub's default runner labels are `self-hosted`, operating-system labels,
  and architecture labels; `runnermesh-admit` is not a default label.
- GitHub custom labels are case-insensitive, so all comparisons use a canonical
  case-insensitive form and configuration must use the lowercase spelling.
- GitHub does not attach product ownership metadata to a custom label. Name
  reservation is therefore a RunnerMesh contract, not server-enforced
  ownership.
- enrollment must bind the exact account/repository or organization scope,
  exact runner identity, and reserved label in a durable non-secret ownership
  record;
- readiness must prove no other runner visible in that scope carries the
  selector. If another runner carries it, RunnerMesh refuses eligibility claims
  and never removes it from that unrelated runner;
- if the exact runner already carries the selector without a matching ownership
  record, ownership is ambiguous and mutation is refused;
- an unused server-side label name is not itself capacity. GitHub may retain an
  unused custom label temporarily before deleting it.

RunnerMesh may read labels, add `runnermesh-admit`, or remove
`runnermesh-admit` on the exact configured runner. It must not use the REST
operations that replace all labels or delete all custom labels. It must not
mutate unrelated labels, runners, runner groups, registration, or repository
access.

## GitHub authority and credential boundary

The control binding contains only non-secret identity metadata:

```text
registration_scope=<organization | repository>
account_owner=<configured owner or organization>
repository=<required only for repository scope>
runner_id=<exact server runner id>
runner_name=<expected display identity>
reserved_label=runnermesh-admit
credential_ref=<opaque provider reference>
```

The runner ID must resolve exactly once in the configured scope, and its
expected identity must match before any mutation. Name-only lookup is never
mutation authority. Runner disappearance, duplicate/ambiguous identity,
registration drift, or a selector collision enters `Refused`.

For organization runners, the least documented fine-grained permission is
organization `Self-hosted runners: write`. For repository runners, it is
repository `Administration: write`. Classic-token scopes are broader and are
not the preferred product contract. The public abstraction remains
provider-neutral and exposes only exact-runner label observation/add/remove;
it does not expose generic GitHub administration.

Normal JSON configuration stores an opaque credential reference, never a token.
On Windows the product adapter must use an OS-backed secret facility such as
Windows Credential Manager or a DPAPI-backed store. A provider-neutral secret
resolver may return short-lived credential material to the request adapter,
but bearer values must not be logged, serialized into snapshots/receipts, or
persisted beside normal configuration. Creating, selecting, or rotating a real
credential is an explicit Owner setup action outside G11R-A/B/C.

Credential expiry or revocation is an authentication failure, not a drained
state. Authentication failures stop automatic mutation attempts until the
credential state changes or bounded revalidation is requested. API
unavailability and timeouts preserve the last known observation but withdraw
achieved-state claims when freshness is insufficient. Rate limits honor the
provider's retry/reset guidance. All transient retries are bounded, use
backoff, and surface a stable reason code; none fall back to local destructive
shutdown.

## Failure policy and reason codes

If the selector is known present and ordinary withdrawal cannot remove and
confirm it, RunnerMesh reports `WithdrawalBlocked` (or an equivalent
withdrawing state plus the same reason) and keeps the Worker alive.

Stable reason families include:

```text
admission.api_unavailable
admission.authentication_failed
admission.rate_limited
admission.selector_observation_unknown
admission.runner_unavailable
admission.runner_identity_drift
admission.registration_drift
admission.reserved_label_ownership_drift
admission.selector_collision
admission.local_evidence_inconsistent
```

CLI text, stable JSON, and Tray presentation consume these typed reasons from
the authoritative `AgentSnapshot`. Presentation strings are not behavior.

## Restart and recovery

Persist desired `UserMode`/Zen intent and the non-secret admission binding.
After an Agent restart, independently reconstruct:

1. desired admission state;
2. exact runner identity and registration scope;
3. observed reserved-selector state;
4. observed GitHub link/control state;
5. exact local Listener/Worker, image, home, registration, and work-root state.

A remembered mutation attempt or acknowledgement is not proof of its outcome.
Unknown selector state fails closed and cannot yield achieved `FULL` or
`DRAINED`. Exact drift refuses mutation. An unrelated same-name process remains
outside authority.

## Workflow contract

RunnerMesh remains GitHub Actions native: it does not parse workflows, schedule
jobs, implement the runner protocol, or proxy source/logs/artifacts. The prior
"zero developer workflow change" wording is narrowed, however. Every job meant
to consume RunnerMesh-managed workstation capacity must require the reserved
selector:

```yaml
runs-on:
  - self-hosted
  - Windows
  - X64
  - runnermesh-admit
```

GitHub documents cumulative label matching: an eligible runner must match all
specified labels. A job that uses only generic labels bypasses RunnerMesh's
admission selector and is outside the managed-capacity guarantee.

## Safety properties

The source model and G11R-B implementation must prove:

1. **P1** — normal withdrawal never destructively kills an active Worker;
2. **P2** — `DRAINED` is never reported while the selector is known present;
3. **P3** — `DRAINED` is never reported while an exact bound Worker is active;
4. **P4** — mutation/readback uncertainty never becomes success;
5. **P5** — a racing assignment is represented conservatively, not hidden;
6. **P6** — selector-removal acknowledgement alone does not imply work ended;
7. **P7** — restart reconstructs desired, remote, and local state separately;
8. **P8** — runner identity drift refuses mutation;
9. **P9** — reserved-label ownership drift refuses destructive correction;
10. **P10** — unrelated labels are never mutated;
11. **P11** — unrelated same-name Listener/Worker processes remain outside
    authority;
12. **P12** — `FULL` re-advertisement requires selector-presence confirmation.

These are product safety properties, not a claim about undocumented upstream
scheduler ordering. Trusted H1 must later exercise the observable routing
behavior and preserve independent qualification/restore results.

## Upstream evidence and limits

Official GitHub documentation establishes that:

- a self-hosted runner must match every label in `runs-on` to be eligible;
- custom labels can be added and individually removed from an exact runner;
- labels are case-insensitive and unused custom labels are automatically
  deleted after a retention period;
- organization add/remove operations require fine-grained `Self-hosted
  runners: write`, while repository operations require `Administration: write`;
- the API also offers replace-all/remove-all operations, which RunnerMesh
  explicitly forbids.

References:

- [Using self-hosted runners in a workflow](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/use-in-a-workflow)
- [Using labels with self-hosted runners](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/apply-labels)
- [REST API endpoints for self-hosted runners](https://docs.github.com/en/rest/actions/self-hosted-runners?apiVersion=2026-03-10)
- [Self-hosted runner routing reference](https://docs.github.com/en/actions/reference/runners/self-hosted-runners)

Those sources do not document a concurrency guarantee ordering a label API
response or readback against an assignment already in flight. This ADR
therefore claims only observed selector state and RunnerMesh eligibility, with
the conservative race policy above.

Historical official-runner source audit also showed why normal local signals
are excluded: Ctrl+C/service shutdown can cancel or ultimately kill active work,
and `run --once` leaves idle withdrawal unresolved and carries a deprecation
warning. Ephemeral/JIT gives a different registration lifecycle and remains
beyond v0.1.

## PR #17 salvage map

PR #17 exact head audited: `a8a028e472ff1271003ee161b7307c3e70818b40`.

| Component | Disposition | Reason |
|---|---|---|
| Exact runner-home and executable-image scoping | `KEEP_WITH_REFACTOR` | Bind local evidence to the accepted admission model |
| Registration/image/work-root fingerprints | `KEEP_WITH_REFACTOR` | Preserve drift refusal without permanently pinning a supported runner update |
| Bounded port/fake seam | `KEEP_WITH_REFACTOR` | Useful for deterministic exact-scope tests |
| Ctrl+C / Ctrl+Break rejection for ordinary Busy drain | `KEEP_AS_IS` | No ordinary withdrawal signal is permitted |
| Busy no-signal behavior | `KEEP_AS_IS` | Active normal work must finish |
| `run.cmd --once` production assumption | `TEST_EVIDENCE_ONLY` | Incomplete idle semantics; not an architectural requirement |
| Safe-wait reconstruction | `KEEP_WITH_REFACTOR` | Safe under uncertainty when combined with remote observation |
| Idle-withdrawal refusal | `SUPERSEDE` | The selected label protocol handles idle withdrawal |
| Process ancestry/scoping | `KEEP_WITH_REFACTOR` | Process name/ancestry alone never grants authority |
| G11-only CLI/executor activation | `DELETE_IN_REPLACEMENT` | Do not ship a qualification switch as the product lifecycle |
| Historical run-once ADR | `SUPERSEDE` | It left idle withdrawal unproven and was never merged |
| Historical local/host qualification | `TEST_EVIDENCE_ONLY` | It informs tests but is not current host acceptance |

PR #17 is salvage-only and must not be merged as-is or used as the predecessor
for G11R-B.

## Consequences and non-goals

G11R-B is authorized after this ADR merges. It must implement a typed,
exact-scope admission backend, a synthetic backend, Agent integration, error
states, restart reconstruction, and the safety properties above. G11R-C then
adapts the single readiness/H1 transaction family to this architecture.

G11R-A/B/C do not create or use a real credential, mutate labels/groups or
registration, dispatch H1, change branch protection, start/stop a service, kill
a Listener/Worker, or mutate a real work root or production runtime.
