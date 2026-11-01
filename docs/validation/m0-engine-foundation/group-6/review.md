# 组 6（6.1–6.8）独立复核报告

- 复核日期：2026-10-31
- 复核对象：OpenSpec change `m0-engine-foundation` 组 6（任务 6.1–6.8），包含 `aline-markdown` 的 pre/build/tree/blocks/protection/budget/reuse 源码、测试、夹具、示例与 `docs/validation/m0-engine-foundation/group-6/` 证据
- 仓库与基线：`<repo>`，分支 `main`，HEAD `99867bf2c5775aae4e8ed910713496d4ff84323c`
- 复核立场：实现者不是复核者。实现者的报告、日志与探针只作为待验材料；本报告的结论来自本次独立重跑、独立重算、外部探针与读码，不把实现者自述当作已证明事实
- 写入范围：仅本文件。未修改 Rust 源码、测试、夹具、Cargo、lint/guard、`tasks.md`、其他 `docs/*.md` 或 OpenSpec 文件；未执行 `git commit`
- 快照说明：复核期间工作区仍有并发写入（`step-6.4/goldens-check/` 于 2026-10-31T09:10:14Z 出现；`group-7` 后台实现仍在生成文件）。除特别注明外，本报告结论以 2026-10-31T09:10:44Z–09:13:36Z 读到的状态为准，HEAD 未变；晚于该快照的追加内容不在本报告覆盖范围内

## 1. 工作区状态（`git status --porcelain` 逐字）

抓取时间：2026-10-31T09:10:44Z（分支 `main`，HEAD 未变）。

```text
 M crates/aline-markdown/src/lib.rs
 M crates/aline-markdown/src/mapping/mod.rs
 M crates/aline-markdown/src/mapping/projection.rs
A  docs/validation/m0-engine-foundation/group-7/step-7.1/goldens/edit-goldens.tsv
?? crates/aline-edit/src/budget.rs
?? crates/aline-edit/src/command.rs
?? crates/aline-edit/src/errors.rs
?? crates/aline-edit/src/permissions.rs
?? crates/aline-edit/src/refusal.rs
?? crates/aline-edit/src/reread.rs
?? crates/aline-markdown/examples/
?? crates/aline-markdown/src/budget.rs
?? crates/aline-markdown/src/build.rs
?? crates/aline-markdown/src/config.rs
?? crates/aline-markdown/src/diagnostic.rs
?? crates/aline-markdown/src/mdast.rs
?? crates/aline-markdown/src/parse.rs
?? crates/aline-markdown/src/pre/
?? crates/aline-markdown/src/protection.rs
?? crates/aline-markdown/src/reuse.rs
?? crates/aline-markdown/src/style.rs
?? crates/aline-markdown/src/tree/
?? crates/aline-markdown/tests/hostile.rs
?? crates/aline-markdown/tests/html.rs
?? crates/aline-markdown/tests/mapping.rs
?? crates/aline-markdown/tests/protection.rs
?? crates/aline-markdown/tests/reuse.rs
?? crates/aline-markdown/tests/rewrite.rs
?? crates/aline-markdown/tests/spec.rs
?? docs/validation/m0-engine-foundation/group-6/
?? docs/validation/m0-engine-foundation/group-7/commands.log
?? docs/validation/m0-engine-foundation/group-7/step-7.1/goldens-pre-implementation.log
```

要点：

- 组 6 的源码、测试、夹具与证据全部未提交；仓库 HEAD 仍是组 6 夹具提交 `99867bf`。
- `openspec/changes/m0-engine-foundation/tasks.md` 未被修改，6.1–6.8 在复核时仍全部为 `[ ]`。
- `docs/engine-design.md` 未被修改，仍保留“`Aligned`/`Hidden`/`Synthetic`、实体/HTML/合成换行与块局部映射仍仅契约未验证（第 6 组）”以及“GFM/产品语法的运行期 Options 与映射/HTML 金样仍须第 6 组验证”的旧记录。
- 上述状态中的 `group-7` 条目与本组无关，仅为逐字快照的一部分；本报告不评价组 7。

## 2. 执行环境与命令

### 2.1 环境

