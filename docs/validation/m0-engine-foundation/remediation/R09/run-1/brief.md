# R09 派发契约：F-07 E3 同夹具可审查 dump（文字/来源/精确性/源码身份）

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `/Users/xian/Project/editor/Aline`。前置 R02（`pre/rewrite.rs`）与 R03（`reuse.rs`）已完成；**不要写这两个文件**。

## 文件所有权（只有你可写这些文件）

- `crates/aline-markdown/src/parse.rs`（`impl fmt::Display for Parsed`，必要时 `Parsed` 上加私有辅助函数）
- `crates/aline-markdown/src/mapping/projection.rs`（仅当需要为 `MappingPrecision` 增加稳定名称方法；不得改 `MappingSegment`/`Projection` 的构造与映射语义）
- `crates/aline-markdown/tests/dump.rs`（新建）或 `crates/aline-markdown/tests/spec.rs`（二选一，优先新建 `dump.rs`）
- `docs/validation/m0-engine-foundation/remediation/R09/run-1/` 下你自己新建的证据文件

不得写其它文件（尤其 `crates/aline-markdown/src/reuse.rs`、`src/pre/**`、`src/mapping/coordinates.rs`、`crates/aline-buffer/**`、`crates/aline-edit/**`、任何 `Cargo.toml`、任何 `scripts/**`）。不得 `git reset`、不得回退他人改动。

## 失败事实（F-07，严重，维度 D2/D3/D7）

`openspec/changes/m0-engine-foundation/proposal.md:24` 的 E3：

> E3 dump 展示树与原始范围 | 同一夹具 dump 文字、块/Gap、源范围、映射精确性、保护范围和诊断；独立预期人工复核 | 6、9

现状（我本轮已独立复验）：`cargo run -q -p aline-markdown --locked --offline --example dump_tree -- crates/aline-markdown/tests/fixtures/m0/basic.md` 的 25 行输出只有：

```
config 1 features 0x0000000?
source 319
drawn 197
block 0..16 FrontMatter
gap ...
protected ...
style ...
diagnostic ...
```

其中 `source 319` / `drawn 197` 只是**长度**，不是文字、不是来源、不是身份。`crates/aline-markdown/src/parse.rs:143` 的 `Display` 实现就止于此。因此「来源映射错误但块范围和总长度相同」时 E3 的 dump 审查发现不了。

## 目标

1. **扩展 dump（只增字段，不改既有字段前缀）**：
   - `source <len>` 行保留原样前缀，其后追加源码**身份**：内容摘要（见下）与快照修订号。既有测试用 `dump.contains("source 100")`，前缀不能改。
   - `drawn <len>` 行保留，其后追加**绘制文字本体**：整段用 `{:?}` 形式的转义单行（`\n`/`\t`/引号转义），保证 dump 稳定且可人工比对；不得截断。
   - 新增逐片段行：`segment <drawn_start>..<drawn_end> -> <source_start>..<source_end> <Exact|Aligned|Hidden|Synthetic>`，按绘制起点升序，覆盖 `projection().segments()` 全部片段（无省略、无合并）。
   - `block`/`gap`/`protected`/`style`/`diagnostic` 行保持既有格式。
   - 输出必须**逐字节稳定**（同一输入两次运行完全一致），不得打印指针地址、时间或进程相关量。
2. **源码身份摘要（不得新增依赖）**：`aline-markdown` 只有 `aline-buffer` + `pulldown-cmark`，禁止新增 crate。用一段手写的 FNV-1a 64（或同等无依赖、确定性）对**源码原始字节**求摘要，输出定长十六进制（例如 `identity fnv1a64 0x...`），并在文档注释里写明算法与「不是密码学哈希」。同时输出快照 `revision()`（`TextSnapshot::revision()` 已有）。注意 `SnapshotId` 的 `Debug` 是 `SnapshotId(..)`，**不要**用它做身份输出。
3. **E3 同夹具独立预期**：
   - 以 `crates/aline-markdown/tests/fixtures/m0/basic.md`（319 字节）为唯一夹具。
   - 在证据目录写 `basic.dump.expected.txt`：**手写推导**的完整期望 dump（逐字节、逐片段），依据是 fixture 的原始字节（`od -c` 可读）与 M0 映射规则；不得把实现输出复制成期望。
   - 测试 `e3_basic_fixture_dump_matches_independent_expectation` 读入该期望并与之逐行比较（期望文件以 `include_str!` 或测试内常量均可，但内容必须来自手写推导，并在注释中写明推导方法）。
   - 若你推导后发现**实现**与你的独立推导不符，先判断是实现缺陷还是推导错误：若是实现缺陷，不要改实现掩盖，改为登记 `DONE_WITH_CONCERNS` 并在 handoff 单列；若只是你推导错误，修正期望并写明依据。
4. **必须同时存在的独立断言**（每项先手写期望再跑，不能只比较整段 dump）：
   - 绘制文字等于 fixture 的可见文本（含中文、行内标记隐藏后的结果），并显式断言 `drawn` 长度与文字字节长度一致；
   - 片段总数、每片 precision 与来源区间逐条列出；
   - 至少一个 `Hidden` 与一个 `Synthetic` 片段被断言（若 basic.md 不含 Synthetic，改用能产生 Synthetic 的现有夹具并写明为什么）；
   - 块/Gap 覆盖无缝、保护区间与诊断区间逐条断言；
   - 身份摘要：手写计算同一 FNV-1a 值并与 dump 输出比对（可在测试里用独立实现的小循环手算，或直接写死常量并在注释里给出推导步骤）。
5. **不得回归**：`crates/aline-markdown` 全部既有测试（含 `spec.rs` 的 `dump_contains_config_blocks_gaps_source_protection_and_diagnostics`、`diagnostics.rs` 的三条 dump 断言）保持通过且断言不放宽；`examples/dump_tree.rs` 不改行为（只是输出更丰富）。
6. 测试不得 `unwrap()/expect()`；生产代码不得新增 `unwrap/expect/panic!/unreachable!/assert!`（clippy deny）。dump 生成不得返回 `Result` 之外的失败路径，`Display` 内的 `writeln!` 保持既有风格。
7. 全部 Cargo 命令 `--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」。
8. 提交（Conventional Commits，scope `aline-markdown`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R09/run-1/`）

- `pre-fix.txt`（修复前 `dump_tree` 对 basic.md 的原始输出，argv/cwd/时间/exit）
- `post-fix.txt`（修复后同命令原始输出 + `cargo test -p aline-markdown --locked --offline` 全量原始输出）
- `basic.dump.expected.txt`（手写推导的期望 dump）
- `derivation.md`（逐片段推导表：fixture 字节区间 → 绘制区间 → precision → 依据；FNV 摘要手算步骤）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（dump 格式契约、推导与实现差异、对 6.3/9.3 的影响说明、未覆盖项）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、一行测试小结、dump 新增字段清单、手写期望与实现是否一致（差异条数）、concerns、证据路径。不要贴长日志。
