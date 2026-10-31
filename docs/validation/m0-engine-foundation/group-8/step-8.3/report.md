# 8.3 记录：组合候选、发布等待与当前快照

日期：2026-10-31。状态：`cargo test -p aline-ui --test document_composition --locked --offline` 退出 0，25 例通过（`targeted-tests/cargo-test-document_composition.log`）。本文不宣称 V-08、AC-05 或 M0 出口通过；`tasks.md` 8.3 尚未勾选，待独立复核。

## 1. 实现范围

| 文件 | 作用 |
| --- | --- |
| `crates/aline-ui/src/document/composition.rs` | `CompositionState`、`CompositionView`；候选只进草稿视图，不写 Buffer |
| `crates/aline-ui/src/document/publication.rs` | `PublicationKind`（`Save`/`SaveAs`/`Export`）、`PublicationOutcome`、`PublicationResult`、`WaitHandle`、`WaitState`、`WaitStatus`、`WaitView` |
| `doc_core.rs` | `composition_start`/`composition_update`/`composition_commit`/`composition_cancel`；`begin_publication_wait`/`request_cancel_wait`/`cancel_wait`/`finish_publication` |

组合确认是一次独立的 `OperationSource::ImeCommit` 提交；取消恢复组合前选区并推进草稿代次、不产生历史步；发布等待只在组合活动时建立，等待期间输入与候选更新不被冻结，取消请求只改等待状态。

## 2. 用例覆盖

`step-8.3/state-tables/composition.tsv` 的 25 行与 `crates/aline-ui/tests/document_composition.rs` 的 `case_8_3_01`…`case_8_3_25` 一一对应；`待定` 列只做前后相等比较。

| 用例段 | 覆盖 |
| --- | --- |
| 8.3-01…05 | 首次 update 隐式开始；候选替换；相同候选仍推进 dg；确认产生单个 IME 节点；确认后 update 开启新组合 |
| 8.3-06…10 | 取消恢复组合前文本/选区且无空步；取消后旧身份失效；IME 节点不与相邻输入合并；取消保留已提交输入；空候选确认 `Unchanged` |
| 8.3-11…16 | 建立等待无快照副作用；重复等待拒绝；等待不冻结候选；组合未结束不能完成；确认后等待仍挂起；平台自然结束后取当前快照 |
| 8.3-17…18 | SaveAs/Export 等待可建立 |
| 8.3-19…22 | 取消请求只改等待状态；已取消等待不能完成；update 清除已取消等待；等待后取消组合恢复组合前状态 |
| 8.3-23 | 自动保存无 `DocCore` 入口 |
| 8.3-24 | 恢复读取不改快照身份 |
| 8.3-25 | 组合开始不误并普通待定组（T-8-05） |

## 3. 期望与实现的差异、实现裁决

- **T-8-05（组合开始时的待定组）**：实现先提交并封口已有普通输入组，再进入组合（`start_composition` 调 `commit_typing_group`）。TSV 状态列在该行多为 `待定`，`case_8_3_25` 只断言不误并：等待列为 `none`，候选文本未进入已提交 Buffer，唯一草稿槽与组合视图一致。
- **T-8-06（重复相同候选）**：仍推进 draft generation；`case_8_3_03` 断言 `assert_version(&doc, 0, 3)`。
- **T-8-07（无变化确认）**：`composition_commit` 返回 `CommitOutcome::Unchanged`，不写 Buffer，但 dg 前进（`next_draft`）；`case_8_3_10` 覆盖。
- **T-8-08（隐式开始与范围）**：首次 `composition_update` 以当前选区隐式开始，范围内候选替换；`case_8_3_01` 断言组合视图 `1..1` 与候选「中」。
- **T-8-12（自动保存/恢复）**：`PublicationKind` 只含 `Save`/`SaveAs`/`Export`，自动保存与恢复保护没有 `DocCore` 入口；`case_8_3_23`/`24` 只记录协议边界。
- **T-8-16（非组合期的发布等待）**：实现要求组合活动，否则返回 `DocError::NoComposition`；期望文档中的二选一在此固定为「拒绝」。
- 本步未发现期望与实现的其他差异。

## 4. 证据

- 命令与退出码：`step-8.3/commands.log`（2026-10-31T11:13:04Z，exit 0）。
- 原始测试输出：`step-8.3/targeted-tests/cargo-test-document_composition.log`（`25 passed; 0 failed`）。
- 实现前期望：`step-8.3/expectations.md`、`step-8.3/state-tables/composition.tsv`。
- 全量门禁：`../leader-verify/workspace-gates/`（17/19；`safety`、`safety-self-test` 失败原因见 `../README.md` 状态节）。

## 5. 限制与未覆盖

- 真实 IME 候选窗口、marked text、平台组合回调时序未测；本组只有状态机口径。
- 真实保存/另存/导出、磁盘 IO、写盘回执与文件协调未测；`DocCore` 不承担文件协调。
- 未做独立复核；`tasks.md` 8.3 保持未勾选。
- 全量 `safety` 门禁失败原因已记录，守卫未修改，待用户裁决。
