# M0 整改进度

此文件是 M0 整改的唯一持续进度记录；任务契约与执行协议见[整改任务表](../../../maintenance/m0-remediation/tasks.md)。每次换批、审查退回、验证结论或工作树基线变化后更新本文件。记录不代替原始证据，也不代表 M0 已闭合。

## 当前运行

- 更新时间：2026-11-01 11:47 UTC。
- 仓库：`<repo>`。
- 当前 HEAD：`96024867f7ae949d7328232b05d93ed5bc6033d9`；与原复核基线 `2f1dcea46d5998f4e2ba213e73c11de166f943c4` 不同。
- 工作树：存在先前 R06 safety-results 输出变更/新增、R05 run-2 独立复核文件、R05c/run-1 修复前证据，以及任务表状态编辑。已保留；未 reset、清理或覆盖。
- 当前写入所有权：R01–R13均完成并保留concerns；R14九项单独门禁8/9 exit0，public-api self-test exit1。主机默认缓存 workspace test 361 passed；登记隔离缓存的 unified entrypoint exit1（clippy/test101、dependency/public-api因缺`rust-embed`未核实）。独立 governance reviewer 使用显式 warm cache 串行运行 engineering/lints/safety/public-api normal/self-tests 七项均exit0；两种cache结果分别保存，不混称。
- R14交付草案已完成18 findings、46任务、Requirement/Scenario矩阵、E1–E7、命令包、guard索引和结论。引擎范围review **APPROVE_WITH_CONCERNS**；守卫/文档review仍在收尾。Governance reviewer发现并促成若干交付勘误（F01/F03映射、2.5/4.4/8.3任务边界、R08 review路径等）；matrix仍需修正S11测试名、实现路径目录glob、S13写盘回执对应，并按实际覆盖保守报告。
- F-12残留为`openspec/CODE_STANDARDS.md:3`“当前尚无Cargo workspace”。已向用户请求R14最小更正授权，回复尚未到达，暂不改该行。46项按现有证据重核为26/20/0；六capability真实规格16R/45S，治理delta 1R/2S，R07新增S36解释与brief 44S差异。工作树HEAD仍为`96024867f7ae949d7328232b05d93ed5bc6033d9`，未提交。M0/R14出口未裁决，不能启动M1。

## 批次状态

