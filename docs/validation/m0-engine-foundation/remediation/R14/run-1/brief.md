# R14 派发契约：全量验证与 M0 出口独立裁决（F-14 及全部 findings）

基线 HEAD：派发时 `git rev-parse HEAD`（**必须是 R01–R13 全部落地后的最终状态**；若仍有批次未完，本批不得开始）。仓库根 `<repo>`。

本批是整改的**收口批**，不是新实现批。它自己的原始证据必须独立落盘——这是 F-14 的直接要求：旧 9.6 的「独立运行」只有主 Agent 转述，原始 stdout/stderr 未独立保存。**不得引用旧 review 结论替代本批的实测。**

## 文件所有权

- 只写 `docs/validation/m0-engine-foundation/remediation/R14/run-1/**`（全部证据与最终交付文件）
- `docs/maintenance/m0-remediation/tasks.md`（**只许**：§1 状态表、§3 状态/交接列、§6 交付物「当前产物」列填入本批链接）
- `openspec/changes/m0-engine-foundation/tasks.md`（**只在裁决明确支持时**改勾选，逐条给证据；不得批量勾选）
- 若某 finding 需追加勘误，只能**追加**到对应历史 `report.md` 末尾并标明 R14/日期

**不得写** `crates/**`、`scripts/**`、`Cargo.*`、`.github/**` 任何内容。若实测发现实现缺陷，**登记 `待裁决` + 最小复现**，不自行修改（本批是裁决批，不是修复批）。不得 `git reset`、不得回退他人改动、不得重写 Git 历史。

## 前置

R01–R13 全部处于 `完成`（或明确 `待裁决` 且裁决结论已记录）。先核对 §1 状态表与实际 `git log`，列出未完成项；有未完成项即 **BLOCKED**，不要「先跑门禁再说」。

## 目标 1：统一门禁取证（tasks.md §6 逐条，真实 argv/cwd/环境/起止/exit/原始输出）

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
python3 scripts/check_engineering.py --self-test
python3 scripts/check_lints.py --self-test
python3 scripts/check_safety.py --self-test
python3 scripts/check_public_api.py --self-test
openspec validate m0-engine-foundation --strict --no-interactive
git diff --check
```

- `CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。
- **另读当前 CLI 后运行实际统一检查入口**（`scripts/check_workspace.py` 的实际参数），以及正常 workspace 的依赖/lint/safety/public-api/catalog 检查。**不能只跑隔离 self-test**。
- 守卫须同时具备三类证据：正常范围通过、**每个新增违规被明确拒绝**（复用 R05/R06 的隔离夹具，在受控临时副本里做）、移除违规后恢复通过。
- 记录测试总数，并与原基线 **312 passed** 逐项对比差异（新增/改名/删除分别列出），不得只写「全绿」。
- 代码变化后按开发流程跑 `codegraph sync` / `codegraph status`，确认无遗漏源文件；把输出落盘。

## 目标 2：五份最终交付文件（放 `R14/run-1/deliverables/`）

1. **18 项处置表**（`findings-18.md`）：finding → 根因 → 修改 → 失败回归 → 通过证据 → 独立审查 → 残留限制。逐项引用 R 批次证据路径与提交哈希。F-01～F-18 一个不缺。
2. **原 46 任务重新核对表**（`tasks-46.md`）：每条 `已核实 / 证据不足 / 与事实不符`，**逐条**给证据；据实维护 `openspec/changes/m0-engine-foundation/tasks.md`，**不批量勾选**。
3. **Requirement/Scenario 矩阵**（`requirement-matrix.md`）：六新增 capability 的 **16 Requirement / 44 Scenario** 及修改的工程治理 delta，逐条对应实现与测试；**未测的登记不算覆盖**。
4. **E1–E7 出口表**（`exits-e1-e7.md`）：每项全部要素、证据、未闭合原因。
5. **最终命令包**（`final-commands.md`）：实际统一入口与各门禁、环境、逐条 exit 与原始日志文件名、新增测试清单与相对 312 的差异。

## 目标 3：独立复核（F-14 核心）

- 复核者**未参与**其所审范围的实现与证据编写；可按范围分派（如 mapping/engine、UI/宿主集成、守卫脚本、文档与历史事实），但**不得把抽样升级为全量**，也不得只做抽样就写「全量通过」。
- 复核者必须：实际读实现、实际跑其范围内的命令、**保全原始 stdout/stderr 到本批证据目录**；其结论必须能由落盘输出复算。
- 主 Agent 负责去重、追踪漏项、最终裁决；不得只转述复核者结论。

## 目标 4：最终结论（`conclusion.md`，四项分开回答，不得合并）

1. **M0 是否闭合**——只有前置出口按规范满足才可写「闭合」；仍有证据不足/事实不符/阻断未关闭时**必须**写「未闭合」并列出阻断数。
2. **M1 能否接线**——取决于 M0 出口，不得因整改批完成而自动写「可以」。
3. **阻断数**——6 项阻断（F-01/F-02/F-03/F-04/F-05/F-06）逐项给最终状态。
4. **平台与完整 AC/V 状态**——显式登记未执行项：真实 IME/焦点/窗口、Intel Mac、Windows 10、Linux、远端 CI、Release 性能/发布限额、现有文件安全替换/文件协调、资源迁移清理。**macOS 无头、all-targets、create-only 发布与引用保活不证明上述能力**；AC 基础不升格完整验收；**V-01～V-09 不因本轮修复自动整项通过**。

## 适用规范

`AGENTS.md`（尤其铁律与 RS-05）、`openspec/CODE_STANDARDS.md` §6.2 证据边界、`docs/development-process.md`、`docs/maintenance/m0-remediation/tasks.md` §5/§6。

## 约束

- 工具缺失/离线失败记「未核实」，**不联网安装、不擅自升级依赖**。
- 全部输出用「本轮新证据」口径；不得把本轮实测写成原复核时就有。
- **不提前宣布 M0 完成**；整改批完成 ≠ M0 闭合。
- 提交（Conventional Commits）：纯文档用 `docs`；若改动 `openspec/**` 的公共工程规则用 `workspace`。

## 证据目录

`docs/validation/m0-engine-foundation/remediation/R14/run-1/`

- `gate/`（每个门禁一条：`<name>.command.json` + `<name>.stdout.log` + `<name>.stderr.log`）
- `guards/`（正常通过 / 每个违规拒绝 / 移除后恢复，三类各一组）
- `deliverables/`（上述五份）
- `reviews/`（各范围复核者的原始输出与结论）
- `conclusion.md`
- `commands.txt`（argv、cwd、环境、起止时间、exit code）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、最终 HEAD、九条门禁逐条 exit、测试总数与 312 差异、18 项处置是否有未关闭项、46 任务三态计数、16R/44S 覆盖计数、E1–E7 未闭合项、**M0 闭合结论与阻断数**、未执行平台/能力清单、concerns、证据路径。不要贴长日志。
