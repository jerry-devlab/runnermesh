# AGY continue prompt — GT-05 Leaf 2

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-05 leaf 1 已完成：代码锚 `3020b0a…`。**立即进入 GT-05-LEAF-02**，不要空等 Owner。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T05-ADMISSION-MULTIRUNNER
LEAF=GT-05-LEAF-02
PRIOR_TRAIN=RM-160H-T04-TRUSTED-SCM
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=3020b0a62281182ccb375d0acc4dca5167027c33
CODE_ANCHOR=3020b0a62281182ccb375d0acc4dca5167027c33
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

## GT-05 LEAF-02 优先队列（源码/沙箱，自动连续）

1. **P05-05/06 弹性与失败矩阵测试**：
   - 重启与 Agent 崩溃恢复后的状态与意图重建；
   - 陈旧 PID / 不可访问进程 / PID 复用防拼接验证；
   - 远程选择器回读不确定（Unknown/None）与网络断开时保持 fail-closed；
   - 批量 Runner 命令的部分失败（部分成功部分失败不冒充全量成功）；
   - 证明无任何破坏性终止 Worker、无无界重试循环、无自动重新启用已禁用 runner。
2. 更新 `goals/RM-160H-T05-CHECKPOINT.md`；有界提交 Draft #65；CI 绿再进下一叶。

开始执行。
