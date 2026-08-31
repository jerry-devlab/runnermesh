# RunnerMesh Fast Lane

Authority: `QUALITY_GATES.md`.

## Core rule

```text
classify changed risk
-> implement/focused local tests
-> self-review and settle one candidate
-> normally push once
-> run one final gate per active risk dimension
-> reuse unchanged-risk evidence
-> merge
```

Run the repository-owned local entrypoint from the settled candidate:

```text
python tools/quality/fast_gate.py --base <accepted-main>
```

On a governed Windows Conda host where the `python` app alias is disabled, use
`conda run -n base python` in place of `python`.

Use `--full` when candidate-level all-target tests and Clippy are required.
`DOCS_ONLY` runs the lightweight docs/public/diff gates without Cargo.  The
classifier's path hints are assistance only; the Goal still declares semantic
risk and any additional gate.

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

Independent review uses accepted prior evidence plus the current risk delta.
Routine code needs Implementer self-review, focused tests, and one candidate CI,
not an automatic independent auditor.  Aim for 2-5 minutes on an ordinary delta
and 10-20 minutes on material runner/source risk; trust/security and H1/H2/release
take the depth they require.  These aims never create an automatic PASS.

## Post-merge overlap

After merge, verify remote `main` and start the next safe Goal while main CI runs
asynchronously.  Do not merge the next PR until that prior main run passes; latch
a failure immediately. Full main-push CI remains required while the required PR
gate does not enforce an up-to-date base, even when main protection itself is
machine-enforced.

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
