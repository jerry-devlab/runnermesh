# G11R-C — Qualification Readiness

Type: **Autonomous readiness train**

Expected duration: **6-12 hours**

Real runner mutation: **forbidden until the final readiness receipt is complete**

## Mission

Prepare every source, routing, workflow, rollback, recovery, and evidence prerequisite for one-shot H1 qualification before the real workstation is mutated.

Principle: **prepare everything first; mutate the real host last.**

## Required readiness surfaces

### Source

- accepted G11R-B exact candidate;
- exact-head hosted CI;
- lifecycle/race fixtures PASS;
- public privacy PASS.

### Trusted routing

- trusted private qualification repository/workflow;
- exact target selector strategy;
- uniqueness proof or an Owner-established unique selector;
- runtime assertion that the dispatched job actually runs on the intended target;
- no public pull-request trigger or untrusted code path.

### Qualification witnesses

Pre-create and validate bounded workflows/modes for:

- primary trusted job;
- no-admission witness;
- reconnect witness;
- failure/timeout witness where useful.

Do not wait until after service/Listener mutation to discover routing prerequisites.

### Host prestate

Read-only verifier for:

- exact service identity/config/security;
- runner home and registration fingerprint;
- work root and execution identity;
- exact bound Listener/Worker process scope;
- unrelated runner isolation;
- qualification workspace containment.

### Rollback and recovery

- deterministic normal automatic restore path;
- controller/primary interruption handling;
- timeout handling;
- durable transaction state;
- emergency recovery only for states that cannot be automatically restored;
- exact recovery admission checks rather than global process counts.

### One-shot transaction

Generate one stable H1 transaction family. Do not create V5/V6/V7-style variants for every newly discovered prerequisite.

Qualification result and restoration result must be independent:

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
```

## Readiness gate

Real-host mutation is forbidden unless all are true:

```text
SOURCE_READY=true
HOST_PRESTATE_READY=true
ROUTING_READY=true
TRUSTED_WORKFLOW_READY=true
ROLLBACK_READY=true
RECOVERY_READY=true
SELECTOR_UNIQUE=true
OWNER_GATE_READY=true
```

A false/unknown field stops before service mutation.

## Failure injection

Use sandbox/fake fixtures to prove at least:

- transaction dies before mutation;
- transaction dies after mutation begins;
- qualification workflow fails;
- routing cannot dispatch;
- active job exceeds expected duration;
- controller disappears;
- normal restore succeeds after qualification failure;
- emergency recovery refuses ambiguous/unowned state.

## Hard boundaries

No UAC, real service mutation, real registration mutation, real work-root destructive mutation, Organization runner setting mutation, or production cutover in this Goal.

Owner may separately establish a unique trusted selector if the accepted G11R architecture requires a GitHub setting that cannot be created autonomously; once established, readiness must re-verify it before H1.

## Exit

Return a single H1 handoff with immutable candidate identity, transaction identity, readiness evidence, exact Owner command, and emergency-recovery contract.

Next: Human Gate H1 one-shot qualification.