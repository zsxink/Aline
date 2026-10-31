# 组长复核：全量门禁与 `safety` 拦截诊断

日期：2026-10-31。范围：在当前 macOS 主机按统一环境运行 `scripts/check_workspace.py`，并对失败门禁做只读诊断。本文不是裁决，不声称 19 项门禁通过。

## 1. 全量门禁结果

- 命令与原始输出：`workspace-gates/summary.json` 及逐项 `*.command.json` / `*.stdout.log` / `*.stderr.log`。
- 时间：2026-10-31T11:17:19Z–11:17:22Z。
- 结果：`passed: false`，19 项中 17 项退出 0（含 `fmt`、`clippy`、`test`、`dependency`、`lint`、`public-api`），以下两项退出 1：
  - `safety`：4 条失败，均为 `rule: test-module-resolution`、`detail: common`，文件为
    `crates/aline-ui/tests/document.rs`、`document_typing.rs`、`document_composition.rs`、`document_results.rs`。
  - `safety-self-test`：8 个固定用例全部通过，但 `workspace_after_fixtures` 复跑同一工作区扫描，仍带上述 4 条失败，故整体 1。

## 2. 诊断证据

| 检查 | 命令 | 结果 | 文件 |
| --- | --- | --- | --- |
| 单文件扫描 | `check_safety.py --file crates/aline-ui/tests/document.rs --kind ui-test` | exit 0，`failures: []` | `safety-diagnosis/single-file.json` |
| 目录布局探针 | `check_safety.py --root <tmp>/`，布局为 `tests/document/main.rs` + `tests/document/common/mod.rs` | exit 0，无失败 | `safety-diagnosis/probe-directory-layout.json` |
| 守卫是否修改 | `git status --porcelain scripts/check_safety.py scripts/safety_lex.py` | 空 | 本目录 |
| 守卫 blob / sha256 | `git hash-object scripts/check_safety.py`、`python3 -c "import hashlib;…"` | `3578d76…` / `c66a77f…` | 本目录 |

## 3. 根因（只读推断）

`scripts/check_safety.py:116` 对每个文件计算 `mod` 声明的主目录：

```python
home = path.parent if path.name in ('lib.rs', 'main.rs', 'mod.rs') else path.parent / path.stem
```

对集成测试 crate 根 `tests/document.rs`，该式得到 `tests/document`，于是 `mod common;` 只查找
`tests/document/common.rs` 与 `tests/document/common/mod.rs`，找不到真实的 `tests/common/mod.rs`，报
`test-module-resolution`。Cargo 对 `tests/document.rs` 的真实解析是把该文件当作 crate 根，`mod common;` 解析到
`tests/common.rs` 或 `tests/common/mod.rs`。因此这是守卫对集成测试根文件的建模缺口，不是源码中的安全违规：

- 同一文件按单文件模式扫描为 0 失败；
- 把同一内容改成 Cargo 也接受的目录布局（`tests/document/main.rs`）后，目录扫描为 0 失败；
- 四个测试目标均无 `unsafe`、生产 `expect`/`panic` 出口，内容只含测试辅助与断言。

## 4. 待用户裁决的选项

- **A. 修改守卫**：让 `workspace_scan` 把 `tests/*.rs` 识别为 crate 根（`home = tests`），并补 `safety-self-test` 用例与独立安全评审。按现行约束，修改守卫须用户具体裁决后才能进行。
- **B. 调整测试布局**：每个目标改为 `tests/<name>/main.rs` + `tests/<name>/common/mod.rs`。已用探针验证可通过守卫，但 283 行共享夹具需要按目标复制或另设共享方式，且会改变 `cargo test --test <name>` 的目录结构。
- **C. 保留现状**：接受 `safety`/`safety-self-test` 失败并作为已记录阻塞，待 A 或 B 裁决后重跑。

在裁决前，不修改守卫、不重构测试、不把本组标记为门禁通过。
