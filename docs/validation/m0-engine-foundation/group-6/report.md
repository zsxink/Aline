# 组 6（aline-markdown）组级报告

日期：2026-10-31（UTC）。范围：OpenSpec change `m0-engine-foundation` 组 6（`aline-markdown`，任务 6.1–6.8）。

本文件是组级索引，只汇总各步骤报告的路径、测试计数、时间戳与证据目录。任务验收与复核结论在 `review.md`（round 1）与 `review-round-2.md`（round 2），不在本文件；本文件不复述、不替代、不新增复核判断。`docs/engine-design.md` 的 §6.2 记录与 §12.1 记录两处链接指向本文件。

## 1. 逐任务索引

任务名逐字取自 `openspec/changes/m0-engine-foundation/tasks.md` 组 6。计数按各步骤报告文字及其所引日志核对；报告同时给出修复前后两组计数时并列。报告没有写出的数字记为“未记录”，不从日志反推为报告结论。

| 任务（tasks.md 原文） | 步骤报告 | 测试目标与计数（报告记录） | 证据目录 |
|---|---|---|---|
| 6.1 先建 CommonMark/GFM 与 MD-01～06 基础夹具，再接显式 Cargo features/运行期 Options；记录各语法走原生选项或必要 pass 的依据。`cargo test -p aline-markdown spec` 验字面代码区避让、配对文首 Front matter、未闭合不吞全文，公式/Mermaid 只识别来源不引渲染后端。 | `step-6.1/report.md` | `spec`：修复前 8（08:48:11Z）→ 修复后 10（09:42:10Z）；报告记录 10 个用例 | `step-6.1/targeted-tests/` |
| 6.2 建必要 RewriteMap 逐级复合/跨片段事件来源与构树，控制字节 U+0001～U+0008 碰撞、合成围栏、重叠改写及映射失败有原文回退反例；`cargo test -p aline-markdown rewrite` 比较独立源范围预期，不仅比较可见字符。 | `step-6.2/report.md` | `rewrite`：修复前 6（08:48:11Z）→ 修复后 9（09:42:10Z）；报告记录 9 个用例 | `step-6.2/targeted-tests/` |
| 6.3 建块/Gap 与无头 dump 入口（拟 `cargo run -p aline-markdown --example dump_tree -- tests/fixtures/m0/basic.md`）；空文档、嵌套/多空行/无尾换行覆盖测试确认顶层分区精确覆盖且块不含尾分隔。保存 dump 含版本/块/间隙/来源/保护/诊断并人工对独立金样审查，示例同样禁 unwrap。 | `step-6.3/report.md` | 无独立测试目标；`dump_tree` 示例 exit 0（08:48:13Z），报告记录 10 块/9 Gap/3 保护（与日志一致）；`spec` 报告未记录计数（所引日志为 8 passed，08:48:11Z）；修复后 dump 日志存在，报告未记录修复后计数 | `step-6.3/dump-tree.stdout.log`、`step-6.3/dump-tree.stderr.log` |
| 6.4 建 source_at/drawn_at/aligned、局部/全文和 UTF-16 统一映射；`cargo test -p aline-markdown mapping` 验转义、实体/多字符实体、中文/emoji/ZWJ/组合字符、隐藏/合成、重复文字/空白、SoftBreak/HardBreak/br 来源、选区方向；原样精确往返、规范锚点稳定/对齐幂等。最小非法半字符/代理对/旧投影失败，不能要求全源码双射或靠 clamp/backscan 修补。 | `step-6.4/report.md` | `mapping`：修复前 10（08:48:13Z）→ 修复后 16（09:42:10Z）；`mapping-goldens.tsv` 6 例；报告记录 16 个用例 | `step-6.4/targeted-tests/`、`step-6.4/goldens/`、`step-6.4/goldens-check/` |
| 6.5 建有限 HTML 与保护子区间金样：支持标签、无 href、引号内 `>`、交叉嵌套、注释/空白、实体、过滤标签、不完整尾部；`cargo test -p aline-markdown html` 与 protection 过滤验证来源/文字/语义/精确性/保护；脚注引用与普通正文相邻时仅保护子区间，HTML 原文语义不因展示成段落消失。 | `step-6.5/report.md` | `html`：修复前 11（08:48:13Z）→ 修复后 20（09:42:10Z）→ round-2 补 N-F3-2 现状用例后 21（10:32:02Z）；`protection`：修复前 6（08:48:13Z）→ 修复后 6（09:42:11Z）；报告记录 html 21、protection 6 | `step-6.5/targeted-tests/` |
| 6.6 建扫描步数/嵌套/节点/改写/映射预算与取消检查；敌意深栈、重复未闭合 HTML/引号、巨段/巨表及超限反例在分配/循环中终止，保持源快照、不截断；`cargo test -p aline-markdown hostile` 记录注入预算/实际步数和第三方长调用限额盲区，数值不是 V-06 发布限额。 | `step-6.6/report.md` | `hostile`：修复前 10（08:48:13Z）→ 修复后 15（09:42:11Z）→ round-2 补 N-F2-2/N-F2-3 后 19（10:23:36Z）；报告记录 19 个用例；扫描步数探针 09:13:09Z（阈值 80） | `step-6.6/targeted-tests/`、`step-6.6/fix-n-f2-2-n-f2-3/`、`step-6.6/scan-budget-probe-main.rs`、`step-6.6/scan-budget-probe.*.log` |
| 6.7 建全量路径与最小头尾配对/重定位差分用例：前文插入、引用定义/脚注/围栏/配置变化，比较树/Gap/范围/映射/保护/导航；用 reuse 过滤测试记录实际复用命中与安全回退，旧绝对范围反例须失败，不以所有样例全回退冒充优化证明。 | `step-6.7/report.md` | `reuse`：修复前 7（08:48:13Z）→ 修复后 13（09:42:11Z）→ round-2 补 N-F2-1 跨行标签后 18（10:19:53Z）；报告记录 18 个用例 | `step-6.7/targeted-tests/`、`step-6.7/fix-n-f2-1/` |
| 6.8 回填引擎 §6/7/11/12、解析配置及 V-06/V-07 基础夹具；将 SoftBreak、有限 HTML 尚未固化金样逐项列实验结果/缺口，不变更 MD 口径或宣称公式图表能力/AC-09 完成。 | `step-6.8/report.md` | 修复前（08:48:11Z–08:48:13Z，`commands.log`）：spec 8、rewrite 6、mapping 10、html 11、protection 6、hostile 10、reuse 7、全 crate 62；修复后（09:42:10Z–09:42:11Z，`commands-post-fix.log`）：spec 10、rewrite 9、mapping 16、html 20、protection 6、hostile 15、reuse 13、diagnostics 5、全 crate 98；工作区门禁 134→187 passed（见 §3） | `step-6.8/targeted-tests/`、`step-6.8/targeted-tests-post-fix/`、`step-6.8/evidence-post-fix/`、`step-6.8/workspace-gates/`、`step-6.8/workspace-gates-post-fix/`、`step-6.8/backfill-hashes.log`、`step-6.8/environment-per-variable.*`、`step-6.8/goldens-independent.*` |

