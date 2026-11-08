# RM-LONG-TRAIN-LRM-RUNTIME-001 — Master Train checklist

Status: **ACTIVE** · `SOURCE_AND_SANDBOX_ONLY` · `PRODUCTION_MUTATION=false`

```text
TRAIN_ID=RM-LONG-TRAIN-LRM-RUNTIME-001
START_MAIN=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
LRM_BASE=#59@8fe1109
BUDGET_HOURS=8-12
```

## Stages

| Stage | Goal | Acceptance | Stop if |
|---|---|---|---|
| T0 | DRAINED/OFFLINE matrix + #60 gate hygiene | ADR0004: DRAINED≠ListenerStopped; Offline still needs stop evidence; #60 no waiver-as-independent | unblockable ADR conflict |
| T1 | Registry ↔ Agent persistence | sandbox two-runner intent survives restart | FileRegistry ownership fail-closed |
| T2 | Windows read-only discovery | real SCM/session observation injectible + Linux build OK | needs privilege write |
| T3 | Agent/CLI/Tray E2E | same snapshot; intent≠achieved; CN/EN | SCM/label write temptation |
| T4 | Bounded integration + hosted CI | focused tests + one settled CI | 3 failed repair cycles |

## Hard stops

No real Service/label/credential/autostart/H2/v0.1.0; no AGY; Cursor self-review ≠ independent H2 acceptance.

## Recovery

`goals/RM-CONTINUOUS-TRAIN-CHECKPOINT.md` + this file.
