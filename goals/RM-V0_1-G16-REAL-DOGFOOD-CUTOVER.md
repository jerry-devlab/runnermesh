# G16 — Real Workstation Cutover and Dogfood

Human Gate: **H2 — explicit Owner authorization required**.

## Mission

Perform one production-style cutover using an immutable authorized RC artifact on a trusted interactive Windows workstation.

## Required prestate

Capture installed/running execution backend, runner registration, work-root ownership, autostart state, active Agent state, and rollback procedure before mutation.

## Prove

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
- graceful drain;
- reboot/sign-in autostart;
- Agent crash/restart/adoption;
- immutable active version and rollback;
- source repository can build/test while installed stable/RC runtime remains unaffected.

## Hard boundaries

No arbitrary worktree binary. No untrusted public PR execution. No destructive active-job termination. Every mutation is ownership-bounded and reversible where practical.

## Exit

Exact dogfood receipt records prestate, mutations, final healthy state, and tested rollback.

Next: G17.
