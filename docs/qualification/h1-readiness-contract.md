# H1 Qualification Readiness Contract

Status: **mechanism-neutral prototype; not accepted H1 readiness**

This document prepares one product-independent H1 transaction family without
mutating a real runner, service, registration, work root, repository selector,
or production runtime. It does not make G11R-C ready while G11R-A and G11R-B
remain incomplete.

## Single entrypoint

The source-only equivalent of `runnermesh qualify readiness` is the pure
`qualify_readiness` library function. It accepts typed evidence and returns a
schema-versioned receipt. It performs no discovery or mutation itself; future
adapters must collect evidence before calling it.

All fields are three-state evidence, not booleans guessed from absence:

```text
SOURCE_READY=<PASS|FAIL|UNKNOWN>
HOST_PRESTATE_READY=<PASS|FAIL|UNKNOWN>
ROUTING_READY=<PASS|FAIL|UNKNOWN>
TRUSTED_WORKFLOW_READY=<PASS|FAIL|UNKNOWN>
ROLLBACK_READY=<PASS|FAIL|UNKNOWN>
RECOVERY_READY=<PASS|FAIL|UNKNOWN>
SELECTOR_UNIQUE=<PASS|FAIL|UNKNOWN>
OWNER_GATE_READY=<PASS|FAIL|UNKNOWN>
```

Only eight `PASS` values under the supported schema produce:

```text
DISPOSITION=READY_FOR_OWNER_GATE
H1_MUTATION_ALLOWED=true
```

Any `FAIL`, `UNKNOWN`, or schema mismatch produces `BLOCKED`. The prototype is
executable in `src/qualification.rs` and currently carries no host adapter.

## Evidence meanings

| Check | Required proof |
|---|---|
| `SOURCE_READY` | exact accepted G11R-B head, exact-head hosted CI, lifecycle/race fixtures, privacy pass |
| `HOST_PRESTATE_READY` | exact service/config/security, runner home, registration, image, execution identity, work root, exact Listener/Worker scope, containment |
| `ROUTING_READY` | accepted G11R-A mechanism, exact selector semantics, required server authority present without exposing credentials |
| `TRUSTED_WORKFLOW_READY` | predefined private workflow bytes/hashes, bounded modes, trusted trigger and source boundary |
| `ROLLBACK_READY` | exact baseline, bounded reverse operations, result-independent automatic restore path |
| `RECOVERY_READY` | durable phase state and a restartable recovery controller with exact admission checks |
| `SELECTOR_UNIQUE` | one intended runtime target can match and the job asserts its exact target at runtime |
| `OWNER_GATE_READY` | frozen source/workflows/transaction, exact bounded Owner command, no unresolved evidence |

Current prototype disposition:

```text
SOURCE_READY=UNKNOWN
HOST_PRESTATE_READY=UNKNOWN
ROUTING_READY=UNKNOWN
TRUSTED_WORKFLOW_READY=UNKNOWN
ROLLBACK_READY=PROTOTYPE_ONLY
RECOVERY_READY=PROTOTYPE_ONLY
SELECTOR_UNIQUE=UNKNOWN
OWNER_GATE_READY=UNKNOWN
H1_MUTATION_ALLOWED=false
```

`PROTOTYPE_ONLY` is narrative here, not a machine evidence value. The machine
receipt represents it as `UNKNOWN` until an authorized adapter proves it.

## Trusted workflow family

The actual workflow must live in a private/trusted repository selected during
the future Owner preparation. Public pull-request code must not be eligible to
run on the persistent personal workstation.

Use one predefined `workflow_dispatch` family with a closed mode input:

```text
mode = primary | no-admission | reconnect | controlled-failure
selector_profile = primary
candidate_sha = exactly 40 lowercase hexadecimal characters
transaction_id = bounded machine token
```

Requirements:

- `mode` and `selector_profile` are workflow `choice` inputs, not arbitrary
  shell fragments;
- every shell branch is a predefined literal selected by a typed case;
- `candidate_sha` and `transaction_id` are validated before use and never
  interpolated into an executable command name;
- the workflow definitions are hashed and frozen before the Owner transaction;
- the exact routing selector is defined in the private H1 envelope;
- the first job step asserts the intended runtime runner identity using private
  expected evidence and stops on mismatch;
- job and workflow timeouts are bounded;
- no `pull_request` or public-fork trigger is present;
- no workflow input permits arbitrary commands, paths, labels, repositories, or
  scripts;
