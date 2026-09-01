# RunnerMesh developer train helpers

These helpers compose the existing quality policy and normal protected GitHub
pull-request flow. They are local developer tooling, not a generic CI service,
product runtime, runner controller, or credential store.

## Baseline

The bounded baseline below uses recent accepted CI runs observed on 2026-09-01.
Durations are workflow wall time, rounded to the nearest second.

| Flow | Approximate seconds | Observed behavior |
|---|---:|---|
| docs-only pull request | 22 | Fast Gate and stable CI Gate; Cargo skipped |
| code pull request | 96 | Windows and Ubuntu code jobs passed |
| code main push | 97 | Windows and Ubuntu code jobs passed |
| docs-only main push | 96 | Windows and Ubuntu Cargo ran before DVP1 |

The known flow costs were automatic host sleep during unattended work, repeated
model polling of GitHub CI, a second ledger-only PR after ordinary milestones,
full Cargo on docs-only main pushes, and a hosted repair cycle for a Linux-only
Clippy/cfg failure. In that recent failure, Ubuntu failed after about 34 seconds
while Windows continued until about 88 seconds, which is the bounded basis for
enabling matrix fail-fast.

## Local candidate

From a clean, committed candidate:

```text
python tools/dev/train.py candidate --base <accepted-main> --portability auto
```

On a governed Windows host where the `python` app alias is disabled, use
`conda run -n base python`. The command reuses
`tools/quality/fast_gate.py --full`; it does not repeat the same Cargo test
family. Documentation-only work does not run Cargo or require portability. On
Windows, portability `auto` uses
an already-ready WSL Rust environment and the separate `target-wsl` directory.
It never bootstraps a toolchain. Results are `PASS`, `N/A`, `UNAVAILABLE`, or
`FAIL`; `UNAVAILABLE` is visible and is never printed as a pass.

## Development pipeline

The individual commands are authoritative:

```text
python tools/dev/train.py health
python tools/dev/train.py wait-pr --pr <number> --expected-head <sha>
python tools/dev/train.py merge --pr <number> --expected-head <sha>
python tools/dev/train.py wait-main --expected-main <sha>
```

`health` reports concise `KEY=value` state. GitHub commands use the installed
and authenticated `gh`; they neither read nor persist token bytes. `wait-pr`
refuses a changed head, applies the remaining deadline to every GitHub call,
and has bounded interval/timeout options. `merge` is restricted to
`jerry-devlab/runnermesh`, an open PR targeting `main`, an exact head, passing
`CI Gate`, fully healthy current-main job shape, and active no-bypass
protection. It also fails closed when merge-queue rules are present or GitHub
does not enforce strict base freshness; the current non-strict repository
policy is reported as `SAFE_MERGE_PROTECTION=FAIL` and is not changed by this
tool.
It invokes a normal merge with `--match-head-commit`; there is no admin or
bypass path. After the attempt it binds the PR's actual `mergeCommit.oid` to
authoritative `main`, verifies its parents are the health-checked main and exact
PR head, and repeats the main-ref check, including when the `gh` process itself
reports an ambiguous failure. `wait-main` verifies the exact new main run and
requires the full Windows/Ubuntu shape for code changes or the lightweight
shape for docs-only changes under one bounded deadline.

Pipeline timestamps are transient local data in the worktree Git metadata, not
a service or database. They are keyed by exact pipeline SHA so overlapping
post-merge health waits cannot mix receipts. When available, commands report
local validation, PR CI, merge wait, main CI, and total pipeline seconds.

## Windows train wrapper

```powershell
pwsh tools/dev/Invoke-RunnerMeshTrain.ps1 `
  -Profile jerry-implementer `
  -Model gpt-5.6-sol
```

The wrapper requests only `ES_CONTINUOUS | ES_SYSTEM_REQUIRED` on its own
thread, invokes Codex with an argument array and governed role profile, and
restores `ES_CONTINUOUS` in a `finally` block. An optional `-Prompt` is passed
after the option terminator, so it cannot become a Codex subcommand or an
approval, sandbox, hook-trust, configuration, feature, or path override. The
wrapper exposes no generic Codex-argument forwarding surface. It does not keep
the display awake, modify a power plan or the Registry, require UAC, or persist
a setting. Normal exit, nonzero exit, and wrapper exceptions have deterministic
cleanup coverage. Ctrl+C follows the same `finally` path when PowerShell handles
cancellation; if the process is forcibly terminated, Windows also releases the
thread-scoped request with the thread.
