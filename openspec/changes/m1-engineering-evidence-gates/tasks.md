# Tasks

## 1. 统一入口与拒绝证据

关联 QMD W7、R-17、S-2，使用现有 workspace 状态所有者。

- [x] 1.1 在 `check_workspace.py` 接入 M1 汇总器/收集器自测及 UI token 正反例；增加失败后继续执行的自测，输出目录拒绝覆盖新增 CLI。验收 `python3 scripts/check_workspace.py --self-test`、`python3 scripts/check_validation_output.py` 及各接入脚本自身自测，原始输出经路径工具留本地。**2026-11-08：** 统一入口使用隔离缓存运行 27 项通过，含上述自测与输出目录 11 项；记录在 `logs/validation/m1-single-window-readonly/2026-11-08-current/workspace-final/`。此前两次失败保留，不把它们改写为通过。

2026-11-09 补充接线：同一 workspace 入口新增 `sample_m1_performance.py --self-test`、`capture_m1_readonly_actions.py --self-test`，不启动真实窗口。输出路径拒绝矩阵新增两个 CLI 的普通/自测入口与采样器 `--worker` 越界反例；本轮输出路径自测 12 项、采样器收据/RSS/超时合成自测、真实回调记录状态自测通过。它们只证明校验和拒绝机制，不是性能/GUI 操作证据；原始输出在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/new-entrypoints/`，完整 workspace 仍由 3.2 单独记录。

W1 runner 稳定后，同一入口再接 `run_w1_offscreen.py --self-test`，并为普通/自测入口补输出越界反例；局部自测 18 项、输出目录自测 12 项通过，证据在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/w1-entry/`。此接线不执行真实离屏构建，不把候选锁或准入配置接入生产图，真实能力结论仍由 W1 负责。

2026-11-10 补充接线：M1 7.1 的三个 external-trace 入口 `m1_trace_metrics.py`、`m1_trace_record.py`、`m1_rederive_performance.py` 的 `--self-test` 已接入同一顺序清单（label `m1-trace-metrics-self-test` / `m1-trace-record-self-test` / `m1-rederive-performance-self-test`，仍单条失败不短路）；输出路径拒绝矩阵同时覆盖三者的普通与自测入口。守卫新增 10 项违规覆盖矩阵（伪 present、缺 present request、display 早于 commit、锁屏/显示器睡眠、run_id/app_sha256/pid 身份漂移、重推跨来源身份不一致、补采编号重复、输出目录越界），要求每项的反例与正例都在对应脚本的结构化自测记录里被标成 reject/accept 并通过，另以桩记录自证矩阵会拒绝缺项与伪造记录；输出路径自测 14 项、workspace 33 项通过。证据 `logs/validation/m1-single-window-readonly/2026-11-10-71-closeout-01/`。该接线只证明校验与拒绝机制，不是性能或 GUI 操作证据；2.2 仍缺真实初窗失败路径，本轮未倒填。

2026-11-11 缺陷修复（主 Agent）：统一门禁暴露 `m1-performance-sampler-self-test: exit=1`，stderr 为 `performance: [Errno 1] Operation not permitted`。根因是 `scripts/sample_m1_performance.py` 的子进程清理路径只捕获 `ProcessLookupError`，而 macOS 在进程组 leader 已退出但未被 wait 时对 `killpg` 返回 `EPERM`；该异常从清理分支逃逸并顶替掉原始失败原因（`trace marker 未绑定目标 pid`）。同类漏捕共三处：`_release_gate` 与 `sampled()` 的 `os.killpg`、`_kill_pid` 的 `os.kill`；修复为三处统一收敛 `PermissionError`，两处 `killpg` 逻辑提取共用辅助 `_kill_process_group()`。修复前真实时序复现：错误 pid marker 场景 400 次中 142 次（35.5%）抛 `PermissionError`，修复后同脚本 400 次全为预期的 `ValueError`；频率受时序影响波动很大，早期一次 60 次采样只出现 2 次，能稳定暴露它的是它已接入统一门禁。回归有效性：逐处退回旧行为后对应自测立即失败（基线 46 passed / 退回 A exit 1 / 退回 B exit 1 / 恢复后 46 passed）。采样器自测由 38 项增至 46 项，新增用例以"先真实终止进程组再抛 EPERM"注入并以调用计数断言清理分支确实被走到，避免 helper 提前退出导致假通过。统一门禁 33 项全部 exit 0，OpenSpec strict 16/16 通过；M1 7.1 标准入口复跑仍 exit 0、`passed: true`、144 组与 3,240 行及最长停顿 93.479 ms、峰值 RSS 1,036,763,136 bytes、虚拟实例上限 29 逐字未变，未重录矩阵、未改测量口径与门槛。证据 `logs/validation/m1-single-window-readonly/2026-11-11-71-killpg-eperm/`。本轮只修清理契约缺陷，不改"不把 draw 提交称为屏幕呈现、不把事件循环抖动称为精确主线程停顿"的既有口径；7.3、8.2、V-09 整项、M0 豁免与 M2 前 V-08 保存闭环仍未关闭，不表示 M1 出口通过。

## 2. 本地化与失败可见

关联 QMD W7、AGENTS 铁律 7、S-7/11；不改变引擎 panic 规则。

