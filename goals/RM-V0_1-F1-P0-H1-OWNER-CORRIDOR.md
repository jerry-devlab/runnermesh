# RM-V0_1-F1-P0-H1-OWNER-CORRIDOR

Status: **Planned fast-closeout Goal**

Target active time: **2-4 hours**

Default launch:

```powershell
codex --yolo -m gpt-5.6-sol
```

`--yolo` does not authorize live mutations outside the explicit Owner gates below.

## Mission

Close the historical P0 baseline prerequisite, then—only after P0 independently passes—continue in the same operator/Codex session into the prepared H1 Owner corridor and complete one H1 qualification plus mandatory restore.

This Goal compresses context handoff; it does **not** merge P0 and H1 authority. They remain two independently gated phases.

## Admission

Start from fresh authoritative protected `main`. Read:

- `goals/RM-V0_1-FAST-CLOSEOUT-PLAN.md`;
- `goals/RM-V0_1-P0-SUPERVISED-BASELINE-RESTORE.md`;
- `goals/RM-V0_1-EXECUTION-STATUS.md`;
- `dev_governance_files/QUALITY_GATES.md`;
- the private H1 Owner bundle prepared under the evidence root.

Require a clean governed repository and verify PR #33 remains unmerged/source-prepared.

## Phase A — P0 closeout

Use the accepted current-invariant P0 model. Historical intervention state is not required.

Freshly determine exactly one branch:

```text
START_SERVICE_ONLY
TERMINATE_EXACT_ORPHAN_THEN_START_SERVICE
NO_MUTATION_REQUIRED
BLOCKED_PRECONDITION
```

Durable target identity may reuse accepted evidence when unchanged. Volatile process evidence has:

```text
LIVE_PROCESS_EVIDENCE_TTL=SAME_OWNER_TRANSACTION_ONLY
```

Reacquire volatile evidence immediately before any process mutation.

### Independent evidence

Use a fresh independent read-only Auditor where the active risk policy requires it. Reuse the current WindowsApps-filtered/inbox-PowerShell admission route. Do not restart a broad sandbox forensic investigation merely because an admission attempt is noisy.

The P0 retry budget is **one fresh bounded transaction** after the material evidence change established by the 2026-09-03 sandbox forensic result.

If the historical target is again blocked only by non-product audit/recovery infrastructure, STOP the P0 branch and surface:

```text
P0_HISTORICAL_TARGET_RETRY_EXHAUSTED=true
FRESH_QUALIFICATION_TARGET_RECOMMENDED=true
```

Do not automatically register/replace a runner. A fresh official H1 qualification target requires an explicit Owner decision.

### P0 Owner gate

Before mutation, emit a concise exact action summary and wait for explicit authorization:

```text
AUTHORIZE_F1_P0_CURRENT_INVARIANT_ACTION
```

Then perform only the selected minimum action. Never terminate an active Worker or touch unrelated runners.

### P0 acceptance

Require fresh independent postverification of the existing P0 acceptance fields. If P0 is not `PASS`, stop the whole Goal. Do not enter H1.

## Phase boundary

After P0 `PASS`, record a durable checkpoint:

```text
F1_P0=PASS
F1_H1_ELIGIBLE=true
```

Do not create a separate ceremony-only Goal. The same Codex process may continue to Phase B, but all H1 live evidence and authorization must be fresh.

## Phase B — H1 Owner corridor

Use the already-prepared non-repository Owner bundle. Do not redesign G11R/H1.

Complete only the remaining Owner prerequisites:

1. harden the exact private evidence scope required for H1;
2. configure the approved opaque credential reference without printing secret bytes;
3. verify exact local/remote runner binding;
4. establish/verify the reserved `runnermesh-admit` selector with exact ownership/uniqueness;
5. deploy/verify the trusted bounded H1 workflow;
6. prove routing and restore readiness;
7. collect all eleven H1 live readiness fields.

Unknown remains fail-closed.

### H1 readiness gate

Do not mutate admission state until:

```text
H1_LIVE_READINESS=PASS_11_OF_11
```

Then emit the exact planned H1 transaction and wait for a **new** explicit Owner authorization:

```text
AUTHORIZE_F1_H1_QUALIFICATION
```

P0 authorization does not carry into H1.

## H1 qualification

Run exactly one bounded qualification transaction using accepted G11R semantics:

- advertise exact capacity;
- prove trusted workflow routing to the exact runner;
- prove active-job preservation;
- request selector withdrawal and positive readback;
- conservatively permit any in-flight assignment around the control point to finish;
- prove achieved drain without destructive Worker signaling;
- re-advertise/reconnect;
- independently restore the original accepted baseline.

Report qualification and restoration separately:

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
```

## Exit

Best case:

```text
F1=PASS
P0=PASS
H1_LIVE_READINESS=PASS_11_OF_11
QUALIFICATION=PASS
RESTORE=PASS
PR33_ACCEPTANCE_GATE=OPEN
```

Do not merge PR #33 in F1. Stop after H1 restore and privacy-safe state recording.

## Final receipt

```text
DISPOSITION=
GOAL_ID=RM-V0_1-F1-P0-H1-OWNER-CORRIDOR
START_MAIN=
FINAL_MAIN=
P0=
P0_ACTION=
H1_LIVE_READINESS=
H1_QUALIFICATION=
H1_RESTORE=
PR33_ACCEPTANCE_GATE=
PRODUCTION_MUTATION=
NEXT_GOAL=RM-V0_1-F2-PRODUCTIZATION-ACCEPTANCE
```