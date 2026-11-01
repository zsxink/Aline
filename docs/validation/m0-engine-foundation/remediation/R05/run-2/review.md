# R05b 独立复核（run-2）

复核对象：`814148e`（R05b 修复）与 `e4a6af0`（R05 原始）；未采信 report/handoff/post-fix 的结论。
环境：python3 3.9.6；cargo/rustc 1.98.1；`CARGO_NET_OFFLINE=true`、`--locked --offline`、`CARGO_HOME=<cargo-home>`。
自建夹具：`<tmp>/r05brev/{f1,f2,q4,q5,q7}`（不复用 r05f4/r05main/weakrepo/brief 夹具），复核后清理。

## A 结论

**REJECT** —— 撞名判断只在「同一展平键同时存在真实表」时生效：真实 `[workspace.lints.rust]` + `[workspace]` 下引号 `"lints.clippy"`（无真实 clippy 表）时新脚本仍 exit 0，而 Cargo 仅报 `unused manifest key: workspace.lints.clippy`、clippy deny 实际未生效；这正是任务书 Q1 要排除的 F-08 同类静默 fail-open。

## B 逐问题结论与自建证据

### 1. 撞名 fail-open：仅部分修复
- F1（真实弱 rust/clippy 表 + `[workspace]` 引号 `"lints.rust"`/`"lints.clippy"` 强值）：old script exit 0；new exit 1，detail 为 `<tmp>/r05brev/f1/Cargo.toml:19: ambiguous-table-path: 表路径 ('workspace','lints','rust')（第 1 行）与 ('workspace','lints.rust')（第 19 行） 展平为同一键 'workspace.lints.rust'...`；Cargo check exit 0 且报两条 `unused manifest key`。此窗口确已关闭。
- F2（真实 `[workspace.lints.rust]` 第 1 行 + `[workspace]` 下引号 `"lints.clippy"` 第 8 行，无真实 clippy 表）：old exit 0，**new 仍 exit 0**；Cargo check exit 0，仅 `warning: Cargo.toml: unused manifest key: workspace.lints.clippy`；`cargo clippy -p aline-buffer --locked --offline -- -D warnings` 对源码里的 `value.unwrap()` exit 0（deny 未生效）。对称夹具（真实 clippy + 引号 `"lints.rust"`）含 `unsafe { 1 }`，Cargo check exit 0 且仅报 `unused manifest key: workspace.lints.rust`（`unsafe_code=forbid` 未生效）。Q1 的「已有 rust 表 + 引号 clippy 点分键」形态未过。

### 2. 键集/类型/值拒绝路径的断言
- 读码确认：`TABLE_PATH_CASES` 2 条拒绝均带手写 `line/paths/key`；`MEMBER_CASES` 4 条拒绝分别覆盖多余键、`false`、`"true"`、`1`；`run_member_self_checks` 断言具体 needle，不只是 exit code。
- 变异 rev6/rev7/rev8 分别把期望行号 4→5、期望路径改成 clippy 元组、期望展平键改成 `workspace.lints.clippy`，均 exit 1 并命中 reject-table-then-quoted-dotted-key 的具体 detail（见 C）。断言有效。

### 3. `_assert_rejection_detail` 是否真的有效
- rev1–rev5 五种实现单点变异全部 exit 1、命中预期 case、0 traceback：去 `file:line` 前缀、headline 用首行、第二条路径打印成首条、成员错误去 name、`is not True`→`!= True`（见 C）。
- 副本均为 `scripts/_revN.py`，每次跑完立即 `rm -f`；结束 `git status --porcelain scripts/` 为空。结论：补强断言确实会红。

### 4. 成员继承反例与判据
- v1（`[lints] workspace=true` + `[lints.clippy] unwrap_used="allow"`）：Cargo check/clippy 均 exit 101，`cannot override workspace.lints in lints`；守卫 exit 1。
- v2（不继承、手写 `[lints.clippy] unwrap_used="allow"`）：Cargo 接受，clippy 对 unwrap exit 0；守卫 exit 1。守卫拒绝的是「未继承」，符合本仓库全成员继承策略，不是误报。
- v3 `workspace=false` → Cargo 101 `` `workspace` cannot be false ``；v4 `"true"`、v5 `1` → Cargo 类型错误；v6 `extra=1` → Cargo `expected a map`；守卫全部 exit 1。依据 Cargo.toml 语义，不存在「继承 + 覆盖」的合法形态，守卫的 `{workspace:true}` 严格判据与 Cargo 一致。

### 5. 深嵌套
- 30 层数组 exit 0（低于上限 100，无 traceback）；101/500 层数组、500 层内联表均 exit 1，`数组/内联表嵌套深度超过上限 100`；110 段点分键 exit 1，`键/表路径深度 110 超过上限 100`。
- 旧脚本 500 层数组/内联表各 1 处 `RecursionError` traceback，新脚本 0 处；`sys.setrecursionlimit(60)` 下 90 层输入 new exit 1、`lint 检查拒绝: maximum recursion depth exceeded`、无 traceback（兜底生效）。

