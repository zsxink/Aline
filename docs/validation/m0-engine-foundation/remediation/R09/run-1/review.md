# R09 独立复核

复核对象 `72c8bc6`。复核者未参与该批实现或期望文件编写；`report.md` / `handoff.md` 结论未被采信，
下列结论均来自本人重跑与独立重算。工具链 rustc/cargo 1.98.1；所有 cargo 命令含 `--locked --offline`
且位于 `--` 之前；需要解析依赖的命令带 `CARGO_HOME=<cargo-home>`。修复前树由
`git archive 72c8bc6^ | tar -x -C <tmp>/r09old` 取得（未用 `pre-fix.txt` 作为唯一依据）。

## A. 规格合规

| 项 | 结论 | 依据 |
|---|---|---|
| Q1 F-07 关闭 | **达标** | 自造同长 319 字节、同块范围的 A/B（`**粗体** 与 *斜体*` vs `*粗体* 与 **斜体**`）：旧树 dump `diff` exit 0（0 字节，完全不可区分）；新树 exit 1（612 字节，身份 `0xacb96228ce0d7842` vs `0x8b89856b89da7e1a`，并有 7 行 `segment` 差异）。 |
| Q2 只增不改 | **达标**（输出层面） | 旧 25 行 dump 每行都是新 dump 某行的字节前缀（PREFIX_BAD 0）；剥去新增后缀后 basic.md / md11.md 与旧树 dump 逐字节相同（exit 0）。block 10 / gap 9 / protected 3 行 IDENTICAL。`git diff 72c8bc6^ 72c8bc6 -- crates/` = 3 文件 452 增 2 删，2 删正是 `source`/`drawn` 两行被替换为带前缀的超集；`build.rs` 未改。 |
| Q3 期望独立性 | **达标** | 本人用 `<tmp>/r09probe`（仅依赖 `pulldown-cmark =0.13.6`，剥离 front matter 0..19 后 +19 回填）重出 71 条事件（与 `derivation.md` 所用事件表逐字节相同），再只读 fixture 原始字节 + 事件表重算，得 49 行 `segment`、197 字节绘制、FNV `0xacb96228ce0d7842`，与 `basic.dump.expected.txt` **0 差异**。未 import/调用 `aline-markdown`。 |
| Q4 写入顺序弱点 | **可接受** | 见 E-F5：弱点真实存在，但独立输入已能复现全部结论，不构成「照实现写测试」。 |
| Q5 消费方 | **达标但引入新脆弱点** | 13 个测试目标 125 passed / 0 failed（exit 0）；`hostile.rs:336` 确定性断言仍成立。但见 E-F1。 |
| Q6 FNV 正确性 | **达标** | 逐字节 `hash ^= byte; hash = hash.wrapping_mul(PRIME)`（`parse.rs:150-161`），初值/素数/环绕正确；`iter_chunks()` 失败 → `None` → 打印 `unavailable`；唯一调用点是 `Display`；文档注释明示非密码学、禁用于安全/去重。 |
| Q7 测试会变红 | **达标** | 3 处变异全部 exit 101，均还原且 sha256 一致（见 D）。 |
| 手工推导「两处错误」归属 | **主 Agent 说法成立** | `build.rs` 中 `fn math` / `fn footnote_reference` 均以 `self.source.get(source)` 整切片 `Exact` 绘制；`table_cells` 仅在 `Start(Table)` 重置、按整表累计。实现与推导修正后的规则一致，**非实现缺陷**。 |

## B. 实跑命令（cwd / 起止 UTC / exit / 关键输出）

