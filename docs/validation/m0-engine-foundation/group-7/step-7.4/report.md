# 7.4 记录：文档回填、写入通道审查与 R/S、场景覆盖

日期：2026-10-31。状态：4 份文档已追加组 7 记录（组 7 自身 8 条记录、占 16 行含分隔空行；同一时点 `git diff --stat docs/` 合计 38 行，其余 22 行为组 6 并发追加——见 §1 的更正）；统一工作区门禁 19/19 退出 0，`cargo test --workspace` 195 passed。本文记录回填内容、写入通道审查、R/S 与 spec 场景覆盖，不宣称 V-07、AC-04 或 M0 出口通过。

## 1. 回填结果

回填前 SHA-256（`step-7.4/backfill-hashes-before.log`，回填开始前实测）：

| 文档 | 回填前 SHA-256 | 追加位置 |
|---|---|---|
| `docs/engine-design.md` | `e60101de16e81ed0e2dd747faaaf8bc99634efa835e1eb89446d8b6f99057627` | §8.3 末（命令拒绝零副作用后）、§9.2 末（表格结构命令后）、§9.4 组 5 记录后、§14 反退化段后 |
| `docs/v1-architecture.md` | `0e94cca4cf19a19f4a3643ff88e820b42a7aeefa4940c659c3adafd503895637` | §7.1 组 6 补充后、§7.2 格式化段后 |
| `docs/v1-prd.md` | `47962a6fff1051c61fb00f0c6618e5c205c4ab6da397856406949154d5e08e34` | §9 M0 表后（组 6 记录后） |
| `docs/v1-validation-plan.md` | `5ad602af9c70c5dcd0766dc79f3fc480e9fd34975cad426480a726cafaaaa09b` | V-07 组 6 修复段后 |

回填后 SHA-256（`step-7.4/backfill-hashes-after.log`）：

| 文档 | 回填后 SHA-256（作用域：截至 2026-10-31T10:11:09Z） |
|---|---|
| `docs/engine-design.md` | `7211b27750393f39f71e310f0e9c588cd9aaf17e7e9de3ce318f6c504818f107` |
| `docs/v1-architecture.md` | `0b6c5cbbf820a7431712cee9c8a6fc3ccee3ae4c6e1bdd8c4d5a70a56e9b1d14` |
| `docs/v1-prd.md` | `e6cadea4263ea08eaaaf1c3e4f0a5195ca785a9901aa8fcec1a683860fbfb8f4` |
| `docs/v1-validation-plan.md` | `762a1010954fe0202cd96b565cdbdd0bac407bf9ce46242104e23cfc8d604851` |

**更正（复核 N-F3-1，哈希漂移）**：上表 4 个哈希只对 10:11:09Z 时点成立。此后组 6 继续写入 `docs/engine-design.md`、`docs/v1-architecture.md`、`docs/v1-validation-plan.md`，复核者于 10:38:13Z 实测三者已分别漂移为 `7211b277…`→`15a54523…`、`0b6c5cbb…`→`5b6aa421…`、`762a1010…`→`1ca293ec…`（`docs/v1-prd.md` 仍一致）。漂移不影响本组的两个结论：组 7 记录恒为 16 行、`git diff docs/` 删除行为 0。原表保留为当时点快照，不改写。

`git diff --stat docs/`：4 files changed, **38 insertions(+), 0 deletions**（2026-10-31T10:11:09Z 时点；其中组 7 记录为引擎 4 条/架构 2 条/PRD 1 条/验证计划 1 条，共 8 条记录、16 行含分隔空行，其余 22 行为组 6 并发追加）。**更正（复核 N-F1-3）**：本报告原先把这 38 行整体归为组 7 回填，属指标归因错误；组 7 自身贡献恒为 16 行（最终 44 行快照中仍为 16 行），组 6 贡献 22→28 行。回填只追加带日期的记录段，未改写任何既有规范句；`git diff --check docs/` 退出 0。

过程中曾把 §9.2 的一句规范文字（“允许按已冻结风格调整相应表格源码”）误改为“相应源码”，随后立即还原为原句，因此最终 diff 中该行为 0 删除。这次误改与还原都发生在同一次回填内，未进入任何提交。

## 2. 回填边界

