# RM-160H-DELIVERY-PROGRAM-001 — RunnerMesh delivery roadmap (160 active engineer-hours)

Status: **PLANNED / SOURCE-AND-SANDBOX SCOPE**. This document is a budget and executable Goal DAG, **not** evidence of a live qualification, permission to mutate production, or an unconditional release commitment.

Planning date: 2026-11-08. Baseline main: `ca1dd96e75a606b80d1978f83e973bd2586d36e4`. Product lane tip at planning: PR #63 @ `8cd996acfee23e5f2006e7801139c70d0d67551d` (Draft; exact-head Windows/Ubuntu CI PASS). Re-read GitHub refs, quality gates, and the execution ledger at run time.

Authoritative contracts: [end-to-end plan](../RM-END-TO-END-DELIVERY-PLAN.md), [Local Runner Manager goal](../RM-LOCAL-RUNNER-MANAGER-001.md), [v0.1 ledger](../RM-V0_1-EXECUTION-STATUS.md), [ADR 0004](../../docs/adr/0004-g11r-admission-linearization.md), [ADR 0005](../../docs/adr/0005-local-multi-runner-lifecycle.md), [quality gates](../../dev_governance_files/QUALITY_GATES.md), [autonomous trains](../../dev_governance_files/AUTONOMOUS_TRAINS.md). ADR 0006 in PR #63 remains **Draft**, not approved. This 160h program supplements the 473–891h v0.1-to-v1.0 portfolio; it does not replace that scope or guarantee v1.0.

## 1. Verified starting point and evidence boundaries

| Lane | Supported starting evidence | Not proved / blocked |
|---|---|---|
| v0.1 | G15R immutable RC on `fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab`, hosted run `42452639359` PASS; PR #60 G15R ledger/H2 plan merged; main at `ca1dd96e75a606b80d1978f83e973bd2586d36e4` | H2 independent security acceptance and fresh exact Owner cutover token; H2 real cutover; >=24h real dogfood; G17/H3 release; no v0.1.0 release |
| v0.2-A | PR #51–#59, #61–#63 Draft stack; FileRegistryStore↔Agent, CLI/Tray intent, discovery, HUX-0 sandbox, Win32 SCM source, mock stop/start/restore and fake GitHub matrix; PR #63 CI PASS | Windows live SCM write gate unissued, independent privilege review, live HUX-2, same-scope selector decision, real multi-runner HUX-3, packaging and release |
| Human UX | Owner manually accepted PR #63 development Tray as `HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED`; no per-item screenshots claimed | HUX-1 **nonempty on ZenBookDuo** unproven; screenshots not independently attested |
| Host discovery | On **laptop-zenbook14**, PR #63 candidate `8cd996a` ran `discover-readonly` and reported zero candidates with enumeration/process channels Ok; independent Windows Service and Listener/Worker counts were all zero | This is a valid **ZenBook14 empty-host** observation, not ZenBookDuo verification. Do not mark HUX1_ZENBOOKDUO PASS |
| Authorization | `SyntheticScmTestGrant` test-only; `UnissuedOwnerAuthorizationVerifier` denies trusted SCM authorization; actual writes remain disabled | No permission for UAC, Windows SCM write, GitHub labels/registration, production installation, H2/HUX-2 transactions, releases |

Do not confuse `Cargo.toml version = 0.1.0` with a published `v0.1.0` GitHub release. Do not claim `SOURCE_READY` is `LIVE_QUALIFIED`.

## 2. Target outcomes at the 160h budget boundary

**Primary:** a reviewed, installable **v0.2-A Local Runner Manager release candidate** with exact per-runner enable/disable intent, correct Windows Service and owned User Session identity, safe admission-withdraw-before-stop, bounded restart/rollback, working CLI/Tray, and independently verified HUX-2/HUX-3 *when their Owner gates are granted*. Live qualification and publication may remain `WAITING_FOR_OWNER` despite source completion.

**Parallel:** close v0.1 via its existing immutable G15R RC, authorized H2, >=24h real ordinary-use dogfood, G17, and separate H3 release approval **only if all gates are satisfied**. Otherwise leave a completed, specific preflight package and accurate `WAITING_FOR_OWNER`.

**Bounded extension:** v0.2-B **read-only** resource/probe baseline and measurement protocol; Windows Job Objects/EcoQoS **real enforcement is not an expected 160h deliverable**. Mesh, Linux, Broker, untrusted workload isolation, and v1.0 GA remain later portfolio phases.

Acceptance requires product evidence, not accumulated PR count or hours. 160h is a *planning cap for active engineering*, not elapsed wall time, a guarantee of nonstop execution, or authorization to fill unused time with features.

## 3. Work-package budget (sum exactly 160h)

