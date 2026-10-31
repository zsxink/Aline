# Proposal

## Why

Aline 尚无 Cargo workspace 或应用实现，需要先把现行规范转为可无头验证、可独立审查的 M0 引擎地基，再向 M1 交接。此变更只规划架构 §13 的 M0，不以工程骨架、历史参考成绩或 OpenSpec 校验替代产品验收。

## What Changes

- 目标阶段：M0。按兼容取证→骨架/门禁/catalog→公共边界→source→buffer→markdown→edit→无窗口 DocCore→出口复核的九组任务推进。
- 建立八 crate 的最小职责骨架及真实依赖检查；五引擎无 GPUI、无线程、无运行期全局状态。新增依赖必须逐项准入。2026-10-30 用户批准第 1 组隔离审计后的最小例外，现行架构 §3.2 与本 change 同步：仅 app/ui/plugin 的锁定 GPUI 0.3.9 既有 regex 路径及非桌面 WASI p2 绑定可按精确身份/边/target/features 留证，五引擎、自有直接及其它路径仍禁止。
- 2026-10-31 用户批准编辑源码保留原始 CR/LF（分离头部 BOM），存储往返仅允许 CRLF/LF 行尾表示等价，包含边界新形成的 CRLF；补丁、历史和工作版本仍以实际源码精确校验。
- 冻结受检坐标、身份、版本、补丁和错误协议；建立字节保真、块/Gap、来源映射、保护范围、原子补丁、历史及单草稿的无头测试边界。
- 提供展示树/原始范围 dump、AC-01～AC-04 基础证据和 M1 缺口清单。当前 apply 状态以 tasks.md 勾选和实际证据为准，规划表本身不代表实现通过。
- 非目标：M1 窗口/只读渲染/像素命中，M2 真实 IME、编辑/保存/外改/恢复闭环，M3 完整命令/表格/格式化，M4 插件后端/主题/导出，M5 应用壳，性能及三平台完成声明。

### Acceptance Evidence

下表逐项对应 docs/v1-architecture.md §13 M0 行；E 编号只用于本变更追踪，不新增 AC/V 编号。每项目前均为待实现、待取证。

| 出口 | M0 交付与证据 | 任务 |
|---|---|---|
| E1 八 crate、精确锁版/依赖门禁/catalog | 候选准入账本、实际 lockfile/metadata、成员 lint 正反例、双语键/插值检查及三条 Cargo 日志 | 1–3、9 |
| E2 source/buffer/markdown/edit 无头核心及预算入口 | 各 crate 公共 API 测试、无窗口执行、输入/扫描/节点/预演/历史预算注入 | 4–7 |
| E3 dump 展示树与原始范围 | 同一夹具 dump 文字、块/Gap、源范围、映射精确性、保护范围和诊断；独立预期人工复核 | 6、9 |
| E4 AC-01～AC-04 基础及非法输入无 panic | 原字节往返、局部补丁、Unicode 映射/选区、900ms/IME 模拟组、拒绝零副作用 | 4–8 |
| E5 块+间隙覆盖与对齐映射 | 无重叠覆盖源码，块排除尾分隔；原样精确往返、规范锚点稳定、对齐幂等 | 6 |
| E6 V 未测能力明列 | V-01 目标登记、V-06/V-07 基础夹具及分项结果、未测平台和后续阶段清单 | 1、9 |
| E7 TA-01 输入组/提交接口 | 单草稿同块多组提交、离块不重复、快照不封口，实验未完成项单列 | 5、8、9 |

### Traceability

AC 原口径保持 docs/v1-prd.md §8 的定义；以下仅划出基础贡献，不重新定义 AC。

| PRD / AC / V | M0 可取证部分 | 保留后续验收 |
|---|---|---|
| SAFE-01～03；AC-01 | 有效/非法 UTF-8、BOM、混合行尾/平票、孤立 CR、无尾换行的 decode/encode 金样 | 真实打开/保存链路 |
| EDIT-01、SAFE-02/03；AC-02 | 局部补丁与逆补丁、只读模式状态切换零文本/历史变化 | 双模式 UI、保存重开 |
| EDIT-04～06；AC-03 | 中文/emoji/组合字符/实体/转义映射、非法 UTF-16 拒绝、模拟组合确认/取消与选区历史 | 像素命中、候选窗与真实输入法 |
| EDIT-06、08～14；AC-04 | 输入组/命令分隔、代表性补丁命令、拒绝不变性 | 完整格式、列表、表格操作 M3 |
| MD-01～11；AC-09 基础关联 | 解析语义、有限 HTML、字面区避让、脚注引用/目录/Front matter 保护和来源 | 原生显示/导航与资产支持，不宣称 AC-09 完成 |
| SAFE-04～08；TA-02、ED-01 | 快照/重载历史边界及发布等待的纯协议，为 V-08 留身份接口 | M2 保存队列/恢复/外改/临时图片实测；M4 导出 |
| V-01 | macOS/Intel Mac、Windows 10、Linux 兼容目标与设备/环境缺口登记 | M1 macOS 冒烟，之后 Windows/Linux；不标整项通过 |
| V-06 / V-07 | 编码、映射、保护、预算注入和输入组基础夹具；实施后只回填分项实际结果 | 真实限额、平台输入、完整命令/格式化等继续未完成 |
| V-08 | 身份/快照/重载的接口准备，最小原文件保全故障样例 | M2 前身份/保存方案，M2 最小安全闭环；不声称 V-08 通过 |
| V-02～05、V-09 | 保留接口和缺口登记 | 本变更不执行完整实验 |

