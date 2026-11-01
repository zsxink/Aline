# R01 独立复核记录（主 Agent 保存）

复核者：独立 reviewer（`a9218cefe97300e43`，未参与 R01 实现，只读）。
复核对象：`79c3b54`（BASE `2f1dcea`）。复核日期：2026-11-01。

## 裁决

**APPROVE_WITH_CONCERNS**（无阻断 finding）。

## reviewer 实际运行（主 Agent 已核对其落盘输出）

| 范围 | 命令 | 结果 |
|---|---|---|
| BASE 复现 | `2f1dcea` 只读导出 + 自写探针 `r01_base_probe` | exit 0，输出 `redo_ok=true current="xa" draft=None redo_depth=0`、`selection=(4,4)` —— F-01/F-05 真实存在 |
| HEAD 目标 | `cargo test -p aline-ui --test document_typing --locked --offline` | exit 0，30 passed |
| HEAD crate | `cargo test -p aline-ui --locked --offline` | exit 0，8 目标 ok |
| HEAD 全量 | `cargo test --workspace --locked --offline` | exit 0，**45 ok / 0 failed** |
| clippy | `cargo clippy -p aline-ui --all-targets --locked --offline -- -D warnings` | exit 0（`Finished`，无 warning） |
| fmt | `cargo fmt --all --check` | exit 0 |
| 工作树 | 因 R02 在写 `aline-markdown` 而编译失败（E0425） | **失败源为 R02，未计入 R01** |

我核对了 `/tmp/r01-verify-ws.log`：45 条 `test result: ok`、0 条 `FAILED`；
`/tmp/r01-verify-clippy.log` 末行 `Finished` 无 warning。

## reviewer 的六项逐条结论（摘要）

1. 复现方法为 `git archive` 只读副本 + 自写探针；曾短暂建 worktree，收到我的范围澄清后已删除，未动工作树。
2. 五条用例用全量 `capture()` 前后相等，非恒真；快照 id、历史 0/0/0、`assert_open(...,4,2)`、§9.3 结算均实测。
3. 穷尽性：start_group 两分支、append_to_group、undo 捕获、redo 探针校验、commit、组合四路径、sync_block、close_block、reload 全查，无路径把陈旧 `after_selection` 送入 redo。
4. 契约：拒绝待定组 redo 最贴近 design.md 第 5 条与 §9.3；T-8-17 仍待裁决，登记为非缺陷。
5. 铁律：生产 diff 无新增 `unwrap/expect/panic!/unreachable!/assert!`；无第二份可写全文/第二历史；未绕过映射层。
6. 证据链：pre/post 原始输出与 commands 行号逐条对应，TDD 编译失败偏差已如实披露。

## Findings 处置（主 Agent 逐条复核后）

| # | Finding | 我的复核 | 处置 |
|---|---|---|---|
| 1 | 「workspace 62 个 ok」实为整份文件合计，该段实际 45 | **成立**。我重数：6 段合计 62，第 6 段（文件 446 行起）为 45 | 已在 `report.md`/`commands.txt`/`handoff.md` **追加勘误**，原文保留 |
| 2 | `commands.txt` 第 8/9 行缺时间戳 | **成立**（原文以 `—` 占位） | 保留原样，勘误中说明「不事后补造时间」 |
| 3 | workspace 段跑在含 R02 未提交改动的树上，commands/report 未复述 | **成立**（`post-fix-tests.txt:444` 有注，上游文件未复述） | 已在勘误中复述 |
| 4 | `group.after_selection` 仅由 `undo()` 捕获覆盖维持权威，`set_selection` 不同步，建议加注释 | **成立**，且这正是 F-05 成立的原因（我在 `main-agent-verification.md` §2 已独立读出） | 建议类，不改代码；登记为后续注释项 |

## 仍未裁决

- **T-8-17**：待定组存在时 redo 的行为。实现取「受检拒绝」（`DocError::PendingInput`），reviewer 认为最贴近 §9.3 文字，但属规范裁决，不由实现方自定。保持 `待裁决`。
- 宿主侧 `DocError::PendingInput` 的 `match` 影响：仓内无法验证（无穷尽匹配），需 M1 接线时处理。
- `capture()` 把快照错误吞为 `None`：既有 helper 行为，非本批引入。

## 结论

R01 实现与证据通过独立复核，仅需口径勘误（已追加）。R01 可记 `完成`；
T-8-17 保持 `待裁决`，不因本批通过而自动关闭。
