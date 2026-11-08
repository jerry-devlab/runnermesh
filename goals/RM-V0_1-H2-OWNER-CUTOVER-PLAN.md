# H2 Owner cutover plan (privacy-safe)

Status: **Prepared for Owner review — production mutation NOT authorized**

This plan is bound to the fresh G15R immutable RC. It does not execute H2.

## Exact RC identity

```text
RC_COMMIT=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
RC_PACKAGE=runnermesh-0.1.0-windows-x64.zip
RC_ARCHIVE_SHA256=b9e7e3ec19a999f4b62038e1c267e8c90517921c0fcb418c6c54a3c9f12bb571
RC_OPERATOR_HELPER=runnermesh-package.exe
RC_OPERATOR_HELPER_SHA256=78838d65c7432753ef63fbcda9e1e813b7fe0b739c15ad4423cf061f266958d9
RC_HOSTED_EVIDENCE=https://github.com/jerry-devlab/runnermesh/actions/runs/42452639359
CHANNEL=rc
TARGET=x86_64-pc-windows-msvc
```

Only these exact hashes may be used. Re-hash both files immediately before any mutation; mismatch refuses cutover.

## Scope of mutation (when authorized)

- Install/replace the owned RunnerMesh version slots under the Owner-designated installation root.
- Establish exactly one non-secret `config/runtime-binding.json` through the typed installation boundary.
- Enable current-user Startup to the stable installed Agent entry only.
- Launch the ledger-owned stable Agent.
- Perform bounded admission advertise/withdraw proofs using the reserved `runnermesh-admit` label on the already-authorized exact runner.

## Explicit non-scope

- No Worker kill / destructive job termination.
- No change to official runner registration identity, credentials, or groups.
- No global Windows security policy changes.
- No publication of `v0.1.0`.
- No use of worktree/`target/` binaries.

## Prestate required before Owner token

```text
G15R=PASS
H2_RC_READY=true
SOURCE!=BUILD!=RELEASE!=INSTALLED!=ACTIVE
PRIVATE_RUNTIME_BINDING_PREPARED=true
ROLLBACK_PLAN_READY=true
INDEPENDENT_SECURITY_ACCEPTANCE_FOR_H2=PASS
PRODUCTION_BASELINE_CAPTURED=true
```

`QUALITY_GATES.md` requires genuine independent acceptance for the H2 trust /
production-mutation boundary. Cursor self-review and Owner convenience notes
do **not** satisfy `INDEPENDENT_SECURITY_ACCEPTANCE_FOR_H2`. Keep
`INDEPENDENT_ACCEPTANCE_PENDING=true` until an independent result is recorded;
do not waive that gate in this plan.


## Rollback / restore actions

1. Disable owned Startup entry by exact handle.
2. Stop Agent after drain (no Worker signal).
3. Roll back active version slot to the captured prestate slot, or uninstall owned content only.
4. Verify remote/local admission absence for `runnermesh-admit` on the bound runner.
5. Confirm production baseline restoration receipt.

## Required authorization string

```text
AUTHORIZE_MASTER_H2_CUTOVER
```

Must be a **new** token. The previous H2 authorization was consumed by the FAIL_POST_MUTATION attempt that was later restored.

## Stop line

Cursor Implementer stops before any production mutation until the Owner returns the exact authorization for this RC identity.
