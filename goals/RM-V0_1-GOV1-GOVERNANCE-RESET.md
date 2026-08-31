# GOV1 — Governance and Durable Execution State Reset

Type: **Accepted historical docs/governance train**

## Mission

Make future 6-24 hour autonomous development rely on durable repository state instead of chat reconstruction, and align machine-enforced repository governance with `AGENTS.md` and `AUTONOMOUS_TRAINS.md`.

## Autonomous scope

- maintain `goals/RM-V0_1-EXECUTION-STATUS.md` as the first status source;
- refresh stale README/roadmap status claims;
- ensure the then-current roadmap-v2 Goal references are coherent;
- add validation that public status docs contain no private host identifiers;
- document the expected main-branch protection policy.

## Historical Owner repository-settings recommendation

When the Owner chooses to apply it, `main` should require:

- pull-request based updates;
- the stable required hosted check named `CI Gate`;
- no force push;
- no branch deletion;
- no ordinary direct push bypass for autonomous writers.

This recommendation was later satisfied by the active `protect-main` ruleset.
Current enforcement state is authoritative only in
`RM-V0_1-EXECUTION-STATUS.md`.

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

This Goal is complete historical evidence. Roadmap v3 now governs source and
Owner lanes, and the execution ledger records machine-enforced main protection.
