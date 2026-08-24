# RunnerMesh Fast Lane

Authority: `QUALITY_GATES.md`.

## Core rule

```text
classify changed risk
-> implement/focused tests
-> settle one candidate
-> run one final gate per active risk dimension
-> reuse unchanged-risk evidence
-> merge
```

## Quick classes

- **D docs/governance**: diff/link/privacy sanity; product gates `N/A`.
- **C ordinary code**: focused tests + one hosted code CI.
- **V tray**: C + one representative Windows tray/presentation proof.
- **P probes/policy**: C + deterministic evidence/policy families; live read-only proof only when needed.
- **R runner control**: C + synthetic lifecycle fixture; real-runner proof only for changed real semantics and only at an authorized gate.
- **S persistent config/autostart**: ownership/atomicity/idempotence/drift/restore family.
- **I install/update**: staging/verification/durable transaction/rollback/source-runtime isolation family.
- **X security/privacy**: focused independent review.
- **L release**: exact RC/artifact/provenance/publication closure; reuse unchanged product evidence.

## Evidence reuse

If relevant risk diff is empty:

```text
<GATE>=REUSED
<GATE>_REUSED_FROM=<sha>
<GATE>_RISK_DIFF=EMPTY
```

Do not rerun simply because HEAD advanced.

## Blockers

Latch one sufficient unchanged blocker:

```text
BLOCKER_LATCHED=true
```

Do not spin on it.

## Automatic repair budget

Ordinary compile/test/fmt/clippy/deterministic-fixture/hosted-CI defects may receive at most three materially distinct repair cycles per failure family. After that, stop with a blocker fingerprint.

## Human gates

Stop unattended work before UAC, real Windows Service mutation, real runner registration/work-root destructive mutation, Organization runner-access changes, production autostart activation, installed stable-runtime mutation, production cutover, public release publication, destructive active-job termination, secrets/trust changes, or ambiguous ownership/security state.
