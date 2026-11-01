# 组 7（aline-edit）round-3 独立复核

日期：2026-10-31（UTC）。复核窗口：`2026-10-31T11:12:17Z`–`2026-10-31T11:17:59Z`。基线 HEAD：`7916cea59247d4678765c8e66f69e9f33f680e6e`（`main`）。

本报告只判定 `review-round-2.md` 中组 7 发现的关闭状态，并给出组 7 逐任务结论。结论依据为代码、测试、日志与 git 对象，不以报告文字自证。工作区内并发的组 8 改动（`crates/aline-ui/src/document/`、`crates/aline-ui/tests/`、`docs/validation/m0-engine-foundation/group-8/`）未评价、未修改、未提交。本次复核只读：未改动源码/测试/既有证据，未提交，未勾选 `tasks.md`。

## 1. 方法与命令

| 类别 | 内容 |
|---|---|
| 实跑 | `cargo test -p aline-edit --locked --offline --test refusals`（16 passed，exit 0） |
| 实跑 | `cargo test -p aline-edit --locked --offline refusal`（字面命令：refusals 目标 7 passed/9 filtered，goldens 目标 1 passed/5 filtered，exit 0） |
| 实跑 | `cargo test -p aline-edit --locked --offline`（26 passed，exit 0；goldens 6、protection 4、refusals 16） |
| 实跑 | 独立重跑 `step-7.4/fix-f3-3-f3-6/run-write-channel-review.sh`（11:15:39Z，exit 0；脚本内部各 `rg` 退出码见日志） |
| 只读核对 | `git rev-parse/log/cat-file/ls-tree/ls-files/shasum`；`summary.json` 字段；`rg`/`sed` 读 `plan.rs`、`refusals.rs`、报告与金样 |
| 未执行 | 19 项工作区门禁复跑、`cargo clippy`/`fmt`、`cargo test --workspace`、真实 UI/DocCore/IME、Windows/Linux；10:38:13Z 时点的哈希无法从 git 重建 |

