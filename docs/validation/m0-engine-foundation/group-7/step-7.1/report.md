# 7.1 记录：金样先行、纯 plan/validate 与候选复读

日期：2026-10-31。状态：`cargo test -p aline-edit --test goldens` 6 例通过；金样 30 例中 26 例为 plan 用例。本文记录金样的建立顺序、字节规则、`allow_source_footnote` 目标选区的**勘误**、实现范围与未覆盖项。本文不宣称 V-07、AC-04 或 M0 出口通过，也不把任务勾选当成验收。

## 1. 金样先于实现

| 证据 | 值 | 来源 |
|---|---|---|
| 金样 mtime（epoch） | `1793437725` → 2026-10-31T09:08:45Z（本地 `+0800` 显示为 17:08:45） | `step-7.1/goldens-pre-implementation.log` |
| 金样 sha256 | `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743` | 同上；复核时工作区与索引一致 |
| 索引暂存 blob | `2d3703f42f89f64b667321b4ced93ac216111e5b` | `git ls-files --stage` |
| 暂存 UTC | 2026-10-31T09:08:50Z | `group-7/commands.log` 第 1 条 |
| 暂存时 `crates/aline-edit` 内容 | 只有 `Cargo.toml` 与 `src/lib.rs` | `goldens-pre-implementation.log` |
| 实现源码 mtime（**修复前**，09:23:06Z 扫描时点） | 2026-10-31T09:18:49Z（`lib.rs`/`plan.rs`/`validate.rs`/`reread.rs`/`budget.rs`） | 独立复核 `review.md` 第 3.1 节记录 |
| 测试文件 mtime（**修复前**） | 09:19:00Z（`goldens.rs`）、09:19:01Z（`refusals.rs`）、09:20:51Z（`protection.rs`） | 同上 |
| `git log --all -- <golden>` | 复核时点为空（当时金样尚未进入提交）；证据提交 `55bd376` 后已跟踪，blob `2d3703f…`、内容 sha256 未变 | `review-round-3.md` R3-F3-2；当前 `git ls-files --stage` |

金样早于实现约 10 分钟，且暂存时实现目录只有骨架，因此“金样先写、实现后写”在 Git 索引、mtime 与日志三处可观察。

**证据强度限制（复核 F3-4，如实记录）**：mtime 与索引暂存都不是密码学证明；`goldens-pre-implementation.log` 把本地 `+0800` 的 17:08:45 标成了 `Z`（真实 epoch 为 09:08:45Z），且该日志只记录文件是否存在，不记录当时 `lib.rs` 的内容或哈希。测试文件是在实现之后写的。因此“金样先行”只在上述可观察证据上成立，不能外推为不可伪造。

**mtime 作用域更正（复核 N-F1-4）**：上表两行 mtime 是 09:23:06Z 扫描时点的**修复前**值。round-1 复核后的修复使源码与测试 mtime 前进（当前 `plan.rs` 09:58:27Z、`reread.rs` 09:59:45Z、`refusals.rs` 09:58:54Z；`goldens.rs` 仍为 09:19:00Z）；修复后时间见 §7。原表保留为修复前快照，不改写。

**索引状态更正（复核 N-F3-4、R3-F3-2）**：上表“索引暂存 blob”是 09:08:50Z 时点的事实。该暂存后来被移出索引（10:34:01Z `git ls-files --stage` 仍可查到 blob `2d3703f…`，10:40:18Z 为空、porcelain 为 `??`），当时“当前不可复现”的描述成立。证据提交 `55bd376` 之后，该文件已重新进入跟踪（blob 仍为 `2d3703f42f89f64b667321b4ced93ac216111e5b`，porcelain 干净）；金样内容 sha256 仍为 `cf2504f2…`、mtime 未变，因此不是回填，也不改变“金样先行”的结论。10:40:18Z 至 `55bd376` 之间的索引变化由调度者提交证据造成；此前变化的责任方无法由现有证据确定。

## 2. 金样内容与字节规则

`step-7.1/goldens/edit-goldens.tsv` 共 30 例：plan 26 例（7 例 `Changed`、2 例 `Unchanged`、17 例 `Refused`），另 4 例为 validate/consume 场景（3 例 validate 拒绝、1 例 consume）。拒绝类别共 15 类：plan 侧 `ExpectedFragmentMismatch`（2）、`OutOfBounds`、`ReversedRange`、`NotCharBoundary`、`BudgetExceeded`（3：`PatchCount`/`SourceBytes`/`Parse`）、`RereadFailed`（3：`HiddenSourceChanged` 1、`VisibleTextChanged` 2）、`HtmlBoundary`、`ProtectedRange`、`WrongMode`、`EmptySelection`、`PendingInput`、`CompositionActive`；validate 侧 `WrongVersion`/`ProjectionMismatch`/`SelectionMismatch`。