- 主机：`Darwin XianMac.local 27.2.0`（Darwin Kernel 27.2.0，`arm64`，Apple T8142）
- 工具链：`rustc 1.98.1 (48a229cea 2026-09-01)`，`cargo 1.98.1 (797e8a9bc 2026-08-05)`
- 环境变量：`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`
- 所有 cargo 命令均使用 `--locked --offline`；本次复核未联网，未新增依赖

### 2.2 本次实际执行的命令与结果

| 命令 | UTC | 退出码 | 结果摘要 |
| --- | --- | --- | --- |
| `cargo fmt --all --check` | 2026-10-31T09:00:49Z–09:00:57Z 窗口 | 0 | 无格式差异 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 2026-10-31T09:00:49Z–09:00:57Z 窗口 | 0 | 无 warning，未触发 `-D warnings` |
| `cargo test --workspace` | 2026-10-31T09:00:49Z–09:00:57Z 窗口 | 0 | workspace 合计 `134 passed`、0 failed |
| `python3 scripts/check_workspace.py --cargo-home <cargo-home> --output-dir <临时输出目录>` | 2026-10-31T09:06:42Z–09:07:01Z | 0 | 19/19 项 `exit_code=0`，`summary.json` 顶层 `passed=true`；测试合计 134 |
| 独立 Python 重算组 6 块/Gap 金样（`python3 -I <tmp>/aline-review-goldens.py <repo>`） | 2026-10-31T09:08:56Z | 0 | 7 个夹具、57 个区间、0 问题 |
| `cargo test -p aline-markdown --test {spec,rewrite,mapping,html,protection,hostile,reuse} --locked --offline` | 2026-10-31T09:11:28Z–09:11:38Z | 每项 0 | 8 / 6 / 10 / 11 / 6 / 10 / 7，合计 58 passed |
| 外部探针 `<tmp>/aline-review-probe` 的 `cargo run --offline` | 2026-10-31T09:13:28Z–09:13:30Z | 0 | 输出映射片段/规范锚点、复用命中/拒绝、HTML 行为（见第 3 节） |
| `cargo tree -p aline-markdown --locked --offline` | 2026-10-31T09:13:36Z | 0 | 直接依赖仅 `aline-buffer` 与 `pulldown-cmark 0.13.6` |
| `git log/show/stat`、`stat`、`sed`、`grep` 读码与证据核对 | 2026-10-31T09:0xZ–09:1xZ | 0 | 用于夹具先行、mtime、状态、日志一致性与代码路径核对 |

说明：

- 本次复核没有重跑全部 19 项 workspace gates 的单条命令，而是用统一脚本重跑并核对 19/19 的结果；脚本输出目录为系统临时目录，未写入仓库。
- 本次复核在组 7 并发写入开始前完成了 workspace 级 fmt/clippy/test；组 7 后续新增文件可能改变工作区编译图，但不影响上述已记录结果，也不影响组 6 的定向测试重跑。
- 未执行：Windows/Linux 或其他 target 构建；真实 UI/IME/磁盘/自动保存/并发布置；V-05/V-06/V-07 的实验矩阵；M0 出口验收。未评价不存在的组 6 回填文本。

## 3. 独立重算与核验

### 3.1 夹具先行

- 提交 `99867bf`（`test(aline-markdown): 固定组 6 夹具与块金样`，提交时间 2026-10-31 15:15:56 +0800，即 07:15:56Z）只新增组 6 的夹具/金样文件，不包含实现源码。
- 夹具文件 mtime 为 15:14:28–15:15:45 +0800；实现源码 mtime 为 16:42–16:43 +0800；定向证据命令时间为 16:48 +0800。实现晚于夹具约 87 分钟。
- 结论：在本次可观察的 Git 历史与文件时间上，夹具先于实现成立。局限：mtime 本身可被改写，本报告不以 mtime 单独证明“先写期望、后写实现”；但提交内容与时间顺序一致，未发现相反证据。

### 3.2 块/Gap 金样独立重算

