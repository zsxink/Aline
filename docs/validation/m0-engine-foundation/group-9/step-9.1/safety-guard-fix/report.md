# 组 9 / 9.1 前置：`safety` 守卫修复（用户裁决 A）

日期：2026-10-31。状态：守卫已按用户裁决修改；`safety` 与 `safety-self-test` 在本机重跑通过；全量 19 项门禁在修复后重跑 19/19 exit 0。**独立安全评审结论由评审者另行撰写**（`security-review.md`），本文不代写。

## 1. 冲突与裁决

组 8 交付后，统一门禁 19 项中 17 项 exit 0，`safety` 与 `safety-self-test` exit 1：

- `safety`：4 条 `rule: test-module-resolution`、`detail: common`，文件为 `crates/aline-ui/tests/document.rs`、`document_typing.rs`、`document_composition.rs`、`document_results.rs`。
- `safety-self-test`：固定用例全部通过，但 `workspace_after_fixtures` 复跑同一工作区扫描，带同样 4 条失败。

只读诊断（`group-8/leader-verify/safety-diagnosis/`）确认这是守卫对 Cargo 集成测试 crate 根的建模缺口，不是源码安全违规：单文件扫描 exit 0；改成 Cargo 也接受的目录布局（`tests/document/main.rs`）后目录扫描 exit 0。

用户裁决：**A 修改守卫**（需具体实现 + `safety-self-test` 新用例 + 独立安全评审）。裁决前未改守卫、未重构测试、未宣称门禁通过。

## 2. 修改内容

`scripts/check_safety.py` 的 `workspace_scan`，模块 home 计算由：

```python
home = path.parent if path.name in ('lib.rs', 'main.rs', 'mod.rs') else path.parent / path.stem
```

改为：

```python
relative = path.relative_to(directory)
is_crate_root = path.name in ('lib.rs', 'main.rs', 'mod.rs') or (
    len(relative.parts) == 2 and relative.parts[0] == 'tests')
home = path.parent if is_crate_root else path.parent / path.stem
```

并在候选存在性判断后加入自引用拒绝（安全评审 F1，见 §3.2）：

```python
if len(existing) != 1:
    failures.append({... 'rule': 'test-module-resolution', 'detail': name})
elif existing[0] == path:
    # `mod self;` 解析到声明文件自身：rustc 报 circular modules，按解析失败拒绝。
    failures.append({... 'rule': 'test-module-resolution', 'detail': name})
elif testing:
    ...
```

即：成员 `tests/` 目录的**直接子文件**（`tests/<name>.rs`）按 Cargo 集成测试 crate 根处理，其 `mod` 声明相对 `tests/` 解析；`tests/<dir>/<file>.rs`、`src/**` 等其余文件维持原规则。`src/bin/*.rs` 与 `examples/*.rs` 在 Cargo 里也是 crate 根，但本仓库当前没有带 `mod` 声明的这类文件，其解析失败仍按 `test-module-resolution` 拒绝（fail-closed），本次未建模。

未修改 `scripts/safety_lex.py`，未修改任何 Rust 源码、测试或期望。

### 2.1 新增 self-test 用例（期望先于实现手写）

| 用例 | 布局 | 期望 |
|---|---|---|
| `integration-test-crate-root` | `tests/check.rs` 声明 `mod common;`，`tests/common/mod.rs` 存在 | 无失败 |
| `integration-test-missing-module` | `tests/check.rs` 声明 `mod common;`，文件不存在 | `test-module-resolution` |
| `integration-nested-module-resolution` | `tests/common/mod.rs` 声明 `mod nested;`，`tests/common/nested.rs` 存在 | 无失败 |
| `integration-sibling-file-is-own-root` | `tests/check.rs` 声明 `mod sibling;`，`tests/sibling.rs` 存在 | 无失败 |
| `integration-root-does-not-see-src-module` | `tests/check.rs` 声明 `mod checks;`，只有 `src/checks.rs` | `test-module-resolution` |
| `integration-self-reference-is-rejected`（评审 F1） | `tests/selfref.rs` 声明 `mod selfref;`，解析到文件自身 | `test-module-resolution` |
| `integration-nested-file-is-not-crate-root`（评审 F2） | `tests/sub/dir/file.rs` 声明 `mod x;`，仅 `tests/sub/dir/x.rs` 存在 | `test-module-resolution` |
| `integration-old-wrong-location-is-rejected`（评审 F3） | `tests/check.rs` 声明 `mod common;`，仅 `tests/check/common.rs` 存在 | `test-module-resolution` |

