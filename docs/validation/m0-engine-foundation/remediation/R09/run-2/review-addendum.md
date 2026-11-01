# R09 补充审查与主 Agent 核验

范围：核对首轮 review 的 F1/F2/F3 后续处理，并独立检查新加的 F1 持久回归。首轮 reviewer 未参与 R09 实现或该测试实现；reviewer 受只读限制，结论由主 Agent据其原始返回落盘。原始复核结论见 `R09/run-1/review.md`，补充复核提交身份为 HEAD `96024867f7ae949d7328232b05d93ed5bc6033d9`。

## 裁决

**APPROVE：建议合并 R09；F1/F2/F3 的后续处置充分。** 之前唯一低风险关注（敏感词情形缺持久回归）已由 `diagnostics.rs` 新测试补齐。此次结论仅覆盖原 review concerns 的处理，不重做 R09 整体实现审查，也不代表 M0 出口通过。

- **F1：关闭。** 原 `716f788` 行族过滤有效，新增 `dump_structure_checks_ignore_sensitive_words_in_drawn_text` 持久覆盖两个独立路径：预算回退正文含 `Table CodeBlock diagnostic`，Mermaid 正文含 `diagnostic`。用例同时断言完整 dump 保留正文词、block/diagnostic 结构行族的结果，能防止用完整 dump 做负向匹配。独立 reviewer 逐行审查用例，并重跑 exact 测试通过；主 Agent 核对 diff 后也复跑通过。
- **F2：关闭。** reviewer 独立 `wc -l` 得到期望文件 74 行；现有逐行比较没有硬编码 65 行限制。report §11.1 已勘误旧报告和提交说明的“65 行”。
- **F3：登记可接受。** reviewer 核对 `Backing` 私有、Overlay 唯一构造先验证、`iter_chunks` 失败条件及调用链，判定 `unavailable` 当前不能由公开 API 构造。无须为覆盖非法内部状态而暴露可写 API；保持“未动态覆盖、未来新增 fallible backing 时重新审查”限制。

## 独立 reviewer 的执行摘要

reviewer 只读审查，未写文件。其命令原始输出在 reviewer 会话中产生并随审查消息返回；测试命令及输出摘要如下（stderr 均为空）：

| argv | cwd | UTC | exit | 输出 |
|---|---|---|---:|---|
| `cargo test -q -p aline-markdown --locked --offline --test diagnostics --test dump` | 仓库根 | `08:23:07.411265–08:23:07.484187` | 0 | diagnostics 5 passed；dump 7 passed |
| `cargo test -q -p aline-buffer --locked --offline --test public_boundaries` | 仓库根 | `08:23:07.484222–08:23:08.282244` | 0 | 2 passed |
| `wc -l docs/validation/m0-engine-foundation/remediation/R09/run-1/basic.dump.expected.txt` | 仓库根 | `08:23:08.282274–08:23:08.286148` | 0 | `74 docs/validation/m0-engine-foundation/remediation/R09/run-1/basic.dump.expected.txt` |
| `cargo run -q -p aline-markdown --locked --offline --example dump_tree -- /dev/stdin` | 仓库根 | `08:23:24.086273–08:23:24.752451` | 0 | 正文含敏感词时完整 dump 匹配为 true；block/diagnostic 结构行族匹配为 false / 0 |
| 同上 | 仓库根 | `08:23:24.752504–08:23:25.158621` | 0 | Mermaid 正文 `diagnostic` 留在 drawn text；CodeBlock 行保留，diagnostic 行族为 0 |

其独立复算还核对了 `716f788` diff、F3 构造链和 `public_boundaries`；详情随审查消息存档。本目录另存主 Agent 在新测试落盘后的原始复跑记录 `review-evidence/commands.jsonl`、stdout/stderr，用于核对本次工作树。

## 主 Agent 的 diff 与复跑

检查 `crates/aline-markdown/tests/diagnostics.rs` 的新增测试：预算场景调用 `with_max_segments(0)`，通过既有 `assert_whole_source_fallback` 校验 Raw 回退、原文与 BudgetExceeded 诊断，再校验敏感正文在结构行族内不误认；Mermaid 场景校验 drawn text、CodeBlock 结构及空 diagnostic 行族。修改只新增测试，不改生产 API。

在新增测试后的工作树运行：

- `cargo test -q -p aline-markdown --locked --offline --test diagnostics dump_structure_checks_ignore_sensitive_words_in_drawn_text -- --exact`：exit 0，1 passed，0 failed。
- `git diff --check`：exit 0。
- 实际 argv/cwd/UTC/exit/stdout/stderr：`review-evidence/commands.jsonl` 与 `01.*`、`02.*`。

worker 另跑完整 diagnostics 6/6、`cargo fmt --all --check` 与 `git diff --check`，完整命令与输出见 `source-fix-evidence/`。初次 fmt 失败和随后修正均保留。R09 原有非阻断限制（FNV 非密码学；F3 无动态覆盖）仍有效；全工作区门禁留 R14。