- [x] 2.1 补用户可见 UI 源码串/字面 key 的静态检查与正例、最小硬编码/缺 key 反例，接入现有统一入口；现有 catalog 同键/占位符测试继续运行。明确宏和动态串的人工复核边界，更新现有文档入口；验收守卫正反例、实际源码检查及 app-core catalog 测试。
- [ ] 2.2 核对 UI 启动错误和 panic 最后兜底的实际边界，确认 CLAUDE 仅指向 AGENTS、无 vendor 时不生成补丁清单；必要结论回填现有验证计划，不引入 catch_unwind 放宽引擎限制。验收源码/文档引用复核和真实失败路径所需平台证据，缺证据保留未完成。
  - [x] 2.2a 源码/文档静态边界复核：2026-11-11 复核 `app/src/main.rs` 普通启动错误走本地化 `eprintln!` + `ExitCode::FAILURE`，`crates/aline-ui/src/native.rs` 初窗失败 `eprintln!` + `exit(1)`；`CLAUDE.md` 仅指向 AGENTS；无 `vendor/`、无 `[patch]` 本地来源、无 `catch_unwind`、无 `panic::set_hook`。静态复核不替代真实失败路径证据。
  - [x] 2.2b 必要结论回填：`docs/v1-validation-plan.md` 已记录静态边界（初窗失败立即以状态码 1 退出）与「真实初窗失败尚无平台收据」，未把静态结论写成平台通过。
  - [x] 2.2c 真实 macOS 初窗失败路径平台证据：按 2026-11-11 用户裁决（决定 §34）接受“静态边界证据 + 显式缺口”闭环。静态边界见 2.2a/2.2b；真实 macOS 初窗失败运行收据仍未取得，缺口保留，不把静态结论写成平台通过。本项只关闭记录口径，不证明真实失败路径。

2026-11-08 本轮验证：静态检查补充可访问名称和窗口标题字面量反例，catalog 自测 6 项、实际 app/ui 源码扫描、输出目录自测 11 项、workspace 失败继续执行自测均通过；`cargo test -p aline-app-core --locked --offline` 通过。命令、stdout/stderr 与边界复核在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/`。后续已核对 `docs/README.md` 的统一守卫入口、静态扫描盲区和双语测试说明，2.1 关闭。2.2 的真实 macOS 初窗失败路径尚无运行证据；静态确认普通启动错误经本地化 stderr/exit 1，未新增 panic hook/catch_unwind，`CLAUDE.md` 仍仅指向 AGENTS，无 vendor，因此不生成补丁清单。静态结论不替代该项要求的平台失败证据，保持未完成。

2026-11-09 集成修正：完整 workspace 发现点分证据文件名被误报为缺 key，已将 key 检查限定到真正翻译调用的字面首参，补非翻译调用文件名正例与真实拼错 key 反例；catalog 自测增至 7 项，当前 app/ui 源码扫描通过。证据 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/catalog-callsite-fix/`；该次 workspace 原失败保留，3.2 等修正后全量重跑，不覆盖历史结论。

## 3. 真实报告与集成

关联 QMD W7、R-11-01、S-14-03、V-05；合成正例不替代真实报告。

- [x] 3.1 以 M1 7.1 真实 Release 报告逐字段核对环境、commit/锁定构建、fixture SHA、方法、冷热、真实计时边界、P50/P95、最长 UI 停顿、峰值内存、raw、复现命令和不测量项；更新 V-05 摘要，缺项失败，不冻结新门槛。
- [x] 3.2 运行完整 `check_workspace.py` 与 OpenSpec 严格校验，复核八成员 lint 继承、无第二套门禁/文档正文、证据路径和实际平台限制；统一入口失败不勾选。依赖缓存与真实平台缺口如实记录，不自动安装或下载。

2026-11-09 主 Agent：统一 workspace-final-02 共29项全部通过，OpenSpec strict 15/15；后续 W1 runner 自测已补接，最终复跑单列。UI/catalog 误报文件名已通过翻译调用首参的精确检查修复，未按文件名豁免。W7 3.1 仍未完成：360次部分性能矩阵已采到真实分段/RSS/收据，但缺真正屏幕呈现和精确UI停顿；2.2 仍缺真实初窗失败路径。

2026-11-09 最终统一入口（含W1离屏runner自测）workspace-final-03 的30项全部通过，fmt/Clippy/workspace test/真实依赖及正反例守卫均exit0。首次计时时钟分辨率包络失败与catalog文件名误报保留原日志，修复后通过；不覆盖历史失败。

2026-11-10 W7 3.1 逐字段核对通过：以 `logs/validation/m1-single-window-readonly/2026-11-10-performance-01/performance/summary.md`、`summary.json` 与 `samples.jsonl` 核对环境（macOS 27.2 arm64、32 GiB、`gpui-pre`/`gpui-pre-macos` 0.3.9、rustc/cargo 1.98.1、Xcode 27.2）、commit `1893d4ac…`/Release 构建 `06740b89…`、9 类 fixture SHA-256 与字节数、方法（同钟 Instant 边界；`first_present` 为 Metal System Trace 显示链，`main_thread_stall_ns` 为 runloop 扣除 `waiting_for_events` 的最长连续忙碌段）、冷热定义、trace 相对计时边界与 1 ms 起始不确定度、144 组 P50/P95、最长主线程停顿 93.479 ms、峰值 RSS 1,036,763,136 bytes、raw 入口与逐用例 `raw_sha256`、复现命令、未测项，均无缺项。V-05 已更新（`docs/v1-validation-plan.md` 2026-11-10 条）。未冻结新性能门槛，合成正例未替代真实报告。3.1 勾选只表示字段核对完成，不表示 M1 出口或完整 AC-16；2.2 的真实初窗失败路径仍缺平台证据，保持未完成。
