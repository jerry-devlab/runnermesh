# G06R — Native Tray and Persistent Agent Runtime

## Historical correction

G06 presentation contracts are accepted: stable menu IDs, localized rendering,
theme behavior, and UI-thread ownership were proven against synthetic Agent
snapshots. A real persistent Windows Agent and notification-area backend were
not proven by that Goal. This recovery Goal closes only that gap; it does not
reopen or replace G06.

## Mission

Implement a non-elevated `runnermesh-agent` development runtime with the
existing Agent Core as its only business authority, a user-local Named Pipe,
and a real native Windows tray icon/menu backend.

## Deliver

- a Cargo-native Agent binary distinct from the CLI;
- isolated development-root configuration/state only;
- real `tray-icon`/native Windows notification-area backend and event loop;
- native menu/icon/tooltip refresh from the existing `TrayRender` model;
- typed tray commands through local Agent IPC;
- real ordinary-user runtime smoke evidence, including Pipe reachability and
  theme/language refresh and a direct Owner-session presentation check.

## Windows popup-theme decision

The initial native implementation correctly used `muda` for the logical menu
and stable command IDs, but left the notification-area popup to its default
Windows rendering. `muda`'s `MenuTheme` APIs apply to a window menu bar, not
to submenus or context menus, so they are unsuitable for the RunnerMesh tray
popup. G06R therefore uses the documented Win32 owner-drawn menu path
(`MFT_OWNERDRAW`, `WM_MEASUREITEM`, and `WM_DRAWITEM`) on the tray window
handle exposed by `tray-icon`. The renderer consumes only `TrayRender` labels
and the effective presentation theme; it has no Agent, policy, runner, or
probe authority. All menu ownership and refresh remain on the tray UI thread.

## System preferences and contextual help

`ThemePreference::System` and `LanguagePreference::System` remain persisted
intent values. The Agent observes their effective presentation values at
runtime: Windows `UISettings` foreground color determines the effective theme,
and the first documented current-user UI-language entry determines the
effective locale (`zh-*` maps to `zh-CN`; other values map to `en-US`). The
snapshot exposes both preference and effective value for diagnostics.

The native tray uses the documented `UISettings.ColorValuesChanged` event to
request a System-theme refresh, with the owner UI thread applying the updated
owner-draw palette. The event callback never mutates an `HMENU`.

`Show option descriptions` is an enabled-by-default presentation preference.
The native popup maps stable semantic menu keys to localized hints and uses
documented `WM_MENUSELECT` plus a 500 ms owner-UI-thread delay. The original
`muda` popup theme route was unsuitable because it does not theme tray context
menus. The first tracking-tooltip registration attempts also failed because the
Agent executable did not actually embed its Common Controls v6 manifest. G06R
now embeds that manifest and uses one documented native `TOOLTIPS_CLASS`
tracking tool, owned by the dedicated UI-thread HintHost window, with a stable
numeric tool identity. It preserves stable menu IDs and does not own Agent,
policy, runner, or probe state.

Native hint placement is recalculated for every hovered semantic item. It uses
the displayed item's documented screen rectangle (`GetMenuItemRect`), the
native bubble dimensions (`TTM_GETBUBBLESIZE`), and the nearest monitor's work
area (`MonitorFromRect` and `MONITORINFO.rcWork`). A DPI-scaled gap and a
deterministic Right/Left/Below/Above scorer prefer a fully visible position that
does not overlap the active item; a final clamp keeps normal bubbles inside the
work area, including negative-coordinate secondary displays and taskbar-reduced
work areas. Cursor anchoring is only a monitor-aware fallback when item geometry
is unavailable. The former screenshot harness limitation remains
evidence-tooling only; final popup presentation proof is a direct Owner-session
check.

The Agent executable embeds both Common Controls v6 and the documented DPI
manifest settings: legacy `dpiAware` uses `true/pm`, while modern
`dpiAwareness` selects `PerMonitorV2, PerMonitor`. Owner-drawn menu typography
comes from `SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS)` and its
`lfMenuFont`; native metrics come from the requested DPI or are RunnerMesh
logical values scaled exactly once. On a tray-owner `WM_DPICHANGED`, the UI
thread rebuilds the menu font, metrics, and procedural small-icon resource
without changing Agent policy. The native Common Controls tooltip retains its
own DPI-correct font and recomputes width, bubble size, and placement for its
actual presentation DPI.

## Non-goals

No Windows Service, elevation, installed-runtime activation, production
autostart, real runner start/stop/drain/re-registration, work-root mutation, or
Organization configuration.

## Risk vector

Ordinary code + tray/presentation + sandbox persistent configuration.

## Gates

Deterministic tray/IPC tests; an isolated real Windows Agent/tray/Pipe smoke;
public Windows and Ubuntu hosted CI; exact-head privacy audit.

## Exit

The real Windows tray/runtime layer is proven without claiming any official
runner lifecycle mutation. Next: G10R.
