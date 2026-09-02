# RunnerMesh Quality Gates

This file is the authoritative gate-selection policy. `FAST_LANE.md` is the compact execution reference.

## Core rule

```text
validate changed risk once
reuse unchanged-risk evidence
avoid Goal ceremony
```

## Risk vector

Every Goal classifies:

```text
CODE_CHANGED
TRAY_PRESENTATION_CHANGED
PROBE_OR_POLICY_CHANGED
RUNNER_CONTROL_CHANGED
USER_PERSISTENT_CONFIG_CHANGED
INSTALL_ACTIVATION_CHANGED
SECURITY_PRIVACY_CHANGED
RELEASE_BOUNDARY
```

A Goal may activate several dimensions. Gates are selected from active dimensions, not from milestone names.

`tools/quality/change_classifier.py` conservatively selects only
`DOCS_ONLY` versus `RUST_OR_RUNTIME_CHANGE`.  Its path hints assist the Goal's
risk-delta review; they do not infer the semantic risk vector or prove evidence
reuse.

## D — docs/planning/governance

Required:

- bounded diff/cross-document consistency;
- Markdown/link sanity where applicable;
- public privacy audit.

Product runtime gates are `N/A` unless the change itself alters a product/security claim requiring independent review.

## C — ordinary Rust code

During iteration:

- `cargo fmt`;
- focused affected tests;
- `cargo check`/Clippy as useful.

At settled candidate:

- one normal hosted code CI on the candidate head.

Use hosted CI as settled-candidate evidence.  Normally push one reasonably
settled candidate after local iteration and self-review; bounded mechanical
repair pushes remain allowed.

Do not add tray, real-runner, or production gates unless another risk dimension is active.

## V — tray/presentation

Add one representative final Windows tray/presentation proof after the candidate settles. Prove changed menu IDs, localization/theme behavior, icon/tooltip semantics, and UI-thread ownership as applicable.

Do not rerun presentation proof for non-presentation changes.

## P — probe/policy

Require deterministic fixtures/table tests for normalized evidence, precedence, Unknown/Unavailable semantics, and reason codes. Add one focused live read-only proof only when a platform observation claim cannot be proven synthetically.

A probe implementation detail never becomes authoritative policy state directly.

## R — runner control

Require deterministic lifecycle/process fixtures first. A trusted real official-runner proof is required only when changed semantics involve actual start/listen/busy/drain/stop/reconnect/adoption behavior that cannot be proven synthetically.

Real-runner mutation is a Human Gate unless the Goal carries explicit Owner authorization.

## S — persistent config/autostart

Prove one ownership-safety family covering:

- exact ownership;
- minimal mutation;
- atomicity;
- idempotence;
- unrelated-content preservation;
- concurrent drift refusal;
- uninstall/restore safety.

Tests should use sandbox roots by default. Production autostart activation is a Human Gate.

## I — install/activation/update

Prove:

- immutable staging;
- artifact/checksum verification;
- explicit active-version selection;
- durable transaction intent/receipt;
- interruption reconciliation;
- failed-activation rollback;
- old-version recovery;
- source/build/release/runtime isolation;
- active-job-safe deferral model.

Use sandbox install roots until production dogfood is explicitly authorized.

## X — security/privacy

A focused independent review is required for changed trust boundary, secret handling, external destructive write authority, persistent personal-runner exposure, or public privacy boundary.

Public repository content must contain no private dogfood identifiers, credentials, private topology, or personal infrastructure evidence.

## L — release

Release closure freshly proves release-specific artifacts, checksums, provenance, package inspection, and publication. Reuse accepted unchanged-risk tray/probe/runner/config evidence. Add only a representative trusted final dogfood smoke where useful.

Public release publication is a Human Gate.

## One-final-candidate pattern

Preferred:

```text
implement
-> focused tests
-> settle candidate
-> one final hosted CI
-> one final additional gate per active risk dimension
-> merge
```

Avoid repeated full suites, repeated auditors, or repeated real-runner tests when the relevant risk did not change.

## Dedicated audit policy

A separate auditor is reserved for:

- changed destructive/persistent external writes;
- security/privacy boundary changes;
- high-risk concurrency/ownership with plausible corruption;
- ambiguous defects;
- production cutover;
- release/publication;
- explicit Implementer request.

No generic audit loop exists for routine development.

Audit the accepted prior evidence plus the current risk delta.  Do not re-audit
unchanged history.  Non-binding scope guidance is:

- ordinary delta review: aim for 2-5 minutes;
- material runner/source-risk review: aim for 10-20 minutes;
- trust/security boundary: deeper as needed;
- H1, H2, and release: no artificial shortcut.

These are not automatic PASS timeouts.  Once sufficient evidence exists, stop
expanding scope merely to consume more time or tokens.

## Blocker Policy v2

1. **Separate diagnosis from acceptance.** A normal read-only diagnostic
   process may explain a blocker with `EVIDENCE_SCOPE=DIAGNOSTIC_ONLY`; it never
   claims independent acceptance. Independent acceptance is required only when
   the active risk policy above calls for it.
2. **Gate current invariants, not obsolete intervention paths.** A satisfied
   current safety or postcondition invariant is a no-op success for that
   subgoal. Do not recreate an old failure state merely to replay its cleanup.
