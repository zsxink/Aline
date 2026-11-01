# R09 独立复核契约（reviewer 必读）

## 身份要求

你**没有**参与 R09 的任何实现或期望文件编写。本批由主 Agent 实现并自验，
因此你的任务是独立复核，**不得以主 Agent 的 report.md / handoff.md 结论为依据**，
必须自己重跑命令、自己核对字节。

## 复核对象

- 提交：`72c8bc6 feat(aline-markdown): dump 追加绘制文字、逐片段来源与源码身份`
- 代码：`crates/aline-markdown/src/parse.rs`、`crates/aline-markdown/src/mapping/projection.rs`
- 测试：`crates/aline-markdown/tests/dump.rs`（新增）
- 证据：`docs/validation/m0-engine-foundation/remediation/R09/run-1/`
  （`pre-fix.txt`、`post-fix.txt`、`basic.dump.expected.txt`、`derivation.md`、
  `commands.txt`、`report.md`、`handoff.md`）
- 需求来源：`docs/maintenance/m0-remediation/tasks.md` 中 R09 行（F-07 / E3 同夹具可审查 dump）

## 必须独立回答的问题

1. **F-07 是否真的被关闭**：`git show 72c8bc6^:crates/aline-markdown/src/parse.rs` 的旧 dump
   能否区分「块范围与总长度相同但来源映射不同」的两份源码？新 dump 能否区分？请自己造两份
   同长度、同块范围的源码实测，不要只看 `pre-fix.txt`。
2. **只增不改**：`source <len>` 与 `drawn <len>` 前缀是否字节级保留？block / gap / protected /
   style / diagnostic 各行是否与旧版逐字节相同？请用 `git diff 72c8bc6^ 72c8bc6 -- crates/` 自己判断，
   并实跑 `cargo test -p aline-markdown` 确认既有断言未被削弱。
3. **期望文件的独立性**：`basic.dump.expected.txt` 与 `tests/dump.rs` 的期望值是否**能**从
   fixture 原始字节 + `pulldown-cmark` 事件 + M0 规则独立重建？请至少手工重算 5 行
   `segment`（含 1 条 Hidden、1 条 Synthetic、1 条 Aligned）与 FNV-1a 摘要，
   用 `od -c` / 独立 python 脚本核对，不得调用 `aline-markdown`。
4. **裁决第 7 节声明的唯一关注点**：期望文件是「实现已能输出扩展 dump之后」才落盘的。
   这算不算不可接受的「照实现写测试」？请给出你的独立判断与理由，不要复述主 Agent 的自我评价。
5. **dump 新增行是否可能破坏既有消费方**：`tests/spec.rs`、`tests/diagnostics.rs` 的三处 dump 断言、
   `tests/hostile.rs:336` 确定性断言。特别是 `diagnostics.rs` 的 `!dump.contains("CodeBlock")` /
   `!dump.contains("Table")` —— 新增的 `segment` 行只输出四个 precision 名，是否仍然安全？
6. **`source_identity()` 的正确性与边界**：FNV-1a 实现是否真的按字节逐个处理？
   `iter_chunks()` 失败分支返回 `None` → dump 打印 `unavailable`，这条路径是否可能被静默绕过？
   摘要是否被误用为安全/去重依据（看文档注释与代码调用点）？
7. **新测试是否真的会变红**：请做至少 3 处临时变异（例如把 `as_str()` 某分支返回值改错、
   把某个 segment 的 source 起点偏移 1、把 FNV 素数改掉），确认对应测试失败（exit 101），
   **变异后必须还原并核对 sha256 未变**。

## 硬性约束

- 工具链 1.98.1，所有 cargo 命令带 `--locked --offline` 且在 `--` 之前；
  需要解析依赖时用 `CARGO_HOME=/Users/xian/.cargo`（隔离 CARGO_HOME 注册表为空会 exit 101）。
- 不得联网、不得安装工具；无法核实的事项写「未核实」，不要猜。
- **不得修改任何被复核代码或测试**（临时变异必须还原并核对 sha256）。
- 不得 `git reset`、不得回退他人改动、**不得 commit**。
- 不要派生子 agent。
- 你的产出只写一个文件：`docs/validation/m0-engine-foundation/remediation/R09/run-1/review.md`
  （若该文件已存在则用 `review-run2.md`，不得覆盖）。≤120 行，含：
  A. 规格合规（达标 / 未达标逐条）；B. 你实跑的每条命令 argv/cwd/起止 UTC/exit/关键原始输出；
  C. 独立重算的 5 行 segment 与 FNV 值及比对结果；D. 变异自证表；
  E. 你发现的 finding（按严重度，标 [严重]/[一般]）；F. 裁决
  （APPROVE / APPROVE_WITH_CONCERNS / REQUEST_CHANGES）与理由。
- 返回给主 Agent 的正文 ≤800 汉字：只给裁决、finding 条数与最重一条、以及证据文件路径。

## 特别注意

主 Agent 已在 `report.md` §4 记录「推导脚本自身出过两处错误（DisplayMath/FootnoteReference
应按源码切片绘制；表格 tab 计数器整表累计）」。这两处**是推导方的错误，不是实现的**。
你要自己验证这个说法是否成立——如果实现的行为与这两条规则不一致，那就是实现缺陷。
