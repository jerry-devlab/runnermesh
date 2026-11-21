# AGY 启动与执行提示词

从 `runnermesh-160h` 工作树的根目录在普通权限 PowerShell 启动。AGY CLI 1.5.3 的启动参数已通过本机 `agy --help` 核对。

```powershell
agy --sandbox --dangerously-skip-permissions --prompt-interactive '读取 goals/RM-160H-AGY-DELIVERY-PROMPT.md 的执行提示并开始接手。'
```

此参数自动通过 AGY 的工具确认，`--sandbox` 在可用时启用终端沙盒。该会话限源码、沙盒和证据准备。真实 Owner 事务进入单独的受控会话。参见 [Antigravity CLI 参数](https://antigravity.google/docs/cli/reference/#command-line-flags)。

## 执行提示

你是 RunnerMesh 160H 的唯一 AGY Implementer。Owner 将项目交由你推进至可安装的 Windows x64 LRM 包交付。现在开始安全范围内的工作，无需为每个普通源码或沙盒步骤等待指令。

1. 先读 `AGENTS.md`、远端 main 的 `goals/RM-V0_1-EXECUTION-STATUS.md`、[交付路线图](RM-160H-AGY-DELIVERY-ROADMAP.md)、[交付清单](RM-160H-AGY-DELIVERY-CHECKLIST.md) 和 [Phase-R Checkpoint](RM-160H-PHASE-R-CHECKPOINT.md)。执行 `git status`，只 fetch 所需的 160H 与 main 引用，核对本地交接、Draft #65、CI 和 G15R。若分支或远端候选有意外漂移，保留现场并先核对其来源。
2. 按路线图顺序校正证据、核实 R4/R6 已知缺口、做已准入的最小源码与沙盒工作。优先复用现有打包与测试入口，准备候选 ZIP、同源 helper、摘要和可复核的验收证据。若可信执行所需的权限或设计超出已准入边界，明确记录阻塞，继续独立的安全准备。等待真正独立的 R2 结论。AGY 自审记录为源码诊断。
3. Git 写入只在 `feat/160h-gt01-field-baseline`，且仅纳入本次所有权明确的改动。只向既有 Draft #65 提交范围内的候选，核对目标和所需审批后再进行远端写入。禁止 `reset --hard`、`clean -fd`、强推、强制检出或删除其他工作树。保护 `feat/lrm-09-*`、productization 和未跟踪的 `goals/RM-160H-DELIVERY-PROGRAM-001/`。公开材料使用 LF，私有路径、身份、凭据和原始私有收据留在授权位置。
4. 保持 `PRODUCTION_MUTATION=false`。YOLO 只用于本轮工具确认。SCM、UAC、注册、GitHub 标签或组、生产安装与 Startup、H2、HUX-2/3、真实 HUX-4、合并及发布均沿各自 Owner 门执行。H2 只用路线图绑定的 G15R RC。每次取得明确授权后，在单独受控会话只执行该门并核对恢复。遇到授权缺失、Busy、身份或证据漂移时先停该事务，记录处置。
5. 已关闭的 P02–P08 不增设功能叶，范围限现有验收阻塞与安装包交付。Mesh、Linux、Broker 和 Job Object enforcement 不在本次范围。确需桌面交互时，准备一个具体的 Computer Use 验收步骤和预期证据，保留 AGY 的唯一写入权。
6. 按清单持续推进。每个门记录候选、证据、判定、恢复和下一步，更新 Progress、Checklist、Checkpoint。安全工作做完后，在需要 Owner 的门上交付可审阅的就绪包与一条明确下一步。最终交付提供可取得的包、两份 SHA-256、来源、安装与回滚说明及收据。
