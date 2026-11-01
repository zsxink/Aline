# R06 派发契约：F-09/F-15 安全扫描范围有效

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `/Users/xian/Project/editor/Aline`。

## 文件所有权（只有你可写这些文件）

- `scripts/check_safety.py`
- `scripts/safety_lex.py`（仅在确有必要时；改了要在报告里说明理由）
- `docs/validation/m0-engine-foundation/remediation/R06/run-1/` 下你自己新建的证据文件

不得写其它文件（尤其不要动 `scripts/check_lints.py`、`scripts/check_workspace.py`、任何 `Cargo.toml`、任何 `crates/**`）。不得 `git reset`、不得回退他人改动。

## 两个失败（必须先各自写出失败回归，再修）

**F-09（严重）**：`workspace_scan()` 虽然用 `cargo read-manifest` 拿到了 `targets[].src_path`，却仍然只遍历成员目录下的 `*.rs`。Cargo 合法的目录外 target（例如成员 `Cargo.toml` 写 `[lib] path = "../../outside.rs"`，或 `build.rs`）完全不被扫描，`failures=[]`、exit 0，而该文件可以含生产 `unwrap()` 并带局部 `#![allow(clippy::unwrap_used)]`，同时躲过词法扫描和 Clippy deny。

原复核夹具：`/tmp/aline-m0-independent-review.pTFf8u/D/safety-external-target/`（八成员副本 + buffer `[lib] path="../../outside.rs"`）。若已不存在，按同样形状在你的证据目录下的临时副本重建。

**F-15（一般）**：根不存在时 `python3 scripts/check_safety.py --root /不存在的路径` 仍 exit 0，`failures/scanned/member_reads` 均为空——空扫描被当成通过。

## 目标

1. **范围校验（F-15）**：
   - 根必须是存在的目录且含 `Cargo.toml`；否则非零退出并给出明确规则（例如 `root-missing` / `root-not-workspace`）。
   - 八成员集合必须齐全（`app` + 七个 `crates/*`）；缺成员、成员目录不存在、成员不在允许清单、包身份不符，都必须给出具体规则而非静默跳过。
   - `scanned` 为空必须视为失败，不能「零命中即通过」。
2. **target 与 build-script 纳入受审范围（F-09）**：
   - 从 `cargo read-manifest`（或 `cargo metadata`）拿到每个成员的实际 targets 的 `src_path`、以及 `build` 脚本路径。
   - 目录外的真实 target / build script 必须**受审或明确拒绝**，二选一并写清规则名。推荐做法：对目录外路径明确拒绝（fail-closed，规则如 `external-target-path`），并在输出里带上实际路径与 argv 证据；若你选择「受审」，就必须以正确 kind（engine/ui、production/test）扫描它并纳入 `scanned`。
   - `#[path = ...]` 的既有 `unproven-module-path` 拒绝逻辑不得放宽。
3. **不得回归**：正常仓库 `python3 scripts/check_safety.py` 必须仍 exit 0 且 `scanned` 覆盖八成员；`--self-test` 必须仍全部通过（原 45 项正反例一项都不能减少）。
4. self-test 增补至少三项（写入 `--output-dir` 隔离目录）：目录外 `[lib] path` 夹具被拒绝；目录外 build script 夹具被拒绝（或受审命中 unwrap）；不存在根夹具非零且带具体规则。每项都要有独立预期（先手写期望再跑），不能拿实现输出当预期。

## 约束

- 所有 Cargo 命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`，`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」，不要联网安装。
- 夹具放受控临时目录或 `--output-dir` 下，不写进仓库受跟踪路径。
- 违规只放临时副本；成功拒绝必须给出具体规则/路径，不能把 Cargo 环境失败或任意非零当正确拒绝。
- 提交（Conventional Commits，纯工程脚本 → scope `workspace`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R06/run-1/`）

- `pre-fix.txt`（修复前：外部 target 夹具 exit 0、不存在根 exit 0 的实际输出）
- `post-fix.txt`（修复后：两者均被拒绝；正常仓库 exit 0 且扫描覆盖完整）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（根因、规则设计、逐条断言对照、未覆盖范围）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、一行测试小结、concerns、证据路径。不要贴长日志。

## 勘误（2026-11-01 追加，不改原文）

上文「目标 3」写「原 45 项正反例」，与实测不符。本轮在 `e2b12bc` 上用
`CARGO_NET_OFFLINE=true RUSTUP_TOOLCHAIN=1.98.1 python3 scripts/check_safety.py --self-test`
实测：`passed: true`，`cases` 数组长度 **53**，exit 0。

因此下限以 **53** 为准：不得减少任何一项既有 case，并在其上**至少新增 3 项**（目标 4），
即完成后 case 数 ≥ 56。原文的「45」是笔误，不构成放宽依据。