- 引擎 §8.3：记录 `plan_change`/`validate_change` 的实际检查顺序与“结构上无副作用”，并明确不包含 UI/DocCore、IME、块分组、自动保存与跨平台。
- 引擎 §9.2：记录 `ChangePlan` 绑定字段、三份复读契约、隐藏语义保留内部空白、**纯空白片段与片段边界不比较**这一已接受限制、只实现 `Replace`/`ToggleInlineFormat`、Visual `Replace` 的范围权威规则。
- 引擎 §9.4：记录“替换前重验版本与命中文本”的最小支撑，并明确查找/全部替换/全文格式化未实现。
- 引擎 §14：记录 E-T09/E-T10/E-T11/E-T12 的 M0 基础证据与未覆盖项，并写明不构成 V-07 通过。
- 架构 §7.1：`plan_change`/`validate_change` 行补一条“组 7 后状态”，说明已实现到什么程度、`apply_change` 仍无 edit 层实现。
- 架构 §7.2：记录六步链条实现到第五步、第六步归 DocCore，以及 MD-11/HTML/Source 的实际行为。
- PRD：只写组 7 对 AC-04 的“基础贡献”，明确 AC-04 未通过。
- 验证计划：V-07 追加组 7 段，含勘误与已接受语义限制，明确 V-07 与 AC-04 均未通过。

## 3. 人工写入通道审查（任务 7.4 第二项）

证据 `step-7.4/manual-write-channel-review.log`（只读扫描，09:23:06Z）：

| 检查 | 结果 |
|---|---|
| `aline-edit` 公开面可变写入 API | 无匹配 |
| `aline-edit` 源码中 buffer 变更调用（tests 除外） | 仅 `plan.rs:242`、`plan.rs:330` 的 `CheckedBatch::new`——构造受检批次，不改 Buffer（**更正（复核 N-F1-2）**：09:23:06Z 扫描时行号为 `235`/`323`，后续修复使行号漂移到 `242`/`330`；结论不变） |
| workspace 生产代码实际写入入口（排除测试/示例/文档） | 无匹配 |
| 文件系统写入原语（引擎生产代码） | 只在 `aline-source/src/atomic.rs`（第 4.5 组 create-only 原语） |
| `aline-edit` 依赖 | 只有 `aline-buffer`、`aline-markdown` |

结论：`aline-edit` 公开面只有 `plan_change`/`validate_change` 与数据类型，无 `StringBuffer` 可变引用、无 `apply`/`reload`/`undo`/`redo`、无文件系统写入。`StringBuffer::apply` 是组 5 已交付的 DocCore 写原语；本组只在测试中以显式 `CheckedBatch` 调用模拟 DocCore。

**复核 F3-3 的缺口（如实记录）**：该日志只保留了扫描结果，没有逐条记录使用的 `rg`/`grep` 命令与完整参数，因此第三方无法从日志本身重跑同一扫描。上面五行的检查口径来自该日志的小节标题，但**命令不可复现**。

**round-2 处置（F3-3）**：新增 `step-7.4/fix-f3-3-f3-6/run-write-channel-review.sh`，逐条记录命令、输出与退出码，输出保存在同目录 `manual-write-channel-review-replay.log`（2026-10-31T10:55:07Z，HEAD `99867bf2…`）。首次运行因脚本里的相对路径少一层（`../../../..` → `../../../../../../`）导致 `rg` 全部报 “No such file or directory” 退出 2；该失败输出原样保留为 `manual-write-channel-review-replay-first-attempt.log`，修正后的脚本与日志不覆盖它。重跑结果与上表一致：`CheckedBatch::new` 仅在 `plan.rs:242`/`330`，`&mut`/`apply`/`reload`/`undo`/`redo` 在 `aline-edit/src` 无匹配（rg 退出 1 表示未找到），`std::fs`/`OpenOptions`/`fs::write`/`File::create` 在四个 crate 的 `src` 均无匹配，依赖仅 `aline-buffer`、`aline-markdown`。

**复核 F3-7 的缺口**：没有真实 UI/DocCore 消费者，因此“所有已开放写入口都经过 `plan_change`”在 M0 只能由“edit 层没有任何写入通道”这一结构性事实支撑，不能由运行时检查证明。

