# 纯引擎依赖候选审计

日期：2026-10-30。状态：审计探针，**不是正式 Cargo workspace，也不表示五个正式引擎 crate 已存在**。候选依据为 `AGENTS.md`、`docs/engine-design.md` §18.2 和 `docs/v1-architecture.md` §3.2 的依赖边界。工具链和完整命令见 [commands.md](commands.md)。

## 当前候选

| 使用方 | 精确候选 / crates.io 来源 | 许可证 | feature 选择与消费证据 | 功能替代 |
|---|---|---|---|---|
| `aline-markdown` | `pulldown-cmark 0.13.6`；crates.io；Cargo.lock checksum `3db2be46835f013dd3f4fa7f5f7402c537787b54ebb6fe2b94bbdd7e2669bb7b` | MIT | `default-features = false`，不启用 `html`/HTML 输出后端。探针实际调用 `Parser::new(source).count()` 消费事件；`cargo run -p aline-markdown-probe --example probe-consume` 输出 `pulldown-cmark events=6`（exit 0）。crate 默认 feature 是 `getopts, html`。 | 不引入第二个 Markdown parser；缺失语法按现行设计由保留映射的预处理补足。 |
| `aline-app-core` | `toml 1.1.8+spec-1.1.0`（manifest requirement `=1.1.8`；semver 版本匹配忽略 `+spec-1.1.0` build metadata，真实解析包身份由 Cargo.lock 锁为 `1.1.8+spec-1.1.0`）；crates.io；checksum `20489e00e4d8741d6be680764cc12e270655e375a20d1011e844a9c3379e678d` | MIT OR Apache-2.0 | `default-features = false, features = ["std", "parse", "serde"]`。探针解析 rust-embed 取得的 TOML 为 `toml::Table` 并读取字符串。默认另启 `display`，本探针不需要。 | 不额外引入另一 TOML parser；使用单一 TOML crate 解析双语目录。 |
| `aline-app-core` | `rust-embed 8.13.0`；crates.io；checksum `19afa5b4b6a611de00bd1bdae6ae6f39084c9399f0679c3f52d8469cf335cc23` | MIT | `default-features = false, features = ["debug-embed"]`。通过 `#[derive(RustEmbed)]` 从资产目录嵌入中文 `zh-CN.toml`，再交由 toml 解析；debug 构建运行 `cargo run -p aline-app-core-probe --example probe-consume` 输出 `embedded title=Aline`（exit 0），证明开发配置实际取到嵌入的目录内容。随后源码复核将错误路径改成类型化 `Result`；修订后 check、release build 和运行仍 exit 0。启用 `debug-embed` 是为使开发配置也走嵌入代码路径，避免 debug 下 `RustEmbed::get` 读取资源文件系统。 | `include_str!` 可逐个静态包含文件；资源目录采用 derive 宏批量嵌入。未另行准入替代包。 |

Cargo 对上述注册表依赖均解析为 `registry+https://github.com/rust-lang/crates.io-index`。包元数据和许可证来自 `cargo info`；版本/checksum/source 来自锁文件。主次依赖真实版本、feature 和边、目标种类及路径见 [dependency-ledger.json](dependency-ledger.json) 与 [Cargo.lock](Cargo.lock)。

## 实际消费和闭包

隔离探针含五个名为 `*-probe` 的 crate：`aline-source-probe`、`aline-buffer-probe`、`aline-edit-probe` 只有标准库；`aline-markdown-probe` 实际解析 pulldown 事件；`aline-app-core-probe` 实际 derive 嵌入中文 TOML 并读取其字段；公开探针 API 返回 `Result<String, ProbeError>`，区分资源缺失、UTF-8、TOML、字段缺失和字段类型错误，示例打印具体错误变体后返回失败退出码。所有探针源码有 `#![forbid(unsafe_code)]`，没有 `unwrap`/`expect`；source/buffer/edit 仅证明零第三方依赖，不包含正式业务 API。

**实际命令结果：** `cargo metadata --format-version 1`、`cargo tree --target all -e all` 及 macOS arm64 / macOS x86_64 / Windows x64 MSVC / Linux x86_64 / Linux aarch64 的五次 `cargo metadata --filter-platform` 均退出 0。macOS arm64 宿主目标 `cargo check --workspace --target aarch64-apple-darwin` 与 `cargo build --workspace --release --target aarch64-apple-darwin` 均退出 0。Windows/Linux 和 macOS Intel 只做依赖图解析，没有在这些平台编译或执行。

Cargo resolve 闭包包含 45 个节点（含五个本地探针）；Markdown 与 app-core 两个示例消费者均运行成功。tree 与账本中没有 GPUI、regex/regex-*、notify、tokio、wasmtime、wasi 或 wasip2。这里的结果只描述这三个精确 feature 配置下的候选消费方。`cargo tree -e all` 显示 rust-embed 带入 compile-time derive 所需的 `rust-embed-impl`/`rust-embed-utils`、`walkdir`、`mime_guess`、哈希和 proc-macro 依赖；这不是正式 workspace 最终锁文件，也不是对未来 features 或其它依赖图的许可。没有新增 entity、emoji、Unicode segmentation、随机、diff 等依赖；它们目前无探针真实消费，尚未准入。

## 证据边界

报告仅证明：给定本机 Rust/Cargo 1.98.1、指定候选版本和 feature 组合，隔离纯逻辑消费者可以解析，并且 macOS arm64 上 check/release build 可消费。metadata 目标过滤是 Cargo 依赖图解析，不是跨平台构建。没有运行正式 workspace 的 fmt/clippy/test；五个正式 crate、门禁、catalog 协议和 V-01～V-09 仍未验证。探针预期依据是调用的 crate 公共 API、crates.io Cargo 元数据与当前工程设计约束，不是产品金样。