| # | cwd | argv | 起–止 UTC | exit | 关键原始输出 |
|---|---|---|---|---|---|
| 1 | repo | `cargo test -q -p aline-markdown --locked --offline` | 07:08:55–07:08:56 | 0 | 13 段 `test result: ok`，合计 125 passed / 0 failed |
| 2 | repo | `cargo clippy -q -p aline-markdown --locked --offline --all-targets -- -D warnings` | 07:08:56–07:08:57 | 0 | 无告警输出 |
| 3 | repo | `cargo fmt -p aline-markdown --check` | 07:08:57–07:08:57 | 0 | 无 diff |
| 4 | repo | `cargo run -q -p aline-markdown --locked --offline --example dump_tree -- crates/…/basic.md` | 07:05:31–07:05:33 | 0 | 74 行，`source 319 identity fnv1a64 0xacb96228ce0d7842 revision 0` |
| 5 | `<tmp>`/r09old | `CARGO_HOME=… cargo run -q … --example dump_tree -- <tmp>/r09_A.md` | 07:05:33–07:05:33 | 0 | `source 319` / `drawn 197`，25 行 |
| 6 | `<tmp>`/r09old | 同上 `<tmp>/r09_B.md` | 07:05:33–07:05:33 | 0 | 与 #5 逐字节相同 |
| 7 | repo | `… dump_tree -- <tmp>/r09_A.md` | 07:05:33–07:05:34 | 0 | `identity … 0xacb96228ce0d7842` |
| 8 | repo | `… dump_tree -- <tmp>/r09_B.md` | 07:05:34–07:05:34 | 0 | `identity … 0x8b89856b89da7e1a` |
| 9 | repo | `diff <#5> <#6>`（进程替换） | 07:11:29–07:11:31 | 0 | 0 字节 |
| 10 | repo | `diff <#7> <#8>` | 07:11:31–07:11:32 | 1 | 612 字节 |
| 11 | repo | `diff <tmp>/r09_new_basic.txt …/basic.dump.expected.txt` | 07:11:32–07:11:32 | 0 | 0 字节（实际 dump 与期望逐字节相同） |
| 12 | repo | `python3 -I`（旧/新 dump 前缀与族分类） | 07:11:32–07:11:32 | 0 | `old_dump_lines 25 new_dump_lines 74 PREFIX_BAD 0`；block/gap/protected `IDENTICAL` |
| 13 | repo | `python3 -I`（剥离新增后缀后 diff 旧树/新树 dump，basic.md 与 md11.md） | 07:19:00–07:19:01 | 0 | 两次均 0 字节（25/25、21/21 行） |
| 14 | `<tmp>`/r09probe | `CARGO_HOME=… cargo run -q --locked --offline -- …/basic.md` | 07:17:25–07:17:26 | 0 | 71 行事件，与 `derivation.md` 事件表 `diff` exit 0 |
| 15 | repo | `python3 -I <tmp>/q3derive4.py`（独立推导） | 07:13:19–07:13:19 | 0 | `# drawn_len 197` `# fnv1a64 0xacb96228ce0d7842`；49 行与期望 `diff` exit 0 |
| 16 | repo | `cargo run … dump_tree -- <tmp>/r09_words.md` / 旧树同命令 | 07:15:01–07:15:03 | 0 | 新 `grep -c Table` = 1；旧 = 0 |
| 17 | repo | `bash <tmp>/r09_mut.sh`（M1/M2/M3） | 07:10:26–07:10:29 | 101×3 | 见 D |
| 18 | repo | `shasum -a 256 -c <tmp>/r09bak/sha.txt` | 07:11:32–07:11:32 | 0 | 4 文件全 `OK` |
| 19 | repo | `git status --porcelain crates/` | 07:11:32–07:11:32 | 0 | 0 行（工作树干净，未 commit） |

## C. 独立重算的 5 行 segment 与 FNV

仅用 fixture 原始字节 + 独立探针事件表，**未调用 `aline-markdown`**（脚本 `<tmp>/q3derive4.py`）。

| segment 行 | 独立得到的源码切片（原始字节） | 判定 |
|---|---|---|
| `0..0 -> 0..23 Hidden` | `b'---\ntitle: demo\n---\n\n# '`（23 字节） | 零宽绘制 ✓ |
| `33..37 -> 64..70 Aligned` | `b'`code`'`（6 字节 vs 绘制 4） | 反引号隐藏 ✓ |
| `119..120 -> 209..209 Synthetic` | 空切片；锚点字节 `b' '`（`TableCell` Start） | 零宽源锚点 ✓ |
| `70..116 -> 151..197 Exact` | 代码块正文 46 字节，两侧长度相等 | ✓ |
| `160..174 -> 280..294 Exact` | `b'$$\nE = mc^2\n$$'`（14 字节） | `$$` 整体绘制 ✓ |

