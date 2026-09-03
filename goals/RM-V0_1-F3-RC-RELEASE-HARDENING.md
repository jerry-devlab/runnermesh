# RM-V0_1-F3-RC-RELEASE-HARDENING

Status: **Planned fast-closeout Goal**

Target active time: **3-5 hours**

Default launch:

```powershell
codex --yolo -m gpt-5.6-sol
```

## Mission

Produce exactly one immutable authoritative-main G15R release candidate and close the remaining v0.1 release-security debt without turning G15R into a second implementation project.

## Entry gate

Require:

```text
G12=ACCEPTED
G13=ACCEPTED
G14=ACCEPTED
G15=ACCEPTED
AUTHORITATIVE_MAIN_HEALTH=PASS
```

## G15R scope

G15R is integration evidence. Reuse accepted unit/ownership evidence from G12-G15 and focus on the end-to-end chain:

```text
exact authoritative-main package
-> provenance/checksum verify
-> sandbox install
-> select active version
-> Agent startup
-> Tray/CLI/IPC smoke
-> policy/probe representative smoke
-> runner lifecycle integration using non-production/sandbox seams where possible
-> staged update
-> active-job deferral proof
-> rollback/reconcile
-> uninstall/owned-content cleanup
```

Do not rewrite installation/update/package modules unless the integrated chain exposes a concrete defect.

## RC identity

Produce exactly one bounded release-candidate identity tied to an immutable authoritative-main commit.

Require:

- exact source commit;
- target `x86_64-pc-windows-msvc`;
- package manifest and member SHA-256 verification;
- final archive SHA-256;
- source/build/release/installed/active separation;
- no publication as stable release.

A repair candidate supersedes the previous RC identity; only one final RC may enter F4.

## Named Pipe DACL release debt

Close the existing release blocker inside this Goal:

```text
NAMED_PIPE_EXPLICIT_DACL=REQUIRED_BEFORE_RELEASE
```

Implement the minimum explicit Windows Named Pipe security boundary consistent with the existing single-user/session product contract.

Required evidence:

- authorized current-user/logon client can connect;
- unauthorized principal/path is denied in deterministic coverage;
- existing IPC protocol tests remain green;
- no broad `Everyone`/world access is introduced;
- no unrelated Windows security policy is modified.

Keep this as a focused IPC security delta. Do not create a generic Windows ACL framework.

## Release hardening

In the same focused PR/train, verify release-relevant items that are cheap to close now:

- active workflow dependencies remain pinned to immutable reviewed SHAs;
- package/provenance/checksum tooling is deterministic;
- public/privacy scan is clean;
- release artifact naming is frozen;
- rollback path is explicit;
- docs required for H2 operator use are current.

Do not publish `v0.1.0`.

## Validation economy

Run one settled candidate gate and the risk-specific security/IPC/integration checks. Reuse accepted G12-G15 evidence when the risk diff is empty.

Require exact-head hosted Windows/Ubuntu code CI and one focused independent security review because the Named Pipe trust boundary changes.

Do not run a comprehensive whole-project audit.

## Exit

Best case:

```text
F3=PASS
G15R=PASS
H2_RC_READY=true
RC_COMMIT=<exact authoritative-main sha>
RC_ARCHIVE_SHA256=<sha256>
NAMED_PIPE_EXPLICIT_DACL=PASS
RELEASE_SECURITY_DEBT_BLOCKING=0
PRODUCTION_CUTOVER=false
```

## Final receipt

```text
DISPOSITION=
GOAL_ID=RM-V0_1-F3-RC-RELEASE-HARDENING
START_MAIN=
FINAL_MAIN=
PR=
RC_COMMIT=
RC_ARCHIVE_SHA256=
G15R=
NAMED_PIPE_DACL=
LOCAL_GATE=
CI_GATE=
FOCUSED_SECURITY_REVIEW=
H2_RC_READY=
PRODUCTION_MUTATION=false
NEXT_GOAL=RM-V0_1-F4-H2-CUTOVER-DOGFOOD
```