| 批次 | 状态 | 当前事实 / 后续 |
|---|---|---|
| R00 | 完成 | 初始恢复记录 `R00/run-1/` 保留；本轮 HEAD/工作树/工具链追加为 `R00/run-2/`，现有脏状态和并发新增文件均未清理。 |
| R01 | 完成，复核有 concerns | 原实现和 review 证据在 `R01/run-1/`；T-8-17 仍待 R14 独立核对。 |
| R02 | 完成，复核有 concerns | F-03 已修；R02b 原修复已在 `9602486`。补充首层 Synthetic 起点/内部/终点手写预期测试，markdown crate 127 passed、rewrite 15 passed、clippy/fmt/diff 检查 exit 0；独立 review APPROVE_WITH_CONCERNS，确认 F-01 关闭。仅留一条 P3 建议：永久测试可再补 Aligned/Synthetic EOF fallback；reviewer 的独立探针已覆盖，当前不阻断。 |
| R03 | 完成，复核有 concerns | `128954e` 实现；独立复核 APPROVE_WITH_CONCERNS，无代码阻断；主 Agent 已按 P3 勘误 `report.md` 对修复前失败证据与手写期望的过度表述，原始日志保留。复核原始命令与身份见 `R03/run-1/review-evidence/`；R14 汇总残留。 |
| R04 | 完成，独立复核 APPROVE | 依赖 R01/R02/R03 均满足，redo 语义已裁决无冲突。R04 reviewer 核对四个 UI 文件 SHA、普通与 IME 共用受检发布链、所有拒绝先于 apply/seal；History 私有 pending/时间字段未直接读取，按提交时序补证。主 Agent核对最终相关 crate 测试 291 passed、相关 clippy 与 fmt 通过。review 原文见 `R04/run-2/review.md`，实现证据 `R04/run-1/`。真实 GPUI/IME/三平台/性能仍未验证。 |
| R05 | 完成，独立复核 APPROVE | R05c 修复 D1/D2：孤立策略引号键 fail-closed；含空格目录 self-test 通过。12 个表路径/原 parser/member self-test、9 个定向变异、正常 guard 通过；独立 Cargo target 对照解释共享 target 的 stale success 并保留原始日志。reviewer 核验 8 个 Cargo fresh target；主 Agent 查看实际 diff，重跑完整 `python3 scripts/check_lints.py --self-test` 与 `git diff --check` 均 exit 0。未跑 workspace clippy/test，留 R14。证据 `R05c/run-2/`。 |
| R06 | 完成，复核有 concerns | `60ace78` + R06b `15502eb`；独立 APPROVE_WITH_CONCERNS。D1/D2/D3 关闭；D4 `targets=[]` 缺显式拒绝被登记为非阻断限制。reviewer 独立核 89 文件无扫描漂移、65/65 自测、3 种规则变异均捕获，并确认原证据未被复核污染。主 Agent读取 review/evidence，保留已存在 safety-results 变更。D4 仍列入 R14 残留。 |
| R07 | 完成，独立复核 APPROVE | task 6.9 实现和回填完成：投影绑定的边界/正反移动、完整上下文/身份拒绝、scalar 与 grapheme 分开、Unicode17 Indic 差异、combining/RI/ZWJ/Hangul 手写金样与 4096/256 预算。用户准入精确依赖、版本差异与预算；OpenSpec 审查 APPROVE。主 Agent核对相关三 crate 291 passed、相关 clippy、fmt-check、预热缓存后的 engineering guard exit 0；唯一直接新边 `aline-markdown → unicode-segmentation 1.13.3`。独立 reviewer APPROVE，见 `R07/run-4/review.md`。协议偏差（误执行联网 `cargo fetch --locked`、不能声称 cold-cache offline）完整披露于 `R07/run-3/report.md`。F-10 的 M0 纯边界缺口关闭；V-07/系统字素/IME/跨平台行为仍未通过。回填 engine-design §7.2/7.3/10.2/18.2、架构依赖表与 V-07。
| R08 | 完成，独立复核 APPROVE_WITH_CONCERNS | 14 行独立 TSV 覆盖空删除5、粘贴6、HTML3；拒绝完整状态与 redo 已核。首审 BLOCK 的历史证据缺口已收窄为不可追认事实：third-run 只证实 `VisibleTextChanged`，fourth-run 只定位 `del_body/RereadFailed`，原 TSV 行未保留，输入字段和换夹具原因不推测。新增环境原始记录：`env-cargo-version`/`env-rustc-version` 均为 1.98.1；显式 env 下测试27 passed、Clippy/fmt/diff-check exit 0。reviewer 确认新记录索引完整一致；历史 TSV 输入限制与 workspace 门禁留 R14。边界样例不代表 V-07 已冻结。 |
| R09 | 完成，独立复核 APPROVE | `72c8bc6`；F1 `716f788` 行族修复加 `diagnostics.rs` 持久敏感词回归；F2 74行勘误；F3 公开 API 不可达登记。首轮补充 reviewer APPROVE_WITH_CONCERNS 后建议回归；主 Agent 核对 diff 与定向测试 1/1、diff check 通过，另一 reviewer 独立核验用例 APPROVE。证据见 `R09/run-2/`；workspace 全门禁留 R14。 |
| R10 | 完成，独立复核 APPROVE_WITH_CONCERNS | 前置 R02/R03/R07/R09 已完成。worker 新建不调用 aline-markdown 映射 API 的复算器；51 行（source identity、drawn、49 segment）逐行 diff 为0，共197字节；Hidden21/Exact24/Aligned1/Synthetic3 分类记录。工具测试、Clippy、fmt、依赖树检查完成。旧报告原77行、19个既有金样/探针/日志/R09文件 hash+size+mtime 保持不变，54项当前源码输入 hash 稳定。主 Agent已 CodeGraph 探索生产解析链并核对独立工具/报告；Sol reviewer 复核原始摘要与保全证据，APPROVE_WITH_CONCERNS，详见 `R10/run-2/review.md`。一次初始 fmt argv 漏 `--locked/--offline`，环境离线，后续锁定离线 fmt-check 通过，原日志保留。限 basic fixture，不宣称盲写或 V-07/M0出口。 |
| R11 | 完成，独立复核 APPROVE | 首轮 REQUEST_CHANGES 及全部 findings 关闭。run-3 安全 collector、fake runner 7项自测、17×3新采集、4条历史 env 字段补记、窗口/regex 勘误与 README hostname替换完成；run-1 collector/commands/legacy JSON原样保留，仅报告追加勘误。run-4 reviewer 独立核验51次写盘前脱敏、51日志结构、历史哈希、README全文件字节、schema/链接并通过；敏感命中0。主 Agent `git diff --check` 与 `openspec validate --all --strict --no-interactive` 通过。跨平台、旧时点与CI缺项仍保留。 |
| R12 | 完成，独立复核 APPROVE | 依赖 R05/R06 已满足。修改 check_lints/engineering/engineering self-test/public API 四脚本；六个曾缺锁依赖执行点均补齐 `--locked`，新 fixture 初次 `generate-lockfile --offline` 独立登记；public API 16 条命令 JSON 均含 argv 派生字段。最终 AST 25 调用点、16/16依赖检查 locked、缺锁0、初始生成3；lint normal/self-test、engineering 97/97、public API 8/8、safety 65/65 与 pycompile/diff 均通过。首次 public API 因隔离 CARGO_HOME 缺索引失败，原始证据保留；预热缓存离线重跑通过。主 Agent追加 engineering guard report 原勘误和后续纠正，reviewer APPROVE；证据 `R12/run-1/`，复核 `R12/run-2/review.md`。workspace 门禁留 R14。 |
| R13 | 完成，独立复核 APPROVE_WITH_CONCERNS | F-12 四处现行文档声明已按 scope 更新；F-17 报告勘误为 32；F-18 scope 偏差登记。HEAD 仍为 `96024867…` 且未提交，dirty inputs 已记录；未保留的 pre-fix/7条 attempt-1 stdout/stderr 已披露，摘录另与 HEAD 复核。**残留：CODE_STANDARDS.md:3 仍说当前无 workspace**，R13 权限只允许改 153/154；列入 R14，不宣称 F-12 全量关闭。独立复核见 `R13/run-2/review.md`。 |
| R14 | 进行中（全量验证与出口复核） | 九项单独门禁8/9通过；当前workspace 361 passed；登记隔离cache unified exit1，warm-cache独立守卫七项通过。tasks-46 26/20/0；矩阵计数16R/45S、治理delta1R/2S，名义场景行47；39个场景测试名在本轮输出中命中，但审查要求避免将命名测试升格为完整scenario通过。S11/S13和实现路径需最后修订/复核；守卫/文档review、F14总复核尚未签结。F12与T-8-17待裁决。详见`R14/run-1/`。 |

## 当前下一步

1. 完成matrix最后修订并由独立reviewer核对所有Scenario名称、实现路径及WHEN/THEN覆盖边界；收齐review原始输出。
2. 更新 final-commands、findings、exits、conclusion、handoff 与 tasks.md 交付链接；运行最终文档diff-check/链接检查，给出M0/M1结论，不提前接线。

## 当前待裁决

- T-8-17 undo 后新草稿与 Buffer redo 分支的失效时序仍待独立确认，不借 R04 推断其策略。
- 原复核无法追认的历史证据（旧 TSV 来源、旧采集缺项等）只可标记证据不足，不补造历史。
