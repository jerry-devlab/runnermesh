# G06 — Tray, Theme, and Localization

## Mission

Implement the primary Windows daily UI without moving operational authority out of the Agent.

## Deliver

- native lightweight tray/event loop;
- version/channel, Agent health, capacity/mode, runner phase, GitHub link state;
- Zen toggle;
- mode submenu;
- per-probe enablement + runtime state menu;
- status/doctor/logs entries;
- settings submenu: system/light/dark, system/zh-CN/en-US, start-on-login preference, idle-threshold presets, update check, config/data location;
- check-update and exit-after-drain commands;
- stable menu/action IDs independent of visible strings;
- UI-thread ownership for menu/icon mutation;
- state icon/tooltip semantics that do not rely on color alone.

Use synthetic or Agent snapshots. Tray commands route through `AgentCommand`.

## Non-goals

No real runner control, production autostart activation, TUI, WebView shell, or full settings window.

## Risk vector

Presentation + ordinary code.

## Gates

Hosted CI plus one representative Windows tray/presentation proof on settled candidate. Test language/theme changes do not change machine contracts or policy.

## Exit

Tray can fully render/control the synthetic Agent contract in English and Simplified Chinese and switch system/light/dark safely.

Next: G07.
