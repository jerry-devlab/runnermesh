# G12 — User Autostart

## Mission

Implement user-session start-on-login without activating production autostart on a real owner workstation.

## Deliver

A Windows user-scoped autostart backend with exact ownership, idempotent install/remove, drift refusal, unrelated-content preservation, and uninstall/restore behavior. Autostart targets the installed stable activation entry, never a source tree.

## Risk vector

Persistent-config/autostart safety; sandbox roots/fixtures only.

## Gates

Ownership/minimal-mutation/idempotence/drift/restore family + hosted CI.

## Exit

Autostart can be managed safely in sandbox qualification and is ready for G16 activation.

Next: G13.
