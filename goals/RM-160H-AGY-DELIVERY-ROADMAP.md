# RunnerMesh 160H 安装包交付路线图

AGY 是接手后的唯一 Implementer。Owner 启动 AGY 后，它负责从源码和沙盒准备持续推进到安装包交付。本文替代 D24 的启动与排队顺序，沿用 [RunnerMesh 治理](../AGENTS.md) 和既有 [Phase-R 门禁](RM-160H-REMAINING-CHECKLIST.md)。

## 交付目标

160H 主交付按 **v0.2-A LRM Windows x64 可安装包**规划。用现有 ZIP 和同源 `runnermesh-package.exe` 交付，附两者的 SHA-256、构建来源、安装与回滚说明和验收收据。公开发布是单独的 Owner 动作。

v0.1 C0 保持独立。唯一 H2 产物是 G15R RC `fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab`，对应 [托管构建 42452639359](https://github.com/jerry-devlab/runnermesh/actions/runs/42452639359)。由该轨的 R2 独立审查、新 R3 授权、H2 切换与恢复证据、至少 24 小时普通使用和 G17 顺序推进。H3 发布另取授权。

## 接手坐标

| 坐标 | 已核实值 |
|---|---|
| 工作树与分支 | `runnermesh-160h` / `feat/160h-gt01-field-baseline` |
| 本地交接提交 | `c19acb34ba9b7713e48477938af43331e9fdb4a5` |
| 远端 Draft #65 与 CI | `26775d04dd2a1db3ad80295eb48dc095033481aa` / [42765028290，6/6 PASS](https://github.com/jerry-devlab/runnermesh/actions/runs/42765028290) |
| 代码锚点 | `b221eaf72336c5caac676b10405c06e8cdd90b06` |
| #65 当前 base | `feat/lrm-productization` / `8cd996acfee23e5f2006e7801139c70d0d67551d` |
| 接手时的远端 main | `d2b37444342d085e650f996e5fb4322f775bbe6f` |

GT-01 至 GT-07 源码与沙盒已收尾，D24 材料已备。R2 独立结论、目标机 HUX-1、真实 H2/HUX-2/HUX-3 和源码接收仍待证据。启动时重查远端与 CI，以实际 Git 和 GitHub 证据为准。

## 执行顺序

1. **接手并把候选做实。** 保留本地交接提交和未跟踪文件。读取 main 的 v0.1 执行台账，校正实际要交给审查者的 SHA、CI 和源码路径。核实两项既有验收缺口，R4 的可信 SCM 授权入口与 R6 的常规 Agent 多 Runner 执行链。仅在已准入的 160H 源码范围内做证据支持的最小修复与沙盒验证。需要新增信任权力或跨出该范围时，带具体阻塞项停在边界。
2. **准备两个可复核的候选证据包。** v0.1 复用现有 G15R 字节。LRM 在已核实的提交上用隔离目录构建 Windows x64 沙盒包，验证 ZIP、helper、摘要、来源和安装链。先完成所有可安全推进的步骤。R1b 仅按 Owner 指定在物理 ZenBookDuo 上采集，开发机的 `EMPTY_OK` 单独保留。
3. **执行现有真实验收门。** R2 按轨绑定准确 SHA，由真正独立的审查者判定。LRM 先 R4 单台一次性 Service，再按 R5 兼容路由执行 R6 双 Runner 隔离。HUX-0 复用 #63 Owner 证据，只有验收需要 Windows 桌面时才安排 Computer Use。每个真实事务都有独立读回、前态、停止条件与恢复收据。一次只执行 Owner 明确指定的一扇门。
4. **接收源码并冻结 LRM 安装包。** R7 按 Owner 明确批准的目标和路径接收源码。合并 #65 到其当前 base 时，验证该目标及其 CI。若实际进入 `main`，另核对远端 `main` 与该次合并的 CI。对最终接受的 LRM HEAD 使用获准的 Windows 打包路径，绑定版本、渠道、提交和摘要。现有 [v0.1 RC 工作流](../.github/workflows/rc-candidate.yml) 只接收 `main` 且固定 `0.1.0`，LRM 路径需单独验证和接收。已有打包器优先复用，版本或打包配置的必要变更走相应源码和发布门禁。
5. **用最终字节验收并交付。** 在适用的 HUX-4 授权后，用同一个不可变包验证安装、启动、重启、升级、回滚及普通使用。将已验收的 ZIP、同源 helper、两份 SHA-256、provenance 和收据交付到 Owner 指定位置。等待 Owner 选定的门未通过时，仍可交付已验证的沙盒候选和准确阻塞清单，交付状态保留为待验收。

源码、构建目录、发布产物、安装目录和激活版本各自隔离。运行时验收复用已通过且风险差异为空的证据。风险变化则在新 HEAD 或新包摘要上补对应验收。进度以 [交付清单](RM-160H-AGY-DELIVERY-CHECKLIST.md) 和 [Checkpoint](RM-160H-PHASE-R-CHECKPOINT.md) 记录。启动入口在 [AGY 提示词](RM-160H-AGY-DELIVERY-PROMPT.md)。
