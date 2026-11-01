# R05b 修复报告：`scripts/check_lints.py`

基线：HEAD `74b876e`（在 `128954e` 之上的纯文档提交），本批是 R05（`e4a6af0`）的修复轮。
唯一改动文件：`scripts/check_lints.py`（+165 / −15）。R05 既有成果（真实 TOML 解析、
零外部依赖、`--self-test` 57/0）全部保留。

## 1. 根因

### 严重 1：展平键冲突

`_flatten_tables` 以 `".".join(path)` 作为输出键：

```python
path = prefix + (key,)
joined = ".".join(path)
out[joined] = value          # 后写入者覆盖先写入者
```

- 表头 `[workspace.lints.rust]` → 路径元组 `("workspace", "lints", "rust")` → 键 `workspace.lints.rust`
- `[workspace]` 下的引号点分键 `"lints.rust" = {...}` → 路径元组 `("workspace", "lints.rust")` → **同一个键**

两条路径在 TOML 里是不同结构，在 Cargo 里语义完全不同（引号点分键落在 `[workspace]`
表下，只产生 `unused manifest key`），但守卫只看展平键，取到的是字典遍历顺序上后写入的那份。
因此「真实弱表 + 后写入的强策略引号键」会让守卫读到强策略并 exit 0，而 Cargo 侧完全没生效——
与 F-08 同一类静默 fail-open。

### 严重 2：拒绝路径无断言

`audit_member_manifest` 的判据是

```python
if not isinstance(lints, dict) or set(lints) != {"workspace"} or lints["workspace"] is not True:
```

三个子条件（必须是表、键集恰好 `{"workspace"}`、值必须是布尔真）此前没有任何用例
分别触发，整条判据被弱化成 `not isinstance(lints, dict)` 后 self-test 仍全绿。

### 一般 4：深嵌套逃出异常元组

`_parse_array` / `_parse_inline_table` 用递归下降，嵌套 ≥498 层即抛 `RecursionError`；
`main` 的 `except (CheckError, OSError, ValueError, KeyError)` 不含它，于是打印 traceback。
exit 1 仍是 fail-closed，但输出不是规范拒绝信息，且错误信息不可读。

## 2. 规则设计

### 2.1 `ambiguous-table-path`（撞名 fail-closed）

```python
seen[joined] = (path, lines.get(id(value), 0))       # 记住首次出现的**路径元组**与行号
previous = seen.get(joined)
if previous is not None and previous[0] != path:     # 展平键相同、路径元组不同
    raise CheckError("%s:%d: ambiguous-table-path: 表路径 %r（第 %d 行）与 %r（第 %d 行）"
                     " 展平为同一键 %r，Cargo 语义与守卫判定不一致" % ...)
```

设计要点：

1. **比较路径元组而不是展平字符串**。这正是缺陷的根源，元组能区分 `("workspace","lints","rust")`
   与 `("workspace","lints.rust")`。
2. **不按写入顺序裁决**。契约明确禁止「取第一个/取最后一个」；只要两条不同路径展平成同一键
   就拒绝，因此先后顺序不影响结论（两条 reject 用例分别覆盖「表在前」「键在前」）。
3. **方向无关**：同一分支同时覆盖「表头覆盖引号键」与「引号键覆盖表头」。
4. **行号可定位**：`_TomlParser` 新增 `self.lines`（`id(dict) → 起始行`），在解析时对每个
   表/内联表/点分键子树记录起始行；报错给出冲突的第二处 `file:line` 与第一处的行号。
5. **正常路径不受影响**：真实仓库不含该构造，`accept-quoted-dotted-key-without-collision`
   与 `accept-table-and-distinct-quoted-key` 证明「引号点分键本身合法、展平键不冲突时照常通过」。
6. **已知边界（契约要求写明）**：若**只有**引号点分键、没有任何真实嵌套表，Cargo 侧会
   报 `` `workspace.lints` was not defined ``（exit 101），**不可静默**；危险窗口仅限
   「真实弱表 + 引号点分键覆盖」。因此本批只关闭该窗口，不额外拒绝单引号键形态，
   以免误伤真实 TOML（例如 `[package]` 下的 `"a.b" = 1`）。