前三例中的 `integration-root-does-not-see-src-module` 与后三例都是负例方向：确认集成测试根不得解析到 `src/`、不得解析到自身、不得把嵌套文件当根、不得接受 rustc 拒绝的旧位置（防止修复把真实缺失模块放行）。

## 3. 实测证据

目录：`group-9/step-9.1/safety-guard-fix/`。`commands.log` 逐条记录 UTC、实测 exit 与命令原文；原始 stdout/stderr 同目录保存。

| UTC | 命令（摘要） | exit | 结果 |
|---|---|---|---|
| 2026-10-31T13:09:06Z | `python3 <tmp>/…/pre-fix/check_safety.py --root <repo>`（HEAD `1dd0174` 的守卫副本） | 1 | 复现 4 条 `test-module-resolution`（`pre-fix-workspace.stdout.json`） |
| 2026-10-31T13:09:08Z | `python3 scripts/check_safety.py --root <repo>` | 0 | 0 失败，扫描 84 个文件（`post-fix-workspace.stdout.json`） |
| 2026-10-31T13:09:10Z | `python3 scripts/check_safety.py --self-test --output-dir …/self-test` | 0 | 50 例全部符合预期；`workspace_after_fixtures` 0 失败（`self-test/summary.json`） |
| 2026-10-31T13:12:24Z–13:12:42Z | `python3 scripts/check_workspace.py …` | 0 | 19/19 exit 0；`cargo test --workspace` 44 个 `test result` 行合计 295 passed、0 failed（`workspace-gates/summary.json`） |

守卫 blob：HEAD `3578d761c739a29fa543590af625c2de2c3f176d` → 首次修改后 `3c02a3568e6d30be105a1f0322614cae2fef7b8b` → 安全评审 F1～F3 处置后 `539f37c17546aa923cb6e7e2b56cfcf91177883c`（详见 §3.2）。

修复前后 `scanned` 的 84 个文件与 `kind` 分类完全一致（0 处分类变化）：本次改动只改变 `mod` 声明目标文件的解析位置，不改变任何文件被当作测试还是生产。

### 3.1 证据自纠（如实记录）

本目录 `commands.log` 的早先版本有两条不可信记录，已整体替换并保留更正说明：

1. 前三条 `git rev-parse/show/hash-object` 的 `exit=0` 是写日志时硬编码的占位值，不是实测；
2. 一条 `exit=128` 被错误归给一条 Python 命令——该 Python 命令实际从未执行，其所在 `&&` 链因前一条 `git show … > <tmp>/…` 失败而中断。

重写后每条命令单独执行并立即捕获 `$?`。其中 `git hash-object HEAD:<path>` 确实失败（`git hash-object` 不支持该语法，`exit=128`，stderr 保留在 `hash-pre-fix.stderr.log`），改用 `git rev-parse HEAD:scripts/check_safety.py` 取得旧版 blob。

### 3.2 安全评审 F1～F3 处置（`security-followups/`）

`security-review.md` 的判定是 **ACCEPT WITH LIMITATIONS**，并给出 F1（自引用 `mod a;` 被接受，rustc 报 circular modules）、F2（缺“嵌套 tests 文件不得当 crate 根”负例）、F3（缺“旧错误位置 `tests/<name>/<module>.rs` 必须拒绝”负例）。三项均按评审建议落地：

| 项 | 实现 | 新用例 |
|---|---|---|
| F1 | 解析结果等于声明文件自身时按 `test-module-resolution` 拒绝 | `integration-self-reference-is-rejected`（`tests/selfref.rs` 含 `mod selfref;`）期望拒绝 |
| F2 | 不变（`len(relative.parts) == 2` 已是实现语义） | `integration-nested-file-is-not-crate-root`（`tests/sub/dir/file.rs` 声明 `mod x;`，仅 `tests/sub/dir/x.rs`）期望拒绝 |
| F3 | 不变 | `integration-old-wrong-location-is-rejected`（`tests/check.rs` 声明 `mod common;`，仅 `tests/check/common.rs`）期望拒绝 |

