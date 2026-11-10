# RunnerMesh 160H 安装包交付清单

按 [交付路线图](RM-160H-AGY-DELIVERY-ROADMAP.md) 执行。只有证据对应实际候选时才勾选。

## 1. 接手与沙盒候选

- [x] 确认唯一 AGY writer、当前工作树与分支。重查 #65、CI、main 和 G15R，记录本地与远端 SHA。
- [x] 保护其他工作树、分支和未跟踪文件。交接与证据修订使用 LF，做公开内容审计。
- [x] 对现有 R4 可信授权入口和 R6 Agent 执行链做源码核对。对已准入的最小修复记录风险向量，跑受影响测试和对应候选的托管 CI。
- [x] 在任务专属隔离目录用 Windows x64 release 构建和现有 packager 生成沙盒候选。记录源 HEAD、版本、渠道和构建证据。
- [x] 检查包内容，独立复算 ZIP 与 helper SHA-256。运行 helper 的 `identity`、`verify`、`extract`，对照包内 manifest、provenance 和运行时报告。
- [x] 用现有沙盒或隔离的 Windows CI 验证用户级安装、Agent/Tray/CLI/IPC、doctor、版本选择、更新、回滚和按所有权卸载。真实 Startup 和已安装运行时属于单独的 Owner 边界。
- [x] 输出沙盒候选收据和下一扇门的就绪状态。若某门在等待，继续其余已获准且不依赖该门的准备。

## 2. 真实验收与源码接收

| 门 | 通过所需证据 | 记录 |
|---|---|---|
| HUX-0 | #63 Owner 已接受的 Tray 证据。改动触及可见风险时对最终候选复验 | `PASS_OWNER_ACCEPTED`（#63） |
| R1b / HUX-1 | Owner 指定的物理 ZenBookDuo 只读发现、计数和枚举状态 | `PENDING_NOT_RUN` |
| R2 / G15R | 独立审查者结论绑定 G15R 的准确 SHA | `PENDING` |
| R2 / LRM | 独立审查者结论绑定 LRM 候选及改变的信任边界 | `PENDING` |
| R3 / H2 | 新 Owner 授权、精确 G15R 哈希、切换与恢复分别记录、至少 24 小时实际普通使用、G17 收尾 | `WAITING_FOR_OWNER` |
| R4 / HUX-2 | R2、可信执行入口、精确且空闲的一次性 Service、撤回 admission、Stop/Start 独立读回和恢复。已更新 RM-HUX-2-READINESS-PACKAGE.md，验证 TokenValidatingOwnerVerifier、mint_trusted_live 及所有拒绝路径，沙盒与合成测试全通 | `WAITING_FOR_OWNER` |
| R5 / ADR 0006 | Owner 书面决定，R6 路由符合已接受约束。RM-160H-R5-ADR006-OWNER-BRIEF.md 简报已备 | `DRAFT_NOT_ACCEPTED` |
| R6 / HUX-3 | R4 通过、真实多 Runner 执行链、兼容路由和授权。A 停用时 B 可完成合法 CI，重启后意图保留。已编制 RM-HUX-3-READINESS-PACKAGE.md，从常规 Agent 入口完成 A 停用 B 意图与执行不受影响及 ADR 0004 拦截验证，测试全通 | `WAITING_FOR_OWNER` |
| R7 | Owner 接收准确源码和实际目标分支，合并后复核远端与目标 CI | `OPEN_DRAFT` |
| HUX-4 / R8 | 最终 LRM 字节经批准在指定环境完成安装、Startup、重启、升级、回滚和至少 24 小时普通使用。公开发布另取该轨授权 | `WAITING_FOR_OWNER` |

## 3. 最终交付

- [ ] 接受的 LRM HEAD 和构建来源已核对，证据复用条件已确认。
- [ ] 最终 Windows x64 ZIP、同源 helper、两份 SHA-256 和 provenance 可由 Owner 取得并独立校验。
- [ ] 最终包摘要与适用验收收据一致。安装、启动、升级、回滚和卸载说明已核对，凭据和私有身份另由 Owner 管理。
- [ ] 给出安装包位置、完整 SHA-256、版本、候选 HEAD、构建记录及各项处置。
- [ ] 更新 Progress、Remaining Checklist 和 Phase-R Checkpoint，发布状态单独记录。

每门完成后更新记录，再等待下一扇受控门。活体事务记录 `QUALIFICATION` 与 `RESTORE`。缺证据时沿用 `BLOCKED` 或 `UNPROVEN`，并写明解除条件。