全量复核（本人脚本）：49 行绘制侧连续覆盖 197 字节、0 处语义违例（Hidden 全零宽 / Exact 等长 /
Aligned 不等长 / Synthetic 零宽源）；源码侧覆盖至 319 且仅 3 处 Synthetic 零宽锚点落在随后 Hidden 区间内
（按绘制顺序的必然结果，`dump.rs:30-32` 已注释）。FNV-1a 64 逐字节手算 = `0xacb96228ce0d7842`，
与期望文件、与 `tests/dump.rs:25` 常量三者一致。

## D. 变异自证（每处均还原并核对 sha256）

| 变异 | 文件 | 改动 | exit | 失败测试 | 还原 sha256 |
|---|---|---|---|---|---|
| M1 | `mapping/projection.rs` | `Hidden` → `HiddenMut` | 101 | `e3_segments_match…`、`e3_basic_fixture_dump…`（5 passed / 2 failed） | `a24287b3…a490b` ✓ |
| M2 | `src/parse.rs` | `source.start` → `source.start + 1` | 101 | `e3_basic_fixture_dump…`（6 passed / 1 failed） | `bc14adcd…50c99` ✓ |
| M3 | `src/parse.rs` | FNV 素数 `…01b3` → `…01b5` | 101 | `e3_source_identity…`、`e3_basic_fixture_dump…`（5 passed / 2 failed） | `bc14adcd…50c99` ✓ |

`shasum -a 256 -c <tmp>/r09bak/sha.txt` 4 文件全 OK；`git status --porcelain crates/` 为空；未 commit、未 reset。

## E. Findings

- **[一般] F1 — 新增 `drawn <len> text "…"` 使既有负向断言变为内容敏感。**
  `tests/diagnostics.rs:142-143` 的 `!dump.contains("CodeBlock")` / `!dump.contains("Table")`
  原只可能被 block 行触发；现在整篇源码文字进入 dump，源码里出现这些词就会误判失败。
  实测：`<tmp>/r09_words.md`（`Table and CodeBlock words`）新 dump `grep -c Table` = **1**，旧 dump = **0**。
  同类风险在 `:240` 的 `!dump.contains("diagnostic")`（实测 `diagnostic word` 同样 1 vs 0）。
  当前两处夹具源码不含这些词，故**测试仍通过**，属潜伏脆弱性，非当前失败。
- **[一般] F2 — 证据文档行数自相矛盾。** `report.md:59`、`report.md:118` 称「65 行期望 dump」，
  实际 `basic.dump.expected.txt` 为 **74 行**（`wc -l`），测试比对亦以 74 行为准。文档数字错误，
  不影响测试与实现。
- **[一般] F3 — `unavailable` 分支无测试覆盖（主 Agent 已自述）。** 本人确认该路径经公开 API 不可达：
  `parts()` 仅在 `Backing::Overlay` 越界时返回 `InvalidBacking`，而 Overlay 构造前已由
  `validate_range` 校验字符边界与身份。因此不是缺陷，但未来改动无回归护栏。

非缺陷观察（不计入 finding）：`tests/dump.rs` 用 `include_str!` 耦合 docs 证据文件（与
`tests/mapping.rs:452` 同型先例）；`Display` 内 `drawn_text()` 被调用两次（`len()` 与本体）；
期望文件「实现先、文件后」的写入顺序弱点真实存在，但独立输入已 0 差异复现全部结论，判定可接受。

## F. 裁决

**APPROVE_WITH_CONCERNS**（3 条 [一般]，无阻断）。

理由：F-07 已实质关闭（本人自造同长同块范围源码实测：旧 dump 完全相同、新 dump 可区分）；
dump 输出对既有消费方是字节级超集，125 个测试与 clippy/fmt 全绿；期望文件可由 fixture 字节 +
独立 `pulldown-cmark` 探针独立复现（49 片段 / 197 字节 / FNV 三项 0 差异），故写入顺序弱点不构成
「照实现写测试」；3 处变异均能变红并已还原。遗留关注点为 F1（负向断言内容敏感，潜伏）、
F2（证据文档行数错误）、F3（`unavailable` 无覆盖），均不阻断本批验收，建议随 R10/后续批次修正。
