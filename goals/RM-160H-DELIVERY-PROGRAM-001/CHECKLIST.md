# RM-160H-DELIVERY-PROGRAM-001 — Goal checklist and acceptance ledger

This is a **pre-execution work checklist**, not evidence of completion. Main on planning: `ca1dd96e75a606b80d1978f83e973bd2586d36e4`. Code tip on planning: PR #63 `8cd996acfee23e5f2006e7801139c70d0d67551d`. The owner has accepted HUX-0 development Tray experience; no full click screenshots were supplied. the development host HUX-1 zero is truthful but **ZenBookDuo is PENDING**.

Read [ROADMAP.md](ROADMAP.md), `goals/RM-V0_1-EXECUTION-STATUS.md`, `dev_governance_files/QUALITY_GATES.md`, `dev_governance_files/AUTONOMOUS_TRAINS.md` and ADRs 0004/0005 before any implementation.

## P00 — field baseline and real host discovery (10h)

- [ ] P00-01 Re-fetch main, PR #51–#63 exact heads, their target base chain, active CI, v0.1 ledger and the latest sealed candidate; report SHA drift rather than assuming this document is fresh.
- [ ] P00-02 Preserve `HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED` as Owner-supplied evidence only; do not rerun without changed UX risk.
- [ ] P00-03 Run `runnermesh-hux-sandbox discover-readonly` on the **actual ZenBookDuo**, not `the non-target development Windows host`. Collect only `candidate_count,observed_only_count,refused_count,identity_complete_count,service_enumeration,session_enumeration,process_observation,incomplete`.
- [ ] P00-04 Cross-check *counts* of `actions.runner.*` services and `Runner.Listener.exe`/`Runner.Worker.exe`/`RunnerService.exe` processes. Do not publish exact IDs, paths, services or private topology.
- [ ] P00-05 Diagnose session-vs-service when `candidate_count=0`; do not convert an empty target into nonempty PASS and do not register/install a real runner without exact Owner scope.
- [ ] P00-06 Reconcile the latest PR #63 branch QA/CI and status; independent security still pending.
- [ ] **EXIT** HUX-1 truth scoped to actual host, with `PASS_NONEMPTY`, `EMPTY_OK`, `INCOMPLETE`, or `PENDING_NOT_RUN`; no fake success.

## P01 — v0.1 C0 closeout (20h active + >=24h elapsed dogfood)

- [ ] P01-01 Reverify G15R RC SHA `fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab`, run `42452639359`, archive/operator SHA-256 from existing H2 plan; no silent RC rebuild because main has docs-only changes.
- [ ] P01-02 Reconcile v0.1 ledger to current accepted reality, including any stale narrative; do not declare H2 acceptance from green CI.
- [ ] P01-03 Gather independent source/security evidence required for H2. Cursor self-review only is not acceptance.
- [ ] P01-04 Prepare exact H2 Owner transaction: private target and prestate, versioned installed path, authority, interval, rollback, remote/local readback and STOP conditions.
- [ ] P01-05 **HUMAN GATE:** do not perform H2 cutover until a new exact authorization for that immutable RC is issued; previous H2 token was consumed by historical failed attempt.
- [ ] P01-06 After authorized successful H2 and verified restore obligations, conduct real ordinary-use dogfood for **at least 24h elapsed**; capture timestamps without inventing hours.
- [ ] P01-07 Prepare G17 evidence, provenance, release notes; **separate H3 Owner approval** before `v0.1.0` publication.
- [ ] **EXIT** `G15R=PASS, H2=PASS, H2_RESTORE=PASS, DOGFOOD>=24h PASS, G17=PASS, H3=PASS` or honest `WAITING_FOR_OWNER` and release unpublished.

## P02 — exact Windows identity + owned Session (18h)

- [ ] P02-01 Verify completeness/errors of SCM enumeration, service config buffer, image token, start account, SID/identity hash and command-line quoting; fail closed on absent evidence.
- [ ] P02-02 Correct process image attribution by exact path boundaries (not prefix sibling path), PID reuse, current-owner evidence, stale snapshots, inaccessible processes and Worker linkage.
- [ ] P02-03 Distinguish Windows Service, owned User Session, unsupported scheduled task and other wrappers. No cross-user adoption; no filesystem-wide secret crawl.
- [ ] P02-04 Implement/refine owned User Session backend source and fake lifecycle; actual process start/stop is an Owner-gated mutation.
- [ ] P02-05 Validate `RunnerRecord` exact home/workroot/server ID/scope binding, duplicate-homes, duplicate identities and same-scope collisions; no auto-enroll.
- [ ] P02-06 Inspect mixed platform and Windows hosted tests; one real read-only nonempty case where available, otherwise hold as PENDING.
- [ ] **EXIT** exact instances are distinguishable with honest Unknown/Refused, with no scope leak.

