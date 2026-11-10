# P01 — G17 / H3 release prep (WAITING_FOR_OWNER)

`PRODUCTION_MUTATION=false` · prepare-only · does **not** authorize publication.

## Immutable RC (do not rebuild)

```text
G15R_RC_HEAD=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
G15R_HOSTED_RUN=42452639359
G15R=PASS
```

Docs-only main advancement after this RC does not invalidate it.

## G17 evidence checklist (public-safe)

- [ ] Exact RC SHA and hosted run cited above remain authoritative
- [ ] H2 cutover + restore receipts exist (private Owner packet)
- [ ] Dogfood elapsed ≥24h ordinary use on authorized installed artifact
- [ ] Provenance: artifact hashes bound to RC, not worktree `target/`
- [ ] Release notes draft prepared offline; no private host IDs in public notes
- [ ] Independent security acceptance recorded for the cutover candidate

## H3 publication gate

```text
H3=WAITING_FOR_OWNER
PUBLICATION=NOT_AUTHORIZED
G17=PREPARED_CHECKLIST_ONLY
```

Do not tag `v0.1.0` or publish GitHub Release from this Train. Cursor self-review
is not independent security acceptance.

## Disposition

```text
P01_G17_SOURCE_PREP=READY_FOR_OWNER
P01_G17_LIVE=WAITING_FOR_OWNER
P01_H3=WAITING_FOR_OWNER
G15R_REVERIFY_2026-11-08=PASS_READ_ONLY
BOUND_SOURCE_TRAIN_TIP=c3f939611b4028cf565ec1d441112467d0187e07
```

Docs-only and LRM Draft tip advancement does not rebuild or retag G15R.