统一字节规则（金样按 UTF-8 字节手算，不使用字符数）：

- 补丁是 before 坐标的半开区间，替换文本按字节数计入 after 长度。
- 目标选区取“最后一个补丁终点之后”，即 `最后一个补丁的起点 + 替换文本字节数`。
- 格式 toggle 插入两枚 2 字节标记后，选区两端各后移 2 字节。

手算摘录：

| 用例 | 源（字节数） | 编辑 | 手算目标 | 金样目标 | 预览 |
|---|---|---|---|---|---|
| `source_replace_single` | `"alpha beta\n"`（11） | 6..10 `"beta"`→`"gamma"` | `6+5=11` | 11..11 | `alpha gamma` |
| `source_batch_three` | `"alpha beta gamma\n"`（17） | 0..5→`one`、6..10→`two`、11..16→`three` | `11+2=13` | 13..13 | `one two three` |
| `format_strong_toggle` | `"alpha beta\n"`（11） | 选区 6..10 两端插 `**` | `6+2=8`、`10+2=12` | 8..12 | `alpha beta` |
| `format_strike_toggle` | 同上 | 两端插 `~~` | 8..12 | 8..12 | `alpha beta` |
| `allow_source_html_raw` | `"a <b>x</b> c\n"`（13） | 5..6 `"x"`→`"y"` | `5+1=6` | 6..6 | `a y c` |
| `allow_visual_body_beside_footnote` | `"before [^1] after\n\n[^1]: note\n"`（30） | 0..6 `"before"`→`"BEFORE"` | `0+6=6` | 6..6 | `BEFORE [^1] afternote` |
| `allow_source_footnote` | `"text[^1] more\n\n[^1]: note\n"`（26） | 4..8 `"[^1]"`→`"[^2]"` | `4+4=8` | **7..7** | `text[^2] morenote` |

独立复核（`review.md` 第 3.2 节）用仓外 Python 脚本只读 TSV 与源字符串、不读实现输出，按同一字节规则独立重算 26 个 plan 用例：25 个一致，仅 `allow_source_footnote` 的目标选区不一致。

## 3. 勘误：`allow_source_footnote` 目标选区 7..7 → 8..8

- 源 `"text[^1] more\n\n[^1]: note\n"` 共 26 字节；编辑范围 4..8（`"[^1]"`，4 字节）被 4 字节的 `"[^2]"` 替换。
- 按第 2 节统一规则（与其余 25 个 plan 用例一致），目标应为 `4 + 4 = 8`，即 `8..8`。
- 金样写 `7..7`。7 落在 `"[^2]"` 的第 3 个字节之后（`[`、`^`、`2` 之后、`]` 之前），即光标停在脚注标记内部；把 4 字节的 `"[^2]"` 误计为 3 字节是最合理的解释。
- 实现与仓外探针都产生 `8..8`。该 8 不是从实现输出反向填充：它可由同一字节规则独立推出，并与独立重算、探针一致。
- `crates/aline-edit/tests/goldens.rs` 的 `expected_target` 先 `assert_eq!(golden, SelectionSpec::new(7, 7))`，再返回 `SelectionSpec::new(8, 8)`，并在注释中指向本文件。这样 `golden_plan_cases` 仍能通过，同时金样原值被保留为证据。
- 处置：**不修改实现为 7，不回填金样**。本文件即该差异的书面解释；复核 F1-3 要求的“记录勘误并让测试例外引用报告”已满足。
- 影响：该行金样不构成单一真值。只运行 `cargo test` 看到 6 passed 的读者不会发现 TSV 里写的是 7；这是证据缺陷，不是实现缺陷。

## 4. 实现范围（`aline-edit`）

