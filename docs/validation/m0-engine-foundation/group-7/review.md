# 组 7（7.1–7.4）独立复核报告

- 复核日期：2026-10-31
- 复核对象：OpenSpec change `m0-engine-foundation` 组 7（任务 7.1–7.4），包含 `aline-edit` 的 `budget/command/errors/permissions/plan/refusal/reread/validate` 源码、`crates/aline-edit/tests/{goldens,refusals,protection}.rs`、`docs/validation/m0-engine-foundation/group-7/step-7.1/goldens/edit-goldens.tsv` 与 `group-7/` 证据
- 仓库与基线：`<repo>`，分支 `main`，HEAD `99867bf2c5775aae4e8ed910713496d4ff84323c`
- 复核立场：实现者不是复核者。实现者的报告、日志与探针只作为待验材料；本报告的结论来自本次独立重跑、独立重算、外部探针与读码，不把实现者自述当作已证明事实
- 写入范围：仅本文件。未修改 Rust 源码、测试、金样、Cargo、lint/guard、`tasks.md`、其他 `docs/*.md` 或 OpenSpec 文件；未执行 `git add`、`git commit` 或 `git push`
- 快照说明：工作区状态抓取于 2026-10-31T09:37:06Z（HEAD 未变）；独立重跑窗口为 2026-10-31T09:30:51Z–09:40:25Z。复核期间仍有组 6 并发写入：快照时 `docs/engine-design.md` +10 行、`docs/v1-validation-plan.md` +4 行；快照后（UTC 09:38–09:44）组 6 的 6.8 回填又给 `docs/v1-architecture.md`/`docs/v1-prd.md` 各加 2 行、给前两者各加 2 行，逐字核对均为组 6 记录；`crates/aline-markdown` 仍在改动。09:30:53Z 的 `cargo test -p aline-edit` 曾因 `aline-markdown` 中间态编译失败，09:31:13Z 后恢复；本报告不以该瞬时失败判 `aline-edit` 失败。晚于 09:40:25Z 的追加不在覆盖范围内

## 1. 工作区状态（`git status --porcelain` 逐字）

抓取时间：2026-10-31T09:37:06Z（分支 `main`，HEAD `99867bf2c5775aae4e8ed910713496d4ff84323c`）。

```text
 M crates/aline-edit/src/lib.rs
 M crates/aline-markdown/src/lib.rs
 M crates/aline-markdown/src/mapping/mod.rs
 M crates/aline-markdown/src/mapping/projection.rs
 M docs/engine-design.md
 M docs/v1-validation-plan.md
A  docs/validation/m0-engine-foundation/group-7/step-7.1/goldens/edit-goldens.tsv
?? crates/aline-edit/src/budget.rs
?? crates/aline-edit/src/command.rs
?? crates/aline-edit/src/errors.rs
?? crates/aline-edit/src/permissions.rs
?? crates/aline-edit/src/plan.rs
?? crates/aline-edit/src/refusal.rs
?? crates/aline-edit/src/reread.rs
?? crates/aline-edit/src/validate.rs
?? crates/aline-edit/tests/
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
?? crates/aline-markdown/tests/diagnostics.rs
?? crates/aline-markdown/tests/hostile.rs
?? crates/aline-markdown/tests/html.rs
?? crates/aline-markdown/tests/mapping.rs
?? crates/aline-markdown/tests/protection.rs
?? crates/aline-markdown/tests/reuse.rs
?? crates/aline-markdown/tests/rewrite.rs
?? crates/aline-markdown/tests/spec.rs
?? docs/validation/m0-engine-foundation/group-6/
?? docs/validation/m0-engine-foundation/group-7/commands.log
?? docs/validation/m0-engine-foundation/group-7/run-targeted-tests.sh
?? docs/validation/m0-engine-foundation/group-7/step-7.1/goldens-pre-implementation.log
?? docs/validation/m0-engine-foundation/group-7/step-7.1/targeted-tests/
?? docs/validation/m0-engine-foundation/group-7/step-7.2/
?? docs/validation/m0-engine-foundation/group-7/step-7.3/
?? docs/validation/m0-engine-foundation/group-7/step-7.4/
```

同次抓取的 `git diff --stat`：

```text
 crates/aline-edit/src/lib.rs | 20 ++++++++++++++++++++
 docs/engine-design.md        | 10 ++++++++++
 docs/v1-validation-plan.md   |  4 ++++
 3 files changed, 34 insertions(+)
```

要点：

- `crates/aline-edit` 只有 `src/lib.rs` 被修改（文档与公开导出，20 行）；其余实现、测试、金样均未跟踪。`crates/aline-edit/Cargo.toml`、workspace lint/guard、`rust-toolchain`、`scripts/` 未改。
- 金样已 staged（`A`），索引 blob 为 `2d3703f42f89f64b667321b4ced93ac216111e5b`；工作区文件 sha256 为 `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743`，与 `git show :<path>` 一致。
- 快照时 `docs/engine-design.md` 的 10 行改动逐字核对后是组 6 的 6.1/6.2、6.4/6.5、6.5 HTML、6.6、6.7 记录；`docs/v1-validation-plan.md` 的 4 行是组 6 的预算/敌意输入与映射/保护夹具记录；`docs/v1-architecture.md`、`docs/v1-prd.md` 无 diff。快照之后（UTC 09:38–09:44）组 6 的 6.8 回填新增 8 行（architecture/prd 各 2 行，engine-design/validation-plan 各 2 行），逐字核对仍是组 6 记录，没有组 7 的 §8/§9/§14、架构 §7.1/§7.2、V-07/AC-04 回填；晚到内容未复核。
- `openspec/changes/m0-engine-foundation/tasks.md` 未被修改，7.1–7.4 在复核时仍全部为 `[ ]`。
- `docs/validation/m0-engine-foundation/group-7/` 下没有任何 `report.md`/`README`；7.1–7.4 的四个 step 报告缺失。`crates/aline-edit/tests/goldens.rs:291` 引用的 `step-7.1/report.md` 也不存在。
- 状态中的 `aline-markdown`、`group-6` 条目是并发写入，与本组无关；本报告只在“并发写入使 workspace 级门禁不可独立重跑”的意义上提及，不评价组 6。

