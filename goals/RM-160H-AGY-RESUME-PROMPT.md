# AGY resume prompt — Phase-R Owner closeout

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H **剩余工作（Phase-R Owner Closeout）**。你是唯一 AGY Implementer。

源码/沙箱已 `SOURCE_AND_SANDBOX_CLOSEOUT=PASS`（GT-01…GT-07）。**禁止再发明 P02–P08 功能叶或 Mesh/Linux/enforcement。** 权威剩余计划：

- `goals/RM-160H-REMAINING-ROADMAP.md`
- `goals/RM-160H-REMAINING-CHECKLIST.md`
- `goals/RM-160H-PHASE-R-CHECKPOINT.md`

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
PHASE=RM-160H-PHASE-R-OWNER-CLOSEOUT
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=68eccdd6f57823c9142e7ca63ac30b217c58e50e
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true_until_exact_owner_gate
```

## 硬约束

1. 保持 worktree `runnermesh-160h` + `feat/160h-gt01-field-baseline`；禁 `feat/lrm-09-*`；禁覆盖 productization。
2. 禁 `reset --hard` / `clean -fd` / 删其他 worktree。
3. 保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`。
4. 公开文档无绝对路径/密钥/私有 runner 标识（public_audit）。
5. **v0.1 H2 只用 G15R_RC**；**PR #65 tip 不得冒充 G15R**。
6. 无 Owner 明文门禁：禁 SCM 写、UAC、标签/注册、生产安装、H2/HUX-2/3、发布。
7. AGY/Cursor 自审 ≠ `INDEPENDENT_SECURITY_REVIEW=PASS`。

## 立即执行（无需等 Owner）：R0

1. 读 REMAINING-ROADMAP + REMAINING-CHECKLIST + T07 Checkpoint + HUX-2/H2 预飞文档。
2. 核对 tip/CI 与坐标；修正公开包字段绑定（无扩 scope）。
3. 更新 `goals/RM-160H-PHASE-R-CHECKPOINT.md`。
4. 有界提交 Draft #65；LF；去尾空行；CI 绿。
5. **EXIT R0** 后：若无 Owner 新授权 → 停并报告 `WAITING_FOR_OWNER`；若有授权 → 只执行授权的那一扇门（R1…R8）。

## Owner 门禁队列（各需独立授权文本）

| Gate | ID | 一句话 |
|---|---|---|
| ZenBookDuo 只读 | R1 | `discover-readonly`；EMPTY_OK 合法 |
| 独立安全审查 | R2 | 外部审查绑定 exact HEAD |
| H2 v0.1 | R3 | 仅 G15R_RC + 新 token；RESTORE 分列 |
| HUX-2 | R4 | disposable Service；禁 dogfood |
| G-ADR006 | R5 | Owner 接受/拒绝 scoped selector |
| HUX-3 | R6 | 双 runner；A 不影响 B |
| Merge #65 | R7 | 源码接受 ≠ live PASS |
| H3/发布 | R8 | 分轨授权 |

## 每完成一扇门

写收据字段（QUALIFICATION/RESTORE 分列）→ 更新 PHASE-R Checkpoint + REMAINING-CHECKLIST 勾选 → 停或问 Owner 下一扇门。**不要自动连开多扇 live 门。**

开始：执行 **R0 packet polish** → 推送 → 等待或执行 Owner 指定的下一扇门。
