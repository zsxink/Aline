# 命令与执行记录

环境：macOS 27.0.1 (26A434)，Apple M5 / aarch64；`rustc 1.98.1 (48a229cea 2026-09-01)`，`cargo 1.98.1 (797e8a9bc 2026-08-05)`。实际主机版本见 [环境命令原始结果](../admission-2026-10-30/environment-commands.json)；原摘要误记的 macOS 26.1 已按该证据修正。Cargo 缓存复用 `admission-2026-10-30/isolation.json` 指定的专用 `CARGO_HOME`；`CARGO_TARGET_DIR` 为独立临时目录。没有安装工具链或更改全局配置。

依赖信息查询（均从 crates.io；三条退出码均为 0）：

```sh
CARGO_HOME=<isolation.json cargo_home> cargo info pulldown-cmark --registry crates-io
CARGO_HOME=<isolation.json cargo_home> cargo info toml --registry crates-io
CARGO_HOME=<isolation.json cargo_home> cargo info rust-embed --registry crates-io
```

完整消费者解析与检查（工作目录为 `isolation.json` 的 candidate_root；所有命令均使用隔离 `CARGO_HOME` 和独立 `CARGO_TARGET_DIR`）：

```sh
cargo metadata --format-version 1                         # exit 0
cargo tree --target all -e all                            # exit 0
cargo metadata --format-version 1 --filter-platform aarch64-apple-darwin # exit 0
cargo metadata --format-version 1 --filter-platform x86_64-apple-darwin   # exit 0
cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc # exit 0
cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu # exit 0
cargo metadata --format-version 1 --filter-platform aarch64-unknown-linux-gnu # exit 0
cargo check --workspace --target aarch64-apple-darwin     # exit 0（修正后）
cargo build --workspace --release --target aarch64-apple-darwin # exit 0
cargo run -p aline-markdown-probe --example probe-consume # exit 0，输出 pulldown-cmark events=6
cargo run -p aline-app-core-probe --example probe-consume # exit 0，输出 embedded title=Aline
```

错误处理修订后重新执行：

```sh
cargo check -p aline-app-core-probe --target aarch64-apple-darwin # exit 0
cargo build -p aline-app-core-probe --release --target aarch64-apple-darwin # exit 0
cargo run -p aline-app-core-probe --example probe-consume # exit 0，输出 embedded title=Aline
```

首次消费编译失败诊断保存在 `logs/first-attempt-toml-serde-failure.log`：初始 `toml` 配置仅启用 `std, parse`，因 `Table`/`from_str` 由 `serde` feature 门控，`cargo check` 与 `cargo build` 均 exit 101。随后显式启用 `serde`，并将精确需求写作 `=1.1.8`（Cargo lock 实际版本含 `+spec-1.1.0`，避免 Cargo 关于 semver metadata 的警告）。`rust-embed` 开启 `debug-embed`，以令调试与发布配置都走嵌入资源路径；crate 源码显示未开时 debug 的 `get` 走文件系统。修复后的检查见对应新日志。

日志文件保留命令标准输出/错误；所有 target metadata JSON 文件退出码记录在上文，`.stderr` 文件保留标准错误。`cargo test`、clippy、fmt、Windows/Linux build/check、macOS Intel build/check 均未执行。两个 example 是消费探针，不是 catalog 协议测试。
