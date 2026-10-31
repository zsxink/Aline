# 9.6 独立审查与现行文档回填

基准：`16f8e30` 后尚未提交的工作树。9.6 独立 reviewer 未参与实现或 E1–E7 证据编制；本文由文档回填 Agent 根据主 Agent 转交的独立结论和实际文件整理，不能作为实现者自行出具的 review 结论。当前记录不将任务勾选、报告存在或归档当作 M0 出口验收。

## 独立复核的事实与处理

reviewer 首轮指出：9.3 的 E3/E5 标签与 proposal 不一致、E1 把 plugin 的允许依赖误称为实际消费者，以及 ui→plugin、plugin→edit、plugin→app-core 三条无消费者边。标签已按 E2 无头 API/预算、E3 dump/原始范围、E5 覆盖/映射修正；E1 文案按源码修正；三条边从 manifest/lock 删除并重新执行统一门禁，plugin 仍作为八成员之一保留为空骨架。早先无消费者的 ui→edit 直接边也已清理。

清理后 reviewer 复判认为 9.1–9.5 的证据可闭合，未发现剩余可操作代码问题；其核查覆盖唯一 DocCore 所有者/写入路径、不可变资源引用与保活、配置/request/关闭/模式失效、请求代次耗尽时的拒绝原子性、依赖精确准入、五引擎隔离及证据来源。普通具体类型、函数和显式状态转移足以支撑当前范围；没有新增 Registry/Service、通用事件框架、依赖或 unsafe 例外。DocCore 仍只持一个 StringBuffer/History 和至多一个打开块；尚无通用 edit 计划到 DocCore 的消费入口，真实文件/资源协调器留后续。

reviewer 对清理后记录报告：19/19 exit 0、312 passed；77 条依赖记录、53 个安全用例及 8 个 API 编译例符合各自期望；74 项当时指纹匹配，9.2–9.5 共 112 个当时本地链接存在。reviewer 只读，未独立落盘 stdout/stderr；这些计数是其观察记录，不冒充另一次完整门禁。独立定向执行 14 个 `document_followups` 的时窗为 2026-10-31T15:00:19Z–15:00:20Z，exit 0；该次发生在最终依赖清理门禁前，也未保存独立原始输出。

复判当时 `git diff --check` 曾报告 `crates/aline-plugin/Cargo.toml:9: new blank line at EOF`，exit 2。该告警是实际历史结果；主 Agent 随后仅移除了本次删除依赖段引入的文件尾空行，证据整理者刷新了 manifest 字节指纹，未把旧检查改写为通过。后续检查以其真实日志为准。此处记录的是工作树检查，不能替代或编造 `git diff --cached --check` 的结果。

## 原始证据入口

| 出口分项 | 证据 |
|---|---|
| 正式统一门禁 | [9.1](../step-9.1/report.md)、[清理后 summary](../step-9.1/workspace-gates-after-cleanup/summary.json)、[入口命令](../step-9.1/formal-run-after-cleanup/commands.log) |
| E1 工程/依赖/lint/catalog | [9.2](../step-9.2/report.md) |
| E2 无头 API/预算，E3 dump，E5 覆盖/映射 | [9.3](../step-9.3/report.md) |
| E4 AC-01～04 基础，E7 输入/组合协议 | [9.4](../step-9.4/report.md) |
| E6 V 未测清单与 M1/M2 交接 | [9.5](../step-9.5/report.md) |
| CodeGraph 检查 | [index-check 命令](./index-check/commands.log)与同目录原始输出；最终是否再次同步以主流程记录为准 |

清理后正式门禁实际时窗为 2026-10-31T15:02:22Z–15:03:05Z，Rust 1.98.1、指定离线 Cargo home、当前 macOS arm64 主机。Clippy/test 显式带 `--locked --offline`；隔离夹具部分辅助 Cargo 命令只有 offline、早先依赖清理遗漏 locked 和原始输出的操作偏差均在 9.1 如实说明，不能把它们隐藏或计作合规正式命令。

## 现行文档回填

- [架构 §13](../../../../v1-architecture.md#13-m0m6-实施顺序与阶段出口)：分项证据入口、依赖整洁、M1 原生/字体/性能/真实语法顺序及 M2 前 V-08 独立出口。
- [引擎 §15](../../../../engine-design.md#15-与-m0m6-的关系)：无头机制证据、资源登记前提、快照来源身份及真实平台/命令/保存缺口。
- [Rust §19](../../../../rust-style-guide.md#19-待验证事项与收口时点)：实际工具/API 守卫结果、作用范围、命令偏差及宏/第三方/平台盲区。
- [验证计划](../../../../v1-validation-plan.md)：区分已执行基础分项与未完成完整 V 项，更新 V-01/V-06/V-07 和结果表，不标 V-01～V-09 通过。

本次文档回填仅重新读取上述原始日志，未重跑 Cargo。随后独立 reviewer 核对回填及证据引用，未发现事实或规范阻断；`openspec validate m0-engine-foundation --strict --no-interactive` 和最终 `codegraph sync/status` 均 exit 0，索引显示已更新，实际命令及原始输出见 [final-check](./final-check/commands.log)。这些是分项检查，不代表 M0 或 V 项整项通过。

**最终工作树补充验证：** 清理 plugin manifest 文件尾空行后，2026-10-31T20:52:17Z–20:52:21Z 在同一 macOS 主机重新执行 fmt、workspace Clippy 和 workspace 测试，三项均 exit 0，测试合计 312 passed。命令及原始 stdout/stderr 见 [final-worktree-gates](./final-worktree-gates/commands.log)；这次只重跑三项，不是重跑全部 19 项统一门禁。

**最终统一门禁：** 2026-10-31T20:53:11Z–20:53:40Z 对当前代码和配置重跑完整统一入口，19 项均 exit 0，workspace 测试 312 passed；[summary](./final-workspace-gates/summary.json)、[逐项命令](./final-workspace-gates/commands.log)与原始输出分别保留。随后 OpenSpec 严格校验、CodeGraph sync/status、工作树及暂存区 `git diff --check` 均 exit 0，见 [入口与收尾命令](./final-full-run/commands.log)及其原始输出；当时暂存区为空，工作树 diff 检查不覆盖未跟踪文件。此前 9.1 记载的离线夹具内部部分 Cargo 命令未带 `--locked` 的限制仍适用，本次不将其描述为全程均带该参数。

## 未执行与交接约束

当前本机无头与 create-only 原语不证明原生窗口、布局命中、灰色只读呈现、系统字素/真实 IME/焦点/候选窗、通用命令/跨块/结构命令、现有文件 metadata 保全替换、自动保存/恢复/资源事务、Release 性能或其他 OS/架构。资源齐备依赖宿主事先完整登记；保守强持有不验证文件存在、恢复图片可读或安全清理。

M1 仍须先取得原生 macOS 单窗冒烟、中文字体回退、正式性能基准及真实语法/命中样本；Windows 10、Intel Mac 和 Linux X11/Wayland 目标保留。M2 前身份/事务方案与保存/1 秒自动保存、外改/可撤销重载、10 秒恢复及恢复图片可读的最小安全闭环为独立出口。完整 V-01～V-09、AC 和 M0 出口均不据本包整项判通过；本包也未提前实现或自动授权 M1+。
