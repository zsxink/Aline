# 9.5：E6 未测清单与 M1 交接

本包落实 [proposal E6](../../../../../openspec/changes/m0-engine-foundation/proposal.md)要求的“V 未测能力明列”。它核对当前代码、正式 9.1 原始日志与现行验证契约，记录 M0 基础结果和后续入口；不调整 PRD/AC/V 口径，也不自行授权 M1 接线或判 M0 出口通过。

## 事实基准与分项结果

[9.1 首轮正式执行](../step-9.1/formal-run/commands.log)为 2026-10-31T14:52:24Z–14:53:10Z、exit 0；[summary](../step-9.1/workspace-gates/summary.json)记录 19/19 exit 0，[test.stdout](../step-9.1/workspace-gates/test.stdout.log)合计 312 passed、0 failed。该次及[原指纹](../step-9.4/source-fingerprints.tsv)、[原测试观察清单](../step-9.4/observed-tests.json)保留为依赖清理前记录。

**当前最终来源：** 依赖清理后于 2026-10-31T15:02:22Z–15:03:05Z 重跑，[命令](../step-9.1/formal-run-after-cleanup/commands.log) exit 0，[汇总](../step-9.1/workspace-gates-after-cleanup/summary.json)为 19/19 exit 0，[测试 stdout](../step-9.1/workspace-gates-after-cleanup/test.stdout.log)仍合计 312 passed、0 failed。当前 [9.4 指纹](../step-9.4/source-fingerprints-after-cleanup.tsv)覆盖清理后的 `Cargo.lock` 和各成员 manifest，[测试观察清单](../step-9.4/observed-tests-after-cleanup.json)从最终日志确认同样的 180 个相关外部测试。环境仍为 Rust 1.98.1、指定离线 Cargo home、当前 macOS arm64 主机；本步骤只读取日志，没有重新执行 Cargo、平台或性能实验。[本次输入/链接清单](./handoff-inputs-and-links-after-cleanup.json)和 [inspection-after-cleanup](./inspection-after-cleanup/)独立保留清理后的检查，旧记录未覆盖。

最终 manifest 补充：[manifest-whitespace-final.json](./manifest-whitespace-final.json)保存文件尾空白修正后的当前指纹；它与清理后原始快照的比较仅发现 plugin manifest 少一个末尾 LF，其余 manifest/锁文件指纹相同。原清理后指纹、快照及 15:02 门禁原始输入记录均保留，未重新运行 Cargo；核查命令与原始输出由 9.4 的 `verify_manifest_whitespace.py` 和所属 commands.log 保存。

V-01 的设备/目标登记仍以[现行验证计划](../../../../v1-validation-plan.md)为准：当前有 Apple M5 / macOS 27.0.1 / aarch64 主机；Intel Mac、Windows 10 和 Linux 原生验证环境尚未接入。macOS 14、Win10 22H2、Ubuntu 24.04 LTS、Debian 12 及 X11/Wayland 是候选，不是支持成绩。目标解析成功、target 已安装和 `--all-targets` 都不代表其他 OS 原生构建/运行通过。

| V 项 | 已有 M0 基础事实 | 未完成能力与后续阶段 |
|---|---|---|
| V-01 兼容与发布 | 候选工具链/精确 GPUI 来源、当前主机登记、目标依赖图与无窗口工程门禁 | M1 先原生 macOS 冒烟；Intel/低系统版本、Win10 原生、Linux 发行版/显示会话、安装/签名/关联/单实例/真实 IME 在后续取证；M5–M6 全平台矩阵。未整项通过 |
| V-02 导出 | ED-01 的 Save/SaveAs/Export 等待纯协议已模拟，不生成产物 | M2 接保存、M4 导出真实 PDF/Word/HTML 与阅读器检查；矩阵、分页/主题/资源/取消失败均未测。未整项通过 |
| V-03 媒体/协议 | 公式/Mermaid 仅识别来源；有限 HTML 为引擎文本语义 | M1 真实语法展示，M4 图片/公式/图表/高亮后端与协议样本，实际渲染/尺寸/取消/离线未测。未整项通过 |
| V-04 主题/字体 | catalog 已无头验证，不代表主题或字体能力 | M1 中文字体回退实验；M4 前主题包格式/限额和原生加载/失败策略，M6 跨平台。未整项通过 |
| V-05 性能 | StringBuffer、全量解析、最小复用有正确性测试；没有正式 Release 性能样本 | M1 首个基准：32/128 KiB、1/4 MiB、巨型单块/表格、冷/热、P50/P95/停顿/内存；M6 发布门禁。没有数字可冻结，也未授权高频路径同步昂贵计算。未整项通过 |
| V-06 预算/降级 | source 编解码/文件原语、buffer 原子预算、markdown 敌意输入/取消/原文回退、edit 预算拒绝均在 9.1 测试运行；注入小预算只验证出口 | 文件/单块/表格/图片/公式/图表/ZIP/字体资源上限、时限和 UI 汇总仍未测；第三方长调用不能凭合作取消测试宣称可中断；M1–M4 测量，M6 回归。未整项通过 |
| V-07 语法/编辑 | 块+Gap、来源映射/保护、有限 HTML、最小复用、代表命令、DocCore 输入组/组合/身份纯协议有基础执行，详见 [E4/E7](../step-9.4/report.md)与组 6/7/8 原始日志 | SoftBreak、有限 HTML 空白/块级换行/未知与 void 标签口径仍待真实样本冻结；M1 真实语法、命中与灰区；M2 输入/选择/搜索，M3 命令/表格/格式化，M4 前统计。系统字素、真实 IME/焦点、跨块/结构命令未验。未整项通过 |
| V-08 文件/资源/恢复 | 文档/工作版本/快照来源/配置/request 的采纳协议、冻结草稿与已登记资源、ED-01、自动保存延后/独立恢复读取、create-only Written 分类是设计准备 | M2 前身份/事务方案和最小安全闭环是独立出口：保存/1秒自动保存、外改/可撤销重载、10秒恢复与恢复图片可读、取消/失败；现有文件替换/metadata、权限/磁盘满/断电/竞争、多资源事务均未测。M4 完整迁移、M5 路径/多窗。未整项通过 |
| V-09 视觉/焦点 | 未创建应用窗口，M0 无视觉样本 | M1 主窗基准，M4–M5 设置/主题/菜单/门控/默认值，M6 中英文/键盘/平台验收。未整项通过 |