8 个 `step-6.N/report.md` 均存在（逐文件核对）。修复前计数与日志逐项一致：`spec` 8、`rewrite` 6、`mapping` 10、`html` 11、`protection` 6、`hostile` 10、`reuse` 7、全 crate 62；修复后为 `spec` 10、`rewrite` 9、`mapping` 16、`html` 20、`protection` 6、`hostile` 15、`reuse` 13、`diagnostics` 5、全 crate 98。round-2 复核处置后的最终计数为 html 21、hostile 19、reuse 18、全 crate 108（见 §3 的调度者复跑）。

## 2. 证据目录清单

以下按条目说明用途；路径均相对 `docs/validation/m0-engine-foundation/group-6/`。

| 条目 | 用途 |
|---|---|
| `commands.log` | 修复前命令记录（14 行；测试/示例集中在 08:48:11Z–08:48:13Z，末行为 09:13:09Z 扫描探针） |
| `commands-post-fix.log` | 修复后命令记录（13 行；09:42:10Z–09:42:11Z） |
| `evidence/` | 修复前环境、git 状态、测试清单的 stdout/stderr（6 个文件） |
| `follow-ups.md` | round-1 复核遗留项的调度者处置记录；文首声明不替代复核结论 |
| `review.md` | round-1 独立复核报告 |
| `review-round-2.md` | round-2 独立复核报告 |
| `run-targeted-tests.sh` | 生成修复前定向证据的脚本 |
| `run-post-fix-tests.sh` | 生成修复后定向证据的脚本（写入独立目录与日志） |
| `verify_goldens.py` | 仅用标准库的金样独立校验脚本 |
| `step-6.1/targeted-tests/` | `spec` 修复前 stdout/stderr |
| `step-6.2/targeted-tests/` | `rewrite` 修复前 stdout/stderr |
| `step-6.3/dump-tree.stdout.log`、`.stderr.log` | `dump_tree` 示例输出与空 stderr |
| `step-6.4/targeted-tests/` | `mapping` 修复前 stdout/stderr |
| `step-6.4/goldens/mapping-goldens.tsv` | 6 例映射金样 |
| `step-6.4/goldens-check/` | 仓外探针工程（`probe-main.rs`、`gap-probe-main.rs`、`probe-Cargo.toml`）与运行日志（09:10:14Z、09:12:23Z） |
| `step-6.5/targeted-tests/` | `html`、`protection` 修复前 stdout/stderr |
| `step-6.6/targeted-tests/` | `hostile` 修复前 stdout/stderr |
| `step-6.6/fix-n-f2-2-n-f2-3/` | N-F2-2/N-F2-3 处置证据：定向 hostile 19 passed、全 crate 107、clippy/fmt、探针修复前后输出（10:19:48Z/10:22:10Z） |
| `step-6.6/scan-budget-probe-main.rs`、`.stdout.log`、`.stderr.log` | 扫描步数阈值探针（阈值 80） |
| `step-6.7/targeted-tests/` | `reuse` 修复前 stdout/stderr |
| `step-6.7/fix-n-f2-1/` | N-F2-1 处置证据：定向 reuse 18 passed、全 crate 103、clippy、改动文件 rustfmt 检查 |
| `leader-verify-round-2/` | 调度者 round-2 处置后复跑：fmt/clippy/全 crate 与 19 项工作区门禁 |
| `step-6.8/targeted-tests/` | 修复前全 crate stdout/stderr |
| `step-6.8/targeted-tests-post-fix/` | 修复后 8 个定向目标、dump、全 crate 的 stdout/stderr |
| `step-6.8/evidence-post-fix/` | 修复后 git 状态与测试清单 |
| `step-6.8/workspace-gates/` | 修复前 19 项门禁的 `command.json`、stdout/stderr、`summary.json`，以及 dependency/lint/safety/public-api 夹具与依赖图 |
| `step-6.8/workspace-gates-post-fix/` | 修复后同结构门禁证据 |
| `step-6.8/backfill-hashes.log` | 回填前后 SHA-256 记录（生成 09:45:51Z） |
| `step-6.8/environment-per-variable.*` | 逐变量环境证据（09:23:35Z） |
| `step-6.8/goldens-independent.*` | 金样独立校验输出（7 个夹具/57 区间/0 问题） |

