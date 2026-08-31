# G11 — Historical Real Runner Lifecycle and Graceful Drain Goal

Status: **SUPERSEDED by accepted G11R-A / G11R-B / G11R-C and Roadmap v3**

This file is retained as historical context only. Do not start a new V3/V4/V4R/V4S-style qualification from this Goal.

The 2026-08-30 product/architecture audit found that the historical path could prove portions of `Busy -> Drain` but did not yet prove the `Listening -> Drain` admission-linearization semantic required by the v0.1 product contract. Qualification infrastructure also began discovering routing/recovery prerequisites only after real-host mutation had started.

The replacement sequence is:

1. `RM-V0_1-G11R-A-ADMISSION-ARCHITECTURE.md` — choose the truthful admission/withdrawal mechanism and linearization point;
2. `RM-V0_1-G11R-B-LIFECYCLE-IMPLEMENTATION.md` — implement and prove the lifecycle in synthetic/integration fixtures;
3. `RM-V0_1-G11R-C-QUALIFICATION-READINESS.md` — prepare routing/workflows/recovery/rollback completely before host mutation;
4. Human Gate H1 — execute one prepared real qualification with automatic restore attempt.

Useful historical invariants remain valid:

- user-session start/listening/busy behavior must be qualified;
- active normal work must not be destructively killed for ordinary drain;
- no new capacity after the accepted withdrawal linearization point;
- restart/reconnect/reconstruction and one-identity/one-work-root safety remain required;
- no untrusted public PR code, silent registration change, broad ownership rewrite, or global `safe.directory` workaround;
- qualification PASS and restoration PASS are separate claims.

PR #17 is retained as research/salvage material. Its exact runner scoping, no-signal Busy-drain work, safe-wait reconstruction, and run-once experiments may be reused only when they match the architecture selected by G11R-A.

See `RM-V0_1-ROADMAP.md` and `RM-V0_1-EXECUTION-STATUS.md` for the authoritative current sequence.
