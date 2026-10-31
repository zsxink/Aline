# 组 7（aline-edit）round-4 独立复核

日期：2026-10-31（UTC）。复核窗口：`2026-10-31T12:56:11Z`–`2026-10-31T13:05:30Z`。基线 HEAD：`1dd0174ca8fc4aa975209ee5d6c3169c36695fad`（`main`）。

本报告只判定工作区内 4 处文档更正的准确性，并复判任务 7.4。结论依据为代码、测试、日志与 git 对象，不以报告或更正文字自证。本次复核只读：未改动源码/测试/期望/既有报告/`tasks.md`，未 `git add`、未提交、未 push；只新建本报告与 `review-round-4-evidence/` 下的日志。工作区内并发的组 8 改动（`crates/aline-ui/src/document/`、`group-8/`）与组 6 更正（`docs/v1-architecture.md`、`scripts/check_safety.py`、`group-6/step-6.7/report.md`）未评价、未修改。

## 1. 方法与命令

| 类别 | 内容 |
|---|---|
| 实跑 | `cargo test -p aline-edit --locked --offline --test refusals`（16 passed，exit 0） |
| 实跑 | `cargo test -p aline-edit --locked --offline --test goldens`（6 passed，exit 0） |
| 实跑 | `cargo test -p aline-edit --locked --offline`（26 passed = 0+6+4+16，doc-tests 0，exit 0） |
| 只读核对 | `git log --all / ls-files --stage / hash-object / show / ls-tree / cat-file -s`、`shasum -a 256`、`git status --porcelain`、`stat` |
| 只读核对 | `summary.json` 的 19 项计数、退出码、最早 `started_utc`、最晚 `finished_utc` |
| 只读核对 | `rg`/`sed` 读 `crates/aline-edit/tests/refusals.rs:674-720`、`crates/aline-ui/src/document/doc_core.rs`、4 份回填文档、`specs/engine/edit/spec.md`、`tasks.md`、`architecture-guide.md` |
| 只读核对 | 残留文本全局搜索：`10:04:56`、`从未进入提交`、`当前不可复现` |
| 未执行 | 19 项工作区门禁复跑、`cargo clippy`/`fmt`、`cargo test --workspace`、真实 UI/DocCore/IME、Windows/Linux；`summary.json` 的历史时点不可由 git 重建 |

环境：`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`，cargo 一律 `--locked --offline`。

命令与退出码：`review-round-4-evidence/commands.log`；git 与 summary.json 原始输出：`git-verification.log`、`summary-verification.log`；测试日志：`cargo-test-refusals.stdout.log`、`cargo-test-goldens.stdout.log`、`cargo-test-full.stdout.log`；扫描与残留搜索：`write-channel-and-code-scan.log`、`workspace-write-channel-at-head.log`、`residual-search.log`。

## 2. 逐项更正判定

| # | 位置 | 更正内容 | 判定 | 依据方式与证据 |
|---|---|---|---|---|
| 1 | `step-7.1/report.md:118` | 门禁结束时间改为 `10:04:27Z–10:04:53.722Z`，括注 `summary.json` 最晚 `finished_utc` `10:04:53.722236Z` | **准确** | 只读核对：`summary.json` 恰 19 项、`passed=true`、全部 `exit_code=0`；最早 `started_utc=10:04:27.889574Z`、最晚 `finished_utc=10:04:53.722236Z`，与行内两个端点一致。同一行 `195 passed` 由该目录 `test.stdout.log` 的 40 个 `test result` 行求和复核（151 属 09:21 首轮，另见 `step-7.4:120` 的说明，与 195 不冲突） |
| 2 | `step-7.1/report.md:16` | `git log --all -- <golden>` 改为“复核时点为空（当时金样尚未进入提交）；证据提交 `55bd376` 后已跟踪，blob `2d3703f…`、内容 sha256 未变” | **准确** | 只读核对 git 对象：`git log --all` 现返回 `55bd376`；`git ls-files --stage` 显示 `100644 2d3703f42f89f64b667321b4ced93ac216111e5b 0`；`git hash-object` 与暂存 blob 相同；`shasum -a 256` = `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743`；`git show 55bd376 --stat` 显示该文件 461 insertions；`git ls-tree 55bd376` 为同一 blob；`cat-file -s` 20126 字节 |
| 3 | `step-7.1/report.md:24` | 索引状态补“`55bd376` 后重新跟踪、porcelain 干净、内容 sha256 `cf2504f2…` 未变，不是回填” | **准确** | 只读核对：`git status --porcelain -- <golden>` 为空；blob 仍为 `2d3703f42f89f64b667321b4ced93ac216111e5b`；`shasum -a 256` 仍为 `cf2504f2…`；`stat` epoch `1793437725` = `2026-10-31T09:08:45Z`，与首次暂存时点未变。行内“10:40:18Z 至 `55bd376` 之间的索引变化由调度者提交证据造成；此前变化的责任方无法确定”是证据可判定的边界，未越界断言 |
| 4 | `step-7.4/report.md:149` | N-F2-1 断言强度改为“绘制文本 `"ab"`；`blocks().len()` 2→1（`tests/refusals.rs:716-717`）；块种类只在注释中记录，未被断言” | **准确** | 只读核对源码：`tests/refusals.rs:674-720` 的 `pure_whitespace_hidden_separator_is_accepted_limitation` 循环两例（`"a\n\nb\n"` 删 1..3、`"- a\n\nb\n"` 删 3..5）；断言为 `before_drawn == "ab"`、`preview().drawn_text() == "ab"`、`preview().after_len()`、`capture(&buffer)? == before`、`parsed.blocks().len() == 2`、`candidate_parsed.blocks().len() == 1`。块种类 `[Paragraph, Paragraph]`→`[Paragraph]`、`[List, Paragraph]`→`[List]` 仅出现在第 677–678 行注释。实跑 `--test refusals` 16 passed，含该用例 |