实测（`security-followups/commands.log`，2026-10-31T13:43:05Z–13:43:17Z，每条单独执行）：

| UTC | 命令（摘要） | exit | 结果 |
|---|---|---|---|
| 13:43:05Z | `git hash-object scripts/check_safety.py` | 0 | `539f37c17546aa923cb6e7e2b56cfcf91177883c` |
| 13:43:05Z | `python3 scripts/check_safety.py --root <repo>` | 0 | 0 失败、84 文件 |
| 13:43:07Z | `python3 scripts/check_safety.py --self-test --output-dir …/security-followups/self-test` | 0 | 53 例全部符合预期；`workspace_after_fixtures` 0 失败（`self-test.stdout.json`） |
| 13:43:11Z | 变异 M2（任意 `tests/` 后代都当 crate 根）self-test | 1 | 只失败 `integration-nested-file-is-not-crate-root`（`mutant-M2.stdout.json`） |
| 13:43:13Z | 变异 M7（新旧两个 home 都接受）self-test | 1 | 只失败 `integration-old-wrong-location-is-rejected`（`mutant-M7.stdout.json`） |
| 13:43:15Z | 变异 M-F1（删掉自引用拒绝分支）self-test | 1 | 只失败 `integration-self-reference-is-rejected`（`mutant-M-F1.stdout.json`） |
| 13:43:17Z | 恢复原实现后 self-test | 0 | 53 例全部通过（`mutant-restored.stdout.json`） |

变异在 `<tmp>/aline-mut-followups/` 的隔离副本上进行，仓库内脚本未被变异；每个变异只让**目标负例**失败，其余 52 例仍通过，说明三条负例确实钉住了评审指出的边界（此前 M2/M7 变异下 50 例全通过，即评审 F2/F3 的原始观察）。

处置后全量 19 项门禁重跑（`workspace-gates-after-followups/`，2026-10-31T13:43:39Z–13:43:59Z）：19/19 exit 0，`cargo test --workspace` 295 passed、0 failed。

### 3.3 提交前三门禁（`commit-gates/`）

提交本次守卫修改与文档更正前，按提交规范重跑三条门禁（HEAD 仍为 `1dd0174`，改动在索引/工作区）：

| UTC | 命令 | exit | 结果 |
|---|---|---|---|
| 2026-10-31T14:01:18Z | `cargo fmt --all --check` | 0 | 无输出（`fmt.stdout.log` / `fmt.stderr.log`） |
| 2026-10-31T14:01:18Z | `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 0 | 无 warning（`clippy.stdout.log` / `clippy.stderr.log`） |
| 2026-10-31T14:01:18Z | `cargo test --workspace --locked --offline` | 0 | 44 个 `test result` 行合计 295 passed、0 failed（`test.stdout.log`） |

第一次尝试（`commit-gates-attempt-1-timestamp-format-invalid/`）的三条命令同样 exit 0、`test` 同为 295 passed，但脚本用 BSD `date +%3N` 生成时间戳，在本机展开成 `14:00:31.3NZ` 这种无效字面量，时间列不可信。该次输出原样保留、未删除，目录名标注了失效原因；正式记录用 §3.3 上表这次（时间戳为 `%Y-%m-%dT%H:%M:%SZ`）。

## 4. 未覆盖与不宣称

- 未建模 `src/bin/*.rs`、`examples/*.rs`、`benches/*.rs` 的 crate 根语义；当前仓库没有带 `mod` 声明的这类文件，缺失时仍 fail-closed。
- 守卫仍不是完整 Rust 解析器：不判断调用目标、类型内部可变性、过程宏输出、隐式 panic。
- 门禁只在 macOS arm64 / Darwin 27.2.0 / rustc 1.98.1 上运行；Windows/Linux、其他 target、CI 未执行。
- 本文不宣称 M0 出口通过，不宣称 V-01～V-09 通过；是否关闭本次守卫修复的评审发现，由独立安全评审判定。
