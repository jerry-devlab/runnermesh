# G16 — Real Workstation Cutover and Sustained Dogfood

Human Gate: **H2 — explicit Owner authorization required**.

G16 is split into a one-shot cutover and a sustained ordinary-use dogfood window. It may begin only with the exact immutable RC accepted by G15R.

## H2-A — One-shot real cutover

### Mission

Perform one production-style cutover using the immutable authorized RC artifact on a trusted interactive Windows workstation.

### Required prestate

Capture installed/running execution backend, runner registration, work-root ownership, autostart state, active Agent state, accepted admission architecture, and rollback procedure before mutation.

### Prove

- user-level install into owned runtime root;
- conflict resolution with any prior execution backend only as explicitly authorized;
- preserved official runner registration where safe;
- user-owned work root;
- user-session Agent and tray;
- CLI/status/doctor/version;
- Auto Lite and User Activity probe;
- Steam Game probe with actual Steam App transition where available;
- Process List probe;
- Zen and manual modes;
- accepted admission/withdrawal semantics from G11R/H1;
- active-job-safe drain/withdrawal;
- reconnect and Agent restart/reconstruction;
- reboot/sign-in autostart;
- immutable active version and rollback;
- update/rollback behavior at safe points;
- source repository can build/test while installed stable/RC runtime remains unaffected;
- uninstall/recovery path is known and ownership-bounded.

### Hard boundaries

No arbitrary worktree binary. No untrusted public PR execution. No destructive active-job termination. Every mutation is ownership-bounded and reversible where practical.

### H2-A exit

Exact cutover receipt records prestate, mutations, healthy installed state, rollback target, and the start time of the sustained dogfood window.

## H2-B — Sustained dogfood

Minimum v0.1 release gate: **24 hours** of ordinary workstation use after successful cutover. Prefer 48-72 hours when practical.

This is not a synthetic stress loop. Use the workstation normally and collect durable product evidence.

Review for at least:

- spontaneous Agent/tray crashes;
- wrong admission/withdrawal decisions;
- stale tray/CLI state;
- failed mode/Zen transitions;
- stale Listener/Worker state;
- reconnect/reconstruction failures;
- autostart/sign-in failure;
- suspend/resume anomalies where encountered;
- unexpected console windows/background shell-outs;
- abnormal CPU/memory overhead;
- source/runtime isolation violations;
- user-visible interference;
- update/rollback or uninstall/recovery defects if those paths are exercised.

A Codex process does not need to run continuously for the full window. RunnerMesh/runtime logs and receipts should be durable; after the window, perform a bounded evidence audit.

## Release gate

G17 may begin only if:

```text
H2_CUTOVER=PASS
DOGFOOD_DURATION_HOURS>=24
DOGFOOD_RELEASE_BLOCKER=false
ROLLBACK_AVAILABLE=true
```

Any critical lifecycle/admission/restore defect resets the relevant acceptance evidence; do not merely wait out the timer.

Next: G17 after sustained dogfood PASS.