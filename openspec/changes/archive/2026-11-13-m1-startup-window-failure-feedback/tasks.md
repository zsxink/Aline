# Tasks

## 1. 本地化文案与 catalog（AGENTS 铁律 7、UI §14）

- [x] 1.1 在 `crates/aline-app-core/locales/zh-CN.toml` 与 `en.toml` 的 `[startup]` 段新增
  `no_display` 键（中文「未检测到可用显示器，未显示窗口。」、英文
  "No display was available; no window was shown."）；把
  `crates/aline-app-core/tests/m1_window_catalog.rs` 的键清单补上 `startup.no_display`。
  验收 `cargo test -p aline-app-core --locked --offline` 通过，且 `scripts/check_ui_catalog.py`
  正常检查与自测通过。
  **2026-11-12 修订**：文案由「无法连接到窗口服务，未显示窗口。」改为因果中性的
  「未检测到可用显示器，未显示窗口。」（复审 finding 5，用户裁决）。

## 2. 启动期显示检测（CODE_STANDARDS §2.2、§3.2）

- [x] 2.1 在 `crates/aline-ui/src/native.rs` 增加显示可用性检测：`Platform::displays()` 为空时
  延迟约 2 秒复检，仍为空则调用 `report_startup_failure` 输出本地化 `startup.no_display`
  并以状态码 1 退出。延迟常量定义为私有 `const`，附中文注释说明它是本轮选定值而非平台常量。
  **实施修正（相对设计初稿）**：检测放在 `Application::run` **之前**，不在启动闭包内。
  理由是**可观察行为**：该条件下只能观测到「无窗口、无 stderr、进程不退」，无法区分启动
  回调是否执行过，因此选一个一定执行得到的检测点。首版曾写作「回调根本不会被调用」，
  该断言无埋点支撑，已按 2026-11-12 裁决降级（复审 finding 3）。
  验收 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace` 全部 exit 0；不新增依赖、不引入 `unsafe`、不改 `run_application` 签名。
- [x] 2.2 补最小失败路径测试：在 `native.rs` 的测试模块内，用可注入的「显示计数」
  验证「空 → 复检空 → 失败」与「空 → 复检非空 → 继续」两条分支，以及「首查非空 →
  立即继续」。测试不得启动真实窗口、不得依赖真实 WindowServer；纯规则验证，不冒充真实
  失败收据。验收对应 `cargo test -p aline-ui` 通过。
  **2026-11-12 修订**：把恒真的 `is_failure` 换成纯函数 `startup_failure(first, recheck)`，
  并补一条「首查非空时即使复检为空也不判失败」的编排断言（复审 finding 9）。
- [x] 2.3 失败出口只输出本地化文案：删除 `": no active display after recheck"` 等英文技术
  诊断，既有 `window_failed` 分支同口径。验收 `cargo clippy -D warnings` 通过、真实剖面
  stderr 全文只剩本地化句（复审 finding 4，用户裁决判为铁律 7 违规）。

## 3. 真实窗口复核与回填（V-09 分项、AGENTS 文档规则）

- [x] 3.1 用重建后的 Release 二进制在 macOS arm64 上做两条真实剖面复核：正常启动应无
  `startup.no_display` 输出；`deny-windowserver` 剖面应出现本地化 `startup.no_display`
  且退出码为 1。原始 stdout/stderr、窗口列表与退出码经 `scripts/evidence_paths.py`
  归一化后存 `logs/validation/m1-single-window-readonly/2026-11-12-window-failure/`。
  未取得真实剖面时本项保持未完成，不用合成测试代替。
  **2026-11-12 修订**：沙箱剖面下进程在窗口枚举前已退出，报告不再写「窗口 0 / 0」，
  改记 `window_probe_run=false`；基线补测首窗在屏时刻 0.71 s（run-3；复审 finding 2、7）。
  判别信号补独立同进程 Swift 探针 `signal-probe/`，含源码与三剖面原始输出
  （复审 finding 1）。
- [x] 3.2 回填 `docs/ui-spec.md` §14 与 `docs/v1-validation-plan.md` V-09 分项：记录
  「显示不可用时给出本地化提示并退出」的已实现状态、检测信号与 2 秒复检的边界；
  在 `openspec/changes/m1-engineering-evidence-gates/tasks.md` 2.2c 追加指向本变更的说明。
  保留 V-09 未整项通过、Windows/Linux 未测、显示器睡眠期可能误判、显示枚举非空也不证明
  窗口服务可用的边界。验收 `openspec validate --all --strict --no-interactive` 通过、
  `git diff --check` 通过、修改文件无本机绝对路径。

## 4. 独立复审后的工件一致性

- [x] 4.1 proposal 的 What Changes 与 design 的 Decisions 2/5 按实际实现改写，说明否决
  闭包方案的依据与「回调是否执行不可判定」的结论（复审 finding 6）。
- [x] 4.2 交叉引用 `docs/ui-spec.md` §5 → §14（复审 finding 11）。
- [x] 4.3 复审 finding 8：把 `run_window_failure_probe.py` / `run_signal_probe.py` 移入
  `scripts/` 并接入 `check_workspace.py` 自测。**未完成**：属新增范围，本轮以证据目录
  `report.md` §5 的「最小输入 + 预期结果」固化复跑，不移动脚本。
  - **2026-11-12 用户裁决与落地（决定 §42）**：授权新增范围。`scripts/run_window_failure_probe.py` / `scripts/run_signal_probe.py` 已迁入，必要 Swift 源 `scripts/window_list.swift` / `scripts/signal_probe.swift` 一并移入；两者新增 `--self-test`（窗口侧 17 例、信号侧 8 例）并接入 `check_workspace.py` 顺序清单与 `check_validation_output.py` 输出路径拒绝矩阵。门禁 `check_workspace.py --cargo-home <隔离缓存> --output-dir <logs>` 37/37 全部 exit 0；`check_validation_output.py` 14/14。自测只证明文案分类/信号解析与证据解释逻辑，不执行真实沙箱或窗口实验；既有真实收据结论不变。证据 `logs/validation/m1-single-window-readonly/2026-11-12-startup-4_3/`。
  - **2026-11-12 独立复审与修复（不改写上句）**：独立复审结论 `pass-with-findings`；4 条中级 findings 已修复：窗口探针 baseline 与 case 的 `window_probe_run` 语义统一并新增 `window_probe_ok`，字段缺失或类型不符不再默认成 0 窗口；自测绑定实际 locale 文件与剖面独立字面量，并固定用例名单（自测增至窗口 32 例、信号 11 例），删除或新增用例而不更新名单会失败；真实模式原始 stdout/stderr、excerpt 与 `results.json` 统一经 `evidence_paths` 规范化；两个 Python 入口与两个 Swift 源随本次提交纳入 git。复审另指出低severity 项已一并处理（进程清理 `finally`、`SystemExit` 改 `ValueError`、Swift 注释与信号 profile 摘要、输出目录唯一性说明）。修复后复跑：`check_workspace.py` 37/37、`check_validation_output.py` 14/14、窗口自测 32/32、信号自测 11/11。复跑证据 `logs/validation/m1-single-window-readonly/2026-11-12-startup-4_3-r2/`。自测仍不执行真实沙箱或窗口实验，既有真实收据结论不变。
