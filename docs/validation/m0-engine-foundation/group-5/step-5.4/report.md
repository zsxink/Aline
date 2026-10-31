# 5.4 记录：历史节点、待定组、保存点与重载边界

日期：2026-10-31。状态：`crates/aline-buffer/tests/history.rs` 的 9 个集成测试和全 crate 29 个测试通过。本文只记录低层内存历史，不宣称自动保存、外改提示、磁盘基线或 UI dirty 已闭环。

## 1. 历史语义覆盖

| 测试 | 验证内容 |
|---|---|
| `history_records_source_selections_and_monotonic_revisions` | 步骤记录来源、before/after 选区、before/after revision；apply/undo/redo 的 revision 严格递增，Undo/Redo 不回退代次 |
| `history_typing_merge_window_and_seal_boundaries` | 900ms 内相邻普通输入合并为一个待定组；901ms 不合并；`seal_pending` 后不合并；非 `Typing` 来源不并入；一次 Undo 撤销整个待定组，Redo 恢复最后 after 选区 |
| `history_same_stack_depth_after_save_undo_new_edit_is_not_save_point` | 保存 A→Undo→输入 B 后栈深与保存时相同，但 `is_at_save_point` 为 false；撤销 B 不能回到旧保存文本 |
| `history_content_equal_redo_keeps_new_identity_dirty` | Undo+Redo 回到与保存点相同文本，但新快照身份仍判脏；`saved_revision` 保留 |
| `history_redo_branch_is_discarded_by_new_edit` | 新编辑截断 redo 分支，新分支节点和 undo/redo 深度正确 |
| `history_trimming_protects_save_point_and_current_node` | 节点预算 3 只保留最近 3 个；`trim_history(1)` 删除 2 个；保存点前缀不被裁剪；超预算 apply 失败后状态零变化；Undo 到保存点内容仍保守判脏 |
| `history_undo_and_redo_budget_failures_leave_state_unchanged` | Undo 的当前文本物化预算不足、Redo 的正向输出预算不足时，文本/索引/选区/revision/undo/redo 均不变 |
| `history_reload_is_independent_memory_boundary` | 重载来源为 `Reload`，以整段前向/逆向补丁形成独立可撤销边界；Undo/Redo 只改内存；无变化重载保留 redo |
| `history_handles_keep_trimmed_patch_resources_alive` | 裁剪后旧 `HistoryStepHandle` 仍可读取补丁文本，`Arc` 资源不被提前释放 |

保存点按快照身份保护，不以 `undo.len() == saved_at` 或 revision 判断；内容相同但身份不同的 Redo 仍保守显示修改，未做每键全文哈希或后台内容比较。裁剪从最旧节点开始，保留保存点所在前缀和当前节点。

## 2. 预算与原子性

- `HistoryBudget` 同时限制节点数与全部补丁文本字节数；记录前 `prepare_record` 检查，失败不发布新文本/索引/选区/revision。
- Undo/Redo 先在临时文本上执行逆/正补丁，再重建快照、行索引、选区；任何预算/上下文错误都返回原状态。
- 测试明确覆盖“Undo 当前文本物化预算”和“Redo 正向输出预算”，不是所有预算组合的穷举。
- 重载只接受显式整段新文本、after 选区和注入时间；无磁盘路径、无自动保存计时器、无外部文件监听。

## 3. 定向与全 crate 证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T06:24:21Z | `cargo test -p aline-buffer --locked --offline --test history` | 0 | `targeted-tests/history.stdout.log` / `.stderr.log` |
| 2026-10-31T06:24:21Z–06:24:22Z | `cargo test -p aline-buffer --locked --offline` | 0 | `targeted-tests/full-crate.stdout.log` / `.stderr.log` |

历史过滤：`9 passed; 0 failed`。全 crate：5 个单元测试、5 个锚点、2 个 checked batch、9 个历史、2 个金样、2 个公共边界、4 个拒绝，共 29 个测试；0 失败、0 ignored、0 filtered out（doc tests 为 0）。

审计未发现哈希/指纹符号，历史步骤只保存补丁、选区、来源、revision 和快照身份/长度；没有每键全文比较路径。

## 4. 未覆盖与边界

- 未实现/验证自动保存调度、保存点内容比较、后台哈希、dirty 状态发布、关闭 guard；这些属于 UI/会话层。
- TA-02 的真实自动保存暂停、外改检测与提示尚未实现，仍属 M2；本步的重载只是内存历史边界，不读磁盘、不移动磁盘基线（`aline-buffer` 没有磁盘基线字段）。
- 未做历史预算的真实内存测量或长会话压力测试；预算值是注入上限。
- 未验证 IME 确认/取消、跨块命令、组合输入与历史的交错。
- 未运行其他 OS/target。

## 5. 独立复核

本节不写复核结论。独立复核者应检查：900ms 边界是否按注入时间计算、保存点是否确按快照身份保护、内容相同 Redo 的保守 dirty 是否与规范一致、裁剪是否可能误删保存点/当前节点、预算失败是否真的零发布、重载是否被误写成磁盘基线闭环。
