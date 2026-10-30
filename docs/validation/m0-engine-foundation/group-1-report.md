# 第 1 组完成记录：兼容目标、工具链与依赖准入

日期：2026-10-30。change：`m0-engine-foundation`，schema：`spec-driven`。本记录只关闭任务 1.1～1.4；不是 M0 出口、V-01～V-09 或三平台支持声明。正式八成员 workspace 尚未建立，属于第 2 组。

## 完成项与独立预期

| 任务 | 完成行为与证据 | 未被本项替代的验证 |
|---|---|---|
| 1.1 | [V-01 目标栏](../../v1-validation-plan.md#v-01-三平台兼容与发布矩阵)登记 CPU、系统/发行版与显示环境候选、当前设备和缺口，保留 Intel Mac、Windows 10，先 macOS 后 Windows/Linux | 候选最低系统未冻结；Intel/Windows/Linux 安装、原生构建/运行、输入法均未执行 |
| 1.2 | Rust/Cargo 1.98.1、edition 2024、rustfmt 1.9.0、Clippy 0.1.98，GPUI 发布包校验和、上游快照/发布差异、实际构建日志与格式/Clippy 结果；edition 2015+async 最小反例得到 E0670，独立预期来自旧 edition 不支持该语法 | 未声明 MSRV；未安装新工具链或 Xcode，原生 GPUI 平台后端未验证 |
| 1.3 | [当前准入身份](./group-1-admission.json)、GPUI/pure 两份完整机器依赖账本及实际 metadata/tree 分目标/feature 组合；候选守卫 60 条命令全部符合预期 | 候选检查不替代第 2 组实际 workspace 的依赖方向、成员 lint/catalog 与 CI 门禁 |
| 1.4 | 人工复核拟用五引擎归属/隔离候选闭包、所有 GPUI 例外路径、版本/来源/校验和与 target 条件，完成规定 docs/工程规范/change 同步；独立复核问题均已修复 | 第 2 组仍须在正式成员与最终 lock 上重新解析完整闭包，不从临时工程推断正式接线已合规 |

## 实际命令、结果与失败

- GPUI 的 `cargo +1.98.1 metadata --format-version 1`、`cargo tree --target all -e all`、分目标/feature 解析、macOS aarch64 `check`/`build`、候选 `fmt --check` 与 `clippy ... -- -D warnings`，以及 `test-support` 编译检查见 [GPUI 报告](./admission-2026-10-30/report.md)。每条 command JSON 保存实际 argv、退出码、工作目录和日志；自有探针补 `#![forbid(unsafe_code)]` 后 fmt/Clippy/build 重新通过，日志前缀为 `safety-`。
- 纯候选的 `cargo metadata`、`cargo tree --target all -e all`、五目标解析、macOS aarch64 `check` 与 release build、两个消费者运行见[纯候选报告](./pure-candidates-2026-10-30/report.md)及[命令记录](./pure-candidates-2026-10-30/commands.md)。Markdown 输出 events=6；嵌入 TOML 输出 title=Aline。它们是第三方 API 消费探针，独立预期来自 crate API 与夹具，不是产品金样或 catalog 协议验收。
- 主 Agent 最终运行 `python3 docs/validation/m0-engine-foundation/admission-2026-10-30/test_candidate_guard.py`，退出 0：[命令](./group-1-guard.command.json)、[输出](./group-1-guard.stdout.log)。60 条子命令中 14 个守卫正例、29 个守卫反例、17 条 Cargo 采集结果均与独立预期一致；7 个真实本地 Cargo 夹具的 alias/kind/target 形状亦通过核对。包括别名/目标/dev/build/feature/中间包禁库、第二解析器、原始 gpui 未批准来源、同包旁路、身份/校验和/target/features 漂移、完整 GPUI 子图未选节点的 lock 来源漂移及解析失败。最后复查原始正例通过。详见[守卫说明](./admission-2026-10-30/guard-summary.md)。
- 保留实际失败：crates.io API curl HTTP 403（Cargo registry 下载成功）；初始 Python 无 tomllib；初次 fmt 不符及错误 Clippy 参数顺序；toml 未启 serde 的消费失败；守卫开发期语法/诊断断言问题及修复前的 53/58 条汇总。edition 2015 和违规依赖图的非零退出是明确预期，不当成普通构建通过。故意失败 Cargo 的空 metadata stdout 以 `.json` 文件原样保留，其命令退出 101，不能改成合法 JSON 伪装解析成功。

## 准入及人工复核

当前正式消费计划：GPUI 仅限 app/UI/plugin 的已审精确组合；markdown 使用 `pulldown-cmark 0.13.6`；app-core 使用 `toml 1.1.8+spec-1.1.0`（manifest 精确需求 `=1.1.8`，build metadata 身份由 lock 确定）及 `rust-embed 8.13.0`。三个纯库关闭默认 features；toml 显式 std/parse/serde，rust-embed 显式 debug-embed，避免开发配置下从文件系统读 catalog。source/buffer/edit 当前只有标准库。实体/emoji/Unicode 分段、随机/diff 和所有资产后端尚未准入，不为未来需求预装。

主 Agent 独立遍历五个 probe 根的完整 resolve 闭包：source/buffer/edit 各 1 个节点，markdown 5 个，app-core 38 个（各自含消费方，共享节点不重复计入整个 45 节点候选 workspace）。未发现 GPUI 或禁用包；人工核对各消费方源码、受检错误和依赖声明，无 source→buffer/UI、buffer→source、app-core→业务层反向边。此处只审已消费的外部依赖与拟用归属，正式主链接线将在第 2 组核验。

功能重叠审查：Markdown 只采用 pulldown-cmark；TOML 只由 app-core 消费一个直接解析库，GPUI 过程宏的 Cargo manifest 读取工具属于第三方构建用途，不另设 Aline 配置解析器；rust-embed 按现行 catalog 契约使用。未知包是否实现隐含 Markdown 解析器或线程/全局行为仍需要人工源码审查，本次不把包名检查声称为对第三方全部源码的安全证明。

2026-10-30 用户在具体审计/例外草案交付后指示继续完成第 1 组，已按该指示同步现行架构 §3.2 与全部相关入口：

- 仅允许锁定 GPUI 的既有 regex 实现路径（包括 test-support→proptest 的既有 regex-syntax 入边）；五引擎、自有直接依赖和其它路径继续禁止。
- 仅允许完整图保留精确 getrandom→WASI p2 绑定边，所有已审桌面目标实际 tree 不选入；不批准 WASI 执行宿主。metadata-only 的旧 wasi 不列入例外。
- 校验所有入边/根路径及整个 GPUI metadata 子图的包身份/来源/校验和/features/边/选包状态；正式锁文件可换临时根或增合规节点，不能扩大例外，GPUI 子图变化须重审。

适用人工检查为 R-1/K-01/K-08、S-2/S-13-06/S-15、RS-04/RS-05。自有 Rust 探针 forbid unsafe、无 unwrap/expect；未授权任何 unsafe 例外。没有创建分支或 commit/push。未来若提交该组工程规则/守卫及配套文档的原子变更，主责 scope 为 `workspace`；纯文档提交使用 `docs`，不能把临时 probe 名当正式 crate scope。

## 独立复核及修复

按 AGENTS 路由进行独立 reviewer 复核，主 Agent 验证修复并复跑守卫。复核发现并修复：

1. 整体 Cargo.lock 字节一致要求改为受审 GPUI 子图一致，允许正式 workspace 换根并加入合规节点。
2. 补齐 engineering-governance 的 MODIFIED delta，澄清历史 0.3.9 本身不构成冻结证据。
3. app-core 探针不再用 `.ok()?` 吞掉 UTF-8/TOML 错误，改为 `ProbeError` 分类结果，重新 check/release build/run 通过。
4. 守卫补充未批准 `gpui` 原始包来源拒绝及完整 GPUI metadata 未选节点的 lock 来源/校验和校验，新增反例均按预期拒绝。
5. 纯候选命令摘要的 OS 版本误记和代码围栏已按原始 sw_vers 证据修正。

Reviewer 最终只读核验修复及 60 条留存结果，结论为未发现仍需阻止第 1 组出口的问题；没有重复完整构建或其它平台测试。

复核收口见[独立复核记录](./group-1-review.md)。最终 `openspec validate m0-engine-foundation --strict --no-interactive`、`git diff --check` 均退出 0；CodeGraph sync 退出 0，报告索引已是最新。168 个当时正常证据 JSON 解析通过，唯一排除的是具有退出 101 原始采集记录的故意失败 Cargo 空 stdout；Python 语法、本组报告本地链接和自有 Rust 探针禁 unwrap/expect/panic 文本扫描通过。详见[最终检查记录](./group-1-final-checks.json)。这些扫描不冒充正式工程守卫或平台验证。

本组新增/修改文件与实际 SHA-256 清单见[文件登记](./group-1-files.json)，包括现行规范入口、change 工件、候选脚本/夹具和原始日志；没有把本次未改的既有 engine delta 声明为本组新增。

## 交接与限制

第 1 组的输入、候选组合、合规/拒绝路径及人工作用范围均已留证，可进入第 2 组审查关口。当前不自动跨组实施：第 2 组需创建正式八成员 workspace、统一依赖与锁文件/工具链、实际 metadata 方向和禁令守卫、逐成员 lint 继承、安全反例、双语 catalog 及完整 fmt/Clippy/test 和 CI 接线。正式图若因合并依赖或 feature 统一改变 GPUI 子图，应依规则补审。

当前未执行或未验证：八成员工程门禁、CI、真实窗口、原生平台后端/IME/字体/保存、性能和其它平台原生构建/运行。V-01～V-09 均未整项通过；不声明 M0 出口通过，不开始 M1。系统候选与设备缺口继续按 V-01 留存，`metal` 缺失只记录事实，不修改全局开发目录或自动安装 Xcode。
