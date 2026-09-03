# RM-V0_1-F4-H2-CUTOVER-DOGFOOD

Status: **Planned fast-closeout Goal**

Target active time: **1-2 hours**, followed by **at least 24 hours wall-clock dogfood**

Default launch:

```powershell
codex --yolo -m gpt-5.6-sol
```

## Mission

Cut over the exact immutable G15R RC to the real single-workstation RunnerMesh environment under explicit Owner authorization, then complete the minimum sustained ordinary-use dogfood required for v0.1 release eligibility.

## Entry gate

Require:

```text
H2_RC_READY=true
G15R=PASS
NAMED_PIPE_EXPLICIT_DACL=PASS
RC_IDENTITY=IMMUTABLE
ROLLBACK_READY=true
OWNER_PRESENT=true
```

Do not rebuild or silently substitute the RC after entry.

## H2-A Owner cutover

Before production mutation, show the exact cutover/rollback plan and wait for explicit Owner authorization.

Suggested authorization token:

```text
AUTHORIZE_F4_H2_CUTOVER
```

Perform only the accepted production install/activation path. Preserve the prior known-good rollback state.

Representative live acceptance must cover the product behaviors that cannot be replaced by synthetic evidence:

- installed-runtime startup and user-session autostart;
- Agent + Tray + CLI + Named Pipe IPC;
- manual mode / Zen / Auto Lite representative behavior;
- human-first activity handling;
- exact official runner observation and admission;
- active-job preservation;
- withdrawal / drain / re-advertisement / reconnect;
- workstation restart and/or suspend-resume behavior where required by existing H2 policy;
- doctor/runtime isolation checks;
- rollback remains available.

Do not expand H2 into a new comprehensive architecture audit.

If cutover fails after mutation, restoration is mandatory before any further development.

## H2-B sustained dogfood

After successful cutover, start the minimum ordinary-use window:

```text
DOGFOOD_MINIMUM=24h
```

48-72h is preferred only when convenient; it is not required by this fast-closeout plan.

Dogfood must represent normal workstation use, not a synthetic stress loop. Record only decision-relevant observations:

- unexpected Agent/Tray/CLI failure;
- incorrect human-first admission;
- destructive active-job behavior;
- runner disconnect/reconnect failure;
- sleep/resume/reboot regression;
- persistent config/autostart drift;
- update/rollback failure;
- security/privacy regression.

A release-blocking lifecycle fault resets eligibility until corrected and freshly qualified as appropriate.

## Parallel release preparation

The 24h window is not idle time. Non-mutating source/document work may prepare F5 in parallel:

- release notes/changelog;
- README/operator docs;
- artifact names and public checksums layout;
- provenance verification commands;
- G17 closeout checklist;
- H3 tag/publication command plan.

Do not publish stable artifacts or tag `v0.1.0` before dogfood passes.

## Exit

Best case:

```text
F4=PASS
H2_CUTOVER=PASS
DOGFOOD_HOURS=>=24
DOGFOOD=PASS
RELEASE_BLOCKING_LIFECYCLE_FAULT=false
G17_ELIGIBLE=true
```

## Final receipt

```text
DISPOSITION=
GOAL_ID=RM-V0_1-F4-H2-CUTOVER-DOGFOOD
RC_COMMIT=
H2_CUTOVER=
ROLLBACK_READY=
DOGFOOD_START=
DOGFOOD_END=
DOGFOOD_HOURS=
DOGFOOD=
RELEASE_BLOCKING_FAULT=
G17_ELIGIBLE=
NEXT_GOAL=RM-V0_1-F5-RELEASE-CLOSEOUT
```