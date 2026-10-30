# 2.4 Catalog 与最小应用名消费验证

## 范围

在 `aline-app-core` 中加入实例级 `Locale`、`Translator` 与 `CatalogError`；使用已准入的 `toml 1.1.8` 和 `rust-embed 8.13.0` 读取编译嵌入的双语资源。当前资源仅包含 `app.name`。`aline-ui::application_name` 将查询结果转换为 `gpui::SharedString`；app 入口消费该结果并返回 `ExitCode`，不创建窗口、不打印错误或文案。

实现与测试位于 [i18n.rs](../../../../crates/aline-app-core/src/i18n.rs)、[aline-ui/src/lib.rs](../../../../crates/aline-ui/src/lib.rs)；资源为 [en.toml](../../../../crates/aline-app-core/locales/en.toml) 与 [zh-CN.toml](../../../../crates/aline-app-core/locales/zh-CN.toml)。catalog schema 错误使用 `EmptyCatalog`、`InvalidKey`、`InvalidValueType` 具体变体；TOML/UTF-8 错误保留原错误并由 `Error::source` 暴露。

## 环境和命令证据

宿主环境为 macOS `aarch64-apple-darwin`。命令记录均包含完整 argv、cwd、环境覆盖、开始/结束时间、退出码及 stdout/stderr SHA-256；对应完整输出分别保存在同目录的 `.stdout.log` / `.stderr.log`。环境使用已记录的隔离 Cargo home：`/var/folders/rx/cwp1dm6536789l30qt9hf9rm0000gn/T/aline-m0-admission-k4_p5j6a/cargo-home`，固定 `RUSTUP_TOOLCHAIN=1.98.1`。来源记录见 [isolation.json](../pure-candidates-2026-10-30/isolation.json)。所有 Cargo 依赖命令均带 `--locked --offline`。

| 检查 | 完整 argv | 退出码 | 结果 |
|---|---|---:|---|
| Cargo 版本 | `cargo -V` | 0 | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |
| rustc 版本 | `rustc -Vv` | 0 | `1.98.1`，host `aarch64-apple-darwin`，LLVM `22.1.8` |
| rustfmt 版本 | `rustfmt --version` | 0 | `rustfmt 1.9.0-stable` |
| Clippy 版本 | `clippy-driver --version` | 0 | `clippy 0.1.98` |
| 格式 | `cargo fmt --all --check` | 0 | 无差异 |
| lint | `cargo clippy -p aline-app-core -p aline-ui -p aline --all-targets --locked --offline -- -D warnings` | 0 | 无警告 |
| app-core debug | `cargo test -p aline-app-core --locked --offline` | 0 | 5 passed |
| app-core release | `cargo test -p aline-app-core --release --locked --offline` | 0 | 5 passed |
| UI 消费 | `cargo test -p aline-ui --locked --offline` | 0 | 1 passed |
| app 入口 | `cargo run -p aline --locked --offline` | 0 | 程序正常退出，无窗口或输出 |

逐命令记录与完整输出：[`catalog-evidence/`](catalog-evidence/)。

## 行为用例与独立预期

实际运行的 app-core 单元用例在 debug 与 release 都通过；UI 应用名用例通过。用例逐项断言了：

- `rust-embed` 返回的资源为 `Cow::Borrowed`，嵌入英文资源可解析并查到 `app.name`。
- 键集合不一致、两种语言占位符集合不一致、缺失或非法花括号、无效 UTF-8、坏 TOML、空 catalog、非法层级键、非字符串叶值、非法占位符分别返回指定 `CatalogError`；UTF-8 与 TOML 错误保留原错误类型并可由 `Error::source` 取得。
- 未知键、缺少参数、多余参数、重复参数返回明确错误；合法插值替换成功。占位符顺序不同但集合相同可以构造成功。
- 两个 `Translator` 实例可各自选择语言；依次切换两侧后，断言另一实例的语言与翻译都未改变。
- 重复使用同一占位符会在每个位置替换；非 ASCII 参数值保持原样输出。
- UI helper 确实把嵌入 catalog 查到的名称转为 `gpui::SharedString`；app 入口执行该消费并以状态码结束。

独立预期来自 M0 任务 2.4 对双语 catalog、显式 locale、类型化错误、插值与真实消费的约束，以及 [engine-design §10.1](../../../engine-design.md) / [v1-architecture §8.2、§10.1](../../../v1-architecture.md) 关于从 M0 使用 `t`、显式 locale、缺键失败和禁止引擎全局语言的决定。`rust-embed` 的已准入消费证据与 `debug-embed` 选择见 [候选审查报告](../pure-candidates-2026-10-30/report.md)。正反结果由测试中的明确 `assert!` / 错误变体模式独立判定，未将编译失败或其它非预期失败当作拒绝通过。

## 未验证项

- 仅在本机 macOS arm64 实际构建/运行；Windows、Linux、macOS Intel 未验证。
- V-01～V-09 未执行，不能据此声称通过。
- 当前两个 app 名称都为 `Aline`，不是完整界面文案覆盖，也未实现系统 locale 探测、设置持久化或 UI 全局语言传播。
