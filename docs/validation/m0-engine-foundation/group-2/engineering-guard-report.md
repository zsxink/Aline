# 任务 2.2 正式依赖门禁证据（2026-10-31）

## 检查入口与责任

`python3 scripts/check_engineering.py` 同步调用 Cargo，核查正式八成员 workspace、完整依赖方向、五引擎闭包和架构 §3.2 精确例外。`--output-dir` 指定日志目录；不指定时用隔离临时目录。Cargo 始终 `--locked --offline`，不安装工具或下载依赖。运行环境使用第 1 组登记的专用 `CARGO_HOME` 与 `RUSTUP_TOOLCHAIN=1.98.1`，实际 env 覆盖记录在 command JSON。

`python3 scripts/check_engineering.py --self-test` 在隔离夹具目录调用真实 Cargo 并运行明确标记的图变异；日志目录同样可用 `--output-dir` 指定。正式根 Cargo 清单、lock 与 crate 源码均不被自测改写。

`engineering_dependencies.py` 只处理依赖规则，不把源码安全/catalog 规则冒充已完成。`engineering_policy.json` 的指纹不自动更新；`engineering_audit_baseline.py` 只输出待审差异并生成 `reviewed=false` 策略，不能作为正常入口偷偷接受漂移。当前具体基线已由独立 reviewer 与主 Agent 审阅批准，见 `engineering-baseline-review.md`。

## 图模型与目标覆盖

- 完整未过滤 metadata 保存 normal/dev/build/全部 target 的保守闭包，按真实 package 名识别别名，检查每个成员经中间包到其它成员的可达方向、每个引擎的完整闭包。
- 实际分目标 tree 决定本次 Cargo 选包，包含本机 host 的 proc-macro/build 依赖；另存 target-filter metadata，要求其 package 声明、feature 及边条件是完整图子集。不能用 target-filter metadata 删掉实际 host 依赖。
- Windows 目标中 libc 经本机 rust-embed proc-macro 被选中，已有实际逆树证据；没有删除真实图或设包豁免。metadata 选包过近似与实际 tree 的差异逐项输出；无法解释的差异拒绝，不当通过。
- 实际解析 all-target、`aarch64-apple-darwin`、`x86_64-apple-darwin`、`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu` 和 all-features。当前八成员没有自有 feature；未来自有 feature 定义变化须登记具体组合，不能仅靠 all-features 并集。未开启 GPUI 平台后端、test-support 或尚未准入 features；其他 target/feature 组合未验证。
- 所有图上的 GPUI 来源、版本、checksum、边、feature 与选包指纹冻结。regex/WASIp2 例外还核全部实际入边与移除 GPUI 后的根可达路径，合法路径不能遮蔽自有旁路；metadata-only 既有禁包旁路也拒绝。

## 实际命令与结果

完整 argv/cwd/时间/env/退出码和 stdout/stderr SHA-256 均在 `engineering-complete-results/` 的 command JSON，原始输出同行保存。

1. `python3 scripts/check_engineering.py --self-test --output-dir <engineering-self-test-reviewed>`：退出码 0。`engineering-self-test-reviewed/summary.json` 记录 **77 个子命令全部符合独立预期**：43 个预期 0、33 个规则拒绝预期 1、1 个真实 Cargo 缺失清单预期 101。
2. 自测后恢复正式 workspace，运行 `python3 scripts/check_engineering.py --output-dir <engineering-reviewed-graphs>`：退出码 0。七种场景通过，原始 Cargo 图在 `engineering-reviewed-graphs/`。
3. `cargo tree --locked --offline --workspace --target x86_64-pc-windows-msvc -i libc -e normal,build,dev`：退出码 0，保存实际 host proc-macro 路径。

自测的真实 Cargo 夹具是 20 个独立八成员 workspace：2 个正常方向正例、3 个直接非法方向、2 个中间包非法方向、6 个引擎 GPUI 泄漏（别名/dev/build/target/feature/中间包）、7 个 UI 禁库/替代 GPUI/第二解析器反例。所有外部同名违规包为明确的本地 `0.0.0` 合成包，不是真实发布库；仅解析，不构建或安装。

另外 10 个明确图变异覆盖既有 regex 旁路、metadata-only WASIp2 旁路、普通与 metadata-only checksum、target/kind/feature/version/source 漂移和原图正例；采集失败/hash/target/feature/重复参数信封及真实 Cargo 失败均不能当合规。变异的 JSON/lock 与 `*.synthetic-command.json` 明确注明是规则测试构造，不能当真实 Cargo 输出；实际子进程判定与退出码独立记录。

## 失败与修复

保留 `self-test-initial.*`：退出码 1，夹具假定正式图存在旧 metadata-only wasi 节点而触发 StopIteration；改为正式图实际存在的 WASIp2，并在明确合成变异中构造未选状态。

保留 `self-test-final.*`：退出码 1，target 变异追加了重复 `--target`，暴露入口只看首参数的问题。已拒绝重复参数，同时分别测试正确的 target 值漂移与重复参数；最终 `self-test-reviewed.*` 为真实修复及独立审查后结果。不能把先前失败抹成通过。

独立审查还指出旧 `engineering-self-test-complete` 的 metadata-only checksum 夹具同时改了选包，无法隔离证明 checksum 检查。最终 reviewed 版本改为实际 all 图中本已未选的 `rav1e` 节点，仅修改其 lock checksum，其余输入完全不变；再次运行完整自测及正常入口均退出 0。旧证据保留，不作为该项最终证明。

`engineering-partial-status.md`、`engineering-partial-results/` 保留此前契约暂停时的中途状态，不再代表当前实现状态。

## 限制

本项仅证明依赖规则与当前具体解析组合，未执行跨平台构建/窗口/真实输入，不能宣称 V-01～V-09、E1 或完整 M0 通过。源码安全、成员 lint 实际继承、catalog 和完整 Cargo 检查仍由后续编号任务分别取证。第三方包源码未因本守卫全面安全审计；未知动态宿主的语义不能仅按包名自动识别，新增依赖仍须先走准入审阅。metadata 在选包边上可能过近似，检查器保守拒绝，不静默扩大例外。