命令与退出码：`docs/validation/m0-engine-foundation/group-7/review-round-3-evidence/commands.log`；git 与 summary.json 原始输出同目录 `git-verification.log`；测试与重跑日志同目录 `cargo-test-*.log`、`write-channel-review-rerun.log`。环境：`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。

## 2. 逐项判定

| ID | 判定 | 依据方式 | 证据指针（代码/日志/git） |
|---|---|---|---|
| N-F1-1 拒绝用例计数 | **关闭** | 实跑 + 只读核对 | `step-7.2/report.md:3` 首段写 **16 例**并注明 7→15→16；`--test refusals` 实跑 16 passed；`tests/refusals.rs:674` 起的 N-F2-1 用例计入其中 |
| N-F1-2 行号 | **关闭** | 实跑 + 只读核对 | 重跑写入通道脚本输出 `CheckedBatch::new` 仅在 `crates/aline-edit/src/plan.rs:242` 与 `:330`；`step-7.4/report.md` §3 表格已同步并注明 09:23 扫描时为 235/323 |
| N-F1-3 38 行归因 | **关闭** | 只读核对 | `git show 45d4324 -- docs/` 实测 44 insertions（engine 22、architecture 6、PRD 4、validation-plan 12）；其中组 7 记录 8 条（引擎 4/架构 2/PRD 1/验证计划 1），连同分隔空行为 16 行；其余 28 行为组 6，与 `step-7.4/report.md` §1 的 16+28 更正一致 |
| N-F1-4 mtime 作用域 | **关闭** | 只读核对 | `step-7.1/report.md` §1 两行 mtime 标注“修复前（09:23:06Z 扫描）”，并给出修复后值（`plan.rs` 09:58:27Z 等）；原表保留为快照 |
| N-F2-1 纯空白隐藏片段限制 | **关闭** | 实跑 + 只读核对 | `tests/refusals.rs:674-720`：两例（`"a\n\nb\n"` 删 1..3、`"- a\n\nb\n"` 删 3..5）断言前后绘制文本 `"ab"`、缓冲不变、候选块数 1 vs 原 2；注释明确标为 V-07 已接受限制而非“完整保留”；`--test refusals` 16 passed。报告措辞略有夸大，见 R3-F3-3 |
| N-F3-1 哈希漂移 | **关闭** | 只读核对 | `step-7.4/report.md` §1 已加“截至 2026-10-31T10:11:09Z”作用域与 10:38:13Z 漂移记录，并保留原表。当前 `45d4324` 的哈希为 engine `2c9d2eab…`、architecture `5b6aa421…`、PRD `e6cadea4…`、validation-plan `b6210cf7…`；architecture 与 PRD 等于报告中 10:38 漂移值，engine/validation 与漂移值不同，与“其后组 6 继续写入”一致。10:38:13Z 的工作区态不可由 git 重建，只能核对逻辑一致性 |
| N-F3-2 门禁结束时间 | **部分关闭** | 只读核对 | `summary.json`：19 项、全部 exit 0、最早 `10:04:27.889574Z`、最晚 `10:04:53.722236Z`；`step-7.4/report.md:112` 已更正为 `10:04:27Z–10:04:53.722Z` 并在 §6 说明 `commands.log` 未登记该命令。但 `step-7.1/report.md:118` 仍写 `10:04:27Z–10:04:56Z`，见 R3-F3-1 |
| N-F3-3 工作区未冻结 | **关闭（作为已记录环境限制）** | 只读核对 | 当前工作区确有组 8 未提交改动（`git status` 快照与本轮窗口一致）；M0 门禁只能在本机并发工作区运行，报告已如实记录“目标图解析不等于跨平台构建”、未称冻结。无代码处置空间 |
| N-F3-4 金样索引状态 | **关闭（更正与当时事实一致）** | 只读核对 | 金样内容 SHA-256 仍为 `cf2504f2…`；`55bd376` 已将其以 blob `2d3703f…` 提交，当前 `git ls-files --stage` 可查到且 porcelain 干净。这使 `step-7.1/report.md:16` “从未进入提交”与 `:24` “当前不可复现”两句在提交后失真，见 R3-F3-2 |
| F3-3 写入通道审查可复现 | **关闭** | 实跑 | 独立重跑 `run-write-channel-review.sh`（11:15:39Z，exit 0）：`&mut StringBuffer|StringBuffer::apply|.apply(|.reload(|.undo(|.redo(` 在 `aline-edit/src` 无匹配（rg 退出 1）；`CheckedBatch::new` 仅 `plan.rs:242`/`:330`；四个 crate 的 `src` 无 `std::fs`/`OpenOptions`/`fs::write`/`File::create`；依赖仅 `aline-buffer`、`aline-markdown`（`Cargo.toml:11-12`）。与 `step-7.4` §3 表和首次回放日志一致 |
| F3-6 字面命令 | **关闭** | 实跑 + 只读核对 | `step-7.4/fix-f3-3-f3-6/commands.log:4` 记录 `2026-10-31T10:55:15Z … 0 cargo test -p aline-edit --locked --offline refusal`；本轮复跑同一字面命令：refusals 目标 7 passed/9 filtered、goldens 目标 1 passed/5 filtered，exit 0；`step-7.2/report.md` §5 已说明名字过滤弱于 `--test refusals` 的 16 例 |
| F3-8 证据已提交 | **关闭** | 只读核对 | `a476a94` 为实现提交、`55bd376` 含 group-7 证据、`45d4324` 为文档回填，均 `git cat-file -t`=`commit`；`git ls-tree 55bd376` 可查 `review-round-2.md`、`edit-goldens.tsv`（blob `2d3703f…`）、`fix-f3-3-f3-6/commands.log`；提交树无 `.codegraph/` |

## 3. 新发现

### R3-F3-1（轻微）`step-7.1/report.md:118` 仍使用错误门禁结束时间

该行仍写 `2026-10-31T10:04:27Z–10:04:56Z`。`summary.json` 最晚 `finished_utc` 为 `10:04:53.722236Z`，且 `step-7.4/report.md:114` 已按同一文件更正为 `10:04:53.722Z`。`step-7.2/report.md` §6 又指向 `step-7.1/report.md` 第 7.2 节，因此错误时间仍在被引用的位置。最小更正：把该行改为 `10:04:27Z–10:04:53.722Z`（或整行引用 `summary.json`）。

### R3-F3-2（轻微）`step-7.1/report.md:16`/`:24` 的“从未进入提交 / 当前不可复现”在证据提交后失真

`55bd376` 已将 `edit-goldens.tsv` 以 blob `2d3703f42f89f64b667321b4ced93ac216111e5b` 提交；本轮 `git log --all -- <golden>` 返回 `55bd376`，`git ls-files --stage` 也重新可见该 blob。金样内容 SHA-256 仍为 `cf2504f2…`、mtime 未变，因此“非回填”的结论仍成立；但第 16 行“空（金样从未进入提交）”与第 24 行“当前不可复现”已不再是当前事实。最小更正：注明“证据提交前为空/不可复现；`55bd376` 之后的当前状态为已跟踪，blob 与暂存时相同”。该变化由调度者提交证据造成，不影响 7.1 的金样先行论证。

### R3-F3-3（轻微）`step-7.4/report.md:149` 夸大 N-F2-1 用例的断言强度

该行称测试“断言其块结构后果：`a\n\nb\n` 删 1..3 → `[Paragraph, Paragraph]` 变 `[Paragraph]`；`- a\n\nb\n` 删 3..5 → `[List, Paragraph]` 变 `[List]`”。实际 `tests/refusals.rs:716-717` 只断言 `blocks().len()` 从 2 变 1；块种类 `[Paragraph, Paragraph]`/`[List, Paragraph]`/`[List]` 只出现在用例开头的注释里，未被断言。N-F2-1 的实质（钉住“2 块变 1 块”的后果并标为已接受限制）成立，但报告“断言”一词应改为“记录/注释给出种类后果，测试断言块数 2→1”，或补上种类断言。

## 4. 未覆盖与未执行

- 未复跑 19 项工作区门禁、`cargo clippy`、`cargo fmt --check`、`cargo test --workspace`；只读取 `leader-verify-post-fix/workspace-gates/summary.json` 与其中退出码，不据本轮重跑为门禁背书。
- 10:38:13Z 的文档哈希是历史工作区态，无法由 git 重建；只核对了“architecture/PRD 与最终提交一致、engine/validation 其后又变化”的逻辑一致性。
- 未验证真实 UI/DocCore 消费者、“所有已开放写入口”端到端、IME、Windows/Linux 与其他 target；未执行 V-07、AC-04 的验收活动。
- 未评价组 8 的任何文件与结论。
- 本轮未逐字节复核 `step-7.1/goldens/edit-goldens.tsv` 的 30 个用例内容；只核对了内容哈希未变与 `goldens` 6 passed。

## 5. 逐任务结论（M0 范围）

| 任务 | 结论 | 依据 |
|---|---|---|
| 7.1 金样先行 + 纯 plan/validate + 候选复读 | **通过（M0 范围）** | `goldens` 6 passed；金样 30 例、内容 SHA-256 `cf2504f2…` 未变；金样先行的证据强度限制已如实记录；仅余 R3-F3-2 的提交后表述失真 |
| 7.2 拒绝矩阵与完整状态不变 | **通过（M0 范围）** | `refusals` 16 passed；字面命令补跑 7 passed/9 filtered + goldens 1 passed/5 filtered；隐藏链接目标最小反例在 `refuse_hidden_link_target_lost`；状态不变用例覆盖文本/选区/revision/undo/redo/历史节点数 |
| 7.3 保护、HTML 边界与 Source 放行 | **通过（M0 范围）** | `protection` 4 passed；`refusals` 中 `protected_range_refusal_keeps_buffer_state`、`html_boundary_refusal_keeps_buffer_state` 等实跑通过；检查顺序与 `validate_change` 的描述一致（只读核对） |
| 7.4 文档回填、写入通道、R/S 与场景覆盖 | **通过（M0 范围）** | 引擎 §8/9/14、架构 §7.1/7.2、PRD、验证计划的组 7 记录均在 `45d4324`；写入通道脚本可复现（F3-3 关闭）；字面命令已补跑（F3-6 关闭）；R/S 与 spec 场景覆盖表存在。存在 3 处 R3-F3 文档更正，建议先修正文字再考虑勾选 `tasks.md`；不影响实质验收 |

组级判断：round-2 的实质缺口（F3-3、F3-6、N-F2-1、F3-8）均已关闭；N-F3-2 仅部分关闭（`step-7.1:118` 残留），其余为轻微的提交后表述失真与断言措辞夸大。无 R3-F1 阻断项。

## 6. 不宣称

- 不宣称 V-07、AC-04 通过；不宣称 M0 出口通过，也不宣称 M1 可以开始。
- 不宣称 AC-09 或任何 V-03/V-05/V-06 结论。
- 不宣称组 8 状态、不评价其任何实现或文档。
- 不宣称本报告替代调度者处置；不勾选 `tasks.md`，不提交、不 `git add`、不 push。
- `tasks.md` 中 7.4 仍未勾选；本报告不改变复选框状态。
