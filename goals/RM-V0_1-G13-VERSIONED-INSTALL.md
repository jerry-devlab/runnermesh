# G13 — Versioned User Installation

## Mission

Implement the production-safe user-level layout frozen by ADR 0003.

## Deliver

- immutable `versions/<version>/` payloads;
- stable activation entry/shim under `bin/`;
- explicit active-version metadata;
- config/state/log separation;
- exact ownership ledger for install/uninstall;
- source/runtime isolation checks;
- refusal to overwrite foreign/unowned content.

Use temp/sandbox install roots only.

## Risk vector

Install/activation + persistent ownership safety.

## Gates

Install/uninstall/idempotence/foreign-content/drift/source-isolation fixture family + hosted CI.

## Exit

Multiple validated slots can coexist and one can be selected without mutating source/build output.

Next: G14.
