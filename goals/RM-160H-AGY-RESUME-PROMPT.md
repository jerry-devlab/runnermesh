# AGY continue prompt — GT-06 Leaf 02 (Candidate Freeze & HUX-2 Prep)

Copy everything below the line into AGY CLI.

---

继续 RunnerMesh 160H Goal Train。你是唯一 AGY Implementer。

**GT-06 Leaf 01 已完成**（代码锚 `0dd4afe…`）：工作区 autostart 拒绝 + CLI `--diagnostic` 模式已通过 335 项本地测试与 clippy。进入 **GT-06-LEAF-02**：P07-01/03 候选冻结与 HUX-2 Owner 资格包打磨（无实机写）。不要空等 Owner，也不要假 PASS live。

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T06-UX-INSTALL-HUX2
LEAF=GT-06-LEAF-02
PRIOR_TRAIN=RM-160H-T05-ADMISSION-MULTIRUNNER
ROOT_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
CANDIDATE_PR=65
CODE_ANCHOR=0dd4afeb32d15c0a7a70043d28339ead4572ddce
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
7. HUX-0 已 Owner PASS：未改呈现风险则 **不要**重跑点击验收。

## GT-06 优先队列（跳过已证明项）

1. **P07-01/03 准备包（无 live）**：冻结候选 SHA/CI 映射；HUX-2 一次性 disposable Service **公开安全检查清单**（prestate/Busy 缺席/admission ownership/restore/STOP）；独立安全审查仍 PENDING；**不**激活真服务。
2. 更新 `goals/RM-160H-T06-CHECKPOINT.md`；有界提交 Draft #65；CI 绿再进下一叶。

## Owner 门禁（勿假 PASS）

```text
HUX1_ZENBOOKDUO=PENDING
H2=WAITING_FOR_OWNER
HUX2_LIVE / TRUSTED_SCM_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
```
