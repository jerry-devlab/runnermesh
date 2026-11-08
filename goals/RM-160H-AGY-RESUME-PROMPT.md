# AGY continue prompt — GT-03 leaf 4

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-03 leaf 3 已完成：代码锚 `7bf1f47…`。**立即进入 GT-03-LEAF-04**，不要空等 Owner。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T03-IDENTITY-SESSION
LEAF=GT-03-LEAF-04
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=ddf34b483720e1c8ec2d090298ca9398662b6516
CODE_ANCHOR=7bf1f47a6b5699ee7cece7a6100aaa7e2c2d5190
LAST_PASS_ALL=ddf34b483720e1c8ec2d090298ca9398662b6516
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

## LEAF-04 优先队列（按序，跳过已证明项）

完成一叶验收→Checkpoint→下一叶；同类失败最多 3 轮实质不同修复。

1. **P05/P06 客户端/展示一致性**：多 runner CLI status / doctor JSON 渲染完备性，敏感字段完全脱敏；Tray presentation 消费多 runner 状态时保持 snapshot 只读契约；不触及真 SCM 或破坏性进程操作。
2. **P06 安装/打包沙箱保护**：沙箱安装、更新、回滚与 doctor 在遇到第三方路径漂移时的 fail-closed 语义测试加强。
3. **Owner 门禁保持**：H2/HUX-2/TRUSTED_SCM/G-ADR006/H3 维持 WAITING_FOR_OWNER。

非目标：ADR 0006 决策、真标签写、HUX-1 ZenBookDuo 伪造、H2 cutover。

## 立即动作

1. 核对 PR #65 CI 绿。
2. 读 `goals/RM-160H-T03-CHECKPOINT.md` 与 CHECKLIST。
3. 从队列第 1 项开工；本地全测 → 有界提交 → Draft #65 → 等 PASS_ALL。
4. 更新 T03 Checkpoint。
5. 推送：git 优先；失败用 Git Data API；LF；拒空 blob；去尾空行。

## 勿重复 LEAF-01 / LEAF-02 / LEAF-03 已完成项

- LEAF-01: `prepare_launch` Busy/listener Noop、`prepare_multi_*`、DuplicateWorkRoot、restore change_start_mode readback → RestoreFailed。
- LEAF-02: `interrupted-stop-readback-fail`、`interrupted-start-readback-fail`、`restore-start-mode-readback-interrupted`、`concurrent-agent-state-drift`、selector 移除 ≠ job barrier、Service vs Session multi-backend reconcile。
- LEAF-03: closed gate refuse all、granular gate permissions、unissued verifier rejects all、scoped admission simulation & migration sanitizer、distinct scope registry coexistence。

开始：核对 CI → 执行 P05/P06 客户端/沙箱加固。
