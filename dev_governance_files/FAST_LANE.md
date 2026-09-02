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

For the same full gate plus optional existing-WSL portability proof, use:

```text
python tools/dev/train.py candidate --base <accepted-main> --portability auto
```

Use `health`, `wait-pr`, `merge`, and `wait-main` subcommands to replace manual
GitHub polling while retaining exact-head, protected-main, and prior-main-health
checks. An unavailable `gh`, changed SHA, merge queue, or protection without
atomic base-freshness enforcement fails closed for GitHub mutations. The helper
does not change repository protection policy.

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

Before an expensive independent run, prove the fresh profile can start one
harmless child with `tools/dev/auditor_preflight.py`. This is admission evidence
only: it always reports `AUDIT_ACCEPTANCE_PASS=false`. Diagnosis may continue
with `EVIDENCE_SCOPE=DIAGNOSTIC_ONLY` if admission is externally blocked; never
convert that infrastructure failure into a product failure or acceptance.

## Post-merge overlap

After merge, verify remote `main` and start the next safe Goal while main CI runs
asynchronously. Do not merge the next PR until that prior main run passes; latch
a failure immediately. Full Windows and Ubuntu CI remains required for code
main pushes while the required PR gate does not enforce an up-to-date base.
Docs-only main pushes retain Fast Gate and stable `CI Gate` but skip Cargo.

Ordinary milestones should include their ledger delta in the implementation PR
or a bounded train reconciliation, avoiding an automatic second ledger-only PR.
Normal source receipts stay at or below 15 decision-relevant fields unless an
active high-risk surface requires more evidence.

## Blockers

Follow the eight-rule Blocker Policy v2 in `QUALITY_GATES.md`: current
postconditions outrank obsolete intervention paths, durable identity is distinct
from same-transaction process evidence, Owner wait/cancellation is control flow,
and accepted policy is reopened only by new technical evidence.

Latch one sufficient unchanged blocker:

```text
BLOCKER_LATCHED=true
```

Do not spin on it.

## Automatic repair budget

Ordinary compile/test/fmt/clippy/deterministic-fixture/hosted-CI defects may receive at most three materially distinct repair cycles per failure family. After that, stop with a blocker fingerprint.

## Human gates

Stop unattended work before UAC, real Windows Service mutation, real runner registration/work-root destructive mutation, Organization runner-access changes, production autostart activation, installed stable-runtime mutation, production cutover, public release publication, destructive active-job termination, secrets/trust changes, or ambiguous ownership/security state.
