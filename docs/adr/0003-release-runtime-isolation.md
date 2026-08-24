# ADR 0003: Release and Installed-Runtime Isolation

- Status: Accepted
- Applies to: v0.1 and later

## Decision

RunnerMesh freezes the invariant:

```text
SOURCE != BUILD != RELEASE != INSTALLED RUNTIME != ACTIVE VERSION
```

Mutable source trees and Cargo `target/` output are never production deployment sources. Autostart never points into a source tree. Production/dogfood activation uses an immutable release artifact or an explicitly authorized immutable RC artifact.

## Installation model

The user-level installation has:

- immutable `versions/<version>/` payloads;
- a small stable activation entry/shim under `bin/`;
- explicit `current.json` or equivalent active-version metadata;
- config/state/logs outside immutable version slots;
- durable update transaction receipts.

Stable and staged/canary payloads may coexist on disk, but exactly one Agent has runner-control authority.

## Upgrade model

```text
check -> stage -> verify -> validate -> READY_TO_ACTIVATE
      -> safe activation point -> activate -> health/reconcile -> commit
      -> rollback on failure
```

RunnerMesh update must not kill a normal active CI job. Control-plane-compatible updates may replace the Agent while leaving the official Listener/Worker running, followed by safe adoption. Execution-plane migrations wait for drain.

Loss of a synchronous child/helper completion signal is not proof that a transaction failed. Durable intent/receipts plus poststate reconciliation are required for privileged or activation transactions.

## Release source

A merge to `main` is not deployment. Tags/RC publication create immutable artifacts. Routine development cannot mutate installed stable runtime.

## GitHub runner update boundary

RunnerMesh does not become the updater for the official GitHub Actions runner. It must tolerate supported upstream runner updates and rediscover/reconcile.

## Consequences

- Source development can continue while stable RunnerMesh is deployed.
- Failed RCs can roll back to a validated prior slot.
- Development worktree state cannot silently become production state.
