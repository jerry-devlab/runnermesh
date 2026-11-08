# HUX-0 Owner Tray card (human visual)

`PRODUCTION_MUTATION=false` · sandbox only · does not authorize Service/H2.

Cursor automated Tray/CLI contracts are **not** a substitute for this card.
Mark `HUMAN_TRAY_VISUAL=PASS` only after Owner completes the clicks below.

## Prep (absolute sandbox root)

```text
cargo build -p runnermesh --bins
$root = "<absolute-empty-dir>"
.\target\debug\runnermesh-hux-sandbox.exe prepare $root
.\target\debug\runnermesh-agent.exe --development-root $root
```

## Click / observe checklist

| # | Action | Expected |
|---|---|---|
| 1 | Open Tray menu | Both sandbox runners listed with distinct intent |
| 2 | Switch EN ↔ ZH | Visible labels change; action IDs/behavior identical |
| 3 | Disable runner A | Runner B intent unchanged |
| 4 | Read Busy/Drain/Unknown (if injected) | Visually distinct; never equals “Service Stopped” from intent alone |
| 5 | Restart Agent same `$root` | Disabled intent persists |
| 6 | CLI `runners status` | Matches Tray authority from same Agent |
| 7 | Resource unknown | Never shown as “0 saved” |
| 8 | Confirm environment | No production install/Service/label/credential change |

## Disposition

```text
HUMAN_TRAY_VISUAL=<PASS|PENDING_OWNER|FAIL>
```
