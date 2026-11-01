# Zed 架构与 GPUI 实践复核

更新日期：2026-10-30。身份：**参考调研，不是产品/架构规范。** 原始研究的完整论证、表、路径及行号已保留在[历史证据](./archive/migration-evidence/zed-notes.md)，按[PROTO-06方案A](./archive/2026-10-30-migration-decisions.md)完成迁入；本稿记录每一研究主题如何服从现行约束，不重新批准原文的“照搬”标签。

## 0. 证据与版本边界

旧稿自述研究对象为仓库外的 Zed 参考克隆、2026-10-26快照，并自述四个代理复核44组断言；未记录足以复原该研究快照的精确commit。本次保全没有重跑这44组断言，没有将当前checkout或gpui-pre来源rev倒补为当时Zed研究commit。

旧稿针对Zed主线新架构：Window/App分参、平台crate拆分等。Aline固定gpui-pre单一来源、精确锁快照；具体工具链/edition/版本、补丁和API须M0登记、V-01验证。旧`spawn/background_spawn/weak.update`、`handle_input`、`add_fonts`、`on_open_urls`、`set_menus`和测试harness均是检索线索，不是已可编译的Aline接口。

当前结果以[PRD 0.9](./v1-prd.md)为准，模块归属见[架构](./v1-architecture.md)，输入/映射见[引擎](./engine-design.md)，跨层边界见[R/K规范](./architecture-guide.md)，工具/类型纪律见[S/C规范](./rust-style-guide.md)。V-01～V-09均未执行。本文“已有承接”表示当前规范有同类契约，不表示Zed算法被逐字采纳或性能已通过。

## 1. GPUI 本体逐主题映射

| 旧位置/主题 | 可借鉴的机制与当前承接 | 不沿用的保证/仍需验证 |
|---|---|---|
| §1.1 Entity/效果系统 | 生命周期由宿主所有者管理；跨实体回调不意外保活；变更发布与窗口失效区分。R-3/R-6/R-8 | Weak升级失败是目标关闭的一类结果，不能用`.ok()`吞活任务错误；引擎不读Global/持GPUI或自建线程 |
| §1.2 Element三阶段/跨帧状态/Interactivity/deferred | 布局派生与业务副作用分离、稳定语义身份、焦点/浮层借还；R-8～10、UI§12/13 | 不冻结具体三阶段签名、状态表/优先级；原稿“不采用Render/a11y全套”不取消Aline键盘/基本无障碍基线 |
| §1.3 变高list/估算高度/ScrollAnchor/延后滚动 | 变高块虚拟化、逐步测量和源码锚点保持位置；架构§8、R-10 | list/uniform_list选择须核锁定API；不能靠块虚拟化掩盖巨段/巨表布局成本，V-05/09 |
| §1.4 LineLayout双帧/hash/wrap/glyph扫描 | 派生布局按真实输入/字体/宽度/换行等依赖复用；R-7/10 | 双帧/hash不是强制缓存布局；命中不证明零分配/永不shape，glyph像素命中仍经来源映射；V-05/07 |
| §1.5 InputHandler/UTF16/configuration | 平台UTF16适配→统一映射→源码坐标；配置无意义重复变化不得破坏组合；引擎§7/10、RS-02/ED-01 | 具体handler注册阶段/候选窗/marked range依锁定源码与真实IME样例；不手造确认/取消；V-01/07/08 |
| §1.6 actions/keybinding/context/chord | 统一命令身份/门控/执行复验，菜单/工具栏/键位一致；UI§10.5、R-3/5 | 原稿PascalCase/registry/context优先级/pending replay是实现候选；不新增未经确认的多键序列产品范围 |
| §1.7 Asset/异步桥/窗口/打开事件 | 宿主后台计算、完整任务身份/旧结果拒收，发布归UI；FILE-02/03、R-6/7 | 不宣称具体spawn/首帧/on_open_urls/set_menus在gpui-pre可用，也不假设macOS天然单实例；V-01/08 |
| §1.8 11项借鉴表 | 原表逐条由以上主题及R/S/UI承接机制；完整原表在证据稿 | “照搬”不是当前算法/API选择；不另引入依赖、运行时、CRDT或IDE全套能力 |

## 2. Workspace 与 UI 组件逐主题映射

| 旧位置/主题 | 当前承接 | 必须消除的旧范围或策略歧义 |
|---|---|---|
| §2.1 Item/事件桥/dirty/save | tab引用文档/图片身份，子视图发意图，DocCore/协调器提交；架构§5/7.5、R-3/15 | 不原封复制Item trait或把每个tab变独立可写会话；dirty相对实际保存内容，不能靠类型/栈深推定 |
| §2.2 Pane/MRU/SaveIntent/close/prompt/拖tab | 标签独立视图状态、关闭保护/焦点归还；PRD FILE-06～08、UI§5.1/12.1 | 逐tab保存/放弃/取消；不照搬SaveAll批量询问、重排关闭顺序、格式化并保存或失败后直接关闭；tab拖排不因旧研究纳入新范围 |
| §2.3 Panel/Dock/树有序展开集/事件打开 | 单窗口工作区、文件树与目标事务；UI§5.2、FILE-04/09 | 不新增三侧dock/远程/collab/预览tab模式；原稿“砍内联重命名”不能删FILE-04重命名能力，具体承载V-09 |
| §2.4 Modal/Toast/Notification/SQLite恢复 | 可取消流程、一次有效目标、去重/焦点、恢复与偏好分开；UI§12/13、TA-03 | 不照搬10s toast/SQLite/窗口tabs自动恢复；常规启动仅最后根/侧栏单窗口，异常恢复先内存。WC-01新增的是有归属/预算的内存诊断汇总，非数据库 |
| §2.5 open_paths/窗口工厂/StyledExt/主题注入/组件白名单 | 应用级会话去重和系统打开语义；组件消费有效配置；R-1/13、PRD FILE | 不把OpenMode/StyledExt/elevation/11组件白名单作为冻结接口或像素；设置/字体/菜单按现行专项与V验证 |
| §2.6 八项壳层借鉴表 | 原表每项归入以上五主题，原表完整保留 | SQLite会话、文件树单击预览/双击激活、SaveIntent整套不可直接搬为Aline需求 |