## 2. 执行环境与命令

### 2.1 环境

- 主机：`Darwin XianMac.local 27.2.0`（Darwin Kernel 27.2.0，`arm64`，Apple T8142）
- 工具链：`rustc 1.98.1 (48a229cea 2026-09-01)`，`cargo 1.98.1 (797e8a9bc 2026-08-05)`
- 环境变量：`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`
- 所有 cargo 命令均使用 `--locked --offline`；本次复核未联网，未新增依赖

### 2.2 本次实际执行的命令与结果

| 命令 | UTC | 退出码 | 结果摘要 |
| --- | --- | --- | --- |
| `cargo test -p aline-edit --locked --offline` | 2026-10-31T09:30:51Z–09:30:53Z | 101 | 并发 `aline-markdown` 中间态编译失败（E0061/E0277/E0425），不是 `aline-edit` 缺陷 |
| `cargo test -p aline-edit --locked --offline` | 2026-10-31T09:31:18Z–09:31:22Z | 0 | goldens 6 + protection 4 + refusals 7，0 failed |
| `cargo test -p aline-edit --locked --offline` | 2026-10-31T09:37:34Z–09:37:37Z | 0 | 同上，另 doc-tests 0 |
| `cargo test -p aline-edit --locked --offline --test goldens` | 2026-10-31T09:37:42Z | 0 | 6 passed |
| `cargo test -p aline-edit --locked --offline --test refusals` | 2026-10-31T09:37:37Z | 0 | 7 passed |
| `cargo test -p aline-buffer --locked --offline` | 2026-10-31T09:37:42Z | 0 | 7 个 test target 合计 29 passed，0 failed |
| `cargo clippy -p aline-edit --all-targets --no-deps --locked --offline -- -D warnings` | 2026-10-31T09:37:37Z | 0 | 无 warning |
| `cargo clippy -p aline-edit --all-targets --locked --offline -- -D warnings` | 2026-10-31T09:40:25Z | 0 | 精确命令重跑，无 warning |
| `cargo tree -p aline-edit --locked --offline` | 2026-10-31T09:37:42Z | 0 | 仅 `aline-buffer`、`aline-markdown`、`pulldown-cmark 0.13.6`（传递 `bitflags`、`memchr`、`unicase`） |
| `python3 -I <tmp>/aline-g7-recalc/recalc.py <goldens.tsv>` | 2026-10-31T09:38:41Z | 1 | 26 个 plan 用例中 25 个一致；仅 `allow_source_footnote` 目标 7 vs 8 |
| 仓外探针 `<tmp>/aline-g7-probe` 的 `cargo run --quiet` | 2026-10-31T09:37:01Z、09:39:59Z | 0 | 17 项检查，见第 3 节 |
| `rg` 禁用的 `unwrap/expect/panic/todo/unimplemented/unsafe` | 2026-10-31T09:37:42Z | 0 | `crates/aline-edit` 0 命中 |
| `rg` 写入通道与 `&mut` 扫描 | 2026-10-31T09:39Z | 0 | `aline-edit/src` 无 `StringBuffer`/`apply`/`undo`/`redo`/`fs`/`File` 命中；`&mut` 仅 `fmt` |
| `git status/stat/show/log`、`stat`、`shasum` | 2026-10-31T09:37:06Z–09:39Z | 0 | 用于金样先行、mtime、状态与日志一致性核对 |

说明：

- 本次复核**未在并发写入窗口内独立重跑 workspace 级命令**（`cargo fmt --all --check`、`cargo clippy --workspace`、`cargo test --workspace`、`scripts/check_workspace.py`）。只核对了实现者的 `step-7.4/workspace-gates/summary.json` 与日志：`passed=true`，19 项全部 `exit_code=0`，时间 09:21:09.760743Z–09:21:48.833532Z；workspace 测试日志 39 个 `test result` 行、合计 151 passed、0 failed。该记录只作为快照，不据此判通过或失败。
- 09:30:53Z 的 101 是并发 `aline-markdown` 中间态造成的；同一窗口内 `cargo clippy -p aline-edit --all-targets --no-deps` 在 09:31:12Z–09:31:13Z 退出 0。09:40:25Z 精确 clippy 命令也退出 0。
- 未执行：Windows/Linux 或其他 target；真实 UI/IME/磁盘/自动保存；V-07/AC-04 实验矩阵；M0 出口验收。

## 3. 独立重算与核验

### 3.1 金样先行