**复核 F3-8 的缺口**：组 6/7 的证据目录在本轮写报告时仍全部未跟踪（`git status` 显示 `??`）。这与组 6 第二轮复核的同一观察一致；证据只有提交后才有版本身份。

## 4. R/S 覆盖映射

| 条目 | 本组证据 | 状态 |
|---|---|---|
| R-4-01 源码 UTF-8 字节坐标、受检 newtype、补丁保留 `Vec<(Range<usize>, String)>` | `SourceEdit::new(range, expected, replacement)` 接收源码字节范围；`CheckedBatch::new` 保留裸补丁表示；`src_range`/`src_offset` 受检转换 | 本组范围内成立 |
| R-4-02 转换统一走 markdown 映射层、补丁后锚点变换归 buffer | `plan_change` 只经 `TextSnapshot::src_range`/`src_offset`；目标选区由 `after_selection_for_last_patch` 按补丁字节计算，锚点变换未在 edit 内手算 | 本组范围内成立；未验证 UI 命中 |
| R-4-03 映射保留身份/版本/方向/精确性，失效重算 | `ChangePlan` 绑定 `before_projection`；`validate_change` 比对投影身份；`bind_parsed` 要求 `Parsed` 与快照同源 | 本组范围内成立 |
| R-5-01 Change 绑定工作版本、选区、同基线补丁、目标选区、分组原因 | `ChangePlan` 字段与访问器；`GroupReason` 区分 `SourceReplace`/`BulkReplace`/`VisibleReplace`/`InlineFormat` | 成立 |
| R-5-02 真实写入前完整预检，含预演/reread，不只比较可见文字 | `validate_change` 顺序表；`verify` 的 `Visible` 契约先比隐藏语义再比可见文本；`refuse_hidden_link_target_lost` 最小反例 | 成立 |
| R-5-03 发布是一个提交边界，拒绝保持调用前状态、不产生空步 | `tests/refusals.rs` 16 例比较 `State`（round-2 后；修复后曾为 15 例）；`identity_replace_creates_no_history_step`、`already_formatted_creates_no_history_step` | edit 侧成立；发布边界归 DocCore（第 8 组） |
| R-5-07 MD-11 保护在所有可视化写入口、HTML 边界拒绝、Source 可改 | `check_visual_permissions` 在 `plan_replace` Visual 与 `plan_format`；`tests/protection.rs` 4 例；金样 `refuse_visual_footnote`/`refuse_format_html`/`allow_source_footnote`/`allow_source_html_raw` | 本组命令范围内成立；列表/表格/查找命令未实现 |
| R-5-08 查找含草稿、结果带版本、替换前重验、全部替换一步、格式化仅手动 | 只有“替换前重验”的最小支撑（`validate_change`）；查找/全部替换/格式化未实现 | **未覆盖**，明确不声称 |
| R-3-01 禁止第二份可编辑全文模型 | edit 不持有文本，只借 `&TextSnapshot`/`&Parsed` | 成立 |
| R-3-07 一个意图一个入口、执行时重验 | `validate_change` 重验；无 UI 消费者，不能证明“一个入口” | edit 侧成立，端到端未验证 |
| R-12-01 源文本不从 AST 重建，编辑是区间补丁 | 补丁全部由 `SourceEdit` 给出；候选文本由 `build_candidate` 在 before 文本上应用补丁，不经 AST | 成立 |
| R-12-02 块与 Gap 覆盖、隐藏语义不随展示合并消失 | `semantic_hidden` 保留隐藏片段原文；`Visible` 契约比较隐藏语义 | 成立（有第 2 节已接受限制） |
| R-16-01 禁 unwrap/panic，返回类型化错误 | `EditRefusal` 全类型化；clippy `-D warnings` 含 `unwrap_used`/`expect_used`/`panic`；测试用 `TestResult` 不用 `unwrap` | 成立 |
| R-16-02 错误保留类别与上下文 | `EditRefusal`/`RereadFailure` 分类；`RereadFailed { reason, cause }` 带上下文 | 成立 |
| R-16-04 分配与循环受预算约束 | `EditBudget`（补丁数/文本字节/解析）、`try_reserve` 失败转 `Allocation` | 成立 |
| K-02 无头可测、不引 UI | `aline-edit` 依赖只有 buffer + markdown；测试全部无窗口 | 成立 |
| K-03 单一写入口、拒绝零副作用 | 见第 3 节；edit 无写通道 | 结构性成立，端到端未验证 |

