# 组 7 第二轮独立复核报告（review-round-2）

日期：2026-10-31（UTC）。复核者：独立子代理（非实现者、非调度方）。范围：OpenSpec change `m0-engine-foundation`，组 7（`aline-edit`，任务 7.1–7.4）的 round-1 遗留项处置、7.4 回填诚实性与回归。本文件是本轮唯一写入仓库的结论文件；本轮自有日志在 `docs/validation/m0-engine-foundation/group-7/review-round-2-evidence/`。未修改代码、测试、金样、其它文档或 `tasks.md`，未提交。

## 1. 快照与方法

### 1.1 快照

| 项 | 值 |
|---|---|
| HEAD | `99867bf2c5775aae4e8ed910713496d4ff84323c`（复核开始时的 `git status` 快照与 10:40:18Z 的 `git rev-parse HEAD` 一致） |
| 证据窗口 | 2026-10-31T10:21:05Z–10:42:33Z（本报告在窗口结束后撰写） |
| 工作区状态 | 未冻结：复核期间组 6 继续写 `docs/engine-design.md`（mtime 10:35:43Z）、`docs/v1-validation-plan.md`（mtime 10:36:17Z）与 `crates/aline-markdown`；金样索引状态在窗口内从 `A ` 变为 `??`。详见 §4.5 |
| 环境 | macOS 27.2 / Darwin 27.2.0 arm64；`rustc 1.98.1 (48a229cea 2026-09-01)`；`cargo 1.98.1`；`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`；`RUSTUP_TOOLCHAIN=1.98.1`；`CARGO_NET_OFFLINE=true` |

读取的材料：`openspec/changes/m0-engine-foundation/specs/engine/edit/spec.md`（2 个 ADDED Requirement、6 个 Scenario）、`tasks.md` 第 72–75 行（7.1–7.4 仍为 `[ ]`）、四份 step 报告、round-1 `review.md`、`crates/aline-edit/src/{plan,reread,validate,refusal,budget,command,permissions,errors,lib}.rs`、`crates/aline-edit/tests/{goldens,refusals,protection}.rs`、金样 TSV、实现者与调度方的 `commands*.log`/`summary.json`/`test.stdout.log`。

### 1.2 实际运行（本复核独立执行）

| UTC | 命令 | 退出 | 结果 | 日志 |
|---|---|---|---|---|
| 10:21:05–10:21:09 | `cargo test -p aline-edit --locked --offline` | 0 | 25 passed（lib 0、goldens 6、protection 4、refusals 15、doc-tests 0） | `cargo-test-aline-edit.log` |
| 10:21:16–10:21:18 | `cargo clippy -p aline-edit --all-targets --locked --offline -- -D warnings` | 0 | 无 warning | `cargo-clippy-aline-edit.log` |
| 10:21:20–10:21:22 | `cargo fmt --all --check` | 1 | 仅并发写入的 `crates/aline-markdown/examples/probe_events.rs`、`src/budget.rs` 差异，不涉及 `aline-edit` | `cargo-fmt-check.log` |
| 10:31:39–10:31:41 | `cargo fmt -p aline-edit -- --check` | 0 | 输出为空（0 字节） | `fmt-aline-edit-package.log` |
| 10:31:59–10:32:03 | `cargo test -p aline-edit --locked --offline`（第二遍） | 0 | 25 passed（同上） | `cargo-test-aline-edit-2.log` |
| 10:32:03–10:32:04 | `cargo clippy -p aline-edit --all-targets --locked --offline -- -D warnings`（第二遍） | 0 | 无 warning | `cargo-clippy-aline-edit-2.log` |
| 10:32:04 | `cargo fmt --all --check`（第二遍） | 0 | 无输出 | `cargo-fmt-check-2.log` |
| 10:33:47–10:33:49 | 仓外探针 `/tmp/aline-g7-r2/probe`：`cargo run --offline --quiet` | 0 | 14 个用例；`buffer state unchanged: true` 14 次、`false` 0 次 | `probe-run.log` |
| 10:34:01 | 只读静态核对（porcelain、sha256、diff、TSV 分类、索引、mtime、写入通道扫描） | 0 | 见日志 | `static-checks.log` |
| 10:38:13 | 文档最终快照 | 0 | 见日志 | `docs-snapshot-final.log` |
| 10:40:18–10:40:19 | 结束快照（HEAD、mtime、金样哈希与索引状态） | 0 | 见日志 | `final-head-mtimes.log` |
| 10:42:33 | `git status --porcelain`（本文件、证据目录、金样） | 0 | 见日志 | `final-status.log` |

