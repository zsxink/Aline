# 组 5（5.1–5.5）独立复核报告

- 复核日期：2026-10-31
- 复核对象：OpenSpec change `m0-engine-foundation` 组 5（任务 5.1–5.5）
- 仓库与基线：`/Users/xian/Project/editor/Aline`，分支 `main`，HEAD `9b93381cb647eec0dddec869674e6bd86759e001`
- 工作树状态：组 5 的源码、测试、金样与证据均为未提交或未跟踪文件（见 `evidence/git-state.stdout.log`）；复核期间除本文件外未改动其他文件
- 复核立场：实现者不是复核者。报告与日志中的自述只作为待验材料；本报告的结论来自本次独立重跑、独立重算与读码，不把自述当成已证明
- 写入范围：仅本文件

## 1. 执行环境与命令

### 1.1 环境

- 主机：macOS arm64，Darwin 27.2.0（本次复核时）；证据 `environment.stdout.log` 记录 Darwin 27.0.0（见 F3-6）
- 工具链：rustc 1.98.1 (48a229cea 2026-09-01)，cargo 1.98.1 (797e8a9bc 2026-08-05)
- 环境变量：`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`，`RUSTUP_TOOLCHAIN=1.98.1`，`CARGO_NET_OFFLINE=true`
- 所有 cargo 命令均使用 `--locked --offline`

