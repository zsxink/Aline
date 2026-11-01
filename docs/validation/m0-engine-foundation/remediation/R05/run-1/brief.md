# R05 派发契约：F-08 lint 继承必须真实解析

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `<repo>`。

## 文件所有权（只有你可写这些文件）

- `scripts/check_lints.py`
- `docs/validation/m0-engine-foundation/remediation/R05/run-1/` 下你自己新建的证据文件

不得写其它文件（尤其不要动 `scripts/check_safety.py`、`scripts/safety_lex.py`、`scripts/check_workspace.py`、任何 `Cargo.toml`、任何 `crates/**`）。不得 `git reset`、不得回退他人改动。

## 失败（必须先写出失败回归，再修）

**F-08（严重）**：`scripts/check_lints.py` 的 `tables()` 是逐行手写解析器，会把 `[package]` 段里多行字符串（`description = """..."""`）内部的假 `[lints]` / `workspace = true` 当成真实表，从而给「实际没有继承 workspace lint」的成员判通过。

原复核夹具（本机仍在）：

```
<tmp>/aline-m0-independent-review.pTFf8u/D/lint-decoy/
```

其 `crates/aline-buffer/Cargo.toml` 为：

```toml
[package]
description = """
[lints]
workspace = true
[package.metadata.decoy]
"""
name="aline-buffer"
version="0.0.0"
edition="2024"
```

原实测：`python3 scripts/check_lints.py --root <该夹具>` exit **0**，输出「workspace lint 声明有效」。而真实 Cargo 把这段读成 description。对照夹具 `<tmp>/aline-m0-independent-review.pTFf8u/D/lint-real-inheritance/`（真实继承）在 `cargo clippy --manifest-path ... -p aline-buffer --locked --offline -- -D warnings` 下 exit 101（`missing documentation for the crate`），伪继承夹具同命令 exit 0。

若 `/tmp` 夹具已不存在，按上面的 manifest 原文在**你自己的证据目录下的临时副本**里重建（不得写进仓库受跟踪路径）。

## 目标

1. 让 `tables()` 成为**真实 TOML 解析**而不是逐行扫描：至少正确处理 `[table]`、`[[array-of-tables]]`、裸键/引号键/点分键、基本字符串与转义、多行基本字符串 `"""..."""`、字面字符串 `'...'` 与多行字面字符串 `'''...'''`、注释、布尔、整数、数组（含多行数组）、内联表 `{ ... }`。无法安全解释的构造必须**拒绝（抛 `CheckError`）**，不能静默忽略。
2. 环境是 **Python 3.9.6，没有 `tomllib`，也没有 `tomli`/`toml` 第三方包**，且不得联网安装。所以你需要在脚本内自带解析实现，或仅用标准库。不得引入外部依赖。
3. 校验语义要落到真实解析结果上：成员 `[lints]` 必须恰好是 `{"workspace": true}` 且不得有 `[lints.*]` 子表；根 `[workspace.lints.rust]` / `[workspace.lints.clippy]` 必须与 `EXPECTED_RUST`/`EXPECTED_CLIPPY` 完全相等。
4. self-test 增补（写入 `--output-dir` 指定的隔离目录，不污染仓库）：
   - 伪继承夹具（上面那段 description 原文）→ 守卫必须**拒绝**，给出具体规则/路径；
   - 真实继承夹具 → 守卫**通过**；
   - Cargo 实际诊断对照：伪继承夹具 `cargo clippy ... -D warnings` exit 0，真实继承夹具 exit 101 且含 `missing documentation`（或等价证明继承生效的诊断）。记录真实 argv/exit/原始输出。
5. 不得回归：正常仓库 `python3 scripts/check_lints.py` 必须仍 exit 0；`--self-test` 必须仍通过（原有 10 组 debug/release 正反例与继承反例不得减少）。

## 约束

- 所有 Cargo 命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`，`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」，不要联网安装。
- 生成的夹具必须放受控临时目录或 `--output-dir` 下，不写进仓库受跟踪路径。
- 提交（Conventional Commits，纯工程脚本 → scope `workspace`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R05/run-1/`）

- `pre-fix.txt`（修复前伪继承夹具被误判通过的实际输出与 exit code）
- `post-fix.txt`（修复后：伪继承拒绝、真实继承通过、正常仓库 exit 0）
- `cargo-control.txt`（伪继承 vs 真实继承的 Cargo 诊断对照）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（根因、解析器方案、逐条断言对照、未覆盖的 TOML 构造）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、一行测试小结、concerns、证据路径。不要贴长日志。
