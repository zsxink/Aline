# v1-architecture 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/v1-architecture.md`；原文件 59552 bytes，SHA-256 `8af1cb1e1d854d0c897271402e51b98f0026dbba7656fda29a56680091aee30b`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L10 · 原蓝图身份

原片段 SHA-256：`b775db278f9ed93c87bf57c43e7f3e26259cb97abb12ef511ce837da670abd7a`。

~~~~~~~~markdown
# Aline V1 技术架构与实现细节

> 将 [v1-features.md](./v1-features.md) 的 9 组功能点映射到具体架构与实现。
> 引擎机制的调研依据与设计取舍见 [engine-design.md](./engine-design.md)（elymark 逆向分析），本文不重复论证，只给"怎么建"。
> Zed 源码（GPUI 生态最完整参考实现）的架构与实践调研见 [zed-notes.md](./zed-notes.md)——各章借鉴清单按"照搬/改造/不采用"分档，§5 有映射到本文各节的全景表；注意其 gpui 主线与 gpui-pre 存在版本代差。
> 实现期必须遵守的边界与纪律（【强制】/【建议】条款）见 [architecture-guide.md](./architecture-guide.md)；第 15 章收录 Ely 实证的 GPUI 实现要点，与该规范配套。
> 本文的里程碑取代 engine-design.md §2.13。

---

~~~~~~~~

## L570–L645 · 旧外置插件宿主研究/索引（Wasmtime等路线无当前效力）

原片段 SHA-256：`f05685682a2303a2253fda031c9e9ef70505797c123bc75a2b07791fd27a91d7`。

~~~~~~~~markdown
## 9.3 加载模型与外置化路线（参考 Zed 源码）

> 依据：Zed 扩展系统源码分析（2026-10-26，稀疏克隆于 `/Users/xian/Project/book/zed`，关键文件索引见本节末）。
> V1 不实现本节；这是 V2 外置插件的定稿路线，写下来是为了让 V1 的接口形状不做返工。

### 9.3.1 最重要的分层借鉴：声明式资源不进沙箱

Zed 把扩展能力拆成两层：**主题 JSON、语言 config.toml、snippet 这类声明式资源由宿主直读，根本不进 wasm**；wasm 只跑动态逻辑（slash command、语言服务器代理、下载安装）。代价高的边界调用只发生在真正需要逻辑的地方。

Aline 的映射：

| 资源 | 层 |
|---|---|
| 主题 JSON 包、社区语言包（i18n catalog TOML） | 声明式，宿主直读，永远不进沙箱 |
| PlantUML、图表引擎、图床、其他公式后端 | wasm 插件（资产型：传块源码，回 SVG/URL 补丁） |
| 表格/数学/Mermaid/高亮 | 内置插件（元素级 API，进程内） |

### 9.3.2 宿主机制（照抄 Zed，wasmtime 48 + component-model）

- **Engine 全局单例**：`wasm_component_model(true)` + `epoch_interruption(true)` + 后台 100ms 心跳 `increment_epoch`；store 设 `epoch_deadline_async_yield_and_update`——超时**协作式让出**而非硬杀。Zed 没有 fuel、没有内存限制，Aline 同样不引入。
- **编译缓存走内存**：wasmtime `CacheStore` 包装 moka LFU 缓存（~32MB），不落盘；每扩展编译产物约 512KB 条目。
- **每扩展一个 actor**：mpsc channel 把对该实例的调用**串行化**（天然免去重入），跑在 tokio 上；要碰主线程的结果经 channel 回 GPUI。
- **文件系统沙箱**：WASI 只预开放 `work/<id>/` 读写；宿主侧对路径 canonicalize 后做前缀校验防 `..`/symlink 逃逸；guest 侧禁 `chdir`。
- **能力门禁（双层）**：manifest `capabilities` 声明 × 用户设置授权（默认拒绝），用于 `process:exec`、任意 URL 下载。Aline V2 第一版可只信本地 dev 扩展，门禁后加。
- **依赖隔离**：wasmtime 编译很重，宿主放独立 crate `aline-extension-host` + cargo feature 门控，不拖累默认构建。

### 9.3.3 接口、版本与 guest 形态

