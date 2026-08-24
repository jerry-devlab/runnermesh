# Windows Workstation Mode

The first product path is GitHub Actions on Windows interactive workstations using the official GitHub self-hosted runner. v0.1 is a single-workstation, first-usable admission/lifecycle controller.

See [`v0.1-design-freeze.md`](v0.1-design-freeze.md) for the complete accepted contract.

## Default execution model

Ordinary Workstation Mode runs in the intended interactive user session:

```text
ordinary user session -> RunnerMesh Agent -> official GitHub runner
```

It does not require `NETWORK SERVICE` or another service identity. Service/headless execution is a distinct optional future backend.

## Agent, Tray, CLI, and IPC

The ordinary user-session Agent is persistent and non-elevated. Windows Tray is the daily UI; CLI is the scripting/diagnostic UI. Both use typed Agent contracts over local Named Pipe IPC. The Agent is the only runner/policy authority.

Only one controlling Agent may exist per user profile.

## PowerShell contract

The Windows baseline is PowerShell 7. Installation method is intentionally not prescribed: a supported `pwsh` may come from a user-scoped packaged/MSIX installation or another supported installation model. The requirement is that `pwsh` is resolvable and functional from the selected execution identity.

## Work-root ownership

One execution identity owns one active work root. The same identity may reuse that root. Different identities must not share an active root, and ownership conflicts must not be papered over by globally changing Git `safe.directory`.

## v0.1 observation and Auto Lite

Windows observation includes local runner lifecycle, CPU/memory, user idle/session state, and probe evidence.

v0.1 Activity Probes:

- User Activity Probe;
- Steam Game Probe, detecting a running Steam App rather than Steam client presence;
- configurable Process List Probe.

The first Steam backend may use the current user's `HKCU\Software\Valve\Steam\RunningAppID` as local evidence, behind the probe abstraction. It is not a public Valve compatibility contract.

Auto Lite is conservative: activity or unknown safety evidence drains new admission; `FULL` requires idle/away evidence and all relevant probes permitting contribution.

## Zen and manual control

Zen is a persistent human-exclusive override layered above the selected mode. It immediately denies new admission, gracefully drains active work, stops contribution afterward, suspends nonessential probes without disabling their configuration, and leaves the minimal Agent/Tray/IPC shell available for Resume.

## Tray preferences

v0.1 supports system/light/dark theme preferences and system/Simplified-Chinese/English language preferences. Localization affects presentation only, never serialized machine contracts.

## Autostart and recovery

v0.1 supports user-session start on login. Autostart targets an installed stable activation entry, never a source tree. Agent startup reconstructs transient state and may adopt an existing listener only after ownership/identity checks.

## Networking

The official runner initiates outbound connectivity. Ordinary workstation operation does not require inbound public access, port forwarding, DDNS, or VPN merely for GitHub Actions. Future RunnerMesh control sessions are also intended to be outbound-initiated.

v0.1 reports typed GitHub Actions connection evidence; a process existing is not proof of `Connected`.

## Installation boundary

Production runtime is isolated from source development:

```text
SOURCE != BUILD != RELEASE != INSTALLED RUNTIME != ACTIVE VERSION
```

User-level immutable version slots, explicit activation metadata, durable update receipts, and rollback protect a deployed stable runtime while source development continues.
