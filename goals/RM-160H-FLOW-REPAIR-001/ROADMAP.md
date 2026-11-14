# RM-160H-FLOW-REPAIR-001 — 临时流程修复路线图

日期：2026-11-14。状态：OWNER_APPROVED_PLAN / IMPLEMENTATION_NOT_STARTED。
配套：[开发与验收清单](CHECKLIST.md)。
依据：[AGENTS](../../AGENTS.md)、[Quality Gates](../../dev_governance_files/QUALITY_GATES.md)、[Fast Lane](../../dev_governance_files/FAST_LANE.md)。

## 1. Goal 契约

| 项目 | 已批准边界 |
|---|---|
| 目标 | 修复完成声明、候选入口、证据绑定及等待流程；完成后返回原160H工作 |
| 工作树 / 分支 / PR | runnermesh-160h / feat/160h-gt01-field-baseline / #65 |
| 起点 | 7107b0df19f89e02574a2ecfa84118cab03d7f07；启动时以实际Git核对漂移 |
| 已接受main参考 | d2b37444342d085e650f996e5fb4322f775bbe6f；#65目标为feat/lrm-productization，不默认main |
| 唯一实现者 | AGY。Codex本次仅获准落计划文件；不得并发修改实现或Git |
| 工时 | 目标3–5有效工时，上限6h；不以超时为PASS，不扩展为流程平台 |
| 生产权限 | PRODUCTION_MUTATION=false；无新增trust authority或系统变更权限 |
| 产品状态 | REMEDIATION_REQUIRED / R2_BLOCKED；临时Goal通过不清除产品blocker |
| 退出 | 本Goal验收通过，明确返回原路线；失败则保留具体日志、未完成项与恢复点 |

本计划已获Owner批准写入，并交由AGY按启动指令执行。计划落库不等于实现或产品验收完成。
保留既有未跟踪 goals/RM-160H-DELIVERY-PROGRAM-001/；不reset --hard、clean -fd、force-push、删除其他worktree，不写feat/lrm-09-*或其他productization工作树，不git add全目录。协调器模式仍须满足现有writer lease约束。

## 2. 允许修改与非目标

实现文件仅限：
- tools/dev/train.py：候选专用的精确保留项处理、接入证据检查。
- tools/dev/evidence_check.py：新增只读机械校验模块，不启动审查/CI，不执行Git写入。
- 对应 tools/dev/tests/ 测试；必要时添加对既有runner的stdin/超时行为回归。
- tools/dev/README.md、本目录计划/清单。
- goals/RM-160H-PROGRESS-BOARD.md：一次当前事实纠正。
- 既有私有 FINAL-EVIDENCE-INDEX.json：补充逐项证据，禁止将私人路径/标识/日志纳入仓库。

不修改产品Rust代码、.github/workflows/、change_classifier.py、ci_policy.py、分支保护、merge准入、全局clean判断、审查profile/沙盒/审批/准入规则。
不在此Goal修复六项产品缺陷，不执行现场、真实安装/激活、服务/会话/注册/标签控制、合并或发布。不新增P02–P08功能叶、Mesh、Linux、Broker或资源enforcement。
证据工具只校验来源、范围、引用与一致性，不能判断代码语义安全、证明进程隔离或替代独立R2。实现若需要改变权限/信任边界，应停下报告范围偏差，不以流程优化豁免安全门。

## 3. 实施顺序

| 阶段 | 工作 | 验收 | 估计 |
|---|---|---|---:|
| F0 | 固定当前事实与完成定义 | 活动缺口、正常入口、失败反例、预期结果、证据位置齐全；未修复保持OPEN | 0.5h |
| F1 | 使现有本地candidate入口可用 | 精确允许登记的既有未跟踪目录，其他脏改动仍拒绝，merge行为不变 | 0.5–1h |
| F2 | 证据机械检查及反例 | 错SHA、错包、范围扩大、缺报告、仍有缺陷却报就绪均拒绝 | 1–1.5h |
| F3 | 统一执行、审查和等待约定 | 复用现有runner/预检/wait；stdin、日志、退出码、截止和恢复信息可查 | 0.5–1h |
| F4 | 演练、一次候选CI与收口 | 工具门通过，产品仍准确BLOCKED，返回原路线 | 0.5–1h |

每次迭代跑受影响工具测试，保留本地小提交；本地收敛后统一推送一次。不要逐项推送、逐项启动全套CI/独审。阶段进度报告实际通过的反例和剩余缺口，不只报提交数或CI绿色。

## 4. F0 — 一处当前事实，明确完成含义

以既有私有证据索引维护当前LRM候选事实，公开看板仅为隐私安全摘要，历史路线图/报告不覆盖当前Git和原始证据。不要建立第二套调度器、数据库或锁服务。

至少登记：
1. 当前六项审查发现：Session gate可伪造、不完整进程观察、生命周期虚假成功、账本解析/错误处理、scope校验差异、底层Busy保护。
2. 原清单尚缺：正常逐Runner装配、迁移后单一控制权、关停失败后的控制权保留、不同真实版本的升级/回滚资格。
3. 每项的requirement/check ID、正常入口、可替换外部边界、失败反例、预期零副作用或读回结果、状态与证据引用。