未运行：workspace 级 `cargo test --workspace`、`cargo clippy --workspace`、`scripts/check_workspace.py`。对实现者/调度方的 workspace 门禁只核对了 `summary.json`、`commands.log` 与 `test.stdout.log`，未独立重跑，因此不把它当作本轮独立通过证据（见 F3-5）。

## 2. round-1 遗留项逐项判定

判定语义：**成立**＝在当前工作树仍可复现该主张，且 round-1 允许的处置未落实或未落实到位；**部分成立**＝核心已处置，但仍有一部分子项/证据未闭合；**未成立**＝按 round-1 明确允许的处置核对后，当前工作树已无可复现的未闭合缺陷依据（不等于该问题从未存在）。

| ID | round-1 主张（摘要） | 本轮核对 | 判定 |
|---|---|---|---|
| F1-1 | 四份 step 报告缺失；`goldens.rs:291` 引用不存在的 `step-7.1/report.md` | 四份报告均存在：`step-7.1/report.md`（10:05:47Z）、`step-7.2`（10:06:02Z）、`step-7.3`（10:06:57Z）、`step-7.4`（10:21:01Z）；`goldens.rs:281/291` 的例外注释明确指向 `step-7.1/report.md`，§3 即勘误 | **未成立** |
| F1-2 | 7.4 回填缺失（引擎 §8/§9/§14、架构 §7.1/§7.2、V-07/AC-04） | 四份文档均有组 7 记录：引擎 4 条、架构 2 条、PRD 1 条、验证计划 1 条，合计 16 行；`git diff docs/` 删除行为 0；AC-04/V-07“未通过”与未覆盖命令已写入 | **未成立** |
| F1-3 | `allow_source_footnote` 目标金样 `7..7` 错误，应为 `8..8` | 金样仍为 `7..7`；但 round-1 指定的处置已全部落实：step-7.1 §3 记录勘误、`goldens.rs` 例外引用报告、实现为 `8..8`、金样未回填 | **未成立**（按指定处置闭合；金样不构成单一真值这一证据缺陷仍被保留并已书面记录） |
| F2-1 | Visual Replace 未绑定选区，M1 前须约束 | round-1 允许的第二种处置已落实：`plan.rs:218–224` 定义“命令范围权威、`before_selection` 只用于 validate”的协议；`refusals.rs:423/457` 两个测试覆盖成功计划与 validate 语义 | **未成立** |
| F2-2 | `semantic_hidden` 只比非空白、不比较边界 | 核心已修：非纯空白隐藏片段保留内部空白，链接标题内空格删除被拒绝并有真实缓冲测试（`refusals.rs:401`）；纯空白片段与片段边界仍不比较，已写成 V-07 已接受限制（`reread.rs:144–149`），但该限制无测试钉住，且块结构后果未写入回填 | **部分成立** |
| F3-1 | 真实 `StringBuffer` 上缺 BudgetExceeded/RereadFailed/ProtectedRange/HtmlBoundary/AlreadyFormatted 用例 | `refusals.rs` 15 例，新增 8 例覆盖上述类别；全文件 17 处 `assert_eq!(capture(&buffer)?, before)`；探针 14/14 状态不变 | **未成立** |
| F3-2 | MD-11 粘贴无独立用例；HTML 跨界格式无独立用例 | `step-7.3/report.md:28` 仍承认“没有单独的剪贴板/UI 用例”；`:31` 仍承认只有左端触及的一条金样，右端/整元素格式未覆盖 | **成立** |
| F3-3 | `manual-write-channel-review.log` 无具体命令，不可复现 | 日志仍只有结果；`step-7.4/report.md:54` 明确承认“命令不可复现”，只给出命令形状且不声称已在日志中运行 | **成立** |
| F3-4 | 金样先行证据强度限制；`+0800` 被标成 `Z` | 原始日志 `goldens-pre-implementation.log:4` 仍是 `mtime=2026-10-31T17:08:45Z`；`step-7.1/report.md:20` 已如实披露真实 epoch（09:08:45Z）；mtime/索引仍非密码学证明，测试文件晚于实现（09:19:00Z） | **成立**（限制仍在，已如实披露，不阻断 M0） |
| F3-5 | 环境与 workspace 门禁限制 | `step-7.4/report.md:126` 已解释 `printenv` 多参数只打印第一个变量，`summary.json` 记录三项；但 workspace 门禁仍只覆盖 macOS arm64，且我未独立重跑，工作区在复核期间仍在变化 | **部分成立** |
| F3-6 | 字面命令 `cargo test -p aline-edit refusal` 未运行 | 仍未运行；`step-7.2/report.md:67` 承认；实际运行 `--test refusals`（整目标 15 例，强于名字过滤） | **成立** |
| F3-7 | 无真实 UI/DocCore 消费者 | 仍无；`step-7.4/report.md:62` 承认只能由 API 结构证明 | **成立** |
| F3-8 | 证据全部未跟踪；复核期间并发写入 | 组 7 证据仍全部未跟踪；复核期间文档、`aline-markdown` 与索引继续变化 | **成立** |

