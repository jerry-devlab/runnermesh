# G11 — Real Runner Lifecycle and Graceful Drain

Human Gate: **H1 — explicit Owner authorization required before mutation**.

## Mission

Qualify real official GitHub Actions runner behavior needed by v0.1 supervision
in a trusted controlled lane. The bounded executor uses the run-once job-lease
model in [ADR 0004](../docs/adr/0004-g11-run-once-job-lease.md); it does not
claim a signal-based drain or race-free idle withdrawal.

## Prove

- user-session start;
- connected/listening evidence;
- trusted job accepted;
- Busy observed;
- Busy drain requested without signalling the active Listener or Worker;
- the active run-once job completes and its Listener exits naturally;
- active job not destructively killed;
- job completes;
- Listener reaches desired drained/offline state;
- restart/reconnect;
- Agent adoption/reconciliation;
- one-execution-identity/one-owned-work-root invariant;
- exact restoration or rollback on qualification failure.

## Hard boundaries

No untrusted public PR code. No weakening runner/Organization access. No silent registration change. No global Git `safe.directory` workaround. Capture prestate and rollback before mutation.

`CTRL_BREAK_BUSY_DRAIN=REJECTED`. If future idle-withdrawal atomicity cannot be
proven, its disposition is `UNPROVEN`; do not manufacture a pass or weaken the
frozen product invariant.

## Risk vector

Real runner control + ownership/trust.

## Exit

One exact-head trusted qualification proves the semantics required by G12-G15.

Next: G12 only after PASS/restored state.