- WIT world：guest 只强制导出 `init-plugin` 一个函数，其余（`render-block` / `run-command` / …）可选——Zed 同款约定。渲染返回**数据描述**（WIT record 优先，配置类参数才用 JSON 字符串，Zed 两种都用）：

```wit
package aline:plugin@0.1.0;          // 草图，外置化启动时定稿

world plugin {
  export init-plugin: func();        // 唯一强制导出
  export render-block: func(source: string, lang: option<string>,
                            theme: theme-info) -> result<render-desc, string>;
  export run-command: func(id: string, arg: string) -> result<string, string>;
  import log: func(level: log-level, msg: string);
}
// render-desc = svg(bytes) | styled-runs(...) | highlight-ranges(...) —— 数据，不是元素
```

- guest 侧发 `aline-extension-api` crate：`trait Plugin`（方法全带默认实现）+ `register_plugin!` 宏（导出 `init-plugin` 符号），扩展作者 `cargo build --target wasm32-wasip2` 即得组件——形态完全对照 Zed 的 `zed_extension_api`。
- **版本内嵌**：build.rs 把 API 版本写成 wasm 自定义段 `aline:api-version`（Zed 用 6 字节 3×u16），宿主加载时先读段再选 linker。
- **版本策略裁剪**：Zed 为 10 个 WIT 版本各生成一个 bindgen 模块 + 枚举分发 + 旧版降级 stub（每版本约千行样板），以支持市场里所有旧扩展共存。Aline **只支持当前一个版本**，API 变更即断（扩展就几个、重编成本为零）；将来有外部用户再引入多版本分发。

### 9.3.4 清单、目录与 dev 工作流（照抄结构）

- `extension.toml`（id / version / lib / 声明式资源引用 / capabilities），带自动推导：扫目录补默认值（Zed 的 `populate_defaults`）。
- 磁盘布局：`extensions/installed/<id>/{extension.toml, extension.wasm, …}` + `extensions/work/<id>/`（沙箱可写区）+ `index.json`（反查表，mtime 决定重建）。
- **dev 扩展工作流**：`cargo build --target wasm32-wasip2` → 在 `installed/<id>` 建 symlink 指向源目录；`is_dev` 的判定就是这个 symlink。重建 = 重编 + touch。
- **不做**：扩展市场、tar.gz 分发、签名验证——Zed 的信任链在服务端 registry，宿主侧只查 Content-Length；个人项目本地目录即信任。

### 9.3.5 工作量预估

有 Zed 作地图，V2 外置化 ≈ 2–3 周：`aline-extension-api`（trait + 宏 + WIT）约 3 天、`aline-extension-host`（engine/actor/沙箱/dev 工作流，单版本）约 1.5 周、首个 wasm 插件（PlantUML）走通端到端约 2–3 天。

### Zed 关键文件索引（扩展宿主）

```
crates/extension_host/src/wasm_host.rs            Engine/Store/实例创建、epoch、缓存、actor
crates/extension_host/src/extension_host.rs       ExtensionStore：安装/目录/index/dev 扩展
crates/extension_host/src/wasm_host/wit.rs        版本枚举 + 宿主→guest 调用的版本分发
crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs  bindgen、linker、host import 实现
crates/extension_host/src/capability_granter.rs   manifest×settings 双层能力门禁
crates/extension/src/extension_manifest.rs        extension.toml schema 与自动推导
crates/extension/src/extension.rs                 宿主侧 Extension trait
crates/extension/src/extension_host_proxy.rs      宿主→应用注入接口（8 个 proxy trait）
crates/extension/src/extension_builder.rs         cargo/wasi-sdk 编译、grammar 编译
crates/extension_api/src/extension_api.rs         guest trait + register_extension! 宏
crates/extension_api/wit/since_v0.8.0/extension.wit  最新 world 定义
```

---

~~~~~~~~

## L734–L787 · Ely原始GPUI要点/测试索引/类型速查

原片段 SHA-256：`bd46537cd0a7bf66992684d34449e048a365be9f1492078e601faacd26201a66`。

~~~~~~~~markdown
# 第 15 章 Ely 实证的 GPUI 实现要点

本章收录 ElyOffice 源码验证过、但前 14 章未展开的壳层与 GPUI 落地要点；逐条纪律的【强制】依据见 architecture-guide §8–§10、§13。

## 15.1 窗口骨架与启动

