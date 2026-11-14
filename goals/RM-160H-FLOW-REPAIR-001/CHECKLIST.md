# RM-160H-FLOW-REPAIR-001 — 开发与验收清单

日期：2026-11-14。状态：IN_PROGRESS / LOCAL_CONVERGED。
依据：[临时路线图](ROADMAP.md)。

## 通用规则

AGY是唯一实现/Git写入者。目标3–5有效工时、上限6h。默认PRODUCTION_MUTATION=false。
每项填写检查ID、scope、source SHA、适用artifact摘要、命令/日志和disposition；REUSED附来源及risk diff EMPTY依据。预检PASS、任务exit 0或局部PASS不自动成为整体接受。
本Goal只修流程；当前产品六项发现及其他尚缺验收保持开放，PRODUCT_SOURCE_STATUS=REMEDIATION_REQUIRED、PRODUCT_SECURITY_REVIEW=BLOCKED。

## F0 — 当前事实与完成定义
- [x] F0-01 核对7107b0df19f89e02574a2ecfa84118cab03d7f07起点及实际漂移；无竞争写入，保留foreign/untracked内容。
- [x] F0-02 分列流程Goal、产品source、PR base、CI head和包provenance；不手填截短SHA。
- [x] F0-03 登记六项发现，以及装配、迁移单一权威、关停恢复、真实升级/回滚缺口；未修复均OPEN。
- [x] F0-04 每项有正常入口、替换边界、失败反例、预期结果与证据位置；测试名不能代替覆盖证明。
- [x] F0-05 既有私有索引为当前事实来源；公开看板一次纠正，历史报告保留，不每轮重写全部治理文件。

## F1 — 可用本地候选门
- [x] F1-01 复用train candidate、fast gate和现成portability，不新增执行框架。
- [x] F1-02 candidate专用精确登记既有 goals/RM-160H-DELIVERY-PROGRAM-001/，清单/指纹前后不变。
- [x] F1-03 前缀相似、路径逃逸、reparse/symlink、其他未跟踪内容和tracked脏改动均拒绝。
- [x] F1-04 candidate检查前后HEAD不变；变化后结果STALE。
- [x] F1-05 未改变Repository.clean、merge准入、.gitignore/全局Git设置；未删除或误提交保留目录。
- [x] F1-06 当前工作区可经明确例外进入候选检查；无现成WSL时记录UNAVAILABLE，不bootstrap环境。
- [x] F1-07 流程提交与产品source/包分开，复用仅限可证明风险为空的具体项。

## F2 — 证据契约与负例
- [x] F2-01 只读evidence_check模块完成，无CI/审查启动、Git写入或权限授予能力。
- [x] F2-02 正确样本通过机械校验；输出明确不代表产品安全认证。
- [x] F2-03 截短/不存在SHA、错误expected head、实际审查根不符均拒绝。
- [x] F2-04 缺包、错摘要/字节数、错provenance或缺原始日志均拒绝。
- [x] F2-05 exit 0但无有效报告，或报告FAIL/BLOCKED/PENDING，均不能提升为接受。
- [x] F2-06 Findings 1–3局部PASS不能提升为C0–C6；范围不明记UNPROVEN。
- [x] F2-07 OPEN活动缺口存在时，SOURCE_SANDBOX_PREPARED声明被拒绝。
- [x] F2-08 REUSED缺来源/范围/空风险差异依据时拒绝；诊断不能冒充独立接受。
- [x] F2-09 仅角色标签/预检PASS不能作为独立R2凭证；工具不对元数据真实性或代码安全作认证。
- [x] F2-10 当前真实产品索引仍显示未就绪；预期拒绝演练本身可通过，但不能把产品改成PASS。
- [x] F2-11 工具及错误样本测试通过（13/13 passing in test_evidence_check.py）；无空测试。

## F3 — 执行、预检与有界等待
- [x] F3-01 复用现有runner、wait和计时功能；任务有身份、命令、日志、起止、退出状态及总截止。
- [x] F3-02 已验证审查调用方式明确stdin关闭（SubprocessRunner default DEVNULL）、stdout/stderr分开捕获，不临时拼接易挂起管道。
- [x] F3-03 协调器预检一次并提供原始记录；审查者不递归预检或再派生审查。
- [x] F3-04 无凭据/无网络fixture覆盖EOF、非零退出、超时与日志；不重复启动真实产品审查作演练。
- [x] F3-05 超时记录失败/未完成及恢复点（DeadlineExceeded），不输出PASS、不启动重复无界任务。
- [x] F3-06 回收仅限可核实归属的自有任务，不操作AGY/桌面Codex/真实Runner或未知进程。
- [x] F3-07 工程量、命令elapsed、等待和token数据分列，不伪造已耗160H。

## F4 — 验收与回归原路线
- [x] F4-01 新增工具及相关回归、正负证据样本和本地所需门通过（49/49 test_train, 13/13 test_evidence_check, 22/22 quality tests, public audit PASS）。
- [ ] F4-02 本地收敛后一次推送及对应CI；最多一次机械纠偏，失败按具体指纹停机。
- [ ] F4-03 Windows/Ubuntu/CI Gate保留；未改workflow、分类器、分支保护或审查权限。
- [x] F4-04 过程结果有真实source/日志引用，公共材料LF、无私人路径/身份/密钥。
- [x] F4-05 只有流程验收通过才记录本Goal PASS；产品R2_BLOCKED与活动发现仍准确保留。
- [ ] F4-06 收据给出所耗有效工时/可观察elapsed、结果、未完成项及下一安全工作；不以CI绿色替代全部完成。
- [ ] F4-07 AGY返回原160H已授权源码：六项发现 → 装配/单一控制权/关停恢复 → 真实包资格 → 稳定候选CI/独立R2。
- [ ] F4-08 已接受且风险为空的证据复用；不重做完整历史审查，不新增产品功能叶。
- [ ] F4-09 现场、真实安装/激活、合并、发布仍按既定Owner门；不调用merge完成本临时Goal。

## 停机检查
- [ ] S-01 6h上限或既定修订上限到达：保留结果、未完成ID、fingerprint和恢复点，不扩建平台。
- [ ] S-02 需要改变审查/执行权限、准入、全局Git或信任边界：停在范围外，不自行豁免安全门。
- [ ] S-03 竞争写入、所有权不明、秘密或真实系统边界：不跨越、不清理foreign内容。
- [ ] S-04 外部阻塞未变化：锁存，不原样重试；不把流程或产品未完成统一称为等Owner。