S-5/S-7/S-12 属安全/治理条目，本组只贡献“拒绝路径类型化、无 panic、预算受控”，其余由门禁脚本与独立复核覆盖，本报告不代写。

## 5. spec 场景覆盖

`openspec/changes/m0-engine-foundation/specs/engine/edit/spec.md` 的 2 个 ADDED Requirement、6 个 Scenario：

| Requirement | Scenario | 证据 | 判定 |
|---|---|---|---|
| 变更计划可验证 | 代表性成功计划 | 金样 7 个 `Changed` 用例；`source_batch_three` 一批多补丁；`consume_batch_single_history_step` 单历史步；`plan_change` 只读 | 覆盖 |
| | 版本或前提改变 | 金样 `refuse_wrong_version`/`refuse_wrong_projection`/`refuse_wrong_selection`；`refuse_expected_fragment` | 覆盖 |
| | 预演失败或预算耗尽 | 金样 `refuse_budget_patch`/`text`/`parse`；`refuse_hidden_link_target_lost`/`visible_text_changed`/`format_across_blocks`；`budget_exceeded_refusal_keeps_buffer_state` | 覆盖 |
| 拒绝完整状态不变 | 保护与 HTML 拒绝 | `tests/protection.rs` 4 例；`refusals.rs::protected_range_refusal_keeps_buffer_state`、`html_boundary_refusal_keeps_buffer_state` | 覆盖 |
| | 允许安全源码编辑 | 金样 `allow_source_footnote`、`allow_source_html_raw`；`protection.rs::front_matter_visual_refused_source_allowed`、`toc_visual_refused_source_allowed` | 覆盖 |
| | 待定输入时拒绝 | 金样 `refuse_pending_input`、`refuse_composition`；`refusal_keeps_pending_typing_group_open`、`refusal_keeps_composition_state_untouched`、`refusal_keeps_redo_branch_and_history_intact` | 覆盖 |

**覆盖不等于通过**：以上是 M0 无头层的场景覆盖；真实 UI/DocCore、IME、块分组与跨平台不在其中。

## 6. 统一工作区门禁

两次运行，均保留：

| UTC | 目录 | 结果 |
|---|---|---|
| 2026-10-31T09:21:09Z–09:21:48Z | `step-7.4/workspace-gates/` | 19/19 exit 0；`cargo test --workspace` 151 passed |
| 2026-10-31T10:04:27Z–10:04:53.722Z | `step-7.4/leader-verify-post-fix/workspace-gates/` | 19/19 exit 0；`cargo test --workspace` 195 passed |

**更正（复核 N-F3-2）**：上表第二行原写结束时间 10:04:56Z，与 `summary.json` 的最晚 `finished_utc`（10:04:53.722Z，文件 mtime 10:04:53Z）不符，现按 `summary.json` 更正为 10:04:53.722Z。另：调度方 `group-7/commands.log` 未登记这条 workspace 门禁命令（该文件只登记组 7 实现者运行的命令），因此时间与结果的权威来源是 `leader-verify-post-fix/workspace-gates/summary.json` 与其中 19 项的 stdout/stderr，不是 `commands.log`。门禁命令本身与下方代码块一致。

命令（第二次）：

```sh
CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home \
RUSTUP_TOOLCHAIN=1.98.1 CARGO_NET_OFFLINE=true \
python3 scripts/check_workspace.py \
  --cargo-home /Users/xian/Library/Caches/aline-m0/cargo-home \
  --output-dir docs/validation/m0-engine-foundation/group-7/step-7.4/leader-verify-post-fix/workspace-gates
```

19 项：工具链/工具存在性 4、版本 4、`fmt-effective-config`、`fmt`、`clippy`、`test`、`dependency`、`dependency-self-test`、`lint`、`lint-self-test`、`safety`、`safety-self-test`、`public-api`。

**F3-5 说明**：`environment.stdout.log`（09:21:05Z）在 macOS 上执行 `printenv CARGO_HOME RUSTUP_TOOLCHAIN CARGO_NET_OFFLINE` 时只打印了第一个变量，因此该日志只能看到 `CARGO_HOME`。真实值另由 `summary.json` 的 `env_overrides` 记录三项；这不是“变量未设置”。