| ID | Active cap | Scope and dependency | Evidence / exit |
|---|---:|---|---|
| P00: Identity and field baseline | **10h** | Observe actual ZenBookDuo read-only; distinguish Service vs User Session; confirm CI and current PR stack. Depends on #63 | Honest HUX-1 host-specific disposition with private details never pushed; operator-facing gap list. Empty host remains EMPTY_OK, not fabricated nonempty PASS |
| P01: v0.1 C0 closeout | **20h** | H2 exact-RC preflight, independent security evidence, bounded Owner transaction **only if separately authorized**, dogfood receipt and G17/H3 conditional release | `H2_PASS + RESTORE_PASS + DOGFOOD>=24h + G17 + H3` or clearly `WAITING_FOR_OWNER`. 24h ordinary use is **elapsed**, not 24 active engineer-hours |
| P02: Exact discovery and owned Session | **18h** | End-to-end Windows SCM/session identity, service config path quoting, PID reuse, partial enumeration, ownership and registry enrollment; follow P00 observations | Nonempty synthetic plus genuine read-only where available, fail-closed ambiguity, independent A/B runner identity; no credential reads |
| P03: Trusted SCM lifecycle | **24h** | SCM read/status/write boundary, trusted authorization issuance/verification design, least-privilege helper, transaction journal/readbacks, crash restore; follows P02 | Full mock matrix + native read-only; real writes remain closed until independent review and exact Owner token; no external Worker kill |
| P04: GitHub-native admission routing | **24h** | ADR 0006 scoped selector decision, same-scope collision, legacy generic-label workflows, GitHub exact per-runner selector adapter and readback, race/Busy/drain tests; follows identity proof | Accepted or explicitly BLOCKED selector contract, workflow migration/rollback rehearsal, no replace-all labels; live writes gated |
| P05: Multi-runner controller | **18h** | Bind Registry, observation, policy, admit/drain, SCM/user-session into one Agent, per-runner reconciliation and partial failures; follows P02–P04 contracts | A/B independence, ForceCI/Zen/manual ordering, observed-vs-desired correctness, restart and active-job safety |
| P06: Operator UX and installer | **16h** | CN/EN CLI/Tray, first-run enroll flow, runtime packaging/migration, v0.1 compatibility and rollback, error/Doctor clarity; follows P05 | HUX-0 evidence reused; meaningful Windows manual UX and isolated install/update/rollback, zero uncontrolled production mutation |
| P07: Qualification and release candidate | **16h** | Narrow independent review, HUX-2 on disposable one-runner test target and HUX-3 2-runner proof **only behind exact Owner gates**, RC hashes/receipts | Live and restore separately PASS or truthful PENDING; two-runner separation; v0.2-A RC, no automatic publication |
| P08: v0.2-B resource baseline only | **8h** | Exact process-tree resource read-only attribution, sampling/freshness/uncertainty, Job Object/EcoQoS capability model; no enforcement | Reproducible read-only measurements or `UNAVAILABLE`, policy skeleton only |
| P09: Risk reserve | **6h** | Unexpected focused fixes in accepted scope; no speculative new feature | Evidence-backed actual use; may remain unused |
| **TOTAL** | **160h** | P01=20h v0.1; P00+P02–P07=126h v0.2-A; P08=8h v0.2-B; P09=6h reserve | No work/time fabricated |

Work-package numbers are ceilings for budgeting and triage, not proof that tasks are complete after their cap. If a gate is blocked, park it and shift *remaining eligible budget* to another prepared source/sandbox package without expanding scope. Keep unused reserve unused rather than invent work.

## 4. Seven resumable Goal Trains (up to 24h each)

| Train | Active budget | Start and primary focus | Mandatory checkpoint |
|---|---:|---|---|
| GT-01 `RM-160H-T01-FIELD-BASELINE` | **16h** | P00 host truth and P02 identity; audit actual ZenBook14-vs-ZenBookDuo difference; initialize program ledger | HUX1 host evidence, exact HEAD, candidate gap map |
| GT-02 `RM-160H-T02-V01-CLOSEOUT-PREP` | **24h** | P01 v0.1 H2/G17/H3 preparation and any currently unblocked P02/P04 work | RC hash/independent review/Owner line; do not block source train on token |
| GT-03 `RM-160H-T03-IDENTITY-SESSION` | **24h** | P02 owned Session backend and P03 SCM semantics/source; native read-only and sandbox | exact multi-instance ownership tests, baseline compatibility |
| GT-04 `RM-160H-T04-TRUSTED-SCM` | **24h** | P03 scoped privilege and recovery, P04 GitHub admission contracts | sealed live gate remains closed absent authorized transaction |
| GT-05 `RM-160H-T05-ADMISSION-MULTIRUNNER` | **24h** | P04 selector/routing; P05 policy and partial-failure integration | collision/legacy/Busy/Unknown matrix and clean owner-state delta |
| GT-06 `RM-160H-T06-UX-INSTALL-HUX2` | **24h** | P05/P06 agent, CLI, Tray, install/rollback; P07 qualification prep | HUX2 RC/Owner packet, HUX-3 manual checklist; release isolation |
| GT-07 `RM-160H-T07-QUALIFY-RESOURCE-CLOSE` | **24h** | P07 gated live qualifications when authorized; P08 read-only resource; P09 reserve, receipts | final honest `LIVE`/Owner/CI/RC disposition, no false release |
| **TOTAL** | **160h** | Seven train sessions, one program DAG; exact package accounting takes precedence over loose session labels | Recovery after every train and at key commits |

