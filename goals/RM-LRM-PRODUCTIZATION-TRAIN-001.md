# RM-LRM-PRODUCTIZATION-TRAIN-001

Status: **ACTIVE** · `SOURCE_AND_SANDBOX_ONLY` · `PRODUCTION_MUTATION=false`

```text
TRAIN_ID=RM-LRM-PRODUCTIZATION-TRAIN-001
START_MAIN=ca1dd96e75a606b80d1978f83e973bd2586d36e4
BASE=#62@729359f8ae44b2b56da0405da05ff4af3484249b
BRANCH=feat/lrm-productization
BUDGET_HOURS=16-24 CAP=24
```

## Stage board

| Stage | Focus | Evidence class target |
|---|---|---|
| T0 | Baseline + evidence taxonomy | docs |
| T1 | Discovery identity/evidence | SOURCE + SYNTHETIC (+ host RO) |
| T2 | Tray/CLI UX checklist | HUMAN_UX checklist + auto contracts |
| T3 | SCM executor (gate closed) | SOURCE + SYNTHETIC |
| T4 | Admission↔Service loop | SOURCE + SYNTHETIC |
| T5 | Registry/startup recovery | SOURCE + SYNTHETIC |
| T6 | CLI/Tray management | SOURCE + SYNTHETIC |
| T7 | HUX-2 synthetic qualification | SYNTHETIC only |
| T8 | Owner gate + security delta | docs + PENDING independent review |
| T9 | Draft PR + CI | hosted CI |

## Hard stops

No live SCM mutation; no H2; no merge onto frozen v0.1 RC; Cursor self-review ≠ independent security acceptance.