**未覆盖**：门禁只在 macOS arm64 / Darwin 27.2.0 / rustc 1.98.1 上运行；Windows/Linux、其他 target、CI 未执行。

## 7. 未覆盖与不宣称

- 未覆盖：块打开/提交分组与 900ms 真实计时、IME 确认/取消、离块与 Undo/Redo 交错、自动保存/恢复/导出交错、ED-01 组合期等待/取消。
- 未实现：列表/表格/任务框/代码语言/全文格式化命令、查找替换事务、不可见命中定位。
- 未覆盖：MD-11 粘贴独立用例、HTML 右端越界与跨整个元素的格式命令、真实 UI/DocCore 消费者、灰色只读区呈现、Windows/Linux。
- 不宣称 V-07、AC-04 或 M0 出口通过；`tasks.md` 勾选不等于出口通过；本报告不代写独立复核结论。

## 9. round-2 复核新发现（N-F1-1～N-F1-4、N-F2-1、N-F3-1～N-F3-4）处置

本节只记录调度者对 `review-round-2.md` 新发现的处置位置；**是否关闭由独立复核判定，本节不写复核结论**。

| ID | 处置 | 证据 |
|---|---|---|
| N-F1-1 | `step-7.2/report.md:3` 状态行改为 15 例，并注明修复前 7 例 | 本文件 §9；`step-7.2/report.md` 首段 |
| N-F1-2 | 写入通道表的行号更正为当前 `plan.rs:242`/`330`，并标注 09:23 扫描时为 `235`/`323` | 本文件 §3 表格行 |
| N-F1-3 | 38 行归因更正：组 7 自身 8 条记录 / 16 行（含分隔空行），其余 22 行为组 6；最终 44 行中组 7 仍 16 行 | 本文件首段与 §1 |
| N-F1-4 | `step-7.1/report.md` 的实现/测试 mtime 标注“修复前”，并给出当前修复后值 | `step-7.1/report.md` §1 |
| N-F2-1 | 新增 `pure_whitespace_hidden_separator_is_accepted_limitation`（`tests/refusals.rs`），把“纯空白隐藏片段与片段边界不比较”钉成测试，并断言其块结构后果：`a\n\nb\n` 删 1..3 → `[Paragraph, Paragraph]` 变 `[Paragraph]`；`- a\n\nb\n` 删 3..5 → `[List, Paragraph]` 变 `[List]`，两例前后绘制文本均为 `"ab"` | `step-7.4/fix-n-f2-1-n-f1/`（refusals 16 passed、fmt 0）；10:52Z |
| N-F3-1 | 回填后哈希表加作用域“截至 10:11:09Z”，并记录 10:38:13Z 的三处漂移与“不影响 16 行/0 删除”结论 | 本文件 §1 |
| N-F3-2 | 门禁结束时间按 `summary.json` 更正为 10:04:53.722Z，并说明 `commands.log` 未登记该命令 | 本文件 §6 |
| N-F3-3 | 工作区未冻结属环境限制，已在各报告与组级报告如实记录，无代码处置 | `review-round-2.md` §4.5；本文件 §7 |
| N-F3-4 | 金样索引状态变化记录在 `step-7.1/report.md` §1（内容 sha256 与 mtime 未变，非回填；责任方不可确定） | `step-7.1/report.md` §1 |
| F3-3 | 新增可复现扫描脚本 `run-write-channel-review.sh` 与逐条命令日志；首次路径错误输出原样保留 | `step-7.4/fix-f3-3-f3-6/`（10:55:07Z） |
| F3-6 | 补跑字面命令 `cargo test -p aline-edit --locked --offline refusal`，退出 0，并记录“名字过滤弱于整目标”的事实 | `step-7.4/fix-f3-3-f3-6/commands.log`（10:55:15Z）；`step-7.2/report.md` §5 |

N-F2-1 的口径说明：该用例固定的是**当前已接受限制**，不是“已验证隐藏语义完整保留”。若 V-07 日后收紧为比较块结构，应把该用例改为拒绝并同步更新回填；本报告不据此声称 V-07 通过。
