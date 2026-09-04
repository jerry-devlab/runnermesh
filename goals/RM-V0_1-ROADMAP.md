# RunnerMesh v0.1 Implementation Roadmap v3

Status: **Accepted implementation sequence**

Roadmap v3 adopts the accepted 2026-08-31 comprehensive health-audit
conclusions as the current planning truth. It supersedes roadmap v2 for
remaining execution without reopening the accepted G11R architecture or the
frozen product contract in `docs/v0.1-design-freeze.md`.

The project is not globally blocked. Safe source preparation and Owner-gated
live work are separate lanes:

```text
EXECUTION_MODEL=PARALLEL_SOURCE_PREP_WITH_H1_MERGE_GATE
P0_PRODUCT_BLOCKER=false
H1_TARGET_AUTHORITY_BLOCKER=false
H1_TARGET_AUTHORITY_WAITING_FOR_OWNER=false
P0_SOURCE_DEVELOPMENT_BLOCKER=false
H1_SHOULD_BLOCK_SOURCE_PREPARATION=false
```

The Owner-accepted fresh-target replacement now gates live qualification and
product acceptance. Its completed target decision is not a project blocker;
the exact qualification transaction remains separately Owner-gated.
G12-G15 may be extracted, refactored, tested, and prepared before H1, but they
may not be accepted or merged as product milestones until H1 qualification,
restoration, and target authority pass.

## Product invariant carried forward

RunnerMesh v0.1 remains a Windows, single-workstation, human-first GitHub
Actions admission/lifecycle controller for an already-configured official
self-hosted runner.

- human activity has priority over CI;
- uncertainty fails closed for new CI admission;
- active normal jobs are not destructively terminated for ordinary mode or
  drain transitions;
- GitHub Actions owns workflow scheduling and the job protocol;
- RunnerMesh manages contributed capacity, admission, and lifecycle;
- one execution identity owns one active work root;
- `SOURCE != BUILD != RELEASE != INSTALLED RUNTIME != ACTIVE VERSION`;
- production, privilege, and trust mutations occur only behind explicit Owner
  gates.

## Accepted foundation

G01-G10, G06R, G10R, the native process-observation hotfix, G11R-A, G11R-B,
G11R-C, and Fast Lane v2 are accepted on authoritative `main`.

The accepted G11R mechanism is the exact-runner GitHub-native custom label
`runnermesh-admit` with two-phase withdrawal, positive readback, drift refusal,
and natural completion of active work. ADR 0004 remains authoritative. The
historical `run.cmd --once` executor in PR #17 is superseded; its useful
exact-scope and no-signal evidence is already preserved in the ADR and current
tests.

## 1. Governance truth and parallel preparation

Roadmap v3 and `RM-V0_1-EXECUTION-STATUS.md` are the durable execution truth.
Repository governance is machine enforced by the active `protect-main`
ruleset, with no bypass actors and required check `CI Gate`. Full post-merge
Windows and Ubuntu code CI remains enabled because the required status policy
is intentionally not strict/up-to-date. Docs-only main pushes retain the
classification/Fast Gate/`CI Gate` path without Cargo.

Source work uses focused branches, exact-head CI, selected risk gates, and
protected-main merges. PR #17 closes as superseded; PRs #14-#16 remain intact as
selective-extraction assets for the productization preparation Goal.

## 2. P0 supervised baseline restore

Goal: `RM-V0_1-P0-SUPERVISED-BASELINE-RESTORE-001`.

This is a current-state reconciliation Owner transaction for one historical
incident, not a product-development or qualification Goal. It uses:

1. fresh read-only exact-scope preflight;
2. Owner presence and explicit authorization;
3. the minimum exact action selected by current invariants, possibly no action;
4. independent postverification of the known-good baseline;
5. stop.

It does not continue into H1 and does not introduce another proliferating
recovery-transaction family. Historical R1/R2/R3 details remain private.

Orphan absence is the desired cleanup postcondition, not a reason to reproduce
the historical orphan. Durable runner/service/ownership bindings may survive
preparation, while PID, creation time, session, and process-instance evidence
have `LIVE_PROCESS_EVIDENCE_TTL=SAME_OWNER_TRANSACTION_ONLY` and are reacquired
immediately before any mutation.

The authoritative ledger records that the historical target consumed its one
permitted fresh retry. The final v0.1 Master Goal must not retry it, repair its
generic Auditor route, invent P0 PASS, or recreate historical failure state.
H1 entry is instead governed by:

```text
H1_ENTRY_AUTHORITY =
    P0_PASS
    OR
    (
        HISTORICAL_P0_TARGET_RETIRED_BY_OWNER=true
        AND
        FRESH_OFFICIAL_H1_TARGET_SELECTED=true
    )
```

Retiring the historical target and selecting or establishing the exact fresh
official target are one explicit, plan-bound Owner decision. This changes only
qualification authority; all H1 safety, live-readiness, and restoration
requirements remain intact.

## 3. H1 live adapters and readiness

Autonomous source work completes the reusable live layer behind the accepted
G11R boundaries without using real credentials or mutating the real runner:

- authenticated GitHub REST transport limited to exact-runner label read,
  add-one, remove-one, and positive readback;
