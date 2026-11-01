# R14 引擎范围独立复核

Reviewer 未参与 F-01–F-07/F-10/F-11 的实现或 R14 证据写入。实际阅读 DocCore/edit、RewriteMap、ReusedBlock、grapheme、R08 金样、R10 复算及对应修复前反例，并独立运行：

```text
cargo test -p aline-buffer -p aline-markdown -p aline-edit -p aline-ui --locked --offline --quiet
```

Reviewer 原始记录：cwd `<repo>`，`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`，2026-11-01T11:35:01.652025Z–11:35:03.027491Z，exit 0，321 passed / 0 failed。Reviewer 遵守独立审查角色的只读约束，未自行写文件；主 Agent 按其精确 argv 复跑并保全原始 stdout/stderr/JSON，见 [`engine-independent-command/`](./engine-independent-command/)。主 Agent 复跑的时间及 exit 以该目录 command.json 为准，不冒充 reviewer 原命令时刻。

## 结论

**APPROVE_WITH_CONCERNS。**所审引擎整改可接受；没有发现新的、有证据的生产代码阻断。修复判断包括：

- 待定输入在 Buffer redo 历史移动前被拒；Undo 保存方向选区，Redo 先探测选区，再发布。
- 普通输入和 IME 共用 `publish_input → plan_change → validate_change → Buffer.apply`；宿主拒绝用例比较真实文档状态。
- RewriteMap 按片段边界复合，Synthetic 零来源锚点不再扩成非空来源。
- ReusedBlock 坐标绑定旧快照，并校验新身份及 UTF-8 边界。
- grapheme 分开 scalar/UTF-16/字素，覆盖两向移动、Unicode 17/18 Indic 差异与 4096/256-byte 预算。
- R08 的14行结果为删除5、粘贴6、HTML3；R10 当前51行复算输出相等。

## Concerns

1. Reviewer 初审时的 R14 Scenario matrix 存在目录级占位，无具体测试名/日志；已退回矩阵撰写者补齐。这是交付证据缺口，不是实现缺陷；补齐后需再次核验。
2. T-8-17 新草稿起始与 Buffer redo 分支失效时序仍待语义裁决。拒绝能防止丢草稿，但不代替产品时序决定。
3. R08 历史 TSV 输入、R10 旧 TSV 独立推导来源不可追认；本轮不能补造历史。
4. 真实 GPUI/IME、系统字素、跨平台和性能仍未验证。

Reviewer 还直接核对了 R01/R02/R03/R04 修复前失败原始输出：包括 R01 先编译失败、随后真实行为失败；R03 半字符时旧路径 `Some(4)`、新路径 `None`；R04 两条真实宿主保护回归在修复前失败。此处只引用历史证据，不把历史日志称为本轮运行。reviewer 的 51-row 复算摘要和 14-row R08 摘要见其最终复核原文；主 Agent将本轮新 review 事实与原日志路径一并纳入 findings 表。

审查范围不含守卫/文档/历史事实最终复核；其余 R14 reviewer 结论见本目录其它 `*-review.md`。本 review 不单独裁决 M0/M1。
