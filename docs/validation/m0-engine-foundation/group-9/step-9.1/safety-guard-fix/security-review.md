# 安全守卫修改独立评审：集成测试 crate 根模块解析

## 1. 评审范围

评审对象：工作区未提交修改 `scripts/check_safety.py`（对照基线 `HEAD` 中的版本），以及随附的 `scripts/safety_lex.py`（未改动，仅用于理解词法/作用域模型）。评审只针对本次守卫改动，不对 M0 出口、其它门禁或产品代码下结论。

评审者独立复算，不采信调度者报告中的结论。评审期间未修改仓库内任何源码；只写了本文件与同目录 `security-review-commands.log`。self-test 按既有设计写入 `docs/validation/m0-engine-foundation/group-2/safety-fixtures/` 下的固定 fixture（改动前已存在的 5 个未跟踪目录被重新写入，属既有行为，未清理）。

## 2. 环境与基线

| 项 | 值 |
|---|---|
| HEAD | `1dd0174ca8fca975209ee5d6c3169c36695fad` |
| Python | 3.9.6 |
| cargo / rustc | 1.98.1 / 1.98.1 |
| 平台 | macOS (darwin), zsh |
| 评审时间窗（UTC） | 2026-10-31T13:08Z 至 2026-10-31T13:35Z；最终带时间戳复核见 commands log |
| 待审文件 SHA-256 | `scripts/check_safety.py` = `804169233b0dcd8dd19aff843a8c0d24f9a0212d96e9a8f0f5d559bf9a03243c`；`scripts/safety_lex.py` = `358c17a9e24e502f159eb1d7f0ccaacd36e4df5c6928814634b58154173ce2dd` |

## 3. 变更理解

改动只涉及 `workspace_scan()` 的模块 home 计算（`scripts/check_safety.py:116-122`）和 `self_test()` 新增的 5 个固定用例（`:202-243`）：

- 旧规则：`home = path.parent if path.name in ('lib.rs','main.rs','mod.rs') else path.parent / path.stem`。
- 新规则：额外把 `relative.parts == ('tests', '<name>.rs')` 的文件视为 Cargo 集成测试 crate 根，`home = path.parent`（即 `tests/`）。
- 作用域模型未改：`test_files` 初值仍是 `relative.parts[0] == 'tests'` 的全部文件（`:102`）；`test_targets`/`production_targets` 仍由声明是否带精确 `#[cfg(test)]` 决定，`test_files.update(test_targets - production_targets)`（`:132`）。

## 4. 独立复算结果

### 4.1 旧版复现

用 `git show HEAD:scripts/check_safety.py` 取旧版到 `/tmp/old_check_safety.py`（同目录复制 `safety_lex.py`），对仓库根执行：

- exit 1，`failures=4`，`scanned=84`，全部为 `test-module-resolution`，`detail=common`，文件正是 `crates/aline-ui/tests/document.rs`、`document_composition.rs`、`document_results.rs`、`document_typing.rs`。与背景事实一致。

### 4.2 新版仓库根

- exit 0，`failures=0`，`scanned=84`；`scanned` 的 `(file, kind)` 集合与旧版逐一比较无差异（`engine-production` 44、`engine-test` 24、`ui-production` 10、`ui-test` 6）。

### 4.3 self-test

- `python3 scripts/check_safety.py --self-test --output-dir /tmp/aline-review-selftest-final` exit 0，50/50 用例通过，其中新增 5 例全部通过；结尾 `workspace_after_fixtures` 为 0 failures、84 files。

### 4.4 Cargo/rustc 语义对照（独立于实现）

用最小 Cargo 工程实测（日志见 commands log 的 `cargo-ground-truth` 段）：

| 形状 | cargo 结果 | 守卫行为 |
|---|---|---|
| `tests/a.rs` `mod b;` + `tests/b.rs` | exit 0 | 通过（正确） |
| `tests/a.rs` `mod b;` + `tests/a/b.rs`（旧规则接受的位置） | exit 101 `E0583` | 拒绝（正确，新规则比旧规则更贴近 Cargo） |
| `tests/a.rs` `mod src_mod;` + 仅 `src/src_mod.rs` | exit 101 `E0583` | 拒绝（正确） |
| `tests/foo/main.rs` `mod helper;` + `tests/foo/helper.rs` | exit 0，`cargo read-manifest` 确认 `foo` 是 test target | 通过（由既有 `main.rs` 规则覆盖，正确） |
| 嵌套：`tests/sub/dir/file.rs` `mod x;`，`x` 仅在 `tests/sub/dir/x.rs` | exit 101 `E0583` | 拒绝（正确：home 仍为 `tests/sub/dir/file`） |
| 同上，`x` 在 `tests/sub/dir/file/x.rs` | exit 0 | 通过（正确） |
| `tests/a.rs` `mod a;`（自引用） | exit 101 `circular modules` | **通过（差异，见 F1）** |

### 4.5 对抗性 fixture（评审者临时目录，未写入仓库）

17 个 fixture root（`/tmp/aline-review-fixtures*`）逐项结果：

