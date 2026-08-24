# RunnerMesh Quality Gates

This file is the authoritative gate-selection policy. `FAST_LANE.md` is the compact execution reference.

## Core rule

```text
validate changed risk once
reuse unchanged-risk evidence
avoid Goal ceremony
```

## Risk vector

Every Goal classifies:

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

A Goal may activate several dimensions. Gates are selected from active dimensions, not from milestone names.

## D — docs/planning/governance

Required:

- bounded diff/cross-document consistency;
- Markdown/link sanity where applicable;
- public privacy audit.

Product runtime gates are `N/A` unless the change itself alters a product/security claim requiring independent review.

## C — ordinary Rust code

During iteration:

- `cargo fmt`;
- focused affected tests;
- `cargo check`/Clippy as useful.

At settled candidate:

- one normal hosted code CI on the candidate head.

Do not add tray, real-runner, or production gates unless another risk dimension is active.

## V — tray/presentation

Add one representative final Windows tray/presentation proof after the candidate settles. Prove changed menu IDs, localization/theme behavior, icon/tooltip semantics, and UI-thread ownership as applicable.

Do not rerun presentation proof for non-presentation changes.

## P — probe/policy

Require deterministic fixtures/table tests for normalized evidence, precedence, Unknown/Unavailable semantics, and reason codes. Add one focused live read-only proof only when a platform observation claim cannot be proven synthetically.

A probe implementation detail never becomes authoritative policy state directly.

## R — runner control

Require deterministic lifecycle/process fixtures first. A trusted real official-runner proof is required only when changed semantics involve actual start/listen/busy/drain/stop/reconnect/adoption behavior that cannot be proven synthetically.

Real-runner mutation is a Human Gate unless the Goal carries explicit Owner authorization.

## S — persistent config/autostart

Prove one ownership-safety family covering:

- exact ownership;
- minimal mutation;
- atomicity;
- idempotence;
- unrelated-content preservation;
- concurrent drift refusal;
- uninstall/restore safety.

Tests should use sandbox roots by default. Production autostart activation is a Human Gate.

## I — install/activation/update

Prove:

- immutable staging;
- artifact/checksum verification;
- explicit active-version selection;
- durable transaction intent/receipt;
- interruption reconciliation;
- failed-activation rollback;
- old-version recovery;
- source/build/release/runtime isolation;
- active-job-safe deferral model.

Use sandbox install roots until production dogfood is explicitly authorized.

## X — security/privacy

A focused independent review is required for changed trust boundary, secret handling, external destructive write authority, persistent personal-runner exposure, or public privacy boundary.

Public repository content must contain no private dogfood identifiers, credentials, private topology, or personal infrastructure evidence.

## L — release

Release closure freshly proves release-specific artifacts, checksums, provenance, package inspection, and publication. Reuse accepted unchanged-risk tray/probe/runner/config evidence. Add only a representative trusted final dogfood smoke where useful.

Public release publication is a Human Gate.

## One-final-candidate pattern

Preferred:

```text
implement
-> focused tests
-> settle candidate
-> one final hosted CI
-> one final additional gate per active risk dimension
-> merge
```

Avoid repeated full suites, repeated auditors, or repeated real-runner tests when the relevant risk did not change.

## Dedicated audit policy

A separate auditor is reserved for:

- changed destructive/persistent external writes;
- security/privacy boundary changes;
- high-risk concurrency/ownership with plausible corruption;
- ambiguous defects;
- production cutover;
- release/publication;
- explicit Implementer request.

No generic audit loop exists for routine development.

## Blocker latch

After one sufficient observation of an unchanged external blocker, record a stable fingerprint and:

```text
BLOCKER_LATCHED=true
```

Do not repeat the same expensive/destructive lane until source, evidence, trust state, Owner action, or the external prerequisite changes.

## Production mutation default

Ordinary source-development Goals default to:

```text
PRODUCTION_MUTATION=false
```

An installed stable RunnerMesh, official runner registration, work-root ownership, Organization runner access, and production autostart are outside unattended authority unless a specific Goal/Owner gate says otherwise.
