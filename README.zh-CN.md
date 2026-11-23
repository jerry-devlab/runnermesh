# RunnerMesh

**把 Windows 工作站的空闲算力交给 GitHub Actions，同时让正在使用电脑的人始终掌握控制权。**

[English](README.md) · 简体中文

[下载 Windows 预发布版](https://github.com/jerry-devlab/runnermesh/releases/tag/v0.2.0-rc.2) · [快速开始](#快速开始) · [产品路线图](docs/roadmap.md) · [MIT 许可证](LICENSE)

RunnerMesh 管理官方 GitHub Actions 自托管 Runner 何时可以接收任务。Windows Agent 负责策略，系统托盘和 CLI 通过本地命名管道读取状态、发送请求。GitHub Actions 继续负责调度任务、保存日志和产物。

## 当前可用版本

| 轨道 | 可用状态 |
| --- | --- |
| Local Runner Manager（LRM） | [v0.2.0-rc.2](https://github.com/jerry-devlab/runnermesh/releases/tag/v0.2.0-rc.2) 是 Windows x64 预发布版，提供 ZIP、同源安装助手及 SHA-256 校验清单。可以管理多个已登记的本地 Runner，各实例按独立作用域控制。 |
| v0.1 单工作站基线 | 候选包仍在现场验收阶段。稳定版将在授权切换、日常使用观察与发布收口后交付。参见 [v0.1 发布规程](docs/v0.1-release-operations.md)。 |

## 快速开始

请在 Windows x64 工作站上使用准备运行 Agent 的交互式用户账户。运行 CI 任务还需要官方 GitHub Actions 自托管 Runner，并确保其执行账户可以运行 PowerShell 7（`pwsh`）。以下步骤适用于首次安装 LRM 预发布版。

1. 从同一个[预发布页面](https://github.com/jerry-devlab/runnermesh/releases/tag/v0.2.0-rc.2)下载 `runnermesh-0.2.0-rc.2-windows-x64.zip`、`runnermesh-package.exe` 和 `SHA256SUMS`。
2. 在下载目录打开 PowerShell。运行安装助手前先校验两个文件，再验证压缩包及其构建来源。

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

3. 在同一个 PowerShell 会话中为当前用户安装、开启登录自启，并首次启动已安装的 Agent。如果已有安装，请先查看[激活与恢复说明](docs/windows-workstation.md#autostart-and-recovery)。

```powershell
$root = Join-Path $env:LOCALAPPDATA 'RunnerMesh'
$stage = Join-Path $env:TEMP ('runnermesh-stage-' + [guid]::NewGuid().ToString('N'))
& $helper install $archive $hashes[$zipName] $stage $root
if ($LASTEXITCODE -ne 0) { throw 'Installation failed' }

& $helper autostart enable $root
if ($LASTEXITCODE -ne 0) { throw 'Autostart setup failed' }
Start-Process -FilePath (Join-Path $root 'bin\runnermesh-agent.exe')
```

4. 托盘图标出现后，用已安装的 CLI 核对运行版本。预期版本为 `0.2.0-rc.2`，构建提交为 `2593e25c8c9c624eade5a70324d79d0d47f6f1b2`。

```powershell
$cli = Join-Path $root 'versions\0.2.0-rc.2\runnermesh.exe'
& $cli status --json
```

安装完成后会启动 Agent 和托盘。只有另外配置并授权 Runner 后，Agent 才能控制该实例。有关运行方式，参见 [Windows 工作站指南](docs/windows-workstation.md) 和 [GitHub 集成指南](docs/github-integration.md)。卸载时，通过已校验的安装助手请求 `stop-after-drain`，等待 Agent 退出，再运行助手的 `uninstall <install-root>` 移除其拥有的安装。

## RunnerMesh 如何工作

- **使用者优先。** 前台工作优先。手动策略优先于自动感知，Zen 可让工作站专供当前用户。安全证据不确定时暂停接纳新的 CI 任务。
- **排空后退出。** 撤回新任务容量时让正在运行的任务自然完成。只有选择器已撤回且没有相应的活动 Worker 时，才报告 `DRAINED`。`THROTTLED` 等受限状态留给后续资源控制。
- **沿用 GitHub Actions。** 工作流继续用原生写法。队列、任务分配、官方 Runner 协议、日志、检查结果和产物均由 GitHub 管理。
- **本地且限定权限。** 每个用户配置由一个 Agent 控制，每个执行身份独立拥有自己的活动工作目录。官方 Runner 主动向外连接。常驻工作站用于明确受信任的任务，公开或不受信任的贡献可使用 GitHub 托管 CI。

要使用 RunnerMesh 管理的算力，请在常规自托管平台标签之外加上保留选择器。

```yaml
runs-on:
  - self-hosted
  - Windows
  - X64
  - runnermesh-admit
```

RunnerMesh 只在已绑定的确切 Runner 上添加或移除 `runnermesh-admit`。撤回准入适用于请求该选择器的任务。参见[架构说明](docs/architecture.md)、[GitHub 集成指南](docs/github-integration.md)和[威胁模型](docs/threat-model.md)。

## 开发与路线图

[产品路线图](docs/roadmap.md)说明各版本的范围。[v0.1 执行台账](goals/RM-V0_1-EXECUTION-STATUS.md)记录稳定版基线的进展。

```powershell
cargo check
cargo test
```

公开贡献通过 GitHub 托管 CI 验证。开发流程见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

RunnerMesh 以 [MIT 许可证](LICENSE)发布。
