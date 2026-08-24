# Evidence Contract

RunnerMesh uses explicit, decision-relevant evidence.

## Dispositions

- `PASS`: requirement proved.
- `FAIL`: requirement disproved.
- `BLOCKED`: an external or trust boundary prevented required execution.
- `UNPROVEN`: evidence is insufficient for pass or fail.
- `REUSED`: accepted prior evidence remains valid because the relevant risk diff is empty.
- `N/A`: the gate is outside the changed-risk vector.

Never silently convert `UNPROVEN` to success.

## Exact-head evidence

Fresh evidence for a settled candidate binds to:

```text
EXPECTED_HEAD == checked_out_head == evidence_head
```

For hosted CI, the PR/check head must match the candidate being admitted.

## Evidence reuse

Reuse is first-class when the relevant risk surface did not change. Record:

```text
<GATE>=REUSED
<GATE>_REUSED_FROM=<sha>
<GATE>_RISK_DIFF=EMPTY
```

A documentation-only closeout does not invalidate previously accepted runner-control, tray, configuration, or dogfood evidence.

## Evidence authority

- unit/table tests prove deterministic model/policy contracts;
- hosted CI proves build/test/lint on the candidate;
- synthetic process/IPC/install fixtures prove deterministic lifecycle and ownership behavior;
- read-only host probes prove local observation claims;
- trusted real-runner qualification proves changed official-runner control semantics;
- production-style dogfood proves install/cutover/recovery claims;
- release verification proves published artifact/provenance claims.

Do not use a weaker evidence authority to prove a stronger claim.

## Receipts

Receipts should contain only active gates, candidate identity, disposition, relevant proof references, production-mutation status, blocker fingerprint if any, and the next safe Goal.
