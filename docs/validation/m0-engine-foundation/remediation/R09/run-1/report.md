# R09 报告：E3 同夹具可审查 dump（F-07）

## 1. 结论

| 项 | 结果 |
|---|---|
| 状态 | **DONE_WITH_CONCERNS**（见第 7 节，仅一条证据顺序上的自述，不影响判定） |
| 失败事实 | F-07（严重，D2/D3/D7）——原 dump 只打印长度，看不出「块范围与总长度相同但来源不同」 |
| 实现范围 | `crates/aline-markdown/src/parse.rs`、`crates/aline-markdown/src/mapping/projection.rs`、新增 `crates/aline-markdown/tests/dump.rs` |
| 新增测试 | 7 个，全绿（`test result: ok. 7 passed; 0 failed`） |
| 既有测试回归 | `aline-markdown` 全包 12 个测试目标全绿，无一失败 |
| 手写推导 vs 实现差异 | **0 处**（49 片段、197 字节绘制文字、FNV 摘要逐项一致） |
| clippy / fmt | clippy `-D warnings` exit 0；fmt `--check` exit 0 |

## 2. 失败事实的复现（修复前）

`pre-fix.txt` 记录修复前的 dump 输出：`source 319` / `drawn 197` / 10 个 block / 9 个 gap /
3 个 protected，**没有任何片段或文字本体**。这意味着下列两种不同源码会产生完全相同的 dump：

- 正常源码；
- 长度与块范围一致、但某个可见文字的来源区间被错误映射的源码（例如把「粗体」映射到别的
  源码区间，块范围不变）。

F-07 的判定依据即此：E3 要求「同一夹具下可人工审查的映射结果」，而旧 dump 无法区分上述两种情况。

## 3. 修复内容

### 3.1 dump 只增不改

`impl fmt::Display for Parsed`（`src/parse.rs`）保留全部既有行与前缀，仅追加三类信息：

1. `source` 行追加身份与修订号：
   `source <len> identity fnv1a64 0x<16 位小写 hex> revision <u64>`
   —— 前缀 `source <len>` 不变，`tests/spec.rs` 的 `"source 100"` 断言不受影响。
2. `drawn` 行追加完整绘制文字本体（`{:?}` 转义，不截断）：
   `drawn <len> text "<转义后的完整文字>"`
   —— 前缀 `drawn <len>` 不变。
3. 新增逐片段行，按绘制起点升序、覆盖 `projection().segments()` 全部片段，不省略不合并：
   `segment <drawn_start>..<drawn_end> -> <source_start>..<source_end> <Exact|Aligned|Hidden|Synthetic>`

block / gap / protected / style / diagnostic 各行字节级未改。

### 3.2 源码身份摘要

`Parsed::source_identity()`：手写 FNV-1a 64（初值 `0xcbf29ce484222325`，素数 `0x100000001b3`，
64 位环绕），读取走 `iter_chunks()`，不物化全文、不注入新预算；底层表示异常时返回 `None`，
dump 打印 `unavailable` 而不是给假摘要。文档注释明确声明它**不是密码学哈希**，不得用于安全
判定或去重裁决。**未引入任何新依赖。**

`MappingPrecision::as_str()` 新增为 `const fn`，返回 `Exact|Aligned|Hidden|Synthetic`；
只用于 dump 输出，不改动 `MappingSegment` / `Projection` 的构造与映射语义。

### 3.3 E3 独立期望

`tests/dump.rs` 的 7 个测试：

| 测试 | 断言内容 |
|---|---|
| `e3_basic_fixture_dump_matches_independent_expectation` | 逐行比对 65 行期望 dump |
| `e3_drawn_text_matches_visible_source_and_length` | 绘制文字全等 + 长度 197 + `~~`/URL 不得出现 + `**` 恰好 2 处（代码块字面）+ `字面 **不解析**` 保留 |
| `e3_segments_match_independent_table_one_by_one` | 49 行逐片段区间与精确性、绘制侧连续且覆盖 197 字节 |
| `e3_hidden_aligned_and_synthetic_are_present` | 计数 Hidden 21 / Exact 24 / Aligned 1 / Synthetic 3，并逐片段断言首个 Hidden、首个 Synthetic（`119..120 -> 209..209`）、唯一 Aligned（`33..37 -> 64..70`） |
| `e3_blocks_and_gaps_partition_source_without_overlap` | 10 块逐行比对 + 9 间隙 + 严格相接、无重叠、覆盖 319 字节、无诊断 |
| `e3_protected_ranges_match_independent_expectation` | 3 条保护区间逐行比对 + `[^1]` 切片核对 |
| `e3_source_identity_is_byte_derived_not_snapshot_identity` | 独立 `fnv1a64` 实现与常量一致；两个不同 `SnapshotId` 的同文本快照 dump 相同；同长度变异后摘要改变 |

测试代码无 `unwrap()` / `expect()`；生产代码未新增 `unwrap/expect/panic!/unreachable!/assert!`。

## 4. 手写推导与实现的一致性

`derivation.md` 记录了推导全过程：只依赖 `pulldown-cmark =0.13.6` 的独立探针事件表 +
fixture 原始字节 + 四条 M0 规则（R1 标记隐藏 / R2 文字绘制 / R3 源码切片整体绘制 /
R4 表格合成 tab），由 `/tmp/r09derive.py` 生成片段表。

比对结果：

| 项 | 独立推导 | 期望文件 | 差异 |
|---|---|---|---|
| `segment` 行 | 49 | 49 | 0（逐行 diff 为空） |
| 绘制文字 | 197 字节 | 197 字节 | 0 |
| FNV-1a 64 | `0xacb96228ce0d7842` | 同 | 0 |

