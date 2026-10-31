# 8.4 记录：受控结果采纳、拒绝零副作用与重载历史边界

日期：2026-10-31。状态：`cargo test -p aline-ui --test document_results --locked --offline` 退出 0，17 例通过（`targeted-tests/cargo-test-document_results.log`）。本文不宣称 V-08、AC-05 或 M0 出口通过；`tasks.md` 8.4 尚未勾选，待独立复核。

## 1. 实现范围

`DocCore::accept_derived_result` 按固定顺序校验派生结果身份：

1. 目标仍打开（`None` → `DocError::ClosedTarget`）；
2. 取当前快照（包含最新草稿）；
3. 逐项比较文档、工作版本与快照身份（`Error::WrongDocument` → `WrongDocument`；`StaleWorkingVersion`/`StaleSnapshot` → `StaleResult`）。

成功只返回 `()`（T-8-14 的实现决定），不改变状态。`reload(text, selection, now_millis)` 通过 Buffer 重载建立独立历史边界，不并入 `Typing` 节点。

## 2. 用例覆盖

`step-8.4/state-tables/stale-results.tsv` 的 17 行与 `crates/aline-ui/tests/document_results.rs` 的 `case_8_4_01`…`case_8_4_17` 一一对应；`待定` 的 dg 列只做前后相等比较，无 `DocCore` 入口的行只断言已确定的状态列。

| 用例段 | 覆盖 |
| --- | --- |
| 8.4-01 | 当前身份接受且零副作用 |
| 8.4-02…05 | 旧初始身份、其他文档、错误 dg、提交前捕获的请求全部拒绝 |
| 8.4-06 | Undo 回到相同文本后，旧身份仍按 dg 拒绝（不能按内容相等接受） |
| 8.4-07…08 | `InputIdentity` 缺解析配置代次/request id 字段；只断言状态列，不调用采纳 |
| 8.4-09 | 版本/快照匹配但目标已关闭 → `ClosedTarget`，不复活关闭对象 |
| 8.4-10…11 | 草稿基线冲突对快照/提交的拒绝，且不 `drop_open` |
| 8.4-12 | 中间版本投影拒绝（`StaleResult`）且输入/选区保留 |
| 8.4-13…16 | `reload` 独立历史边界；Undo/Redo 重载；无变化重载保留 redo |
| 8.4-17 | 文件协调无 `DocCore` 入口 |

## 3. 期望与实现的差异、实现裁决

- **T-8-09（`ClosedTarget` 顺序）**：先检查目标是否打开，再比较身份；`case_8_4_09` 固定该顺序。
- **T-8-10（身份字段缺口）**：`InputIdentity` 未含解析配置代次与 request id；`case_8_4_07`/`08` 只断言状态列，采纳结果列保持 `待定`，没有伪造通过。
- **T-8-11（错误透传）**：`WrongDocument`/`StaleResult` 作为独立 `DocError` 变体返回（不是 `Identity(IdentityError::…)` 透传），`case_8_4_03`…`05`、`12` 固定。
- **T-8-13（重载入口与 dg）**：实现提供 `reload`；dg 按 `after_commit` 推进，`case_8_4_13` 注释记录该值为 `待定` 并不断言。历史节点的 `source=Reload` 不在公开 API 暴露，测试只通过节点数、Undo/Redo 文本与深度验证独立边界。
- **差异（快照 id 复用）**：`case_8_4_06` 中撤销待定组不触碰 Buffer，撤销后的 `SnapshotId` 复用未变 Buffer 的 id；测试注释明确「snapshot_id=new 相对撤销前的草稿快照成立」，没有把该 id 当作版本身份。该点留待架构/规范裁决。
- **差异（节点来源不可观察）**：`Reload` 节点来源未通过访问器暴露；`case_8_4_13` 用节点计数与撤销/重做结果间接验证，未声称已断言 `source`。
- **T-8-17（Undo 后新草稿与 redo 分支）**：由 `case_8_4_06` 与 8.2 的 redo 用例覆盖可观察部分，Buffer redo 分支的失效时序仍属 `待定`，未额外断言。

## 4. 证据

- 命令与退出码：`step-8.4/commands.log`（2026-10-31T11:13:04Z，exit 0）。
- 原始测试输出：`step-8.4/targeted-tests/cargo-test-document_results.log`（`17 passed; 0 failed`）。
- 实现前期望：`step-8.4/expectations.md`、`step-8.4/state-tables/stale-results.tsv`。
- 全量门禁：`../leader-verify/workspace-gates/`（17/19；`safety`、`safety-self-test` 失败原因见 `../README.md` 状态节）。

## 5. 限制与未覆盖

- 无真实解析/投影引擎参与；结果由测试构造，只验证身份协议与零副作用。
- 无真实文件协调、外部磁盘变化与恢复记录；`DocCore` 不承担这些职责。
- 未做独立复核；`tasks.md` 8.4 保持未勾选。
- 全量 `safety` 门禁失败原因已记录，守卫未修改，待用户裁决。