四项更正均与代码、测试、git 对象、`summary.json` 一致，**无一夸大或与事实相反**。

## 3. 剩余问题

### R4-F1（轻微，文档时效）`step-7.4/report.md` §3 表格“workspace 生产代码实际写入入口”现已在 HEAD 失效

该行为“无匹配”，其扫描时点是 `09:23:06Z`（§3 标题已注明）。但组 8 的 `crates/aline-ui/src/document/doc_core.rs` 在 `96c6573`（`2026-10-31T12:45:40Z`）落地，HEAD `1dd0174` 上实测有 3 处 `.apply(`（`doc_core.rs:474/904/973`）、1 处 `.undo`（`:658`）、1 处 `.redo`（`:702`）、1 处 `.reload`（`:728`）。因此“workspace 生产代码实际写入入口 = 无匹配”只在扫描时点成立，在 HEAD 不再成立。

这**不推翻任务 7.4 的实质结论**：任务要求的是“DocCore 以外无实际写入通道”，而 HEAD 上的全部 buffer 变更原语都在 `doc_core.rs` 内，`crates/*/src` 与 `app/src` 的其余部分（`aline-edit`、`aline-buffer`、`aline-markdown`、`aline-app-core`、`aline-plugin`、`app`）均无 `.apply/.undo/.redo/.reload`（`workspace-write-channel-at-head.log`）。文件系统写入仍只在 `crates/aline-source/src/atomic.rs`。建议最小更正：把该行改为“扫描时点（09:23:06Z）无匹配；HEAD 上的 buffer 写入入口只有第 8 组 `DocCore`（`.apply/.undo/.redo/.reload`），DocCore 以外无写入通道”，或为整行加时间作用域。属文档时效问题，不阻断 7.4。

### R4-F2（纯格式）`step-7.4/report.md` 章节号从 §7 跳到 §9

该文件标题序列为 `1–7` 后直接 `## 9. round-2 复核新发现…`，无 §8。内容完整、无缺失章节，仅编号不连续。不影响任何判定；若日后回填可顺手改为 §8 或续编。

### 残留错误搜索结论

- `10:04:56` 仅出现在 `step-7.4/report.md:114`（作为更正说明引用旧值）与 `review-round-2.md`/`review-round-3.md`（历史记录）。执行文档中无未加限定的残留。
- `从未进入提交` 仅出现在 `review-round-3.md`（历史复核记录），执行文档已无该句。
- `当前不可复现` 出现在 `review-round-2.md`/`review-round-3.md`（历史记录）与 `step-7.1/report.md:24`，后者已明确限定为“当时……描述成立”并补出提交后的当前状态，属合规表述。
- 未发现任何执行文档仍以未限定口吻夸大 N-F2-1 断言强度。

按任务口径，`review-round-2/3` 保留原文属历史记录，允许。

## 4. 未覆盖与未执行

- 未复跑 19 项工作区门禁、`cargo clippy`、`cargo fmt --check`、`cargo test --workspace`；只读取 `leader-verify-post-fix/workspace-gates/summary.json` 与其中 19 项退出码，不据本轮为门禁背书。
- `summary.json` 的 `10:04:27Z`–`10:04:53Z` 是历史时点，无法由 git 重建；只核对了文件内字段自洽与 195 passed 求和。
- 未验证真实 UI/DocCore 消费者的命令入口、IME、Windows/Linux 与其他 target；未执行 V-07、AC-04 验收活动。
- 未逐字节复核 `edit-goldens.tsv` 的 30 个用例内容；只核对内容 SHA-256 未变、blob 未变、`goldens` 6 passed 与 `allow_source_footnote` 勘误仍在（`goldens.rs` 先断言 `7..7` 再返回 `8..8`）。
- 未评价组 8、组 6 的任何文件与结论；`doc_core.rs` 只用于核对“DocCore 以外无写入通道”。

