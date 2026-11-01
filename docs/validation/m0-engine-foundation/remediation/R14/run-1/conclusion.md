# R14 最终裁决

## M0 是否闭合

**未闭合。**R14 按要求完成门禁和资料复核，但当前工作树有 20/46 项原任务证据不足，三态重核中无与事实不符项；需求矩阵须按源规格处理 16R/45S（brief 写 16R/44S）及工程治理 delta 1R/2S；E1、E2、E4、E5、E7 未闭合。`check_workspace.py` 在准入登记的离线隔离缓存上因缺少 `rust-embed` 使 workspace Clippy/test、dependency、public API 项未能核实。该环境失败不作代码失败诊断，也不作通过证据。

## M1 能否接线

**不能。**M0 出口未闭合；本轮不启动 M1。

## 六项原始阻断 finding

| Finding | 最终状态 |
|---|---|
| F-01 | 待定输入时 Redo 丢草稿已修；缓冲区历史移动前拒绝并有丢稿回归。保留新草稿开始后旧 redo 分支失效策略的 T-8-17 待裁决。 |
| F-02 | DocCore 普通/IME 共用受检消费链及拒绝状态回归已通过；真实 GPUI/IME 未验。 |
| F-03 | 多阶段来源片段复合及 Synthetic 锚点回归已通过。 |
| F-04 | 绑定旧快照、目标身份和 UTF-8 边界的重定位整改已通过。 |
| F-05 | Undo/Redo 方向选区保存/恢复回归已通过。 |
| F-06 | 不依赖 Aline 映射 API 的 51 行独立复算零差异；仅 basic fixture，旧 TSV 生成链不可追认。 |

**六项原始 finding 中仍未完成其指定整改的数量：0。**这不代表 M0 的其他退出条件已通过。未闭合事项另见下文：F-14 总范围独立复核仍待归档；T-8-17 语义仍待裁决；当前指定隔离缓存导致 E1 Cargo 检查未核实。F-12 第3行已按用户授权作最小更正并经治理 reviewer 核验。

## 46 项原任务及 Requirement/Scenario

原 46 项以原任务 1.1–9.6 为准，排除 R07 后追加的 6.9。R14 按可见证据重核为 **26 已核实、20 证据不足、0 与事实不符**；与历史 26/15/5 的差异及不能逐行追认的限制见 `deliverables/tasks-46.md`。六项 capability 源规格计数为 **16 Requirements / 45 Scenarios**；工程治理 delta 另含 **1 Requirement / 2 Scenarios**。这与派发简报所称 16R/44S 不一致，已保留全部源场景并说明多出的 Scenario 是 R07 Unicode 17 字素版本/上下文预算补充。场景覆盖状态逐条见 `deliverables/requirement-matrix.md`；未执行的 Scenario 不记为覆盖。

## 平台、AC/V 与能力限制

- 未执行真实 GPUI 输入命中、灰区只读呈现、真实 IME、系统焦点与窗口回调；Unicode 17 算法库测试不证明系统字素行为。
- 未在 Intel Mac、Windows 10 或 Linux 运行；未验证远端 CI 实际执行。
- Release 性能、解析/资源发布限额、第三方长调用中断性未测。
- 未验证已有文件的安全替换、权限/磁盘失败、外部修改协调、竞争时序与多资源事务。
- 资源迁移/清理与完整图片恢复未执行。
- AC 的基础用例不升格为 AC 全项通过；V-01～V-09 本轮均不整项通过，具体残留见 `deliverables/exits-e1-e7.md` 和 `docs/validation/m0-engine-foundation/group-9/step-9.5/report.md`。

## 本轮结果与 concerns

- 基线 HEAD：`96024867f7ae949d7328232b05d93ed5bc6033d9`；本轮无提交。R01–R13 的改动在工作树，精确基线见 `final-baseline-*`。
- 九项独立门禁中 8 项 exit 0，public API self-test exit 1（离线缓存缺依赖）；workspace test 361 passed、0 failed，比历史 312 新增 49、删除0、同名改写0。统一入口 exit 1；精确命令及逐项日志见 `deliverables/final-commands.md`。
- Guard matrix 计数/独立复核完成状态以其交付文件和 `reviews/` 最终结果为准。独立引擎 reviewer APPROVE_WITH_CONCERNS：没有新生产代码阻断，要求具体化 Scenario 映射，并保留 T-8-17 与历史 TSV 限制。F-12 行3已按用户授权最小更正并经独立 reviewer 核验；F-14 总体复核仍待归档。
- 重要限制：R13 前复核存在原始 stdout/stderr 未保全，本轮不补造；本轮新独立复核原始命令保存在 R14。当前隔离 Cargo cache 不能证明完整统一门禁通过。

证据入口：本目录 `gate/`、`guards/`、`reviews/`、`deliverables/`；全程状态见 `docs/validation/m0-engine-foundation/remediation/progress.md`。