### 1.2 实际执行的命令与结果

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo test -p aline-buffer --locked --offline` | 0 | 29 个测试通过（5 个单元测试 + 5 anchors + 2 checked_batch + 9 history + 2 patch_goldens + 2 public_boundaries + 4 rejections） |
| `cargo test -p aline-buffer --locked --offline patch` | 0 | 恰好 3 个测试：`anchors_transform_multiple_patches_by_cumulative_delta`、`history_handles_keep_trimmed_patch_resources_alive`、`forward_inverse_patch_goldens_round_trip_after_coordinates` |
| 独立 Python 重算正逆补丁金样 | 0 | 134/134 例一致，0 问题；覆盖正补丁、after 坐标逆补丁、往返、after 选区边界/UTF-8、无变化分类、相邻逆补丁合并 |
| 独立 Python 重算行索引金样 | 0 | 11/11 例一致，0 问题；覆盖行数、LF/CRLF 排除、孤立 CR、offset 到行、预算拒绝 |
| `shasum -a 256` 五份回填文档；`git show HEAD:<file> \| shasum -a 256` | 0 | 当前哈希与 `step-5.5/report.md` 的“回填后”一致；HEAD 版本与“回填前”一致（见 3.6） |
| `git diff --stat -- docs/`；`git diff --check` | 0 | 5 个文档、10 行新增、0 行删除；无空白错误 |
| Python 读取 `step-5.5/workspace-gates/summary.json` | 0 | 顶层 `passed=true`，19 项结果全部 `exit_code=0` |
| 汇总 `workspace-gates/*.log` 的 `test result: ok.` | 0 | 29 条结果行，合计 76 个测试通过 |
| `/tmp/aline-review-probe` 的 `cargo run --offline` | 0 | 对抗性探针 C1–C6，输出见第 4 节 |
| `uname -a`、`stat`、`nl`、`rg` 读码与证据核对 | 0 | 用于行号、mtime、环境与日志一致性核对 |

说明：

- 本次复核未单独重跑 `--test anchors`、`--test rejections`、`--test history` 三个过滤命令；全 crate 运行已包含这些测试并全部通过，且实现者 `commands.log` 中对应行均为 `exit=0`。
- 本次复核未重跑 `step-5.3/run-audit.sh`，只核对其 `audit.stdout.log`。
- 本次复核未重跑 19 项 workspace gates，只核对 `summary.json`、`test.stdout.log` 与命令记录。
- 证据中的环境日志只显示 `CARGO_HOME`，经本机复现，macOS `/usr/bin/printenv A B C` 只打印第一个参数，因此不能据此判断另外两个变量未设置；`run-targeted-tests.sh:7-9` 与 `workspace-gates/summary.json` 的 `env_overrides` 均记录了三者。

### 1.3 临时产物

- `/tmp/aline-review-crate.log`：全 crate 测试输出
- `/tmp/aline-review-patch.log`：`patch` 过滤测试输出
- `/tmp/aline-review-probe/`：对抗性探针 crate（`Cargo.toml`、`src/main.rs`，以 path 依赖 `aline-buffer`）
- `/tmp/aline-review-probe.out`、`/tmp/aline-review-probe.err`：探针输出
- `/tmp/envsim.out`、`/tmp/envsim.err`：`printenv` 行为复现输出

除上述系统临时目录产物和本文件外，未写入仓库文件。

### 1.4 未执行

- 未重跑 workspace gates 与 `run-audit.sh`
- 未在 Windows/Linux 或其他 target 上构建/测试
- 未验证真实 IME、UI、自动保存/外改提示、真实磁盘读写、断电/权限/竞争、恢复闭环
- 未单独重跑 clippy/fmt；以 workspace-gates 记录为准

## 2. 总体结论

- 5.1–5.5 的证据足以支持勾选。金样数量与内容、29 个 crate 测试、`patch` 过滤 3 个测试、76 个 workspace 测试、19 项门禁 exit 0、5 份文档哈希与 append-only 回填，均经本次独立复核一致。
- 未发现 F1 阻塞问题。
- 发现 1 个 F2 语义不一致：锚点变换使用原始批次补丁，而 `apply` 会先剔除无操作补丁；同一批次会得到“文本已变/未变”与“锚点失效/有效”的矛盾结论。它不损坏文本，也不阻塞本次勾选，但应在 M1 锚点消费者接入前裁决。
- 其余为 F3 残余风险（裁剪策略未决、无变化路径不校验选区、audit 脚本无 `set -e`、金样先行只能靠 mtime、undo/redo 只比长度、证据 OS 指纹不同）。
- 本复核不判定 V-07、AC-01～04 或 M0 出口通过。TA-02 的真实自动保存暂停/外改提示仍属 M2。本 crate 没有磁盘基线：`reload` 只接收 `String` 并修改内存，文档中的“不移动磁盘基线”是面向未来集成的契约表述，不是本 crate 已实现的行为。

## 3. 逐项核对

### 3.1 补丁坐标与逆补丁（5.1）

- 规范点：正补丁用 before 坐标，逆补丁用 after 坐标；逆补丁范围要计入前面补丁造成的长度差；相邻逆补丁合并；无操作补丁不产生发布。
- 代码位置：`crates/aline-buffer/src/string_buffer.rs:143-195`（`apply` 先校验、再 `effective_patches`、构造 after 文本/逆补丁/行索引/历史计划，最后统一赋值）；`:377-400`（剔除空插入和内容相同的替换）；`crates/aline-buffer/src/batch.rs:41-96`（批次校验）。
- 测试与证据：`crates/aline-buffer/tests/patch_goldens.rs:49-186` 断言 134 例的逆补丁、after 选区、after 文本、revision、历史节点、历史正补丁与逆补丁往返；金样 `step-5.1/goldens/forward-inverse.tsv` 为 2 行头/注释 + 134 行数据；`generate_goldens.py` 用固定 seed 20261031，14 例手工 + 120 例随机。
- 独立复核：本次用独立 Python 实现重算全部 134 例，包括 `abcXYZ -> aQQc!`、空插入、纯删除、CRLF、多字节字符、相邻逆补丁合并、无变化分类与 after 选区边界，结果 0 问题。
- 结论：通过。正/逆坐标、长度差累计、合并与无操作剔除均有代码、测试与独立重算支撑。

### 3.2 批次原子性与拒绝（5.2）

- 规范点：同基线、升序、不重叠、合法半开 UTF-8、受检长度；任一项非法整批拒绝且零修改；无变化批次不发布、不清 redo。
- 代码位置：`crates/aline-buffer/src/batch.rs:41-85`（数量/边界/UTF-8/升序/重叠/同点空插入歧义/长度预算/输出长度）；`:88-96`（先 revision 后快照身份）；`crates/aline-buffer/src/string_buffer.rs:151-188`（所有校验和构造在赋值之前，`prepare_record` 失败时尚未发布）。
- 测试与证据：`crates/aline-buffer/tests/rejections.rs` 4 个测试，状态快照包含文本、行索引/行范围、选区、revision、undo/redo 深度、节点数、快照身份；覆盖外部快照、非法 after 选区、文本预算、行数预算、历史节点/字节预算；`:267` 覆盖 Undo 后无变化批次不清 redo，随后仍可 redo。
- 独立复核：读码确认赋值顺序为 `snapshot/selection/index` 之后才 `commit_record`；探针 C5 确认空批次返回 `Unchanged` 且不移动选区（选区校验问题见 F3-2）。
- 结论：通过。要求中的拒绝类别均整批失败且状态不变；无变化批次不发布、不清 redo。

### 3.3 锚点（5.3）

- 规范点：替换起点按 `Before/After` affinity，替换内部失效，替换终点映射到替换文本之后；插入点按 affinity；同点插入+替换累计长度差；选区方向保持，任一端失效则整组失效；锚点与批次必须绑定当前快照。
- 代码位置：`crates/aline-buffer/src/anchors.rs:45-54`（`transform_anchor`）、`:56-73`（`transform_selection`）、`:75-116`（`transform_position`）。
- 测试与证据：`crates/aline-buffer/tests/anchors.rs` 5 个测试，覆盖替换起点 Before/After 与内部失效、插入与同点插入+替换 `[(2..2,"X"),(2..4,"Y")]`、多补丁累计差、方向性选区与半有效拒绝、外部快照/过期批次。
- 独立复核：读码确认累计差、插入 affinity、替换起点提前返回与内部失效逻辑正确；探针发现无操作补丁组合的不一致（F2-1）。
- 结论：规范列出的锚点场景通过；无操作批次与锚点组合存在语义矛盾，见 F2-1。

### 3.4 历史（5.4）

- 规范点：900ms 只合并相邻普通输入；seal/source 边界；保存点按快照身份而非栈深；内容相等 redo 保守 dirty；新编辑丢弃 redo 分支；裁剪保护保存点与当前节点；undo/redo 预算失败原子；reload 是独立内存边界且无变化不清 redo；历史句柄在裁剪后仍可达。
- 代码位置：`crates/aline-buffer/src/history.rs:9`（900ms）、`:262-340`（`prepare_record`）、`:342-383`（`commit_record`）、`:239-248`（`protected_prefix`）、`:424-447`（`mark_saved`/`is_at_save_point`）、`:495-525`（`can_merge`）；`crates/aline-buffer/src/string_buffer.rs:200-251`（undo/redo）、`:256-296`（reload）。
- 测试与证据：`crates/aline-buffer/tests/history.rs` 9 个测试，分别覆盖来源/选区/单调 revision、900ms 边界（900 合并/901 不合并）、seal 与非普通输入边界、保存后 Undo 再编辑栈深相同但非保存点、内容相等 redo 仍 dirty、redo 分支丢弃、裁剪保护保存点/当前节点、undo/redo 预算失败状态不变、reload 独立边界与无变化保留 redo、裁剪后句柄仍持有补丁。
- 独立复核：读码确认 `can_merge` 只允许 Typing、待定节点、900ms 内、相邻折叠选区、单补丁、插入-插入或退格-退格；`mark_saved` 只按快照身份匹配，不用栈深；`commit_record` 在截断/裁剪后同步调整或清空保存点索引；`HistoryStepHandle` 持有 `Arc<HistoryStep>`，被裁剪步骤的补丁仍可达。
- 磁盘基线：`crates/aline-buffer/Cargo.toml` 无 `[dependencies]`，源码无文件 I/O；`reload` 只把传入的 `String` 替换进内存并生成正逆补丁，因此本 crate 没有磁盘基线，“不移动磁盘基线”只是未来集成契约。
- 探针 C3：保存点位于 Undo 后状态时 `mark_saved` 得到 `node_index=None`，裁剪保护为 0，受限历史预算下新编辑返回 `HistoryBudgetExceeded` 且状态不变（见 F3-1）。
- 结论：规范要求的历史语义通过；裁剪策略的未决行为见 F3-1。

### 3.5 低层边界（5.5）

- 规范点：不引入 I/O、解析、unsafe/libc、哈希库或新依赖；`StringBuffer` 唯一持有状态；历史补丁用 `Arc` 共享；裁剪后的句柄仍可达。
- 代码位置与证据：`crates/aline-buffer/Cargo.toml` 只有 package/lints，无 `[dependencies]`；`step-5.3/audit.stdout.log` 第 1 节（I/O/unsafe/libc）为空，第 2 节唯一命中是 `crates/aline-buffer/src/text.rs:149` 的文档注释“markdown”，第 3 节（哈希/指纹）为空，第 4 节 `cargo tree` 只有 `aline-buffer v0.1.0`；第 5 节显示 `StringBuffer` 字段与 `HistoryStep` 的 `Arc<Vec<...>>`；`tests/history.rs:483-527` 验证裁剪后句柄仍可读旧补丁。
- 独立复核：读码确认无 `unsafe`、无文件/网络调用、无解析器或哈希依赖；`effective_patches` 与 `apply_patches_to_text` 只用 `Vec`/`Arc`/`String`。
- 结论：通过。未发现越界依赖或所有权共享问题；audit 脚本自身的健壮性问题见 F3-3。

### 3.6 证据与报告一致性

| 项目 | 报告/证据声称 | 本次复核结果 |
| --- | --- | --- |
| 正逆补丁金样 | 134 例（14 手工 + 120 随机，seed 20261031） | TSV 为 2 行头/注释 + 134 行数据；独立重算 134/134 一致 |
| 行索引金样 | 11 例 | TSV 为 2 行头/注释 + 11 行数据；独立重算 11/11 一致 |
| crate 测试 | 29 个通过 | 重跑 `cargo test -p aline-buffer --locked --offline` 为 29 passed，exit 0 |
| `patch` 过滤 | 3 个测试 | 重跑恰好 3 个，名称与报告/指南一致 |
| workspace 测试 | 76 个通过 | 29 条 `test result: ok.`，合计 76 passed |
| workspace 门禁 | 19 项全部 exit 0 | `summary.json` 顶层 `passed=true`，19 项 `exit_code=0`；时间窗 06:29:09.117462Z–06:29:31.775798Z 与报告一致 |
| 定向测试 UTC/退出码 | 06:24:19Z–06:24:22Z，全部 exit 0 | `commands.log` 一致；本次重跑核心命令也 exit 0 |
| 文档哈希 | 5 对回填前/后 | `shasum` 当前值与“回填后”一致；`git show HEAD:` 与“回填前”一致 |
| 文档差异 | 5 文件/10 行新增/0 删除 | `git diff --stat -- docs/` 完全一致；`git diff --check` 通过 |

补充说明：

- 完整工作树的 `git diff --stat` 还包含 `crates/aline-buffer/src/lib.rs`（+13）与 `src/text.rs`（+27）的未提交组 5 源码改动；“5 文件/10 行”是文档回填的统计，两者不矛盾。
- 金样先于实现的唯一可核对证据是文件 mtime（本地时间，UTC+8）：金样 14:11:30，`string_buffer.rs` 14:21:32，`patch_goldens.rs` 14:23:17，定向测试 14:24:19–14:24:22，门禁 14:29:09–14:29:31。由于这些文件均未跟踪，git 无法证明先后顺序；金样内容本身已由独立重算验证（见 F3-4）。
- 报告与文档没有把本地测试写成 V-07/AC/M0 通过：`v1-prd.md:400` 明确 AC-01～04 均未通过；`v1-validation-plan.md:93` 明确不能据此判定 V-07 通过；`engine-design.md:324` 与 `v1-architecture.md:500` 明确不构成 V-07/M0 出口通过、TA-02 仍属 M2；`step-5.5/report.md` 明确 `--all-targets` 不替代其他 OS/target 与 CI。

### 3.7 文档回填

- 5 份文档均为追加式状态段，无删除：`docs/engine-design.md:324`、`docs/v1-architecture.md:500`、`docs/rust-style-guide.md:273`、`docs/v1-prd.md:400`、`docs/v1-validation-plan.md:93`。
- 内容边界明确：不含命令规划、IME、自动保存/外改提示、全文指纹、资源阈值；AC-01～04 四项均未通过；V-07 未通过；TA-02 真实自动保存暂停/外改提示仍属 M2。
- 哈希与 diff 见 3.6；`tasks.md` 中 5.1–5.5 仍为未勾选状态，符合“复核后再勾选”的流程。

## 4. 发现

### F1 阻塞问题

未发现阻塞问题。

### F2-1 锚点变换与 `apply` 对无操作补丁的处理不一致

- 位置：`crates/aline-buffer/src/anchors.rs:53`（`transform_anchor` 直接使用 `batch.patches()`）、`crates/aline-buffer/src/anchors.rs:65-66`（`transform_selection` 同样使用原始补丁）；`crates/aline-buffer/src/string_buffer.rs:152`（`apply` 使用 `effective_patches`）、`:377-400`（剔除空插入和内容相同的替换）。
- 复现（本次探针 `/tmp/aline-review-probe`，exit 0）：
  - C1：before 文本 `"abcd"`，批次 `[(1..3, "bc"), (3..3, "X")]`。`apply` 返回 `Changed`，文本 `"abcXd"`，after 偏移 2 是合法 UTF-8 边界；但 `transform_anchor(before, offset 2, batch, Before)` 返回 `Ok(Invalidated)`。
  - C2：before 文本 `"abcd"`，纯无操作批次 `[(1..3, "bc")]`。`apply` 返回 `Unchanged`，revision 仍为 0；但同一批次的锚点 2 仍返回 `Ok(Invalidated)`。
- 实际：`apply` 把内容相同的替换视为未变化并只发布实际变化的插入；锚点变换却按原始补丁把偏移 2 判为“非空替换内部”，返回失效。`transform_selection` 在任一端失效时返回 `None`，会把选区整组丢弃。
- 期望：同一批次下，锚点/选区变换应与实际发布语义一致。无操作替换不应使内部锚点失效；或者 API 必须明确要求调用方先剔除无操作补丁。当前 `effective_patches` 是私有函数，调用方无法自行预过滤。
- 影响：不损坏文本，不造成数据丢失；但下游若用锚点保持光标/选区，会在“格式化命令恰好产生内容相同替换”等场景丢失位置。锚点文档本身写明“非空替换内部失效”，因此代码与自身文档一致，矛盾在系统层：`apply` 与 `transform_*` 对同一批次给出不同结论。
- 建议（三选一）：在 `transform_*` 内复用 `effective_patches`；或公开受检的过滤函数并要求调用方预过滤；或明确禁止把含无操作替换的批次传给 `transform_*` 并补测试。无论选哪种，都应增加“无操作替换 + 锚点/选区”回归测试。
- 阻塞性：不阻塞 5.1–5.5 勾选。规范列出的 5.3 场景（替换内部失效、同深分支、重载边界、预算）不包含无操作批次，且当前无下游锚点消费者；但必须在 M1 锚点消费者接入前裁决。

### F3-1 裁剪策略在保存点与预算下的未决行为

- 位置：`crates/aline-buffer/src/history.rs:239-248`（`protected_prefix`：保存点 `node_index=None` 时保护数为 0）、`:424-441`（`mark_saved` 只匹配节点末步的 `after_id`）、`:342-383`（`commit_record` 的自动裁剪与保存点索引调整）。
- 复现（探针 C3）：文本 `"a"`，历史预算 3 节点；依次应用 A、B，Undo，`mark_saved`，再应用 C、D（节点 3），再应用 E。结果 `apply_E = Err(HistoryBudgetExceeded { needed: 4, limit: 3 })`，节点数仍 3，文本仍 `"aACD"`。
- 实际：保存点位于 Undo 生成的新快照上，`mark_saved` 找不到对应历史节点，`node_index=None`，裁剪保护为 0，因此不裁剪，直接受检失败。
- 期望/规范：规范允许“受检失败或执行已声明的安全裁剪协议”；该行为是保守且安全的（不丢保存点、不丢文本）。但 `apply` 在预算内自动裁剪最旧节点（`tests/history.rs:298-310` 断言 5 次编辑后只保留 3 个节点），且 `ApplyOutcome` 不报告裁剪；最终裁剪策略在 `engine-design.md` §9.3 仍标注待 V-06。下游不能假定撤销深度固定。
- 影响：保存点位于 Undo/Redo 状态后，受限历史预算可能拒绝编辑而不是裁剪；无数据损坏。现有测试未覆盖“保存后 Undo 再受限预算”的组合。
- 建议：在文档中明确该保守失败语义，并补一条回归测试；V-06 冻结裁剪策略时再决定是否把内容等价的保存点映射到节点。

### F3-2 `Unchanged` 路径不校验 after 选区

- 位置：`crates/aline-buffer/src/string_buffer.rs:153-155`（`apply` 在 `forward.is_empty()` 时直接返回 `Unchanged`）、`:264-266`（`reload` 在 `old_text == new_text` 时直接返回 `Unchanged`）。
- 复现（探针 C5/C6）：对 `"abcd"` 调用空批次 `apply` 并传 `SelectionSpec::new(999, 999)`，返回 `Ok(Unchanged)`，选区保持 0..0；对相同文本调用 `reload` 并传同一非法选区，返回 `Ok(ReloadOutcome::Unchanged)`。
- 实际：有变化的批次会因非法 after 选区被拒绝；无变化批次却静默忽略非法选区。
- 期望：至少应明确“无发布时不校验选区”的契约；若希望 API 一致，应在返回 `Unchanged` 前也做边界校验。
- 影响：不损坏状态，但可能掩盖调用方错误；属于 API 语义不一致。
- 建议：在文档中写明，或统一校验；补一条无变化路径的选区边界测试。

### F3-3 `run-audit.sh` 只有 `set -u`，没有 `set -e`

- 位置：`docs/validation/m0-engine-foundation/group-5/step-5.3/run-audit.sh:3`。
- 实际：脚本用 `rg ... || true` 处理“空结果表示无命中”是合理的；但 `cargo tree` 或其他关键命令失败时不会中止，后续 `rg` 成功仍可能让脚本整体 exit 0。本次 `audit.stdout.log` 显示 `cargo tree` 实际成功、各节输出齐全，因此当前证据未被掩盖。
- 影响：未来审计失败可能被误记为通过。
- 建议：改为 `set -eu` 并对关键命令显式检查状态，或为每节记录独立 exit code。

### F3-4 “金样先于实现”只能靠 mtime 证明

- 位置：`evidence/git-state.stdout.log` 显示组 5 源码/测试/金样均未跟踪；`stat` 显示本地时间金样 14:11:30、`string_buffer.rs` 14:21:32、`patch_goldens.rs` 14:23:17、定向测试 14:24。
- 实际：mtime 顺序支持“先金样后实现”，但未跟踪文件无法用 git 历史证明；金样内容已由独立重算验证，因此内容正确性不依赖该顺序。
- 影响：流程声明可被质疑，但不影响功能正确性。
- 建议：后续组先提交金样再提交实现，或在证据中记录金样哈希与生成时间。

### F3-5 undo/redo 的上下文检查只比较字节长度

- 位置：`crates/aline-buffer/src/string_buffer.rs:204`（undo 检查 `text.len() != step.after_len()`）、`:230`（redo 检查 `text.len() != step.before_len()`）。
- 实际：补丁应用前只验证长度，不验证内容身份；在内部顺序不变量下（历史步骤由本 crate 生成、快照不可变、补丁 `Arc` 只读）是安全的，但如果未来允许外部注入历史步骤或跨缓冲区复用步骤，长度相同而内容不同的文本可能被错误应用。
- 影响：当前低风险；属于未来扩展的设计约束。
- 建议：在历史步骤文档中写明“步骤只能由同一 `StringBuffer` 的顺序历史使用”的不变量，或增加内容/身份校验。

### F3-6 证据环境指纹与本次复核主机不同

- 位置：`docs/validation/m0-engine-foundation/group-5/evidence/environment.stdout.log:2` 记录 `Darwin ... 27.0.0`；本次复核主机为 Darwin 27.2.0。
- 实际：若证据运行后主机发生过系统升级，则差异无实质影响；否则说明证据不是在当前主机状态下生成。核心命令已在 Darwin 27.2.0 上重跑通过，测试数量与结果一致。
- 影响：环境可复现性存在不确定性；不影响 5.1–5.5 的功能结论。
- 建议：在证据中同时记录 `sw_vers` 与复核时环境，或说明升级时间。

## 5. 未覆盖与未执行清单

- 未重跑 workspace gates（19 项）与 `run-audit.sh`，只核对已有 `summary.json`、日志与命令记录
- 未在 Windows/Linux 或其他 target 构建/测试；证据中的 `--all-targets` 只代表当前 macOS 主机
- 未验证真实 IME/组合确认/取消、UI、命令规划、格式化差异确认、只读保护
- 未验证自动保存、外部修改检测、冲突提示、真实磁盘保存/重开、断电/权限/竞争、恢复闭环
- 未验证真实文件字节保真与磁盘基线；本 crate 没有磁盘基线，`reload` 仅为内存边界
- 未单独重跑 clippy/fmt；以 workspace-gates 记录为准
- 未单独重跑 anchors/rejections/history 过滤命令；全 crate 运行已覆盖其测试
- 未验证 CI 是否执行；证据报告已明确不证明 CI

## 6. 最终建议

- 建议勾选 5.1–5.5。依据是本次独立复核确认了金样、测试数量、门禁、哈希、原子性/拒绝、锚点规范场景、历史语义与低层边界；未发现 F1 阻塞问题。
- 勾选前没有必须修复的阻塞项。F2-1 是唯一的语义不一致，建议作为组 5 的后续任务，在 M1 锚点消费者接入前完成裁决与回归测试；若流程要求“零已知语义矛盾才勾选”，则它是唯一应先处理的项。
- 建议在勾选记录中保留 F3-1、F3-2 的未决说明，并修复 F3-3 的审计脚本；F3-4、F3-5、F3-6 作为流程/设计残余风险跟踪。
- 本建议不等于 V-07、AC-01～04 或 M0 出口通过；TA-02 的真实自动保存暂停/外改提示仍属 M2；组 5 只证明无头 buffer 原语。
