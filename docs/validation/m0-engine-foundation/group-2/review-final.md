# 任务 2.2 独立终审（2026-10-31）

结论：**当前任务 2.2 的依赖守卫及其规定证据通过独立审查，可由主 Agent 勾选；无未解决的实现缺陷。** 此结论不覆盖任务 2.3 及之后，不判 E1 完整通过，也不判任何平台或 V 项通过。前次 `review-independent.md` 是暂停时快照，下列结果解决其已列问题，不覆盖旧日志。

## 已解决问题

- 成员经中间包绕依赖方向：现在核对每成员完整可达内部依赖，实际隔离 Cargo 的 buffer→helper→source、plugin→helper→ui 均拒绝，诊断含目标边。
- 自有另一路径进入 GPUI metadata-only 禁包：完整图计算移除 GPUI 根后的可达集合；已有节点不再被 GPUI 子图整体遮蔽。明确标记的 metadata-only wasip2 旁路反例命中该拒绝分支。
- Windows host 宏依赖误拒：已由独立 `cargo tree -i libc` 证实 rust-embed 的 host proc-macro→sha2→cpufeatures→libc 路径。采用完整 metadata 约束身份、全部边与保守闭包；实际目标 tree 包含目标及本机 host 的实际选包；filtered metadata 仅交叉检查为完整图子集。不是删除 libc 或放宽禁包。
- 离线采集标签：检查输出 hash、成功退出、manifest/lock 关系、target/features/边范围及重复参数，最小错误信封和真实 Cargo 失败均拒绝。
- metadata-only checksum 原负例同时删 tree 节点，不能单独证明 checksum 验证。最新 `engineering-self-test-reviewed` 改为实际未选中的 rav1e，仅改其 lock checksum，metadata/tree 均不变；独立 diff 已确认只有该 checksum 行变化，守卫拒绝。

## 复核证据

- `review-complete-graphs.*`：独立调用七场景实际采集输入检查，退出码 0。正式完整 all-target GPUI 子图对历史受审 base-final 无包/版本/来源/checksum/边变化；唯一变化为已准入 TOML 带来的 toml_datetime serde feature。精确禁包例外的 source/checksum/全部入边与已审政策一致。主 Agent 据此批准具体 policy；未扩大例外。
- `engineering-self-test-reviewed/summary.json`：77 子命令全部符合独立规范预期。其中实际隔离 Cargo 解析使用明确标记的本地 0.0.0 合成包，不安装真实禁用库；身份/checksum/target/features 漂移等使用明确标记的图变异，不伪称 Cargo 原输出。
- `review-final-checks.py` 与 `.stdout.json`：独立核对全部 77 条退出码、stdout/stderr hash、预期诊断及修正后的最小 checksum 差异，命令 `python3 docs/validation/m0-engine-foundation/group-2/review-final-checks.py` 退出码 0。
- `engineering-complete-results/actual-workspace-reviewed.*`：恢复正式正常图，实际入口退出码 0，七场景均通过。主 Agent 另有 `root-dependency-checks` 重跑证据。

## 覆盖与限制

已覆盖直接及中间非法内部边、五引擎别名/中间包/dev/build/target/feature 泄漏 GPUI、禁库与第二解析器、合法精确例外及同包旁路、来源/版本/checksum/target/kind/features 漂移、失败采集不放行。当前八成员无自有 feature，未来新增 feature 组合须登记；不以 all-features 并集证明未知组合。

host 仅 macOS aarch64。Intel Mac、Windows、Linux 五个目标只有依赖解析证据，原生构建、窗口、真实输入及平台支持均未验证。守卫检查已知包身份与图规则，不能识别任意新包内部是否隐藏解析器/动态宿主语义；新增依赖仍须人工逐项准入。当前未运行 Rust 完整门禁、catalog、安全 lint 或 source 功能验收，这些留后续任务。
