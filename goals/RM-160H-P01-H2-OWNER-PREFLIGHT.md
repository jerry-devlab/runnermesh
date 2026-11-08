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

## Program disposition

```text
P01_SOURCE_PREP=READY_FOR_OWNER_PACKET
P01_LIVE=WAITING_FOR_OWNER
```

Continue LRM P02–P06 source while H2 remains Owner-gated.