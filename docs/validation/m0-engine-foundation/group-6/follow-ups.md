# 组 6 复核遗留项与处置

日期：2026-10-31。状态：处置进行中；本文件由主 Agent（调度者）记录处置计划与边界，**不替代**独立复核结论，也不写复核结论。复核结论以 [review.md](./review.md) 为准。

## 1. 来源

独立复核（非实现者）结论：6.1–6.4 通过（M0 范围）；6.5–6.7 按 M0 范围通过但附 F2；6.8 阻塞（F1-1）。F1 1 项、F2 3 项、F3 8 项。复核时 `tasks.md` 6.1–6.8 全部未勾选。

## 2. 处置表

| 编号 | 复核问题 | 处置 | 验收方式 |
|---|---|---|---|
| F1-1 | 6.8 文档回填与 V-06/V-07 基础夹具未完成；`docs/engine-design.md` §6/§7/§11/§12 旧记录未更新；无组 6 报告 | 主 Agent 在本轮补齐：回填引擎 §6/§7/§11/§12 与解析配置记录、V-06/V-07 基础夹具与实验/缺口逐项记录，写 `step-6.1`～`step-6.8` 与组级报告 | 回填前后 SHA-256 表 + `git diff --stat`/`--check`；复核者对回填文本二次核对 |
| F2-1 | 有限 HTML 与 §11.2/§7.1 不一致：过滤集缺 `title/textarea/xmp/noembed/noframes/plaintext`；`input/link/meta/base` 被 void 判断抢先整体隐藏；无空白折叠、无块级标签合成换行；HTML 测试覆盖窄 | 实现子代理改 `tree/html.rs` 与 `build.rs` 的 HTML 分支并补测试；非过滤集 void 标签行为不擅自改，作为 V-07 待定项记录 | `--test html`、`--test protection`；缺口逐项写入报告 |
| F2-2 | 复用全局依赖漏掉块引用/列表内的引用定义与围栏；定义目标改变后仍命中；差分只比文本/种类/范围 | 实现子代理改 `reuse.rs` 与 `tests/reuse.rs`：前缀剥离后识别定义/围栏，保守拒绝；差分比较加入映射来源/精确性、保护范围与可比的导航目标 | `--test reuse`；反例（改目标/删除定义/围栏内容变化）必须拒绝 |
| F2-3 | 无可注入取消接口/用例；`max_mapping_points` 未验证；未记录实际扫描步数与第三方长调用盲区 | 实现子代理在 `budget.rs`/`parse.rs` 增加可注入取消检查与解析统计，补用例；第三方 `pulldown-cmark` 长调用限额仍为盲区，如实记录 | `--test hostile`、新增取消/统计用例；统计值写入报告 |
| F3-1 | ZWJ、重复文字、空白无独立测试；`mapping-goldens.tsv` 无消费者；局部投影 + UTF-16 组合覆盖不足 | 补 `tests/mapping.rs` 用例并让测试解析 `mapping-goldens.tsv` 逐项断言 | `--test mapping` 用例数与新增断言 |
| F3-2 | 多空行块/Gap 测试缺失；dump 仅子串断言 | 补 `tests/spec.rs` 多空行用例 | `--test spec` |
| F3-3 | `RawFallback` 诊断分支无直接用例 | 新建 `tests/diagnostics.rs` 覆盖 RawFallback/BudgetExceeded | `--test diagnostics` |
| F3-4 | 解析选项依据无落盘记录；Mermaid 来源识别无测试；Rewrite“合成围栏”无独立用例 | `tests/diagnostics.rs` 覆盖合成围栏与 Mermaid 仅来源；选项依据写入引擎 §6.2 记录 | `--test diagnostics` + 文档 |
| F3-5 | HTML 金样覆盖窄（仅 `b/i/a`、`script`、`&amp;`） | 随 F2-1 一并补实体/过滤嵌套/零可见用例 | `--test html` |
| F3-6 | 复用差分只比文本/种类/范围；列表/围栏/引用定义仅测顶层；HTML 仅一个方向 | 随 F2-2 一并加强 | `--test reuse` |
| F3-7 | 环境日志只显示 `CARGO_HOME`（macOS `printenv` 多参数行为）；门禁仅当前 macOS，无 CI/其他 target | 环境证据补一条逐变量 `printenv` 输出；平台缺口写入报告“未执行/未覆盖” | 证据日志 + 报告 |
| F3-8 | 组 6 证据未提交且复核期间仍有追加 | 本轮提交组 6 全部证据；报告注明提交 SHA 与证据快照时点 | `git show --stat` |

## 3. 不做的处置

- 不改 `tasks.md` 勾选，直到回填与二次复核完成。
- 不修改 `docs/engine-design.md` 的既有规范句、AC/V/TA/ED/PA 状态或链接；只追加带日期的记录段。
- 不宣称 F2 修复等于 V-07/AC-09/M0 出口通过；V-06/V-07 仍为未通过。
- 不把 `ParseBudget::m0()` 数值冻结为发布限额。
