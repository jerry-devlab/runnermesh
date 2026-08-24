# v0.1 Release Criteria

v0.1 is released only when it is useful as a long-lived first usable product, not merely a feature demo.

## Product criteria

- ordinary-user Agent starts and remains single-authority;
- Tray and CLI reflect the same `AgentSnapshot`;
- modes, Zen, probe controls, and Auto Lite are explainable and persistent;
- User Activity, Steam Game, and Process List probes have correct Active/Inactive/Unknown/Unavailable/Suspended semantics;
- official runner observation, supervision, restart/reconnect/adoption, and graceful drain are qualified;
- active jobs are not destructively killed by normal drain/Zen/update behavior;
- user-session autostart and restart recovery are safe;
- versioned install, staging, activation, rollback, and uninstall ownership are proven;
- source/build/release/runtime isolation is proven;
- `doctor`, logs, version provenance, and stable JSON are useful for diagnosis;
- Simplified Chinese/English and system/light/dark preferences work without changing machine contracts.

## Trust criteria

- public PR CI remains GitHub-hosted;
- no private dogfood information exists in public repository/release artifacts;
- persistent personal workstations do not execute arbitrary untrusted fork code by default;
- no hidden GitHub PAT requirement is introduced for supervising an already-configured runner;
- one execution identity / one active owned work root remains enforced.

## Packaging criteria

- exact RC head identified;
- Windows x64 artifact produced from release workflow;
- SHA-256 checksums and build provenance present;
- package contents inspected;
- clean user-level install and rollback path tested;
- representative trusted dogfood on the exact candidate passes;
- release notes truthfully distinguish implemented v0.1 from future roadmap items.

## Publication gate

Public `v0.1.0` tag/release creation requires explicit Owner authorization. Publication is not implied by an RC PASS.
