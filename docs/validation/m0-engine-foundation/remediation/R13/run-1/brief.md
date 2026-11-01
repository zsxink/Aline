# R13 派发契约：F-12/F-17/F-18 现行状态与历史勘误一致

基线 HEAD：派发时 `git rev-parse HEAD`（须晚于 R01–R12 全部提交）。仓库根 `/Users/xian/Project/editor/Aline`。

## 文件所有权（只有你可写这些文件）

- `AGENTS.md`（**只改「构建与检查」一节中与事实不符的现行陈述**，不改铁律）
- `docs/README.md`（只改事实性过期陈述，追加而非重写历史语境）
- `docs/rust-style-guide.md`（只改末段「仓库尚无 Cargo workspace 或 CI」这类现行陈述）
- `openspec/CODE_STANDARDS.md`（只改 `:153`/`:154` 两行的「当前未启用」）
- `docs/validation/m0-engine-foundation/group-4/step-4.1/report.md`（**只许追加勘误段**）
- `docs/validation/m0-engine-foundation/remediation/R13/run-1/`（证据）

**不得写**：任何 `crates/**`、`scripts/**`、`Cargo.*`、`.github/**`、`openspec/changes/**/tasks.md`、任何历史 `*.command.json`/`*.log`/旧 `report.md`（除上面指定的追加）。不得 `git reset`、不得回退他人改动、**不得重写 Git 历史**。

## 三个失败事实（均为本轮直接核验）

### F-12（严重，维度 D2/D6）：现行文档仍称「无 workspace / 无配置 / 无 CI」

- `AGENTS.md:48`「当前仓库仅有文档。M0 建立 Cargo workspace 后，提交前必跑…」——实际已是 8 成员 workspace（`app` + `crates/` 七个），三条门禁长期在跑。
- `AGENTS.md:55` 末尾「…当前尚无配置或 CI」——实际根 `Cargo.toml` 已配置 `[workspace.lints]` 且各成员 `[lints] workspace = true`；`scripts/check_*.py` 守卫已启用并有 self-test；`.github/workflows/m0.yml` 存在（`workflow_dispatch` + `push: main`）。
- `docs/README.md` 第 3 段「当前无 Cargo workspace，M0 工程守卫及 V-01～V-09 仍未完成」——前半句与事实不符。
- `docs/rust-style-guide.md` 末段「仓库尚无 Cargo workspace 或 CI，文中检查属于实施要求及待建立计划」——同上。
- `openspec/CODE_STANDARDS.md:153`/`:154`「M0 建立，当前未启用」——`check_engineering.py` 与 `check_lints.py` 均已启用并有 self-test。

**必须区分两件事**：CI **配置存在** ≠ CI **已在远端实际运行**。我本轮未核实 GitHub Actions 是否有成功运行记录，因此**不得**写成「CI 已通过」；只能写「已配置工作流 `m0.yml`，远端实际运行状态未核实」。

### F-17（一般，维度 D2）：4.1 报告写 31 例，实际 32 例

- `group-4/step-4.1/report.md:33`「加载时同时绑定用例总数为 31」；
- 金样 `decode-golden.v1.txt` 实际 32 行用例（我用 `awk` 计数为 32）；
- 同报告 `:35` 的独立脚本输出 `checked=32 ok=21 err=11`，与 31 自相矛盾；
- 需**追加勘误**，不得改原文第 33 行。必须确认代码里绑定的常量到底是 31 还是 32（读 `crates/aline-source/tests/**` 的断言常量），勘误要说明「报告文字错、代码/金样是 32」还是「代码也错了」——**只读核对，不改代码**；若代码也错，登记 `待裁决` 并给出最小位置。

### F-18（一般，维度 D2）：`16f8e30` 提交 scope 偏差

- `16f8e30` 只改 `openspec/changes/m0-engine-foundation/tasks.md`（1 文件、2 增 2 删，纯文档勾选），却用 `docs(workspace)`。
- `AGENTS.md:43` 的 RS-05 已确认：「纯文档用 `docs`，公共工程配置/规则用 `workspace`」。
- 需**登记为已有偏差**，不重写历史、不为此制造新提交。登记位置建议放在 `docs/decisions.md` 或 R13 证据的 `scope-deviation.md`（由你判断哪个更符合现行文档职责，并在 `report.md` 说明理由）。

## 目标

1. 逐条修正上面 F-12 的四处现行陈述，保留**明确历史语境**：原文若是「当时快照」性质的表述（如 `docs/README.md` 里其它段落的「本轮」「当时」），不得改动；只改那些**声称当前状态**且已被证伪的句子。
2. 修正措辞必须可核对：写「8 成员 workspace」时给出实际成员名；写「守卫已启用」时给出脚本路径与 self-test 命令；写 CI 时写明「已配置 / 远端运行未核实」。
3. F-17 追加勘误段，给出 31/32 矛盾的三方事实（报告文字、金样行数、独立脚本输出）与代码常量核对结果。
4. F-18 登记偏差，附 `git show --stat 16f8e30` 的真实输出与 RS-05 原文引用。
5. **不提前宣布 M0 完成**：任何改写都不得写成「M0 已闭合」「V-01～V-09 已通过」「守卫已验证充分」。
6. **不重写历史**：所有勘误用「追加 + 引用原文」的方式；不得删除既有句子，除非该句是纯粹的**当前状态声明**且被本轮事实证伪——这种情况要在 `report.md` 逐条列出「删了哪句、为什么它是现行声明而非历史快照、替换成什么」。

## 适用规范

- `AGENTS.md` 铁律与 `:43` RS-05；`docs/development-process.md` 的勘误规则；`docs/maintenance/m0-remediation/tasks.md` §5。
- `openspec/CODE_STANDARDS.md` §6.2 证据边界。

## 约束

- 本批以文档为主；若需跑命令核对事实（`git show`、`awk`、`grep`、读 `Cargo.toml`），记录 argv/cwd/时间/exit。
- 若需 Cargo 命令，`--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`；工具缺失/离线失败记「未核实」。
- 提交（Conventional Commits）：纯文档改动按 RS-05 用 `docs`；若同时改 `openspec/CODE_STANDARDS.md` 这类公共工程规则，按 RS-05 用 `workspace` 并在正文列影响范围。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R13/run-1/`）

- `pre-fix.txt`（四处过期陈述的原文摘录 + 证伪事实，含实际文件/行号）
- `post-fix.txt`（改后原文摘录 + `git diff`）
- `fact-check.txt`（8 成员清单、`[workspace.lints]` 实际内容、守卫脚本清单、`m0.yml` 存在与触发条件、代码常量 31/32 核对）
- `scope-deviation.md`（F-18 登记）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（逐条处置表：原文 → 证伪事实 → 处置（改/追加/保留）→ 理由；未改动项清单）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、F-12 改动条数与位置、F-17 代码常量核对结论（31 还是 32）、F-18 登记位置、是否有「保留未改」的过期陈述及理由、concerns、证据路径。不要贴长日志。