- 公开面只有 `plan_change`/`validate_change` 与数据类型；无 `StringBuffer` 可变引用、无 `apply`/`reload`/`undo`/`redo`、无文件系统写入。`plan_change` 接收 `&TextSnapshot`/`&Parsed`/`&SrcSelection`/`&EditContext`/`&EditCommand`/`EditBudget`。
- `Replace`：Source 与 Visual 共用受检批次构造；逐条核对期望片段；内容相同的替换被剔除，全部剔除时返回 `Unchanged(ReplacementIdentical)`；Visual 下多补丁返回 `UnsupportedVisualBatch`。
- `ToggleInlineFormat`（仅 `Strong` `**`、`Strike` `~~`）：要求 Visual 模式、非空选区、格式范围等于选区；两端已被同一标记包围时返回 `Unchanged(AlreadyFormatted)`。
- 候选文本：在 before 文本上应用受检批次构造候选 `TextSnapshot` 并重新解析；诊断中 `BudgetExceeded` 转为 `EditRefusal::BudgetExceeded { kind: Parse }`。
- 复读契约：`Source` 只要求候选可解析；`Visible` 比较隐藏语义并核对候选绘制文本等于“before 绘制文本按替换拼接”的预期；`Format { marker }` 要求绘制文本、块种类一致，隐藏语义去掉成对标记后一致。
- 目标选区经候选快照的 `validate_selection` 复核；预览只读。
- `validate_change` 重验模式、待定输入、组合、revision、快照身份、投影身份、选区、批次、期望片段、Visual 权限，并在候选上重跑同一复读契约。

## 5. 定向证据