### 2.2 `MAX_NESTING_DEPTH = 100` + `RecursionError` 兜底

两条独立防线，顺序为「显式上限先命中，递归元组兜底」：

- 值容器：`_enter_container(parse)` 在进入数组/内联表前检查 `self.depth`，超限即 `_fail`；
  `finally` 里回退，异常路径不污染深度。
- 键/表路径：`_check_path_depth(keys, pos)` 在表头、点分键赋值、内联表键解析处调用，
  路径长度 > 100 即 `_fail`。这一条覆盖「不递归但路径极长」的形态。
- `main` 的异常元组追加 `RecursionError`，作为上限之外的兜底（上限只在解析器控制的
  路径上生效；解释器递归栈仍可能被其它构造耗尽）。

上限取 100 的依据：真实语料最大容器深度为 5（见 §4），100 留出 20 倍余量；
同时 100 层递归远低于 CPython 默认 1000 的递归上限，显式拒绝一定先于 `RecursionError` 发生。

### 2.3 三条成员继承反例

新增 `MEMBER_CASES` 5 条（1 正 4 反）。期望值**先手写**（按 Cargo 的 `[lints] workspace`
继承语义：键集必须恰好是 `{"workspace"}`，值必须严格为布尔 `true`），再跑实现对照，
没有用实现输出当预期。反例：

| 用例 | 构造 | 触发的子条件 |
|------|------|--------------|
| `reject-extra-key` | `[lints] workspace = true` + `extra = 1` | 键集 |
| `reject-workspace-false` | `workspace = false` | 值真值 |
| `reject-workspace-string-true` | `workspace = "true"` | 值类型 + 真值（`"true" is not True`） |
| `reject-workspace-integer-one` | `workspace = 1` | 值类型 + 真值（判据用 `is not True`，`1 is not True` 成立） |

## 3. 逐条断言对照（自证有效性）

「弱化实现 → 对应断言必须变红」逐项实测，副本在 `/tmp/r05b/` 下，均不提交：

| 弱化方式 | 变红的断言 | 实测结果 |
|----------|-----------|----------|
| `audit_member_manifest` 整条换成 `not isinstance(lints, dict)` | `reject-extra-key`（最先命中） | exit 1；修复前同形弱化 exit 0 |
| 只删 `set(lints) != {"workspace"}` | `reject-extra-key` | exit 1 |
| 只删 `lints["workspace"] is not True` | `reject-workspace-false` | exit 1 |
| 删 `_flatten_tables` 的撞名分支 | `reject-table-then-quoted-dotted-key` | exit 1；两条 accept 仍通过 |
| `_check_path_depth` 与 `_enter_container` 判断改 `pass` | `reject-deep-nested-array` | exit 1；既有 57 条仍全过 |
| 从异常元组去掉 `RecursionError` | 无对应断言，直接观测量级 | `setrecursionlimit(60)` 下立刻退回 Traceback（副本 /tmp/r05b/weak-recerr） |

逐用例矩阵（直接调用 `audit_member_manifest`，不依赖 cargo）：

| 实现 | accept-exact | reject-extra | reject-false | reject-"true" | reject-1 |
|------|--------------|--------------|--------------|---------------|----------|
| 修复版 | PASS | PASS | PASS | PASS | PASS |
| 全弱化 | PASS | FAIL | FAIL | FAIL | FAIL |
| 无键集检查 | PASS | FAIL | PASS | PASS | PASS |
| 无值真值检查 | PASS | PASS | FAIL | FAIL | FAIL |

结论：三条拒绝各有独立断言，任一单点弱化都会命中至少一条；`accept` 用例在弱化下仍通过，
说明断言没有靠「一律拒绝」蒙对。

断言总数：57（基线，一条未减）+ 2（parser 深嵌套）+ 4（table-path）+ 5（member）= **68**。

## 4. 验证结果

