# Spec Delta

## ADDED Requirements

### Requirement: 验证方法采纳必须标明来源和效力边界

QMD-01 的方法采纳 MUST 逐项标明来源，区分 quiet-md 已有机制、Aline 既有规则与 Aline 追加/补强；没有直接证据的 quiet-md 行为 MUST 标为“未确认”或“仅方向参考”，MUST NOT 写成已观察事实或 Aline 已实现能力。研究文档 MUST 只作证据索引，与现行 `docs/` 冲突时以现行规范为准。方法采纳 MUST NOT 改变产品契约、依赖图或阶段范围；改变依赖、夹具、验收方式或跨模块契约时 MUST 先建对应 OpenSpec change。关联 QMD-01、开发流程 §2/§3.1、CODE_STANDARDS §6。

#### Scenario: 引用 quiet-md 机制

- **WHEN** 规范或 change 声称采纳 quiet-md 的某项做法
- **THEN** 同时给出 quiet-md 的观察位置/证据等级与 Aline 的适配差异；若该做法是 Aline 既有或追加，明确标注，不把两者合并成“采纳 quiet-md”

#### Scenario: 方法需要新增依赖或改变验收

- **WHEN** 落实某项方法需要新增/升级依赖、增加夹具、改变 V 项报告字段或修改跨模块契约
- **THEN** 先建立独立 OpenSpec change 并完成评审，再写依赖该变化的代码；不得以 QMD-01 或本研究文档直接授权

#### Scenario: 研究结论与现行规范冲突

- **WHEN** 研究文档的推论、建议或数值与现行 `docs/` 不一致
- **THEN** 暂停受影响实现并列出冲突，由现行规范或用户裁决；不按目录、日期或“参考实现如此”自行选择

### Requirement: 随机不变量与性能证据必须可复现和可追溯

引擎随机测试 MUST 使用确定性 RNG/固定 seed，生成器 MUST 按 Aline 语法只产出合法或预期拒绝输入；每步 MUST 检查该层不变量，并周期性执行保存往返或与全量权威路径差分。失败 MUST 保留 `seed、步号、操作日志、最小输入`；最小输入整理为正式夹具后提交，原始日志只留 `logs/validation/`。已知语义丢失 MUST NOT 用测试辅助函数规范化后当作通过。默认 seed/步数 MUST 在 W3 立项时按 CI 时间确定，扩量运行只作补充证据。性能结论 MUST 记录环境与版本、源码 commit/锁定构建、样本 SHA-256、方法/操作序列、冷热缓存、计时起止、P50/P95、最长停顿、峰值内存、原始日志入口、复现命令及明确不测量项；数值门槛 MUST 由 V-05 实测与 PRD 目标冻结，MUST NOT 由本规格或 quiet-md 数值给出。关联 R-11-01、R-17-03、S-12-06、S-14-03、E-T20、V-05/V-07/V-08。

#### Scenario: 随机测试失败

- **WHEN** fuzz 或属性测试发现失败
- **THEN** 记录 seed、步号、操作日志与最小输入，提供可复跑命令；把最小输入和独立预期整理为回归夹具，不删除操作日志或只保留失败断言

#### Scenario: 性能报告缺少追溯信息

- **WHEN** V-05 报告只有汇总数字，缺少设备/版本、样本指纹、计时边界、冷热、原始输出或复现命令
- **THEN** 判定报告不完整，不用该数字冻结门槛或宣称性能通过；补测或明确标为未完成

#### Scenario: 用 fuzz 或微基准替代验收

- **WHEN** 只有随机测试或局部微基准数据，缺少金样、真实 IME、文件故障或端到端呈现证据
- **THEN** 只记为补充证据，不替代对应 V 项或 AC；明确未覆盖平台与失败路径

### Requirement: 保存与恢复必须分离且故障安全

自动保存（1 秒防抖）MUST 只写用户文档；10 秒恢复快照 MUST 写应用管理的独立目标，二者共用版本/资源协议但 MUST NOT 互相代替。写入 MUST 至少核对：同目录临时文件 → 写完 → 同步 → 平台受检替换；写前 MUST 核对磁盘基线，失败 MUST 保留原文件、内存与恢复材料。恢复 MUST 先进入内存、不覆盖磁盘；恢复快照 MUST NOT 以文档旁 `.bak`/`.corrupt-*` 副本作为恢复手段。原子替换与目录同步的保证范围 MUST 按平台实测说明，MUST NOT 称为跨进程 compare-and-swap。关联 R-15-13、SAFE-04/05/07/08、V-08。

#### Scenario: 自动保存与恢复快照被合并

- **WHEN** 实现把恢复快照写入用户文档、用文档旁 `.bak` 代替恢复目标，或用自动保存状态宣称恢复已受保护
- **THEN** 判定违反分离要求；恢复写入失败单列诊断，旧记录不得看起来仍保护最新内容

#### Scenario: 平台替换能力未验证

- **WHEN** 目标平台的文件系统、权限、占用或断电语义尚未实测
- **THEN** 只声明已观察到的保证范围，保留失败/未测项；不把 `rename`、`sync_all` 或一次 stat 泛化为所有文件系统的原子承诺

#### Scenario: 失败后丢失原文件或内存状态

