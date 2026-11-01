# 组 6（aline-markdown）round-3 独立复核

日期：2026-10-31（UTC）。复核窗口：`2026-10-31T11:12:09Z`–`2026-10-31T11:17:56Z`。基线 HEAD：`7916cea59247d4678765c8e66f69e9f33f680e6e`（`main`）。

本报告只判定 `review-round-2.md` 中组 6 发现的关闭状态，并给出组 6 逐任务结论。复核对象是代码、测试、日志与 git 对象，不以报告文字作为结论依据。工作区内并发的组 8 改动（`crates/aline-ui/src/document/`、`crates/aline-ui/tests/`、`docs/validation/m0-engine-foundation/group-8/`）未评价、未修改、未提交。本次复核只读：未改动源码/测试/既有证据，未提交，未勾选 `tasks.md`。

## 1. 方法与命令

| 类别 | 内容 |
|---|---|
| 实跑 | `cargo test -p aline-markdown --locked --offline --test reuse`（18 passed，exit 0） |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test hostile`（19 passed，exit 0） |
| 实跑 | `cargo test -p aline-markdown --locked --offline`（108 passed，exit 0；分目标：diagnostics 5、hostile 19、html 21、identity_mapping 2、mapping 16、protection 6、public_boundaries 2、reuse 18、rewrite 9、spec 10） |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test rewrite parse_budget_failure_falls_back_to_original_text`（1 passed，exit 0） |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test html unsupported_non_void_tags_record_the_v07_pending_asymmetry`（1 passed，exit 0） |
| 实跑 | 只读链接检查器 `python3 -I .../review-round-3-evidence/check_links.py .`（exit 0；139 个 md、1316 个链接目标、1259 个相对链接、111 个缺失，均不在 `docs/validation/`） |
| 只读核对 | `git rev-parse/log/cat-file/ls-tree/show --stat`；`rg`/`sed` 读源码与报告；逐条比对 `parse.rs`/`build.rs`/`projection.rs`/`reuse.rs`/`tree/html.rs` |
| 未执行 | 19 项工作区门禁、`cargo clippy`/`fmt`、`cargo test --workspace`、`dump_tree` 示例、修复前日志逐字节复跑、Windows/Linux |

命令与退出码：`docs/validation/m0-engine-foundation/group-6/review-round-3-evidence/commands.log`；git 核对原始输出同目录 `git-verification.log`；链接检查 `link-check.log`；测试日志同目录 `cargo-test-*.log`。环境：`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。

## 2. 逐项判定

