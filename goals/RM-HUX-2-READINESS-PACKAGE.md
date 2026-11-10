# HUX-2 Formal Qualification Readiness Package

`PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001` · `TRAIN_ID=RM-160H-T06-UX-INSTALL-HUX2` · `PRODUCTION_MUTATION=false`

This package prepares Owner-gated live Service qualification. It does **not**
authorize live SCM mutation. Implementer self-review is not independent security
review.

```text
PACKAGE_BUILD_COMMIT=ee27461f848285f5c06cd69bd6a4f657e5e225a3
CODE_ANCHOR=ee27461f848285f5c06cd69bd6a4f657e5e225a3
PREDECESSOR_DOCS_PR_HEAD=ca8b8e96bfbc92b34a6bc6b081970fa3240e3aa0
PREDECESSOR_DOCS_PR_CI=42802351951 (PASS_ALL 6/6 jobs)
SYNTHETIC_PASS=PASS                 # Mock SCM + Fake GitHub (344/344 cargo tests)
SOURCE_READY=true                   # Win32 query fixed; write paths inert without arm
TRUSTED_AUTH_VERIFIER=VERIFIED      # TokenValidatingOwnerVerifier + refusal paths tested
INDEPENDENT_REVIEW_PENDING=true
LIVE_NOT_AUTHORIZED=true
IMPLEMENTER_SELF_REVIEW=SOURCE_LOCAL_PASS  # not independent acceptance
TRUSTED_OWNER_AUTH=UNISSUED         # UnissuedOwnerAuthorizationVerifier default
```

## 1. Exact Service / Runner identity + Owner authorization scope

Owner records privately (never in public receipts):

| Field | Public rule |
|---|---|
| SCM service name | exact name only; no wildcards |
| Image path | store only opaque `image_token` publicly |
| Execution identity / start account | store only opaque `execution_identity_token` |
| Runner home / work root | private |
| Registration scope + server runner ID | private |
| Start mode + running state | captured in `ServicePrestateCapture` |
| Admission selector ownership | managed selector for that exact runner only |

Authorization window (when granted):

- Exactly one named Service matching prestate identity tokens
- Rights: QueryStatus, QueryConfig, Start, Stop, ChangeConfig for that name
- Optional withdraw/restore of that runner’s managed admission selector
- Duration 15–120 minutes; auto restore attempt mandatory

Forbidden under any token: credential files, registration/unregister, unrelated
labels, production dogfood identity without a separate token, Worker kill,
wildcard SCM opens.

## 2. Minimum privilege + real executor security review checklist

Source surfaces requiring independent review:

| Area | Path | Review focus |
|---|---|---|
| Synthetic vs trusted auth | `src/windows_scm_executor.rs` (`SyntheticScmTestGrant`, `TrustedOwnerAuthorizationVerifier`, `UnissuedOwnerAuthorizationVerifier`) | synthetic ≠ Owner; trusted path unissued |
| Sealed gate | `src/windows_scm_executor.rs` (`LiveScmExecutionGate`) | no public bool open; grant kind tracked |
| Exact executor | `src/windows_scm_executor.rs` (`ScmControlExecutor`) | identity binding; observe does not rewrite trust flags; restore_pass independent |
| Windows adapter | `src/windows_scm_executor.rs` (`WindowsScmBackend`) | query-only API; Start/Stop/ChangeConfig inert until authorized path exists |
| Control contract | `src/service_control_contract.rs` | withdraw→idle→stop; Busy never skip; restore plan binds transaction |
| Synthetic matrix | `src/hux2_synthetic.rs` | fail-closed matrix; never promotes to LIVE PASS |
| Prepare-only backend | `src/runner_service_backend.rs` | no live SCM |

Reviewer must verify:

1. Arbitrary `LiveScmExecutionGate { allow_*: true }` construction is impossible.
2. `observe_verified` never overwrites `ownership_verified` / `image_path_matches`.
3. `expected_image_token` participates in every mutate/restore path.
4. Prestate and restore plan share service, image, identity, and `transaction_id`.
5. `restore()` checks every backend error and independent start-mode / run-state /
   admission readback; incomplete restore cannot return `restore_pass=true`.
6. `WindowsScmBackend::{start,stop,change_start_mode}` remain GateClosed in this
   Train (no UAC, no live mutation).

## 3. State machine — Admission Withdrawal / Worker Busy / Service Stop-Start

