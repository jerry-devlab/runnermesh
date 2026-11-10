# P01 — v0.1 H2 Owner preflight (WAITING_FOR_OWNER)

`PRODUCTION_MUTATION=false` · prepare-only · does **not** authorize H2 cutover.

## Exact RC (do not rebuild)

```text
G15R_RC_HEAD=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
G15R_HOSTED_RUN=42452639359
G15R=PASS
AUTHORITATIVE_MAIN_AT_PLAN=d2b37444342d085e650f996e5fb4322f775bbe6f
```

Docs-only main advancement after the RC does **not** invalidate the immutable RC.
Do not silently rebuild or retag this RC for LRM work.

## Reverification (source train, read-only)

```text
REVERIFIED_AT=2026-11-08T14:50Z_approx
REVERIFY_COMMIT_API=PASS  # fdd9fe6b… still resolves
REVERIFY_HOSTED_RUN=PASS  # run 42452639359 conclusion=success headSha=fdd9fe6b…
RC_REBUILD=NOT_PERFORMED
```

## Still required before live H2

```text
INDEPENDENT_SECURITY_REVIEW=PENDING   # Cursor self-review is not acceptance
OWNER_TOKEN=REQUIRED_NEW              # prior H2 token consumed by historical attempt
H2_CUTOVER=WAITING_FOR_OWNER
H2_RESTORE_PLAN=required_in_private_Owner_packet
DOGFOOD_ELAPSED_24H=not_started
G17=PREPARED_CHECKLIST_ONLY   # see goals/RM-160H-P01-G17-RELEASE-PREP.md
H3=WAITING_FOR_OWNER
```

## Operator packet fields (private offline)

Exact installed path, private runner identity, prestate, rollback steps, STOP
conditions, remote/local readback commands. Never paste private paths/IDs into
this public file.

## Public-safe readiness checklist (no private values)

- [ ] Independent security review bound to exact G15R RC HEAD (not Draft #65 tip)
- [ ] Fresh Owner token issued for this RC (prior token consumed)
- [ ] Private packet lists prestate capture + restore plan + STOP conditions
- [ ] Rollback rehearsal documented offline; restore disposition separate from cutover
- [ ] Dogfood clock starts only after H2_CUTOVER=PASS and H2_RESTORE=PASS
- [ ] G17/H3 remain blocked until dogfood elapsed ≥24h ordinary use

## Program disposition

```text
P01_SOURCE_PREP=READY_FOR_OWNER_PACKET
P01_LIVE=WAITING_FOR_OWNER
BOUND_PASS_ALL_TIP=c3f939611b4028cf565ec1d441112467d0187e07
```

Continue LRM P02–P06 / GT-02 source while H2 remains Owner-gated.
