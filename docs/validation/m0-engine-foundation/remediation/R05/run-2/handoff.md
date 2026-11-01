# R05b 交接（≤80 行）

## 状态
三项复核退回全部修复，未提交（按契约由主 Agent 串行提交）。

## 改动
- `scripts/check_lints.py`（唯一代码改动，+165/−15）
- `docs/validation/m0-engine-foundation/remediation/R05/run-2/`：
  `pre-fix.txt`、`post-fix.txt`、`commands.txt`、`report.md`、`handoff.md`（本文件）

未触碰：`scripts/check_safety.py`、`scripts/check_workspace.py`、任何 `Cargo.toml`、
任何 `crates/**`、`R05/run-1/**`。未 `git reset`，未回退他人改动。

## 修复内容（三处，均在 check_lints.py）
1. `_flatten_tables` 改为按**路径元组**登记（`seen[joined] = (path, line)`），
   展平键相同但路径元组不同即抛 `CheckError`，规则名 `ambiguous-table-path`，
   信息含 `file:line` 与两条冲突路径。不按写入顺序选赢家。行号由新增的
   `_TomlParser.lines`（`id(dict) → 起始行`）提供。
2. 新增 `PARSER_CASES` 2 条（深嵌套数组、深点分键）、`TABLE_PATH_CASES` 4 条
   （2 正 2 反）、`MEMBER_CASES` 5 条（1 正 4 反：多余键 / `false` / `"true"` / `1`），
   以及 `run_table_path_self_checks` / `run_member_self_checks`，接入 `run_self_test`，
   JSON 增加 `table_path_cases` / `member_cases` 字段。
3. 新增 `MAX_NESTING_DEPTH = 100`：`_enter_container`（值容器）、`_check_path_depth`
   （键/表路径）显式拒绝；`main` 异常元组追加 `RecursionError` 兜底。

## 验证（全绿；原始捕获在 /tmp/r05b/evidence/）
- 正常仓库 `python3 scripts/check_lints.py` → exit 0，八成员不变。
- `--self-test` → exit 0，**68 条符合预期 / 0 条不符**（基线 57 一条未减 + 新增 11），
  `parser_cases 25, table_path_cases 4, member_cases 5`。
- 撞名夹具：修复前 exit 0 静默（Cargo 仅 `unused manifest key`）→ 修复后 exit 1，
  `ambiguous-table-path` + `Cargo.toml:11` + 两条路径。
- 深嵌套：修复前 traceback → 修复后 `数组/内联表嵌套深度超过上限 100`，exit 1；
  `setrecursionlimit(60)` 下 `lint 检查拒绝: maximum recursion depth exceeded...`，return 1。
- 弱化自证（5 个副本，均变红）：整条判据弱化→`reject-extra-key`；只删键集→`reject-extra-key`；
  只删值真值→`reject-workspace-false`；删撞名分支→`reject-table-then-quoted-dotted-key`；
  深度判断改 pass→`reject-deep-nested-array`；去掉 RecursionError→退回 Traceback。
- 语料回归：9488 个 Cargo.toml/Cargo.lock，0 拒绝，最大容器深度 5。

## 提交建议
`fix(workspace): lint 守卫拒绝表路径撞名并补齐继承反例`
（纯工程脚本，scope `workspace`；提交时只 add 上述 6 个文件）

## 待复核 / concerns
1. **基线差异**：契约写基线 `128954e`；本批开工时工作树 HEAD 是 `74b876e`
   （`128954e` 之上的纯文档提交，代码内容一致）。收尾时 HEAD 已随他人串行提交前移到
   `63b08eb`，未触碰本批任何文件。
2. **已知边界**：只有引号点分键、无真实嵌套表时，守卫仍 exit 0，但 Cargo exit 101
   `` `workspace.lints` was not defined ``，不静默。本批只关闭「真实弱表 + 引号键覆盖」
   这一危险窗口，未额外拒绝单引号键形态（避免误伤 `[package]` 下 `"a.b" = 1` 这类真实写法）。
3. **深度上限 100 是策略值**（真实语料最大 5）。若未来合法清单超过 100 层会被误拒，
   阈值是单一常量，便于调整。
4. **Cargo 结论只在本机 1.98.1 实测**；Cargo 对照统一 `CARGO_HOME=/Users/xian/.cargo`
   （默认指向 admission 隔离目录、registry 为空），理由写在 `commands.txt`。
5. 本批零 Rust 改动，未跑 `cargo test` / `cargo build`。

## 主 Agent 复核后追加（2026-11-01）
- 复核独立复现：三项修复行为成立；但 self-test 对报错信息要件无断言（行号写死、
  路径/键被删仍绿），已补 `_assert_rejection_detail` + 手写 detail，九种变异现全部变红。
- self-test 仍 68 条 / exit 0，正常仓库 exit 0；证据追加在 `post-fix.txt`「[追加]」段，
  说明追加在 `report.md` §7。上文 worker 原文未改。
- 残留：`RecursionError` 兜底无 self-test 断言，靠外部观测量级。