- opaque credential-reference provider boundary with OS-backed Windows adapter;
- exact remote runner and local home/image/identity/work-root binding;
- reserved-selector uniqueness and ownership observation;
- live readiness evidence collection;
- trusted workflow identity/contract verification;
- routing and restore-readiness verification;
- product integration seams that remain disabled until explicit Owner
  configuration and a live readiness pass.

Source and synthetic proof can validate adapters and the verifier but cannot
authorize H1. Before the Owner gate, all eleven live readiness fields must be
positively proved under the accepted schema.

## 4. Parallel productization source preparation

Goal: `RM-V0_1-PRODUCTIZATION-SALVAGE-PREP-001`.

Selectively extract and correct reusable G12-G15 assets from PRs #14-#16 onto
current authoritative main. Work remains source/sandbox-only and may produce
focused draft candidates while the Owner lane is pending.

```text
G12_G15_SOURCE_PREPARATION_ALLOWED=true
G12_G15_ACCEPTANCE_REQUIRES_H1_PASS=true
G12_G15_ACCEPTANCE_REQUIRES_BASELINE_RESTORE_PASS=true
```

Do not rebase or merge the stale stack wholesale.

## 5. H1 qualification and restore

After `H1_ENTRY_AUTHORITY` is satisfied, protected private evidence storage,
approved credential/binding/workflow/routing configuration, and all eleven
live-readiness fields pass, one immutable accepted source candidate enters one
Owner transaction.

H1 proves advertised capacity, trusted routing, active-job preservation,
selector withdrawal/readback, conservative racing-assignment handling,
achieved drain, re-advertisement/reconnect, and automatic baseline restoration.

```text
QUALIFICATION=<PASS|FAIL|BLOCKED>
RESTORE=<PASS|FAIL>
```

Productization acceptance requires:

```text
H1_QUALIFICATION=PASS
H1_RESTORE=PASS
H1_TARGET_AUTHORITY=ACCEPTED
```

`H1_TARGET_AUTHORITY=ACCEPTED` means either the original P0 path passed or the
explicit fresh-target Owner path was accepted.

## 6. Productization acceptance and G15R

After H1 and restoration pass, accept focused corrected G12-G15 milestones:

- G12 user-session autostart;
- G13 immutable versioned installation;
- G14 staged update and rollback;
- G15 packaging, provenance, and doctor.

Then build exactly one authoritative-main G15R candidate and prove the complete
package -> sandbox install -> Agent/Tray/CLI -> policy/probes -> lifecycle ->
update -> rollback -> uninstall chain. Only this immutable RC may enter H2.

## 7. H2 cutover and sustained dogfood

H2-A is an Owner-authorized cutover of the immutable G15R RC with preserved
rollback and source/runtime isolation. H2-B is at least 24 hours of ordinary
workstation dogfood; 48-72 hours is preferred when practical.

G17 cannot begin until the minimum dogfood window completes without a
release-blocking lifecycle fault.

## 8. G17 closeout and H3 release

G17 freezes the candidate, reconciles the ledger and documentation, binds
dogfood evidence, and completes hosted CI, privacy/security, package,
provenance, and checksum checks. It does not publish a stable release.

H3 is the explicit Owner transaction that tags and publishes `v0.1.0`, verifies
the public artifacts, and stops without automatically starting v0.2.

## Owner transaction state vocabulary

Owner availability and cancellation are control flow, not source defects.

| State | Meaning |
|---|---|
| `PREPARING` | source, preflight, rollback, and evidence prerequisites are still being assembled |
| `PREPARED` | all non-Owner prerequisites are complete and freshly verifiable |
| `WAITING_FOR_OWNER` | preparation is resumable but fresh authorization/Owner presence is pending |
| `OWNER_CANCELED` | this authorization attempt ended before completion; it is not an implementation failure |
| `BLOCKED_PRECONDITION` | a required verified precondition is false or unknown |
| `BLOCKED_EXTERNAL` | an external trust/service/platform dependency prevents progress |
| `FAIL_PRE_MUTATION` | an execution defect occurred with no external mutation started |
| `FAIL_POST_MUTATION` | an execution defect occurred after mutation began; restoration remains mandatory |
| `PASS` | the bounded result and its required verification passed |

`WAITING_FOR_OWNER` is nonterminal and resumable. `OWNER_CANCELED` never permits
reuse of an old nonce, transaction, handoff, preflight, or authorization.
Qualification and restoration remain independent after mutation.

## Remaining security debt

- Harden the private evidence ACL before storing H1 live artifacts or opening
  the H1 Owner gate. This is a separately authorized Owner action.
- Keep active workflow dependencies pinned to reviewed immutable full commit
  SHAs before release.
- Add an explicit current-user/logon Named Pipe DACL and denial coverage before
  v0.1 release.

Public repository material must not contain private host identifiers, real
runner IDs, credential material, private workflow identities, or private
topology.

## Goal discipline

Every Goal reads this roadmap and the execution ledger first, starts from
authoritative main or an admitted predecessor, preserves foreign work, declares
its risk vector and non-goals, uses one writer, validates changed risk once,
reuses unchanged-risk evidence explicitly, stops at true Owner boundaries,
verifies remote main after merge, and emits a privacy-safe receipt.

Historical V3/V4/V4R/V4S and R1/R2/R3 artifacts are retained private evidence,
not active execution templates.
