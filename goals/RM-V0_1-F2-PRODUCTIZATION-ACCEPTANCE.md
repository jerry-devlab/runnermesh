# RM-V0_1-F2-PRODUCTIZATION-ACCEPTANCE

Status: **Planned fast-closeout Goal**

Target active time: **1-2 hours**

Default launch:

```powershell
codex --yolo -m gpt-5.6-sol
```

## Mission

Convert the already-prepared G12-G15 productization source into one accepted protected-main milestone with the minimum necessary delta work.

Primary prepared asset at plan creation time:

```text
PR #33
PRE-H1: prepare G12-G15 sandboxed productization source
```

Do not split G12/G13/G14/G15 into four ceremonial acceptance PRs unless the source has materially diverged and cannot be reviewed safely as one coherent change.

## Entry gate

Require fresh authoritative evidence:

```text
P0=PASS
H1_QUALIFICATION=PASS
H1_RESTORE=PASS
```

If any is false/unknown, stop. Do not weaken the gate.

## Source handling

Refresh authoritative `main` and compare the prepared productization branch/PR against it.

Prefer the least disruptive path:

1. if the prepared branch is still cleanly mergeable and its risk assumptions remain valid, update only what is necessary;
2. if base drift requires reconciliation, rebase/merge main into the prepared branch using normal protected development practice;
3. do not rewrite the productization modules merely to produce new evidence.

Retain the prepared G12-G15 scope:

- G12 user-session autostart targeting the stable installed entry;
- G13 immutable explicit-root versioned installation;
- G14 staged update, active-job deferral, rollback, and reconciliation;
- G15 explicit-input package/provenance/doctor source.

## Acceptance compression

Treat the productization delta as one coherent acceptance surface when practical.

Reuse unchanged-risk evidence from the prepared source. Freshly validate only the risk introduced by rebasing/adaptation to post-H1 main.

Required final candidate evidence:

- deterministic public/privacy audit;
- settled local candidate gate;
- hosted Windows and Ubuntu CI at exact head;
- focused ownership/security review of the changed delta;
- no production install/autostart/update/release mutation.

Do not run a new comprehensive project audit.

## PR policy

Prefer the existing PR #33 or its minimum safe successor. Make it Ready for Review only after the H1 gate is open and its exact head is settled.

Use protected-main/DVP1 flow. No `--admin`, no bypass, no direct push to `main`.

The implementation PR should carry the decision-relevant ledger update so acceptance can atomically record:

```text
G12=ACCEPTED
G13=ACCEPTED
G14=ACCEPTED
G15=ACCEPTED
```

Do not create four ledger-only follow-up PRs.

## Exit

Best case:

```text
F2=PASS
G12=ACCEPTED
G13=ACCEPTED
G14=ACCEPTED
G15=ACCEPTED
PRODUCTIZATION_PR=MERGED
MAIN_CI=PASS
G15R_ELIGIBLE=true
```

No real production install or release is performed in F2.

## Final receipt

```text
DISPOSITION=
GOAL_ID=RM-V0_1-F2-PRODUCTIZATION-ACCEPTANCE
START_MAIN=
FINAL_MAIN=
PRODUCTIZATION_PR=
CANDIDATE_HEAD=
G12=
G13=
G14=
G15=
LOCAL_GATE=
CI_GATE=
FOCUSED_REVIEW=
PRODUCTION_MUTATION=false
NEXT_GOAL=RM-V0_1-F3-RC-RELEASE-HARDENING
```