3. **Separate durable and volatile identity.** Runner scope/registration,
   runner home, service configuration/security, work root, execution identity,
   and ownership bindings may be frozen. PID, creation time, process handle,
   session, and a specific Listener instance are volatile and have
   `LIVE_PROCESS_EVIDENCE_TTL=SAME_OWNER_TRANSACTION_ONLY`; reacquire and
   revalidate them immediately before mutation.
4. **Owner control flow is not product failure.** `WAITING_FOR_OWNER` is
   resumable and `OWNER_CANCELED` ends only that authorization attempt. Both
   require fresh transaction evidence before a later mutation.
5. **Audit admission failure is infrastructure evidence.** Record
   `STOP_REASON=AUDIT_ADMISSION_FAILED` and, when applicable,
   `INDEPENDENT_ACCEPTANCE_PENDING=true`; it is not proof that the audited
   artifact failed and does not prevent diagnostic work.
6. **Latch one unchanged blocker.** After sufficient evidence, set
   `BLOCKER_LATCHED=true` and retry only after relevant source, evidence, trust,
   Owner action, live state, or external prerequisites change.
7. **Respect accepted Owner policy.** An unchanged generic review may record a
   preference but cannot reopen an explicitly accepted policy decision as a
   blocker without new technical evidence.
8. **Bound retries and corrections.** Use the existing three materially
   distinct deterministic repair-cycle limit and one live Owner transaction
   per authorization. Review the focused correction delta instead of
   automatically re-auditing unchanged history.

Before an expensive independent review, use the admission-only preflight:

```text
conda run -n base python tools/dev/auditor_preflight.py --profile jerry-auditor
```

The preflight starts a fresh profile, admits exactly one harmless PowerShell
child, verifies the configured read-only/never contract, and always reports
`AUDIT_ACCEPTANCE_PASS=false`. On Windows it removes Microsoft Store
`WindowsApps` executable resolution only from that child environment so the
Codex restricted-token sandbox selects the inbox `powershell.exe`; it does not
change the global profile, sandbox implementation, ACLs, or Windows policy.

## Repository automation

Run the common local gate from a settled commit with:

```text
python tools/quality/fast_gate.py --base <accepted-main>
```

On a governed Windows Conda host where the `python` app alias is disabled, use
`conda run -n base python` in place of `python`.

Add `--full` for candidate-level all-target tests and Clippy.  The entrypoint
classifies the delta, runs the deterministic public audit and tooling tests, and
prints additional Goal-declared risk-gate responsibility.

The developer-train candidate command composes that same gate without
duplicating its test families:

```text
python tools/dev/train.py candidate --base <accepted-main> --portability auto
```

Empty path hints may support the normal evidence receipt only after semantic
review confirms the relevant risk diff is empty:

```text
<GATE>=REUSED
<GATE>_REUSED_FROM=<sha>
<GATE>_RISK_DIFF=EMPTY
```

The public audit is a deterministic baseline, not proof that arbitrary content
or a changed trust boundary is safe.

## Post-merge pipeline

Retain full Windows and Ubuntu code CI for every Rust/runtime `main` push while
the required PR status policy does not enforce an up-to-date base. A docs-only
`main` push uses classification, Fast Gate, and the stable `CI Gate` while
skipping Format and Cargo. After a passing exact-head PR merges, verify remote
`main` immediately and permit work on the next safe Goal while post-merge CI
runs asynchronously. Do not merge the next PR until the prior risk-appropriate
`main` CI is healthy; latch any failure as a blocker.

```text
POST_MERGE_CI_ASYNC_PIPELINE=true
NEXT_PR_MERGE_REQUIRES_PRIOR_MAIN_HEALTH=true
POST_MERGE_MAIN_CODE_CI=FULL_WINDOWS_UBUNTU
POST_MERGE_MAIN_DOCS_CI=LIGHTWEIGHT
```

Only a future Owner-verified state with all of the following may consider a
lightweight post-merge integrity check instead of full code main-push CI:

```text
MAIN_PROTECTION=ENFORCED
REQUIRED_PR_GATE=ENFORCED
DIRECT_PUSH_BLOCKED=true
BASE_FRESHNESS_ENFORCED=true
```

## Ledger and ordinary receipt economy

Prefer one milestone implementation PR containing its own decision-relevant
ledger delta. A multi-PR train may batch one reconciliation when useful.
Separate ledger-only PRs are mainly for external/Owner state changes, material
blocker changes, or closeout that could not safely bind earlier.

Normal source receipts should contain no more than 15 decision-relevant fields:
identity and SHAs, PR, active risk vector, local/portability/CI gates, any
actually relevant security review, production mutation, Owner action, and next
Goal. H1/H2/release and changed high-risk surfaces retain the additional
evidence their risk requires.

## Blocker latch

After one sufficient observation of an unchanged external blocker, record a stable fingerprint and:

```text
BLOCKER_LATCHED=true
```

Do not repeat the same expensive/destructive lane until source, evidence, trust state, Owner action, or the external prerequisite changes.

## Production mutation default

Ordinary source-development Goals default to:

```text
PRODUCTION_MUTATION=false
```

An installed stable RunnerMesh, official runner registration, work-root ownership, Organization runner access, and production autostart are outside unattended authority unless a specific Goal/Owner gate says otherwise.
