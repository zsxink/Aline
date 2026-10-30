# 正式 GPUI 子图复核（2026-10-31）

独立 reviewer `guard_review` 对新图模型及七个具体场景运行 `check_inputs`，结果见 `review-complete-graphs.*`。主 Agent 已读取实际输出与完整 all-target 差异，批准当前 `scripts/engineering_policy.json` 的具体指纹，将 `reviewed` 改为 true。

- 比较历史 base-final 完整子图，正式完整图无新增包、版本、来源、校验和或边；唯一 feature 变化是 `toml_datetime 1.1.2+spec-1.1.0` 增加 serde，来自已准入的 app-core TOML 依赖。
- regex 三包及 WASI p2 绑定的身份、来源、校验和、全部合法入边和 target 约束保持不变；没有扩展禁库例外。
- 采用完整 all-target metadata 保留 host proc-macro/build 的目标边；实际分目标 tree 核选包，另用 filtered metadata 交叉核对。Windows tree 中 libc 的真实路径为 `aline-app-core → rust-embed → rust-embed-impl（host proc-macro）→ rust-embed-utils → sha2 → cpufeatures → libc`，见 `engineering-complete-results/windows-host-libc.*` 与独立 reviewer 的 `review-windows-libc.*`。没有删除该包或建立 libc 豁免。
- `engineering-baseline-diff.json` 对历史各分目标图的更多边变化，来自当前使用完整 metadata 保留所有 target 边以及关闭尚未消费的平台后端；逐项保留，不把 filtered metadata 当作完整 host+target 编译图。

此批准仅限当前具体指纹，不允许未来自动刷新以追随漂移。任何新的子图或 target/feature 组合须重新审计。依赖图解析不等于平台构建/窗口支持；完整自测与正式入口结果另行记录，不能单凭本复核勾选任务 2.2 或判 E1/V 项通过。