- 夹具目录：`crates/aline-markdown/tests/fixtures/m0/`，共 7 个 `.md` 文件（`basic.md` 与 6 个 spec 夹具），不含 `README.md`。
- 我未调用 Rust，先用 Python 按原始字节独立编码每个夹具的期望块行范围与种类，再由源文件字节推导块/间隙区间，最后与两份 TSV 比对。
- 结果：`spec.blocks.tsv` 38 个区间、`basic.blocks.tsv` 19 个区间，合计 57 个；7 个夹具全部满足：
  - 区间严格升序、互不重叠、从 0 无缝覆盖到源字节长度；
  - 每个端点都是 UTF-8 字符边界；
  - 块尾不含 `\n`/`\r`，分隔换行归入 Gap；
  - Gap 只含 `\n`/`\r`；
  - 我独立推导的块种类/行范围与 TSV 一致，0 问题。
- 我最初把 `front_matter_plus.md` 的 Paragraph 行号写在自己的推导里（第 5 行），与夹具实际不符；修正的是我的推导脚本，不是仓库金样。修正后与 TSV 完全一致，这说明本次比对不是拿实现输出反向填充。
- 仓库自带的 `verify_goldens.py` 只检查结构约束，不判断块种类是否正确；我的重算在此之上加入了独立块种类/行范围推导。两者结果一致，均为 0 问题。
- 证据：`docs/validation/m0-engine-foundation/group-6/step-6.8/goldens-independent.stdout.log`（实现者证据）与本次 `09:08:56Z` 重算；Rust 侧 `cargo test -p aline-markdown --test spec` 8 passed。

### 3.3 映射契约手算与探针

我在不读取实现输出的情况下先手算以下用例的片段、来源范围、精确性、规范锚点，再用仓外探针 `<tmp>/aline-review-probe` 对照；结果全部一致，也与 `step-6.4/goldens/mapping-goldens.tsv` 的 6 个用例一致。

| 用例 | 源（转义） | 手算片段（drawn → source，precision） | 规范锚点 |
| --- | --- | --- | --- |
| entities | `a &amp; b\n` | `0..2→0..2 Exact`；`2..3→2..7 Aligned`；`3..5→7..9 Exact`；`5..5→9..10 Hidden`；drawn `a & b` | drawn 2 Before=2，After=7 |
| multichar_entities | `&NotEqualTilde; &#x1F600;\n` | `0..5→0..15 Aligned`；`5..6→15..16 Exact`；`6..10→16..25 Aligned`；`10..10→25..26 Hidden`；drawn `≂̸ 😀`（UTF-8 5+1+4 字节） | drawn 5 Before=After=15 |
| soft_break | `a\nb\n` | `0..1→0..1 Exact`；`1..2→1..2 Exact`；`2..3→2..3 Exact`；`3..3→3..4 Hidden`；drawn `a\nb` | drawn 1 Before=After=1 |
| hard_break | `a  \nb\n` | `0..1→0..1 Exact`；`1..2→1..4 Aligned`；`2..3→4..5 Exact`；`3..3→5..6 Hidden`；drawn `a\nb` | drawn 1 Before=1，After=4 |
| br_newline | `a<br>b\n` | `0..1→0..1 Exact`；`1..2→1..1 Synthetic`；`2..2→1..5 Hidden`；`2..3→5..6 Exact`；`3..3→6..7 Hidden`；drawn `a\nb` | drawn 1 Before=After=1 |
| hidden_synthetic | `**x** y<br>z\n` | `0..0→0..2 Hidden`；`0..1→2..3 Exact`；`1..1→3..5 Hidden`；`1..3→5..7 Exact`；`3..4→7..7 Synthetic`；`4..4→7..11 Hidden`；`4..5→11..12 Exact`；`5..5→12..13 Hidden`；drawn `x y\nz` | drawn 0 Before=2；drawn 3 Before=After=7 |

核验结论：

- 实体解码、一源多字符、隐藏/合成、SoftBreak/HardBreak/`<br>` 的来源与精确性符合 `docs/engine-design.md` §7.1 的映射表；多字符实体与 emoji 的 drawn 宽度按 UTF-8 字节计算正确。
- 测试没有要求整源双射；`identity_roundtrip_covers_all_char_boundaries` 只对恒等投影（合法双射）遍历边界，其余用例比较规范锚点/来源范围。
- 读码未发现 `clamp`/`backscan` 修补：`mapping/projection.rs:350` 与 `reuse.rs:87` 只有“不 clamp”的文档说明，源码中未出现 `clamp`、从尾部回找或静默吸附；UTF-16 对代理对中间位置与越界位置返回错误。
- 缺口（F3-1）：任务 6.4 明列的 ZWJ、重复文字、空白用例没有独立测试；`mapping-goldens.tsv` 未被任何 Rust 测试或脚本消费，只是证据文件；局部投影与 UTF-16 的组合覆盖也不完整。本次手算只能证明这 6 个用例，不证明全类。

