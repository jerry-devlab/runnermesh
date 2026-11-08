# AGY continue prompt (paste as first user message)

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

上一轮 GT-02 closeout-prep 公开包打磨已 PASS（tip `0c95e417…`，Hosted CI `42496421737` PASS_ALL）。**不要停在 WAITING_FOR_OWNER 空转**；按 ROADMAP §6.7：H2/HUX-2 缺 Owner 时，继续已授权的 P02–P06/P08 源码与沙箱，并进入 **GT-03** 中不依赖 Owner Gate 的工作。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T03-IDENTITY-SESSION
PRIOR_TRAIN=RM-160H-T02-V01-CLOSEOUT-PREP
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=0c95e417255ea0a094ff3aff7b4d46cfef0dd331
CODE_ANCHOR=90740eef1250a2ce0ec5aba39b9738ba0ad15dcd
LAST_PASS_ALL=0c95e417255ea0a094ff3aff7b4d46cfef0dd331
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
```

## 硬约束

1. 保持 worktree `runnermesh-160h` + 分支 `feat/160h-gt01-field-baseline`；禁止 `feat/lrm-09-*`；禁止覆盖 productization worktree。
2. 禁止 `reset --hard`、`clean -fd`、强制 checkout、删除其他 worktree。
3. 保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`（应等于 main）。
4. 公开文档/receipt **不得**写入本机绝对路径、私有 Service 名、runner home、SID、token（public_audit 会 FAIL）。
5. HUX0=PASS_OWNER_ACCEPTED；HUX1 开发机=EMPTY_OK；**HUX1_ZENBOOKDUO=PENDING**（不可冒充）。
6. 不执行真 SCM 启停/ChangeConfig、UAC、标签/注册、生产安装、H2、HUX-2/3、发布。

## Owner 门禁（保持 WAITING，勿假 PASS）

```text
HUX1_ZENBOOKDUO=PENDING
H2=WAITING_FOR_OWNER
HUX2_LIVE / TRUSTED_SCM=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
```

## 本轮目标（GT-03 源码叶，自动连续）

优先做依赖已满足、不需 Owner 的叶 Goal；每完成一叶：验收 → 证据/Checkpoint → 选下一叶 → 直接执行。

1. **P02 残余**：owned User Session 合成后端与多实例所有权测试加强；Busy/Unknown/foreign/stale-PID 矩阵补洞；不触真实进程启停。
2. **P03 源码**：SCM 语义/journal/Interrupted≠RESTORE；`UnissuedOwnerAuthorizationVerifier` / synthetic grant 永不打开 live Windows 写；GateClosed 路径保持。
3. **P04 源码（无 ADR 采纳）**：保留 `runnermesh-admit` v0.1；same-scope dual managed 继续 REFUSE；FakeGithub/scoped_admission_sim 仅仿真；**不得**自称 ADR 0006 已接受。
4. **P05/P06 残余**：多 runner 控制器/部分失败/隐私/安装沙箱补强；不增生产 mutation。
5. 若同类确定性失败修 3 轮仍失败：锁 blocker，转向独立安全叶。
6. 每 60–90 分钟更新 `goals/RM-160H-T03-CHECKPOINT.md`（新建）与 T02 状态；有界提交 + Draft #65；CI 绿后再进下一叶。

## 立即动作

1. 只读：`git status`、远端 tip、`gh pr view 65` / checks；确认 `0c95e417` PASS_ALL。
2. 读：`goals/RM-160H-T02-CHECKPOINT.md`、`goals/RM-160H-T01-CANDIDATE-GAP-MAP.md`、`goals/RM-160H-DELIVERY-PROGRAM-001/CHECKLIST.md`、ROADMAP。
3. 开写 `goals/RM-160H-T03-CHECKPOINT.md`，声明 `ACTIVE_TRAIN=RM-160H-T03-IDENTITY-SESSION`。
4. 从 CHECKLIST 中选下一项 **未勾选且不依赖 Owner** 的 P02/P03 叶，开始实现+测试+推送。
5. 推送：优先 git；失败则 Git Data API；LF；拒空 blob；去尾空行。

## 勿重复大段重做

GT-01 源码包、FakeGithub 全矩阵、P05 优先级/A-B/重启拒绝、P06 部分失败与隐私、G15R 只读复核、H2/G17/P04 公开 Owner 包、路径脱敏 public_audit 修复。

开始：核对 tip/CI → 写 T03 Checkpoint → 执行第一项可做的 P02/P03 安全源码叶。
