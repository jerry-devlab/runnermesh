# H1 qualification readiness contract

Status: **G11R-C plus H1 live-adapter source; live H1 remains Owner-gated**

G11R-C prepares one fail-closed H1 transaction family for the accepted
GitHub-native `runnermesh-admit` architecture. Source and synthetic fixtures do
not mutate a real runner, label, credential, service, registration, work root,
workflow, or production runtime, and they do not authorize H1.

## Readiness verifier

`verify_h1_readiness` accepts three-state evidence:

```text
SOURCE_READY=<PASS|FAIL|UNKNOWN>
HOST_PRESTATE_READY=<PASS|FAIL|UNKNOWN>
GITHUB_AUTHORITY_CONFIGURED=<PASS|FAIL|UNKNOWN>
EXACT_RUNNER_IDENTITY_READY=<PASS|FAIL|UNKNOWN>
RESERVED_SELECTOR_READY=<PASS|FAIL|UNKNOWN>
SELECTOR_UNIQUE=<PASS|FAIL|UNKNOWN>
TRUSTED_WORKFLOW_READY=<PASS|FAIL|UNKNOWN>
ROUTING_READY=<PASS|FAIL|UNKNOWN>
ROLLBACK_READY=<PASS|FAIL|UNKNOWN>
RECOVERY_READY=<PASS|FAIL|UNKNOWN>
OWNER_GATE_READY=<PASS|FAIL|UNKNOWN>
```

Any `FAIL`, `UNKNOWN`, or schema mismatch returns `BLOCKED`. Evidence also has
explicit `LIVE` or `SYNTHETIC` provenance:

- all-pass `SYNTHETIC` evidence returns `PASS_SYNTHETIC` and always sets
  `h1_mutation_allowed=false`;
- only all-pass `LIVE` evidence returns `READY_FOR_OWNER_GATE` and can prepare
  the future transaction; and
- the transaction still requires the explicit `OwnerGateAccepted` event before
  progressing.

The source suite proves the synthetic verifier and provides collectors for the
future live evidence. It does not invent missing Owner evidence or convert
source capability into a live PASS.

## Live-adapter source boundary

The prepared source layer consists of:

- an exact `H1LiveBinding` for GitHub scope, runner ID/name, canonical reserved
  selector, opaque credential and execution-identity references, local runner
  home, work root, Listener/Worker images, trusted workflow identity, and
  restore/recovery receipt identities;
- a fixed-authority GitHub REST transport and Windows WinHTTP client;
- a Windows Credential Manager provider behind an injectable read-only store
  boundary;
- an exact local filesystem/identity collector behind an injectable ownership
  verifier;
- GET-only exact-runner, selector-uniqueness, immutable workflow-content, and
  routing verifiers; and
- `collect_h1_live_readiness`, which maps the typed observations into the
  existing eleven-field `H1ReadinessEvidence` contract.

Binding and response drift fail closed. The workflow client can only read the
configured file at an immutable commit reference; it has no dispatch method.
That read proves source identity but leaves the private runtime runner-name
variable `UNKNOWN` until a separate Owner-side verifier proves its exact value.
The admission client retains only exact-runner observation and add-one/remove-
one reserved-label operations with positive readback. The source adapters are
not activated by default and tests inject only synthetic credentials and fake
HTTP responses.

## Evidence meanings

| Check | Required live proof |
|---|---|
| `SOURCE_READY` | exact accepted G11R-B/G11R-C source head, exact-head hosted Windows and Ubuntu CI, lifecycle/race fixtures, and public privacy pass |
| `HOST_PRESTATE_READY` | exact service/config/security metadata, runner home, registration fingerprint, execution identity, work root, exact Listener/Worker image scope, and contained qualification workspace |
| `GITHUB_AUTHORITY_CONFIGURED` | non-secret reference resolves through the approved OS-backed provider to the least required runner-label authority, without logging or normal-JSON secret storage |
| `EXACT_RUNNER_IDENTITY_READY` | configured scope plus exact runner ID/name and local runner-home/image/work-root binding agree |
| `RESERVED_SELECTOR_READY` | the canonical `runnermesh-admit` binding and ownership receipt are valid for the exact runner |
| `SELECTOR_UNIQUE` | no unrelated runner in the bound scope carries the reserved selector and the intended route resolves to one exact target |
| `TRUSTED_WORKFLOW_READY` | frozen workflow bytes exist in the Owner-selected trusted private repository with dispatch-only trigger and exact runtime-identity assertion |
| `ROUTING_READY` | primary, withdrawn/no-new-admission, and reconnect witnesses can be observed without broad runner or group mutation |
| `ROLLBACK_READY` | exact original baseline and bounded, idempotent reverse operations are frozen before mutation |
| `RECOVERY_READY` | durable phase state can resume exact restore or refuse ambiguous ownership into Owner recovery |
| `OWNER_GATE_READY` | exact source/workflow/transaction identities and one bounded Owner command are frozen with no unresolved evidence |

## Inert trusted-workflow template

[`templates/h1-workflow.yml`](templates/h1-workflow.yml) is an inert source
template, not an active public workflow. It has only `workflow_dispatch`, no
public-pull-request or automatic trigger, no arbitrary command/script input,
and no secret context. Its `runs-on` contract is cumulative:

```yaml
runs-on:
  - self-hosted
  - Windows
  - X64
  - runnermesh-admit
```

The first step validates the exact accepted source SHA, a bounded transaction
token, the private qualification-repository identity, and `${{ runner.name }}`
against Owner-configured private repository variables. Fixed `primary`,
`no-new-admission`, and `reconnect` choices are witnesses inside one transaction
family; none accepts an arbitrary shell command.

