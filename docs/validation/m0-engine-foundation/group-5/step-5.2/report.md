# 5.2 记录：补丁拒绝、无变化与原子性

日期：2026-10-31。状态：`crates/aline-buffer/tests/rejections.rs` 的 4 个集成测试通过。本文只记录低层拒绝语义，不宣称 UI/命令层已消费或 V-07 通过。

## 1. 覆盖的反例

测试 `rejection_cases_leave_text_index_selection_revision_and_history_unchanged` 在 `abcXYZ` 上覆盖：

| 反例 | 期望 |
|---|---|
| 末项越界 `6..7` | `CheckedBatch::new` 拒绝 |
| 重叠 `0..3` 与 `2..4` | 拒绝 |
| 逆序 `4..5` 后接 `0..1` | 拒绝 |
| 同点歧义插入 `3..3→a` 与 `3..3→b` | 拒绝 |
| 半字符 `1..3` 落在 `甲😀Z` 的 emoji 中间 | `NotCharBoundary` |

测试 `apply_rejections_are_atomic_for_baseline_selection_and_capacities` 覆盖：

| 反例 | 期望 |
|---|---|
| 批次基线来自另一个 buffer | `SnapshotMismatch` |
| after 选区端点不是 UTF-8 边界 | `NotCharBoundary { offset: 2 }` |
| after 文本超过 `TextBudget` | `BudgetExceeded` |
| 新行数超过 `LineIndexBudget` | `LineCountExceeded { count: 2, limit: 1 }` |
| 历史节点预算为 0 | `HistoryBudgetExceeded` |
| 历史补丁文本字节预算为 1 | `HistoryBudgetExceeded` |

每个反例在调用前后比较：文本、行数、逐行内容范围、原始选区、`DocumentRevision`、undo/redo 深度、历史节点数、当前 `SnapshotId`。全部零变化，证明拒绝发生在发布前，未只改部分字段或计数器。

## 2. 合法边界

测试 `legal_empty_insertion_and_no_change_replacement_are_distinct`：

- 空区间插入 `1..1→X` 产生 `aXbc`、逆补丁 `1..2→""` 和一个历史节点。
- 内容相同替换 `1..2→b` 返回 `Unchanged`，文本、行索引、选区、revision、undo/redo、历史节点数和快照身份均不变。
- 混合批次中的空文本插入被剔除，只记录实际替换 `1..2→B`，历史正/逆补丁对应实际变化。

测试 `no_change_after_undo_does_not_clear_redo_branch`：Undo 后再提交内容相同的替换，返回 `Unchanged`，redo 分支仍保留，随后 Redo 正常恢复 `abcX` 与 after 选区。

## 3. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T06:24:21Z | `cargo test -p aline-buffer --locked --offline --test rejections` | 0 | `targeted-tests/rejections.stdout.log` / `.stderr.log` |

`4 passed; 0 failed; 0 ignored; 0 filtered out`。命令与缓存环境同 5.1。

## 4. 未覆盖与边界

- 未做模糊/随机反例生成；反例为手写固定集合。
- 未测真实内存耗尽、OOM、整数回绕之外的操作系统级容量失败；预算均是注入值。
- 未验证命令规划层是否会先做自己的保护检查；5.2 只证明 buffer 入口拒绝时零发布。
- 快照身份比较只覆盖当前实现；跨进程/跨设备身份不在本步。

## 5. 独立复核

本节不写复核结论。独立复核者应检查：反例是否真的覆盖任务所列类别、状态比较是否包含快照身份、`CheckedBatch::new` 与 `apply` 的拒绝分工是否清楚、无变化批次是否确实不影响 redo。
