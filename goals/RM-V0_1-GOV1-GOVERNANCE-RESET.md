# GOV1 — Governance and Durable Execution State Reset

Type: **Autonomous docs/governance train + optional Owner repository-settings step**

## Mission

Make future 6-24 hour autonomous development rely on durable repository state instead of chat reconstruction, and align machine-enforced repository governance with `AGENTS.md` and `AUTONOMOUS_TRAINS.md`.

## Autonomous scope

- maintain `goals/RM-V0_1-EXECUTION-STATUS.md` as the first status source;
- refresh stale README/roadmap status claims;
- ensure roadmap-v2 Goal references are coherent;
- add validation that public status docs contain no private host identifiers;
- document the expected main-branch protection policy.

## Owner repository-settings recommendation

When the Owner chooses to apply it, `main` should require:

- pull-request based updates;
- the stable required hosted check named `CI Gate`;
- no force push;
- no branch deletion;
- no ordinary direct push bypass for autonomous writers.

Changing GitHub repository/Organization settings is not authorized by this Goal unless the Owner explicitly grants that separate action.

Only after the Owner verifies all three conditions below may a future Goal
consider replacing full main-push code CI with a lightweight integrity check:

```text
MAIN_PROTECTION=ENFORCED
REQUIRED_PR_GATE=ENFORCED
DIRECT_PUSH_BLOCKED=true
```

## Acceptance

```text
EXECUTION_LEDGER_PRESENT=true
ROADMAP_V2_AUTHORITATIVE=true
README_STATUS_CURRENT=true
PRIVATE_HOST_DATA_IN_PUBLIC_LEDGER=false
MAIN_PROTECTION_RECOMMENDATION_DOCUMENTED=true
```

## Exit

Ordinary source development may proceed under roadmap v2 even if the Owner has not yet applied the optional GitHub setting change, but `AUTONOMOUS_12H_READY` must remain qualified until machine-enforced main protection is verified.