## 5. 7.4 复判

任务 7.4 要求（`openspec/changes/m0-engine-foundation/tasks.md:75`）：回填引擎 §8/9/14、架构 §7.1/7.2、V-07/AC-04 基础覆盖及未覆盖列表/表格/格式化命令；人工审查 DocCore 以外无实际写入通道；记录 R/S 与场景覆盖。

| 要求 | 证据（本轮实测） | 判定 |
|---|---|---|
| 引擎 §8/9/14 回填 | `docs/engine-design.md` 组 7 记录在 `286`（§8.3）、`310`（§9.2）、`334`（§9.4）、`527`（§14）；均由 `45d4324` 提交（`git show 45d4324 --stat -- docs/` = 44 insertions：engine 22、architecture 6、PRD 4、validation-plan 12） | 成立 |
| 架构 §7.1/7.2 回填 | `docs/v1-architecture.md:555`（§7.1 组 7 后状态）、`:569`（§7.2 六步链条至第五步、第六步归 DocCore） | 成立 |
| V-07/AC-04 基础覆盖 | `docs/v1-validation-plan.md:105`（组 7 段，明确 V-07 未通过）；`docs/v1-prd.md:404`（AC-04 基础贡献，明确 AC-04 未通过） | 成立 |
| 未覆盖列表/表格/格式化命令 | 引擎 §8.3/§9.2/§9.4/§14 与 `step-7.4` §7 均列“列表/表格/任务框/代码语言/全文格式化未实现”；`validate.rs`/`plan.rs` 实际只有 `Replace` 与 `ToggleInlineFormat`（`lib.rs` 导出核对） | 成立 |
| 人工审查 DocCore 以外无实际写入通道 | `step-7.4` §3 表 + `run-write-channel-review.sh` 可复现；本轮 HEAD 扫描：`aline-edit/src` 无 `.apply/.reload/.undo/.redo`（rg 退出 1），`CheckedBatch::new` 仅 `plan.rs:242/330`；HEAD 的 buffer 写入入口全部在 `crates/aline-ui/src/document/doc_core.rs`；fs 写入仅 `aline-source/src/atomic.rs`。表内“无匹配”一行有时效问题，见 R4-F1 | 成立（实质结论），文字待限定 |
| R/S 覆盖记录 | `step-7.4` §4 共 17 个 ID（R-4-01/02/03、R-5-01/02/03/07/08、R-3-01/07、R-12-01/02、R-16-01/02/04、K-02/03），全部在 `docs/architecture-guide.md` 存在；代码断言（`ChangePlan` 绑定字段、`check_visual_permissions` 调用点、`EditRefusal` 类型、`EditBudget`、依赖仅 buffer+markdown）与本轮只读核对一致 | 成立 |
| 场景覆盖记录 | `step-7.4` §5 的 2 个 ADDED Requirement、6 个 Scenario 与 `openspec/changes/m0-engine-foundation/specs/engine/edit/spec.md` 完全对应（变更计划可验证 3 例、拒绝完整状态不变 3 例） | 成立 |

**7.4 判定：通过（M0 范围）。** 无阻断项。四项更正全部准确；仅余 R4-F1（§3 表一行的时间作用域）与 R4-F2（章节编号）两处非阻断文档问题，建议先改文字再考虑勾选 `tasks.md`，但不影响实质验收。

补充事实（供后续组参考，不属 7.4 缺陷）：HEAD 上 `DocCore` 的提交路径直接调用 `StringBuffer::apply`，不经过 `plan_change`/`validate_change`（`rg plan_change|validate_change crates/aline-ui/src` 退出 1）。组 8 `README.md:43` 已声明“本组没有 DocCore 命令入口”，故这与 7.4 所记“端到端未验证”“apply_change 单一写入口归第 5/8 组”一致，不构成 7.4 的新缺口。

## 6. 不宣称

- 不宣称 V-07、AC-04 通过；不宣称 M0 出口通过，也不宣称 M1 可以开始。
- 不宣称 19 项门禁、clippy、fmt、`cargo test --workspace` 在本轮通过；只对 `aline-edit` 的 refusals/goldens/全 crate 实跑结果负责。
- 不宣称组 8、组 6 的任何状态或结论；不评价其实现与文档。
- 不宣称本报告替代调度者处置；不勾选 `tasks.md`，不 `git add`、不提交、不 push。
- `tasks.md:75` 的 7.4 复选框仍为未勾选；本报告不改变其状态。