### 3.4 HTML 与保护

已验证的部分：

- 支持标签样式映射覆盖 PRD 的 `b/strong`、`i/em`、`s/del/strike`、`u`、`mark`、`code/kbd`、`a`；`a` 只在有效 href（`#`、`/`、`http`、`https`、`mailto`）时获得链接样式，其他属性不成为执行入口。
- 测试覆盖：支持标签与嵌套样式、无 href/非法 href、引号内 `>`、交叉嵌套、注释/PI 隐藏、`&amp;` 实体映射、`script` 过滤为字面、未闭合 `b`、HTML 块保护。
- 保护：脚注引用/定义相邻时按子区间保护；行内 HTML 按标签保护；HTML 块整体保护；字面代码不触发保护；RawFallback 整源保护。
- 预算失败回退：`build.rs:274-279` 对 `HtmlError::BudgetExceeded` 返回 `BuildError::BudgetExceeded`，`parse.rs:234-237` 在构树失败时对整个快照调用 `raw_fallback`；`raw_fallback`（`parse.rs:274-322`）生成一个 `Raw` 块、`RawFallback` 保护、一个诊断，并保留完整源文本。不是“只放弃当前事件”的局部回退，不会吞掉此前已隐藏的标签源码。`html.rs` 的 `html_budget_failure_falls_back_to_whole_source` 与 `protection.rs` 的 `raw_fallback_protects_the_whole_source` 与此一致。
- `diagnostic.rs` 只有 `RawFallback` 与 `BudgetExceeded` 两种，和测试使用一致；但 `RawFallback` 诊断分支没有直接用例（见 F3-3）。

与设计不一致的部分（F2-1）：

- `tree/html.rs:474-490` 的过滤集包含 `script/iframe/style/object/embed/form/input/button/link/meta/base/svg/math`，但缺少设计 §11.2 所列的 `title/textarea/xmp/noembed/noframes/plaintext`；且 `is_void_tag`（`tree/html.rs:467-472`）在过滤判断之前执行，`input/link/meta/base` 被整体隐藏，而不是按字面文本显示。
- 没有实现“HTML 多个空白 → 一个空格”的多源合并，也没有记录被消费的空白范围（设计 §7.1、§11.2）。外部探针 `<b>a   b\n c</b>` 的 drawn 保留多个空格，未折叠。
- 没有实现“块级 HTML 标签产生换行”的合成换行；`handle_tag` 的 `_block` 参数（`tree/html.rs:124`）未使用，探针 `<div>a</div><div>b</div>` 的 drawn 为 `ab\n`，两个块之间没有合成换行。
- 探针 `<title>x</title>` 的 drawn 为 `<title>x`（关闭标签被隐藏）；`<textarea><b>x</b></textarea>` 的内部 `b` 仍被当作样式标签。这两种行为都不符合 PRD 过滤集按字面显示的预期。
- 测试只覆盖 `b/i/a` 等少量支持标签与 `script` 一个过滤标签；没有十进制/十六进制实体、多字符实体、过滤标签嵌套、仅标签零可见文字、`title/textarea` 等用例。任务 6.5 所列“支持标签、过滤标签、注释/空白”的证据不足以证明 §11.2 的完整语义。
- 这些缺口不影响本次“整源回退不吞源码”的结论，但在 V-07 冻结前不能宣称有限 HTML 已完成。

### 3.5 预算与敌意输入