## 3. 定向与门禁结果

| 记录 | UTC 时间 | 证据路径 | 记录结果 |
|---|---|---|---|
| 修复前定向与全 crate | 08:48:11Z–08:48:13Z（`commands.log` 末行 09:13:09Z 为扫描探针） | `commands.log`；`step-6.N/targeted-tests/`；`step-6.8/targeted-tests/` | 命令均 exit 0；spec 8、rewrite 6、mapping 10、html 11、protection 6、hostile 10、reuse 7、全 crate 62（日志逐项求和） |
| 修复后定向与全 crate | 09:42:10Z–09:42:11Z | `commands-post-fix.log`；`step-6.8/targeted-tests-post-fix/` | 命令均 exit 0；spec 10、rewrite 9、mapping 16、html 20、protection 6、hostile 15、reuse 13、diagnostics 5、全 crate 98（日志逐项求和） |
| 修复前工作区门禁 | 08:43:35.845279Z–08:44:04.808130Z | `step-6.8/workspace-gates/summary.json`、`test.stdout.log` | 19 项 exit 0；`cargo test --workspace` 134 passed、0 failed |
| 修复后工作区门禁 | 09:42:36.350886Z–09:43:04.038768Z | `step-6.8/workspace-gates-post-fix/summary.json`、`test.stdout.log` | 19 项 exit 0；`cargo test --workspace` 187 passed、0 failed |
| round-2 复核者独立复跑 | 复核窗口约 09:15Z–10:02Z | `review-round-2.md` §1、§5 | 全 crate 98 passed、0 failed；定向计数与 6.8 证据一致（复核者自述，无独立日志文件） |
| 调度者 round-2 处置后复跑 | 10:27:35Z–10:34:04Z | `leader-verify-round-2/`（`commands.log`、`fmt*`、`clippy*`、`full-crate*`、`workspace-gates/`） | `cargo fmt --all --check` 0、`cargo clippy --workspace --all-targets -- -D warnings` 0、全 crate 108 passed 0 failed、`check_workspace.py` 19 项全 0（`cargo test --workspace` 204 passed） |

