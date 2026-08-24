# G14 — Update, Activation, and Rollback

## Mission

Implement durable staged update and rollback semantics without production activation.

## Deliver

- download/stage abstraction;
- checksum/artifact verification;
- compatibility validation;
- durable transaction intent and `READY_TO_ACTIVATE` state;
- safe activation indirection;
- health check/reconcile;
- commit receipt;
- rollback to prior validated slot;
- interruption/poststate reconciliation;
- active-job-safe activation deferral model.

Use sandbox install roots and synthetic Agent/runner state.

## Risk vector

Install/activation transaction safety.

## Gates

Failure-injection family covering interrupted stage/activate/health/commit and old-version recovery + hosted CI.

## Exit

A failed or interrupted activation cannot strand an unrecoverable install and ordinary active-job semantics are preserved.

Next: G15.
