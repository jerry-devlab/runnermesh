# G15 — Packaging and Doctor Hardening

## Mission

Make the candidate distributable and diagnosable without publishing stable v0.1.

## Deliver

- Windows x64 release packaging;
- version/commit/channel/target provenance;
- SHA-256 generation/verification;
- install/update dry-run from package;
- hardened `doctor` covering Agent, runner config, ownership, IPC, autostart, installed/active version, transaction state, and provenance;
- public privacy scan/inspection;
- release workflow dry-run producing immutable candidate artifacts.

## Non-goals

No public stable release and no real production install activation.

## Risk vector

Packaging/release-prep + install inspection.

## Gates

Package inspection, checksum/provenance checks, sandbox install/update/rollback, hosted release workflow dry-run, privacy audit.

## Exit

A candidate artifact is ready for authorized real workstation cutover.

Next: G16 Human Gate H2.
