# R12 派发契约：F-16 嵌套 Cargo 锁定口径与初始 lock 生成分开登记

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `/Users/xian/Project/editor/Aline`。前置：R05（`check_lints.py`）与 R06（`check_safety.py`）已完成并提交——先读它们的 handoff 与最终脚本，不要与它们并行改同一文件。

## 文件所有权（只有你可写这些文件）

- `scripts/check_lints.py`（只改嵌套 Cargo 的 argv/记录方式与说明，不得回退 R05 的 TOML 解析实现）
- `scripts/check_engineering.py`、`scripts/engineering_self_test.py`（仅当确需调整口径时）
- `scripts/check_public_api.py`（仅当确需调整口径时）
- `docs/validation/m0-engine-foundation/remediation/R12/run-1/` 下你自己新建的证据文件

不得写其它文件。不得 `git reset`、不得回退他人改动。

## 失败事实（F-16，一般，维度 D2/D5/D6）

原复核 + 我本轮独立复验：

- `scripts/check_lints.py` 的嵌套 Cargo 命令**只有 `--offline`，没有 `--locked`**：
  - `:211` `cargo check --manifest-path <fixture>/Cargo.toml --package <name> --offline`
  - `:232` `cargo clippy --manifest-path <manifest> --offline --all-targets [--release] -- -D warnings`
  - `:238` `cargo check --manifest-path <manifest> --offline`
  - `:249` `cargo check --manifest-path <manifest> --offline`
  - 实际证据 `admission-2026-10-30/.../inherit-unsafe-aline-buffer.command.json` 的 argv 确认无 `--locked`。
- `scripts/check_public_api.py:66` 的 `cargo generate-lockfile --manifest-path ... --offline` 是**初始 lock 生成**，本不该带 `--locked`（生成时无锁可锁）；但当前证据里没有把它与后续锁定检查分开登记，容易被泛称成「全程 locked/offline」。
- `scripts/check_engineering.py:125` 已有 `["--manifest-path", ..., "--locked", "--offline"]`（正例，不要破坏）。
- `scripts/engineering_self_test.py:87` 的 `cargo tree` 带 `--locked`；`:168` 的负例 `cargo metadata --offline`（故意缺 `--locked`，用于验证 Cargo 报错路径，属正当例外，需登记而不是改掉）。

**离线 ≠ 锁定**：`--offline` 只禁止网络，仍允许 Cargo 改写 `Cargo.lock`；`--locked` 才拒绝改锁文件。所以「隔离夹具里缺 `--locked`」是真实口径缺陷，不是措辞问题。

## 目标

1. **初始生成与后续检查分开**：
   - 夹具若需要初始 lock（新夹具、无 `Cargo.lock`），用 `cargo generate-lockfile --offline`（不带 `--locked`），并在记录里标 `evidence_kind: "initial-lock-generation"`；
   - 之后所有解析/check/clippy/test/tree 命令一律 `--locked --offline`，记录标 `evidence_kind: "locked-check"`。
   - 对 `check_lints.py` 的隔离夹具：现有 `write_fixture` 生成的项目**没有 `Cargo.lock`**。你要么在生成后显式跑一次 `generate-lockfile --offline` 再对所有后续命令加 `--locked`，要么证明这些夹具在当前结构下 `--locked` 会让 Cargo 报「lock file not found」并因此选择「先生成」路径。**选择哪条都要用真实 Cargo 运行证明**，不能只读文档推断。
2. **统一记录口径**：让 `record()`/`check_expected()` 产出的 `*.command.json` 带上 `evidence_kind`（`initial-lock-generation` / `locked-check` / `cargo-version` 等）与 `locked`/`offline` 两个布尔事实字段（由 argv 实际推导，不手填）。这样后续审计不必再解析 argv。
3. **审计脚本化**：写一个只读审计（`remediation/R12/run-1/audit-nested-cargo.py`），扫描 `scripts/*.py` 里所有 `cargo` 调用点（用 `ast` 解析而不是正则），对每个调用点输出：文件、行号、`subcommand`、是否 `--locked`、是否 `--offline`、是否属「初始生成」例外。运行结果落 `nested-cargo-audit.json`/`.md`，并对每个**非例外**缺 `--locked` 的调用点明确标注「已修 / 保留并说明理由」。
4. **不得回归**：
   - `python3 scripts/check_lints.py` exit 0；`python3 scripts/check_lints.py --self-test --output-dir <隔离目录>` 全部通过（R05 新增的伪继承/真继承/Cargo 对照三项必须仍在）；
   - `python3 scripts/check_engineering.py --self-test`、`python3 scripts/check_public_api.py --self-test` 全部通过；
   - `python3 scripts/check_safety.py --self-test` 不受影响（若你改了 `safety_lex.py` 之外的东西就算越权）。
5. **措辞勘误**：现行文档里若有「所有嵌套 Cargo 始终 locked/offline」这类泛称（例如 `docs/validation/m0-engine-foundation/group-2/engineering-guard-report.md` 第 5 行附近），**追加**勘误（不改原文），写明初始生成例外与本次修复时点；历史日志不重跑、不改写。
6. 全部 Cargo 命令按上述口径；`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」，不联网安装。
7. 提交（Conventional Commits，scope `workspace`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R12/run-1/`）

- `pre-fix.txt`（修复前：`check_lints` 隔离夹具实际 argv 无 `--locked` 的原始记录；可引用既有 `inherit-unsafe-aline-buffer.command.json` 并给出本轮新跑的同形记录）
- `post-fix.txt`（修复后：同夹具 argv 含 `--locked --offline`；`--self-test` 全通过原始输出）
- `nested-cargo-audit.json`、`nested-cargo-audit.md`
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（根因、生成/检查两阶段设计、逐调用点对照表、保留的例外与理由、勘误链接）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、一行测试小结、审计出的调用点数/缺 `--locked` 数/修复数、保留例外数、concerns、证据路径。不要贴长日志。
