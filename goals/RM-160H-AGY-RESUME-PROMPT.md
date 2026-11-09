# AGY resume prompt — Phase-R after R0

Copy everything below the line into AGY CLI after Owner names the next gate.

---

继续 RunnerMesh 160H **Phase-R Owner Closeout**。你是唯一 AGY Implementer。

**R0=PASS 已接受。** 禁止再发明 P02–P08 功能叶或 Mesh/Linux/enforcement。权威：

- `goals/RM-160H-REMAINING-ROADMAP.md`
- `goals/RM-160H-REMAINING-CHECKLIST.md`
- `goals/RM-160H-PHASE-R-CHECKPOINT.md`

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
PHASE=RM-160H-PHASE-R-OWNER-CLOSEOUT
R0=PASS
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=6e6990b6b0c500a3c5b9dd10f0b5c8ee324e1236
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
HOSTED_CI_RUN=42646514218
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true_until_exact_owner_gate
WAITING_FOR_OWNER=true
```

## 硬约束

1. 保持 worktree `runnermesh-160h` + `feat/160h-gt01-field-baseline`；禁 `feat/lrm-09-*`；禁覆盖 productization。
2. 禁 `reset --hard` / `clean -fd` / 删其他 worktree。
3. 保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`。
4. 公开文档无绝对路径/密钥/私有 runner 标识（public_audit）。
5. **v0.1 H2 只用 G15R_RC**；**PR #65 tip 不得冒充 G15R**。
6. 无 Owner 明文门禁：禁 SCM 写、UAC、标签/注册、生产安装、H2/HUX-2/3、发布。
7. AGY/Cursor 自审 ≠ `INDEPENDENT_SECURITY_REVIEW=PASS`。
8. **只执行 Owner 本条消息明确授权的那一扇门**；完成后停，勿连开。

## 启动动作（任何门之前）

1. `git fetch` + 确认 tip=`6e6990b6b0c500a3c5b9dd10f0b5c8ee324e1236`（或 Owner 另指定的后继 tip）。
2. 把 Checkpoint / ROADMAP 头坐标同步到上述 tip + CI `42646514218`（R0 收据里若写错完整 SHA，以 `git rev-parse` 为准）。
3. 若本回合仅做坐标同步且无门禁授权 → 有界 docs 提交后仍报 `WAITING_FOR_OWNER`。

## Owner 门禁（各需独立授权文本）

| Gate | 前置 | 一句话 |
|---|---|---|
| **R1** ZenBookDuo 只读 | 物理机可用 | `discover-readonly`；公开仅 counts；EMPTY_OK 合法 |
| **R2** 独立安全审查 | 外部审查人 | 绑定 exact HEAD（H2→G15R；HUX-2→#65 tip） |
| **R3** H2 v0.1 | **R2 PASS(G15R)** | 仅 G15R_RC + 新 token；QUALIFICATION/RESTORE 分列 |
| **R4** HUX-2 live | **R2 PASS(#65)** | disposable Service；禁 dogfood；RESTORE 分列 |
| **R5** G-ADR006 | 无 live 必要 | Owner ACCEPTED/REJECTED/DEFER |
| **R6** HUX-3 | R4 PASS + R5 兼容 | 双 runner；A 不影响 B |
| **R7** Merge #65 | Owner 接受源码 | 合入 ≠ LIVE_DELIVERY=PASS |
| **R8** 发布 | 对应轨证据 | 分轨授权 |

## 每完成一扇门

写收据 → 更新 PHASE-R Checkpoint + REMAINING-CHECKLIST → 停并 `WAITING_FOR_OWNER`（除非 Owner 同条消息已授权下一扇）。

开始：若 Owner 未点名门禁 → 仅同步坐标（可选）后停；若已点名 → **只执行该门**。
