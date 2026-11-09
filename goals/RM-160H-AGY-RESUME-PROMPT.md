# AGY continue prompt — GT-06 Entry

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-05 RM-160H-T05-ADMISSION-MULTIRUNNER 已 SOURCE_CLOSEOUT=PASS（代码锚 `4788d0c…`）。按 ROADMAP 进入 **GT-06 RM-160H-T06-UX-INSTALL-HUX2**：P05/P06 agent, CLI, Tray, install/rollback; P07 qualification prep；live gate 在未授权事务下保持密封关闭。不要空等 Owner，也不要假 PASS live。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T06-UX-INSTALL-HUX2
PRIOR_TRAIN=RM-160H-T05-ADMISSION-MULTIRUNNER
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=4788d0c80b2da7524f2049747727ca94e1006200
CODE_ANCHOR=4788d0c80b2da7524f2049747727ca94e1006200
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
```

## 硬约束

1. 保持 worktree `runnermesh-160h` + `feat/160h-gt01-field-baseline`；禁 `feat/lrm-09-*`；禁覆盖 productization worktree。
2. 禁 `reset --hard` / `clean -fd` / 强制 checkout / 删其他 worktree。
3. 保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`。
4. 公开文档禁止本机绝对路径、私有 Service/runner home/SID/token（public_audit）。
5. HUX1_ZENBOOKDUO=PENDING；H2/HUX2/TRUSTED_SCM live/G-ADR006/H3/INDEPENDENT_SECURITY_REVIEW 保持 WAITING。
6. **禁止**真 SCM Start/Stop/ChangeConfig、UAC、标签/注册/workflow 写、生产安装、H2、HUX-2/3、发布。合成 grant ≠ Owner 授权。

## GT-06 优先队列（源码/沙箱，自动连续）

1. **P06-01/02**: CLI/Tray 多 runner 呈现与国际化；重用 Owner HUX-0 PASS；乐观意图与达成状态区分。
2. **P06-03/04**: 隐私/脱敏与版本化 Registry 迁移/回滚；禁止从 source worktrees 自启动。
3. **P06-05/06**: 不可变包沙箱安装/更新/回滚/卸载崩溃安全验证。
4. **P07-01/03**: HUX-2 单一可支配 Service Runner 测试包准备（独立安全审查准备，无实机写）。
5. 新建 `goals/RM-160H-T06-CHECKPOINT.md`；有界提交 Draft #65；CI 绿再进下一叶。

开始执行。
