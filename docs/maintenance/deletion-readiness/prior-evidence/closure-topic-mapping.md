# 既有审计原件隔离归档：topic-mapping.md

此文件保存此前审计交付快照，**不是本轮独立复核的证明、当前产品规范、V 实验结果或删除授权**。原文中的“通过/无问题/可删除评估/未确认”和运行命令仅记录当时表述，现状以本轮报告及现行规范为准。原相对链接在围栏内作为历史字符保留；可独立使用的目标见[本证据集入口](./README.md)。

原路径：`reports/2026-10-30-migration-closure/topic-mapping.md`；原字节数：99169；原 SHA-256：`b685ca835fe4c290eb3973f8cfd4946fe8d71828f194e868acd795fc6974f38e`。以下围栏正文按 UTF-8 原字节保留，正文末尾的换行属于原件。

~~~~~~~~markdown
# 确认后的331主题归属与证据覆盖

2026-10-30。现行语义去向沿用初审逐章核对；本表补充PROTO-06 A已完成的独立证据保全，不再将“压缩/移出”当无去向。行范围是旧原稿，引用中的旧数值/API/过时规则按现行确认取代；未重复保留的规范性代码草图/旧JSON/估时/OS表不构成当前实施要求。

| 旧路径与原行范围 | 主题 | 初审对应现行契约 | 现有独立原证据 | 本轮语义与删除准备 |
| --- | --- | --- | --- | --- |
| docs_bak/docs/architecture-guide.md:1～14 | Aline 架构规范（architecture-guide） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:15～41 | 目录 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:42～58 | 0. 规范的定位与文档一致性 | §0/17/18 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:59～60 | 1. 分层与依赖（依赖的方向即架构） | §1/3/15 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:61～75 | 1.1 crate 分层 | §1/3/15 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:76～80 | 1.2 跨层调用规则【强制】 | §1/3/15 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:81～88 | 1.3 依赖准入【强制】 | §1/3/15 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:89～93 | 1.4 拆分与隔离【强制】 | §1/3/15 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:94～102 | 2. 公共 API 与模块边界【强制】 | §2/9 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:103～104 | 3. 数据流与真源【强制】 | §3/5/6 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:105～115 | 3.1 单一写路径 | §3/5/6 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:116～120 | 3.2 每帧路径【强制】 | §3/5/6 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:121～126 | 3.3 过期判定与异步出口【强制】 | §3/5/6 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:127～130 | 3.4 阈值常量单一来源【强制】 | §3/5/6 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:131～134 | 3.5 事件单向与回调纪律【强制】 | §3/5/6 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:135～141 | 4. 坐标系与映射边界【强制】 | §4；引擎§7 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:142～155 | 5. 编辑命令与事务【强制】 | §5/8/15 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:156～162 | 6. 并发与调度【强制】 | §6 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:163～168 | 7. 缓存与失效【强制】 | §7 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:169～201 | 8. GPUI 使用规则【强制】 | §8～10；UI§10～15 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:202～203 | 9. 组件形态【强制】 | §9；Rust§9；引擎§10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:204～211 | 9.1 三形态选择表（先选形态再写代码） | §9；Rust§9；引擎§10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:212～218 | 9.2 构建器纪律 | §9；Rust§9；引擎§10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:219～224 | 9.3 文本输入三层分离（对 Aline 最关键的模式，Ely 已验证） | §9；Rust§9；引擎§10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:225～228 | 9.4 能力 trait（第二个可设尺寸/可禁用的控件出现时落地） | §9；Rust§9；引擎§10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:229～235 | 10. 渲染与虚拟化边界【强制】 | §10；V-05/09 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:236～246 | 11. 性能与反面实证 | §11；V-05/09 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:247～258 | 反面实证：MarkFlow 性能诊断（2026-10-25，issue #458/#459/#460） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:259～278 | 12. Markdown 引擎规则【强制】 | §4/11/12；引擎§6～7/11 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:279～287 | 13. 主题 / 设置 / i18n 传播【强制】 | §13；主题规范§5～10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:288～298 | 14. 插件边界 | §14；插件§2～9；PA-01 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:299～305 | 15. 持久化与文件 | §15；PRD SAFE/IMAGE/FILE | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:306～313 | 16. 错误、断言与日志 | §16；Rust§7/10 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:314～325 | 17. 测试架构【强制】 | §17；引擎§14；Rust§12 | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:326～341 | 18. 演进与决策记录 | §17/18；decisions | docs/archive/migration-evidence/architecture-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:342～366 | 附录 A：Ely 证据索引 | §18；完整索引仍旧稿 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:367～375 | 附录 B：门禁分阶段清单 | §18；完整索引仍旧稿 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/architecture-guide.md:376～382 | 附录 C：三项目对照速查 | §18；完整索引仍旧稿 | docs/archive/migration-evidence/architecture-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/decisions.md:1～8 | Aline 决策记录 | decisions §3～9；archive 确认索引 | docs/archive/migration-evidence/decisions.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/decisions.md:9～19 | 2026-10-28 · UI 参照范围与浅色主题 | decisions §3～9；archive 确认索引 | docs/archive/migration-evidence/decisions.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/decisions.md:20～21 | 决策时间线 | decisions §3～9；archive 确认索引 | docs/archive/migration-evidence/decisions.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/decisions.md:22～38 | 2026-10-26 · 技术方案复核裁决（全面复核四份文档后批量修订） | decisions §3～9；archive 确认索引 | docs/archive/migration-evidence/decisions.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/decisions.md:39～46 | 2026-10-26 · HTML 支持进 V1（ElyMark 逆向后裁决） | decisions §3～9；archive 确认索引 | docs/archive/migration-evidence/decisions.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/decisions.md:47～54 | 常设决定（不设截止日期，翻案需新裁决） | decisions §3～9；archive 确认索引 | docs/archive/migration-evidence/decisions.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1～10 | Aline 底层引擎技术方案 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:11～37 | 第 0 部分 TL;DR | §2～4/12/15 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:38～39 | 第 1 部分 ElyMark 深度解析 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:40～73 | 1.1 代码在哪里，分层是什么 | §18.3；架构§3.2 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:74～75 | 1.2 Q1：如何渲染"可视化编辑" | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:76～86 | 1.2.1 四种模式 | §2；PRD EDIT-01/§8 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:87～112 | 1.2.2 核心数据流 | §3～6/8 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:113～142 | 1.2.3 关键结构体 | §4/7/8 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:143～166 | 1.2.4 编辑交互的行为面 | §8～9；PRD EDIT-02/06 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:167～168 | 1.3 Q2：支持哪些 markdown | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:169～200 | 1.3.1 支持矩阵 | §11；PRD MD-01～11 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:201～210 | 1.3.2 特殊块的渲染细节（全部异步 + 资产缓存） | §11.3/12；插件§5～8 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:211～212 | 1.4 Q3：源码是怎么做的（解析管线与映射） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:213～232 | 1.4.1 三层解析管线 | §6 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:233～252 | 1.4.2 映射机制（整个方案最精妙的部分） | §7 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:253～258 | 1.4.3 渲染与增量 | §12；PRD§7.2 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:259～272 | 1.5 Q4：如何做到源码保真 | §2/4/5/7/9 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:273～279 | 1.6 输入法（IME）——中文输入是硬要求 | §10；UI§6/13 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:280～285 | 1.7 GPUI 层面的事实 | §18.3；架构§12 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:286～299 | 1.8 ElyMark 的局限（Aline 的改进空间） | §2/12/16 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:300～306 | 1.9 调研中发现的其他事实 | §18.3；决策§5 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:307～344 | 1.10 HTML 渲染机制（专项逆向） | §7/11.2/14；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:345～387 | 1.10.1 行内 HTML 标签全集 | §11.2；UI§7 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:388～495 | 1.10.2 HTML5 嵌套修复算法 | §11.2；V-07 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:496～574 | 1.10.3 HTML 实体解码机制 | §7/11.2；V-07 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:575～693 | 1.10.4 块级 HTML 渲染管线（**ElyMark 最反直觉的部分**） | §11.2/13；V-06/07 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:694～758 | 1.10.5 空白与换行的"欠账"机制（`owed`） | §7/11.2 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:759～773 | 1.10.6 双向映射的关键：`LineMap` | §7/12/14 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:774～879 | 1.10.7 点击定位与编辑提交链路 | §8～9 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:880～934 | 1.10.8 命令拒绝制规则 | §9/11.2；MD-10 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:935～936 | 第 2 部分 Aline 引擎设计 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:937～953 | 2.0 目标与非目标 | §2；PRD§3/8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:954～976 | 2.1 总体架构 | §3；架构§4 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:977～1090 | 2.2 核心数据模型 | §4/7/8/9 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1091～1092 | 2.3 解析管线 | §6/12 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1093～1106 | 2.3.1 预处理改写器（三个，各带字节映射） | §6/12 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1107～1116 | 2.3.2 pulldown 配置 | §6/12 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1117～1128 | 2.3.3 解析调度 | §6/12 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1129～1141 | 2.4 可视化编辑：四档阶梯（A → B → B+ → C） | §2/7/10；PRD EDIT-03/§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1142～1148 | 阶段 A：开合式编辑（= ElyMark 现状，先跑通全链路） | §2/7/10；PRD EDIT-03/§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1149～1152 | 阶段 B：标记渐隐（纯样式，零映射成本） | §2/7/10；PRD EDIT-03/§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1153～1161 | 阶段 B+（M5 目标）：折叠编辑器 + 光标附近标记显形 | §2/7/10；PRD EDIT-03/§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1162～1177 | 阶段 C：全标记隐藏（Typora 手感） | §2/7/10；PRD EDIT-03/§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1178～1203 | `FoldedInlineEditor` 设计（核心自研组件，阶段 B+/C 共用） | §2/7/10；PRD EDIT-03/§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1204～1240 | 2.5 命令框架（aline-edit） | §9；PRD FORMAT-01/02 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1241～1257 | 2.6 渲染层（aline-ui） | §3/11.3/12；架构§8 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1258～1270 | 2.7 特殊块实现要点 | §11；PRD MD/IMAGE/EDIT | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1271～1279 | 2.8 IME（中文输入，第一优先级） | §10/17；ED-01 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1280～1302 | 2.9 文件层（aline-source，照搬 Ely 语义 + 补强） | §5/8～10；架构§7.3/7.4 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1303～1314 | 2.10 性能策略 | §12；V-05/06 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1315～1327 | 2.11 测试策略（发布门禁） | §14 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1328～1352 | 2.12 依赖清单 | §18.2；架构§3.2 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1353～1366 | 2.13 里程碑 | §15；架构§13；PRD§9 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1367～1381 | 2.14 风险与对策 | §6～13/16 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1382～1402 | 2.15 与 ElyMark 的复用对照 | §18.3；决策§3～5 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1403～1406 | 2.16 跨平台实现注意事项（Windows / Linux / macOS） | 架构§7/12；V-01/08 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1407～1416 | 目标系统版本基线（截至 2026-10-26，发布前复核） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1417～1426 | 文件层（aline-source） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1427～1438 | GPUI 层（aline-ui / app） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1439～1443 | 流程要求 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1444～1447 | 2.17 HTML 实施方案（对齐 §1.10 机制） | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1448～1472 | 2.17.1 架构分层与落点 | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1473～1492 | 2.17.2 增量改动清单 | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1493～1503 | 2.17.3 里程碑划分（对齐 §2.13 / v1-architecture M0–M6） | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1504～1512 | 2.17.4 性能预算 | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1513～1525 | 2.17.5 已知限制 | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1526～1533 | 2.17.6 技术债务预警 | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1534～1545 | 2.17.7 测试策略（并入 §2.11 门禁，对齐 v1-architecture §12 功能组 5） | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1546～1558 | 2.17.8 决策记录 | §3/7/11/14/15；PRD MD-07～10 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1559～1586 | 2.17.9 后续迭代路线图（不承诺时点，V1 之后逐项决策） | PRD§8；决策D13 | docs/archive/migration-evidence/engine-design.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/engine-design.md:1587～1630 | 附录 ElyMark 关键文件索引（实现期对照用） | §18.3；仍在旧稿附录 | docs/archive/migration-evidence/engine-design.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:1～4 | Aline 插件系统设计（V1） | 插件 §1～2 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:5～16 | 1. 目标与范围 | 插件 §2～4 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:17～24 | 2. 插件管理与设置 | 插件 §4/6/9 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:25～26 | 3. 内置插件 | 插件 §1～2 | docs/archive/migration-evidence/plugin-system.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:27～45 | 3.1 Mermaid | 插件 §6.1/7；PL-01 | docs/archive/migration-evidence/plugin-system.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:46～65 | 3.2 表格增强 | 插件 §6.2；EDIT-12～14 | docs/archive/migration-evidence/plugin-system.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:66～77 | 3.3 导出 | 插件 §6.3/8；V-02 | docs/archive/migration-evidence/plugin-system.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:78～86 | 3.4 公式 | 插件 §6.4/7 | docs/archive/migration-evidence/plugin-system.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:87～97 | 3.5 代码高亮 | 插件 §6.5/7；PL-02；主题规范 | docs/archive/migration-evidence/plugin-system.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:98～111 | 4. 插件能力与扩展点 | 插件 §3/7/9；架构 §8.1；PA-01 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:112～119 | 4.1 解析扩展接口归属与生命周期 | 插件 §3/7；引擎 §6；R-12 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:120～130 | 4.2 文档修改提案（未来 AI 插件） | PRD §8；decisions D-2026-10-28-21/PA-01 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:131～141 | 5. 一期验收 | 插件 §10；P-T01～12；AC-11/14/19 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/plugin-system.md:142～150 | 6. 与既有 V1 文档的关系 | 插件 §14；PRD §5.8 | docs/archive/migration-evidence/plugin-system.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:1～15 | Aline Rust 代码规范 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:16～41 | 目录 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:42～49 | 1. 基本原则 | §1/2/5/7/9/12 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:50～68 | 2. 工具链【强制】 | §2/19；M0 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:69～70 | 3. 工程结构与模块组织【强制】 | §3；架构§4 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:71～85 | 3.1 crate 划分与依赖方向 | §3；架构§4 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:86～92 | 3.2 模块组织 | §3；架构§4 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:93～97 | 3.3 可见性 | §3；架构§4 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:98～116 | 4. 命名【强制】 | §4 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:117～144 | 5. 坐标系与类型设计【强制】 | §5；引擎§7；RS-02 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:145～150 | 6. 格式【强制】 | §2/6 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:151～152 | 7. 错误处理【强制】 | §7/16；RS-03 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:153～159 | 7.1 分层策略 | §7/16；RS-03 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:160～166 | 7.2 红线 | §7/16；RS-03 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:167～175 | 7.3 拒绝制不是错误 | §7/16；RS-03 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:176～183 | 8. 所有权、借用与分配 | §8/14；架构R-3 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:184～195 | 9. Option / Result / match | §6/7 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:196～203 | 10. 泛型与 trait | §9 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:204～208 | 11. unsafe【强制】 | §10/16；RS-04 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:209～216 | 12. 文档与注释 | §11 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:217～229 | 13. 测试【强制】 | §12；引擎§14 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:230～231 | 14. 并发与 GPUI【强制】 | §13；架构§6 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:232～242 | 14.1 线程模型 | §13；架构§6 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:243～248 | 14.2 锁与共享 | §13；架构§6 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:249～253 | 14.3 运行时 | §13；架构§6 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:254～262 | 15. 性能 | §5/14/19；V-05/06 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:263～264 | 16. 依赖与提交 | §15；RS-05 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:265～270 | 16.1 依赖 | §15；RS-05 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:271～283 | 16.2 提交 | §15；RS-05 | docs/archive/migration-evidence/rust-style-guide.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:284～299 | 17. 参考基准 | §21 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:300～303 | 18. 参考项目实证条款 | §5/7/9/12～14/21 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:304～310 | 18.1 来自 Ely（正向继承） | §5/7/9/12～14/21 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:311～323 | 18.2 来自 Zed（源码实证） | §5/7/9/12～14/21 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:324～335 | 附录 A：rustfmt.toml | §2/16/17/19 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:336～378 | 附录 B：workspace lints | §2/16/17/19 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/rust-style-guide.md:379～401 | 附录 C：CI | §2/16/17/19 | docs/archive/migration-evidence/rust-style-guide.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:1～4 | Aline 主题包规范 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:5～19 | 配置格式裁决 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:20～39 | 主题包目录 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:40～41 | 可配置项目 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:42～52 | 1. Markdown 与编辑区样式（`markdown.json`） | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:53～59 | 2. 代码高亮语义配色（`highlight.json`） | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:60～66 | 3. 应用界面样式（`application.json`） | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:67～94 | 字段清单 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:95～103 | 3. 不开放给主题包的项目 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:104～115 | 字段继承与合并 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:116～226 | JSON 结构示例 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:227～239 | ZIP 校验与导入 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:240～247 | 主题文件夹与同步 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/theme-package-spec.md:248～252 | 版本与原型边界 | 主题规范 §3～12/15；TH-01～10；TH-T01～23 | docs/archive/migration-evidence/theme-package-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:1～4 | Aline UI 与交互规范 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:5～12 | 1. 参照来源与优先级 | UI §2 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:13～19 | 2. 浅色主题 | UI §3.1 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:20～26 | 3. 图标与操作细节 | UI §3/12/13 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:27～40 | 4. 代码块呈现与操作 | UI §7.2 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:41～44 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:45～51 | 5. Markdown V1 展示夹具 | UI §7/15；prototype/review.md | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:52～76 | 6. 验收清单 | UI §15 | docs/archive/migration-evidence/ui-spec.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:77～87 | 7. 多文档标签栏 | UI §5.1/12.1 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:88～91 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:92～100 | 8. 文件浏览器 | UI §5.2 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:101～104 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:105～110 | 9. 源码模式滚动 | UI §6 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:111～114 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:115～123 | 10. 系统菜单栏与窗口栏 | UI §10/10.5 | docs/archive/migration-evidence/ui-spec.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:124～127 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:128～138 | 11. 右键菜单 | UI §10/13 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:139～142 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:143～162 | 12. 外观设置与主题 | UI §11；主题规范 | docs/archive/migration-evidence/ui-spec.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:163～168 | 设计原因 | UI §2/19 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/ui-spec.md:169～181 | 13. 插件设置 | UI §11.3；插件规范 | docs/archive/migration-evidence/ui-spec.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:1～10 | Aline V1 技术架构与实现细节 | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:11～12 | 第 1 章 总体架构与 crate 划分 | §3～5 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:13～19 | 1.1 分层原则 | §3～5 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:20～41 | 1.2 crate 划分与依赖方向（严格单向） | §3～5 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:42～75 | 1.3 仓库骨架 | §3～5 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:76～81 | 1.4 依赖清单（锁定） | §3～5 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:82～83 | 第 2 章 应用壳层：窗口 / 标签页 / 文件夹浏览器 / 系统打开（功能组 1） | §5/7.5；PRD FILE-01～08 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:84～116 | 2.1 核心结构 | §5/7.5；PRD FILE-01～08 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:117～140 | 2.2 系统文件关联与多窗口（Finder 双击 → 新窗口） | §5/7.5；PRD FILE-01～08 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:141～146 | 2.3 每窗口状态持久化 | §5/7.5；PRD FILE-01～08 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:147～169 | 第 3 章 文件层实现（功能组 1） | §7.3/7.4；引擎§5/8～10 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:170～171 | 第 4 章 编辑模式（功能组 2） | §5；PRD EDIT/MD-11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:172～181 | 4.1 两模式，共享真源 | §5；PRD EDIT/MD-11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:182～187 | 4.2 front matter 的最小处理（功能上"不做"，但要防渲染错乱） | §5；PRD EDIT/MD-11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:188～189 | 第 5 章 查找与替换（功能组 3） | §7.2；PRD FIND | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:190～212 | 5.1 状态与算法（aline-edit/src/find.rs，零 GPUI） | §7.2；PRD FIND | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:213～220 | 5.2 两模式的 UI 行为 | §7.2；PRD FIND | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:221～222 | 第 6 章 可视化编辑引擎（功能组 4） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:223～252 | 6.1 DocCore：编排核心（普通 struct，可无头测试） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:253～284 | 6.2 打开-提交循环（阶段 A/B 的主循环，全部照搬 elymark 语义） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:285～330 | 6.3 命令框架（apart 协议 + 拒绝制 + reread） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:331～336 | 6.4 IME（硬要求，照搬 elymark 对接） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:337～343 | 6.5 折叠编辑器阶梯（B+ 为挑战项，可整体后移） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:344～349 | 6.6 表格 | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:350～377 | 6.7 HTML（V1，D-2026-10-26-13） | §5/7；引擎§4/6～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:378～379 | 第 7 章 渲染层与资产管线（功能组 5） | §8/9；引擎§12 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:380～386 | 7.1 块渲染与虚拟化 | §8/9；引擎§12 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:387～408 | 7.2 资产管线（全部异步，统一模式） | §8/9；引擎§12 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:409～410 | 第 8 章 主题系统（功能组 7） | §8.2；主题规范§3～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:411～459 | 8.1 Token 模型（aline-app-core/src/theme，零 GPUI） | §8.2；主题规范§3～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:460～465 | 8.2 加载与运行时（gpui Global 机制） | §8.2；主题规范§3～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:466～479 | 8.3 三个接线点（实现的关键） | §8.2；主题规范§3～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:480～489 | 8.4 切换端到端时序 | §8.2；主题规范§3～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:490～495 | 8.5 外部主题包 | §8.2；主题规范§3～11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:496～499 | 第 9 章 插件系统（功能组 6） | §8.1；插件规范；PA-01 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:500～554 | 9.1 定位与边界 | §8.1；插件规范；PA-01 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:555～569 | 9.2 五个内置插件的职责切分 | §8.1；插件规范；PA-01 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:570～574 | 9.3 加载模型与外置化路线（参考 Zed 源码） | §3.2/8.1/15；PA-01 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:575～586 | 9.3.1 最重要的分层借鉴：声明式资源不进沙箱 | §3.2/8.1/15；PA-01 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:587～595 | 9.3.2 宿主机制（照抄 Zed，wasmtime 48 + component-model） | §3.2/8.1/15；PA-01 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:596～616 | 9.3.3 接口、版本与 guest 形态 | §3.2/8.1/15；PA-01 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:617～623 | 9.3.4 清单、目录与 dev 工作流（照抄结构） | §3.2/8.1/15；PA-01 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:624～627 | 9.3.5 工作量预估 | §3.2/8.1/15；PA-01 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:628～645 | Zed 关键文件索引（扩展宿主） | 导言/现行状态与逐章处置账本 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:646～647 | 第 10 章 多语言（功能组 8） | §8.3；架构规范R-13；Rust§11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:648～669 | 10.1 机制（aline-app-core/src/i18n，零 GPUI） | §8.3；架构规范R-13；Rust§11 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:670～686 | 第 11 章 性能实现（功能组 9） | §9/10；PRD§7.2；V-05/06 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:687～701 | 第 12 章 测试与验收门禁（映射功能组） | §6/10；引擎§14 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:702～717 | 第 13 章 里程碑（V1 重排，取代 engine-design §2.13） | §13；PRD§9 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:718～733 | 第 14 章 风险与对策（V1 口径更新） | §12；V-01～09 | docs/archive/migration-evidence/v1-architecture.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:734～737 | 第 15 章 Ely 实证的 GPUI 实现要点 | §5/7/12；UI/Rust专项 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:738～743 | 15.1 窗口骨架与启动 | §5/7/12；UI/Rust专项 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:744～748 | 15.2 焦点、undo 与 dirty | §5/7/12；UI/Rust专项 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:749～753 | 15.3 Command 表落地 | §5/7/12；UI/Rust专项 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:754～759 | 15.4 测试夹具与冒烟 | §5/7/12；UI/Rust专项 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:760～766 | 15.5 失焦与布局 | §5/7/12；UI/Rust专项 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:767～770 | 附录 A 解析内核要点索引 | §4/5/12；引擎§18.3 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-architecture.md:771～787 | 附录 B 关键类型速查 | §4/5/12；引擎§18.3 | docs/archive/migration-evidence/v1-architecture.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:1～10 | Aline V1 功能清单 | PRD §3/8/9；decisions §6 | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:11～26 | 1. 文件、标签页与工作区 | PRD §5.1/5.2/5.7/5.12；FILE/SAFE/IMAGE | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:27～32 | 2. 编辑模式（共享同一文本真源与撤销历史） | PRD §5.3；EDIT-01～07 | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:33～38 | 3. 查找与替换 | PRD §5.5；FIND-01～03 | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:39～52 | 4. 可视化编辑（阶段 A + B） | PRD §5.3/5.4；EDIT-02～14 | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:53～67 | 5. 行内与块级元素 | PRD §5.6/5.7；MD/IMAGE | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:68～80 | 6. 插件系统（V1 = 编译期内置插件，接口预留动态加载） | PRD §5.8/5.9；PLUGIN/EXPORT；PA/PL | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:81～89 | 7. 主题系统 | PRD §5.10；THEME；TH | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:90～97 | 8. 多语言（内置子系统，非插件） | PRD §5.11；APP；AGENTS铁律7 | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/v1-features.md:98～103 | 9. 性能与容量 | PRD §7.2；V-05/06 | docs/archive/migration-evidence/v1-features.md（必要原例/来源片段已保全；本段规则按现行改写/取代） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:1～8 | WordCraft 调研：Markdown 解析、性能与保真 —— 及 Aline 改进方案 | WordCraft §1～6 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:9～14 | 0. TL;DR | WordCraft §1～3 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:15～16 | 1. WordCraft 是什么，与 Aline 的可比性 | WordCraft §2～3 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:17～22 | 1.1 项目概况 | WordCraft §2～3 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:23～36 | 1.2 路线差异：先说清楚什么不能学 | WordCraft §2～3 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:37～38 | 2. Markdown 解析实现分析 | WordCraft §1～6 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:39～44 | 2.1 总体路线 | WordCraft §4.1/E02 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:45～55 | 2.2 块级解析：不认识的一律保留 | WordCraft §4.1/W13 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:56～69 | 2.3 行内解析：CommonMark 强调算法 + 三件反退化工程 | WordCraft §4.1/W1/14/16 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:70～80 | 2.4 导出：以"读回恒等"为准绳的转义纪律 | WordCraft §4.2/W12 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:81～86 | 2.5 丢失显式化：Dropped 账本 | WordCraft §4.3/W6/WC-01 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:87～99 | 2.6 对照结论 | WordCraft §3/4.3/5 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:100～101 | 3. 性能架构分析 | WordCraft §1～6 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:102～107 | 3.1 数据面：Arc 写时复制 + 快照 undo + panic 恢复 | WordCraft §4.4/4.7/W8/17 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:108～113 | 3.2 渲染面：增量重排与页级光栅缓存 | WordCraft §4.5/W2/5 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:114～119 | 3.3 I/O 面：原子保存、后台恢复、非阻塞读取 | WordCraft §4.6/W3/4/11 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:120～132 | 3.4 性能文化：案例库 | WordCraft §4.7 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:133～138 | 3.5 对 Aline 的映射 | WordCraft §4.6/5 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:139～140 | 4. 保真与健壮性分析 | WordCraft §3/4.8/W9～16 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:141～149 | 4.1 保真方法论：量化、透传、棘轮 | WordCraft §3/4.8/W9～16 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:150～158 | 4.2 敌意输入防御：预算全家桶 | WordCraft §3/4.8/W9～16 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:159～163 | 4.3 数据安全 | WordCraft §3/4.8/W9～16 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:164～169 | 4.4 对 Aline 的映射 | WordCraft §3/4.8/W9～16 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:170～173 | 5. 方案：让 Aline 更快、更强、保真、更安全、更好用 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:174～204 | 5.1 更快 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:205～230 | 5.2 更强 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:231～254 | 5.3 更保真 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:255～280 | 5.4 更安全 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:281～303 | 5.5 更好用 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:304～314 | 5.6 明确不采纳（防止过度工程） | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:315～343 | 5.7 采纳路径与里程碑归位 | WordCraft §5～6/8 | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/wordcraft-study.md:344～367 | 附录 A：WordCraft 证据索引 | WordCraft §2 E01～09；完整原索引仍旧稿附录A | docs/archive/migration-evidence/wordcraft-study.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:1～6 | Zed 架构与 GPUI 实践笔记 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:7～12 | 0. 版本警告（先读） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:13～14 | 第 1 章 gpui 本体（crates/gpui） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:15～20 | 1.1 Entity / 效果系统 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:21～27 | 1.2 Element 生命周期 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:28～32 | 1.3 虚拟化列表 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:33～39 | 1.4 文本系统 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:40～46 | 1.5 InputHandler 与 IME | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:47～52 | 1.6 actions 与 keybinding | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:53～59 | 1.7 Asset / 异步 / 窗口 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:60～77 | 1.8 gpui 层借鉴清单 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:78～79 | 第 2 章 Workspace 壳层与 UI 组件（crates/workspace、crates/ui） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:80～87 | 2.1 Item trait——"能开成 tab 的东西" | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:88～94 | 2.2 Pane 与标签页 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:95～99 | 2.3 Panel/Dock 与文件树 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:100～105 | 2.4 弹窗 / 通知 / 会话恢复 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:106～111 | 2.5 窗口与组件风格 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:112～126 | 2.6 壳层借鉴清单 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:127～128 | 第 3 章 编辑器内核与查找替换（crates/text、editor、search） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:129～133 | 3.1 事务与 undo（对照 Aline 的逆补丁） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:134～138 | 3.2 高亮与 decorations | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:139～144 | 3.3 IME 落地细节（editor/src/input.rs:2798-3120） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:145～151 | 3.4 查找替换（crates/search/buffer_search.rs） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:152～157 | 3.5 增量失效与高亮缓存 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:158～165 | 3.6 内核借鉴清单 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:166～167 | 第 4 章 主题、设置与文件系统（crates/theme、settings、fs） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:168～175 | 4.1 主题 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:176～181 | 4.2 设置 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:182～191 | 4.3 文件系统 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:192～210 | 第 5 章 对 Aline 的全景影响（映射到 v1-architecture） | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
| docs_bak/docs/zed-notes.md:211～240 | 第 6 章 Zed 关键文件索引 | docs/zed-notes.md §0～5逐主题复核（完整原研究独立保全） | docs/archive/migration-evidence/zed-notes.md（此原文区间完整保全） | PROTO-06 A已实施；无旧目录读取依赖；不放宽V/许可/删除授权 |
~~~~~~~~