### 2.1 F1-3：勘误算术复核

金样 TSV 第 310–325 行（`step-7.1/goldens/edit-goldens.tsv`）：

```
allow_source_footnote	scenario	plan
allow_source_footnote	source	"text[^1] more\n\n[^1]: note\n"
allow_source_footnote	edit.0.range	4..8
allow_source_footnote	edit.0.expected	"[^1]"
allow_source_footnote	edit.0.replacement	"[^2]"
allow_source_footnote	target_anchor	7
allow_source_footnote	target_head	7
```

手算：源 `"text[^1] more\n\n[^1]: note\n"` 共 26 字节；字节 `[4..8]` 恰为 `"[^1]"`（4 字节）；替换文本 `"[^2]"` 为 4 字节；按统一规则目标 = 最后补丁起点 4 + 替换字节数 4 = **8**，即 `8..8`。金样的 `7` 落在候选 `"[^2]"` 的第 3 个字节之后（`[`、`^`、`2` 之后、`]` 之前），即光标停在标记内部。因此 round-1 的算术主张成立，`8..8` 正确。

但 round-1 的处置要求是“记录勘误并让测试例外引用报告；不要把实现改成 7，不要回填金样”。本轮核对：step-7.1 §3（第 46–54 行）给出同一算术与“不修改实现为 7、不回填金样”；`goldens.rs:281–291` 的 `expected_target` 先按金样断言 `7..7`、再按统一规则返回 `8..8`，注释写明“实现前手工计数笔误”并指向 `step-7.1/report.md`。金样未被回填：sha256 仍为 `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743`，blob 仍为 `2d3703f42f89f64b667321b4ced93ac216111e5b`，mtime 仍为 09:08:45Z（早于实现），10:34:01Z 的 porcelain 仍为 `A `。因此该遗留项按指定处置闭合；剩余的是“金样不是单一真值”这一证据缺陷，已在 `step-7.1/report.md:54` 明示。

### 2.2 F2-2：隐藏空白反证复核

`reread.rs:138–149` 的契约与实现一致：非纯空白隐藏片段保留原文（含内部空白）；纯空白隐藏片段整体跳过（`:160`）；片段边界与位置不比较（`:144–149`）。`strip_marker`（`:180–205`）对去标记后仅剩空白的片段同样跳过。

我用仓外探针（与仓库实现同一 workspace 依赖）构造了 14 个反例，全部以 `selection=0..0`、Visual `Replace` 执行；任何已改变绘制文本的隐藏改动都被拒绝：

