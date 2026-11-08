# RM-LONG-TRAIN-LRM-RUNTIME-001 — Master Train checklist

Status: **COMPLETE / SOURCE_CANDIDATE_READY** · `SOURCE_AND_SANDBOX_ONLY` · `PRODUCTION_MUTATION=false`

```text
TRAIN_ID=RM-LONG-TRAIN-LRM-RUNTIME-001
START_MAIN=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
FINAL_MAIN=ca1dd96e75a606b80d1978f83e973bd2586d36e4
LRM_BASE=#59@ed68ba7
BRANCH=feat/lrm-runtime-integration
PR=61
HEAD=1b8f045e41851faf1e7103f04e5580cc5b79c641
CI_RUN=42455849879 PASS
```

## Stages

| Stage | Status | Evidence |
|---|---|---|
| T0 | PASS | ADR0004 DRAINED≠ListenerStopped; #60 merged ledger-only; independent security barrier intact |
| T1 | PASS | FileRegistryStore in AgentCore/runtime; sandbox two-runner restart |
| T2 | PASS | EnumServicesStatusExW + session homes; incomplete SCM omitted |
| T3 | PASS | Tray intent≠presentation; runtime registry+discovery |
| T4 | PASS | local lib+clippy; Windows+Ubuntu hosted CI PASS |
| Post | PASS | scoped_admission_sim; resource_policy_probe read-only |

## Hard stops honored

No real Service/label/credential/autostart/H2/v0.1.0; no AGY; Cursor self-review ≠ independent H2 acceptance; #61 remains Draft off frozen v0.1 main.

## Recovery

`goals/RM-CONTINUOUS-TRAIN-CHECKPOINT.md` + this file + PR #61.