| 项 | 结果 |
|----|------|
| 撞名夹具 | 修复前 exit 0 静默；修复后 exit 1，`ambiguous-table-path` + 文件:行号 + 两条路径 |
| 撞名夹具 Cargo 对照 | exit 0 + `unused manifest key: workspace.lints.{rust,clippy}`（Cargo 侧确实未生效） |
| 深嵌套夹具 | 修复前 traceback；修复后 `数组/内联表嵌套深度超过上限 100`，exit 1 |
| `RecursionError` 兜底 | `setrecursionlimit(60)` 下 `lint 检查拒绝: maximum recursion depth exceeded in comparison`，return 1，无 traceback |
| 正常仓库 | `python3 scripts/check_lints.py` exit 0，八成员 JSON 不变 |
| `--self-test` | exit 0；68 条「符合预期」、0 条「不符合预期」、无 traceback |
| 语料回归 | 9488 个 `Cargo.toml`/`Cargo.lock` 经 `tables()`，0 拒绝，最大容器深度 5 |
| 已知边界（仅引号键） | 守卫 exit 0；Cargo exit 101 `` `workspace.lints` was not defined ``（不静默） |

## 5. 未覆盖范围 / 已知限制

1. **仅引号点分键形态不拒绝**（见 §2.1 第 6 点）。守卫在此形态下仍 exit 0，由 Cargo
   报错兜住。若未来出现「守卫通过、Cargo 也通过但语义不符」的第三种组合，需重新评估。
2. **撞名检测只覆盖表（dict）节点**。标量值的点分键（`a.b = 1` 与 `a = {b = 1}`）由
   既有解析器的「点分键已定义表」「内联表不可再扩展」规则处理，不在本次新规则范围内。
3. **深度上限是策略值 100，不是 Cargo 语义**。真实语料最大深度 5；若将来出现 >100 层
   的合法清单，会误拒。这是有意的 fail-closed 取舍，阈值集中在一个常量便于调整。
4. **未验证 Cargo 1.98.1 之外的版本行为**。`unused manifest key` 与
   `` `workspace.lints` was not defined `` 两条结论都只在本机 1.98.1 上实测。
5. **未运行 Rust 构建/测试**。本批零 Rust 改动；`--self-test` 内部已跑 clippy 的
   debug/release 双 profile 继承用例（沿用 R05 既有机制）。
6. **`CARGO_HOME` 覆盖**：本机默认指向 admission 隔离目录（空 registry），故 Cargo
   对照显式用 `CARGO_HOME=/Users/xian/.cargo`，仍 `--locked --offline`；这是环境规避，
   不是产品行为，记录在 `commands.txt`。

## 6. 证据索引

- `pre-fix.txt` —— 三项缺陷修复前的实际输出
- `post-fix.txt` —— 修复后各命令输出与弱化自证
- `commands.txt` —— argv / cwd / 起止 UTC / exit code / 环境
- 原始捕获：`/tmp/r05b/evidence/`（`pre-*.txt`、`final-*.txt`）
- 夹具：`/tmp/r05b/fixtures/{collision,deep,quoted-only}`；弱化副本：`/tmp/r05b/{weakrepo2,weak-nokey,weak-nolvalue,weak-flatten,weak-depth,weak-recerr}`

## 7. 主 Agent 独立复核后的补强（追加，不改上文）

复核结论：worker 报告的三项修复行为**均可独立复现成立**（撞名 fail-closed、
成员继承三类反例、深嵌套规范拒绝），但我另发现一处**断言缺口**，已由主 Agent 补齐：

- 缺口：`run_table_path_self_checks` 只断言 `"ambiguous-table-path"` 子串与
  「首段含 `:`」，因此把冲突行号写死成 `0`/`1`、或从报错信息里删掉两条冲突路径
  与展平键，`--self-test` 仍 exit 0。复核退回要求的正是「文件:行号 + 两条路径」可定位。
- 补齐：拒绝用例新增手写 detail（期望行号 / 两条路径元组拼写 / 展平键），
  由新增 `_assert_rejection_detail` 校验；行号必须精确，路径与键必须出现在信息里。
- 再验证：self-test 仍 68 条符合预期 / exit 0；正常仓库 exit 0；九种单点变异中
  原先三种「绿」的（行号写死 0 / 行号写死 1 / 删两条路径）现全部变红，
  另有删展平键一种亦变红。原始输出追加在 `post-fix.txt` 的「[追加]」段。

残留：`RecursionError` 兜底仍无 self-test 断言（worker 已声明为「无对应断言」，
只能靠 `setrecursionlimit` 外部观测量级）。本批不新增该断言，登记为残留项。
