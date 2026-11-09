# AGY continue prompt — GT-04 Leaf 2

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

GT-04 leaf 1 已完成：代码锚 `38f7f6b…`。**立即进入 GT-04-LEAF-02**，不要空等 Owner。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T04-TRUSTED-SCM
LEAF=GT-04-LEAF-02
PRIOR_TRAIN=RM-160H-T03-IDENTITY-SESSION
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
REMOTE_TIP=a640de6fe15df2cb2955f1cfb3be641775eb9236
CODE_ANCHOR=38f7f6bbed789fe8dc0ddfa428b55e9e9f020b23
LAST_PASS_ALL=a640de6fe15df2cb2955f1cfb3be641775eb9236
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
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

## GT-04 LEAF-02 优先队列（源码/沙箱，自动连续）

1. **P04-03/04 合约（无 live）**：scoped exact-label add/remove **计划/合成接口** + 新鲜 readback 失败闭合；owned-workflow / `runs-on` 迁移检查清单（文档+测试桩）；generic self-hosted alone = `UNQUALIFIED_LEGACY_ROUTING`。
2. **P04-02/07**：same-scope dual managed 继续 REFUSE；ADR 0006 仍 `DRAFT_NOT_ACCEPTED`；不得假授权真标签写。
3. 更新 `goals/RM-160H-T04-CHECKPOINT.md`；有界提交 Draft #65；CI 绿再进下一叶。

## 勿重复 GT-04 LEAF-01 已完成项

- P03-03: `PrivilegedScmHelper`、`PrivilegedScmAction`、通配/空/外来服务拒止、立即回读校验。
- P03-05/06: `ScmTransactionJournal` 崩溃恢复 `reconcile_interrupted_scm_journal`、中断后无独立回读永不 RESTORE PASS。

开始：执行 P04-03/04 合约与 runs-on 迁移清单加固。