```text
RemainRunning:
  Listening|Busy -> Observe
  Stopped -> PrepareStart (gate+identity)

DrainThenStop / managed StayStopped:
  selector present -> WithdrawAdmission
  Worker busy -> WaitIdle (never Stop)
  Listening idle + selector absent -> PrepareStop
  Stopped -> Observe

Restore (Owner-gated):
  ChangeConfig(start_mode) -> readback mode
  Start|Stop per plan.restore_running -> readback state
  Independent admission_selector_absent readback must match plan
  Any miss -> RestoreFailed, restore_pass=false
```

Achieved signals come from observation, never from a single API acknowledgement.
`DRAINED` ≠ ListenerStopped (ADR0004).

## 4. Interrupt / concurrency / identity-drift / failure-recovery matrix

| Case | Expected | Covered by |
|---|---|---|
| Closed gate + PrepareStop | refuse; stop_calls=0; restore_pass=false | unit + synthetic |
| Tampered Owner seal | GateClosed | unit |
| False ownership/image flags | IdentityMismatch; flags not overwritten | unit + synthetic |
| Image token mismatch | IdentityMismatch | unit |
| Service name drift | NotFound / NameDrift | synthetic |
| Busy Worker | WaitIdle; no Stop | unit + synthetic |
| AccessDenied on query | refuse closed | synthetic |
| ChangeConfig fails mid-restore | RestoreFailed; restore_pass=false | unit + synthetic |
| Admission readback mismatch | RestoreFailed | unit |
| Plan bound to other service | RestoreFailed | unit |
| Windows mutation path | always GateClosed this Train | unit |

Live interrupt (power loss mid-stop) remains Owner-transaction only and is not
claimed PASS by synthetic evidence.

## 5. Prestate capture + independent restore verification

1. `capture_prestate(worker_busy, admission_selector_absent)` binds tokens +
   `transaction_id`.
2. `capture_restore_plan` copies identity + admission expectation; always
   `requires_owner_gate=true`.
3. After every mutate/restore step, re-query via `observe_verified`.
4. `restore_pass=true` only when mode, running state, and admission readback all
   match the bound plan.
5. Public receipts: hashed/privacy-safe ids only; emit QUALIFICATION and RESTORE
   separately.

## 6. Independent security review — concrete evidence pointers

```text
SOURCE:
  src/windows_scm_executor.rs
  src/service_control_contract.rs
  src/hux2_synthetic.rs
  src/runner_service_backend.rs
  src/installation.rs
  src/package.rs

CANDIDATE_FREEZE:
  PACKAGE_BUILD_COMMIT=ee27461f848285f5c06cd69bd6a4f657e5e225a3
  CODE_ANCHOR=ee27461f848285f5c06cd69bd6a4f657e5e225a3
  PREDECESSOR_DOCS_PR_HEAD=ca8b8e96bfbc92b34a6bc6b081970fa3240e3aa0
  PREDECESSOR_DOCS_PR_CI=42802351951 (PASS_ALL 6/6 jobs)

TESTS (all passing locally and on hosted CI):
  cargo test --lib scm_
  cargo test --lib hux2_
  cargo test --lib service_control
  cargo test --lib installation
  cargo test --lib package

DO NOT MARK:
  INDEPENDENT_SECURITY_REVIEW=PASS   # Implementer self-review only; independent review required
  LIVE_QUALIFICATION=PASS            # gate sealed; no live SCM
```

## 7. Human execution plan + stop conditions

Prep (Owner):

1. Independent security review PASS on the exact candidate HEAD.
2. Private identity sheet for one disposable test Runner (not production dogfood).
3. Confirm Worker idle; no active job.
4. Issue sealed Owner authorization for the exact name+image+rights window.
5. Follow `goals/RM-HUX-2-OWNER-SERVICE-GATE-PLAN.md` operator sequence.

Stop immediately when:

- Identity drift / incomplete trust flags
- Busy Worker appears
- Any restore step fails
- Unexpected second Service match
- Owner aborts or window expires
- Any UAC prompt outside the prepared helper path

Exit dispositions:

```text
PASS     stop/start with RESTORE=PASS on exact test identity
BLOCKED  missing review, busy, drift, Owner abort
FAIL     mutation attempted but contract broken
```

Never convert SYNTHETIC or UNPROVEN into LIVE PASS.