- 金样 mtime 的 epoch 为 `1793437725`，换算 UTC 为 2026-10-31T09:08:45Z；staged 时间为 09:08:50Z。实现源码 mtime 为 09:18:49Z（`lib.rs`/`plan.rs`/`validate.rs`/`reread.rs`），测试为 09:19:00Z（`goldens.rs`）、09:19:01Z（`refusals.rs`）、09:20:51Z（`protection.rs`）。金样早于实现约 10 分钟。
- `step-7.1/goldens-pre-implementation.log` 记录：09:08:50Z 时 `crates/aline-edit` 下只有 `Cargo.toml` 与 `src/lib.rs`；`git add` 只加金样，exit 0；staged blob `2d3703f...`；sha256 `cf2504...`。`git log --all -- <golden>` 为空，没有任何提交包含该金样。
- 当前工作区金样与 staged blob 的 sha256 完全一致（见第 1 节），说明金样在复核前没有被回填改写。
- 局限（F3-4）：mtime 与索引暂存时间都不是密码学证明；`goldens-pre-implementation.log` 的 `mtime=2026-10-31T17:08:45Z` 是把本地 +0800 时间标成了 `Z`（epoch 实际为 09:08:45Z）；该日志只列文件是否存在，没有记录当时 `lib.rs` 的内容/哈希；测试文件是在实现之后写的。结论“金样先行”在可观察的 Git 索引、mtime 与日志上成立，但不能外推为不可伪造。

### 3.2 金样独立重算（≥8 例）

我用仓外 Python 脚本 `<tmp>/aline-g7-recalc/recalc.py` 只读取 TSV 与源字符串，不读取实现输出，按 UTF-8 字节独立计算补丁、目标选区和预览，再与金样逐项比对。结果：26 个 plan 用例中 25 个一致，1 个不一致（`allow_source_footnote` 的目标选区）。

手算/重算摘录：

| 用例 | 源字节 | 编辑/补丁 | 手算目标 | 金样 | 预览/结论 |
| --- | --- | --- | --- | --- | --- |
| `source_replace_single` | 11 | 6..10 `"beta"`→`"gamma"` | 11..11 | 11..11 | `alpha gamma`，一致 |
| `source_batch_three` | 17 | 0..5→`one`、6..10→`two`、11..16→`three` | 13..13 | 13..13 | `one two three`，一致 |
| `format_strong_toggle` | 11 | 选区 6..10，插入 `**`×2 | 8..12 | 8..12 | `alpha beta`，一致 |
| `format_strike_toggle` | 11 | 选区 6..10，插入 `~~`×2 | 8..12 | 8..12 | `alpha beta`，一致 |
| `allow_source_footnote` | 26 | 4..8 `"[^1]"`→`"[^2]"`（4 字节） | **8..8** | **7..7** | `text[^2] morenote`，**不一致** |
| `allow_source_html_raw` | 13 | 5..6 `"x"`→`"y"` | 6..6 | 6..6 | `a y c`，一致 |
| `allow_visual_body_beside_footnote` | 30 | 0..6 `"before"`→`"BEFORE"` | 6..6 | 6..6 | `BEFORE [^1] afternote`，一致 |
| `refuse_hidden_link_target_lost` | 15 | Visual 0..14→`"[x]"` | — | — | 可见文字仍为 `x`，隐藏目标 `https://e` 丢失；`RereadFailed/HiddenSourceChanged`，一致 |
| `refuse_wrong_version` | 11 | 计划后 0..0 插入 `"X"`，再 validate | — | — | `WrongVersion`，一致 |
| `refuse_wrong_projection` | 11 | 同一快照重新 parse 后 validate | — | — | `ProjectionMismatch`，一致 |
| `consume_batch_single_history_step` | 17 | 3 个补丁作为一次 `Command` 操作 | — | — | 文本 `one two three\n`，1 个历史节点，undo/redo 正确，一致 |

`allow_source_footnote` 的判定：

- 源 `"text[^1] more\n\n[^1]: note\n"` 共 26 字节；编辑范围 4..8，替换文本 `"[^2]"` 为 4 字节。按其余 25 个 Changed 用例一致使用的“最后一个补丁终点之后”规则，目标应为 `4 + 4 = 8`，即 `8..8`。
- 金样写 `7..7`。7 会落在 `"[^2]"` 的第 3 个字节之后（`[`、`^`、`2` 之后，`]` 之前），即光标在脚注标记内部；这与 `source_replace_single` 的 11、`source_batch_three` 的 13、`format_*` 的 8..12、HTML 的 6 等所有用例使用的规则都不一致。把 `"[^2]"` 误计为 3 字节是最合理的解释。
- 实现与仓外探针都产生 `8..8`；测试 `crates/aline-edit/tests/goldens.rs:281-296` 先 `assert_eq!(golden, SelectionSpec::new(7, 7))`，再返回 `SelectionSpec::new(8, 8)`，所以 `golden_plan_cases` 仍能通过。这个 8 不是从实现输出反向填充：它可以从同一套字节规则独立推出，且与探针、重算一致；但金样本身没有被测试当作唯一真值使用。
- 影响：该行的“金样先行”证据不成立为单一真值；只跑 `cargo test` 看到 6 passed 的读者不会发现 TSV 里写的是 7。测试的例外注释指向 `step-7.1/report.md`，而该报告不存在，所以差异的唯一书面解释也不在证据包内。这是证据缺陷（F1-3），不是实现缺陷；修复方向是补写 step-7.1 报告并记录勘误、让测试例外引用该报告，而不是把实现改成 7，也不应回填金样。