接口存在、模拟返回成功、任务启动/exit 0、报告标题PASS、测试名称和CI绿色均不能单独代表产品完成。
历史accepted证据可按相同检查项和相关risk diff EMPTY复用；诊断、受限范围PASS、整体R2与现场资格分开。无法确定旧报告覆盖范围时记UNPROVEN，不自动扩大。

## 5. F1 — 精确保留未跟踪内容，不放宽Git安全

现有candidate调用Repository.clean()，会因必须保留的program目录而拒绝当前工作区。只为本地candidate增加显式登记的保留项处理，不修改Repository.clean()或merge命令。

- 仅容纳已批准的 goals/RM-160H-DELIVERY-PROGRAM-001/；记录其中既有内容的清单/指纹，检查前后不变。
- 使用规范化后的仓库相对路径和明确边界；拒绝前缀相似路径、逃逸、reparse/symlink及无法证明安全的内容。
- 其他未跟踪内容和已跟踪脏改动继续拒绝；候选HEAD变化立即使结果失效。
- 不删除、不stage保留目录，不修改.gitignore或全局Git设置隐藏问题。
- 复用现有fast gate和portability auto；没有现成WSL环境则如实UNAVAILABLE，不安装新环境。
- 流程Goal SHA与产品source/artifact SHA分列；产品风险差异为空可以明确复用，不把旧包说成在新工具提交上重建。

## 6. F2 — 轻量证据契约

每个被签收项应引用：scope/check ID、source SHA、适用artifact SHA、命令/测试、原始日志/报告、disposition；REUSED还需来源及risk diff依据。SHA和摘要从Git/实物读取，不从聊天手填。

机械检查必须拒绝：
- 39位/不存在SHA、实际审查根或报告绑定不一致。
- 文件缺失、摘要/字节数/provenance不符。
- 进程成功退出但没有有效报告，或报告/检查仍FAIL、BLOCKED、PENDING。
- Findings 1–3的PASS被提升为C0–C6整体PASS。
- 活动缺口仍OPEN，却声明SOURCE_SANDBOX_PREPARED。
- 仅以同进程角色标签或预检PASS替代独立接受。
- REUSED没有可核对的检查范围、来源或空风险差异依据。

工具只输出例如EVIDENCE_BINDING_CHECK，不输出安全认证或自动merge-ready。元数据不能替代原报告；语义覆盖仍由Implementer和独立审查者核实。
当前真实数据应得到“绑定事实可查、产品未就绪”，不能为了让临时Goal通过把产品状态改绿。

## 7. F3 — 复用现有执行能力

复用train.py的计时/wait-pr及已验证runner，不新增AI协调平台或重新实现审查沙盒。
- 每个任务保留命令、阶段、PID/创建时间或平台任务身份、日志、起止、退出状态、deadline。
- 审查使用已验证调用方式，stdin明确关闭，分别捕获stdout/stderr；不用临时拼接输出管道代替完整调用契约。
- 协调器预检一次并传入原始凭证，审查者核对而不递归启动预检/审查进程。
- 后台等待有总截止；超时如实FAIL/BLOCKED/未完成并保留恢复信息，不重复启动无界任务。
- 取消/回收仅作用于确切归属本任务的测试/审查进程，不碰AGY、桌面Codex或真实Runner。
- 正确区分工程量、命令elapsed、CI等待和模型usage，不从任一单项反推已耗160H。

用无凭据、无网络的自有fixture进程验证stdin EOF、非零退出、超时及日志。不得为演练启动多轮真实产品审查。

## 8. F4 — 验收、停机与回归主线

验证新增工具测试、错误样本、当前真实索引的预期拒绝以及正确样本；再按QUALITY_GATES运行所需本地门。不要每个小修改重跑产品全集。
本地settled后一次现有hosted CI，最多一次机械纠偏；保留最终Windows/Ubuntu及现有CI Gate，不伪造跳过后的PASS。若改动实质触及新的安全权限，停在范围边界，不自行扩大本Goal。
临时Goal不因产品R2仍BLOCKED而失败；它必须准确呈现该状态并阻止错误签收。不能把此Goal的PASS替代产品R2或Owner门。

停机条件：6h上限、同一失败族达到既定修订上限、竞争写入/归属不明、需要改变权限或信任边界、外部机制无变化阻塞。记录fingerprint、未完成ID、日志和恢复点，不原样反复重试。

收据至少分列：GOAL_ID、流程候选、LOCAL_GATE、CI_GATE、EVIDENCE_BINDING_CHECK、PRODUCT_SECURITY_REVIEW、PRODUCT_SOURCE_STATUS、PRODUCTION_MUTATION、NEXT_GOAL；正常控制在约15字段。

本Goal验收通过后，AGY无需再次申请普通源码权限，返回原160H的：
六项安全/生命周期整改 → 正常装配与单一控制权/关停恢复 → 最终真实包升级回滚资格 → 一个稳定候选的CI与独立R2。
参照现有私有160H路线图/清单及[审查包](../RM-160H-R2-SECURITY-REVIEW-PACKET.md)，以当前事实纠正历史完成声明，不重做已接受的无关证据。
现场、真实安装/激活、合并与发布仍逐一适用既定Owner门；G15R保持固定RC，严禁由LRM产物替代。
