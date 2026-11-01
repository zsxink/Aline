# R11 不可复现项登记（unverified）

本轮只在本机 macOS / Apple Silicon 主机上取证。下列项目**无法**由本轮补采，
按 tasks.md §5 记为「证据不足」，不得因本文件或本轮新证据升级为「已核实」。

| # | 不可复现项 | 缺什么设备/权限 | 本轮为什么不能补 | 受影响的原任务 |
|---|---|---|---|---|
| U-1 | Intel Mac（`x86_64-apple-darwin`）实机取证 | 无 Intel Mac 设备；也无可用 Intel 虚拟化/远端 mac 主机 | 本机为 Apple Silicon；本轮禁止联网、禁止安装工具与新建远端环境。已装 target 只证明组件存在 | 1.1 设备表（Intel 行只能标「缺设备」）、2.5 CI 目标覆盖、V-01 Intel 分项 |
| U-2 | Windows 10 `x86_64` 实机取证 | 无 Windows 设备、无 Windows VM、无 Windows CI 运行 | 同上；本轮不启动远端 CI，也不创建 VM | 1.1 设备表、2.5 CI 未实际运行、V-01 Windows 分项 |
| U-3 | Linux（`x86_64`/`aarch64`，含 X11 与 Wayland）取证 | 无 Linux 设备/VM；无 X11 或 Wayland 会话 | 同上。旧 `tree-linux-*` 记录只有依赖树解析结果，不含运行取证 | 1.1 设备表、1.3 目标组合解析、V-01 Linux 分项 |
| U-4 | 远端 CI 实际运行 | 需远端 runner 执行与可访问的 CI 记录 | `.github/workflows/m0.yml` 存在，但本轮不得联网触发/读取远端运行结果；`git log origin/main..HEAD` 显示 5 个提交仍未推送，远端不会包含本轮改动 | 2.5「CI 最小接线」的**执行**部分（结构配置存在 ≠ 已运行） |
| U-5 | 旧候选构建的原始时点 | 需当时的采集时点；不可逆 | 旧 `safety-*` / `tree-*` 记录缺 `started_utc`/`finished_utc`，时点无法追认，补造即伪造 | 1.2 候选组合验证的时间链、F-13 相关条目 |
| U-6 | 旧设备/显示观察的原始输出与采集时点 | 需当时的 `system_profiler` 原始输出 | README 设备表未保全原始输出与时点。本轮只能给**当前**观察，不能替代旧记录 | 1.1 实际观察行、F-13 设备观察部分 |
| U-7 | 旧 `environment-commands.json` 的 `cwd` 与时点 | 需当时的采集上下文 | 12 条只有 argv/exit/stdout，无 cwd 与时点 | 1.2 工具链登记的时间链 |
| U-8 | 隔离候选目录的**重建**验证 | 需重新 `cargo` 拉取/构建（联网） | 本轮只做只读观察：候选根与 `cargo-home` 当前仍在，但 registry 缓存为空（仅 `CACHEDIR.TAG`），未重建、未重新准入 | 1.2/1.3 的候选重建与准入（本轮只做身份摘录） |
| U-9 | 五引擎跨平台无头构建 | 需 Windows/Linux 目标运行环境 | 本轮不执行 build/clippy/test（属 R14 统一门禁） | 2.1 逐包无头构建的跨平台部分 |

## 本轮明确**不**做的事

- 不回填任何旧 `*.command.json` 的 `cwd`/`started_utc`/`finished_utc`。
- 不在 `docs/validation/m0-engine-foundation/README.md` 把旧缺口写成「已修复」；
  只在其末尾追加新证据链接与勘误说明。
- 不运行 `cargo update`、不 `rustup update`、不 `rustup component add`、不联网。
- 不声称任何 V-01～V-09 通过；不把 M0 判为闭合。