## P03 — trusted SCM lifecycle (24h)

- [ ] P03-01 Review PR #63 `TrustedOwnerAuthorizationVerifier` / `UnissuedOwnerAuthorizationVerifier` and `SyntheticScmTestGrant`. Test grants must be forgeable and **never authorize real Windows writes**.
- [ ] P03-02 Implement bounded, independently reviewable trusted Owner issuance/verification plan for exact service, image, execution identity, rights, expiry and target transaction; no credentials hardcoded.
- [ ] P03-03 Separate normal-user Agent from narrow privileged helper where needed; no blanket elevation, broad SCM handle, wildcard name, foreign services or generic Process Kill.
- [ ] P03-04 Validate Win32 `SERVICE_STATUS_PROCESS`, aligned `QUERY_SERVICE_CONFIGW`, explicit errors, timeout/readback, name/image drift and Worker Busy.
- [ ] P03-05 Journal prestate/phase/rollback to resilient private storage; process crash mid-stop and restore after resume never imply PASS without readback.
- [ ] P03-06 Fully test fake backend with stop/start/config failure, partial success, concurrent Agent, stale proof and recovery.
- [ ] P03-07 **HUMAN GATE:** implement real live path but do not activate or run actual SCM writes before independent security review + exact Owner token; no production dogfood runner for HUX-2.
- [ ] **EXIT** `SCM_SOURCE_READY` with tested fail-closed no-owner path; `SCM_LIVE_PASS` only with separate Owner transaction.

## P04 — per-runner GitHub admission (24h)

- [ ] P04-01 Decide ADR 0006 successor for per-runner namespace: normalized key stability, collision safety, one owned selector per instance, exact server identity and scope.
- [ ] P04-02 Preserve ADR 0004 `runnermesh-admit` for frozen v0.1 only; block same-scope dual managed enrollment until independently accepted scoped selector/migration design.
- [ ] P04-03 Implement scoped exact-label add/remove + fresh GitHub/local readback behind separately scoped Owner authorization; no replace-all/remove-all operations.
- [ ] P04-04 Build owned-workflow routing completeness / `runs-on` migration checklist. Generic self-hosted labels alone are `UNQUALIFIED_LEGACY_ROUTING` and **not a safe no-new-job claim**.
- [ ] P04-05 Exercise Worker Busy, racing assignment, GitHub link loss, label eventual visibility, stale token, permission denial, scope drift and partial rollback in Fake GitHub.
- [ ] P04-06 Separate `desired_admission`, `observed_selector`, `achieved_DRAINED`, `Stopped`; selector removal alone is never a linearizable job-assignment barrier.
- [ ] P04-07 **HUMAN GATE:** no real label/workflow/group/registration mutation absent separate Owner approval; ADR decision itself does not grant it.
- [ ] **EXIT** no cross-runner remote mutation, no false `FULL`/`DRAINED` and reversible migration plan.

## P05 — two+ runner runtime (18h)

- [ ] P05-01 Wire a single Agent authority across all enrolled local Runners; no duplicate per-user IPC writers.
- [ ] P05-02 Enforce precedence: hard safety and ownership, Zen/maintenance, manual disabled, explicit ForceCI, Auto. Persist intent before claiming applied.
- [ ] P05-03 End-to-end per-runner observe→decide→reconcile, admitted/withdrawing/busy/idle/stopped/refused; separate Service and Session backends.
- [ ] P05-04 Disable A while B is active; prove B’s desired, Worker, service, selector and registry record unchanged (synthetic, and live only under Owner).
- [ ] P05-05 Reboot, Agent crash, resume, stale PID, config corruption, selector readback uncertainty and partial batch failure matrix.
- [ ] P05-06 No destructively terminating Workers; no unbounded retry/reconcile loop, no automatic re-enable of explicitly disabled runner.
- [ ] **EXIT** exact A/B independence, source/sandbox proof, live proof explicitly separate.

## P06 — UX, migration and package (16h)

- [ ] P06-01 CLI/Tray list, discover, enroll, enable/disable, prepare, status, reason, single/partial batch results; proper bilingual labels and optimistic-intent vs achieved distinction.
- [ ] P06-02 Reuse Owner HUX-0 PASS for unchanged presentation paths; additional visual click only if new UX risk changed.
- [ ] P06-03 No secret path/credential/Runner ID in public logs; operator may opt to inspect private local details under controlled diagnostic mode.
- [ ] P06-04 Test versioned Registry migration/rollback and frozen v0.1 single-binding compatibility; avoid self-launch from source worktrees.
- [ ] P06-05 Exercise immutable candidate packaging, per-user Startup, update, rollback, uninstall, crash-safe preservation under sandbox roots.
- [ ] P06-06 Native Windows CLI/Tray/IPC/Doctor/manual usability with no production mutation; choose one settled-head CI.
- [ ] **EXIT** usable development/RC UX, honest fail states, reversible installation candidate.

