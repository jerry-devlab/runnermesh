# RM-V0_1-F5-RELEASE-CLOSEOUT

Status: **Planned fast-closeout Goal**

Target active time: **1.5-3 hours**

Default launch:

```powershell
codex --yolo -m gpt-5.6-sol
```

## Mission

Complete G17 candidate closeout and, after its acceptance checkpoint, perform the explicit H3 Owner publication of `v0.1.0` in the same bounded release session.

This compresses handoff only. G17 freeze/acceptance and H3 publication remain separate gates.

## Entry gate

Require:

```text
H2_CUTOVER=PASS
DOGFOOD=PASS
DOGFOOD_HOURS=>=24
RELEASE_BLOCKING_LIFECYCLE_FAULT=false
RC_IDENTITY=IMMUTABLE
```

## Phase A — G17 closeout

Freeze the exact release candidate. Do not introduce product features.

Freshly prove only release-specific evidence plus any risk delta since F3/F4:

- exact candidate commit and clean protected-main lineage;
- hosted CI required by the final source risk;
- release package contents;
- final archive checksum;
- package member checksums and provenance;
- public/privacy scan;
- explicit Named Pipe DACL remains accepted;
- no unresolved P0/P1 release findings;
- README/operator docs/release notes match the candidate;
- dogfood evidence is bound to the same immutable RC lineage;
- rollback/recovery instructions are current.

Reuse accepted unchanged-risk G11R/H1/G12-G15/H2 evidence. Do not launch a comprehensive full-history audit.

If documentation-only changes are needed, keep them bounded and use the docs fast path. Do not change the RC payload after freeze without creating a new RC and re-evaluating the affected release gates.

### G17 checkpoint

Require:

```text
G17=PASS
RELEASE_CANDIDATE_FROZEN=true
H3_ELIGIBLE=true
```

If not, stop. Do not publish.

## Phase B — H3 Owner publication

Show a concise publication plan and wait for explicit Owner authorization:

```text
AUTHORIZE_F5_H3_V0_1_0_PUBLICATION
```

Then:

1. create the exact `v0.1.0` tag on the frozen accepted commit;
2. publish the stable release using only the already-verified artifacts;
3. publish/check the expected SHA-256/provenance information;
4. fetch/read back the public release metadata;
5. download/verify public artifacts against the accepted hashes;
6. verify no secret/private topology material is exposed;
7. update only the minimum final public ledger/status documentation if needed;
8. stop.

Do not automatically begin v0.2.

## Failure handling

Publication failure does not authorize rebuilding or substituting artifacts silently. Keep the accepted candidate frozen, report the exact publication failure, and retry only the publication operation when safe.

A mismatch between published bytes and accepted hashes is release-blocking and must not be waived.

## Exit

Best case:

```text
F5=PASS
G17=PASS
H3=PASS
TAG=v0.1.0
PUBLIC_RELEASE=PASS
PUBLIC_ARTIFACT_VERIFY=PASS
RUNNERMESH_V0_1_0=RELEASED
```

## Final receipt

```text
DISPOSITION=
GOAL_ID=RM-V0_1-F5-RELEASE-CLOSEOUT
FINAL_MAIN=
G17=
RELEASE_CANDIDATE_COMMIT=
RELEASE_CANDIDATE_SHA256=
H3=
TAG=v0.1.0
PUBLIC_RELEASE=
PUBLIC_ARTIFACT_VERIFY=
PRIVACY_SECURITY=
V0_1_0=
NEXT_ACTION=STOP_AFTER_RELEASE
```