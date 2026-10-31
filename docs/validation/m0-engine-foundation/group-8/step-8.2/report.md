# 8.2 记录：输入分组、900ms 边界与 Undo/Redo 优先级

日期：2026-10-31。状态：`cargo test -p aline-ui --test document_typing --locked --offline` 退出 0，25 例通过（`targeted-tests/cargo-test-document_typing.log`）。本文不宣称 V-08、AC-05 或 M0 出口通过；`tasks.md` 8.2 尚未勾选，待独立复核。

## 1. 实现范围

`DocCore::type_text(at, text, now_millis)` 在显式虚拟时间下维护打开块内的输入组：

- 相邻输入且距上一组起点 `<= TYPING_MERGE_WINDOW_MILLIS`（900ms，取 `aline-buffer` 常量）时并入同一草稿组；`901ms` 先提交并封口旧组，再以本次输入开启新组。
- 非相邻输入（中间移动选区）另起一组。
- `commit_open_block`/`leave_block`/`escape` 都是组边界；`leave`/`escape` 提交剩余组并关闭块，`escape` 在关闭后拒绝（`NoOpenBlock`）。
- Undo 优先撤销待定草稿组，其次撤销已提交节点；Redo 恢复待定组或选中节点，并恢复其选区。
- 取快照不封口、不影响后续合并。

## 2. 用例覆盖

`step-8.2/state-tables/typing-groups.tsv` 的 25 行与 `crates/aline-ui/tests/document_typing.rs` 的 `case_8_2_01`…`case_8_2_25` 一一对应；`待定` 列只做前后相等比较。

| 用例段 | 覆盖 |
| --- | --- |
| 8.2-01…06 | 899/900 合并、901 拆分；边界后的节点数与 revision/dg |
| 8.2-07…11 | 显式提交是边界；封口阻止跨提交合并；非相邻输入分组；无待定组提交为 `Unchanged` |
| 8.2-12…14 | `leave`/`escape` 提交剩余组并关闭；关闭后 `escape` 拒绝 |
| 8.2-15…19 | Undo 待定组优先、二次 Undo 进入节点、Redo 恢复节点 |
| 8.2-20…22 | 快照不封口；快照后输入仍合并；快照+输入只产生一个节点 |
| 8.2-23 | 模式切换在 `DocCore` 无入口（只记录协议边界） |
| 8.2-24…25 | IME 确认后的普通输入按普通分组，不并入 `ImeCommit` 节点 |

## 3. 期望与实现的差异、实现裁决

- **T-8-04（跨 900ms 的版本转换）**：实现暴露两次转换，`case_8_2_03` 断言 `assert_version(&doc, 1, 3)`，即一次 `type_text` 跨越 900ms 后 revision `0→1`、draft generation `1→3`（先提交旧组，再开启新组）。期望文档的手算口径与实现一致。
- **T-8-03（待定组 Undo/Redo 的版本推进）**：待定组 Undo 只回退草稿状态、不触碰 Buffer；`case_8_2_15`…`17` 固定「撤销期间 Buffer revision 不变、dg 前进」的可观察部分，其余列在 TSV 中为 `待定`，未强行断言。
- **T-8-15（模式切换）**：`DocCore` 不提供模式切换入口，`case_8_2_23` 只验证无该入口时的状态不变，不把协议性期望当作实现通过。
- 900ms 比较符沿用 `prepare_record` 的 `<=`：899/900 合并、901 拆分；`case_8_2_02` 覆盖闭区间边界。
- 本步未发现期望与实现的其他差异。

## 4. 证据

- 命令与退出码：`step-8.2/commands.log`（2026-10-31T11:13:04Z，exit 0）。
- 原始测试输出：`step-8.2/targeted-tests/cargo-test-document_typing.log`（`25 passed; 0 failed`）。
- 实现前期望：`step-8.2/expectations.md`、`step-8.2/state-tables/typing-groups.tsv`。
- 全量门禁：`../leader-verify/workspace-gates/`（17/19；`safety`、`safety-self-test` 失败原因见 `../README.md` 状态节）。

## 5. 限制与未覆盖

- 虚拟时间只覆盖状态机口径；真实平台的按键时序、焦点切换与 IME 事件顺序未测。
- 未做独立复核；`tasks.md` 8.2 保持未勾选。
- 全量 `safety` 门禁失败原因已记录，守卫未修改，待用户裁决。
