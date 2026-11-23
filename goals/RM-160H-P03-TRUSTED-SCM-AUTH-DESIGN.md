# P03 — Trusted SCM authorization design (SOURCE_READY plan)

`PRODUCTION_MUTATION=false` · design only · **no** live writes · **no** token issuance.

## Current code reality (PR #63 tip + GT-01)

```text
SyntheticScmTestGrant = forgeable, test/mock only
TrustedOwnerAuthorizationVerifier = live entry trait
UnissuedOwnerAuthorizationVerifier = always AuthorizationNotIssued
WindowsScmBackend mutation_arm = unconstructable in this Train
INDEPENDENT_SECURITY_REVIEW=PENDING
SCM_LIVE=NOT_AUTHORIZED
```

## Required trusted grant material (future Owner issuance)

Exact fields a verifier must bind before opening `LiveScmExecutionGate` with
`TrustedLive` kind:

1. Exact SCM service name (no wildcards)
2. Opaque `image_token` and `execution_identity_token`
3. Rights bitset: Start / Stop / ChangeConfig / Query*
4. Transaction id / expiry / single-use nonce
5. Target host attestation (private offline; public receipts use opaque hashes)
6. Independent security review attestation id for the exact candidate HEAD

## Issuance rules

- Synthetic grants must never satisfy trusted verification.
- Cursor self-review must never mint trusted material.
- Helper elevation (if any) stays narrow; no blanket SCManager rights.
- Expiry and single-use: stale or replayed material → GateClosed.
- Restore path requires the same bound identity + independent admission observer.

## Exit this leaf

```text
P03_DESIGN=READY_FOR_INDEPENDENT_REVIEW
P03_LIVE=NOT_AUTHORIZED
TRUSTED_OWNER_AUTH=UNISSUED
```