| ID | 判定 | 依据方式 | 证据指针（代码/日志/git） |
|---|---|---|---|
| N-F1-1 组级报告与链接 | **关闭** | 实跑 + 只读核对 | `group-6/report.md` 存在且含逐任务索引；`link-check.log` 中 14 条 `GROUP_LINK` 全部 OK；`docs/engine-design.md:179`、`:437` 的组 6 报告链接解析成功；111 个缺失链接全部为 `docs/splat-study.md`（80 个清理前使用绝对路径的链接）与 `docs/maintenance/deletion-readiness`（31 个）既有链接，无一在 `docs/validation/` |
| F3-8 证据已提交 | **关闭** | 只读核对 | `git cat-file -t 55bd376`=`commit`；`git ls-tree 55bd376` 列出 group-6 证据（如 `report.md` blob `665c9745…`、`review-round-2.md` blob `5e9aadff…`、`probe-after.log` blob `f6b80def…`）；`55bd376` 全树 `.codegraph/` 计数 0；`report.md` §5 的 `58fa382`/`55bd376`/`45d4324` 与 `git log` 一致 |
| N-F2-1 跨行引用定义 | **关闭** | 实跑 + 只读核对 | `crates/aline-markdown/src/reuse.rs:250-286`：逐行状态机，`[` 置开态、`]` 闭合并记录 `label_spans_lines`，跨行闭 `]` 后紧跟 `:` 即返回 true，空行重置，`\` 跳过下一字节；`tests/reuse.rs:209-315` 的 5 个跨行用例；`--test reuse` 18 passed |
| N-F2-2 过滤扫描步数 | **关闭** | 实跑 + 只读核对 | `tests/hostile.rs:227-244` 三个过滤样本期望 5/3/3；`cross_validate_scan_steps`（同文件 64-102）用二分逼近真实预算阈值并与 `stats().scan_steps` 断言相等；`step-6.6/fix-n-f2-2-n-f2-3/probe-after.log` 实测 5/5、3/3、3/3、8/8、80/80；复刻循环 `tree/html.rs:648-702` 复用真实扫描器的 `is_filtered_tag`/`filtered_close_end`，报告明确称其为复刻；`--test hostile` 19 passed |
| N-F2-3 预算出口表与双重角色 | **关闭** | 实跑 + 只读核对 | 逐维核对：源码字节→`Err(ParseError::Source)`（`parse.rs:233-237`）；重写字节→`Err(ParseError::BudgetExceeded)`（`parse.rs:246`）；映射片段→`BuildError::BudgetExceeded`（`build.rs:495-496`）→`raw_fallback`（`parse.rs:268-275`）；扫描/嵌套/节点/HTML 栈深→同一 `BuildError` 回退路径（HTML 栈深经 `build.rs:789` 归一）；投影索引点→`MappingError::BudgetExceeded`（`projection.rs:249-253`、`:693`）→`ParseError::Mapping`。`max_mapping_points` 兼作片段数上限已在 `budget.rs:64`、`projection.rs:181`/`:251`、`parse.rs:304`、`step-6.6/report.md` §1、引擎 §12.1 记录与验证计划注明；`tests/hostile.rs:378-414` 用 `[a](u)\n`（2 点/3 片段）固定该语义。架构 §553 的同类表述另有错误，见 R3-F2-1 |
| N-F3-1 过滤/void 判定顺序 | **关闭** | 只读核对 | `step-6.5/report.md` §2 明确更正为 no-op；`tree/html.rs:338-340` 注释同步更正 |
| N-F3-2 未支持非 void 标签不对称 | **关闭** | 实跑 + 只读核对 | `tests/html.rs:390-409` 的 `unsupported_non_void_tags_record_the_v07_pending_asymmetry` 固定 `<foo>a   b</foo>`→`"<foo>a   b"` 与 `<span>` 对照；该单测 exit 0；`step-6.5/report.md` §8 标为 V-07 待定现状 |
| N-F3-3 复用差分比较表述 | **部分关闭** | 只读核对 | `docs/engine-design.md:437` 已加入“差分比较只在测试辅助中成立，生产 `same_block` 只比较块种类与块文本”；但 `step-6.7/report.md:11` 仍保留未限定的原句，见 R3-F3-1 |
| N-F3-4 映射失败两个出口 | **关闭** | 实跑 + 只读核对 | `step-6.2/report.md` §3 区分“片段数超限整源回退”与“重写字节超限 `Err`”；`tests/rewrite.rs:148-162` 以 `with_max_segments(0)` 断言 `Parsed`+`Raw`+1 诊断，本次实跑通过 |
| N-F3-5 映射金样来源 | **关闭** | 只读核对 | `step-6.4/report.md` §2 增“来源限定（复核 N-F3-5）”，不再称“手写”；`docs/v1-validation-plan.md` 的 N-F3-5 段落同口径 |
| N-F3-6 架构预算表述 | **部分关闭** | 实跑 + 只读核对 | `docs/v1-architecture.md:553` 已有“构树类维度整源回退 / 其余 `Err`”的更正，但把“映射片段”列入 `Err` 一侧，与代码和 `step-6.6` 表冲突，见 R3-F2-1 |
| N-F3-7 行内链接目标表述 | **关闭** | 只读核对 | `docs/engine-design.md:437` 明确行内链接目标变化不在 M0 复用比较范围，不再声称整体拒绝复用 |

## 3. 新发现

### R3-F2-1（重要）`docs/v1-architecture.md:553` 把“映射片段”预算超限误写为返回 `Err`

该段（标注“组 6 后状态补充（任务 6.8）”）写道：“超限的构树类维度（扫描步数、嵌套、节点、HTML 栈深）整源回退…而源码/重写字节、**映射片段**与投影索引点超限返回 `Err`（`BudgetExceeded` / `Mapping(BudgetExceeded)`）”。

实际行为：`max_segments` 在构树阶段检查，`build.rs:495-496` 返回 `BuildError::BudgetExceeded`，`parse.rs:268-275` 将其转成整源 `raw_fallback`（诊断 `BudgetExceeded`），**不是** `Err`。最小反例：源 `"# 标题\n\n正文\n"`、`ParseBudget::m0().with_max_segments(0)` 调用 `parse` 返回 `Ok(Parsed)`（绘制文本等于原文、块种类 `Raw`、1 条诊断），由 `tests/rewrite.rs:148-162` 固定，本次实跑 exit 0。`step-6.6/report.md` §1 的“映射片段→`BuildError::BudgetExceeded`→整源回退”才是与代码一致的表述。

影响：架构文档对预算出口的分类与实现、组 6 报告相互矛盾。若按字面实现消费者，会把“整源回退”当成错误而漏处理。建议把“映射片段”移入构树类回退一侧，或明确它指 `max_mapping_points` 兼作的投影片段上限（此时应与“投影索引点”合并表述，避免与 `max_segments` 混淆）。这是 round-2 处置（N-F3-6）新引入的错误，不影响 6.1–6.7 的代码行为。

### R3-F3-1（轻微）`step-6.7/report.md:11` 仍未限定“差分比较”的适用范围

原句：“差分比较不只比文本：还比较块种类、重定位后的范围、投影片段来源与精确性、保护范围、样式范围”。生产路径 `reuse.rs:183-191` 的 `same_block` 只比较块种类与块文本；更丰富的比较只在测试辅助中存在（引擎 §12.1 已注明）。该步骤报告是组 6 报告 §4 指向的处置位置，但未见对应限定。建议补一句“上述逐维比较仅在测试辅助；生产 `same_block` 只比 kind+text”，与 `docs/engine-design.md:437` 对齐。

## 4. 未覆盖与未执行

- 未重跑 19 项工作区门禁、`cargo clippy --workspace --all-targets -D warnings`、`cargo fmt --all --check`、`cargo test --workspace`；本报告不对这些门禁在本时点的结果作新判断（组 8 并发改动会影响 workspace 结果）。
- 未复跑 `dump_tree` 示例与 `step-6.1`–`step-6.8` 的修复前/修复后历史日志；历史计数只按报告引用，不据日志重新求和。
- 链接检查器为文本级 `](...)` 扫描：不校验锚点片段、不解析 HTML/reference 链接；已剥离 fenced/inline code。111 个缺失链接均位于 `docs/validation/` 之外，但本次未逐个修复或判定其历史责任。
- 未执行 V-03、V-05、V-06、V-07、AC-09 的验收活动；未验证真实 UI 命中、IME、灰色只读呈现、Windows/Linux/CI。
- 未评价组 8 的任何文件与结论。

## 5. 逐任务结论（M0 范围）

| 任务 | 结论 | 依据 |
|---|---|---|
| 6.1 夹具与显式配置 | **通过（M0 范围）** | `spec` 10 passed（本轮全 crate 实跑）；报告记录原生选项/必要 pass 依据；无 round-2 发现 |
| 6.2 RewriteMap 与回退反例 | **通过（M0 范围）** | `rewrite` 9 passed；片段数超限整源回退与重写字节超限 `Err` 两个出口均有测试（本轮单测实跑） |
| 6.3 块/Gap 与 dump | **通过（M0 范围；本轮未复跑 dump）** | 组级报告记录 7 夹具/57 区间与 dump 日志；`step-6.3/report.md` 存在；本次未重跑示例 |
| 6.4 映射 | **通过（M0 范围）** | `mapping` 16 passed；金样来源限制已在两处文档记录（N-F3-5） |
| 6.5 有限 HTML 与保护 | **通过（M0 范围）** | `html` 21 passed、`protection` 6 passed；N-F3-1/N-F3-2 均已更正/固定为现状 |
| 6.6 预算、敌意输入与取消 | **通过（M0 范围）** | `hostile` 19 passed；八维出口与代码逐项一致；N-F2-2/N-F2-3 关闭 |
| 6.7 复用 | **通过（M0 范围）** | `reuse` 18 passed；跨行定义状态机与拒绝用例成立；仅余 R3-F3-1 的文档措辞 |
| 6.8 回填与缺口清单 | **部分成立** | 组级报告、链接、引擎 §6/7/11/12 回填与缺口清单均已就位；但标注“任务 6.8”的架构补充段含 R3-F2-1 的错误分类，需修正后才宜认定为完成 |

组级判断：round-2 的唯一阻断项（6.8）在“报告存在/链接可达/证据已提交”意义上已解除；新发现的 R3-F2-1 是 6.8 回填文本中的事实错误，构成本轮唯一重要发现。无 R3-F1 阻断项。

## 6. 不宣称

- 不宣称 V-03、V-05、V-06、V-07 通过；不宣称 AC-09 通过。
- 不宣称 M0 出口通过，也不宣称 M1 可以开始。
- 不宣称组 8 状态、不评价其任何实现或文档。
- 不宣称本报告替代调度者处置；不勾选 `tasks.md`，不提交、不 `git add`、不 push。
- `tasks.md` 中 6.8 仍未勾选；本报告不改变复选框状态。
