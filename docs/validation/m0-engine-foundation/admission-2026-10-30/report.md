# GPUI 隔离候选依赖审计

本页保留下载授权时的审计快照；用户随后指示完成第 1 组，精确例外已同步现行架构 §3.2。最新准入、守卫正反例和独立复核结论见[第 1 组完成记录](../group-1-report.md)，下文“未授权/未准入/不勾选”描述的是此前审计时点。

日期：2026-10-30。用户本次授权：在隔离目录下载 GPUI 候选及依赖，解析/构建并提交实际闭包和规范修订建议。未授权放宽禁令。本报告属于任务 1.2～1.4 的部分证据，不是第 1 组完成声明。

## 结论

`gpui-pre =0.3.9` 的核心库在本机 Rust/Cargo 1.98.1、edition 2024 下构建成功；`regex 1.13.1` 在全部已检查桌面目标中实际可达，所以当前禁令下不得准入。全目标 Cargo tree 还选入条件为 WASI 的 `wasip2`；该路径在本次所有桌面目标中均不选入。未发现所审选包图含 notify、tokio、Wasmtime 或第二个已知 Markdown 解析器，但包名核查不等于任意未知包用途的完整证明。

审计需要区分 metadata 的过近似图与实际 Cargo tree。启用 `test-support` 时，metadata 的节点路径还包含 `zed-scap → getrandom 0.2.17 → wasi 0.11.1`，但实际 `cargo tree --target all` 未选中该路径；逆向 tree 也明确输出 `nothing to print`。不能将此 metadata 路径当成实际启用的依赖，更不能据此增加例外。

## 候选与来源

- 隔离位置及专用 `CARGO_HOME` / target 目录见 [isolation.json](./isolation.json)。未修改全局 Cargo/rustup 配置或安装工具链；未修改 Aline 正式依赖或创建窗口。
- 消费方为 [candidate/Cargo.toml](./candidate/Cargo.toml) 与 [candidate/src/lib.rs](./candidate/src/lib.rs)：只消费 `gpui::SharedString` 类型；关闭 GPUI 默认 features，由命名组合单独启用。未加入 `gpui-pre-platform` 或其它原生后端，故本报告不证明原生显示、字体加载或真实输入能力。
- Cargo registry 来源：`registry+https://github.com/rust-lang/crates.io-index`；GPUI 版本精确为 0.3.9，许可证声明为 Apache-2.0，归档 SHA-256 `34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0` 与实际 [Cargo.lock](./candidate/Cargo.lock) 一致。详情见 [包身份](./gpui-package-identity.json)。
- 发布包声明上游快照 `zed@93d745668805029476e6d7fbb19652116fed51de`。使用官方 GitHub 树的 Git blob SHA-1 比对全部 93 个 `src` 文件、`build.rs` 和 LICENSE，共 95 项；93 项完全一致，无缺失或额外源文件。
- `src/action.rs` 有已识别的发布修订：宏 derive 路径由 `gpui::Action` 改成 `$crate::Action`；许可证由上游 symlink 实体化，正文一致。见 [源码比对](./gpui-source-comparison.json)、[发布包差异](./gpui-package-source.diff)与[官方树身份](./gpui-upstream-tree.json)。未逐文件比对其它 `gpui-pre-*` 包或全部包装材料；不能把它们写成已经独立验证。
- 无 Aline 自有 GPUI 补丁。候选未声明 MSRV；本次没有最低工具链通过结论。

## 实际命令与结果

每条 Cargo 命令的完整 argv、工作目录、退出码与原始 stdout/stderr，见同名 `*.command.json` / `*.stdout.log` / `*.stderr.log`；metadata 正文保存在同名 `*.json`。下表命令省略的共同部分为 `cargo +1.98.1` 和隔离 manifest 的绝对路径，完整值均在 command JSON 中。专用 Cargo 环境见 command JSON，不读取 Aline 尚不存在的 manifest。