| 探针用例 | 编辑（源码字节） | 结果 |
|---|---|---|
| `link_title_internal_ws` `[x](https://e "a  b")` | 16..17 删标题内一个空格 | `RereadFailed { HiddenSourceChanged }` |
| `heading_marker_space` `# Title` | 1..2 删 `# ` 中空格 | `HiddenSourceChanged` |
| `list_marker_space` `- a` | 1..2 删列表标记空格 | `HiddenSourceChanged` |
| `quote_marker_space` `> a` | 1..2 删引用标记空格 | `HiddenSourceChanged` |
| `fence_info_space` ` ``` rust` | 3..4 删 info string 空格 | `HiddenSourceChanged` |
| `hardbreak_spaces` `a  \nb` | 1..3 删硬换行尾随空格 | `VisibleTextChanged` |
| `footnote_body_ws` | 17..18 删脚注正文内空格 | `ProtectedRange` |
| `html_attr_ws` | 5..6 删 HTML 属性内空格 | `HtmlBoundary` |
| `paragraph_half_separator` `a\n\nb` | 1..2 删一个换行 | `VisibleTextChanged` |
| `extra_blank_line` `a\n\n\nb` | 1..3 删到只剩一个空行 | `VisibleTextChanged` |

未能构造出“可见文字改变却被接受”的反例。残余的已接受限制是纯空白隐藏片段整体删除，例如：

- `paragraph_separator`：源 `"a\n\nb\n"`，删除 1..3（隐藏片段 `"\n\n"`）→ `Changed`、`validate Ok`，候选 `"ab\n"`；绘制文本前后都是 `"ab"`，但块种类由 `[Paragraph, Paragraph]` 变为 `[Paragraph]`。
- `list_paragraph_separator`：源 `"- a\n\nb\n"`，删除 3..5 → 候选 `"- ab\n"`；绘制文本不变，块种类由 `[List, Paragraph]` 变为 `[List]`。

这两例的绘制文本未变，故不构成“可见文字改变被放行”的反例；但块结构确实变了，而 `Visible` 契约不比较块种类。该限制在 `reread.rs:144–149` 与 V-07 回填中写成“纯空白片段与片段边界不比较”，但没有测试钉住，回填也未写明块结构合并这一后果（N-F2-1）。另外 `softbreak`（删 1..2）被接受且绘制文本由 `"a\nb"` 变 `"ab"`，但这是命令显式删除一个 Exact 段内的可见换行，属于命令语义本身，不是隐藏片段被静默改写。

### 2.3 F2-1：协议与测试复核

`plan.rs:217–230` 的注释明确：Visual Replace 的权威范围是命令给出的编辑范围；计划不把它扩到选区，也不要求与选区相等；`before_selection` 只用于 `validate_change` 确认选区未被外部改动；`ToggleInlineFormat` 例外，额外要求范围等于选区。`refusals.rs:423` 的 `visual_replace_range_is_command_authority` 以选区 0..0、命令范围 6..10 得到成功计划、目标 10..10、预览 `alpha BETA` 且真实缓冲不变；`:457` 的 `visual_replace_validates_bound_selection_only` 用另一选区 validate 得到 `SelectionMismatch`，证明只校验“当前选区 == `before_selection`”。这正是 round-1 允许的第二种处置（定义协议并加测试），且 `plan.rs:222` 明写 M1 消费者必须由选区派生命令范围，M0 无 UI 无法强制。判定未成立（无未闭合缺陷），残余的仅是 M1 消费者约束，已被记录。

### 2.4 F1-1 / F1-2：报告与回填复核

四份 step 报告均存在且内容覆盖 round-1 要求：7.1 金样勘误、7.2 拒绝矩阵与完整状态、7.3 覆盖/未覆盖、7.4 R/S 与场景覆盖。回填的组 7 记录在四份文档中均可逐条定位（最终快照行号：引擎 286、310、334、527；架构 555、569；PRD 404；验证计划 105；行号会因并发组 6 追加而移动，记录标签本身稳定）。`git diff docs/` 在 10:34:01Z 与 10:38:13Z 均为 0 删除行；组 7 追加合计 16 行，内容包含 AC-04/V-07“未通过”、`allow_source_footnote` 勘误、纯空白/边界已接受限制与 M3 命令未覆盖。7.4 报告第 27 行所述“误改规范句后立即还原”与最终 0 删除一致。F1-1/F1-2 无未闭合项。

注：7.4 报告第 3/25 行把整个 `docs/` 的 38 行新增写成组 7 的成果；实际 38 行中组 7 为 16 行、组 6 为 22 行（10:34 快照），最终 44 行中组 7 仍为 16 行、其他 28 行。这是新发现 N-F1-3，不影响“回填存在”的结论。

### 2.5 F3-1：真实缓冲拒绝状态复核

`refusals.rs` 共 15 个测试（文件 665 行），新增 8 例：`budget_exceeded_refusal_keeps_buffer_state`、`reread_failure_refusal_keeps_buffer_state`、`protected_range_refusal_keeps_buffer_state`、`html_boundary_refusal_keeps_buffer_state`、`already_formatted_creates_no_history_step`、`hidden_fragment_internal_whitespace_change_is_refused`、`visual_replace_range_is_command_authority`、`visual_replace_validates_bound_selection_only`。`capture()`（`:101`）比较 `text/anchor/head/revision/undo_depth/redo_depth/history_node_count`，全文件 17 处前后相等断言。我的第二遍 `cargo test -p aline-edit` 复跑 refusals 15 passed；仓外探针 14/14 状态不变。F3-1 的原缺口（预算、重读失败、保护、HTML 边界、AlreadyFormatted 的真实缓冲状态）已补，判定未成立。

### 2.6 F3-2 至 F3-8（简述）

- **F3-2 成立**：粘贴仍无独立用例（`step-7.3:28` 自认），HTML 跨界格式仍只有左端一条金样（`:31`）。这是 7.3 的覆盖缺口，已如实记录，不能写成“粘贴已验证”。
- **F3-3 成立**：人工写入通道审查日志仍无 `rg` 命令与参数，`step-7.4:54` 自认不可复现。我用独立扫描复核了结论（`aline-edit/src` 只有 `plan.rs:242/330` 两处 `CheckedBatch::new`，无 `StringBuffer`/`apply`/`undo`/`redo`/`fs`；依赖只有 `aline-buffer`、`aline-markdown`），但“日志可复现”本身仍未闭合。
- **F3-4 成立**：`goldens-pre-implementation.log:4` 的 `17:08:45Z` 仍是把本地 `+0800` 标成 `Z`；真实 epoch 1793437725 = 09:08:45Z。`step-7.1:20` 已披露，且金样 mtime 与实现 mtime（09:58）仍可区分先后。限制成立但不阻断 M0。
- **F3-5 部分成立**：`printenv` 解释已写入 `step-7.4:126`；workspace 门禁仍只有 macOS arm64 一次记录，我未独立重跑，且复核期间工作区未冻结。
- **F3-6 成立**：字面命令仍未执行，`step-7.2:67` 自认；实际 `--test refusals` 更强。
- **F3-7 成立**：无真实 UI/DocCore 消费者，`step-7.4:62` 自认。
- **F3-8 成立**：见 §4.5，证据仍全部未跟踪且复核期间继续变化。

## 3. 新发现

| ID | 严重度 | 主张 | 位置 |
|---|---|---|---|
| N-F1-1 | 低 | step-7.2 报告的状态行仍写 `--test refusals` 7 例，与同文 §6 的 15 例及实际 `commands-post-fix.log` 不符；应为 15（并注明修复前 7） | `step-7.2/report.md:3`（对照 `:71–86`） |
| N-F1-2 | 低 | 7.4 报告的写入通道表引用 `plan.rs:235`/`plan.rs:323`，这是 09:23:06Z 修复前扫描的行号；当前两个 `CheckedBatch::new` 在 `plan.rs:242`/`330`，报告未标注行号已漂移。结论（仅两处、无写通道）经我独立扫描仍成立 | `step-7.4/report.md:47`；`plan.rs:242/330` |
| N-F1-3 | 中 | 7.4 报告把整个 `docs/` 的“38 行新增”归为组 7 回填；10:34 实测 38 行中组 7 16 行、组 6 22 行，10:38 为 44 行中组 7 16 行、其他 28 行。回填内容本身属实，但指标归因错误 | `step-7.4/report.md:3`、`:25` |
| N-F1-4 | 低 | 7.1 报告“金样先于实现”表把实现源码 mtime 写成 09:18:49Z、测试 09:19:00/09:19:01/09:20:51Z，未标“修复前”；当前 `plan.rs` 09:58:27Z、`reread.rs` 09:59:45Z、`refusals.rs` 09:58:54Z（`goldens.rs` 仍 09:19:00Z）。§7 第 90 行给出修复后时间，可交叉解读，属标注不严谨 | `step-7.1/report.md:14–15`（对照 `:90`） |
| N-F2-1 | 中 | “纯空白隐藏片段与片段边界不比较”的已接受限制没有测试钉住，且块结构后果未记录：删除段落间空行可让 `[Paragraph, Paragraph]` 合并为 `[Paragraph]`，删除列表与段落间空行可让 `[List, Paragraph]` 变为 `[List]`，均通过 `Changed`/`validate Ok`。绘制文本不变，不构成可见文字被改写 | `reread.rs:144–149`；探针 `probe-run.log` 109–124、168–185 行；`step-7.4/report.md:32` |
| N-F3-1 | 中 | 回填后 SHA-256 已漂移：10:38:13Z 时报告的 4 个回填后哈希有 3 个与文件不符（引擎 `7211b277…`→`15a54523…`、架构 `0b6c5cbb…`→`5b6aa421…`、验证计划 `762a1010…`→`1ca293ec…`；PRD `e6cadea4…` 仍一致）。原因是 10:11 之后组 6 继续写入；组 7 记录 16 行与 0 删除仍成立，但报告未给哈希加“截至 10:11:09Z”的作用域 | `step-7.4/report.md:16–23`；`docs-snapshot-final.log` |
| N-F3-2 | 低 | 工作区门禁报告写成 10:04:27Z–10:04:56Z，但 `summary.json` 的最晚 `finished_utc` 为 10:04:53.722Z（文件 mtime 10:04:53Z）；调度方 `commands.log` 也没有 workspace 门禁命令条目。195 passed 与 19/19 exit 0 本身可核对 | `step-7.4/report.md:112`；`step-7.1/report.md:114`；`leader-verify-post-fix/workspace-gates/summary.json` |
| N-F3-3 | 低 | 复核期间工作区未冻结：`cargo fmt --all --check` 10:21:22Z 因并发 `aline-markdown` WIP 退出 1，10:32:04Z 同一命令退出 0；文档在 10:34–10:36Z 继续被组 6 写入。这是环境性限制，不是 `aline-edit` 缺陷 | `cargo-fmt-check.log`、`cargo-fmt-check-2.log`、`docs-snapshot-final.log` |
| N-F3-4 | 中 | 金样的索引状态在复核窗口内从 `A ` 变为 `??`：10:34:01Z `git ls-files --stage` 仍有 blob `2d3703f…`，10:40:18Z 索引为空且 porcelain 为 `??`。内容 sha256（`cf2504f2…`）与 mtime（09:08:45Z）未变，不是回填；但“索引暂存”证据当前不可复现；该变化非本复核所为，责任方无法由现有证据确定 | `static-checks.log` 第 10/52–55 行；`final-head-mtimes.log` |

## 4. 证据完整性核验

### 4.1 门禁与计数

- 我独立复跑两遍 `cargo test -p aline-edit --locked --offline`，均为 25 passed（lib 0、goldens 6、protection 4、refusals 15、doc-tests 0）；clippy 两遍退出 0；`cargo fmt -p aline-edit -- --check` 退出 0（输出 0 字节）；`cargo fmt --all --check` 第二遍退出 0。
- 第一遍 `cargo fmt --all --check` 退出 1 是并发组 6 的 `aline-markdown` 中间态，不是 `aline-edit` 的格式问题；两次结果都保留在日志中。
- 实现者 `commands.log` 12 条（09:08:50Z–09:23:06Z，全部 exit 0）；`commands-post-fix.log` 6 条（10:00:21Z–10:00:22Z，全部 exit 0，refusals 15/goldens 6/protection 4/全 crate 25）；调度方 `leader-verify-post-fix/commands.log` 6 条（10:04:14Z–10:04:23Z，全部 exit 0，同样 15/6/4/25）。这些计数与我的复跑一致。
- workspace 门禁：`summary.json` passed=true、19 项全部 exit_code 0，最早开始 10:04:27.889Z、最晚结束 10:04:53.722Z；`test.stdout.log` 40 个 `test result` 行合计 195 passed。我未独立重跑，只作核对（N-F3-2）。

### 4.2 金样

- sha256 `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743`；blob `2d3703f42f89f64b667321b4ced93ac216111e5b`；mtime 09:08:45Z（epoch 1793437725），早于实现 09:18/09:58。内容与暂存 blob 在 10:34:01Z 一致，说明修复后未回填。
- 索引状态在 10:40:18Z 变为 `??`，`git ls-files --stage` 为空（N-F3-4）；这不是内容变化。
- TSV 30 例：plan 26（Changed 7、Unchanged 2、Refused 17），validate 3、consume 1；拒绝类共 15 类（plan 12、validate 3）。与 step-7.1 §2 一致。

### 4.3 文档回填

- 10:38:13Z：`git diff --stat docs/` 为 4 files changed, 44 insertions(+)，删除行 0；组 7 追加 16 行（引擎 8、架构 4、PRD 2、验证计划 2），其他（组 6）28 行。10:34:01Z 时为 38 行、组 7 16、组 6 22。组 7 记录条数在两次快照中均为 4/2/1/1。
- 回填前哈希（`backfill-hashes-before.log`）与组 6 `step-6.8/backfill-hashes.log` 记录的 worktree 回填后哈希一致，说明组 7 回填起点干净；回填后哈希在 10:11:09Z 自洽，之后因组 6 写入而漂移（N-F3-1）。
- 最终 diff 0 删除行与 7.4 报告“误改规范句后还原”一致；未发现既有规范句被改写。
- 回填内容明确写“AC-04 未通过”“V-07 未通过”、勘误、纯空白/边界已接受限制与 M3 未覆盖命令；未发现把未实现写成已完成。

### 4.4 `tasks.md`

`tasks.md` 第 72–75 行的 7.1–7.4 仍全部为 `[ ]`；本轮未修改，也不判定是否应勾选。

### 4.5 未跟踪与工作区漂移（精确 porcelain）

10:34:01Z 的 `git status --porcelain`（组 7 与文档相关）：

```
 M docs/engine-design.md
 M docs/v1-architecture.md
 M docs/v1-prd.md
 M docs/v1-validation-plan.md
