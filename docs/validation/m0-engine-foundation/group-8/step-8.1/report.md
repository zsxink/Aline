# 8.1 记录：唯一 Buffer/History/OpenBlock 与当前快照

日期：2026-10-31。状态：`cargo test -p aline-ui --test document --locked --offline` 退出 0，22 例通过（`targeted-tests/cargo-test-document.log`）。本文不宣称 V-08、AC-05 或 M0 出口通过，也不把任务勾选当成验收；`tasks.md` 8.1 尚未勾选，待独立复核。

## 1. 实现范围

| 文件 | 作用 |
| --- | --- |
| `crates/aline-ui/src/document/doc_core.rs` | `DocCore` 独占一个 `StringBuffer`（内含唯一 History）、一个 `OpenBlock` 与快照缓存；所有拒绝在修改前预检 |
| `crates/aline-ui/src/document/open_block.rs` | `OpenBlock`、`DraftSlot`、`TypingGroup`；唯一草稿槽 |
| `crates/aline-ui/src/document/snapshot.rs` | `CurrentSnapshot`、`SnapshotCache`；同一工作版本重复读取保持同一 `SnapshotId` |
| `crates/aline-ui/src/document/mod.rs`、`crates/aline-ui/src/lib.rs` | 仅模块声明与再导出；无 gpui 依赖、无 `unsafe`、无清单/锁文件改动 |

本步使用的入口：`DocCore::new`、`open_block`、`type_text`、`commit_open_block`、`leave_block`、`escape`、`current_snapshot`、`undo`/`redo`/`reload`，以及 `buffer_text`、`open_block_*`、`undo_depth`/`redo_depth`/`history_node_count` 访问器。

## 2. 用例覆盖

`step-8.1/state-tables/open-block-snapshot.tsv` 的 22 行与 `crates/aline-ui/tests/document.rs` 的 `case_8_1_01`…`case_8_1_22` 一一对应，逐列断言；TSV 中 `待定` 的列只做「调用前后相等」的零副作用比较。

| 用例段 | 覆盖 |
| --- | --- |
| 8.1-01…04 | 输入只进唯一草稿；`buffer_text` 不含未提交草稿，`snapshot_text` 含最新草稿；相邻输入合并 |
| 8.1-05…08 | 提交为单个 `Typing` 补丁且块保持打开；提交后快照无副作用；再输入开启新草稿 |
| 8.1-09 | 同一工作版本重复取快照保持同一 `SnapshotId` |
| 8.1-10…16 | 提交/封口/Undo/Redo 的节点与选区；快照不清 redo |
| 8.1-17…21 | `DraftBaselineConflict` 对快照/输入/提交/离开的拒绝且不 `drop_open`；关闭后提交返回 `NoOpenBlock` |
| 8.1-22 | 非 UTF-8 边界输入被拒绝且零副作用 |

## 3. 期望与实现的差异、实现裁决

- **打开块区间（差异，未裁决）**：`step-8.1/expectations.md` §3 F2 假设 `new_doc("abcd\n", (4,4))` 的打开块为 `0..4`；实现 `open_block` 保存调用方传入的 `range`，测试夹具传 `selection.range()`，实际为 `4..4`。TSV 没有断言打开块区间的列（只断言 anchor/head 选区），22 例均不受影响。该差异属于块范围语义，留待架构裁决；未反向改写期望。
- **T-8-01（入口缺口）**：实现提供 `open_block(range, selection)` 与 `leave_block`/`escape`；「已有打开块」由 `BlockAlreadyOpen` 表达，关闭后为 `NoOpenBlock`。
- **T-8-02（草稿写入口径）**：普通输入只改唯一草稿；`buffer_text` 不变、`snapshot_text` 反映草稿；提交时才以一个单区间 `OperationSource::Typing` 补丁写入并封口。
- **T-8-14（公开形状）**：`CurrentSnapshot`、`CommitOutcome`（`Unchanged`/`Committed { revision }`）、`DocError` 由实现固定并从 `aline_ui` 再导出，签名以 `doc_core.rs` 为准。
- **T-8-03/T-8-04/T-8-13** 等跨步骤项见 `step-8.2/report.md` 与 `step-8.4/report.md`；本步未单独裁决。

## 4. 证据

- 命令与退出码：`step-8.1/commands.log`（2026-10-31T11:13:03Z–11:13:04Z，exit 0）。
- 原始测试输出：`step-8.1/targeted-tests/cargo-test-document.log`（`22 passed; 0 failed`）。
- 实现前期望：`step-8.1/expectations.md`、`step-8.1/state-tables/open-block-snapshot.tsv`（手写，早于实现）。
- 全量门禁：`../leader-verify/workspace-gates/`（19 项中 17 项退出 0；`safety` 与 `safety-self-test` 因 `tests/common/mod.rs` 的 `test-module-resolution` 误判退出 1，见 `../README.md` 状态节）。

## 5. 限制与未覆盖

- 纯内存状态；无 GPUI 窗口、真实 IME、真实保存、文件协调、线程/定时器。
- 本步未做独立复核，不构成 8.1 验收；`tasks.md` 保持未勾选。
- 全量 `safety` 门禁失败原因已记录，守卫未修改，待用户裁决。