V-01～V-09 当前均未执行。本次规划只登记 V-01 目标及 V-06/V-07 基础夹具计划，不将其写成实验结果。

## Capabilities

### New Capabilities

- `engine/source`：有效 UTF-8、存储元数据及可逆字节转换。
- `engine/buffer`：受检原子补丁、锚点、版本和历史原语。
- `engine/markdown`：单解析管线、展示树/Gap、来源映射、保护及降级。
- `engine/edit`：纯变更计划、预演、重验和零副作用拒绝。
- `engine/document-core`：由 aline-ui 持有的无窗口单草稿、输入组、当前快照协议；路径表示领域职责，不新增 engine crate。
- `workspace/engineering-gates`：M0 工具取证、依赖/lint/catalog 门禁与出口证据。

按可独立验证的责任边界划分；source 不并入 buffer，映射不下沉 buffer，DocCore 不移入引擎。现有主规格只有 engineering-governance，已提供通用工程约束；本变更新增 M0 专属验收细化，不复制其要求或建立近义治理主规格。

### Modified Capabilities

`engineering-governance`：已按现行架构 §3.2 同步精确依赖例外的工程检查要求，五引擎隔离及其余契约不变；该同步不代表第 2 组门禁已接入。

## Impact

未来实施影响 app、aline-ui/document、aline-plugin 最小边界、aline-edit/change/check/refusal、aline-markdown/pre/build/tree/blocks/protection/budget、aline-buffer/patch/anchors/history、aline-source/decode/encode 及最小受检 I/O、aline-app-core/i18n。只建立实际消费的模块，八 crate 不等于填满架构 §4.3 的目录。

依据：AGENTS 铁律；docs/README.md 权威分工；development-process §1～3；架构 §3.2/4/5/7.1/13；引擎 §5～9/11/12/18.2；Rust §2/5/7/10/12/13/19；CODE_STANDARDS §2～6、engineering-governance；架构 R-1～R-8、R-12/13/15/16、K-01～04/06/08；Rust S-2/5/7/10/11/12/13/15。决定追踪见 docs/decisions.md §5：TA-01(A)、TA-02(B)、ED-01(A)、RS-01～05(A) 均沿用，不重开 UI/PL/TH。

后续文档回填位：架构 §3.2/4/5/7.1/13，引擎 §5～9/11/12/18.2，Rust §2/5/12/19，验证计划 V-01/06/07 的分项证据及 V-08 未测清单；具体任务见 tasks。第 1 组精确依赖裁决另同步 AGENTS、架构规范 R-1-05、Rust §13/15、CODE_STANDARDS 与工程治理主规格，以免 OpenSpec 草案覆盖旧禁令；未授权其它范围变化。

### Risks

GPUI 0.3.9 已证实依赖 regex，并在 all-target 图选入非桌面 WASI p2 绑定，用户批准的精确例外及已执行工具组合以现行架构 §3.2 和审计记录为准。metadata 过近似路径不能当已启用依赖，正式接线仍须逐目标/feature 复核全部路径及身份。原首选 LF 表示的相邻 CR/LF 反例已复现；已批准改用原始行尾表示与限定存储行尾等价，实际编码与基线证据仍须验证；多对一映射和有限 HTML 容易丢来源；待定组与拒绝/快照交错容易改历史；macOS 结果不能外推三平台。分别以前置取证、独立金样、预算/旧投影拒收及证据分项控制。

### Open Questions

待确认问题：无。GPUI 精确例外及工具组合分项已按用户裁决与实际证据登记；其余依赖/平台能力、原始行尾表示的编码/身份算法及预算数值仍按相应任务验证，不是已完成事实。若实际闭包出现未获精确例外的新冲突、源码表示无法满足正常输入保真，或实验迫使缩减产品结果，记录精确冲突及最小裁决路径，暂停依赖项；不得未经裁决改变禁令、拒绝正常输入或把执行顺序参考升格为产品规范。
