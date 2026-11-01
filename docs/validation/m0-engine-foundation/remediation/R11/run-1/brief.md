# R11 派发契约：F-13 候选/工具/设备采集补证（只补证据，不改代码）

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `<repo>`。

## 文件所有权（只有你可写这些文件）

- `docs/validation/m0-engine-foundation/remediation/R11/run-1/` 下你自己新建的证据文件（含临时夹具）
- 允许**追加**（不得改写原文）：`docs/validation/m0-engine-foundation/README.md` 末尾的勘误/新证据链接小节

**不得修改**：`admission-2026-10-30/**` 的任何旧文件（旧日志、旧 `*.command.json`、旧报告一律只读）；任何 `crates/**`、`scripts/**`、`Cargo.*`、`openspec/**`。不得 `git reset`、不得回退他人改动、不得重写历史。

## 失败事实（F-13，严重，维度 D2）

原复核审计结论（我本轮已独立复验）：

- `admission-2026-10-30/` 下共 105 份 `*.command.json`，其中 **14 份缺字段**：
  - `safety-build`、`safety-clippy`、`safety-fmt`、`safety-negative-edition` 四份缺 `cwd` 与 `started_utc`/`finished_utc`（只有 `argv`/`env_overrides`/`exit_code`/`id`）。
  - `tree-{all-base,all-test-support,linux-arm-both,linux-x64-both,linux-x64-wayland,linux-x64-x11,macos-arm,macos-arm-test,macos-intel,linux...,windows-x64}-packages` 共 10 份缺 `started_utc`/`finished_utc`（有 `argv`/`cwd`/`exit_code`/`id`）。
- `environment-commands.json` 记录 `rustc -Vv` / `cargo -V` / `cargo fmt --version` / `cargo clippy --version` / `rustup show` 的 stdout 与 exit，但**没有采集时间、cwd**。
- 设备/显示观察（`README.md` 的 OS/CPU/设备/显示器表）**没有保全对应的 `system_profiler` 原始输出**，也没有采集时间。

**这不是说旧构建没有发生**，也不判定硬件表必错。本轮目标是把「当前可保全的新证据」补上，并把旧缺口如实登记为历史事实。

## 目标

1. **旧缺口登记（只读盘点，不补造）**：用脚本化审计把上面 14 份 + `environment-commands.json` + 设备观察三类缺口落盘为 `legacy-gaps.json`/`legacy-gaps.md`：文件路径、缺失字段、既有字段原值。写明「旧记录无法追认采集时点，本轮不补造」。
2. **新采集（当前可复现的部分）**，每项都要 argv、cwd、起止时间（UTC，ISO-8601）、exit code、原始 stdout/stderr、环境变量覆盖：
   - 工具链：`rustc -Vv`、`cargo -V`、`cargo fmt --version`、`cargo clippy --version`、`rustup show`、`rustup toolchain list`、`python3 --version`。
   - 设备/显示：`sw_vers`、`uname -a`、`system_profiler SPHardwareDataType`、`system_profiler SPDisplaysDataType`。**只抄录非敏感字段**（型号、芯片、核数、内存、分辨率）；序列号/UDID/硬件 UUID 等必须显式打码或不抄录，并在报告里写明打码策略。
   - 磁盘/隔离：`df -h <repo>`、`ls` 隔离 `CARGO_HOME` 路径、`cat docs/validation/m0-engine-foundation/admission-2026-10-30/isolation.json`（确认该 `cargo_home` 路径当前是否仍存在——只记录事实）。
   - 候选依赖身份（只读，不重建）：从 `admission-2026-10-30/gpui-package-identity.json`、`gpui-package-file-git-hashes.json`、`dependency-ledger-base.json`、`candidate/Cargo.lock` 摘出当前仓库仍引用的 `gpui-pre =0.3.9` 身份与校验和，新记录 `candidate-identity-new.json`（注明「从既有证据摘录 + 当前仓库 manifest 比对」，不是重新准入）。
   - 当前仓库 `Cargo.lock` 中 `gpui-pre` 条目的校验和与 `candidate/Cargo.lock` 的同名条目比对（用 `python3` 只读解析，不得运行 `cargo update`）。
3. **不可复现项显式登记**（`unverified.md`）：Intel Mac、Windows 10、Linux（含 X11/Wayland）设备、远端 CI 运行、旧候选构建的原始时点。每项写：缺什么设备/权限、为什么本轮不能补、因此哪些原任务仍为「证据不足」。
4. **不得新增承诺**：新证据只能称「本轮新采集」，不得回填旧 `*.command.json`，不得在 README 里把旧缺口写成已修复。

## 约束

- 需要 Cargo 的命令（若有）一律 `--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。本批以只读观察为主，**不要**运行 build/clippy/test（那属于 R14 统一门禁，避免重复构建与日志混用）。
- 不得联网安装工具、不得 `rustup update`/`rustup component add`。
- 不得写进仓库受跟踪路径以外的临时副本之外的任何位置；`--output-dir` 式隔离，全部落 `remediation/R11/run-1/`。
- 提交（Conventional Commits，纯证据/文档 → scope `docs`，按 RS-05）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R11/run-1/`）

- `commands/`：每条命令一对 `<label>.command.json`（argv/cwd/started_utc/finished_utc/exit_code/env_overrides）与 `<label>.stdout.log`/`<label>.stderr.log`
- `legacy-gaps.json`、`legacy-gaps.md`
- `candidate-identity-new.json`
- `unverified.md`
- `report.md`（盘点方法、打码策略、新采集与旧记录的对照、哪些原任务仍证据不足）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、新采集命令数、仍不可复现项清单（一行）、concerns、证据路径。不要贴长日志。