本表的已测结论只涉及明确列出的基础。当前 [V 结果表](../../../../v1-validation-plan.md#结果记录)仍未将 V-01～09 标为通过；本包不将既有“未执行”历史总述解释成没有任何 M0 分项日志。

## M1 交接顺序与验收输入

| 顺序 | M1 消费接口/工作 | 必须取得的新证据 | 门槛与保留目标 |
|---|---|---|---|
| 1 | 锁定 GPUI 上建立原生 macOS 单窗，只读打开/渲染/点击映射；沿用当前源码快照、Parsed 和映射层 | 原生启动、关闭/目标存活、打开文件与当前显示来源、点击/字符边界、无窗口核心回归；实际版本/设备/命令/结果 | 依赖 9.6 对 M0 出口证据的独立核查；本包不提前实现窗口，不用历史 Zed API 代替锁定源码 |
| 2 | V-04 中文字体回退、V-09 主窗与只读灰区 | 中文/英文/emoji、缺失字形和字体失败，真实 DPI/最小尺寸/焦点/键盘样本 | 不静默更改字体或产品口径；Intel Mac、Win10 留在目标矩阵 |
| 3 | V-05 首个 Release 基准与变高虚拟化 | 设备/OS/Release/fixture hash、冷/热、P50/P95、停顿/内存、1 MiB 实际滚动；巨段/巨表与复杂输入 | 不从正确性测试推导性能，不把候选数值当门禁；同步调度阈值须实测 |
| 4 | V-03/V-07 真实语法夹具与 UI 命中 | SoftBreak、HTML/过滤/实体/未知标签、脚注/目录/Front matter、块+Gap 与点击/只读呈现；冻结输入/期望/原始输出 | 仍只有唯一 pulldown-cmark；不以有限 HTML 无头现状直接冻结产品语义 |
| 5 | 按 V-01 顺序扩展平台证据 | Windows 10 原生构建/运行/输入法/安装；Linux 实际发行版与 X11/Wayland；Intel Mac 与最低系统版本 | 先 macOS 后 Windows/Linux，正式 V1 三平台同发；安装 target 或 all-targets 不代替实际平台运行 |

## M2 前独立安全出口

M1 的只读工作不能消化或延后 V-08：M2 前须形成身份/事务方案与可模拟 I/O 测试，再验证打开→输入→搜索→切模式→保存→重开的安全闭环。DocCore 目前未消费通用 edit 计划、不持有完整文件协调器；资源引用完整性依赖宿主先登记，保守持有不证明实际资源可读或恢复发布齐备。

至少保留：实际磁盘版本与路径身份、冻结输入与实际成功 bytes 的回执结算、新输入不被旧候选覆盖；TA-02 的重载 Undo/Redo 不回退磁盘基线并正确暂停/恢复自动保存；ED-01 的真实平台自然确认/取消与写前重验；现有文件安全替换与 metadata、竞争/权限/取消/失败；10 秒逐文档恢复、未命名临时图片资源齐备后发布且不早删。M4 的完整图片迁移和 M5 多窗口/路径回归不能替代 M2 最小闭环，M2 最小闭环也不能代替后续完整矩阵。

## 实际/人工/未执行分类

- **实际执行来源：** 9.1 完整门禁及各组原始日志；本包未重复扩量测试。
- **本次人工核查：** 对照 PRD、proposal 出口定义、现行 V 计划、源码测试和 9.1 发现清单建立阶段对应；链接/原始输入读取检查记录在 [commands.log](./commands.log)及本目录 inspection。
- **未执行：** 上表所有真实平台、UI、性能、保存/恢复/迁移和发布实验，以及 9.6 最终独立出口复核。不能依据任务勾选、report 或本包存在宣称 M0 出口/V 项通过。

清理后链接检查首次 exit 1：正文引用的 `handoff-inputs-and-links-after-cleanup.json` 在该次检查开始时尚未生成，属于证据索引自引用的生成顺序问题。检查器随即生成清单，再次检查 60 个本地链接全部存在、exit 0；首轮与最终 stdout/stderr 分别保留在 `inspection-after-cleanup/audit.*.log` 和 `audit-final.*.log`，对应时间/命令/退出码见本目录 commands.log。没有把首次失败改写为通过。