A  docs/validation/m0-engine-foundation/group-7/step-7.1/goldens/edit-goldens.tsv
?? docs/validation/m0-engine-foundation/group-7/commands-post-fix.log
?? docs/validation/m0-engine-foundation/group-7/commands.log
?? docs/validation/m0-engine-foundation/group-7/review-round-2-evidence/
?? docs/validation/m0-engine-foundation/group-7/review.md
?? docs/validation/m0-engine-foundation/group-7/run-targeted-tests.sh
?? docs/validation/m0-engine-foundation/group-7/step-7.1/goldens-pre-implementation.log
?? docs/validation/m0-engine-foundation/group-7/step-7.1/report.md
?? docs/validation/m0-engine-foundation/group-7/step-7.1/targeted-tests/
?? docs/validation/m0-engine-foundation/group-7/step-7.2/
?? docs/validation/m0-engine-foundation/group-7/step-7.3/
?? docs/validation/m0-engine-foundation/group-7/step-7.4/
```

10:40:18Z 同一路径的金样行变为：

```
?? docs/validation/m0-engine-foundation/group-7/step-7.1/goldens/edit-goldens.tsv
```

10:42:33Z（本文件写入后）的精确行：

```
?? docs/validation/m0-engine-foundation/group-7/review-round-2-evidence/
?? docs/validation/m0-engine-foundation/group-7/review-round-2.md
```

此时 HEAD 仍为 `99867bf2c5775aae4e8ed910713496d4ff84323c`，金样仍为 `??`。组 7 的证据目录（含本复核的 `review-round-2-evidence/`）与本文件全部未跟踪；证据只有提交后才有版本身份。复核窗口内 HEAD 未变，但文档内容、`aline-markdown` 与索引状态都发生过变化（N-F3-3、N-F3-4）。

## 5. 仍未冻结/未覆盖

- **工作区未冻结**：组 6 在 10:35–10:36Z 仍写 `engine-design.md`/`v1-validation-plan.md`；`aline-markdown` 的并发 WIP 使第一遍 `fmt --all` 失败；金样在窗口内从索引中消失。本报告只对 10:42:33Z 前观察到的状态负责，晚到写入未复核。
- **未覆盖**：真实 UI/DocCore 消费者、IME/组合真实事件、块打开/提交分组与 900ms 真实计时、磁盘/自动保存/恢复/导出、Windows/Linux 或其他 target。
- **未覆盖**：M3 全量命令（列表/表格/任务框/代码语言/全文格式化/查找替换事务）。
- **未覆盖**：MD-11 粘贴独立用例；HTML 右端越界与跨整个元素的格式命令（F3-2）。
- **未钉住**：纯空白隐藏片段与片段边界不比较的已接受限制及其块结构后果（N-F2-1）。
- **未独立重跑**：workspace 级门禁（19/19、195 passed 仅为核对他人 summary/日志）。
- **未验证**：M1 消费者是否由选区派生命令范围；M0 无 UI，只能依赖协议与测试。

## 6. 逐任务结论

| 任务 | 状态 | 依据 |
|---|---|---|
| 7.1 金样先行、纯 plan/validate、候选预演与 reread | **通过（M0 范围）** | 金样先于实现可观察且未回填；勘误、测试例外与实现 `8..8` 均按 round-1 处置落实；plan/validate 只读、探针 14/14 状态不变；F2-1 协议闭合。附 N-F2-1（V-07 已接受限制未钉测试）与 F3-4（金样证据强度限制已披露） |
| 7.2 成功/无变化/版本与前提改变、预算与重读拒绝、完整状态 | **通过（M0 范围）** | 金样拒绝矩阵覆盖任务列出的全部类别；真实缓冲 15 例、17 处状态断言；预算/重读/保护/HTML/空步均补测；独立复跑 15 passed。附 F3-6（字面命令未跑）与 N-F1-1（报告状态行过期） |
| 7.3 交错保护、MD-11、HTML 边界、Source 放行、预检不变、可消费协议 | **通过（M0 范围）** | 跨正文/脚注删除替换拒绝、Source 与 HTML 原文放行、拒绝不改选区/历史、旧计划重验、3 补丁一步历史均有证据。附 F3-2（粘贴无独立用例、HTML 只有左端）为已记录的覆盖缺口，不据此声称“粘贴已验证” |
| 7.4 回填、写入通道审查、R/S 与场景覆盖 | **部分成立** | 四份文档回填、R/S 表、场景覆盖与写入通道结论均存在且内容属实；但人工审查日志不可复现（F3-3）、指标归因错误（N-F1-3）、行号与 mtime 引用过期（N-F1-2/N-F1-4）、回填后哈希已漂移（N-F3-1）、门禁时间表述不准确（N-F3-2），证据包尚不能完全按原文复核 |

## 7. 明确不声称

- **不判定 V-07、AC-04 或 M0 出口通过**；回填中写的是“未通过”，本报告不改变这一点。
- **不判定 `tasks.md` 勾选**；7.1–7.4 仍为 `[ ]`，勾选与否不由本报告决定，勾选也不等于出口通过。
- **不声称隐藏片段已完整保留**；纯空白片段与片段边界不比较，且块结构合并后果未钉测试（N-F2-1）。
- **不声称所见替换已在运行时绑定选区**；只有协议与测试，M1 消费者约束未在运行时强制（F2-1 处置）。
- **不声称 workspace 门禁已独立重跑**；19/19 与 195 passed 仅核对调度方 `summary.json`/日志。
- **不声称 MD-11 粘贴已验证**；无独立用例（F3-2）。
- **不声称金样全部正确**；`allow_source_footnote` 仍写 `7..7`，正确值为 `8..8`，勘误按 round-1 要求保留。
- **不声称证据已冻结**；组 7 证据全部未跟踪，复核期间文档、`aline-markdown` 与索引仍在变化（F3-8、N-F3-3、N-F3-4）。
- **不评价组 6**；并发写入的组 6 文档与 `aline-markdown` 改动不在本报告范围。
- **不覆盖** Windows/Linux、其他 target、真实 UI/IME/DocCore、磁盘与自动保存。

复核者：独立子代理（非实现者）