## 3. 编辑器内核与查找替换

| 旧位置/主题 | 当前承接与改写 | 证据边界 |
|---|---|---|
| §3.1 CRDT事务/undo/300ms/选区快照 | 补丁历史、选区before/after、空事务/拒绝零步；TA-01为900ms相邻输入组、IME确认独立组；引擎§8～10 | 不采用CRDT/Lamport/SumTree undo，也不以旧300ms覆盖当前决定；保存快照不封口 |
| §3.2 HighlightKey/decorations/Replace/高度 | 语义着色层与内容来源分离、打开块估算/测量及稳定布局；R-7/10、UI§6/7 | “height=None不占位”仅是被研究版本的语义；不照抄枚举优先级/BlockPlacement或GPUI类型到引擎 |
| §3.3 IME marked/group_until/关闭autoclose/no_scroll | 组合确认/取消、统一输入状态与来源映射；ED-01显式发布等待自然结束；引擎§10 | marked高亮key、macOS替换范围/候选窗公式须按锁定接口验，不把组合每次更新当已提交历史 |
| §3.4 全量重搜/后台切块/稳定块引用/replace_all | 按含草稿的当前源码查询，结果绑工作/查询版本，过期作废；全部替换一步事务；FIND、引擎§9.4/12 | 原稿“块id+块内range稳定”已失效；不用显示块局部搜索代替可跨显示块的源码匹配，不引入regex/跨文件检索 |
| §3.5 多层DisplayMap/chunk缓存/1ms窗口/多维Edit | 完整来源/投影身份、补丁广播与真实依赖失效、优化与全量差分；引擎§7/12、R-12 | 不冻住五层/1ms/SumTree/Anchor/ropey路线；源坐标持久，绘制补丁不可反写源；V-05/07 |
| §3.6 A/B档清单 | 各机制归以上主题；原索引及原优先级全文保留 | A/B是历史研究分档，不是当前M任务授权；CRDT仍不采用，rope迁移需性能证据 |

## 4. 主题、设置与文件系统

| 旧位置/主题 | 当前承接 | 明确替代/未采用部分 |
|---|---|---|
| §4.1 Family/refine/选择分离/注册表/外部目录 | 四家族三模式、内容/选择/有效回退分开、逐叶继承、有效快照发布；THEME/TH、主题规范§3～11 | 主题正式四JSON/schema1/PNG；不是单TOML，也不按name覆盖冲突；同ID拒绝、受保护GitHub根、失效修复显式重选。无主题自动watch/notify；~15键/~85行只是旧研究自述 |
| §4.2 六层设置/快照/Global/变更重算 | app-core纯模型，UI持有效配置/发布，窗口消费；APP-05/06、R-13 | 不引入六层SettingsStore/derive/inventory整套；引擎不读隐藏locale/Global，用户/工作区作用域依PRD，不凭旧MergeFrom新增覆盖层 |
| §4.3 Fs watch/Rescan/FakeFs ZERO/OnceLock路径/NFS | 时间/文件能力注入，故障/乱序可控夹具；source受检I/O原语，UI编排；架构§7、R-6/15 | V1不预建watch签名/Rescan，不以ZERO去抖固定所有测试；路径服务app注入，不在source设OnceLock目录单例；平台权限由WC-04条件与V-08证明 |

## 5. 全景表、索引与实际使用顺序

旧§5的12行全景表分别由本稿§1～4承接：源码查找/历史/IME/打开块/虚拟化/壳层/主题/设置/文件/插件/系统打开/性能。旧“wasmtime路线已定稿”没有当前效力；PA-01仅确认后续外部方向，执行/隔离/协议/兼容/阶段另设计，不进入V1依赖。

旧§6完整文件索引在历史证据的末段，覆盖gpui的app/context/element/div/list/uniform_list/text_system/line_layout/input/window/keymap/key_dispatch/asset_cache/svg_renderer/platform，workspace的item/pane/dock/modal/notifications/toast/persistence，及project_panel/ui/text/editor/language/search/theme/theme_settings/settings/fs/paths。**索引完整留存不等于路径/行号现今仍相同。** 实现时先按本稿主题定位，再在锁定源码查真实API和测试，不以旧file:line代替核对。

推荐顺序：PRD/验证计划 → 所属架构/引擎/Rust/UI/插件/主题契约 → 本稿对应主题 → 必要时查完整原始证据和锁定源码。具体库版本、缓存结构、事务API、平台能力和限额均按M0/V实验；本文无新的用户待确认问题，不把已明确的WC/TA/ED/RS/UI/PL/TH再次列为待定。
