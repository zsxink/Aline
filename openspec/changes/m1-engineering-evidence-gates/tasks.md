# Tasks

## 1. 统一入口与拒绝证据

关联 QMD W7、R-17、S-2，使用现有 workspace 状态所有者。

- [x] 1.1 在 `check_workspace.py` 接入 M1 汇总器/收集器自测及 UI token 正反例；增加失败后继续执行的自测，输出目录拒绝覆盖新增 CLI。验收 `python3 scripts/check_workspace.py --self-test`、`python3 scripts/check_validation_output.py` 及各接入脚本自身自测，原始输出经路径工具留本地。**2026-11-08：** 统一入口使用隔离缓存运行 27 项通过，含上述自测与输出目录 11 项；记录在 `logs/validation/m1-single-window-readonly/2026-11-08-current/workspace-final/`。此前两次失败保留，不把它们改写为通过。

2026-11-09 补充接线：同一 workspace 入口新增 `sample_m1_performance.py --self-test`、`capture_m1_readonly_actions.py --self-test`，不启动真实窗口。输出路径拒绝矩阵新增两个 CLI 的普通/自测入口与采样器 `--worker` 越界反例；本轮输出路径自测 12 项、采样器收据/RSS/超时合成自测、真实回调记录状态自测通过。它们只证明校验和拒绝机制，不是性能/GUI 操作证据；原始输出在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/new-entrypoints/`，完整 workspace 仍由 3.2 单独记录。

W1 runner 稳定后，同一入口再接 `run_w1_offscreen.py --self-test`，并为普通/自测入口补输出越界反例；局部自测 18 项、输出目录自测 12 项通过，证据在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/w1-entry/`。此接线不执行真实离屏构建，不把候选锁或准入配置接入生产图，真实能力结论仍由 W1 负责。

## 2. 本地化与失败可见

关联 QMD W7、AGENTS 铁律 7、S-7/11；不改变引擎 panic 规则。

- [x] 2.1 补用户可见 UI 源码串/字面 key 的静态检查与正例、最小硬编码/缺 key 反例，接入现有统一入口；现有 catalog 同键/占位符测试继续运行。明确宏和动态串的人工复核边界，更新现有文档入口；验收守卫正反例、实际源码检查及 app-core catalog 测试。
- [ ] 2.2 核对 UI 启动错误和 panic 最后兜底的实际边界，确认 CLAUDE 仅指向 AGENTS、无 vendor 时不生成补丁清单；必要结论回填现有验证计划，不引入 catch_unwind 放宽引擎限制。验收源码/文档引用复核和真实失败路径所需平台证据，缺证据保留未完成。

2026-11-08 本轮验证：静态检查补充可访问名称和窗口标题字面量反例，catalog 自测 6 项、实际 app/ui 源码扫描、输出目录自测 11 项、workspace 失败继续执行自测均通过；`cargo test -p aline-app-core --locked --offline` 通过。命令、stdout/stderr 与边界复核在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/`。后续已核对 `docs/README.md` 的统一守卫入口、静态扫描盲区和双语测试说明，2.1 关闭。2.2 的真实 macOS 初窗失败路径尚无运行证据；静态确认普通启动错误经本地化 stderr/exit 1，未新增 panic hook/catch_unwind，`CLAUDE.md` 仍仅指向 AGENTS，无 vendor，因此不生成补丁清单。静态结论不替代该项要求的平台失败证据，保持未完成。

2026-11-09 集成修正：完整 workspace 发现点分证据文件名被误报为缺 key，已将 key 检查限定到真正翻译调用的字面首参，补非翻译调用文件名正例与真实拼错 key 反例；catalog 自测增至 7 项，当前 app/ui 源码扫描通过。证据 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w7/catalog-callsite-fix/`；该次 workspace 原失败保留，3.2 等修正后全量重跑，不覆盖历史结论。

## 3. 真实报告与集成

关联 QMD W7、R-11-01、S-14-03、V-05；合成正例不替代真实报告。

- [ ] 3.1 以 M1 7.1 真实 Release 报告逐字段核对环境、commit/锁定构建、fixture SHA、方法、冷热、真实计时边界、P50/P95、最长 UI 停顿、峰值内存、raw、复现命令和不测量项；更新 V-05 摘要，缺项失败，不冻结新门槛。
- [x] 3.2 运行完整 `check_workspace.py` 与 OpenSpec 严格校验，复核八成员 lint 继承、无第二套门禁/文档正文、证据路径和实际平台限制；统一入口失败不勾选。依赖缓存与真实平台缺口如实记录，不自动安装或下载。

2026-11-09 主 Agent：统一 workspace-final-02 共29项全部通过，OpenSpec strict 15/15；后续 W1 runner 自测已补接，最终复跑单列。UI/catalog 误报文件名已通过翻译调用首参的精确检查修复，未按文件名豁免。W7 3.1 仍未完成：360次部分性能矩阵已采到真实分段/RSS/收据，但缺真正屏幕呈现和精确UI停顿；2.2 仍缺真实初窗失败路径。

2026-11-09 最终统一入口（含W1离屏runner自测）workspace-final-03 的30项全部通过，fmt/Clippy/workspace test/真实依赖及正反例守卫均exit0。首次计时时钟分辨率包络失败与catalog文件名误报保留原日志，修复后通过；不覆盖历史失败。
