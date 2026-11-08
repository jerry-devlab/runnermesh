# LRM / HUX PR stack readiness (#51–#63)

`TRAIN_ID=RM-LRM-HUX2-READINESS-001` · snapshot timebound; re-check before merge.

Do **not** blindly merge the stack into frozen v0.1 RC. Do **not** promote LRM
multi-runner productization into `v0.1.0` without an explicit ADR.

## Dependency chain (LRM product lane)

```text
main
 └─ #51 feat/lrm-01-discovery-source-only
     └─ #52 feat/lrm-02-registry-persistence
         └─ #53 feat/lrm-03-commands-cli-ipc
             └─ #54 feat/lrm-04-service-backend-synthetic
                 └─ #55 feat/lrm-05-user-session-backend-synthetic
                     └─ #56 feat/lrm-06-multi-admission-reconcile
                         └─ #57 feat/lrm-07-tray-status-resource
                             └─ #58 feat/lrm-08-integrated-gate-owner-prep
                                 └─ #59 feat/lrm-09-reconcile-semantic-fixes
                                     └─ #61 feat/lrm-runtime-integration
                                         └─ #62 feat/lrm-hux-foundation
                                             └─ #63 feat/lrm-productization   ← tip
```

`#60` (`feat/g15r-requalification-from-main` → `main`) is a **parallel G15R/H2
docs lane**, not an LRM stack parent. Its CI failure must not be used as
productization evidence and must not gate LRM tip merges wrongly—or be ignored
for the H2 lane.

## Tip status (at readiness train start)

| PR | Draft | CI | Role |
|---|---|---|---|
| #51–#59 | Draft | PASS | stacked source foundations |
| #60 | Ready | FAIL | G15R docs; separate lane |
| #61–#63 | Draft | PASS | runtime + HUX + productization tip |

Tip baseline before this readiness commit: `#63` @ `3fd2816e24cb…`

## Compatibility / duplication notes

- Tip `#63` absorbs productization of SCM executor + synthetic HUX-2; lower PRs
  remain independently revertible stacked deltas.
- Historical Goal contracts under `goals/` that predate StayStopped admission
  withdraw / sealed Owner gate are superseded by tip source + this readiness pack.
- Duplicate discovery helpers: prefer tip `windows_runner_discovery` /
  `discover_local_runners_read_only`; do not reintroduce parallel private-path
  printers.

## Minimal safe merge plan

1. Keep `#51–#63` Draft while `INDEPENDENT_SECURITY_REVIEW=PENDING` for live SCM.
2. Do not merge tip into protected `main` solely because CI is green.
3. If Owner wants source landed without live authority:
   - merge bottom-up only after intentional review of each layer;
   - tip may land as **source-only** with `LiveScmExecutionGate` sealed and
     `WindowsScmBackend` mutations inert;
   - never enable live Start/Stop in the same merge.
4. Do not fold LRM tip into frozen v0.1 RC cutover; H2 remains a separate Owner
   transaction on the G15R lane.
5. `#60` resolve independently for G15R; do not batch-merge with `#63`.

## Remaining blockers

```text
INDEPENDENT_SECURITY_REVIEW=PENDING   # live SCM surface
HUX1_NONEMPTY_ZENBOOKDUO=UNPROVEN     # host unreachable / not observed here
HUMAN_TRAY_VISUAL=PENDING_OWNER
HUX2_LIVE=NOT_AUTHORIZED
H2=WAITING_FOR_OWNER                  # separate lane
#60 CI=FAIL                           # G15R docs lane
```

## Non-goals

- Opening live SCM / UAC
- Merging the entire stack in one click
- Declaring Cursor self-review as independent security PASS
- Publishing v0.1.0 from this tip
