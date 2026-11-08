# P04 — Scoped selector decision prep (DRAFT / WAITING)

`PRODUCTION_MUTATION=false` · design prep only · **does not** accept ADR 0006.

## Frozen v0.1 authority (unchanged)

```text
ADR_0004=ACCEPTED
RESERVED_ADMISSION_LABEL=runnermesh-admit
SAME_SCOPE_DUAL_MANAGED_ENROLLMENT=REFUSED
LIVE_LABEL_MUTATION=OWNER_GATED
```

ADR 0006 remains **Draft**. This file only records the decision packet needed
before any Owner architecture acceptance. Cursor self-review is not acceptance.

## Decision questions Owner must answer (G-ADR006)

1. Stable local key material: opaque home token only, or opaque home+server-id?
2. Selector grammar: `runnermesh-admit-<key>` length/charset limits vs GitHub labels?
3. Migration: how existing workflows that require exact `runnermesh-admit` move?
4. Collision: refuse second same-scope enrollment forever, or migrate under review?
5. Readback: eventual visibility window and fail-closed rules for add/remove one?
6. Independent security review id bound to the exact candidate HEAD?

## Source already safe without acceptance

- Registry refuses same-scope managed admission collisions.
- Discovery surfaces `same_scope_collision` without auto-enroll.
- `scoped_admission_sim` remains unapproved simulation only.
- FakeGitHub / HUX-2 synthetic Busy/drain paths do not authorize live labels.

## Disposition

```text
P04_01_ADR0006=DRAFT_NOT_ACCEPTED
P04_02_V01_PRESERVE_RUNNERMESH_ADMIT=PASS_SOURCE
P04_07_HUMAN_GATE=WAITING_FOR_OWNER
NEXT=continue FakeGitHub Busy/Unknown/partial-rollback source under P04-05 without live writes
```
