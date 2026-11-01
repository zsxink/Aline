# R09 交接（≤80 行）

## 状态
DONE_WITH_CONCERNS。唯一关注点是期望文件的写入顺序（见下「如实说明」），不是实现缺陷。

## 提交范围
`aline-markdown`（本次由主 Agent 串行提交，worker 未提交）。

## 改动文件
- `crates/aline-markdown/src/mapping/projection.rs` — 新增 `MappingPrecision::as_str()`（`const fn`）。
- `crates/aline-markdown/src/parse.rs` — 新增 `Parsed::source_identity()`（手写 FNV-1a 64）；
  `impl fmt::Display for Parsed` 追加源码身份/修订号、完整绘制文字、逐片段行；既有行字节级未改。
- `crates/aline-markdown/tests/dump.rs` — 新增，7 个测试。
- `docs/validation/m0-engine-foundation/remediation/R09/run-1/` — 全部证据。

## dump 新增字段（只增不改）
1. `source <len> identity fnv1a64 0x<16 hex> revision <n>`（前缀 `source <len>` 不变）
2. `drawn <len> text "<完整转义文字>"`（前缀 `drawn <len>` 不变）
3. `segment <d0>..<d1> -> <s0>..<s1> <Exact|Aligned|Hidden|Synthetic>`

## 验证（全部 exit 0）
- `cargo run --locked --offline -p aline-markdown --example dump_tree -- …/basic.md`
- `cargo test --locked --offline -p aline-markdown` — 12 个测试目标全绿；新增 7 个全过
- `cargo clippy --locked --offline -p aline-markdown --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
原始输出见 `post-fix.txt`；环境 `CARGO_NET_OFFLINE=true RUSTUP_TOOLCHAIN=1.98.1
CARGO_HOME=$HOME/.cargo`（隔离 CARGO_HOME 注册表为空，需解析依赖的命令会 101，原因见 `commands.txt`）。

## 手写推导 vs 实现
差异 **0 处**：49 片段、绘制 197 字节、`fnv1a64 0xacb96228ce0d7842` 三项全部一致。
推导只用 `pulldown-cmark =0.13.6` 探针事件表 + fixture 原始字节 + 4 条 M0 规则，不调用
`aline-markdown`。推导过程中修正过 2 处**推导自身**的错误（数学/脚注应按源码切片绘制；
表格 tab 计数器整表累计），与实现无关。

## 回归面
`tests/spec.rs` 的 dump 断言、`tests/diagnostics.rs` 三处 dump 断言、`tests/hostile.rs:336`
确定性断言均通过，断言强度未削弱。`examples/dump_tree.rs` 行为不变。

## 如实说明（复核者重点看这里）
`basic.dump.expected.txt` 是在实现已能输出扩展 dump **之后**才落盘的，不能称「盲写期望」。
独立性建立在：独立探针 + `/tmp/r09derive.py` 推导先行得出三项结论并与文件逐行一致；
每个片段的源码切片可用 `od -c` 从 fixture 字节直接复核。详见 `derivation.md` 第 6 节。

## 未覆盖
- `identity fnv1a64 unavailable` 分支在正常 `TextSnapshot` 上不可达，未实测。
- FNV-1a 非密码学哈希，不得用于安全判定或去重（已在文档注释声明）。

## 待独立复核
本批次由主 Agent 实现，**尚无独立复核者**；tasks.md 中记为「待独立复核」。
