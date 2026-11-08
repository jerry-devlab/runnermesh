# ADR 0006 (Draft): Per-runner scoped admission selectors

- Status: **Draft — not accepted; does not amend ADR 0004/v0.1**
- Date: 2026-11-08
- Relates: ADR 0004, ADR 0005, RM-LRM-PRODUCTIZATION-TRAIN-001

## Context

ADR 0004 reserves a single `runnermesh-admit` label with one owning identity per
GitHub registration scope. Local multi-runner management (ADR 0005) can enroll
multiple exact runners on one workstation. Same-scope managed admission therefore
collides under the frozen v0.1 selector.

## Decision draft (not approved)

Investigate a **per-runner selector namespace** of the form
`runnermesh-admit-<stable-local-key>` only after:

1. independent security review of label ownership and GitHub permission scope;
2. synthetic migration proofs for workflows that currently require `runnermesh-admit`;
3. explicit Owner acceptance of a new ADR superseding this draft.

Until acceptance:

- same-scope dual managed enrollment remains **refused**;
- discovery may observe collisions but must not auto-rewrite selectors;
- no production workflow `runs-on` mutation is authorized by this draft.

## Consequences

- Source may simulate namespaced migration (`scoped_admission_sim`) as unapproved.
- Live label writes remain Owner-gated and out of this Train.
- v0.1 dogfood continues to use exact `runnermesh-admit` for one identity.
