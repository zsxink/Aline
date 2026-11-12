# Proposal

## Why

2026-11-12 受控失败实验（`logs/validation/m1-single-window-readonly/2026-11-12-startup-failure/report.md`）
证明了一个当前真实存在的产品质量缺口：当 macOS WindowServer 不可达时，锁定后端
`gpui-pre-macos 0.3.9` 的 `MacWindow::open` 返回 `Self` 而非 `Result`，`NSWindow` 仍被构造
成功但从未注册进系统，`App::open_window` 因此不返回 `Err`。结果是产品**无窗口、无提示、
进程不退**，用户面对一个看不见也退不掉的后台进程。

现有失败出口（`native.rs` 的 `report_startup_failure`：本地化 `eprintln!` + `exit(1)`）
只在 `open_initial_window` 返回 `Err` 时触发，而该分支在 `WindowKind::Normal` 下结构不可达。
本变更在 `open_window` 之外补一条**可观察的**失败检测，使产品在该环境下给出本地化提示并
以非零状态退出，从而关闭 2026-11-12 记录的质量缺口。

依据：`docs/decisions.md` §35「执行结果」段与「2026-11-12 追加裁决」；AGENTS 铁律 7
（用户可见字符串走 `t("key")`）、`docs/ui-spec.md` §14 启动与本地化规则。

## What Changes

- 在 `crates/aline-ui/src/native.rs` 于 `Application::run` **之前**检查活跃显示数
  （`Platform::displays()`）：首查为 0 时 `std::thread::sleep` 约 2 秒复检，仍为 0 才输出
  本地化 `startup.no_display` 并以状态码 1 退出。
- 检测点前移到 `run` 之前的理由是**可观察行为**：该条件下只能观测到「无窗口、无 stderr、
  进程不退」，无法区分启动回调是否执行过；因此选一个一定执行得到的检测点，不假设回调
  不会被调用（见 design §「启动回调是否执行」）。
- 失败出口只输出本地化文案，不再拼接英文技术诊断（铁律 7）；既有 `window_failed` 分支
  同一口径。
- 复检不引入新依赖、不引入 `unsafe`、不加平台钩子、不改 `run_application` 签名。
- 检测与既有 `open_initial_window` 的 `Err` 分支互补：前者覆盖「后端不报错但显示不可用」，
  后者覆盖「后端确实报错」。
- 新增 catalog 键 `startup.no_display`（因果中性的「未检测到可用显示器，未显示窗口。」），
  避免把「窗口打开失败」与「无可用显示」混为一谈。
- 回填 `docs/ui-spec.md` §14 与 `docs/v1-validation-plan.md` V-09 的对应分项，并在
  `openspec/changes/m1-engineering-evidence-gates/tasks.md` 2.2c 追加指向本变更的说明。

非目标：不改 `gpui-pre` 锁定依赖、不 vendor、不新增 `WindowKind::AnchoredPopup` 路径、
不改产品清单/锁/策略资产、不引入 panic hook 或 `catch_unwind`、不处理「显示器睡眠后唤醒」
的运行时恢复（只处理启动期显示不可用）。

## Capabilities

### New Capabilities

无。启动失败反馈属现有 UI §14 启动与本地化规则的行为补全，使用 `skip_specs: true`。

### Modified Capabilities

无。不改变现行规格的 requirement 或 Scenario；本变更是把已声明的「启动失败有反馈」
在锁定后端下真正落地。

## Impact

影响 `crates/aline-ui/src/native.rs`、`crates/aline-app-core/locales/{zh-CN,en}.toml` 与
`crates/aline-app-core/tests/m1_window_catalog.rs`；不改公开 API（`run_application` 签名不变）、
不改引擎 crate、不改 Cargo 依赖图。遵循 CODE_STANDARDS §2.2（UI 编排副作用）、§3.2
（默认私有，不新增公开面）、AGENTS 铁律 7/9。风险：显示器睡眠期误判导致启动即退出——
用延迟复检缓解，并在证据中记录该边界。平台结果只适用于实际取证的 macOS arm64，
Windows/Linux 保留未测。