For the withdrawn witness, the future transaction controller dispatches after
positive selector-absence readback and expects the job to remain unassigned for
the bounded witness interval. If that job reaches the target, the template
fails immediately. Remaining queued is evidence only under the frozen route and
bounded interval; the contract does not describe label readback as a globally
linearizable scheduler barrier.

This Goal does not know or publish the trusted private repository identity and
does not install or dispatch the template. Until the Owner establishes that
separate boundary, `TRUSTED_WORKFLOW_READY`, `ROUTING_READY`, and
`OWNER_GATE_READY` remain `UNKNOWN`.

## One H1 transaction family

The only prepared family is:

```text
h1-github-native-admission-label-v1
```

Its required sequence is:

```text
all live readiness gates PASS
-> Owner gate accepted
-> establish or verify exact reserved-label control
-> qualify advertised capacity
-> run the primary trusted job
-> request withdrawal and remove the reserved selector
-> confirm selector absence
-> observe the bounded no-new-admission/racing witness
-> allow any racing active Worker to complete naturally
-> confirm achieved DRAINED
-> request re-advertisement and confirm selector presence
-> pass the exact reconnect witness
-> automatically restore and verify the original baseline
-> emit independent qualification and restore results
```

A selector-removal acknowledgement alone cannot advance to `DRAINED`. A racing
assignment advances to `DRAIN_PENDING`; it must complete before the separate
achieved-drained observation. No phase signals or kills an active Worker.

The durable transaction phase vocabulary is:

```text
PREPARED
OWNER_AUTHORIZED
ADMISSION_CONTROL_ESTABLISHED
ADVERTISED
PRIMARY_JOB_RUNNING
PRIMARY_JOB_COMPLETED
WITHDRAWING
SELECTOR_ABSENT
DRAIN_PENDING
NO_NEW_ADMISSION_WITNESSED
DRAINED
RE_ADVERTISING
RE_ADVERTISED
RESTORE_PENDING
RESTORING
COMPLETE
RECOVERY_REQUIRED
```

The public receipt always separates:

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
```

Normal failure after the Owner gate cannot produce a terminal receipt until
automatic baseline restore is attempted and independently resolved. Restore
failure or ownership ambiguity after the gate enters `RECOVERY_REQUIRED` and
requires a separate exact-bound Owner recovery decision.

## Synthetic failure injection

The in-memory model proves:

| Injection | Qualification | Restore |
|---|---|---|
| blocker before Owner gate | `BLOCKED` | unchanged baseline `PASS`; no restore action claimed |
| workflow failure | `FAIL` | automatic restore attempted |
| route unavailable | `BLOCKED` | automatic restore attempted |
| active job timeout | `BLOCKED` | automatic restore attempted |
| Agent/controller loss | `BLOCKED` | restore pending; interrupted restore fails closed |
| restore failure/interruption | prior result retained | `FAIL`, Owner recovery required |
| racing assignment | pending | natural completion required before `DRAINED` |
| unrelated runner observed | unchanged | zero unrelated-runner control actions |
| ownership ambiguity | `BLOCKED` | refuse unowned correction; recovery required if past gate |

All failure injection uses synthetic state. No real network request,
credential-store read, service adapter, runner-control adapter, or workflow
dispatch is invoked.

## Public/private evidence split

Public source and receipts may contain schema/family IDs, public source SHA/PR,
gate dispositions, qualification/restore results, mutation booleans, and a
privacy-safe blocker class.

The private future H1 envelope owns the exact qualification repository and
workflow identity, runner ID/name, credential reference, service/configuration
metadata, runner-home/work-root paths, execution identity, selector ownership
receipt, transaction artifacts, and recovery command. Those values are never
copied into public code, PRs, logs, or the execution ledger.

## Current live disposition

The accepted source can establish a synthetic proof, ready adapter layer, and
ready source transaction family, but the frozen historical P0 incident and
absent Owner trust configuration keep live H1 fail-closed:

```text
H1_READINESS_VERIFIER=PASS_SYNTHETIC
H1_TRANSACTION_FAMILY_READY=true
H1_LIVE_ADAPTER_SOURCE_READY=true
SOURCE_READY=PASS
HOST_PRESTATE_READY=UNKNOWN
GITHUB_AUTHORITY_CONFIGURED=UNKNOWN
EXACT_RUNNER_IDENTITY_READY=UNKNOWN
RESERVED_SELECTOR_READY=UNKNOWN
SELECTOR_UNIQUE=UNKNOWN
TRUSTED_WORKFLOW_READY=UNKNOWN
ROUTING_READY=UNKNOWN
ROLLBACK_READY=UNKNOWN
RECOVERY_READY=UNKNOWN
OWNER_GATE_READY=UNKNOWN
ROLLBACK_SOURCE_MODEL=PASS_SYNTHETIC
RECOVERY_SOURCE_MODEL=PASS_SYNTHETIC
H1_MUTATION_ALLOWED=false
LIVE_READINESS_EXECUTED=false
H1_EXECUTED=false
```

P0 recovery and H1 preparation remain separate future Owner transactions. This
Goal does not start/stop a service, invoke a recovery helper, touch the real
qualification workspace, change a runner label/group/registration, create a
credential, or dispatch H1.

`WAITING_FOR_OWNER` and `OWNER_CANCELED` remain outer control states, not H1
implementation failures. A later attempt must collect fresh live evidence and
use a fresh transaction identity and authorization; this source change does
not add a generic workflow engine or make an old transaction resumable.
