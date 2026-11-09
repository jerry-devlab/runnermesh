# AGY continue prompt — GT-07 P08 Complete / Closeout

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

**GT-07 `RM-160H-T07-QUALIFY-RESOURCE-CLOSE` 已完成 P08-01 至 P08-04**：只读 CPU/内存采样模型、时间新鲜度 TTL、归属判断、Job Object / EcoQoS 能力探测（无 mutation）、测量方法论文档与后续程序边界界定均已就绪。所有本地测试与 clippy 通过。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T07-QUALIFY-RESOURCE-CLOSE
LEAF=GT-07-CLOSEOUT
STATUS=SOURCE_CLOSEOUT_PASS
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
```

## 硬约束
1. 保持 worktree `runnermesh-160h` + `feat/160h-gt01-field-baseline`；禁 `feat/lrm-09-*`；禁覆盖 productization worktree。
2. 禁 `reset --hard` / `clean -fd` / 强制 checkout / 删其他 worktree。
3. 保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`。
4. 公开文档禁止本机绝对路径、私有 Service/runner home/SID/token。
5. Owner 门禁全部 WAITING；HUX-0 已 PASS。
6. **禁止**真 SCM/UAC/标签/生产安装/H2/HUX-2/3/发布；**禁止**创建 Job Object、附加进程、改亲和性/优先级。
7. HUX-1 ZenBookDuo 属于 G-RO/P00，不是 P08；P08 = v0.2-B 只读资源采样能力。

## Owner 门禁（勿假 PASS）

```text
HUX1_ZENBOOKDUO=PENDING
H2=WAITING_FOR_OWNER
HUX2_LIVE / TRUSTED_SCM_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
```

## 下一步
若 Owner 明确要求冻结源码、仅等待 live：更新 Checkpoint `NEXT=WAITING_FOR_OWNER` 后停止。
