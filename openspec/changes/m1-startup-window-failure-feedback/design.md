# Design

## Context

见 proposal 的动机。2026-11-12 受控失败实验（`2026-11-12-startup-failure/report.md` §3）
沿锁定依赖逐层核对后确认：`Window::new` 全函数唯一可失败点是 `platform.open_window`；
`MacPlatform::open_window` 唯一 `Err` 出口是 `WindowKind::AnchoredPopup`；`MacWindow::open`
返回 `Self`。Aline 用 `WindowKind::Normal`，因此 `open_initial_window` 的 `Err` 分支在
锁定后端下结构不可达。产品于是既不报错也不退出。

判别信号由独立同进程 Swift 探针实测（`logs/validation/.../2026-11-12-window-failure/signal-probe/`，
源码 `signal_probe.swift`、运行入口 `run_signal_probe.py`、原始输出 `run-1/results.json`）：

| 剖面 | `CGGetActiveDisplayList` 计数 | `NSScreen.screens` | `NSWindow.occlusionState.visible` | `windowNumber` |
|---|---|---|---|---|
| 基线 | 1 | 1 | false | 11513 |
| `deny-windowserver` | 0 | 0 | false | 0 |
| `deny-mach`（全部） | 0 | 0 | false | 0 |

关键结论：`occlusionState`（GPUI 的 `Window::is_visible()` 依据）在基线和失败态**都是 false**，
不是判别信号；而 `CGGetActiveDisplayList` 在失败态为 0、基线为 1，是干净的判别信号。
GPUI 已把它包装成 `App::displays()`，其 macOS 实现 `MacDisplay::all()` 的文档明确写着
"Empty if the system can't list them, e.g. without a window server connection"。

探针是独立进程里的 `NSWindow`，不是 Aline 自己的窗口，也不证明 GPUI 启动回调是否执行。

## Goals / Non-Goals

**Goals:** 让「启动期显示不可用」产生本地化提示并以非零状态退出；不新增依赖、
不新增公开 API、不引入 `unsafe`。

**Non-Goals:** 不修 GPUI 后端、不 vendor、不改 `WindowKind`、不处理运行中显示器热插拔
或睡眠唤醒后的窗口恢复；不把该检测当作「窗口确实可见」的证明；不证明「显示枚举非空
⇒ 窗口服务可用」（这是本检测的漏检方向）。

## Decisions

1. **信号选择：`Platform::displays().is_empty()`。** 这是锁定 GPUI 已公开的 API，语义与实测
   一致（见上表），且不依赖任何平台 FFI。不选 `Window::is_visible()`：实测基线也为 false，
   会把正常启动误判为失败。

2. **延迟复检而非立即判定。** 显示器睡眠、Spaces 切换或远程会话建立初期，显示枚举可能
   瞬时为空。立即退出会把「暂时看不到」误判成「服务不可用」。做法：若首查 `displays()` 为空，
   `std::thread::sleep` 约 2 秒后再查一次；仍为空才判定失败。2 秒是本轮选定值，不是平台常量，
   记录在证据里。

3. **失败动作复用既有出口。** 沿用 `native.rs` 已有的 `report_startup_failure`：本地化
   `eprintln!` + `std::process::exit(1)`。这样两条失败路径（后端报错 / 显示不可用）
   对进程外观察者表现一致，不新增第二套退出语义。失败出口**只输出本地化文案**，不拼接
   英文技术诊断——AGENTS 铁律 7 要求用户可见字符串走 `t("key")`；既有 `window_failed` 分支
   同一口径（2026-11-12 裁决）。

4. **诊断文案区分两种失败，且不归因。** 现有 `startup.window_failed` 是「无法打开应用窗口」；
   显示不可用的根因不同，新增 `startup.no_display`。文案为**因果中性**的「未检测到可用显示器，
   未显示窗口。」：代码只能观测「活跃显示数为 0」，不能断言「窗口服务不可达」（显示器睡眠、
   Spaces 切换、远程会话建立初期都可能瞬时为空，此时归因于窗口服务是错误因果）。两者都走
   `t("key")`，双语 catalog 同步。

5. **检测放在 `Application::run` 之前。** 理由是可观察行为，不是回调执行与否：该条件下只能
   观测到「无窗口、无 stderr、进程不退」，这组事实对「回调从未执行」和「回调执行了但后端
   不报错」都成立，现有手段无法区分。因此选一个一定执行得到的检测点。顺序是：先查显示，
   再进 `run`；`open_initial_window` 的 `Err` 分支保持原样，两条路径互不掩盖。

### 启动回调是否执行：不可判定

2026-11-12 首版曾把「启动回调根本不会被调用」写成实测结论，并据此论证前移。独立复审判定
该断言无埋点支撑，且与上一轮报告 §3 的结构性解释（`NSWindow` 构造成功、窗口未注册）互斥。
两轮观测对两种机制都成立。按用户裁决，两轮报告统一降级为「无法判定回调是否执行」，
前移理由改为「选一个一定执行得到的检测点」。

## Risks / Trade-offs

- [显示器睡眠期误判] → 2 秒延迟复检；若仍误判，用户看到的是本地化提示与退出码 1，
  可重启。证据中记录该边界，不宣称「不会误判」。
- [2 秒拖慢正常启动] → 只有首查 `displays()` 为空才进入等待；正常启动不进入该分支，
  实测基线 0.71 秒内出窗（run-3）。
- [检测不等于窗口可见] → 本变更只证明「显示枚举可用」，不证明窗口真的出现在屏幕上；
  不据此勾选任何真实窗口证据项。漏检方向：显示枚举非空但窗口服务不可用时，行为与修复前一致。
- [反馈形态依赖终端启动] → M1 无 `.app` 包，stderr 只在终端启动时可见；GUI 双击启动路径
  未覆盖。
- [平台差异] → `Platform::displays()` 是 GPUI 跨平台 API，但本轮只在 macOS arm64 取证；
  Windows/Linux 未测，不写成三平台通过。

## Migration Plan

先加 catalog 键与测试，再改 `native.rs`，最后跑 fmt/Clippy/test/守卫并做真实窗口复核：
正常启动应无提示、无额外延迟；`deny-windowserver` 剖面应出现本地化提示且退出码为 1。
失败在本地修复；不改引擎、不改生产清单/锁/策略资产（本变更不涉及依赖变更）。