- secrets and exact private selector values never enter public receipts.

If the accepted admission architecture requires creating a label, runner
group, or other server selector, the readiness verifier records the precise
future Owner action. It does not perform that action.

## One H1 transaction family

```text
readiness receipt with eight PASS values
-> freeze exact source/workflow/selector/rollback identities
-> one explicit Owner authorization
-> repeat exact prestate check
-> begin durable host-mutation phase
-> dispatch one predefined witness mode
-> record QUALIFICATION independently
-> always enter automatic restore after mutation began
-> verify restored baseline
-> record RESTORE independently
-> finish or require emergency Owner recovery only when restore fails
```

The durable phase vocabulary is:

```text
PREPARED
HOST_MUTATION_STARTED
WORKFLOW_RUNNING
RESTORE_PENDING
RESTORING
COMPLETE
RECOVERY_REQUIRED
```

The final receipt always separates:

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
```

A qualification `FAIL` with restore `PASS` is a safely completed negative
experiment. `RECOVERY_REQUIRED` is reserved for restore `FAIL`, including
ownership ambiguity after mutation or restore interruption.

## Failure-injection model

The pure `H1TransactionModel` covers the required sandbox/fake families:

| Injection | Qualification | Restore behavior |
|---|---|---|
| Failure before host mutation | `BLOCKED` | `PASS` after unchanged-baseline verification; no restore action claimed |
| Failure after mutation begins | `BLOCKED` | durable controller enters automatic restore |
| Workflow never dispatches | `BLOCKED` | automatic restore attempted |
| Workflow fails | `FAIL` | automatic restore attempted |
| Job timeout | `BLOCKED` | automatic restore attempted |
| Controller loss | `BLOCKED` | reconstructed controller enters automatic restore |
| Agent loss | `BLOCKED` | independent transaction controller enters automatic restore |
| Restore interruption | prior result retained | `FAIL`; emergency Owner recovery required |
| Unrelated runner present | unchanged | no action toward unrelated runner |
| Ambiguous ownership before mutation | `BLOCKED` | baseline unchanged, no mutation |
| Ambiguous ownership after mutation | `BLOCKED` | `FAIL`; refuse unowned automatic action and require recovery |

No failure-injection test calls the real service, runner, GitHub API, work root,
or production installation.

## Automatic restore contract

The transaction controller, not the Agent process alone, owns the durable
restore phase. Loss of the Agent or initiating controller is not evidence that
no mutation occurred. A restarted controller reads the durable phase and exact
poststate, then either resumes the exact restore operation or refuses into
`RECOVERY_REQUIRED`.

Restore operations must be:

- precomputed from the captured exact baseline;
- ordered so the admission/routing barrier is restored deliberately;
- scoped to the exact service, registration, runner home, process images, work
  root, selector, and transaction identity;
- idempotent under verified poststate;
- refusing on ambiguous identity or unrelated content;
- independently verified after completion.

The historical G11 incident contributes only generic design lessons: prepare
routing before host mutation, bind exact ownership rather than global process
counts, persist transaction phase before mutation, and do not treat controller
loss as proof of no side effect. No historical helper is reused by this design.

## Evidence split

Public receipt fields may include:

- exact public source SHA and PR;
- public workflow-definition schema version or privacy-safe hash;
- readiness check dispositions;
- qualification/restore dispositions;
- whether real host mutation occurred;
- blocker class and next Owner action category.

Private H1 evidence contains the exact trusted repository/workflow identity,
runner and selector identities, service/config/security evidence, local paths,
execution identity, work-root identity, transaction artifacts, and recovery
command. Those values must not be copied into public code, logs, PRs, or the
execution ledger.

## Remaining blockers

This prototype cannot become G11R-C acceptance until:

1. G11R-A has an accepted mechanism with no unresolved design-freeze change;
2. G11R-B implements that mechanism and passes exact-head lifecycle/race gates;
3. mechanism-specific read-only source/host/routing adapters exist;
4. private trusted workflow bytes and unique selector evidence exist;
5. the exact rollback/recovery controller is implemented and sandbox-qualified;
6. a final readiness receipt has eight `PASS` values.

Until then:

```text
H1_READINESS_VERIFIER=PROTOTYPE
H1_TRANSACTION_FAMILY_READY=false
OWNER_GATE_READY=UNKNOWN
H1_EXECUTED=false
REAL_HOST_MUTATION=false
```
