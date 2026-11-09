# AGY continue prompt — GT-03 leaf 4 complete / closeout

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-03 leaf 4 已完成：代码锚 `4179596…`。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T03-IDENTITY-SESSION
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=9bab37d6e4c45b7ee23c683b54fe3c80eb80fb05
CODE_ANCHOR=4179596bec1345296e35eff01681ccd761a8907e
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
```

## 硬约束

1. 保持 worktree `runnermesh-160h` + `feat/160h-gt01-field-baseline`；禁 `feat/lrm-09-*`；禁覆盖 productization worktree。
2. 禁 `reset --hard` / `clean -fd` / 强制 checkout / 删其他 worktree。
3. 保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`。
4. 公开文档禁止本机绝对路径、私有 Service/runner home/SID/token（public_audit）。
5. HUX1_ZENBOOKDUO=PENDING；H2/HUX2/TRUSTED_SCM/G-ADR006/H3/INDEPENDENT_SECURITY_REVIEW 保持 WAITING，勿假 PASS。
6. 无真 SCM 写、UAC、标签/注册、生产安装、H2、HUX-2/3、发布。

## 立即动作

1. 核对 PR #65 CI 绿。
2. 读 `goals/RM-160H-T03-CHECKPOINT.md` 与 CHECKLIST。
3. 等待 Owner 决策与 H2/HUX-2 授权，或按需执行下一项独立安全叶。
