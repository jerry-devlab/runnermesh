# RM-160H-DELIVERY-PROGRAM-001 — Cursor execution contract

This file defines the workflow for the **sole Cursor Desktop Implementer**. It grants only ordinary source/sandbox development authority; it does **not** grant Owner transaction or production permission. Refer to [ROADMAP](ROADMAP.md) and [CHECKLIST](CHECKLIST.md).

## Program / train bootstrap

- `PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001`
- `TOTAL_ACTIVE_ENGINEERING_BUDGET=160h`
- `TRAIN_BUDGETS=16,24,24,24,24,24,24h`; recover at each stop instead of claiming one uninterrupted session.
- `SOURCE_AND_SANDBOX_ONLY`; `PRODUCTION_MUTATION=false`.
- GitHub project: `jerry-devlab/runnermesh`. Planning main `ca1dd96e75a606b80d1978f83e973bd2586d36e4`; planning LRM Draft tip PR #63 `8cd996acfee23e5f2006e7801139c70d0d67551d`. **Always re-check exact refs before using.**
- Existing HUX-0 human Tray verdict is Owner PASS. **Current recorded zero-runner HUX-1 is from laptop-zenbook14, not ZenBookDuo.**
- v0.1 H2 remains Owner-gated; previous H2 token is consumed; HUX-2 independent SCM security review and trusted auth unissued.
- **Cursor is sole Implementer. Do not call AGY, Codex CLI, or a second coding Agent.**

## Recovery order

1. Read `AGENTS.md`, `dev_governance_files/QUALITY_GATES.md`, `dev_governance_files/AUTONOMOUS_TRAINS.md`, v0.1 execution ledger and this folder.
2. Fetch remote main/PR #63, inspect local worktrees; do not overwrite foreign local edits, do not reset main/old PRs, and do not force-rebase.
3. Compare last program receipt/checkpoint with GitHub actual head and CI. Reuse proven HUX-0 and prior risk-empty tests; verify newer HEADs before claiming success.
4. Choose next eligible **Goal DAG leaf**, prioritized by earliest human value, dependencies and 160h remaining budget. If its Owner gate is unsatisfied, mark `WAITING_FOR_OWNER` and choose another safe leaf; do not pester Owner at ordinary compile/test/PR milestones.
5. Work within a single Writer-owned worktree and cohesive change set. Do focused local tests, settle candidate, one selected hosted CI, open/update focused Draft PR, then advance.
6. Merge only when normal repository protections, actual independent security requirements, v0.1 frozen RC compatibility, and accepted release lane allow it; not by blanket permission from this document. Never blindly merge #51–#63 stack.
7. Write decision-relevant program checkpoint/receipt at every 60–90 minutes of active engineering and at each train completion. No self-referential HEAD update loop. Include remaining budget, exact remote evidence, privacy-safe redactions and next runnable Goal.

## Train scheduler

Begin GT-01 (16h active cap), then automatically continue GT-02…GT-07 within the legal scope if the environment/session remains available. A lost session is **not** background execution; next Cursor chat reads the checkpoint and resumes remaining leaves. Do not deliberately sleep to reach budget. Stop at 160h used, all eligible tasks completed, or actual hard blockers. All Owner-controlled mutations must be suspended until separate explicit authorization.

If ZenBookDuo cannot be accessed, do not substitute ZenBook14 result; prepare a one-command read-only operator card and continue P02–P06 source. If independent review/Owner token is unavailable, finish remaining safe source and return gate-ready package.

## Explicit hard stops

No real SCM Start/Stop/ChangeConfig, unmanaged User Session process shutdown, GitHub label/registration/group/workflow changes, elevation, credential handling, production install/autostart, H2 cutover, HUX-2/HUX-3 live execution, release publication, or Worker termination without **separate exact** Owner gate and approved rollback. No real new runner installation to manufacture HUX-1 data.

Synthetic grants, `SOURCE_READY`, green CI, a README/ADR or Owner acceptance of HUX-0 are **not** trusted SCM authorization. An independent privilege review cannot be satisfied by Cursor self-review.

## Completion

Output the receipt fields defined in CHECKLIST.md, list every accepted vs pending item, show `H2`, `HUX2_LIVE`, `HUX3_LIVE` and `RESTORE` individually, and identify the next concrete Owner transaction if available. Distinguish an **unpublished RC** from a released version. The 160h program is a planning cap, not an entitlement to consume the entire window regardless of blockers.