- **WHEN** 写盘、替换或恢复写入失败
- **THEN** 保留原文件、当前内存内容与恢复材料，提供重试/另存为/恢复路径；不先删原文件、不原地截断、不在失败后静默改为非原子写

### Requirement: IME 集成必须有显式清理和受检坐标

UI MUST 在实际失焦、点击其他块、切模式或窗口失活时显式结束或清理 marked 与组合状态，MUST NOT 只靠 Blur 事件推断平台已确认或取消。等待平台组合完成结果期间 MUST 保留原上下文，MUST NOT 抢焦点、丢草稿或立即取消组合。候选窗位置 MUST 按元素 frame、文本布局、滚动偏移与窗口坐标逐层换算，并在换行、DPI、多屏变化后重新定位。平台 glyph 索引、UTF-16 端点与字素边界 MUST 只在其声明投影内使用，MUST NOT 互相冒充；跨投影 MUST 经统一映射层。真实中文输入法行为 MUST 由 V-01/V-07 取证，MUST NOT 由纯状态机或模拟替代。关联 ED-01、TA-01、E-T15、UI §6/§13、V-01/V-07。

#### Scenario: Blur 到达但平台尚未完成

- **WHEN** 焦点变化或窗口失活，而平台组合确认/取消结果尚未返回
- **THEN** 等待平台结果并保留当前输入与草稿；不立即 unmark/cancel，不显示保存/导出成功，不生成基于半组合内容的产物

#### Scenario: 坐标投影混用

- **WHEN** 实现把 glyph 索引当字素索引、把 UTF-16 端点当 UTF-8 字节偏移，或跨投影手算偏移
- **THEN** 判定违反坐标边界；改用声明投影与统一映射，并补最小失败样例覆盖 emoji ZWJ、组合符、CJK 标点、CRLF

#### Scenario: 只用模拟状态机证明 IME

- **WHEN** 没有真实中文输入法的候选窗、marked 绘制、焦点与平台 callback 证据
- **THEN** 只记为无头/模拟结果，不宣称 V-01/V-07 或相关 AC 通过

### Requirement: UI 语义 token、中文排版与离屏证据必须按边界使用

UI 组件 MUST 只消费语义 token，原始色值 MUST 集中在 token 模块并注明来源；token 重构 MUST NOT 改变已确认视觉与主题包 schema。中文排版 MUST 先测量后决定：全角标点避头尾、合成斜体、CJK 标点字体回退 MUST 先进入 V-04 夹具，quiet-md 的数值 MUST NOT 写成 Aline 规范值；是否 fork/vendor 锁定 GPUI 包 MUST 另立决定并走依赖准入。离屏视觉证据 MUST 只作补充，MUST NOT 替代真实窗口、IME 与平台证据；引入锁定 GPUI 的 `test-support` feature、`gpui_macos` 平台实现（含其 `test-support` 能力）与 `image` 等 dev-dependency 前 MUST 先完成能力 spike 与依赖准入。离屏截图与真实窗口截图的文件名、证据目录和结论 MUST 分开记录。关联 R-9-02、R-11-01、UI §3.2、V-04/V-09。

#### Scenario: 语义 token 重构

- **WHEN** 组件直接使用原始色值，或 token 重构改变已确认视觉/主题包 schema
- **THEN** 判定违反 token 边界；把色值收回 token 模块并注明来源，重构前后用截图/像素对比作为审查证据，不静默改视觉

#### Scenario: 中文排版数值被直接采纳

- **WHEN** 规范或实现直接采用 quiet-md 的避头尾角度、字号、行高或字体补丁值
- **THEN** 改为 V-04 夹具中的待测候选；先测量锁定 gpui-pre 的实际行为，再决定是否 vendor/fork，数值不因引用而冻结

#### Scenario: 离屏证据被当作平台通过

- **WHEN** 只有 `VisualTestAppContext` 或离屏截图，缺少真实窗口/IME/平台操作
- **THEN** 只记录为补充证据并标明门控与依赖状态；不宣称 V-09、AC-15/AC-16 或对应 UI 行通过，不把离屏与真实截图混在同一结论

### Requirement: W0～W7 必须逐项立项

quiet-md 调研中的 W0～W7 MUST 视为工作分解与建议顺序，MUST NOT 视为已批准任务。每个 W 项实施前 MUST 建立可独立验收的 OpenSpec change，写明范围、非目标、依赖准入、证据表与适用 V 项；W0 未批准 MUST NOT 写代码。依赖准入、验收方式、跨模块契约或阶段范围变化 MUST 在对应 change 中评审后再实施。关联开发流程 §2/§3.1、CODE_STANDARDS §6、QMD-01。

#### Scenario: 直接开始 W 项

- **WHEN** 未建立对应 change 或未获用户批准，就新增依赖、测试夹具、平台代码或改动验收方式
- **THEN** 停止该实施；先补 change 并完成评审，研究文档中的建议 change 名称不自动生效

#### Scenario: 把 W0～W7 当作已批准计划

- **WHEN** 任务表、文档或评审把 W0～W7 写成已立项、已排期或已实现
- **THEN** 改回“工作分解/建议顺序”，保留各项待确认问题；本规格不替用户裁决 test-support 准入、vendor 顺序、fuzz 规模或阶段时点
