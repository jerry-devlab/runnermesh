# RM-160H P03 Trusted SCM Authorization & Admission Proposal

**Author**: AGY (Implementer)\
**Date**: 2026-11-11\
**Status**: SUBMITTED_FOR_OWNER_AND_INDEPENDENT_AUDITOR_REVIEW\
**Target Track**: Track 2 (v0.2-A LRM)\
**Production Mutation**: `PRODUCTION_MUTATION=false` (All live writes remain inert)

---

## 1. Authority Architecture & Roles

```text
[Owner Offline Authority]
       │
       ▼ (Offline secret / token provisioning)
[OwnerAuthorizationIssuer] (Seals all 9 P03 dimensions into HMAC-SHA256 voucher)
       │
       ▼ (Signed TrustedOwnerAuthorizationRequest with single-use nonce & validity window)
[Controlled Assembly / Agent Core]
       │
       ▼
[TokenValidatingOwnerVerifier] (Internal/Sandbox only; verifies scope, clock, and atomic ledger)
       │
       ▼ (Returns Ok only if voucher valid AND atomic try_consume succeeds)
[LiveScmExecutionGate::mint_trusted_live] (pub(crate) only; mints TrustedLive gate)
       │
       ▼
[PrivilegedScmHelper] (Requires TrustedLive gate + exact service matching)
       │
       ▼
[WindowsScmBackend] (SEALED in repository code; mutation_arm remains None -> GateClosed)
```

1. **Trusted Issuance Authority**:
   - The root authority secret is held strictly offline by the workstation Owner.
   - Public code and repository builds do NOT embed or self-generate root authority keys.
   - Default product runtime defaults to `UnissuedOwnerAuthorizationVerifier`, which unconditionally fails closed with `Err(ScmApiError::AuthorizationNotIssued)`.

2. **Verification Authority**:
   - `TokenValidatingOwnerVerifier` is internal to the controlled assembly (`pub(crate)` / sandbox cfg).
   - Public constructors (`new`, `from_authority`) are removed from the crate's public interface, preventing external callers from manufacturing arbitrary verifiers or self-signing tokens.

3. **P03 9-Dimension Cryptographic Binding**:
   Every authorization request and verification cryptographically binds:
   - `service_name`: exact string (no wildcards `*`, `?`, `/`, `\`).
   - `image_token`: opaque SHA-256 digest of normalized runner service executable path.
   - `execution_identity_token`: opaque identity provider key (e.g. current user SID digest).
   - `target_host_token`: opaque host identity token.
   - `candidate_commit`: exact git commit SHA under test.
   - `transaction_id`: bounded transaction UUID.
   - `rights`: explicit bitset (`allow_start`, `allow_stop`, `allow_change_config`).
   - `validity_window`: `not_before_epoch_seconds` .. `expires_at_epoch_seconds` checked against `TrustedClock`.
   - `nonce`: single-use cryptographically unique transaction nonce.

4. **Atomic Single-Use Consumption & Replay Prevention**:
   - Consumption check and recording are unified into an atomic `SharedAuthorizationLedger::try_consume(&token_hash, &nonce) -> bool` under a single mutex lock.
   - If a voucher or nonce has already been consumed, `try_consume` returns `false` immediately.
   - Verification fails closed (`Err(ScmApiError::AccessDenied)`) before any gate can be minted.
   - In concurrent race conditions across $N$ threads presenting the same voucher, exactly 1 thread succeeds; the remaining $N-1$ threads are rejected.

5. **Entrypoint Closure on Authorization Failure**:
   - Any mismatch in the 9 dimensions, clock expiry, premature window, missing authority secret, or ledger replay unconditionally fails closed without mutating state.
   - Even when presented with a valid `TrustedLive` gate, the live `WindowsScmBackend` write paths remain sealed fail-closed (`Err(ScmApiError::GateClosed)`), as the write-enabling `ScmMutationArm` is intentionally unconstructable in public repository code.

---

## 2. Standing Owner Determinations Required for Future Live Gate

Before live Win32 SCM mutation can ever be authorized in a future Owner transaction (HUX-2):
1. **Root Authority Secret Provisioning**: Owner determination of offline key generation and storage boundary.
2. **Transaction Window & Expiry**: Owner determination of transaction duration (15–120 minutes) and automated rollback window.
3. **In-Memory Injection Channel**: Mechanism for Owner token presentation to the controlling Agent without persistence to disk.
