# AGY continue prompt — GT-06 Complete / Awaiting Owner Gates

Copy everything below the line into AGY CLI.

---

RunnerMesh 160H Goal Train: 全部源码与沙箱 Goal（GT-01 至 GT-06，P02–P07）已全部 SOURCE_CLOSEOUT=PASS。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T06-UX-INSTALL-HUX2
STATUS=SOURCE_CLOSEOUT_PASS
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
CANDIDATE_HEAD=2ea0b59995862b0804b612c86d395e01821c5a38
CODE_ANCHOR=0dd4afeb32d15c0a7a70043d28339ead4572ddce
HOSTED_CI_RUN=42607694331
CI_STATUS=PASS_ALL
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
5. HUX1_ZENBOOKDUO=PENDING；H2/HUX2_LIVE/TRUSTED_SCM_LIVE/G-ADR006/H3/INDEPENDENT_SECURITY_REVIEW 保持 WAITING。
6. **禁止**真 SCM 写、UAC、标签/注册/workflow 写、生产 Startup 指向 source、H2 cutover、HUX-2/3 实机、发布。合成 ≠ Owner 授权。

## Owner 门禁（就绪等待中，勿假 PASS）

```text
HUX1_ZENBOOKDUO=PENDING
H2=WAITING_FOR_OWNER
HUX2_LIVE / TRUSTED_SCM_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
```

## 下一步
等待 Owner 决策并开展真实环境授权验证（如 ZenBookDuo 只读发现或具备全新 token 后的 H2/HUX-2 预备事务）。
