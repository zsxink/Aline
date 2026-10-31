# 组 6（aline-markdown）round-4 独立复核

日期：2026-10-31（UTC）。复核窗口：`2026-10-31T12:56:47Z`–`2026-10-31T13:22:16Z`。基线 HEAD：`1dd0174ca8fc4aa975209ee5d6c3169c36695fad`（`main`），工作区含未提交文档更正。

本轮只判定 round-3 的两项发现（R3-F2-1、R3-F3-1）是否关闭，并复判任务 6.8；**不重判 6.1–6.7 的原有结论**。复核对象是工作区代码、测试、日志与 git 对象，不以报告文字作为依据。本次复核只读：未改动源码/测试/期望/既有报告/`tasks.md`，未 `git add`/`commit`/`push`；工作区并发的组 7、组 8 改动（`docs/validation/m0-engine-foundation/group-7/`、`group-8/`）未评价、未修改。

**重要前提**：本轮核验的两处更正（`docs/v1-architecture.md:553`、`step-6.7/report.md:11`）目前**只在未提交的工作区**；HEAD `1dd0174` 的对应行仍是旧文本。判定的是工作区状态，不代表已提交状态。

## 1. 方法与命令

| 类别 | 内容 | UTC / 退出 | 证据指针 |
|---|---|---|---|
| 实跑 | `cargo test -p aline-markdown --locked --offline --test rewrite parse_budget_failure_falls_back_to_original_text` | 12:56:47Z / 0（1 passed） | `review-round-4-evidence/cargo-test-rewrite-segment-fallback.log` |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test rewrite` | 12:56:52Z / 0（9 passed） | `review-round-4-evidence/cargo-test-rewrite.log` |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test hostile` | 12:57:02Z–12:57:03Z / 0（19 passed） | `review-round-4-evidence/cargo-test-hostile.log` |
| 实跑 | `cargo test -p aline-markdown --locked --offline` | 12:57:09Z–12:57:10Z / 0（108 passed：diagnostics 5、hostile 19、html 21、identity_mapping 2、mapping 16、protection 6、public_boundaries 2、reuse 18、rewrite 9、spec 10） | `review-round-4-evidence/cargo-test-markdown-full.log` |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test hostile mapping_point_budget` | 13:07:58Z / 0（2 passed） | `review-round-4-evidence/cargo-test-hostile-mapping-point-budget.log` |
| 实跑 | `cargo test -p aline-markdown --locked --offline --test diagnostics segment_budget_fallback` | 13:07:59Z / 0（1 passed） | `review-round-4-evidence/cargo-test-diagnostics-segment-fallback.log` |
| 只读核对 | 逐行快照 `parse.rs`/`build.rs`/`budget.rs`/`projection.rs`/`reuse.rs`/`tree/html.rs`/`pre/rewrite.rs` 与 `tests/rewrite.rs`/`tests/hostile.rs`/`tests/reuse.rs` | 12:58:38Z / 0 | `review-round-4-evidence/code-snapshot.log` |
| 只读核对 | `git rev-parse`/`status`/`diff`/`show`/`numstat` 与两处更正 diff | 12:58:31Z / 0 | `review-round-4-evidence/git-verification.log` |
| 只读核对 | `rg` 读 `tasks.md` 6.8、引擎 §6/7/11/12、验证计划 V-06/V-07、PRD AC-09、`step-6.7/report.md` | 12:58:44Z / 0 | `review-round-4-evidence/doc-check.log` |
| 只读检索 | 残留检索：`映射片段…Err`、`差分比较|逐维比较|same_block` | 12:58:45Z / 0；13:20:39Z / 0 | `review-round-4-evidence/residual-search.log`、`residual-search-differential.log` |
| 只读核对 | 6.8 回填哈希跨提交比对（`45d4324^`、`45d4324`、`99867bf`、工作区） | 13:12:41Z / 0 | `review-round-4-evidence/backfill-hash-crosscheck.log`、`backfill-posthash-locate.log` |
| 只读核对 | 回填后哈希来源定位与 `step-6.8/backfill-hashes.log` | 13:13:37Z / 0 | `review-round-4-evidence/backfill-posthash-locate.log` |
| 只读核对 | 三次门禁 `summary.json`（19 项、exit code、时点）与 `cargo test --workspace` 计数重算 | 13:12:41Z / 0；13:13:19Z / 0；13:22:16Z / 0 | `review-round-4-evidence/workspace-gates-count-crosscheck.log`、`workspace-test-counts.log`、`workspace-gate-summary.log` |
| 只读核对 | 6.8 宣称边界检索（AC-09/公式/图表/V-06/V-07 通过） | 13:14:21Z / 0 | `review-round-4-evidence/claims-check.log` |
| 未执行 | 19 项工作区门禁、`cargo clippy`/`fmt`、`cargo test --workspace`、`dump_tree` 示例、`step-6.1`–`step-6.8` 历史日志逐字节复跑、链接检查器、Windows/Linux/CI | — | — |

环境：`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`；cargo 一律 `--locked --offline`。

## 2. 更正核验

### 2.1 更正 1（`docs/v1-architecture.md:553`）——**准确**

工作区该段现表述为：“超限的构树类维度（扫描步数、嵌套、节点、HTML 栈深、**映射片段数 `max_segments`**）整源回退…；源码字节或重写字节超限返回 `Err`（`Source` / `BudgetExceeded`），投影索引点与投影片段数（M0 共用 `max_mapping_points`）超限返回 `Err(Mapping(BudgetExceeded))`”，并注明原句曾把“映射片段”误列在 `Err` 一侧。

| 预算维度 | 代码路径 | 实际出口 | 与 §553 一致 |
|---|---|---|---|
| 映射片段数 `max_segments` | `build.rs:494-499` `check_segments` → `BuildError::BudgetExceeded`；`parse.rs:268-275` → `raw_fallback(…, DiagnosticKind::BudgetExceeded, stats)` | `Ok(Parsed)`，整源回退、不截断 | 是（归入构树类回退） |
| 源码字节 | `parse.rs:241-243` `TextBudget::new(budget.max_source_bytes())` → `ParseError::Source` | `Err(Source)` | 是 |
| 重写字节 | `parse.rs:246`；`pre/rewrite.rs:311-313` | `Err(BudgetExceeded)` | 是 |
| 投影索引点 | `mapping/projection.rs:688-695` `push_point` → `MappingError::BudgetExceeded`；`parse.rs:309-316` `.map_err(ParseError::Mapping)` | `Err(Mapping(BudgetExceeded))` | 是 |
| 投影片段数（`max_mapping_points` 兼作） | `projection.rs:251-254`；传参 `parse.rs:304-307`；语义文档 `budget.rs:62-70` | `Err(Mapping(BudgetExceeded))` | 是 |

实测佐证：

- `max_segments=0` → `Parsed`+`Raw`+1 诊断（`tests/rewrite.rs:148-162`，实跑 exit 0）；`tests/diagnostics.rs:93-108` 另固定 `RawFallback` 保护种类与 `BudgetExceeded` 诊断（定向实跑 exit 0）。
- 源码字节 `max_source_bytes=3` → `ParseError::Source(BufferError::BudgetExceeded)`（`tests/hostile.rs:196-211`，随 `--test hostile` 19 passed）。
- 重写字节 `max_rewritten_bytes=2` → `ParseError::BudgetExceeded`（`tests/hostile.rs:271-284`，同上）。
- 索引点：`"abc\n"` 4 点/2 片段，上限 3 失败、4 通过（`tests/hostile.rs:344-375`）；片段数兼作：`"[a](u)\n"` 2 点/3 片段，上限 2 失败、3 通过（`tests/hostile.rs:378-414`）。两项定向实跑均通过。

结论：更正后的分类与代码、测试逐维一致，R3-F2-1 指出的“映射片段误列 `Err` 一侧”已消除。

### 2.2 更正 2（`step-6.7/report.md:11`）——**准确**

工作区该条现表述为：“差分比较在**测试辅助**中不只比文本：…；生产路径 `reuse.rs:183-191` 的 `same_block` 只比较块种类与块文本（kind+text）。”

- 生产路径核实：`crates/aline-markdown/src/reuse.rs:183-191` 仅先比 `kind()`，再比 `block_text`（快照切片原文），无范围/片段/保护/样式比较。
- 测试辅助核实：`tests/reuse.rs:472-486` 的 `BlockSignature`（kind、text、range、segments、protected、styled）与 `SegmentSignature`（source/drawn/precision）只定义在测试文件；`assert_all_reused_pairs_match`（`:495-521`）与 `block_signature`（`:523` 起）仅在测试内使用，`rg` 未在 `crates/aline-markdown/src` 命中这些符号。

结论：更正措辞与代码一致，R3-F3-1 关闭。

## 3. 剩余问题

### R4-F1（轻微，非阻断）`step-6.8/report.md` §1 的“回填后 SHA-256”与“22 行新增”是 09:45:51Z 时点快照，未标注时点

`step-6.8/backfill-hashes.log` 记录生成时间 `2026-10-31T09:45:51Z`、`HEAD=99867bf`，此时 `git diff --stat docs/` 为 4 文件 22 行新增（engine 12、architecture 2、prd 2、validation-plan 6）。但提交 `45d4324` 实际为 22/0、6/0、4/0、12/0（合计 44 行新增、0 删除），其 blob 哈希（engine `2c9d2eab…`、architecture `5b6aa421…`、prd `e6cadea4…`、validation-plan `b6210cf7…`）与报告 §1 的“回填后”值（`e60101de…`、`0e94cca4…`、`47962a6f…`、`5ad602af…`）不同；报告中列出的这四个“回填后”哈希在 git 对象库中不存在（`git rev-list --all --objects` 无匹配）。

原因：`45d4324` 在 09:45 快照之后、提交之前又追加了 round-2 处置段落（引擎 §11.2 的两条更正记录、PRD 与验证计划的处置段），使内容与行数变化。证据日志已记录 `HEAD=99867bf` 与时点，故不构成事实错误；但报告 §1 未标“09:45:51Z 工作区快照”，按当前文件或提交比对会误判。建议在 §1 表头注明时点（不改变任何计数或结论）。

### R4-F2（轻微，非阻断）`docs/v1-validation-plan.md:97` 仍保留未限定的“差分比较加入…”

该段写“复用拒绝已扩到…，差分比较加入映射来源/精确性、保护与样式范围（`step-6.7`）”，未限定“测试辅助”。同文件其余段落已指向 `step-6.7`，而 `step-6.7/report.md:11` 与 `engine-design.md:437` 均已限定；作为 V-07 证据摘要、且链接到已更正的步骤报告，判为可接受。若要求全仓措辞一致，可补“（测试辅助）”。

### R4-F3（轻微，非阻断）`group-6/follow-ups.md:15` 的 F2-2 处置措辞同样未限定

该行描述“差分比较加入映射来源/精确性、保护范围与可比的导航目标”，属 round-2 的处置计划记录（当时要求测试辅助覆盖），不是当前生产行为声明，保留可接受；无需改动。

### R4-F4（观察）`step-6.8/report.md` §4 的“最终计数 204”与 10:40 门禁运行 205 的时点差异

报告写“round-2 复核处置后（10:19Z–10:34Z）的最终计数…工作区 `cargo test --workspace` 为 204 passed”。只读重算：`leader-verify-round-2/workspace-gates/test.stdout.log`（10:27:39Z–10:28:17Z）= **204**；同目录 `workspace-gates-final/test.stdout.log`（10:40:20Z–10:40:49Z）= **205**（html 21 对 20，即 N-F3-2 用例补入后的差异）。报告引用的 204 与其声明窗口吻合，10:40 运行在窗口外；两次均 19/19 exit 0。不构成错误，“最终”一词易被读成 205，建议注明“10:27 窗口运行”。

### 残留检索结论

- “映射片段…返回 `Err`”类**无现存未限定副本**：仅出现在 `review-round-3.md`（描述被指出的错误本身）与 `git-verification.log`（diff 中被删除的旧行）。工作区 `v1-architecture.md:553` 已改正。
- “差分比较不只比文本”类未限定副本：`docs/v1-validation-plan.md:97`（R4-F2）与 `group-6/follow-ups.md:15`（R4-F3），均在证据/计划语境且指向已更正的步骤报告；`engine-design.md:437` 的未限定句后紧跟同一段内的更正，不构成误导。

## 4. 任务 6.8 复判

任务原文（`openspec/changes/m0-engine-foundation/tasks.md:66`，仍未勾选）：“6.8 回填引擎 §6/7/11/12、解析配置及 V-06/V-07 基础夹具；将 SoftBreak、有限 HTML 尚未固化金样逐项列实验结果/缺口，不变更 MD 口径或宣称公式图表能力/AC-09 完成。”

| 要求 | 落地证据 | 判定 |
|---|---|---|
| 引擎 §6/7/11/12 回填 | `engine-design.md:179`（§6.2）、`:220`（§7.1）、`:413`/`:415`/`:417`（§11.2）、`:437`（§12.1）、`:469`（§12.3），均带 `2026-10-31` 日期与步骤报告链接 | 满足 |
| 解析配置 | §6.2：六位 `0x00000c1e`、代次 1、`default-features=false`、Front matter 由前置改写识别、未启用原生 metadata、smart punctuation/heading attributes 关闭，且明确“不改变上面的‘建议配置’措辞” | 满足 |
| V-06 基础夹具 | `v1-validation-plan.md:85`：八维预算、取消、扫描步数 80 交叉验证、不可中断盲区；保留“V-06 仍未通过” | 满足（基础段，非通过） |
| V-07 基础夹具 | `v1-validation-plan.md:97`/`:99`/`:101`：映射金样、保护、复用拒绝、HTML 修复与未冻结项；保留“V-07 仍未通过” | 满足（基础段，非通过） |
| SoftBreak 缺口逐项 | §7.1:220、§11.2:413、验证计划 :97/:99、PRD:402 均列为“带来源换行、待 V-07 冻结” | 满足 |
| 有限 HTML 未固化金样逐项 | §11.2:413 列五项（过滤集补全、空白折叠、块级合成换行、实体/零可见金样、void 展示口径）；:415 记已落地项，:417 记 round-2 更正与仍待冻结项（`<foo>` 不对称现状、`input/link/meta/base` 口径） | 满足 |
| 不变更 MD 口径 | `git show 45d4324 --numstat -- docs/` 为 22/0、6/0、4/0、12/0；对 `docs/` 的删除行检索为空（纯追加）。PRD/验证计划的 AC、V 状态句未改写，仅追加带日期段落 | 满足 |
| 不宣称公式/图表能力或 AC-09 | PRD:402 “AC-09 未通过，公式/图表也仅识别来源、不引渲染后端”；§6.2 “没有为公式/Mermaid 引入渲染后端”；全仓无新增“AC-09/V-06/V-07 通过”表述（`claims-check.log`） | 满足 |

**判定：6.8 通过（M0 范围）。**

R3-F2-1 的关闭依据：工作区 §553 已把 `max_segments` 移入构树类整源回退一侧，并把“投影片段数”与 `max_mapping_points` 合并表述；代码（`build.rs:494-499`+`parse.rs:268-275`；`projection.rs:251-254`/`688-695`+`parse.rs:309-316`）、测试（`tests/rewrite.rs:148-162`、`tests/diagnostics.rs:93-108`、`tests/hostile.rs:344-414`）逐维吻合，定向实跑 exit 0；残留检索无未限定“映射片段→`Err`”副本。该关闭以工作区文本为据，HEAD `1dd0174` 的 §553 仍是旧文本，需随更正一起提交后才在提交历史中生效。

6.8 的通过只表示“回填与缺口清单已按任务原文落地、边界未越界”，不表示 V-06/V-07/AC-09 通过，也不表示 M0 出口通过。

## 5. 未覆盖与未执行

- 未重跑 19 项工作区门禁、`cargo clippy --workspace --all-targets -D warnings`、`cargo fmt --all --check`、`cargo test --workspace`；只读核对了既有 `summary.json` 与日志计数（三次运行均 19/19 exit 0；`cargo test --workspace` 分别为 134、187、204、205），不对本时点的门禁结果作新判断（工作区有组 7/组 8 并发改动）。
- 未复跑 `dump_tree` 示例、扫描步数探针、`step-6.1`–`step-6.8` 历史日志；历史计数只按报告与日志引用。
- 未执行链接检查器（round-3 已做）；本轮只做文本级 `rg`。
- 未执行 V-03、V-05、V-06、V-07、AC-09 验收；未验证真实 UI 命中、IME、灰色只读呈现、Windows/Linux/CI。
- 未评价组 7、组 8 的任何文件或结论；未评价本轮未涉及的其他组 6 代码路径（如 `aline-edit` 的复读）。

## 6. 逐任务结论（M0 范围）

| 任务 | 本轮结论 | 依据 |
|---|---|---|
| 6.1–6.7 | **沿用 round-3 原结论，本轮不重判** | 本轮只核验 R3-F2-1/R3-F3-1 涉及的两处文档与对应代码；全 crate 实跑 108 passed 无回归 |
| 6.7 | 原“通过（M0 范围）”维持；R3-F3-1 已关闭 | `reuse.rs:183-191` 与 `tests/reuse.rs` 测试辅助逐维比较经只读核实；`reuse` 18 passed |
| 6.8 | **通过（M0 范围）**（round-3 的“部分成立”解除） | 上表 8 项要求逐项满足；R3-F2-1 关闭；回填纯追加、无口径变更、无越界宣称 |

组级判断：round-3 的唯一重要发现 R3-F2-1 已关闭，R3-F3-1 已关闭；无新的阻断项。R4-F1～R4-F4 均为时点标注/措辞一致性的轻微观察，不影响 6.8 的 M0 范围判定。

## 7. 不宣称

- 不宣称 V-03、V-05、V-06、V-07 通过；不宣称 AC-09 通过。
- 不宣称 M0 出口通过，也不宣称 M1 可以开始。
- 不宣称组 7、组 8 状态，不评价其实现或文档。
- 不宣称本报告替代调度者处置；不勾选 `tasks.md`，不提交、不 `git add`、不 push。
- 不宣称本轮实跑结果代表跨平台或 CI 结果；平台仅 macOS arm64。
