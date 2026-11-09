# AGY continue prompt — GT-05 Entry

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-04 RM-160H-T04-TRUSTED-SCM 已 SOURCE_CLOSEOUT=PASS（代码锚 `5730516…`）。按 ROADMAP 进入 **GT-05 RM-160H-T05-ADMISSION-MULTIRUNNER**：P04 选择器与路由迁移、P05 多 runner 策略与部分失败集成；live gate 在未授权事务下保持密封关闭。不要空等 Owner，也不要假 PASS live。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T05-ADMISSION-MULTIRUNNER
PRIOR_TRAIN=RM-160H-T04-TRUSTED-SCM
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=57305160c5340ae1d7d90be976ffed912cfe08a7
CODE_ANCHOR=57305160c5340ae1d7d90be976ffed912cfe08a7
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

## GT-05 优先队列（源码/沙箱，自动连续）

1. **P05-01/02**: 单 Agent 权威管理多 local Runners；硬安全/Zen/手动禁用/ForceCI/Auto 优先级收敛与持久化意图。
2. **P05-03/04**: 多实例端到端 observe→decide→reconcile；A/B 独立性（禁用 A 时 B 的 Worker/service/selector/registry 严格保持不变）。
3. **P05-05/06**: 重启、Agent 崩溃、stale PID、配置损坏、回读不确定与部分失败矩阵；禁止杀 Worker、禁止无界重试、禁止自动重新启用。
4. 新建 `goals/RM-160H-T05-CHECKPOINT.md`；有界提交 Draft #65；CI 绿再进下一叶。

开始执行。