### 3.3 纯规划/验证不写真实 Buffer

- `crates/aline-edit/src/lib.rs` 的公开面只有 `plan_change`、`validate_change` 与数据类型；没有 `StringBuffer`、`apply`、`undo`、`redo` 或文件写入。`plan_change` 接收 `&TextSnapshot`/`&Parsed`/`&SrcSelection`/`&EditContext`，返回 `ChangePlan`；`validate_change` 返回借用 `plan`/`batch` 的 `ValidatedChange<'a>`，不持有可写缓冲。
- `EditContext` 只有 `pending_input` 与 `composition_active` 两个布尔值，按值传递；本组 API 没有 `draft` 字段（草稿属于 UI/DocCore 消费方）。
- 仓外探针对真实 `StringBuffer` 抓取 `State { text, anchor, head, revision, undo, redo, nodes }`：`plan_change` 与 `validate_change` 前后完全相等，`target=11`；`refusals.rs` 的 `successful_plan_and_validation_leave_buffer_untouched` 也通过。
- 读码未发现隐藏写入：`aline-edit/src` 中唯一的 `CheckedBatch::new` 出现在 `plan.rs:235` 与 `plan.rs:323`，只构造不可变批次；没有 `&mut StringBuffer`、`&mut self` 或文件系统调用。

### 3.4 拒绝完整状态不变

- 本次重跑 `--test refusals`：7 passed（pending 组保持、redo 分支、composition、WrongVersion、identity 无历史步、Visual 多补丁拒绝、成功计划不写 buffer）。
- 仓外探针在真实 `StringBuffer` 上抽查了 5 条路径，均以 `State` 前后相等、redo 仍可用结束：
  - `ProtectedRange`（脚注引用，`FootnoteReference`）：拒绝后文本/选区/版本/undo/redo/节点数不变，redo 可用；
  - `HtmlBoundary`（HTML 标签内格式 toggle）：同样不变，redo 可用；
  - `PendingInput`：拒绝后 pending 组仍开放（继续输入后仍是 1 个历史节点），状态不变；
  - `PendingInput` + 已有 redo 分支：拒绝后 redo_depth 仍为 1，redo 可用；
  - `CompositionActive` + 已有 redo 分支：同样不变，redo 可用；
  - `WrongVersion`：旧计划 validate 拒绝后状态不变。
- 没有观察到空历史步或 redo 被清除；identity 替换返回 `Unchanged(ReplacementIdentical)`，不产生历史步。
- 局限（F3-1）：仓库的 `refusals.rs` 没有用真实 `StringBuffer` 覆盖 `BudgetExceeded`/`RereadFailed`/`ProtectedRange`/`HtmlBoundary`；`protection.rs` 与 `goldens.rs` 是 plan 级。探针补了 ProtectedRange/HtmlBoundary 与 pending/composition+redo，但预算/重读拒绝的真实 buffer 状态仍只有 plan 级证据。`AlreadyFormatted` 也没有真实 buffer 的空步测试。

### 3.5 Reread 语义

- `RereadContract::Source`：只要求候选能成功 parse；Source 模式允许改隐藏链接目标、HTML 原文、front matter、TOC、脚注引用。探针 `source_hidden_edit_allowed: target=13` 与 `protection.rs` 的 Source 放行用例一致。
- `RereadContract::Visible`：先比较 before/after 的隐藏片段语义，再比较候选 drawn text 是否等于“before drawn text 用替换区间和字面替换量拼接”的结果。`refuse_hidden_link_target_lost` 的可见文字同为 `x`，但隐藏目标 `https://e` 丢失，`semantic_hidden` 前后不等，返回 `HiddenSourceChanged`；探针确认。
- `RereadContract::Format { marker }`：要求 drawn text 相等、块种类相等、隐藏语义去掉成对 marker 后相等。探针的 `format_partial_refused`（对已有 `**alpha**` 的子区间 2..5 再 toggle）返回 `VisibleTextChanged`，是保守拒绝，不是缺陷。
- 缺口 F2-2：`reread.rs:138-166` 的 `semantic_hidden` 只保留隐藏片段中的非空白字符，空白隐藏片段直接丢弃，片段边界也不比较。探针 `[x](https://e "a b")\n`、选区 1..2、Visual 替换 16..17 的空格为 `""` 被允许，target 16..16、preview `x`；链接标题从 `"a b"` 静默变成 `"ab"`。这与 7.1“保留隐藏源片段”和 spec“预演验证保留源片段”的强表述不一致。非空白丢失（隐藏链接目标）会被拒绝，但空白/片段边界变化不会。
- 缺口 F2-1：`plan.rs:184-223` 的 Visual Replace 没有要求编辑范围等于或位于选区；`plan.rs:300` 的 `ToggleInlineFormat` 则有 `selected_range == selection.range()` 检查。探针 `selection=0..0`、Visual Replace 6..10 `"beta"`→`"BETA"` 被允许，target 10..10、preview `alpha BETA`；计划绑定的 `before_selection` 是 0..0，补丁却在 6..10。validate 只检查当前选区等于计划的 `before_selection`，不会发现这个不一致。消费者若把光标当选区、把命令当编辑范围，就会绕过“所见替换”的绑定；这也是 F2-2 的隐藏空白用例能被构造出来的原因之一。

### 3.6 依赖与合规