- 预算字段与注入接口齐全：`ParseBudget` 有 source/rewritten/segments/scan steps/nesting/nodes/html depth/mapping points 上限，均有 `with_max_*` 注入；`budget.rs:1` 与 `:17` 明确说明这些是“可注入的保护上限，不是发布性能限额”“不冻结为产品阈值”。
- 测试使用注入值（如 nesting 8/400、html depth 8、nodes 4、source 3、rewritten 2、scan steps 0/低/中/阈值），没有把 `ParseBudget::m0()` 的数值断言为产品默认；超限路径返回预算错误或整源回退，保留完整源文本，未发现截断或半树发布。
- 敌意测试覆盖深栈、重复未闭合标签、重复未闭合引号、巨段（200k 字符）、巨表（2000 行）、节点/源/重写/扫描步预算；扫描步测试用二分找到阈值，说明扫描步数确实被计入。
- 缺口（F2-3/F3）：
  - `aline-markdown` 的源码与测试中没有 `cancel` 接口或用例（workspace 其他 crate 的取消测试不属于组 6）；任务 6.6 要求“取消检查”，设计 §12.3 也只是“建议可注入预算/取消检查接口”。当前只有预算上限，没有可注入的取消回调/令牌。
  - `max_mapping_points` 有注入接口，但没有任何测试或示例使用它；映射点预算未被验证。
  - 没有记录实际扫描步数、预算命中数与第三方 `pulldown-cmark` 长调用限额盲区；测试只断言阈值处成功/失败，未形成任务 6.6 要求的记录。

### 3.6 复用

- 实际复用命中已确认，不是“全回退冒充优化”：
  - 前文插入的普通文档：3 对头尾配对；
  - 单段内部编辑：2 个外层块配对；
  - 文档末尾追加：4 对文本/种类/范围一致；
  - 无关文档：0 对。