### 6. 正常仓库无回归
- old/new `--root <repo>` 均 exit 0，JSON 逐字节相同。
- old self-test：exit 0，57 符合/0 不符，`parser_cases 23`；new：exit 0，68 符合/0 不符，`parser 25 / table_path 4 / member 5`。
- 规则集合 old 全部保留，new 只新增 `ambiguous-table-path` 与两条深度规则（更严）；failures=0。

### 7. 残留诚实性
- 「`RecursionError` 兜底无 self-test 断言」属实：脚本仅在 `main` 异常元组含它（`scripts/check_lints.py:1164`），无 `setrecursionlimit` 用例。
- 「仅引号键、无真实嵌套表」quoted-only：守卫 exit 0，Cargo exit 101 `` `workspace.lints` was not defined ``，属实。
- 但 report §5.1「危险窗口仅限真实弱表+引号键覆盖」不成立：只要有任一真实 lints 表使 `workspace.lints` 已定义，另一张仅引号表就被 Cargo 静默忽略，Cargo 不再 101（D1）。

## C 变异记录（全部 exit 1、0 traceback）

| 变异（单点） | 变红 case / 具体信息 |
|---|---|
| rev1 消息去掉 `file:line` 前缀 | reject-table-then-quoted-dotted-key：`表路径自检错误缺少文件:行号` |
| rev2 headline 用 first_line | 同上：`行号不符预期 期望第 4 行，实得第 1 行` |
| rev3 第二条路径打印成第一条 | 同上：`未列出冲突路径 ('workspace','lints.rust')` |
| rev4 成员错误省略 name | negative-one-member-inheritance-guard 不符合预期 |
| rev5 `is not True`→`!= True` | reject-workspace-integer-one：`应拒绝但通过了` |
| rev6 期望行号 4→5 | 同上：`期望第 5 行，实得第 4 行` |
| rev7 期望路径改为 clippy 元组 | 同上：`未列出冲突路径 ('workspace','lints','clippy')` |
| rev8 期望展平键改为 clippy | 同上：`未列出展平键 'workspace.lints.clippy'` |

补充：把 `detail["paths"]` 改成展平键拼写后 self-test 仍绿（子串匹配），属断言强度残留，列入 E。

## D 缺陷

1. **[严重] 混合引号键 fail-open（D1）**：`[workspace.lints.rust]` 真实存在、clippy 仅以 `[workspace]` 下引号 `"lints.clippy"` 给出时，`_flatten_tables` 只登记 `workspace.lints.clippy`，无真实 clippy 表可撞名，`audit_manifests`（`scripts/check_lints.py:697-704`）读到强值并通过；Cargo 却把该键当字面键（`unused manifest key`），clippy deny 不生效。实测 old/new 均 exit 0，`unwrap()` 在 `-D warnings` 下仍 exit 0；对称的引号 rust 键可让 `unsafe_code=forbid` 失效。任务书 Q1 要求的输出（新脚本非 0 + 定位信息）未达成。
2. **[一般] self-test 对含空格的输出目录误报**：`--output-dir "<tmp>/r05brev/space dir"` 时 exit 1，`表路径自检错误缺少文件:行号`（已过 61 条后中止），实现本身无错。根因 `_assert_rejection_detail` 的 `head = message.split(" ", 1)[0]`（`scripts/check_lints.py:973`）按空格截断路径；应改为正则 `^(.*):(\d+):` 或按最后一个 `:\d+:` 解析。仅影响 self-test，不影响运行时守卫。

## E 残留与需主 Agent 裁决项

1. D1 必须修复或由主 Agent 明确接受为已知风险。建议：对 `workspace.lints.rust`/`workspace.lints.clippy` 这两个策略路径，若内容由引号点分键提供而无真实嵌套表，直接拒绝（专用规则或 `ambiguous-table-path`），无关键仍放行，以免误伤 `[package] "a.b" = 1`。
2. Q5 字面要求「≥30 层须命名规则 + 非零」与 `MAX_NESTING_DEPTH=100` 冲突：30–100 层实测 exit 0（无 traceback）。若 30 层必须拒绝，需调阈值；若「≥30」只是测试下限，500 层夹具已满足，建议在报告中写明。原缺陷阈值为 498，100 为策略值。
3. `RecursionError` 兜底无断言是已声明残留；外部低递归限制实测生效，可接受，或补一条不依赖深度上限的断言。
4. report §5.1 对边界窗口的表述需按 D1 更正；m5 一类「期望值被改写成展平拼写」的子串断言弱点建议改为结构化比较。

## F 未覆盖范围复核

- 已核实：`RecursionError` 无断言、quoted-only 由 Cargo 101 兜底均属实；两者均非静默。
- 未声明盲区：D1 的「一张真实表 + 一张引号表」组合；它不属于「真实弱表+引号覆盖」，也不是「仅引号键」，是第三种、且 Cargo 正常退出、策略静默失效的组合。
- 边界澄清：`"clippy.unwrap_used"` 这类只展平成 `workspace.clippy.unwrap_used` 的引号键不会命中守卫查找的 `workspace.lints.clippy`，守卫会因缺表拒绝；能造成 fail-open 的只有展平后恰好等于 `workspace.lints.{rust,clippy}` 的引号键。语料 9488 文件未复跑（超出本轮七问），根清单、八成员与 self-test 均无回归。
