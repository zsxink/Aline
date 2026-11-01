# R05 独立复核裁决（F-08 内置 TOML 解析器 / fail-closed）

- 复核对象：`e4a6af0`，证据 `docs/validation/m0-engine-foundation/remediation/R05/run-1/`
- 复核者：独立 reviewer（未参与 R05 实现），主 Agent 另行逐条复核
- 裁决：**REQUEST_CHANGES**（2 项严重，均指向 F-08 同类「静默误放行」）

## A. 规格合规
- 达标：真实 TOML 解析替代正则；零外部依赖；成员 `{"workspace": true}` 严格校验；self-test 增补（parser_cases 23 → 含 13 条反例）；正常仓库不回归。
- **[严重] 1 — 引号点分键与真实嵌套表撞名**：`_flatten_tables`（`scripts/check_lints.py:604-609`）用 `".".join(path)` 生成键，**不区分** `[workspace.lints.rust]` 表与 `[workspace]` 下引号点分键 `"lints.rust" = {...}`；读取处 `:641` 因此可能拿到后写入者。守卫 exit 0，而 Cargo 只报 `unused manifest key: workspace.lints.rust`——即守卫认为策略已生效、Cargo 实际未生效，与 F-08 同类的**静默误放行**。
  **主 Agent 已独立复现**（`<tmp>/r05f4`）：根清单同时含弱表（`unsafe_code="warn"`、`missing_docs="allow"`、`unwrap_used="warn"`）与引号点分键（强策略），`python3 scripts/check_lints.py --root` → **exit 0「workspace lint 声明有效」**；`cargo clippy -p aline-buffer --offline -- -D warnings` → `warning: Cargo.toml: unused manifest key: workspace.lints.rust` + `workspace.lints.clippy`。**结论成立**。
  补充边界（主 Agent 实测）：若**只有**引号点分键、无任何真实嵌套表，成员 `[lints] workspace = true` 会因 Cargo 侧 `workspace.lints was not defined` 而报错（本机 clippy exit 101），此时不可静默；**危险窗口是「真实弱表 + 引号点分键覆盖」这一组合**。

## B. 独立复跑（全部实跑）
- 正常仓库 → exit 0
- decoy 项目 → exit 1，stderr「成员未完整继承 workspace lint: aline-buffer」
- real 项目 → exit 0
- `--self-test` → exit 0，`parser_cases = 23`，符合预期 57 / 不符 0
- Cargo 对照：decoy exit 0；real exit 101，`missing documentation for the crate`

## C. fail-closed
- 自造 6 例（多行伪继承、多余键、`workspace=false`、`"true"`、`1`、缺表）全部 exit 1
- `_strict_equal` 实测 `True ≠ 1`、`1 ≠ True`
- `a="\x41"`、`t=07:32` 均 REJECT（过度拒绝，方向安全）；另测多行内联表被接受（TOML 1.0 非法，非放行风险）

## D. 反「照实现写测试」
- 10 正 / 13 反；外科式删掉 dotted 拒绝后 self-test 立即变红，该用例确为有效断言
- **[严重] 2 — 键集/类型/值三条拒绝路径无断言**：把 `audit_member_manifest` 弱化为 `isinstance(lints, dict)`，整套 `--self-test` 仍 exit 0；extra-key 清单被弱化守卫放行；仅「继承缺失」被覆盖。
  **主 Agent 已独立复现**（`<tmp>/weakrepo`）：仅把 `:636` 的 `set(lints) != {"workspace"} or lints["workspace"] is not True` 换成 `not isinstance(lints, dict)`，在仓库副本上跑 `--self-test` → **exit 0**，输出 `"status": "lint 隔离样例通过"`、`inheritance_cases: 4`、`parser_cases: 23`。**结论成立**：self-test 对「多余键 / `workspace=false` / 非 bool 值」三类拒绝没有独立用例，弱化实现不会被发现。
- **[一般] 3**：`check_expected` 把 `clippy::unwrap_used` 换成 `unwrap-used`，实际只在 `-D clippy::unwrap-used` 提示行命中（既有代码，非本批引入）

## E. 其他
- 无 `eval(` / `exec(` / 网络；`subprocess` 仅 argv 列表、无 shell
- **[一般] 4**：嵌套数组/内联表 ≥498 层抛 `RecursionError`，逃出 main 的异常元组（`:975`），打印 traceback 而非 `CheckError`；深点分键 flatten ≥2000 层同样。exit 1 仍 fail-closed
- 10MB 清单解析 2.12s；错误信息仅「路径:行号」；语料 9471 文件 0 失败；差分 81 条一致 79（2 条 fail-closed，与 report 相符）

## 主 Agent 裁决
- **不予通过**。两项严重发现都已由主 Agent 独立复现，均属 F-08「守卫与 Cargo 判断不一致 → 静默放行」的同类问题，与整改目标直接冲突。
- 修复方向（不改脚本所有权归属，仍在 R05 范围）：
  1. `_flatten_tables` 必须**区分**表头路径与点分键路径；遇到引号键或点分键把已有表路径覆盖时，按 fail-closed 拒绝（`CheckError`），而不是静默覆盖。真实仓库不含此构造，拒绝不影响正常路径。
  2. 为「多余键 / `workspace=false` / 非 bool 值」补 3 条 `--self-test` 反例，使弱化实现必然变红。
  3. `[一般] 4` 把 `RecursionError` 纳入 main 的异常元组（或设深度上限后报 `CheckError`）。
- 按批次规则，修复后须重跑 self-test 并再次由非作者复核。
