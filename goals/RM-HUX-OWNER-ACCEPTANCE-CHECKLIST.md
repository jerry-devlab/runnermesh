# HUX Owner Acceptance Checklist (manual)

`PRODUCTION_MUTATION=false` · Does **not** authorize Service mutation or H2.

Automated contracts cover IPC/CLI intent parity. This checklist is for Owner
visual Tray validation and must not be marked PASS by Cursor alone.

## HUX-0 / productization UX

Prep:

```text
cargo build -p runnermesh --bins
runnermesh-hux-sandbox prepare <absolute-sandbox-root>
runnermesh-agent --development-root <absolute-sandbox-root>
```

| # | Check | Pass? |
|---|---|---|
| 1 | Tray shows both sandbox runners with distinct intent labels | |
| 2 | EN and ZH menus show the same stable action IDs / behavior | |
| 3 | Toggle disable on runner A does not change runner B intent | |
| 4 | UI never equates “intent disabled” with “Service Stopped” | |
| 5 | Busy / Draining / Unknown / Refused are visually distinct if injected | |
| 6 | After Agent restart, disabled intent remains | |
| 7 | CLI `runners status` matches Tray authority (same Agent) | |
| 8 | Resource unknown never displays as “0 saved” | |
| 9 | Incomplete discovery surfaces partial_failure / reason, not fake empty OK | |
| 10 | No production install/Service/label change occurred | |

## HUX-1 nonempty (other hosts)

On a workstation that actually hosts `actions.runner.*` services (e.g. ZenBookDuo):

```text
runnermesh-hux-sandbox discover-readonly
```

Record privacy-safe counts only (candidate_count, identity_complete_count,
enumeration statuses). Do not paste private paths/names into the public repo.

## Automated Tray/CLI consistency (Implementer)

Covered by unit contracts (not a substitute for human clicks):

```text
cargo test --lib tray::
cargo test --lib cli::
cargo test --lib hux_sandbox
```

```text
HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED
HUX0_PRODUCTION_MUTATION=false
HUX0_EVIDENCE_SOURCE=Owner_confirmation_of_PR63_dev_Tray
```

Owner accepted the #63 development Tray UX. Recorded from Owner confirmation;
Cursor did not re-run the human click list.

## HUX-1 (next)

ZenBookDuo nonempty read-only discovery remains `PENDING` until host counts are
supplied. Collection template: `goals/RM-HUX-1-ZENBOOKDUO-READONLY-CHECKLIST.md`.

## HUX-2

```text
HUX2_LIVE=NOT_AUTHORIZED
INDEPENDENT_SECURITY_REVIEW=PENDING
```

Blocked until independent security review PASS + sealed Owner proof. See
`RM-HUX-2-OWNER-SERVICE-GATE-PLAN.md` and `RM-HUX-2-READINESS-PACKAGE.md`.