推导过程中出现过两处**推导自身的错误**，已修正且记录在此，不是实现缺陷：

1. 首版脚本把 `DisplayMath` / `FootnoteReference` 按事件 payload 绘制，漏掉了 `$$` 与 `[^1]`；
   规则实为「按源码切片整体绘制」。
2. 首版脚本把合成 tab 计数器按行重置，而实际是整表累计（4 单元格 ⇒ 3 个 tab）。

修正后与实现完全一致，因此判定：**无实现缺陷**。

## 5. 对 Requirement / Scenario 与消费者的影响

- 组 6.3 / 9.3（E3 同夹具可审查 dump）：从「只能看长度」升级为「可逐片段人工审查」。
- `tests/spec.rs::dump_contains_config_blocks_gaps_source_protection_and_diagnostics`、
  `tests/diagnostics.rs` 的三处 dump 断言、`tests/hostile.rs:336` 的确定性断言：全部保持通过，
  且断言强度未削弱。
- `examples/dump_tree.rs` 行为不变（只是输出更丰富）。
- **不构成 M0 验收通过**：V-01～V-09 未因此轮修复而升级为通过，AC 基线也不升级为全量验收。

## 6. 未覆盖 / 残留

- 本夹具 `basic.md` 未触发 `Aligned` 之外的其他降级路径，`unavailable` 分支（`iter_chunks()`
  失败）在正常 `TextSnapshot` 上不可达，**未通过实测覆盖**，仅由代码路径保证。
- FNV-1a 存在碰撞可能，dump 不能作为去重或安全判定依据（已在文档注释与本报告中明示）。

## 7. 关注点（DONE_WITH_CONCERNS 的唯一理由）

`basic.dump.expected.txt` 的字节是在实现已能输出扩展 dump **之后**才落盘的，因此不能声称
「在完全看不到实现输出时盲写期望」。独立性依据是：`pulldown-cmark` 探针与
`/tmp/r09derive.py` 的推导独立于 `aline-markdown`，且三项结论（49 片段 / 197 字节 / 摘要）
均由该推导先行得出并与文件逐行一致。该顺序上的弱点已在 `derivation.md` 第 6 节与
`handoff.md` 中如实记录，交由独立复核者判断。

## 8. 证据清单

- `pre-fix.txt` — 修复前 dump（argv/cwd/起止/退出码）
- `post-fix.txt` — 修复后 dump + 全包测试 + clippy + fmt 原始输出
- `basic.dump.expected.txt` — 65 行期望 dump
- `derivation.md` — 逐片段推导表与 FNV 手算步骤
- `commands.txt` — 命令与环境的完整记录
- `report.md` — 本文件
- `handoff.md` — 交接摘要

## 11. 独立复核返回后的处理（追加，不改上文）

独立复核见 `run-1/review.md`，结论 **APPROVE_WITH_CONCERNS**（3 条一般，无阻断）。
F-07 已实质关闭（复核自造同长 319 字节 A/B 源码：旧 dump 完全相同、新 dump 可区分）。

### 11.1 勘误：期望 dump 行数

上文 §「验证」表与 §「证据」索引写的「65 行期望 dump」**错误**。实测
`basic.dump.expected.txt` 为 **74 行**（`wc -l`），`tests/dump.rs` 的逐行比对亦以 74 行为准。
提交 `72c8bc6` 的 message 同样沿用了「65 行」，一并作废。行数写错不影响实现与测试。

### 11.2 F1（一般）负向断言变内容敏感 —— 已修

R09 新增的 `drawn <len> text "…"` 把整篇源码文字并入 dump，使
`tests/diagnostics.rs` 的三处负向断言从「只可能被 block 行触发」变成「源码正文也能触发」：

| 位置 | 原断言 | 实测风险 |
|---|---|---|
| `:142-143` | `!dump.contains("CodeBlock")` / `!dump.contains("Table")` | 源码含这些词即误红 |
| `:240` | `!dump.contains("diagnostic")` | 同上 |

主 Agent 独立复现：源码 `Table and CodeBlock words\ndiagnostic word\n` 的新 dump 中
`Table`/`CodeBlock`/`diagnostic` 各命中 **1** 次，全部来自新增的 `drawn … text "…"` 行；
旧 dump 为 0。

修复：负向断言限定到对应行族（`dump_lines_of(&dump, "block ")`、
`dump_lines_of(&dump, "diagnostic ")` 计数为 0）。并用临时探针证明该修复必要：
同一源码 `OLD_STYLE_WHOLE_DUMP_HAS_TABLE=true` 而 `BLOCK_FAMILY_HAS_TABLE=false`。
探针 `tests/_rev_f1_probe.rs` 跑后即删，`git status crates/` 仅剩 `tests/diagnostics.rs`。

### 11.3 F3（一般）`unavailable` 分支无覆盖 —— 登记，不修

复核确认该路径经公开 API 不可达（`parts()` 仅在 `Backing::Overlay` 越界时返回
`InvalidBacking`，而 Overlay 构造前已由 `validate_range` 校验）。非缺陷，登记为无回归护栏。

### 11.4 验证（主 Agent实测，2026-11-01）

| 项 | 结果 |
|---|---|
| `cargo test -q -p aline-markdown --locked --offline` | 12 段全 ok，合计 **125 passed / 0 failed**，exit 0 |
| `cargo clippy … --all-targets -- -D warnings` | exit 0，无告警 |
| `cargo fmt -p aline-markdown --check` | exit 0（先 fmt 再 check） |
