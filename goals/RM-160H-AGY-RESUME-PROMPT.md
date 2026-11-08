# AGY resume prompt (paste as first user message)

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。Owner 已从 Cursor 切换到 AGY 作为唯一 Implementer。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T02-V01-CLOSEOUT-PREP
PRIOR_TRAIN=RM-160H-T01-FIELD-BASELINE
ROOT=C:\Dev\src\runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=5271d5b3d4705c2d9b0be58716ce52d273e37db7
LAST_CODE_PASS_ALL=90740eef1250a2ce0ec5aba39b9738ba0ad15dcd
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
```

## 硬约束

1. **保持** `C:\Dev\src\runnermesh-160h` + `feat/160h-gt01-field-baseline`；禁止切到 `feat/lrm-09-*`，禁止覆盖 productization worktree。
2. 禁止 `reset --hard`、`clean -fd`、强制 checkout、删除其他 worktree。
3. 先读交接：`goals/RM-160H-CURSOR-TO-AGY-HANDOFF.md`，以及 T01/T02 Checkpoint、gap map、P01 H2/G17、P04 selector prep。
4. `goals/RM-160H-DELIVERY-PROGRAM-001/` 未跟踪且应等于 main；无差异不要丢弃。
5. HUX-0=PASS_OWNER_ACCEPTED；HUX1 开发机=EMPTY_OK；**HUX1_ZENBOOKDUO=PENDING**（不可用开发机结果冒充）。
6. 不执行真 SCM 启停、UAC、标签/注册、生产安装、H2、HUX-2/3、发布。

## 立即动作

1. 只读核对：`git status`、远端 tip、`gh pr checks 65`；确认 `5271d5b` CI；代码证据以 `90740ee` PASS_ALL 为锚。
2. 若 git HTTPS 推送失败：用 GitHub Git Data API；LF；拒空 blob；去掉文件尾多余空行（public_audit）。
3. 从 GT-02 安全叶继续：P01 Owner packet 公开字段打磨；或剩余不依赖 Owner 的 P03/P04/P06 源码。
4. 每 60–90 分钟写可恢复 Checkpoint；遇 Owner Gate 停并写清 WAITING_FOR_OWNER。
5. 不要向 Owner 提供无必要路径选择题；非关键问题取最小合理默认继续。

## 已完成（勿重复大段重做）

P02 身份发现；P04 FakeGithub 全矩阵；P05 优先级/A-B/重启拒绝/stale PID；P06 部分失败与隐私；P03 synthetic≠live + stop/start Interrupted；G15R 只读复核；HUX-1 `operator-card-hux1`；Draft PR #65。

开始：先读 handoff 与 Checkpoint，核对 tip/CI，再执行下一项依赖已满足的安全 Goal。
