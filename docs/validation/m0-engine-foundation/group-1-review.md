# 第 1 组独立复核收口

日期：2026-10-30。根据 AGENTS 路由，由独立 reviewer 对规范、候选辅助守卫、机器账本、留存命令/日志及源码进行只读复核；主 Agent 修改或调度实现者修复，随后独立复核修复结果。

## 发现与修复

| 发现 | 处理与复核证据 |
|---|---|
| 整体 lock 字节一致约束不可用于正式八成员 workspace | 改为受审 GPUI 子图身份、校验和、边、target/features 一致；允许更换候选根/增加合规节点并重跑完整图。正例验证换根与增加干净节点 |
| engineering-governance 修改未附对应 delta，历史 0.3.9 措辞可能与实际审计混淆 | 补完整 MODIFIED requirement/scenario；澄清历史值不是冻结证据，正式接线第 2 组复查 |
| 完整 GPUI metadata 的未选节点 lock.source 未与 metadata.source 对照 | 对每个子图节点校验来源一致与 registry 校验和格式；新增真实 metadata-only wasi 的来源漂移及无效校验和反例。29 个最终负例均预期拒绝 |
| 纯候选命令摘要误记 OS 和围栏不闭合 | 按 sw_vers 原始结果修正 macOS 27.0.1 (26A434)，引用环境 JSON，修复围栏 |

主 Agent 另在源码复核中修正 app-core 探针的 `.ok()?` 错误丢失，改为 `Result<String, ProbeError>`；重新 check/release build/run 通过。另补未批准原始 `gpui` 包拒绝及 app/engine/并存反例，防止把合法 `gpui-pre` 的依赖别名与实际 GPUI 包身份混淆。

## 最终结论

Reviewer 最终只读核验全部修复、60 条结果和 7 个真实 Cargo 图形状检查，未发现仍需阻止第 1 组出口的问题。主 Agent 最终串行复跑候选自测，退出 0，见 [root 命令](./group-1-guard.command.json)与[输出](./group-1-guard.stdout.log)。

独立复核未重复完整构建或联网，也未验证其它平台。候选守卫不是正式 workspace 守卫；真实八成员、所有者/API、lint/catalog 和引擎行为仍按第 2～9 组实施。详细未覆盖边界见[第 1 组完成记录](./group-1-report.md)和[守卫说明](./admission-2026-10-30/guard-summary.md)。