- `cargo tree -p aline-edit --locked --offline`：直接依赖仅 `aline-buffer` 与 `aline-markdown`；`aline-markdown` 只依赖 `aline-buffer` 与 `pulldown-cmark 0.13.6`（传递 `bitflags 2.13.2`、`memchr 2.8.3`、`unicase 2.10.0`）。`crates/aline-edit/Cargo.toml` 只有两个 workspace 依赖；`Cargo.lock` 未被修改（`git status` 未列出）。
- `crates/aline-edit`（含 tests）中 `rg` 未发现 `.unwrap()`、`.expect(`、`panic!`、`todo!`、`unimplemented!`、`unsafe`；workspace lint 的 `unsafe_code = "forbid"`、`unwrap_used/expect_used/panic = "deny"` 未被改动。
- 精确命令 `cargo clippy -p aline-edit --all-targets -- -D warnings` 在 09:40:25Z 退出 0；09:30:53Z 的失败来自并发 `aline-markdown` 中间态，不是本 crate。
- `git diff --stat` 显示 `crates/aline-edit` 只有 `src/lib.rs` 的 20 行文档/导出；`Cargo.toml`、`scripts/`、lint/guard 配置均无 diff。

### 3.7 证据完整性

- `group-7/commands.log` 共 12 条命令记录（另有 3 行表头注释），时间 2026-10-31T09:08:50Z–09:23:06Z，全部 `exit=0`；包括金样 staged、环境、git 状态、`-- --list`、goldens/validate-reread/consume/refusals/protection/full-crate、workspace gates、人工写入通道审查。命令、日志、时间与 `summary.json` 一致，未发现伪造迹象。
- `step-7.4/workspace-gates/summary.json`：`passed=true`，19 项结果全部 `exit_code=0`，时间 09:21:09.760743Z–09:21:48.833532Z；workspace 测试日志 39 个 `test result` 行、151 passed、0 failed。该记录**未在并发写入窗口内独立重跑**，只作为快照核对。
- 定向日志计数与本次重跑一致：goldens 6、validate-reread 3、consume 1、refusals 7、protection 4、full-crate 17（6+4+7，另有 0 doc-tests）。
- `step-7.4/environment.stdout.log` 只打印了 `CARGO_HOME`；这是 macOS `printenv A B C` 只打印第一个存在变量的行为，不代表另外两个变量未设置。`summary.json` 的 `env_overrides` 明确记录 `CARGO_HOME`、`CARGO_NET_OFFLINE`、`RUSTUP_TOOLCHAIN`、`PYTHONDONTWRITEBYTECODE`（F3-5）。
- `step-7.4/manual-write-channel-review.log` 的结论（aline-edit 公开面无可变写入 API、src 中只有两处 `CheckedBatch::new`、workspace 生产代码无实际写入入口、文件写入只在 `aline-source/src/atomic.rs`）方向正确；但日志没有记录具体的 `rg`/`sed` 命令，复现性有限（F3-3）。我在 `aline-edit/src` 独立扫描确认无 `StringBuffer`/`apply`/`undo`/`redo`/`fs`/`File`；workspace 级结论未独立重扫全仓。
- 组 7 证据全部未跟踪、未提交；复核期间组 6 仍在写 `docs/engine-design.md`/`docs/v1-validation-plan.md` 与 `aline-markdown`。本报告固定 09:37:06Z 状态与 09:30:51Z–09:40:25Z 重跑窗口，晚到的追加不在覆盖范围（F3-8）。

### 3.8 覆盖对照（7.1–7.4 与 spec Scenario）

spec 实际有 2 个 Requirement、6 个 Scenario（两个 Requirement 各 3 个）；以下逐项核对 6 个，而不是只核对 3 个。

