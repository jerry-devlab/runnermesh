# G04 — Local IPC

## Mission

Connect frontends to the Agent through user-local Windows Named Pipe IPC and enforce single controlling Agent authority.

## Deliver

- typed command/response framing;
- local Named Pipe server/client;
- reconnect and timeout behavior;
- protocol/version rejection behavior;
- user-scoped single-instance guard;
- separate authority from any future observer-only development profile.

## Non-goals

No real runner control, tray presentation, production install/autostart, or elevated service.

## Risk vector

Ordinary code + IPC/concurrency. No production mutation.

## Gates

Deterministic IPC/concurrency tests on Windows plus hosted CI. Verify second controller cannot acquire authority.

## Exit

CLI/test clients can safely round-trip typed commands to one Agent and recover from reconnects.

Next: G05.