- a) `tests/a.rs mod b;`：`b` 存在 → exit 0；缺失 → exit 1 `test-module-resolution`。✅
- b) 仅 `src/src_mod.rs` 存在 → exit 1 拒绝。✅（集成测试根不解析到 `src`）
- c) `#[cfg(test)] mod helper;` + `tests/helper.rs` 含 `assert!` → exit 0，无 `engine-production-assert`。✅
- d) `mod helper;`（无 `cfg(test)`）同样 exit 0、无断言告警。**这是既有行为**：`:102` 的 `test_files` 初值把所有 `tests/` 下文件直接视为测试作用域，不依赖声明；旧版守卫（会先报一条 `test-module-resolution`）对新版解出的同一文件分类完全相同，`(file, kind)` 对比无差异。不是本次改动引入的放宽。
- e) `tests/sub/dir/file.rs mod x;`，仅 `tests/sub/dir/x.rs` 存在 → exit 1 拒绝；仅 `tests/sub/dir/file/x.rs` 存在 → exit 0。✅（新规则未把嵌套文件误当 crate 根）
- f) `src/bin/tool.rs`、`examples/x.rs`：找不到模块 → exit 1（fail-closed）；即使模块位于 Cargo 正确的 `src/bin/helper.rs` / `examples/helper.rs`，守卫仍 exit 1（既有 fail-closed 误报）；而错误位置 `src/bin/tool/helper.rs` / `examples/x/helper.rs` 会被静默接受（既有 fail-open，见 F4）。
- g) `tests/common.rs` 与 `tests/common/mod.rs` 同时存在 → exit 1（歧义拒绝）。✅

### 4.6 变异测试（检验 self-test 负例是否真的抓回归）

在 `/tmp/aline-mut`（脚本与 fixture 的隔离副本）上逐项改坏实现的副本，运行 self-test，然后恢复（仓库未动）：

| 变异 | self-test | 结论 |
|---|---|---|
| M1 回退到旧 home 规则 | 失败 3 例（`integration-test-crate-root`、`integration-nested-module-resolution`、`integration-sibling-file-is-own-root`） | 新正例能抓回归 ✅ |
| M2 把任意 `tests/` 下文件都当 crate 根 | **全部 50 例通过** | 负例覆盖不足，见 F2 |
| M3b 放宽 `test_files` 到任意含 `tests` 路径分量 | **全部通过**（该变异可让 `src/tests/helper.rs` 的 `assert!` 漏报） | 既有测试盲区，见 F6 |
| M5 集成测试根回退解析到 `src/` | 失败 1 例（`integration-root-does-not-see-src-module`） | 负例有效 ✅ |
| M6 对 crate 根跳过“缺失即拒绝” | self-test 崩溃（`IndexError` → `input-failure`，exit 1） | fail-closed 保持，但只靠崩溃捕获，见 F7 |
| M7 同时接受新旧两个 home | **全部通过**（会接受 `tests/a/b.rs` 这种 rustc 拒绝的布局） | 负例覆盖不足，见 F3 |

## 5. 安全性方向判定

- **作用域是否放宽：否。** `test_files` 初值与更新公式未变；新规则只在 `tests/<name>.rs` 上改动 `home`，候选路径仍全部落在该成员 `tests/` 内，而这些文件本来就因 `parts[0] == 'tests'` 在 `test_files` 初值中。`production_targets` 只会进一步抑制测试豁免。仓库 84 个文件与全部对抗 fixture 的 `(file, kind)` 分类新旧一致。
- **是否把真实缺失模块当已解析：除 F1 的自引用边界外，未发现。** 嵌套 `tests/<dir>/<file>.rs`、`src/` 回退、歧义解析均被正确拒绝；旧规则错误接受的 `tests/<name>/<module>.rs` 现在被拒绝（更严格，符合 Cargo）。
- **路径穿越/异常：未发现新问题。** 模块名来自词法分析器的标识符 token，不可能含 `/` 或 `..`；`path.relative_to(directory)` 的 `path` 来自 `directory.rglob('*.rs')`，按构造必在其下，不会抛 `ValueError`；即便抛出，`main()` 也捕获 `ValueError` 并返回 `input-failure`（exit 1，fail-closed）。symlink 行为未变（既有：分类基于词法路径而非解析目标）。
- **对 symlink/非成员路径：** 新规则不改变成员枚举方式，未见误用。

## 6. self-test 新用例质量

- 期望是手写固定字面量，且与 rustc/cargo 实测语义一致，不是把实现行为抄成期望；正例能抓住 M1、负例能抓住 M5。`integration-root-does-not-see-src-module` 能防止“集成根偷看 src”的放宽，价值最高。
- 但负例没有钉住两条边界：`len(relative.parts) == 2` 的作用域边界（M2 存活）和“旧错误位置必须被拒绝”（M7 存活）。两者均是**测试覆盖缺口**，实现本身经评审者 fixture 验证是正确的。
- `integration-test-missing-module` 在旧实现下也会通过（旧规则同样拒绝），单看该例不能区分新旧实现；它防的是“未解析即放行”的方向，仍有价值，但不能替代 F3 建议的用例。