| 要求/场景 | 已覆盖证据 | 未覆盖/缺口 |
| --- | --- | --- |
| 7.1 普通源码替换 | `source_replace_single` 金样、探针 target 11 | — |
| 7.1 安全行内格式 toggle | `format_strong_toggle`、`format_strike_toggle`、`format_strong_already`、探针 8..12 | 已有粗体子区间的 partial toggle 被保守拒绝（非缺陷） |
| 7.1 一批多补丁 | `source_batch_three`、consume 3→1 历史节点 | — |
| 7.1 纯 plan/validate、候选预演、应用前 reread | 公开面只读、探针状态不变、`golden_validate_*` 3 例、三种 reread 契约 | — |
| 7.1 保留隐藏源片段 | 隐藏链接目标丢失拒绝、Source 隐藏编辑放行 | 隐藏空白/片段边界缺口（F2-2）；Visual 范围未绑定选区（F2-1） |
| 7.1 金样先行 | mtime、staged blob、pre-implementation log | 证据强度局限（F3-4）；footnote 目标错误（F1-3） |
| 7.2 成功/无变化/错版本/错片段/错投影/越界/预算/重读失败 | 26 个 plan 金样、refusals 7 例 | 预算/重读拒绝的真实 buffer 状态（F3-1） |
| 7.2 最小反例“可见文字相同、隐藏链接目标丢失” | 金样 + 探针 `HiddenSourceChanged` | 仅非空白隐藏目标；空白隐藏变化未拒绝（F2-2） |
| 7.2 完整状态不变、无空步、no redo 清除 | refusals 7 例；探针 ProtectedRange/HtmlBoundary/PendingInput/pending+redo/composition+redo/WrongVersion | `AlreadyFormatted` 真实 buffer 空步（F3-1）；`BudgetExceeded`/`RereadFailed` 真实 buffer（F3-1） |
| 7.3 正文/脚注交错、跨区删除/替换/粘贴 | `protection.rs` 跨区脚注拒绝、Source 放行；金样脚注 | 粘贴只走同一 Replace 路径，未单独用例（F3-2） |
| 7.3 HTML 触及/跨界格式拒绝 | `refuse_format_html`、探针 HtmlBoundary | 跨界格式未单独用例（F3-2） |
| 7.3 Source 修改、HTML 原文修改允许 | `protection.rs` front matter/TOC/脚注 Source 放行、`allow_source_html_raw`、探针 | — |
| 7.3 pending 组/组合/选区/undo/redo 不被预检改变 | refusals、探针 pending/composition+redo | 仓库测试没有 pending+redo 的单一用例（探针补了） |
| 7.3 旧计划查找、多补丁一步历史 | validate 的 revision/snapshot/projection/selection/expectations 检查；consume 3→1 | — |
| 7.3 不提前实现 M3 命令 | 只有 `Replace` 与 `ToggleInlineFormat`（Strong/Strike） | 列表/表格/格式化命令未覆盖；7.4 要求记录为未覆盖（F1-1/F3-2） |
| 7.4 回填引擎 §8/9/14、架构 §7.1/7.2、V-07/AC-04 | — | 全部缺失（F1-2） |
| 7.4 人工审查 DocCore 以外无实际写入通道 | manual log + 我的 aline-edit 扫描 | 日志无精确命令（F3-3）；workspace 级未独立重扫 |
| 7.4 记录 R/S 与场景覆盖 | — | 四个 step 报告缺失（F1-1） |
| Spec 代表性成功计划 | 金样普通替换/格式/批次、consume 一步历史、探针纯规划 | — |
| Spec 版本或前提改变 | `validate_after_change`（WrongVersion）、`validate_reparsed`（ProjectionMismatch）、`validate_changed_selection`（SelectionMismatch）、`refuse_expected_fragment`、refusals stale plan | — |
| Spec 预演失败或预算耗尽 | 3 个预算金样、隐藏目标丢失、可见文字变化、跨块格式；plan 返回 Err 不写 buffer | 真实 buffer 的预算/重读状态（F3-1） |
| Spec 保护与 HTML 拒绝 | `refuse_visual_footnote`、`refuse_format_html`、protection 跨区、探针状态不变 | — |
| Spec 允许安全源码编辑 | `allow_source_footnote`、`allow_source_html_raw`、protection Source 放行、探针 | — |
| Spec 待定输入时拒绝 | `refuse_pending_input`、`refuse_composition`、refusals、探针 pending+redo/composition+redo | — |

## 4. 逐项结论（7.1–7.4）

总体：7.2/7.3 的代码与定向测试按 M0 范围通过；7.1 主体实现通过，但金样证据与报告缺失阻塞勾选；7.4 阻塞。F1 共 3 个，F2 共 2 个，F3 共 8 条。以下“通过”只描述 M0 任务范围内的实现行为，不代表 V-07/AC-04 或 M0 出口通过，也不代表 `tasks.md` 可以勾选。

### 7.1 纯 plan/validate、候选预演与应用前 reread —— 实现主体通过；证据阻塞（F1-1/F1-3）；语义缺口 F2-1/F2-2

- 已验证：普通源码替换、Strong/Strike toggle、3 补丁批次、无变化（identity/AlreadyFormatted）、候选重建与 reread、目标选区规则；`cargo test -p aline-edit` 6+4+7 passed；探针确认纯规划与 validate 不改真实 buffer。
- 金样先行在 mtime/staged blob/日志上成立，但 `allow_source_footnote` 的 7..7 是手工计数错误，正确为 8..8；测试用例外掩盖了该错误，且注释指向不存在的 `step-7.1/report.md`（F1-3）。
- 隐藏片段保留不完整：非空白隐藏目标丢失会被拒绝，但隐藏空白/片段边界变化不会（F2-2）；Visual Replace 未绑定选区（F2-1）。
- 因此 7.1 不能按现有证据勾选；实现本身不需要为了 footnote 改成 7。

### 7.2 成功/无变化/错版本/错片段/错投影/越界/预算/重读失败与完整状态 —— 按 M0 范围通过（附 F3-1）；证据包缺报告（F1-1）

- 已验证：金样覆盖 7.2 列出的成功、无变化、WrongVersion、ExpectedFragmentMismatch、ProjectionMismatch、SelectionMismatch、OutOfBounds、BudgetExceeded（补丁/文本/解析）、RereadFailed（隐藏目标丢失/可见文字变化/跨块格式）；`refusals.rs` 7 passed。
- 完整状态：探针覆盖 ProtectedRange、HtmlBoundary、PendingInput、pending+redo、composition+redo、WrongVersion；文本/选区/版本/undo/redo/节点数不变，redo 保留且可用，pending 组不封口，无空历史步。
- 缺口：预算/重读拒绝的真实 buffer 状态、`AlreadyFormatted` 真实 buffer 空步、仓库内 pending+redo 单一用例（F3-1）；字面命令 `cargo test -p aline-edit refusal` 未运行，实际运行的是更强的 `--test refusals`（F3-6）。
- 代码/测试通过，但四个 step 报告缺失使证据包不完整（F1-1），不能据此勾选。

