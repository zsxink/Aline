# R12：Cargo lock 生成与锁定检查口径

日期：2026-11-01。实现与证据均为本轮工作树状态，不提交。当前 HEAD 和相关源文件指纹见 [source-identity.json](source-identity.json)。

## 结论

F-16 的 `check_lints.py` 嵌套检查缺 `--locked` 已修复，并区分 fixture 初次 lock 生成和后续锁定检查。另在完整 AST 审计中发现并修正 `engineering_self_test.py` 的正例 metadata 调用缺 `--locked`；故 5 个缺锁调用点已修（check_lints 4 处、engineering self-test 1 处）。唯一保留的依赖检查缺锁调用是故意验证无锁 metadata 的负例。

## 根因及实现

`check_lints.write_fixture()` 创建的隔离 workspace 不带 `Cargo.lock`。修前真实离线 `cargo check` exit 0 并生成了锁文件，证明 `--offline` 不阻止锁文件改写。该探针在临时 workspace 内运行，argv、环境、锁前后状态和输出见 [pre-fix.txt](pre-fix.txt) 与配对命令记录。

`check_lints.record()` 现从实际 argv 推导 `evidence_kind`、`locked`、`offline` 字段。执行有 manifest 的 `locked-check` 前，若 fixture workspace 没有 Cargo.lock，则先单独运行并记录 `cargo generate-lockfile --manifest-path … --offline`，确认 lock 已生成后才执行带 `--locked --offline` 的原检查。lock 定位限于当前 fixture workspace/manifest 祖先，不写仓库根锁文件。修复覆盖之前四个只带 `--offline` 的调用点。`check_engineering.record_command()` 同样从 argv 推导三字段，`engineering_self_test.py` 为每个新 fixture workspace 单独生成 lock，再执行带 `--locked --offline` 的 metadata 和 tree。

`check_public_api.py` 原来已将首次 `generate-lockfile --offline` 与后续 `check --locked --offline` 分开；本轮未改其实现。`engineering_self_test.py` 的 `negative-real-cargo-acquisition` 缺锁 `metadata --offline` 有意保持不加 `--locked`，否则 Cargo 不会到达待验证的缺锁路径。

## 全脚本 AST 审计

[audit-nested-cargo.py](audit-nested-cargo.py) 解析 `scripts/*.py`，追踪字面 argv、列表拼接/变量和命令表循环到命令执行器；只记录真实执行器调用，排除仅序列化的 synthetic 命令记录。结果在 [nested-cargo-audit.json](nested-cargo-audit.json) 与 [nested-cargo-audit.md](nested-cargo-audit.md)：25 个调用点、16 个需 lock 的依赖检查、15 个带 `--locked`、1 个明确保留的缺锁负例；另有 3 个初始 lock 生成例外。所有未满足项均标注已修或保留理由，Python AST 解析失败为 0。版本查询和 `cargo fmt` 不操作依赖 lock，不计入锁定检查。

## 验证

- `python scripts/check_lints.py`：exit 0。
- `check_lints.py --self-test`：exit 0；10 个 target case（debug/release）、4 个继承场景、25 parser case、12 表路径 case、5 成员 case 全部通过；R05 伪继承/真继承/Cargo 对照仍在。
- `check_engineering.py --self-test`：exit 0，97 项通过；初始 lock 生成、后续 locked metadata/tree 均有独立 command record。
- `check_public_api.py --self-test`：首次在初始 lock generation 阶段因自动选择的隔离 CARGO_HOME 无 `pulldown-cmark` registry 缓存而 exit 101，原失败完整保留。随后使用已有预热 `<cargo-home>`、离线工具链与全新输出目录重跑，exit 0、8/8 通过；没有联网或安装。
- `check_safety.py --self-test`：exit 0，65/65。
- 最终 AST audit、py_compile、`git diff --check` 均 exit 0。
- 报告/交接落盘后再次运行 `git diff --check` 并扫描 R12 的 47 个文本证据文件，exit 0、无尾随空白。

两轮 Cargo 自测的嵌套 argv/cwd/time/env/exit、stdout/stderr、fixture manifests、锁文件与 summary 分别保存在 [postfix-command-artifacts.tar.gz](postfix-command-artifacts.tar.gz) 和 [cached-rerun-command-artifacts.tar.gz](cached-rerun-command-artifacts.tar.gz)。顶层命令索引见 [commands.txt](commands.txt)；执行结果详见 [post-fix.txt](post-fix.txt)。首次 public-api 缓存选择失败是环境限制，不是 argv 缺 `--locked`；它发生在初始 `generate-lockfile --offline`，后续带锁检查尚未运行。

## 需主 Agent 追加的历史勘误

按 R12 文件所有权约束，本 Agent 未修改 `docs/validation/m0-engine-foundation/group-2/engineering-guard-report.md`。建议主 Agent 只在原文末尾追加：

> **R12 勘误（2026-11-01）**：第 5 行“Cargo 始终 `--locked --offline`”描述的是本报告所记录的正式依赖图采集命令，不能泛化为当时所有嵌套 Cargo 调用。修前 `check_lints.py` 的四个隔离检查仅带 `--offline`；R12 已在本轮加入 `--locked`，并将无锁 fixture 的初次生成单独记为 `cargo generate-lockfile --offline`。新 fixture 初次生成不带 `--locked` 是必要例外，生成后检查均锁定离线。`engineering_self_test.py` 中用于验证 Cargo 缺失 lock 报错的负例 `metadata --offline` 仍有意不带 `--locked`。原报告和历史日志保持不变。

## 边界

未运行 workspace clippy/test 或完整工程门禁；由 R14 统一执行。未改 `check_safety.py`/`safety_lex.py`，仅运行其 self-test 作不回归验证。没有提交；未改写任何历史采集记录。