## 7. 发现清单（按严重度）

### F1 — LOW（本次改动引入的 fail-open 边界）
`tests/a.rs` 中 `mod a;` 现在解析到文件自身并被接受；rustc 报 `error: circular modules: tests/a.rs -> tests/a.rs`，旧守卫报 `test-module-resolution`。这是模块存在性检查的窄口径漏报，不改变任何测试/生产作用域（该文件本就在 `test_files`），且该源码本身无法通过编译；但属于本次改动相对旧版在严格性上的回退。

- 位置：`/Users/xian/Project/editor/Aline/scripts/check_safety.py:119-127`
- 建议（未动手）：解析结果若等于声明所在文件自身，按 `test-module-resolution` 拒绝；补一个 self-test 用例。

### F2 — LOW（self-test 负例覆盖不足，实现本身正确）
新规则缺少“嵌套 tests 文件不得当作 crate 根”的负例。把条件放宽为任意 `tests/` 路径分量（变异 M2）后 50 例全部通过；而评审者的 `tests/sub/dir/file.rs mod x;` + 仅 `tests/sub/dir/x.rs` fixture 显示该变异会接受真实缺失模块。

- 位置：`scripts/check_safety.py:120-121`（规则）与 `:204-220`（用例集）
- 建议：新增负例 `tests/sub/dir/file.rs` 声明 `mod x;`、仅 `tests/sub/dir/x.rs` 存在，期望 `['test-module-resolution']`。

### F3 — LOW（self-test 负例覆盖不足，实现本身正确）
缺少“旧错误位置 `tests/<name>/<module>.rs` 必须被拒绝”的负例。变异 M7（同时接受新旧 home）后 50 例全部通过，并会接受 rustc 拒绝的 `tests/a/b.rs` 布局。

- 位置：`scripts/check_safety.py:204-220`
- 建议：新增负例 `tests/check.rs` 声明 `mod common;`、仅 `tests/check/common.rs` 存在，期望 `['test-module-resolution']`。

### F4 — INFO（既有局限，非本次引入）
`examples/`、`src/bin/`（以及 `benches/`）在 Cargo 中同样是 crate 根，但未建模：

- 正确位置（`examples/helper.rs`、`src/bin/helper.rs`）被误报 `test-module-resolution` → fail-closed，安全方向；
- 错误位置（`examples/x/helper.rs`、`src/bin/tool/helper.rs`）被静默接受 → fail-open，与 rustc 的 `E0583` 不一致。旧版行为完全相同，仓库当前无这类文件。
- 位置：`scripts/check_safety.py:116-122`（注释只声明了“解析失败仍拒绝”，未覆盖错误位置被接受的情况）。

### F5 — INFO（self-test 稳健性）
新增 fixture 用 `mkdir(parents=True, exist_ok=True)` 累加写入，从不清理旧文件；若未来用例布局改变，残留文件可能改变结果。已核对当前 5 个 fixture 目录内容与用例完全一致，无残留。

- 位置：`scripts/check_safety.py:221-230`

### F6 — INFO（既有测试盲区，与本次 diff 无关）
把 `test_files` 初值（`:102`）放宽到任意含 `tests` 路径分量后，self-test 全通过；该变异会让 `src/tests/helper.rs` 被当作测试作用域而漏报 `engine-production-assert`。本次改动未触及该行，仓库也无此布局，故不是本次回归；但建议补一个“`src/` 下名为 `tests` 的目录不是测试作用域”的用例。

- 位置：`scripts/check_safety.py:102`

### F7 — INFO（fail-closed 保持但报错形态不干净）
若实现改为“crate 根未解析即跳过拒绝”（M6），守卫会因 `existing[0]` 抛 `IndexError`，最后以 `input-failure` 形式 exit 1。安全方向仍是拒绝，但错误信息不指向根因。

- 位置：`scripts/check_safety.py:126-131`

## 8. 明确不构成问题的点

- 未扩大 `test_files`，未把任何生产文件转为测试作用域；分类在仓库与 17 个 fixture 上新旧一致。
- 集成测试根不会解析到 `src/`；缺失模块、歧义候选均拒绝。
- 路径处理无穿越、无新增异常面；模块名 token 不含路径分隔符。
- 仓库内实际受影响的 4 个 `crates/aline-ui/tests/*.rs` 的 `mod common;` 均正确解析到 `crates/aline-ui/tests/common/mod.rs`。

## 9. Verdict

**ACCEPT WITH LIMITATIONS**

改动正确对齐 Cargo 集成测试 crate 根的模块解析语义，未放宽测试/生产作用域豁免，缺失模块仍 fail-closed；仓库门禁层面的 4 条误报被消除且分类不变。限制集中在：F1 一个不涉及作用域的窄口径 fail-open 边界，F2/F3 两个 self-test 负例覆盖缺口，以及 F4/F6 的既有模型/测试局限。以上均不构成阻断本次守卫修复的理由，但建议在后续任务中补上 F2/F3 用例与 F1 的自引用判断。
