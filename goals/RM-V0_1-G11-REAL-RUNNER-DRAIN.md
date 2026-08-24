# G11 — Real Runner Lifecycle and Graceful Drain

Human Gate: **H1 — explicit Owner authorization required before mutation**.

## Mission

Qualify real official GitHub Actions runner behavior needed by v0.1 supervision and graceful drain in a trusted controlled lane.

## Prove

- user-session start;
- connected/listening evidence;
- trusted job accepted;
- Busy observed;
- drain requested;
- no new capacity admitted after drain intent;
- active job not destructively killed;
- job completes;
- Listener reaches desired drained/offline state;
- restart/reconnect;
- Agent adoption/reconciliation;
- one-execution-identity/one-owned-work-root invariant;
- exact restoration or rollback on qualification failure.

## Hard boundaries

No untrusted public PR code. No weakening runner/Organization access. No silent registration change. No global Git `safe.directory` workaround. Capture prestate and rollback before mutation.

If the intended persistent-runner drain mechanism cannot be proven, disposition is `UNPROVEN`/`FAIL`; revise the implementation/ADR instead of manufacturing PASS.

## Risk vector

Real runner control + ownership/trust.

## Exit

One exact-head trusted qualification proves the semantics required by G12-G15.

Next: G12 only after PASS/restored state.