### 7.3 正文/脚注交错、MD-11、HTML 边界、Source 放行、pending/组合/选区/undo/redo 与可消费协议 —— 按 M0 范围通过（附 F2-1/F3-2）；证据包缺报告（F1-1）

- 已验证：跨普通正文与脚注引用的删除/替换被拒绝、Source 放行；HTML 标签内格式被拒绝；front matter/TOC/脚注 Source 修改与 HTML 原文修改放行；拒绝不改变选区；pending/composition/选区/undo/redo 不被预检改变；计划保存 revision/snapshot/projection/selection/expectations，validate 会重新核对；3 补丁 consume 为 1 个历史节点。
- 缺口：MD-11 粘贴没有独立用例（与跨区删除/替换共用 Replace 路径）；HTML 跨界格式没有独立用例；列表/表格/格式化命令未实现（M0 范围，但 7.4 必须记录为未覆盖）；Visual Replace 未绑定选区（F2-1）；IME/UI 未覆盖（F3-2/F3-7）。
- 代码/测试通过，但报告缺失（F1-1），不能据此勾选。

### 7.4 回填与人工写入通道审查 —— 阻塞（F1-1/F1-2）

- `tasks.md` 7.1–7.4 仍为 `[ ]`；`docs/engine-design.md` 没有组 7 的 §8/§9/§14 回填（快照时 10 行、写报告时 12 行 diff 全是组 6 记录）；`docs/v1-architecture.md` §7.1/§7.2 无组 7 更新（快照时无 diff，之后仅组 6 6.8 的 2 行）；`docs/v1-prd.md` AC-04 与 `docs/v1-validation-plan.md` V-07 没有组 7 记录（prd 快照时无 diff，validation-plan 快照时 4 行、写报告时 6 行，均为组 6 记录）。
- 组 7 目录下没有 `report.md`/`README`；7.1–7.4 四个 step 报告缺失，任务 7.4 要求的 R/S 与场景覆盖没有落盘（F1-1）。
- 人工写入通道审查的方向正确：aline-edit 公开面无可变写入 API，src 中只有两处 `CheckedBatch::new`，无 `StringBuffer`/`apply`/`undo`/`redo`/`fs`；我的独立扫描与之一致。但日志没有记录具体命令，workspace 级结论未独立重扫（F3-3）。
- 因此 7.4 不满足可勾选条件；这是文档/证据前置条件，不是 `aline-edit` 代码缺陷。

## 5. 问题分级

### F1 阻塞（勾选任务前必须修复）

- **F1-1（7.1–7.4 证据）**：`group-7/` 下没有任何 `report.md`/`README`；7.1–7.4 四个 step 报告缺失。`tests/goldens.rs:291` 明确引用不存在的 `step-7.1/report.md`，使 footnote 金样差异的唯一书面解释缺失；任务 7.4 要求的 R/S 与场景覆盖也没有记录。必须补齐四个 step 报告（至少包含 7.1 金样勘误、7.2 拒绝状态证据、7.3 覆盖/未覆盖、7.4 R/S 与场景覆盖），否则现有证据包不能支撑任务勾选。
- **F1-2（7.4 回填）**：引擎 §8/§9/§14、架构 §7.1/§7.2、V-07/AC-04 的组 7 基础覆盖与未覆盖清单没有回填；`tasks.md` 7.1–7.4 未勾选。快照时及快照后（UTC 09:38–09:44）`docs/engine-design.md`、`docs/v1-validation-plan.md`、`docs/v1-architecture.md`、`docs/v1-prd.md` 的 diff 逐字核对全是组 6 记录，不能冒充组 7 回填。必须按任务 7.4 逐项回填，并明确列表/表格/格式化命令未覆盖。
- **F1-3（7.1 金样）**：`allow_source_footnote` 的目标选区金样写 `7..7`，按统一终点规则与字节数应为 `8..8`；7 会落在 `"[^2]"` 内部。测试 `expected_target` 先断言金样为 7、再返回 8，使 `golden_plan_cases` 通过而金样仍是错的。这不是实现缺陷，也不是从实现输出反向填充（8 可由同一规则独立推出），但金样作为 7.1 的核心交付物在该行不成立，且唯一解释指向缺失报告。必须在 step-7.1 报告中记录勘误并让测试例外引用该报告；不要把实现改成 7，也不要回填金样。

### F2（M1 前处理）

- **F2-1（7.3/7.1，所见替换绑定）**：`plan_replace` 的 Visual 分支只检查权限，没有要求编辑范围等于或位于选区；`plan_toggle_format` 则要求 `selected_range == selection.range()`（`plan.rs:300`）。探针 `selection=0..0`、Visual Replace 6..10 被允许，target 10..10、preview `alpha BETA`。计划绑定的 `before_selection` 与补丁范围可以不一致，validate 只检查当前选区等于计划的 `before_selection`。M1 UI 接入前必须明确并约束：要么要求 Visual Replace 的编辑范围等于选区，要么定义清楚“选区是光标、命令范围是权威”的协议并加测试；当前状态存在消费者误用风险。
- **F2-2（7.1/7.2，隐藏片段保留）**：`reread.rs:138-166` 的 `semantic_hidden` 只比较隐藏片段中的非空白字符，空白隐藏片段被丢弃，片段边界不比较。探针 `[x](https://e "a b")`、选区 1..2、Visual 替换 16..17 空格为 `""` 被允许，链接标题从 `"a b"` 变成 `"ab"`，可见文字仍为 `x`。隐藏链接目标（非空白）丢失会被拒绝，但空白/片段边界变化不会；这与 7.1“保留隐藏源片段”的强表述不一致。M1 UI 接入前必须要么精确比较隐藏片段（对 Format 插入的 marker 与可见编辑消费的空白做明确例外），要么把该归一化写成 V-07 的已接受限制并补测试。

