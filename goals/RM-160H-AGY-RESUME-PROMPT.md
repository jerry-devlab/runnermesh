# AGY continue prompt — GT-03 leaf 3

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-03 leaf 2 已完成：代码锚 `077344b…`。**立即进入 GT-03-LEAF-03**，不要空等 Owner。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T03-IDENTITY-SESSION
LEAF=GT-03-LEAF-03
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=077344bea22c9a373f390a80cf3131b682d7f45a
CODE_ANCHOR=077344bea22c9a373f390a80cf3131b682d7f45a
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

## LEAF-03 优先队列（按序，跳过已证明项）

完成一叶验收→Checkpoint→下一叶；同类失败最多 3 轮实质不同修复。

1. **P03-02 源码加固**：可信 Owner issuance/verification 类型边界与不可伪造性；`UnissuedOwnerAuthorizationVerifier` 恒拒；synthetic grant 永不授权 live Windows 写；GateClosed 路径完备。
2. **P04-02 源码与仿真保持**：same-scope dual managed 继续 REFUSE；保留 runnermesh-admit v0.1；FakeGithub / scoped_admission_sim 仅仿真；严禁将 ADR 0006 标记为已接受。
3. **P05/P06 残余加固**：多 runner 部分失败与隐私脱敏、安装与打包沙箱补强；不增生产 mutation。

非目标：ADR 0006 决策、真标签写、HUX-1 ZenBookDuo 伪造、H2 cutover。

## 立即动作

1. 核对 PR #65 CI 绿。
2. 读 `goals/RM-160H-T03-CHECKPOINT.md` 与 CHECKLIST P03–P05。
3. 从队列第 1 项未证明缺口开工；本地全测 → 有界提交 → Draft #65 → 等 PASS_ALL。
4. 更新 T03 Checkpoint。
5. 推送：git 优先；失败用 Git Data API；LF；拒空 blob；去尾空行。

## 勿重复 LEAF-01 / LEAF-02 已完成项

- LEAF-01: `prepare_launch` Busy/listener Noop、`prepare_multi_*`、DuplicateWorkRoot、restore change_start_mode readback → RestoreFailed。
- LEAF-02: `interrupted-stop-readback-fail`、`interrupted-start-readback-fail`、`restore-start-mode-readback-interrupted`、`concurrent-agent-state-drift`、selector 移除 ≠ job barrier、Service vs Session multi-backend reconcile。

开始：核对 CI → 执行 P03-02 / P04-02 源码加固与仿真测试。