## P07 — HUX-2/HUX-3 qualifications + RC (16h active)

- [ ] P07-01 Freeze source candidate, map exact code SHA, CI and reviewed security-sensitive diff.
- [ ] P07-02 Obtain separate **independent** focused privilege/security review; Cursor self-review cannot certify it.
- [ ] P07-03 Prepare single disposable, exact-name, verified **Owner** test Service with prestate, active-job absence, admission ownership, restore instructions and bounded window.
- [ ] P07-04 **HUMAN GATE:** authorized HUX-2 Stop→readback→Start→restore; verify RESTORE separately; no change to production dogfood runner.
- [ ] P07-05 **HUMAN GATE:** authorized HUX-3 2+ instance test; A disabled and B completes legitimate CI; restart preserves manual intent; no cross-scope label/service drift.
- [ ] P07-06 Assemble v0.2-A immutable RC, provenance/digest, install and rollback receipts; separate publication authorization if publishing.
- [ ] **EXIT** `HUX2_LIVE/HUX3_LIVE=PASS` only with real Owner evidence, or `BLOCKED_BY_OWNER` while RC source stays ready.

## P08 — v0.2-B read-only resources (8h)

- [ ] P08-01 Read-only exact process-tree memory/CPU sample scope, time/freshness, relationship to enrolled Runner and User Session owner.
- [ ] P08-02 Detect Job Objects/EcoQoS feature support and privilege/OS constraints without creating containers, attaching processes or changing affinity.
- [ ] P08-03 Record measurement methodology and honestly `UNKNOWN/UNSUPPORTED` when attribution is uncertain.
- [ ] P08-04 Keep real resource enforcement for later v0.2-B program, not this one.
- [ ] **EXIT** reusable read-only capability evidence and no fabricated resource-savings claims.

## P09 — focused contingency (6h)

- [ ] Only documented P0/P1 or integration risk deltas; no new Mesh, Linux, broker, remote dashboard, generic scheduler or untrusted-runner infrastructure.
- [ ] Stop if budget cap reached; unused time can remain unused.

## Gate matrix, CI and handoff

| Gate | Preconditions | Executor | Required result |
|---|---|---|---|
| G-RO (ZenBookDuo) | correct host, correct candidate, no mutating commands | Owner/Cursor on host | counts and false-empty classification; `EMPTY_OK` is valid |
| G-H2 (v0.1) | exact G15R RC + independent security acceptance + new Owner token + rollback | Owner-only transaction | true H2 and separate RESTORE receipts |
| G-HUX2 | exact test service, genuine trusted auth, independent review, no Busy Worker | Owner-only transaction | Start/Stop and independent restore; otherwise BLOCKED |
| G-ADR006 | scoped selector ownership/routing design accepted | Owner architecture decision | cannot self-approve same-scope mutations |
| G-HUX3 | HUX2 PASS, routing design, 2 enrolled exact runners, Owner test window | Owner-only transaction | no cross-runner mutation; real job proof |
| G-RELEASE | immutable RC, live/dogfood where required, provenance, release notes, specific approval | Owner | published only when separately authorized |

Gate success is never inferred from CI PASS. A reviewer may read-only inspect PR tip without modifying Implementer worktree.

## Receipt template

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
TRAIN_ID=
START_MAIN=
END_MAIN=
START_LRM_TIP=
END_LRM_TIP=
WORK_PACKAGE=
ACTIVE_HOURS_MEASURED_OR_ESTIMATED=
BUDGET_USED_CUMULATIVE=
BUDGET_LEFT=
CODE_HEAD=
PR=
LOCAL_GATE=
CI_GATE=
HUX0_OWNER_TRAY=PASS_OWNER_ACCEPTED
HUX1_ZENBOOKDUO=
V01_G15R=PASS
H2=
HUX2_SYNTHETIC=
HUX2_LIVE=
HUX3_LIVE=
INDEPENDENT_SECURITY_REVIEW=
OWNER_AUTH=
PRODUCTION_MUTATION=false
RESTORE=
NEXT_SAFE_GOAL=
RECOVERY_CHECKPOINT=
```

Recording hours must indicate measured vs estimated, never substitute Git timestamps/CI elapsed for active engineering time. Owner waits and >=24h dogfood elapsed are excluded. See [Cursor execution contract](CURSOR-EXECUTION.md).