- 一个窗口一个 `Root` 实体：焦点根（`FocusScope.root()`）+ 菜单 + 命令面板 + Toast 视口 + 内容视图，全部从 Root 挂出；焦点链、浮层、toast 都不散落在各视图里。
- 初始化收口一个 `init(cx)`（含 `init_for_tests`）；主题/语言/设置 `Global` 在此注册，首帧主题用"立即应用"路径（Ely `set_mode_now`），避免首帧闪默认色。
- 窗口关闭确认走统一 guard（Ely `shell::guard_close`），不直接 `on_window_should_close`——后者会被自绘标题栏绕过。关闭询问用 SaveIntent 三选（保存/放弃/取消，未保存的排最后）。

## 15.2 焦点、undo 与 dirty

- 打开块子编辑器的 Undo/Redo 用 `capture_action` 接管，转发到文档级历史；写回恒为一个撤销步（§6.2）。
- dirty 是合成判定：`history.undo 栈深 != saved_at || open 有未提交修改`（2026-10-26 修订：不用 stamp——undo/redo 是精确逆补丁，栈深回到保存值 ⟺ 内容与已保存字节一致，stamp 会把"打字后撤销回原文"误判为 dirty；保存隐式封口打字合并组，详见 §6.3）；状态栏、标签页、关闭 guard 消费同一个函数，不各自维护 bool。

## 15.3 Command 表落地

- Command 定义集中在 `workspace/commands.rs` 一张表（`id / keys / blocked / action`），快捷键绑定、菜单、命令面板、禁用态全部由表生成；未知 id 记 warn（architecture-guide §5.2）。
- 菜单文本走 i18n key；`blocked` 由命令自己声明（如无打开块时表格命令禁用），不由菜单层判断。

## 15.4 测试夹具与冒烟

- gpui 集成测试夹具独立成模块（`open` / `press` / `frames` 三件套，Ely 形态），不用私有函数直调，统一走 `debug_selector` 命中。
- `debug_selector` 约定值（随实现一起写）：块 `markdown-block-{ix}`、打开块 `markdown-open`、表格单元格 `markdown-open-cell`。
- M0 起提供 `ALINE_SMOKE` 冒烟钩子（建窗口 → 打开样例文档 → 渲染 N 帧 → 退出），供 CI 与三平台验收复用。

## 15.5 失焦与布局

- 失焦窗口设 `inactive_frame_interval`（Ely 250ms）降频；动画只在运行时请求帧，空闲零重绘。
- 会话恢复的窗口布局保存/运行同形（一个 `Serialize`/`Deserialize` 类型，Ely `PaneLayout` 形态），版本化 JSON、全有或全无恢复（§2.3、architecture-guide §11.5）。

---

# 附录 A 解析内核要点索引

解析内核（改写器、mdast、展示树、sources 映射、alike 增量、解析调度阈值、保真七不变量）的完整设计与 elymark 代码对照，见 engine-design.md §2.2–2.3、§1.4–1.5 与附录文件索引。本文档不重复。

# 附录 B 关键类型速查

```
Source                    文件字节层（BOM/CRLF 重放）
Buffer trait / StringBuffer/CodeBuffer  Buffer 的 v1 实现（纯文本）／编辑器级聚合（Buffer+选区+History+IME，DocCore 持有）
Parsed / Block            解析结果（块源码区间不含尾部换行）
Node / Inline / sources   展示树 + 双向映射（Escapes 四类：Plain/Markdown/Html/Pipes）
Style                     行内样式 token（Strong/Emphasis/Strike/Underline/Code/Mark/Link）
DocCore                   编排状态机（模式/打开块/解析调度/查找）
OpenBlock / OpenKind      打开块（Plain/Panel/Split/Cell）
Command / Change          命令 → (Vec<(Range,String)>, 选区)
EditRefusal               拒绝制（Changed/Code/Math/Html/FrontMatter/RereadMismatch/…）
FindState                 查找替换（纯文本，无正则）
Theme / Tokens / Mode     主题（aline-default × Day/Night/Eye）
Plugin / BlockRenderer    插件接口 v1（渲染器/命令/catalog）
Workspace / Tab / FolderPane  窗口=工作区（标签页 + 文件夹浏览器）
```
~~~~~~~~
