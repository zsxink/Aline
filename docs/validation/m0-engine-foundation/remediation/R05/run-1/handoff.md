# R05 handoff

- 批次：R05 / run-1（F-08 lint 继承必须真实解析）
- 状态：DONE（未提交，按协调者要求停手，由主 Agent 串行提交）
- 代码改动：仅 `scripts/check_lints.py`
  - sha256 `497923a5bb62c45a12d2a309d2e99fa1f234e6f0db14991f93cc64fea5745ec9`
  - 979 行
- 证据：`docs/validation/m0-engine-foundation/remediation/R05/run-1/`
  - `pre-fix.txt`、`post-fix.txt`、`cargo-control.txt`、`commands.txt`、本文件

## 一行测试小结

decoy exit 1、real exit 0、正常仓库 exit 0、`--self-test` exit 0（30→56 条断言，parser_cases=22）。

## 结论

- 逐行扫描换成内置递归下降 TOML 解析器（仅标准库，Python 3.9.6 无 tomllib）。
- 成员 `[lints]` 必须恰好 `{"workspace": true}`；根 lints 与 EXPECTED_* 严格全等。
- 无法安全解释的构造抛 `CheckError`，fail-closed。

## concerns

1. 报告文件 `report.md` 未能落盘：harness 拦截 subagent 写 report/summary 类 .md，
   需主 Agent 落盘。正文已在回报中给出。
2. 与 Cargo 的 `toml 1.1.8` crate 差分：81 条一致 79 条。差异 2 条均为 fail-closed
   （内联表尾随逗号、BOM）。
3. 未覆盖构造 12 条（见 report 第 5 节）；其中 TOML 1.1 的 `\e`/`\x` 转义与
   可省略秒的时间字面量会导致过度拒绝，不会误放行。
4. 仓库语料 9464 个 Cargo 清单零解析失败。
5. 未运行 workspace 级 cargo test/build（R02 中间态）。

## 勘误（追加，不改原文）

1. **指纹**：正文所记 `497923a5…` / `12:19:41Z` 是写 evidence 时的中间态；
   定稿为 sha256 `4ed5a4971d2a9ca8e866b6672206e456e23cb47f29cd2fb7f598050bd4a6b929`
   / mtime `2026-11-01T12:21:57Z` / 981 行。
2. **为何写完 evidence 后仍改动**：正文第 4 节称「已加 dotted-key 修正并纳入自检
   `reject-dotted-key-then-header`」，但落盘时该用例**实际不存在**——修正只在代码里
   （`_insert` 标记 `dotted`、`_define_table` 拒绝），无断言守护，属「修了但会静默退回」。
   现已真正加入该用例。
3. **parser_cases**：22 → **23**；`--self-test` exit 0，0 条「不符合预期」，
   「符合预期」计数 57。新增用例输出：
   `parser reject-dotted-key-then-header: 拒绝符合预期 (… 点分键已定义表 'a'，不得再用表头定义)`
4. **第 5 节第 3、6 条已补实测**（`post-fix.txt` 第 [7] 段）：`\x41`/`\e` 与
   可省略秒的时间在 TOML 1.1 合法、本解析器拒绝 → 过度拒绝，fail-closed。
5. **终版复跑（自跑）**：`--self-test` exit 0；正常仓库 exit 0；decoy exit 1；
   real-inheritance exit 0；语料 9464 个清单 0 失败；差分 81 条一致 79（差异 2 条 fail-closed）。
