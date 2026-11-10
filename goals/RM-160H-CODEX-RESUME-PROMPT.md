# Codex resume prompt — 160H after D24 pause

Copy everything below the line into Codex.

---

你是 RunnerMesh 160H 的**唯一 Codex Implementer**。Owner 已暂停 Cursor/AGY 开发，现由你接手。

**现状：** `SOURCE_AND_SANDBOX_CLOSEOUT=PASS` · `D24_ASSIST_READY` · `WAITING_FOR_OWNER` · 综合进度 ≈80%。
**禁止**发明 P02–P08 新功能叶、Mesh、Linux、Broker、Job Object enforcement。

权威交接与坐标：

- `goals/RM-160H-AGY-TO-CODEX-HANDOFF.md`
- `goals/RM-160H-PROGRESS-BOARD.md`
- `goals/RM-160H-REMAINING-ROADMAP.md`
- `goals/RM-160H-REMAINING-CHECKLIST.md`
- `goals/RM-160H-PHASE-R-CHECKPOINT.md`
- `goals/RM-160H-R2-SECURITY-REVIEW-PACKET.md`
- `goals/RM-160H-R5-ADR006-OWNER-BRIEF.md`

```text
ROOT=C:\Dev\src\runnermesh-160h
LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
PR=65
TIP=ea0d9a36000d5c993c9e7c5d5c46450bdd1f87b8
CI=42743583126
CODE_ANCHOR=b221eaf57be5a0a3952ba0e30d1e3895e34749f9
G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
PRODUCTION_MUTATION=false
```

硬约束：禁 `reset --hard`/`clean -fd`/删其他 worktree；禁碰 `feat/lrm-09-*` 与 productization；保留未跟踪 `goals/RM-160H-DELIVERY-PROGRAM-001/`；H2 只用 G15R_RC，#65 tip 不得冒充；无 Owner 明文禁 SCM/UAC/注册/安装/H2/HUX-2/3/发布；Codex 自审 ≠ `INDEPENDENT_SECURITY_REVIEW=PASS`；公开文档 LF、无绝对路径/密钥。

启动：读 AGENTS.md + 上述交接文件 → `git fetch` 核对 tip/CI → 若 Checkpoint 仍写旧 tip 则有界同步到 `ea0d9a3…`/`42743583126` 并提交本交接文件。

**若 Owner 未点名门禁：** 坐标/交接同步后停，报告 `WAITING_FOR_OWNER`。
**若 Owner 点名一扇门：** 只执行该门（R2/R5/R1b/R3/R4/R6/R7/R8），写收据，更新 Progress/Checklist/Checkpoint，然后停。勿连开多扇 live 门。
