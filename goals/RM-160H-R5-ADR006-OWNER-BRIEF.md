# RM-160H R5 — G-ADR006 Owner Architecture Brief

Status: **BRIEF_READY** · `G-ADR006=DRAFT_NOT_ACCEPTED`  
Authority: Distilled from `docs/adr/0006-scoped-admission-selector-draft.md` and `goals/RM-160H-P04-SELECTOR-DECISION-PREP.md`.

## Context

Under accepted ADR 0004 (v0.1 frozen product contract), RunnerMesh uses a single reserved admission label:
```text
RESERVED_ADMISSION_LABEL = runnermesh-admit
```
ADR 0005 introduces Local Multi-Runner Management, allowing multiple local runner instances on a single Windows workstation. When two or more runners share the same GitHub repository/organization registration scope, assigning `runnermesh-admit` to both causes admission collisions.

Draft ADR 0006 proposes namespaced selectors of the form:
```text
runnermesh-admit-<stable-local-key>
```

## Three Explicit Choices for the Owner

| Choice | Architectural Meaning | Immediate Consequence |
|---|---|---|
| **1. ACCEPTED** | Formally accept ADR 0006 to supersede ADR 0004 for multi-runner workflows. | Authorizes migration work: defining `<stable-local-key>` grammar, updating workflow `runs-on` templates, and testing live label mutations once R2 passes. |
| **2. REJECTED** | Permanently reject per-runner admission labels in favor of single-runner or group-based admission. | Multi-runner remains limited to: exactly one managed runner per GitHub scope, plus unmanaged auxiliary runners or runners targeting distinct GitHub scopes. |
| **3. DEFER (Current)** | Keep ADR 0006 in Draft status for v0.2+; preserve frozen v0.1 admission contract as authoritative. | **Zero change to v0.1**. Same-scope dual managed enrollment remains `REFUSED`. v0.1 H2 cutover and single-runner HUX-2 proceed unaffected. |

## Implementation State in Source (Safe Under All Choices)

- In `src/admission.rs`: Same-scope managed admission collisions are detected and rejected (`same_scope_collision`).
- In `tests/fake_github_admission.rs`: Scoped admission simulations (`scoped_admission_sim`) remain marked unapproved.
- Zero live label mutation occurs without explicit Owner transaction authorization.

## Recommended Owner Action

Unless multi-runner same-scope GitHub admission is immediately required for live dogfooding, select **`DEFER`** to protect the v0.1 single-workstation release boundary.
