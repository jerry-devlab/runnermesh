# RunnerMesh

**Put spare Windows workstation capacity to work on GitHub Actions while keeping the person at the keyboard in control.**

English · [简体中文](README.zh-CN.md)

[Download the Windows pre-release](https://github.com/jerry-devlab/runnermesh/releases/tag/v0.2.0-rc.2) · [Quick start](#quick-start) · [Project roadmap](docs/roadmap.md) · [MIT License](LICENSE)

RunnerMesh manages when an official GitHub Actions self-hosted runner can accept work. The Windows Agent owns the policy, and the Tray and CLI read status and send requests through a local Named Pipe. GitHub Actions continues to schedule jobs and store their logs and artifacts.

## What you can use today

| Track | Availability |
| --- | --- |
| Local Runner Manager (LRM) | [v0.2.0-rc.2](https://github.com/jerry-devlab/runnermesh/releases/tag/v0.2.0-rc.2) is a Windows x64 pre-release with a ZIP, a same-build package helper, and SHA-256 checksums. It supports local management of multiple registered runners with independently scoped control. |
| v0.1 single-workstation baseline | The release candidate is in field qualification. Stable publication follows the authorized cutover, ordinary-use observation, and release closeout. See the [v0.1 release operations](docs/v0.1-release-operations.md). |

## Quick start

Use a Windows x64 workstation and its intended interactive user account. For CI workloads, use the official GitHub Actions self-hosted runner and make PowerShell 7 (`pwsh`) available to its execution identity. The instructions below are for a first LRM pre-release install.

1. Download `runnermesh-0.2.0-rc.2-windows-x64.zip`, `runnermesh-package.exe`, and `SHA256SUMS` from the same [pre-release page](https://github.com/jerry-devlab/runnermesh/releases/tag/v0.2.0-rc.2).
2. Open PowerShell in the download directory. Verify both files before running the helper, then check the archive and its provenance.

```powershell
$zipName = 'runnermesh-0.2.0-rc.2-windows-x64.zip'
$helperName = 'runnermesh-package.exe'
$sums = Get-Content -LiteralPath '.\SHA256SUMS' -ErrorAction Stop
$hashes = @{}
foreach ($name in @($zipName, $helperName)) {
    $pattern = '^[0-9a-f]{64}\s+\*?' + [regex]::Escape($name) + '$'
    $entries = @($sums | Where-Object { $_ -match $pattern })
    if ($entries.Count -ne 1) { throw "Missing or ambiguous checksum for $name" }
    $hashes[$name] = ($entries[0] -split '\s+', 2)[0]
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $name -ErrorAction Stop).Hash
    if ($actual -ine $hashes[$name]) { throw "SHA-256 mismatch for $name" }
}
$archive = (Resolve-Path -LiteralPath $zipName).Path
$helper = (Resolve-Path -LiteralPath $helperName).Path
& $helper verify $archive
if ($LASTEXITCODE -ne 0) { throw 'Package verification failed' }
```

3. In the same session, install for the current user, enable login startup, and open the installed Agent for the first run. For an existing installation, review the [activation and recovery model](docs/windows-workstation.md#autostart-and-recovery) first.

```powershell
$root = Join-Path $env:LOCALAPPDATA 'RunnerMesh'
$stage = Join-Path $env:TEMP ('runnermesh-stage-' + [guid]::NewGuid().ToString('N'))
& $helper install $archive $hashes[$zipName] $stage $root
if ($LASTEXITCODE -ne 0) { throw 'Installation failed' }

& $helper autostart enable $root
if ($LASTEXITCODE -ne 0) { throw 'Autostart setup failed' }
Start-Process -FilePath (Join-Path $root 'bin\runnermesh-agent.exe')
```

4. Once the Tray appears, confirm the active build using the installed CLI. You should see version `0.2.0-rc.2` and commit `2593e25c8c9c624eade5a70324d79d0d47f6f1b2`.

```powershell
$cli = Join-Path $root 'versions\0.2.0-rc.2\runnermesh.exe'
& $cli status --json
```

Installation starts the Agent and Tray. Runner control requires a separately configured, authorized runner. Use the [Windows workstation guide](docs/windows-workstation.md) and [GitHub integration guide](docs/github-integration.md) for the operating model. For removal, request `stop-after-drain` through the verified package helper, wait for the Agent to exit, and use the helper's `uninstall <install-root>` command to remove the owned installation.

## How RunnerMesh works

- **Human first.** Foreground work has priority. Manual policy overrides automatic sensing, and Zen provides a human-exclusive override. When safety evidence is uncertain, admission refuses new CI work.
- **Graceful drain.** RunnerMesh withdraws new capacity while active work finishes. `DRAINED` requires the selector to be absent and no exact bound Worker to be active. Constrained-state vocabulary such as `THROTTLED` is reserved for later resource enforcement.
- **GitHub Actions native.** Jobs use ordinary workflows. GitHub owns queueing, assignment, the official runner protocol, logs, checks, and artifacts.
- **Local, scoped control.** One Agent controls each user profile, and each execution identity owns its active work root. The official runner connects outbound. Persistent workstations are for explicitly trusted workloads, while public or untrusted contributions can use GitHub-hosted CI.

Jobs intended for RunnerMesh-managed capacity use the canonical reserved selector in addition to normal self-hosted platform labels.

```yaml
runs-on:
  - self-hosted
  - Windows
  - X64
  - runnermesh-admit
```

RunnerMesh adds or removes `runnermesh-admit` only on its exact bound runner. Admission withdrawal applies to jobs requesting that selector. See the [architecture](docs/architecture.md), [GitHub integration](docs/github-integration.md), and [threat model](docs/threat-model.md).

## Development and roadmap

The public [product roadmap](docs/roadmap.md) describes the scope of each version. The [v0.1 execution ledger](goals/RM-V0_1-EXECUTION-STATUS.md) records the stable baseline track.

```powershell
cargo check
cargo test
```

GitHub-hosted CI validates public contributions. See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow.

## License

RunnerMesh is available under the [MIT License](LICENSE).