One Cursor chat may finish multiple safe steps, but **do not promise that a single process can run for 160 uninterrupted hours**. Resumable train checkpoints are authoritative. Owner transactions do not inherit unattended autonomy or approval from the 160h program.

## 5. Human experience milestones and decision points

| Human gate | Earliest useful evidence | Accept as PASS only when |
|---|---|---|
| HUX-0 — Dev Tray | **already Owner accepted** on PR #63 | Owner observed actual interaction; no extra screenshot fabricated; no repeat unless changed risk |
| HUX-1 — ZenBookDuo read-only | GT-01 / now | On **ZenBookDuo**, collect actual service/session/process counts and identity completeness; nonempty only if truly observed. A zero result is truthful, not a failure or fabricated nonempty PASS |
| HUX-2 — one disposable Service Runner | GT-04 onward | Independent security review on exact HEAD, private exact host identity/prestate, trusted grant, bounded Owner token, safe stop/start, verified restore. Never touch dogfood runner by default |
| HUX-3 — two distinct Runner instances | GT-06/GT-07 after HUX-2 | Owner-approved real A/B runner control; disabling A never alters or cancels B; clear legacy-label behavior; persisted intention; independent remote/local readbacks |
| HUX-4 — install/dogfood release | After appropriate RC gates | Exact immutable install, startup/reboot/update/rollback and >=24h natural dogfood; v0.1 H2/H3 and v0.2-A release approvals are **separate** |

Do not create new real GitHub Runner identities, registration, or new Windows Service in an autonomous source train just to satisfy nonempty HUX-1. If ZenBookDuo has no existing runner, document that and seek a separately scoped disposable target before HUX-2.

## 6. Branch / release / authority protocol

1. Main is protected and tracks v0.1; #60 merged only ledger+H2 plan. PR #51–#59 → #61 → #62 → #63 is **Draft**, not accepted v0.1 code. Use independent worktrees; do not force rebase or bulk merge into main, and never rebuild/relabel the accepted immutable G15R RC as LRM.
2. Development may open a small number of *cohesive* Draft PRs from the current LRM tip, merging only compatible scope **after required review and CI**. Do not automatically promote trust or mutating backend into public release or main through documentation approval.
3. SCM writes, GitHub labels/groups/registration, UAC, User Session process management, startup/install, H2, HUX-2/3 live trials, and release publication each require the exact applicable Owner Gate, owner-controlled prestate and rollback. No blanket 160h authorization.
4. Cursor is the **sole Implementer**; AGY/Codex agent delegation is unavailable for this program. Cursor self-review ≠ independent security review. Arrange a genuinely separate read-only source/security review on settled head where QUALTIY_GATES requires it.
5. Public repo retains only privacy-safe counts/opaque non-secret tokens. Private Service names, runner paths, machine/user identity, runner IDs, auth tokens, credentials, work roots and logs remain offline/private. Never paste secrets into PRs/CI.
6. One settled-head selective CI and relevant Windows hosted tests per risk-changing candidate; risk-diff-empty evidence may be reused per QUALITY_GATES. No CI loops for cosmetic docs or stale failure claims.
7. If H2 or HUX-2 is blocked by missing Owner action, continue authorized P02–P06/P08 source work; no false acceptance, no indefinite spinner, no forcing live approval.

## 7. Exit / STOP criteria

**Program deliverable:** `PROGRAM_SOURCE_CLOSEOUT=PASS` when accepted source/sandbox checklist and exact evidence are complete; `PROGRAM_LIVE_DELIVERY=PASS` only if all applicable H2/HUX-2/HUX-3/RC/release gates actually passed. These dispositions must not be conflated.

Immediate STOP for current sensitive operation: ownership ambiguity, unexpected Worker Busy, remote label/selector readback uncertainty, wrong service, restore failure, missing independent security approval, missing new exact Owner authorization, or unplanned UAC. Roll back *only when separately authorized* and report `RESTORE=FAIL` honestly if unsuccessful.

When budget is depleted, finish the current safe receipt and checkpoint, leave incomplete scopes clearly `PENDING`/`BLOCKED`, and **do not invent completion or continue unattended host writes**. See [checklist](CHECKLIST.md) and [Cursor execution contract](CURSOR-EXECUTION.md).
