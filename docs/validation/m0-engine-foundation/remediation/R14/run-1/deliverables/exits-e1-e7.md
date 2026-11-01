# E1–E7 M0 出口逐项复核

出口定义来自 `openspec/changes/m0-engine-foundation/proposal.md`。状态以当前工作树可复算证据为准；历史“19/19 / 312 passed”记录保留为旧时点证据，不替代 R14 当前门禁和独立复核。

| 出口 | 定义要求 | 已有证据 | 当前状态与未闭合原因 |
|---|---|---|---|
| E1 工程 | 八 crate 成员、精确锁版/依赖准入，成员 lint 正反例、双语 catalog、三条 Cargo 门禁 | workspace/锁文件和历史依赖图；R12/R13 守卫与 lint 证据；R14 九项单独门禁：fmt/clippy/test、engineering/lint/safety self-test、OpenSpec、diff 均 exit 0，但 public-api self-test exit 1。统一入口实际运行时，其 clippy/test exit 101，dependency/public-api exit 1；离线隔离缓存缺 `rust-embed`，见 `../gate/unified-captured/`。 | **未闭合。**本轮指定缓存下无法核实 Cargo 编译、依赖闭包和公开 API 检查；工程治理 matrix 的治理 delta 也未逐 Scenario 证明。远端 CI 运行未核实。 |
| E2 无头核心 | source/buffer/markdown/edit 的公共 API、无窗口执行和各类注入预算入口 | `cargo test --workspace --locked --offline` 361 passed；独立 reviewer 子集321 passed；R01–R10 对应 API/预算报告和回归。 | **基础证据充足，出口未闭合。**公开 API 隔离编译自测因缺依赖无法核实；46条任务仍22条证据不足；矩阵逐 Scenario 映射复核尚未完成。 |
| E3 展示树 dump | 同一夹具实际 dump 文字、块/Gap、原范围、映射 precision、保护范围、诊断及独立手写期望 | R09 `e3_*` 同夹具测试和独立期望；R10 51行独立复算零差异；R14 workspace 当前测试。 | **已满足基础 fixture 出口范围。**仅证明 basic fixture，不扩张为完整 Markdown/解析语法通过。 |
| E4 AC-01～04 基础/拒绝无副作用 | 原字节 roundtrip、局部补丁、Unicode 映射/选区、900ms/IME 模拟组和拒绝零副作用 | group-4/5 原始记录、R01/R04/R07/R08 回归和 R14 workspace 测试；拒绝状态覆盖在 R04/R08。 | **部分满足。**模拟输入组不等于真实 GPUI/IME/焦点/窗口；文件替换竞争和完整 AC 整体验收未执行。 |
| E5 块+Gap/映射 | 无重叠源码覆盖，块不含尾分隔；精确往返、稳定锚点、对齐幂等 | group-6 goldens、R02/R03/R07/R09/R10 证据；R14 新增映射回归当前通过。 | **基础映射 evidence 存在，出口尚未全闭。**独立复算仅一个 basic fixture；场景矩阵源规范总数与 brief 不同，未完成全部场景逐条审计。 |
| E6 V 未测清单 | 登记 V-01 目标、V-06/V-07 基础 fixture/分项结果和未测平台/阶段 | group-9 9.5 V 状态和后续交接；R11 新工具/设备采集；当前 `progress.md` 与 `conclusion.md` 明示未执行项。 | **清单基本满足；验证出口仍不代表 V 通过。**V-01～V-09 均不因本轮修改整项通过，V-07 的系统字素/IME 与解析口径未冻结。 |
| E7 TA-01 输入组/提交 | 单草稿、多组提交、不重复离块、快照不封口及实验未完成项 | R01/R04 实际 DocCore 发布链和完整状态拒绝测试；R08 负例；R14 当前 workspace tests。 | **未闭合。**T-8-17 新草稿起始与 Buffer redo 分支失效时序未裁决；真实 callback/IME/焦点行为未测。 |

## 出口结论

E3 的限定 fixture 证据及 E6 的“未测项登记”可单独成立；E1/E2/E4/E5/E7 的全部要素未形成闭合证据。由于统一入口依赖的隔离 Cargo home 缺少 `rust-embed`，R14 不把离线构建失败记作代码错误，也不把它记作门禁通过。所有出口综述以本表状态为准：**M0 未闭合，M1 暂不能接线。**
