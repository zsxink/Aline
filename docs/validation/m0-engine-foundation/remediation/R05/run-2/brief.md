# R05b 派发契约：修复独立复核退回的 2 项严重（F-08 同类静默放行）

基线 HEAD：`128954e`（R03 已提交）。仓库根 `<repo>`。
本批是 R05（`e4a6af0`）的修复轮，**不是重做**：R05 的既有成果（真实 TOML 解析、零外部依赖、`--self-test` 57/0）必须全部保留。

## 文件所有权（只有你可写这些文件）

- `scripts/check_lints.py`
- `docs/validation/m0-engine-foundation/remediation/R05/run-2/` 下你自己新建的证据文件

不得写其它文件（尤其不要动 `scripts/check_safety.py`、`scripts/check_workspace.py`、任何 `Cargo.toml`、任何 `crates/**`、`R05/run-1/**`）。不得 `git reset`、不得回退他人改动。

## 背景：复核退回的三项

独立复核裁决 `REQUEST_CHANGES`，证据见 `docs/validation/m0-engine-foundation/remediation/R05/run-1/review.md`（含主 Agent 的独立复现路径与原始命令）。

### 严重 1：引号点分键与真实嵌套表撞名（静默误放行）

`_flatten_tables`（`scripts/check_lints.py:604-609`）用 `".".join(path)` 生成表键，**不区分**表头路径 `[workspace.lints.rust]` 与 `[workspace]` 下的引号点分键 `"lints.rust" = {...}`；读取处 `:641` 因此可能取到后写入者。

主 Agent 复现（`<tmp>/r05f4`，构造已在 review.md 记录）：

- 根清单 = 真实**弱**表（`unsafe_code="warn"`、`missing_docs="allow"`、`unwrap_used="warn"`）+ `[workspace]` 下的引号点分键（强策略，且顺序在后）
- `python3 scripts/check_lints.py --root` → **exit 0「workspace lint 声明有效」**
- `cargo clippy -p aline-buffer --offline -- -D warnings` → `warning: Cargo.toml: unused manifest key: workspace.lints.rust` + `workspace.lints.clippy`

即：**守卫认为强策略已生效，Cargo 实际完全未生效**，与 F-08 同类。

已知边界（不必处理，但报告里要写明）：若**只有**引号点分键、无任何真实嵌套表，Cargo 侧会因 `workspace.lints was not defined` 报错，不可静默；危险窗口仅限「真实弱表 + 引号点分键覆盖」这一组合。

### 严重 2：三条拒绝路径无 self-test 断言

把 `audit_member_manifest` 的 `set(lints) != {"workspace"} or lints["workspace"] is not True` 弱化为 `not isinstance(lints, dict)` 后，整套 `--self-test` **仍 exit 0**（主 Agent 在 `<tmp>/weakrepo` 复现，输出 `"status": "lint 隔离样例通过"`）。也就是说「多余键 / `workspace = false` / 非 bool 值」三类拒绝没有任何独立用例，弱化实现不会被发现。

### 一般 4：深嵌套逃出异常元组

嵌套数组/内联表 ≥498 层抛 `RecursionError`，不在 `main` 的异常元组（`:975`）内，打印 traceback 而非 `CheckError`。exit 1 仍 fail-closed，但输出不规范。

## 目标

1. **撞名必须 fail-closed**：让表路径与点分键路径可区分；当引号键或点分键会把一个**已存在**的表路径覆盖（或反之）时，抛 `CheckError` 拒绝，不静默覆盖。规则名请具体（例如 `ambiguous-table-path`），错误信息带文件:行号与冲突的两条路径。
   - 真实仓库不含该构造，因此拒绝不得影响正常路径（`python3 scripts/check_lints.py` 必须仍 exit 0）。
   - **不要**靠「按写入顺序取最后一个」或「按写入顺序取第一个」来选一个赢家——那是猜，不是裁决。
2. **补 3 条 `--self-test` 反例**，分别覆盖：`lints` 有多余键、`workspace = false`、`workspace` 为字符串 `"true"` 或数字 `1`。每项**先手写期望再跑**，不得拿实现输出当预期；完成后用「弱化 `audit_member_manifest` 应当变红」自证一次（把变红的实际输出记进证据）。
3. **`RecursionError` 纳入 `main` 的异常元组**（或设显式深度上限后报 `CheckError`），使深嵌套输出为规范的拒绝信息而不是 traceback。
4. **不得回归**：`--self-test` 既有 57 项符合预期、0 不符（含 parser_cases 23、inheritance_cases 4、10 正 13 反）一项都不能减少；新增后 case 总数 ≥ 57 + 3。正常仓库 `python3 scripts/check_lints.py` 仍 exit 0。

## 约束

- 所有 Cargo 命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`，`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」，不要联网安装。
- **离线注意**：本机 `CARGO_HOME` 若指向 admission 隔离目录，其 `registry/` 只有 `CACHEDIR.TAG`（空缓存），任何需要解析依赖的 `cargo metadata` 都会 exit 101。跑 Cargo 对照时请显式 `CARGO_HOME=$HOME/.cargo`（仍 `--offline --locked`），并在 `commands.txt` 里写明这一设置及其理由。不要把它当成产品失败。
- 夹具放受控临时目录或 `--output-dir` 下，不写进仓库受跟踪路径。
- 提交（Conventional Commits，纯工程脚本 → scope `workspace`）。**不要自己 commit**，提交由主 Agent 串行执行。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R05/run-2/`）

- `pre-fix.txt`（修复前：撞名夹具 exit 0 而 Cargo 报 unused manifest key 的实际输出；弱化实现后 self-test 仍 exit 0 的实际输出）
- `post-fix.txt`（修复后：撞名夹具被拒且带规则名；弱化实现后 self-test 变红；正常仓库与 self-test 全绿）
- `commands.txt`（argv、cwd、起止 UTC、exit code，含 `CARGO_HOME` 设置）
- `report.md`（根因、规则设计、逐条断言对照、未覆盖范围）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、改动文件清单、一行测试小结、concerns、证据路径。不要贴长日志。