| 检查 | 实际结果 | 证据 |
|---|---|---|
| `metadata --format-version 1` 基础、test-support 及桌面目标组合 | 全部退出 0；metadata 基础 318 个可达包，test-support 全目标 411 个可达包 | `metadata-*.command.json` 与原始 JSON |
| `tree --target all -e all --locked` 基础 / test-support | 两者退出 0；实际选包分别 285 / 317 个（含候选消费方） | `tree-all-base.*`、`tree-all-test-support.*` |
| `tree --target <target> -e all --features <组合> --locked` | 全部退出 0；仅证明解析，不证明该 OS 构建/运行 | `tree-macos-*`、`tree-linux-*`、`tree-windows-*` |
| `check --target aarch64-apple-darwin --features font-kit --locked` | 0 | [命令](./check-macos-arm.command.json)、[日志](./check-macos-arm.stderr.log) |
| `build --target aarch64-apple-darwin --features font-kit --locked` | 0；核心库消费方构建，不是原生应用构建 | [命令](./build-macos-arm.command.json)、[日志](./build-macos-arm.stderr.log) |
| `clippy --target aarch64-apple-darwin --features font-kit --locked -- -D warnings` | 修正调用参数后 0；不代表八 crate lint 继承或第三方全部内部代码 lint 通过 | [命令](./clippy-macos-arm-corrected.command.json)、[日志](./clippy-macos-arm-corrected.stderr.log) |
| `check --target aarch64-apple-darwin --features font-kit,test-support --locked` | 0；未实例化平台 harness、未执行原生事件测试 | [命令](./check-macos-arm-test-support.command.json)、[日志](./check-macos-arm-test-support.stderr.log) |
| `fmt --check` | 初始夹具格式不符退出 1；`fmt` 后重查退出 0 | `fmt-candidate.*`、`fmt-candidate-write.*`、`fmt-candidate-recheck.*` |
| edition 2015 最小反例：`pub async fn candidate() {}` | `check --offline` 退出 101，E0670；独立预期来自 Rust 2015 不支持 async 函数；不代表某个 GPUI 最低版本 | [夹具](./incompatible-edition-2015/src/lib.rs)、[诊断](./negative-edition-2015.stderr.log) |

已解析组合：macOS aarch64 / x86_64 的 `font-kit`；Windows x86_64 MSVC 的 `windows-manifest`；Linux x86_64 的 `x11`、`wayland`、二者同时；Linux aarch64 的二者同时；macOS aarch64 的 `font-kit,test-support`。没有使用 `--all-features` 并集代替这些组合。screen-capture/bench-support/profiler/stacker/inspector 等未消费 feature 未审计，不判通过。

## 实际冲突路径与覆盖边界

1. 已启用且无 target 限定：`candidate（别名 gpui） → gpui-pre 0.3.9 → regex 1.13.1`。其实现依赖 `regex-automata 0.4.18`、`regex-syntax 0.8.11` 同样留在包账本。关闭 GPUI 默认 features 后这条路径仍存在。
2. 全目标选入、桌面目标不选入：`candidate → gpui-pre 0.3.9 → getrandom 0.3.4 → wasip2 1.0.4+wasi-0.2.12`。末边条件为 `cfg(all(target_arch = "wasm32", target_os = "wasi", target_env = "p2"))`。这是 WASI 平台绑定，所审图未引入 Wasmtime 或动态插件宿主；按当前“WASI 全闭包禁止”的字面规则仍需裁决，不能自行视为允许。
3. metadata 过近似、实际 tree 未选入：`gpui-pre → zed-scap → rand → rand_core → getrandom 0.2.17 → wasi 0.11.1+wasi-snapshot-preview1`。保留该路径证据，但不列入本次请求的例外。

见 [metadata 与 tree 交叉核对](./closure-audit.json)、[实际选包](./cargo-tree-selected-packages.json)、[基础完整依赖账本](./dependency-ledger-base.json)、[test-support 账本](./dependency-ledger-test-support.json)。账本保留真实包名、版本、来源、许可证声明、features、target、normal/dev/build 声明及别名；许可证声明不等于所有传递许可证的独立法律审查。`analyze_metadata.py` 可复算路径；该辅助程序不是第 2 组工程守卫，也没有声称通过其正反例。

## 失败、重试与环境缺口

- `curl --fail --location --connect-timeout 15 --max-time 60 --output docs/validation/m0-engine-foundation/admission-2026-10-30/gpui-pre-0.3.9.registry.json https://crates.io/api/v1/crates/gpui-pre/0.3.9` 退出 56，HTTP 403。未把 API 失败当准入通过；实际 Cargo registry 下载及 lock 校验成功，形成了替代来源证据。
- 第一版本地摘要命令尝试 `import tomllib`，因本机默认 Python 未提供该模块退出 1；后续直接从 Cargo 元数据与 lock 中提取，无需安装 Python 库。
- 第一条 Clippy 调用把 `--manifest-path` 误放在 `-- -D warnings` 之后，退出 101；修正 CLI 参数顺序后退出 0。保留原始失败日志，不把调用错误归因为候选代码缺陷。
- `xcrun --find metal` 退出 72；本机当前开发目录为 `/Library/Developer/CommandLineTools`。GPUI 核心库消费方实际构建通过，但原生后端、Metal shader 编译与 M1 冒烟均未执行，不据此申请安装 Xcode 或改变全局开发目录。
- 五引擎与八成员 workspace 均尚未建立；没有五引擎闭包、成员 lint、catalog 或 M0 出口通过结论。Windows/Linux 与 Intel Mac 未构建或运行；V-01～V-09 未通过。

## 后续裁决

具体规范修订建议见 [最小准入例外草案](./proposed-policy.md)。它尚未生效；仅补充证据不勾选 1.2～1.4，不开始第 2 组。

本轮文档/证据收尾检查：`git diff --check` 退出 0；证据 JSON 解析及报告本地链接检查通过；`codegraph sync` 退出 0，报告索引已是最新。独立 reviewer 只读复核另记于本目录，不能代替未执行的八 crate 守卫或平台验证。