以上均为已写入证据文件的记录结果。`leader-verify-round-2/` 的复跑发生在 round-2 复核之后、提交之前，用于确认 N-F2-1/N-F2-2/N-F2-3/N-F3-1/N-F3-2 的处置代码与新增用例在当前工作区可通过门禁；它不构成复核结论，也不改变 `review-round-2.md` 的判定。

## 4. 未覆盖与不宣称

- 平台：仅 macOS Darwin 27.2.0 arm64、rustc 1.98.1；Windows/Linux、其他 target 与 CI 未运行（`step-6.8/report.md` §5；`review-round-2.md` §6.2）。
- V-03、V-05、V-06、V-07 未通过（`review-round-2.md` §6/§8；`step-6.8/report.md` §6 记录未冻结项）。
- AC-09 未通过（`step-6.8/report.md` §1/§6；`review-round-2.md` §8）。
- M0 出口未通过（`review-round-2.md` §8）。
- `tasks.md` 组 6 的 8 个复选框在写入时均为未勾选；复选框状态不等于出口判定。
- 已记录但未冻结/未覆盖：SoftBreak 投影口径、非过滤 void 标签展示口径、空白折叠与块级合成换行口径、真实 UI 命中/IME/灰色只读呈现、局部解析优化与性能收益、第三方长调用不可中断（各步骤报告“未覆盖”节与 `step-6.8/report.md` §6）。
- 复核待处置项（仅 ID，不判断是否已修复）：N-F1-1、N-F2-1、N-F2-2、N-F2-3、N-F3-1、N-F3-2、N-F3-3、N-F3-4、N-F3-5、N-F3-6、N-F3-7、F3-8；见 `review-round-2.md`。
- 处置记录（本报告只指向证据位置，不判断关闭）：N-F1-1 → 本文件与 `docs/engine-design.md` 链接修复；N-F2-1 → `step-6.7/fix-n-f2-1/` 与 `step-6.7/report.md` §3；N-F2-2/N-F2-3 → `step-6.6/fix-n-f2-2-n-f2-3/` 与 `step-6.6/report.md` §1/§3；N-F3-1 → `tree/html.rs` 注释与 `step-6.5/report.md` §2；N-F3-2 → `tests/html.rs` 的 `unsupported_non_void_tags_record_the_v07_pending_asymmetry` 与 `step-6.5/report.md` §8；N-F3-3/N-F3-4/N-F3-5/N-F3-6/N-F3-7 → 各步骤报告与 `docs/engine-design.md`、`docs/v1-architecture.md`、`docs/v1-validation-plan.md` 的更正段落；F3-8 → 待提交，提交后在本文件 §5 追加 SHA。是否关闭由独立复核判定。

## 5. 提交状态

写入时：

```sh
$ git rev-parse HEAD
99867bf2c5775aae4e8ed910713496d4ff84323c
$ git status --porcelain docs/validation/m0-engine-foundation/group-6/
?? docs/validation/m0-engine-foundation/group-6/
```

组 6 证据目录在写入时整体未被跟踪；本文件也属于该未跟踪目录。提交后将在本节追加提交 SHA；写入时尚无提交 SHA。