| UTC | 命令 | 退出 | 用例 | 日志 |
|---|---|---|---|---|
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test goldens` | 0 | 6 passed | `step-7.1/targeted-tests/goldens.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test goldens golden_validate` | 0 | 3 passed（另 3 filtered） | `step-7.2/targeted-tests/validate-reread.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test goldens golden_consume` | 0 | 1 passed（另 5 filtered） | `step-7.2/targeted-tests/consume.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test refusals` | 0 | 7 passed | `step-7.3/targeted-tests/refusals.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test protection` | 0 | 4 passed | `step-7.3/targeted-tests/protection.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline` | 0 | 17 passed（6+4+7，doc-tests 0） | `step-7.4/targeted-tests/full-crate.stdout.log` |

环境：`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`，`--locked --offline`；macOS arm64 / Darwin 27.2.0 / rustc 1.98.1。命令、起止 UTC 与退出码另见 `group-7/commands.log`。

## 6. 独立复核指出的语义缺口（F2-1/F2-2）

以下两条由独立复核（`review.md` 第 3.5 节、第 5 节 F2）提出，属于本任务范围内已知缺口，不是本文的通过结论：

- **F2-1（所见替换未绑定选区）**：`plan_replace` 的 Visual 分支只检查权限，不要求编辑范围等于或位于选区；`plan_toggle_format` 则要求范围等于选区。探针以选区 0..0、编辑 6..10 得到成功计划。处置方向：明确“命令给出的编辑范围是权威范围、`before_selection` 只用于 validate”的协议并补测试，而不是改运行时行为（改行为会改变既有金样 `refuse_hidden_link_target_lost` 的拒绝类别）。
- **F2-2（隐藏片段保留不完整）**：`reread.rs` 的 `semantic_hidden` 只保留非空白字符、丢弃纯空白隐藏片段且不比较片段边界。探针 `[x](https://e "a b")` 中把链接标题空格删掉仍被允许。处置方向：按原文比较非纯空白隐藏片段（保留内部空白），并把“纯空白片段与片段边界不比较”作为 V-07 已接受限制明确记录。

修复与补测证据见 `step-7.2/targeted-tests-post-fix/`、`commands-post-fix.log` 与本文第 7 节（修复后追加）。

## 7. 修复后（F2-1/F2-2/F3-1）

修复在 2026-10-31T09:58Z 前后落在工作区：`src/plan.rs` mtime 17:58:27（本地，即 09:58:27Z）、`src/reread.rs` 17:59:45（09:59:45Z）、`tests/refusals.rs` 17:58:54（09:58:54Z）。三处均为未跟踪新文件，工作区即当前内容。

### 7.1 改动内容（只读源码核对）

- **F2-1（协议而非行为）**：`plan_replace` 的 Visual 分支新增协议注释，明确“命令给出的编辑范围是权威范围，`before_selection` 只用于 validate 确认选区未被外部改动，不要求范围等于选区”；`ToggleInlineFormat` 仍是例外，额外要求范围等于选区。运行时行为未改，因此既有金样 `refuse_hidden_link_target_lost` 的拒绝类别不变。
- **F2-2（隐藏片段比较）**：`semantic_hidden` 改为保留非纯空白隐藏片段的**原文含内部空白**，只跳过纯空白片段；`strip_marker` 配套改为“去标记后仅剩空白的片段不参与比较，其余文本含内部空白原样保留”。注释把“纯空白片段不比较”“片段边界与位置不比较”明示为 V-07 已接受限制。
- **F3-1（真实缓冲拒绝状态）**：`tests/refusals.rs` 从 7 例扩到 15 例（round-2 复核 N-F2-1 后再补 1 例，共 16 例），新增 8 例：`hidden_fragment_internal_whitespace_change_is_refused`（F2-2）、`visual_replace_range_is_command_authority` 与 `visual_replace_validates_bound_selection_only`（F2-1）、`budget_exceeded_refusal_keeps_buffer_state`、`reread_failure_refusal_keeps_buffer_state`、`protected_range_refusal_keeps_buffer_state`、`html_boundary_refusal_keeps_buffer_state`、`already_formatted_creates_no_history_step`。

金样未改：`edit-goldens.tsv` 的 SHA-256 仍为 `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743`（修复后复核时实测），`allow_source_footnote` 的 `7..7` 原值与第 3 节勘误保持不变。

### 7.2 执行证据

实现者运行（`group-7/commands-post-fix.log`，2026-10-31T10:00:21Z–10:00:22Z，全部 exit 0，原始日志在 `step-7.2/targeted-tests-post-fix/`）：refusals 15、goldens 6、protection 4、全 crate 25、`clippy -p aline-edit --all-targets -- -D warnings` 0、`cargo fmt --all --check` 0。

我作为调度方在实现者报告后独立复跑（不经实现者日志，原始日志在 `step-7.4/leader-verify-post-fix/`）：

| UTC | 命令 | 退出 | 结果 |
|---|---|---|---|
| 2026-10-31T10:04:14Z | `cargo test -p aline-edit --locked --offline --test refusals` | 0 | 15 passed |
| 2026-10-31T10:04:15Z | `cargo test -p aline-edit --locked --offline --test goldens` | 0 | 6 passed |
| 2026-10-31T10:04:15Z | `cargo test -p aline-edit --locked --offline --test protection` | 0 | 4 passed |
| 2026-10-31T10:04:22Z | `cargo test -p aline-edit --locked --offline` | 0 | 25 passed（0+6+4+15，doc-tests 0） |
| 2026-10-31T10:04:22Z | `cargo clippy -p aline-edit --all-targets --locked --offline -- -D warnings` | 0 | 无 warning |
| 2026-10-31T10:04:23Z | `cargo fmt --all --check` | 0 | 无输出 |
| 2026-10-31T10:04:27Z–10:04:53.722Z | `python3 scripts/check_workspace.py --cargo-home … --output-dir …/leader-verify-post-fix/workspace-gates` | 0 | 19/19 exit 0；`cargo test --workspace` 40 个 `test result` 行合计 **195 passed**（结束时间按 `summary.json` 最晚 `finished_utc` 10:04:53.722236Z，复核 R3-F3-1） |

两次运行（实现者 10:00:21Z、调度方 10:04:14Z）数字一致。**注意**：组 7 原 `step-7.4/workspace-gates`（09:21:09Z）合计 151 passed，那是组 6 测试尚未最终扩充时的快照；10:04 的 195 passed 才是当前工作区全量。两组都保留，不互相覆盖。

### 7.3 仍未闭合

- 修复只覆盖 F2-1 的**协议与测试**，不改变运行时行为；M1 消费者仍须由选区派生命令范围，本组无法在无 UI 的情况下强制。
- F2-2 仍有两个已接受限制（纯空白片段、片段边界不比较），见 `step-7.2/report.md` 与 V-07 未覆盖项。
- F3-3/F3-5/F3-7/F3-8（手工日志命令不完整、`printenv` 多参数、无真实 UI 消费者、证据未跟踪）在本节未闭合，处置见 `step-7.4/report.md`。
- 独立复核者是否接受本修复，须由复核者另行撰写；本文不代写复核结论。

## 8. 未覆盖与不宣称

- 未覆盖：真实 UI/DocCore 消费者、IME/组合真实事件、Windows/Linux 或其他 target、磁盘与自动保存。
- 未实现（M0 范围外）：列表/表格/格式化等 M3 全量命令；本组只有 `Replace` 与 `ToggleInlineFormat`（Strong/Strike）。
- 未覆盖：MD-11 粘贴的独立用例（与跨区删除/替换共用 Replace 路径）、HTML 跨界格式的独立用例；见 `step-7.3`。
- 不宣称 V-07、AC-04 或 M0 出口通过；`tasks.md` 勾选不等于出口通过。独立复核结论由复核者撰写，本文不代写。