### F3（证据强度限制）

- **F3-1（7.2）**：`refusals.rs` 7 例没有用真实 `StringBuffer` 覆盖 `BudgetExceeded`/`RereadFailed`/`ProtectedRange`/`HtmlBoundary`；`protection.rs` 与 `goldens.rs` 是 plan 级；`AlreadyFormatted` 没有真实 buffer 空步测试。探针补了 ProtectedRange/HtmlBoundary 与 pending/composition+redo，但预算/重读的真实 buffer 状态仍缺。
- **F3-2（7.3）**：MD-11 粘贴没有独立用例（与跨区删除/替换共用 Replace 路径）；HTML 跨界格式没有独立用例；列表/表格/格式化命令未实现（M0 范围），但 7.4 必须把它们记录为未覆盖；IME/UI 未覆盖。
- **F3-3（7.4 人工审查）**：`manual-write-channel-review.log` 没有记录具体的 `rg`/`sed` 命令，复现性有限；我的 aline-edit 独立扫描确认无写入通道，但 workspace 级“无其他实际写入入口”未独立重扫全仓。
- **F3-4（金样先行证据）**：`goldens-pre-implementation.log` 把本地 +0800 的 mtime 标成 `Z`（epoch 实际为 09:08:45Z）；日志只列文件是否存在，不记录当时 `lib.rs` 的内容/哈希；mtime 与索引暂存不是密码学证明；测试是在实现之后写的。结论“金样先行”在可观察证据上成立，但不能外推。
- **F3-5（环境与 workspace 门禁）**：`environment.stdout.log` 只打印 `CARGO_HOME` 是 macOS `printenv` 多参数行为，不代表另外两个变量未设置；`summary.json` 的 `env_overrides` 记录了三者。workspace gates 19/19 exit 0、passed=true、151 passed 只作为快照核对，未在并发写入窗口内独立重跑；09:30:53Z 的并发 `aline-markdown` 中间态失败说明工作区仍在变化，不能用该门禁记录判通过/失败。
- **F3-6（字面命令）**：任务 7.2 写的是 `cargo test -p aline-edit refusal`；实现者与本次复核运行的是 `--test refusals`（整目标 7 例，强于名字过滤）。没有名为 `refusal` 的过滤命令证据；这不影响 7 例全跑的结果，但字面命令未执行。
- **F3-7（真实消费方）**：7.1 的“实际消费方不获得可写真实 Buffer”在 M0 只由 API 形状、`refusals.rs` 与仓外探针验证；没有真实 UI/DocCore 消费者。M0 范围内可接受，但不能外推到 M1。
- **F3-8（证据可变性）**：组 7 证据全部未跟踪、未提交；复核期间组 6 仍在写 `docs/engine-design.md`/`docs/v1-validation-plan.md`/`docs/v1-architecture.md`/`docs/v1-prd.md` 与 `aline-markdown`。快照后（UTC 09:38–09:44）组 6 6.8 回填新增 8 行文档，逐字核对仍是组 6 记录，不是组 7 回填；本报告只覆盖 09:37:06Z 状态与 09:30:51Z–09:40:25Z 重跑窗口，晚到内容未复核。

## 6. 明确不声称

- 本报告**不判定 V-07、AC-04 或 M0 出口通过**。本报告只复核组 7 的代码、测试、金样与证据强度；V-07/AC-04 的回填缺失本身是 F1-2，不能反过来写成“未通过”或“通过”。
- 本报告**不判定真实 UI/IME/磁盘/自动保存/真实 DocCore 消费者**；本组没有真实 UI 消费者，只验证 API、测试与仓外探针。
- 本报告**不判定列表/表格/格式化命令**；M0 只实现 `Replace` 与 `ToggleInlineFormat`（Strong/Strike），没有实现 M3 全量命令，也没有把“未实现”写成“已完成”。
- 本报告**不宣称隐藏片段保留已完整**；F2-2 的隐藏链接标题空白变化是反例。
- 本报告**不宣称所见替换已绑定选区**；F2-1 的 `selection=0..0`、编辑 6..10 是反例。
- 本报告**不宣称金样全部正确**；F1-3 的 `allow_source_footnote` 目标 7..7 是反例，正确值为 8..8。
- 本报告**不宣称任务已勾选**；复核时 `tasks.md` 7.1–7.4 仍全部为 `[ ]`。即使后续勾选，也**不等于** M0 出口通过；tasks checked ≠ exit passed。
- 本报告**不宣称 workspace 级门禁已独立重跑**；只核对了实现者的 `summary.json`、日志与时间戳，未在并发写入窗口内独立重跑 workspace 级命令。
- 本报告**不评价组 6**；并发写入的组 6 文档与 `aline-markdown` 改动不在本报告范围。
- 本报告**不宣称 Windows/Linux 或其他 target**；所有结论仅限当前 macOS/arm64 与 Rust 1.98.1 环境。

复核者：独立子代理（非实现者）
