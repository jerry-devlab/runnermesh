# G17 — v0.1 Release Candidate Closeout

Type: **Autonomous release-closeout train**

Stable publication: **forbidden**

## Required prestate

G17 may begin only after H2/G16 satisfies:

```text
H2_CUTOVER=PASS
DOGFOOD_DURATION_HOURS>=24
DOGFOOD_RELEASE_BLOCKER=false
ROLLBACK_AVAILABLE=true
```

The minimum 24-hour sustained dogfood window is a release gate, not optional narrative evidence.

## Mission

Freeze and reconcile the exact v0.1 candidate after successful real cutover and sustained dogfood, without publishing stable v0.1.0.

## Deliver

- exact candidate head/tag or RC identifier;
- Windows x64 candidate artifact;
- SHA-256 checksums and public provenance;
- package inspection/verification;
- final install/update/rollback/uninstall documentation;
- README/project-status refresh;
- architecture/roadmap/execution-ledger reconciliation;
- release notes/changelog draft;
- explicit known limitations matching the real admission/lifecycle semantic;
- exact-head hosted CI;
- evidence reuse ledger for unchanged tray/probe/runner/config/install gates;
- sustained-dogfood receipt binding;
- final public privacy/security review;
- rollback/recovery instructions;
- release-readiness receipt.

Do not claim stronger admission atomicity, runner authority, or platform support than H1/H2 actually proved.

## Risk vector

Release-boundary preparation; no stable publication.

## Gates

Run only release-specific fresh gates. Reuse accepted unchanged-risk evidence explicitly.

## Exit

```text
DOGFOOD_GATE=PASS
RELEASE_ARTIFACT_VERIFY=PASS
PUBLIC_PRIVACY_SECURITY=PASS
KNOWN_LIMITATIONS_CURRENT=true
RELEASE_READY=true
```

or a concrete blocker.

Do not publish stable release without H3 Owner authorization.

Next: G18 Human Gate H3.