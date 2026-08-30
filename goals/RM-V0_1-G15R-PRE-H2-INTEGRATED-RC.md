# G15R — Pre-H2 Integrated RC

Type: **Autonomous integration train**

Expected duration: **6-12 hours**

Production cutover: **forbidden**

## Mission

Build exactly one authoritative-main Windows x64 RC after G12-G15 are accepted, then prove the complete productized stack in sandbox/development roots before H2.

## Required flow

```text
authoritative main
-> release build
-> package/provenance/checksum
-> fresh sandbox install
-> Agent + Tray + CLI
-> probes / Auto Lite
-> lifecycle integration against synthetic/fake runner seams
-> update
-> rollback
-> uninstall
-> final package/doctor verification
```

## Required invariants

- immutable package bytes and exact SHA-256;
- source/build/release/install/active separation;
- autostart points only to stable installed activation entry in sandbox qualification;
- config/state/logs live outside immutable version slots;
- update does not kill active CI work in fixtures;
- rollback restores a previously validated slot;
- uninstall/cleanup refuses foreign ownership drift;
- public privacy PASS;
- hosted CI only for public repository CI.

## Windows runtime regression

The RC must preserve the accepted PR #18 behavior:

```text
TASKLIST_RUNTIME_CALLS=0
RUNNERMESH_TASKLIST_CHILD_COUNT=0
VISIBLE_CONSOLE_FLASH=false
```

Use a bounded parent-scoped Windows smoke where practical; unrelated external processes do not count as RunnerMesh children.

## Hard boundaries

No production RunnerMesh install, no real autostart activation, no real runner/service mutation, no production cutover, no stable tag/release.

## Exit

```text
H2_RC_HEAD=<authoritative main>
H2_RC_SHA256=<sha256>
PACKAGE_VERIFY=PASS
SANDBOX_INSTALL_UPDATE_ROLLBACK=PASS
SANDBOX_UNINSTALL=PASS
TASKLIST_RUNTIME_CALLS=0
VISIBLE_CONSOLE_FLASH=false
H2_RC_READY=true
```

Only this exact immutable RC may enter H2/G16.