- 顶层全局依赖拒绝：front matter、脚注、TOC、围栏、列表、顶层引用定义在至少一个方向上拒绝；配置变化与 RawFallback 拒绝；HTML 在一个方向拒绝。
- 旧绝对范围：对新快照使用旧范围必须失败；`translate` 对旧块范围返回重定位，对块外偏移返回 `None`，不做 clamp。
- 实际缺陷（F2-2）：
  - `reuse.rs:243-248` 的 `has_reference_definition` 只看 `trim_start` 后以 `[` 开头且包含 `]:` 的行，无法识别块引用内的 `> [x]: /u`；`global_dependency` 也不拒绝 `BlockKind::BlockQuote`。
  - 外部探针验证：`> [x]: /u\n\n[x]\n` 把定义目标改为 `/v` 后，`reuse` 返回 `refusal=None`、1 对命中（Paragraph 11..14 原样复用）；删除/替换该定义也返回命中。`pulldown-cmark` 仍把 `[x]` 解析为指向 `/v` 的链接，因此后文链接语义已变而复用结果未失效。
  - 块引用内的围栏（`> ``` `）同样不被 `has_fence_line` 识别，修改后仍返回命中。
  - 这与设计 §12.1“引用定义、脚注、目录、围栏或列表结构可能影响远处”及“样本包含前文引用定义改变后文链接”直接冲突；任务 6.7 要求比较导航/映射，当前只比较了文本/种类/范围。
- 差分覆盖缺口（F3-6）：`tests/reuse.rs` 的 `reused_text_matches_full_reparse_block_by_block` 只比较文本、种类、范围；没有比较映射来源/精确性、保护范围、样式/导航目标。列表/围栏/引用定义只测了顶层，HTML 只测了一个方向。

### 3.7 测试合规与依赖

- `crates/aline-markdown/{src,tests,examples}` 中未发现 `.unwrap()`、`.expect(`、`panic!`、`todo!`、`unimplemented!`（grep 0 命中）；`examples/dump_tree.rs` 同样未使用这些宏。
- workspace lint 设置 `unsafe_code = "forbid"`、`unwrap_used/expect_used/panic = "deny"`；本次 fmt/clippy 均为 0。
- 依赖树：`aline-markdown` 直接依赖仅 `aline-buffer` 与 `pulldown-cmark v0.13.6`（传递 `bitflags 2.13.2`、`memchr 2.8.3`、`unicase 2.10.0`）；根 `Cargo.toml:28` 为 `pulldown-cmark = { version = "=0.13.6", default-features = false }`。未发现新增第三方依赖。
- 说明：复核期间证据目录新增的 `step-6.4/goldens-check/probe-main.rs` 是仓外探针源码副本（含 `unreachable!`），不属于 crate 源码、测试或示例，也不在 workspace 编译图中；本复核不依赖该探针，未把它计入“示例禁 unwrap”的合规判断。

### 3.8 证据完整性

- `docs/validation/m0-engine-foundation/group-6/commands.log` 共 13 行，UTC 08:48:11–08:48:13Z，全部 `exit=0`；命令、环境、git 状态、定向测试、dump、全 crate 与独立金样日志与记录一致。未发现伪造日志的迹象。
- 定向测试计数与本次重跑一致：spec 8、rewrite 6、mapping 10、html 11、protection 6、hostile 10、reuse 7；全 crate 合计 62（58 个定向 + 2 identity + 2 public API），workspace 合计 134。
- `step-6.8/workspace-gates/summary.json` 顶层 `passed=true`，19 项结果全部 `exit_code=0`；`env_overrides` 明确记录三个环境变量；作用域为当前 macOS，未记录 CI。本次统一脚本重跑同为 19/19。
- 环境日志只打印 `CARGO_HOME` 的现象已复现：本机 macOS `printenv A B C` 只打印第一个存在的变量。因此不能据此判断 `RUSTUP_TOOLCHAIN`/`CARGO_NET_OFFLINE` 未设置；`run-targeted-tests.sh` 与 `workspace-gates/summary.json` 均记录三者，rustc/cargo 版本也与固定工具链一致（F3-7）。
- 组 6 证据全部未跟踪、未提交；复核期间 `step-6.4/goldens-check/` 于 09:10:14Z 被追加（非本报告产物），说明证据集不是不可变快照。本报告固定了读取时间，晚到的追加证据不在覆盖范围（F3-8）。

## 4. 逐项结论（6.1–6.8）

总体：6.1–6.4 通过；6.5–6.7 按 M0 范围通过，但分别附 F2-1/F2-3/F2-2，不得把 F2 缺口写成已完成；6.8 阻塞。F1 共 1 个，F2 共 3 个，F3 共 8 条。以下“通过”只表示 M0 任务范围，不代表 V-06/V-07/AC-09 或 M0 出口通过。

### 6.1 夹具与显式 Options —— 通过（附 F3-4）

- 夹具先行成立；`ParseFeatures` 显式映射 `TABLES/FOOTNOTES/STRIKETHROUGH/TASKLISTS/MATH/GFM`，M0 配置版本 1、bits `0x00000c1e`，dump 首行一致；未启用原生 metadata 选项，front matter 由本 crate 识别。
- `--test spec` 8 passed：夹具金样、basic 金样、字面代码避让、未闭合 front matter 不吞全文、`---`/`+++` 配对、GFM 种类、dump 内容、空文档。
- 缺口：任务要求的“各语法走原生选项或必要 pass 的依据”没有落盘记录；Mermaid 只识别来源/product 限制没有专门夹具或测试；dump 只做了子串断言，人工金样审查没有记录。这些是证据强度限制，不阻塞 M0 解析骨架。

### 6.2 RewriteMap 与失败回退 —— 通过（附 F3-4）

- `--test rewrite` 6 passed：front matter 隐藏并映射回源、多编辑逐段来源、两级 compose、控制字节 U+0001–U+0008 不碰撞、倒序/重叠/非字符边界/越界拒绝、预算失败整源回退。
- 读码确认 `Rewrite::apply` 校验顺序、compose 与 `to_source` 的合成/近似来源；映射失败回退原文而非局部截断。
- 缺口：任务点名的“合成围栏”没有独立用例（现有测试只覆盖 front matter 掩蔽与合成插入/控制字节）；跨片段事件来源主要靠构树测试间接覆盖。F3。

### 6.3 块/Gap 与 dump —— 通过（附 F3-2）

- 7 个夹具、57 个区间经独立 Python 重算 0 问题；Rust `--test spec` 8 passed；`step-6.3/dump-tree.stdout.log` 含 config/source/drawn/block/gap/protected，基本与金样一致。
- 空文档无块/无 Gap；嵌套、无尾换行由夹具覆盖；块尾分隔换行归 Gap 的结构约束通过。
- 缺口：任务明列的“多空行”没有专门测试；dump 测试只检查包含关系，没有逐行对金样的自动比较；人工审查没有留记录。F3。

### 6.4 映射与 UTF-16 —— 通过（附 F3-1）

- `--test mapping` 10 passed；恒等投影边界往返、UTF-16 代理对中间拒绝、实体/转义 Aligned、多字符/数字实体、Soft/Hard Break、`<br>` 合成换行、自定义隐藏/合成、局部投影越界拒绝、选区方向、旧投影/旧快照拒绝。
- 手算 6 个用例与外部探针一致；未发现整源双射要求、clamp 或 backscan；规范锚点在合法边界稳定，隐藏/合成不冒充精确命中。
- 缺口：ZWJ、重复文字、空白没有独立测试；`mapping-goldens.tsv` 未被测试或脚本消费；局部投影 + UTF-16 的组合证据不足。F3。

### 6.5 有限 HTML 与保护 —— 通过（M0 有限范围，附 F2-1/F3-5）

- 已验证：支持标签样式（含 `a` 有效 href）、引号内 `>`、交叉嵌套、注释/PI、`&amp;`、`script` 字面、未闭合 `b`、HTML 预算失败整源回退；保护子区间/整块/字面代码/整源回退均有测试。
- 已证实缺口（F2-1/F3-5）：过滤集缺少 `title/textarea/xmp/noembed/noframes/plaintext`，`input/link/meta/base` 因 void 判断先于过滤被整体隐藏；未实现 HTML 空白折叠与块级标签合成换行；测试只覆盖 `b/i/a` 与 `script`，没有 §11.2 金样清单中的多字符/十进制/十六进制实体、过滤标签嵌套、仅标签零可见文字等。该缺口不影响整源回退结论，但不能宣称 §11.2 有限 HTML 完成；必须在 V-07 固定/M1 前补齐或明确记录为实验限制。

### 6.6 预算、敌意输入与取消 —— 通过（M0 预算范围，附 F2-3/F3）

- 已验证：source/rewritten/segments/scan/nesting/nodes/html depth 预算的注入与超限回退；深栈、重复未闭合标签/引号、巨段、巨表、节点/源/重写/扫描步测试全部通过；预算数值未被冻结为产品默认。
- 已证实缺口（F2-3/F3）：没有可注入的取消检查/令牌，也没有取消用例；`max_mapping_points` 没有测试；未记录实际扫描步数、预算命中与第三方 `pulldown-cmark` 长调用盲区。按 M0 预算范围可通过，但不得宣称任务 6.6 的取消能力已完成；设计 §12.3 将取消列为建议接口，必须在 M1 后台解析前补齐并作为已知限制记录。

### 6.7 复用与重定位差分 —— 通过（M0 范围，附 F2-2/F3-6）

- 已验证：普通头尾配对真实命中（3 对/2 对/4 对），非全回退；顶层 front matter/脚注/TOC/围栏/列表/引用定义拒绝；配置变化与 RawFallback 拒绝；旧绝对范围失败；`translate` 不做 clamp；不相关文档 0 对。
- 已证实缺口（F2-2/F3-6）：块引用内的引用定义与围栏是文档级全局依赖，但 `has_reference_definition`/`has_fence_line` 只看行首，`global_dependency` 不拒绝 BlockQuote；外部探针证明定义目标改变/删除后仍返回 1 对命中。这与设计 §12.1 的“前文引用定义改变后文链接”样例冲突。差分测试只比较文本/种类/范围，没有比较映射、保护、导航。按 M0 范围可通过，但该缺陷不损坏文本，仍必须在 M1 链接/导航消费者接入前修复。

### 6.8 文档回填与 V-06/V-07 基础夹具 —— 阻塞（F1-1）

- `tasks.md` 6.1–6.8 仍为 `[ ]`；`docs/engine-design.md` 未回填 §6/7/11/12，未记录解析配置（`ParseConfig`/`ParseFeatures`/运行期 Options），§7.1 仍写着组 6 映射“仅契约未验证”，依赖章节仍写着“运行期 Options 与映射/HTML 金样仍须第 6 组验证”。
- 未发现 V-06/V-07 的基础夹具或报告文件（`find docs -iname '*v-06*' -o -iname '*v-07*'` 为空）；SoftBreak 的换行/空格口径、有限 HTML 的实验结果与缺口没有逐项落盘。
- 因此 6.8 不满足可勾选条件。该阻塞是文档/证据前置条件，不是 Rust 代码缺陷；在 6.8 完成前，即使 6.1–6.7 可勾选，也不能据此判断 M0 出口。

## 5. 问题分级

### F1 阻塞（勾选任务前必须修复）

- **F1-1（6.8）**：组 6 文档回填与 V-06/V-07 基础夹具未完成；`tasks.md` 未勾选；`docs/engine-design.md` §7.1/§6.2 相关旧记录未更新。没有组 6 报告可供核对。必须在勾选 6.8 前完成回填，并逐项列出 SoftBreak、有限 HTML、复用全局依赖、预算/取消的实验结果与缺口。

### F2（M1 前处理）

- **F2-1（6.5）**：有限 HTML 与设计 §11.2/§7.1 不一致：过滤集缺少 `title/textarea/xmp/noembed/noframes/plaintext`，`input/link/meta/base` 被整体隐藏；无 HTML 空白折叠与块级标签合成换行；HTML 测试覆盖不足。M1/V-07 冻结前必须补齐或明确记录为实验限制，不能宣称 §11.2 完成。
- **F2-2（6.7）**：复用全局依赖漏掉块引用内的引用定义与围栏；定义目标改变/删除后仍命中复用，违背 §12.1。M1 链接/导航/样式消费者接入前必须修复，并补块引用、列表内定义、脚注重排等差分用例。
- **F2-3（6.6）**：没有取消检查接口/用例；`max_mapping_points` 未验证；未记录实际扫描步数与第三方长调用盲区。M1 后台解析/可取消任务接入前必须补齐。

### F3（证据强度限制）

- **F3-1（6.4）**：ZWJ、重复文字、空白测试缺失；`mapping-goldens.tsv` 未被自动消费；局部投影 + UTF-16 组合覆盖不足。
- **F3-2（6.3）**：多空行块/Gap 测试缺失；dump 仅子串断言；人工金样审查无记录。
- **F3-3（6.5/protection）**：`RawFallback` 诊断分支无直接用例，只有 `BudgetExceeded` 回退被测；整源回退代码路径已在读码中确认。
- **F3-4（6.1/6.2）**：解析选项依据记录缺失；Mermaid 来源识别无测试；Rewrite “合成围栏”无独立用例。
- **F3-5（6.5）**：HTML 金样覆盖窄（仅 `b/i/a` 与 `script`，实体只有 `&amp;`），无过滤标签嵌套、仅标签零可见、十进制/十六进制/多字符实体等。
- **F3-6（6.7）**：复用差分只比较文本/种类/范围；未比较映射来源/精确性、保护范围、导航目标；列表/围栏/引用定义仅测顶层；HTML 只测一个方向。
- **F3-7（证据）**：环境日志只显示 `CARGO_HOME` 是 macOS `printenv` 多参数行为，不是环境未设置；workspace gates 仅当前 macOS，无 CI/其他 target 记录。
- **F3-8（证据）**：组 6 证据未提交，且复核期间仍有追加（`step-6.4/goldens-check/` 09:10:14Z）；本报告只覆盖固定快照，晚到证据未复核。

## 6. 明确不声称

- 本报告**不判定 V-01～V-09、AC-09 或 M0 出口通过**。本报告只复核组 6 的代码、测试、夹具与证据强度。
- 本报告**不把 `ParseBudget::m0()` 的数值当作 V-06 发布限额**；这些是可注入的保守上限，V-06 未执行。
- 本报告**不冻结 SoftBreak 投影为换行或空格**；该口径由 V-07 决定。本次只验证当前实现把 SoftBreak 作为带来源的换行/精确片段，不宣称这是最终产品语义。
- 本报告**不宣称公式/Mermaid 渲染能力**；只确认 M0 仅保留/识别来源，无渲染后端。
- 本报告**不宣称有限 HTML 已完成**；F2-1 的过滤集、空白折叠、块级合成换行仍缺口。
- 本报告**不宣称复用全局依赖已完整**；F2-2 的块引用定义/围栏是已证实的反例。
- 本报告**不宣称任务已勾选**；复核时 6.1–6.8 全部未勾选。即使后续勾选 6.1–6.7，也**不等于** M0 出口通过；tasks checked ≠ exit passed。
- 本报告不评价实现者的姓名、会话或未落盘的自述；所有结论以仓库文件与本次重跑为准。

复核者：独立子代理（非实现者）
