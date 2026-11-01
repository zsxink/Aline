# R10：F-06 独立推导与探针身份勘误

日期：2026-11-01。本目录所有新证据均为本轮产物，不声称早于实现。
状态：**DONE_WITH_CONCERNS**，交主 Agent 核验；关注点见 §8。未提交，遵照派发者禁止 commit 的要求。

## 1. 结论与范围

原 6.4 报告第 60 行把直接消费被测 `aline_markdown::parse` 的探针称为「独立工具复算」，
定性错误。本轮保留原文，在[原报告末尾](../../../group-6/step-6.4/report.md#6-勘误2026-11-01r10)
追加 14 行勘误：原 77 行逐字节不变，新增范围为 78–91 行，勘误标题在第 79 行。
同目录只新增 [R10 索引](../../../group-6/step-6.4/goldens-check/R10-evidence-index.md)。

对当前 R09 同夹具独立重算，51 条可比行全部一致：1 条 source（含 identity）、
1 条 drawn、49 条 segment。绘制文字 197 字节；Hidden 21 / Exact 24 / Aligned 1 / Synthetic 3。
差异 **0 处**，没有新增/删除行，没有区间、精确性、绘制文字或身份差异；无待裁决条目。
本批只补当前无头映射证据及事实勘误，不修改引擎行为、API、依赖或验收口径。

适用：M0 映射证据、F-06/D2/D5、E3 与 V-07 的局部无头证据；依据
`docs/engine-design.md` §7、工程治理的「测试和交付必须有独立可信证据」场景、
开发流程及整改任务 §5 的只追加/保留历史协议。普通具体函数足以完成审计；
状态归属单次推导函数，输出仅供审查，不承担应用编辑、持久化或后台调度职责。

## 2. 独立推导：从原字节与第三方事件重算

源码：[independent/src/main.rs](independent/src/main.rs)，清单与锁：
[Cargo.toml](independent/Cargo.toml)、[Cargo.lock](independent/Cargo.lock)。
唯一直接依赖是 `pulldown-cmark =0.13.6`、`default-features=false`；独立 `[workspace]`
不继承 Aline workspace。完整依赖树见 [independent-dependencies.stdout.log](independent-dependencies.stdout.log)：
闭包只有该工具、pulldown-cmark、bitflags、memchr、unicase，**没有任何 Aline crate**。

代码只导入 `std` 和 `pulldown_cmark`；唯一文件输入是 argv 指定的 UTF-8 原文件。
没有 `aline_markdown` 导入、解析/映射调用、子进程、源码包含宏、旧 TSV 或期望文件读取。
[源码检查原始输出](independent-source-review.stdout.log)保全导入与清单检查；
这项字符串检查不是普适安全证明，独立性依据还包括完整依赖树和本轮逐函数人工阅读。

推导过程：

1. 读取原始字节，按开闭 `---` 分隔行识别 front matter，推导正文起点；不写死长度 19 或其它 fixture 偏移。
2. 第三方 Parser 启用表格、脚注、删除线、任务列表、数学、GFM，取得 offset 事件；
   事件区间加正文基址回到原字节坐标。原始事件流见 [independent-events.stdout.log](independent-events.stdout.log)。
3. 按事件原有顺序输出可见文本；Text 必须与其原字节切片一致，否则拒绝未实现的解码。
   Inline code 以 payload 绘制、完整来源区间记 Aligned；M0 math/脚注引用按完整源码切片 Exact。
4. 每个可见事件前未消费的源码形成一个 Hidden，事件间不因文字相同而合并；最后隐藏剩余源码。
   表格自第二单元格起合成 tab，计数跨行、遇新表重置；合成先输出，不推进源码消费游标，
   因而其锚点可位于随后 Hidden 来源区间内部，不能强行按来源起点重排。
5. FNV-1a64 对全部原始字节独立 fold，不使用 SnapshotId、指针或时钟；revision 0 是新快照的明确比较上下文。

规则来源是引擎 §7 的四种关系与当前 M0 的 math/脚注/整表 tab 约定。
编写前已阅读 R09 既有推导和测试材料，因此**不声称盲写期望**；独立性指算法和数据路径
不使用被测实现，而不是声称对既有结论一无所知。R09 的历史片段表和常量不是程序输入，
当前坐标、文本、摘要全部由字节和新事件迭代计算。

[独立原始 stdout](independent-output.txt) 与 `independent-run.stdout.log` 字节相同；
argv/cwd/起止时间/exit 在 [independent-run.command.json](independent-run.command.json) 与 [commands.txt](commands.txt)。
原始 stdout 保持纯 dump，不把元数据混入待比较数据。

## 3. 第二宿主复算：同一实现在另一工程消费

既有 `group-6/step-6.4/goldens-check/probe-main.rs` 的当前第 3 行直接导入
`aline_markdown::{ParseBudget, ParseConfig, parse}`；后续通过 parse/projection/segments/source_at
打印被测结果。当前逐行源码取证见 [second-host-source.stdout.log](second-host-source.stdout.log)。
brief 的“第 2 行”并非当前实际行号，不影响直接导入这一事实。

该探针是「同实现的第二宿主」，可支持当时仓外消费同实现能得到对应输出的有限事实，
不能证明映射算法正确、不能充当独立期望来源，也不能证明旧 TSV 的生成方式或先行时间。
本轮未重跑既有探针，未宣称其旧所有样本已重新核实。旧源码与日志保留不动；
这类证据的历史价值与本轮独立推导分开记录。

## 4. 实现自证：只作比较对象

本轮从当前工作树执行原 `aline-markdown --example dump_tree`，原始完整 dump：
[impl-output.txt](impl-output.txt)。它来自被测实现，不是期望来源。
[implementation-current-repeat.command.json](implementation-current-repeat.command.json)
记录实际离线命令、工作目录、环境、时间和退出码。

为保留 R09 格式，完整 dump 不做修改；[compare.py](compare.py)仅按 `source `、`drawn `、
`segment ` 三种前缀选行，保留原顺序与行字节，生成 [impl-comparable.txt](impl-comparable.txt)。
R09 的 identity 是 source 行字段，没有创造一个不同的 identity 行格式。
block/gap/protected/config 等行族保留在原始 dump，但不在本批独立算法验收范围。

派发时 HEAD 为 `96024867f7ae949d7328232b05d93ed5bc6033d9`，工作树已有他人未提交变更，
不能只凭 HEAD 声称构建身份。实际输入以 [current-input-sha256.json](current-input-sha256.json)
的 54 项源码/清单/锁指纹及 [current-source-diff.stdout.log](current-source-diff.stdout.log) 为准。
首次与重复实现输出相同；重复运行前后对这些输入的指纹复验无变化。

## 5. 输入身份和逐行差异

输入：`crates/aline-markdown/tests/fixtures/m0/basic.md`，319 字节。
SHA-256：`4da2cff1c689141b3c8af8dec66ffc4825e3f0f183025d79d5edd981d2197bc5`。
完整环境/版本见 [fixture-identity.txt](fixture-identity.txt)：Rust/Cargo 1.98.1，macOS aarch64，Python 3.9.6。
FNV-1a64 两方均为 `0xacb96228ce0d7842`；它不是密码学身份保证，本轮以 SHA-256 绑定输入。
时间仅为运行元数据，不充当内容身份。

[diff.txt](diff.txt) 是逐行 unified diff 原始结果，0 字节；
[system-diff.stdout.log](system-diff.stdout.log) 是系统 `diff -u` 的独立复核，同样 0 字节、exit 0。
[diff-summary.json](diff-summary.json)列出零差异清单（`differences=[]`）、行数和精度计数；
两份可比输出 SHA-256 均为 `e96bbcaef10ad74800928feb5114d1d90cf5d11f04ae7af579843668deb61e60`。
没有逐条差异需分类，也没有因一致性而追认旧历史。

## 6. 验证与命令链

所有实际命令记录见 [commands.txt](commands.txt) 的 JSONL；每条有独立 command.json、stdout/stderr。
使用现有默认缓存 `<cargo-home>`，环境 `CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`，
构建产物隔离到 `<tmp>/aline-r10-independent-target`；没有联网下载或安装。

| 检查 | 结果与证据 |
|---|---|
| 初始新工程 lock | `initial-lock-generation` 单独登记，`cargo generate-lockfile --offline` exit 0；按 brief 初始 lock 例外没有 `--locked` |
| 独立算法运行/事件流 | `independent-run` / `independent-events`，`--locked --offline`，均 exit 0 |
| 独立算法测试 | [independent-tests.stdout.log](independent-tests.stdout.log)：4 passed / 0 failed；重复文本不同来源、跨行并跨表 tab、未实现解码/未闭合 front matter 拒绝、已知 FNV 向量 |
| 独立工具 Clippy | [independent-clippy.stderr.log](independent-clippy.stderr.log)：all-targets、`-D warnings`，exit 0 |
| 独立工具格式复验 | [independent-fmt-check.command.json](independent-fmt-check.command.json)：`cargo --locked --offline fmt … --all --check` exit 0 |
| 当前实现运行两次 | `implementation-run` / `implementation-current-repeat`，`--locked --offline`，exit 0；原始输出全等 |
| 比较 | `compare` / `system-diff`，均 exit 0，51 行全部一致 |
| 历史/输入复验 | [verify-evidence.stdout.log](verify-evidence.stdout.log)：19 个历史文件 SHA/字节/mtime 未变，旧报告前缀未变，54 项当前输入指纹未变 |

首次格式化命令 `independent-format` 的 argv 遗漏 `--locked --offline`，这是本轮执行参数偏差；
当时环境仍为 `CARGO_NET_OFFLINE=true`，命令仅格式化新工具，没有解析/构建或下载。
不删除或改写该日志；随后带完整 Cargo 全局 flags 的格式复验已通过。该偏差与初始 lock 例外分开登记。

新增源码是仓外审计工具，未修改仓库 crates；因此只对该独立工具执行 fmt/Clippy/test。
未另行重复全 workspace 门禁，统一最终工作树门禁由主 Agent/R14 协调。
完成上述 Cargo 命令后已通知主 Agent 不再启动 Cargo，以免与统一检查争用。

## 7. 勘误及不造历史

追加节明确引用错误原句、撤回独立推导定性、限定第二宿主证据能/不能证明的内容，
并指向本轮独立源码、原始 stdout、事件流、diff 和夹具身份。
[historical-baseline.json](historical-baseline.json)保全操作前历史文件 SHA/字节/mtime，
[step-6.4-report-before.txt](step-6.4-report-before.txt)保全原报告供只追加校验。
校验脚本为 [verify-evidence.py](verify-evidence.py)，结果见上表。

**不追认旧 TSV 的创作方式，不补造先行时间**。本次未改旧 TSV/日志的内容或 mtime，
未写 crates、scripts、OpenSpec；旧失败、原错误措辞和原证据都仍可复查。

## 8. 限制、关注点与后续

- 独立工具是本次 LF/UTF-8 `basic.md` 审计器，不是完整 Markdown 实现；未独立覆盖旧六例 TSV、实体/转义解码、HTML、任务框、BOM/CRLF、预算/回退或所有非法映射输入。
- 它与产品共享第三方解析器 pulldown-cmark；可独立验证 M0 投影/身份推导，不能证明第三方解析器本身无缺陷。当前 M0 math/脚注/跨行 tab 口径也不因此升级为产品/UI 冻结。
- 本轮可复算当前输出，但没有创作时点的盲写证据；明确读过既有材料，旧 TSV 的实际生成链仍不足，不能恢复历史真实性。
- 首次格式化 argv 的 flags 遗漏如 §6 保留；全部解析/构建/测试命令均显式离线锁定，未发生冷缓存失败或下载。
- 当前运行只有 macOS aarch64；Windows/Linux、真实 UI/命中/IME、V-07、AC、M0 出口未因此通过。独立最终审查及 CodeGraph 汇总同步由主 Agent 安排，本工具自述不替代它们。
