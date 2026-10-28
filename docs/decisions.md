# Aline 决策记录

本文件按 [architecture-guide.md](./architecture-guide.md) §18 的纪律维护：**凡有争议的选型，必须落一条决策记录（含背景、选项、裁决、后果）**，先例可查、不靠记忆。规则与 `docs/` 冲突时以各技术文档为准；本文件与它们冲突时，以较新日期的裁决为准并回写文档。

格式：`D-YYYY-MM-DD-NN`，一条一裁决，时间倒序（新在上）。

---

### 2026-10-28 · UI 参照范围与浅色主题

- **D-2026-10-28-14 · MarkFlow 界面复刻、Aline 品牌与 GitHub Primer 浅色主题**：MarkFlow 当前桌面应用作为 Aline 的界面结构与交互参照，覆盖主窗口、设置页、对话框、菜单、浮层和其他窗口；应用名称与图标使用 Aline。浅色主题按用户后续明确要求采用素白画布和 GitHub Primer 风格中性色、蓝色强调色，并与 Aline 图标协调，因此该主题决定优先于 MarkFlow 原主题色。功能范围仍受 v1-features.md 限定，只迁移允许范围内的功能语义。验收细节与代码块交互见 ui-spec.md。（ui-spec.md、v1-features.md）
- **D-2026-10-28-15 · V1 内置插件管理、格式导出与 Mermaid 专属主题**：一期设置页新增插件管理，管理 Mermaid、表格增强、导出、公式、代码高亮五个编译期内置插件；V1 不提供运行时第三方插件加载。导出插件支持 PDF、Word、HTML，关闭后工具栏和菜单栏的导出入口均隐藏。Mermaid 支持跟随应用明暗状态、指定内置主题及导入/自定义主题；Mermaid 主题只影响图表，不并入 Aline 全局主题。表格插件提供行列悬停快捷插入与右键增删、复制操作；关闭后保留基础表格编辑，只隐藏悬停快捷按钮。完整功能、设置与验收见 plugin-system.md。（plugin-system.md、v1-features.md、v1-architecture.md、ui-spec.md）

## 决策时间线

### 2026-10-26 · 技术方案复核裁决（全面复核四份文档后批量修订）

背景：M0 动工前对 engine-design / v1-architecture / architecture-guide / v1-features 做整体复核，发现 6 处设计矛盾（H）、7 处边界缺失（M）、9 处文档漂移（L）。以下为裁决及落点。

- **D-2026-10-26-01 · Buffer trait 按"rope 可实现"定形**：trait 不出现任何返回 `&str` 借用的方法（ropey 给不出任意区间的连续 `&str`）；`text()/slice()` 返回 `Cow`，`slice` 越界/非 UTF-8 边界返回 `None`（坐标 bug 显式暴露，不 panic），新增 `for_each_chunk` 零拷贝遍历。v1 仍 StringBuffer，v2 换 RopeBuffer 不改签名。（engine-design §2.2）
- **D-2026-10-26-02 · app-core 自有 `Color` 类型**：`Tokens` 全部 `Hsla → Color`、`SharedString → Arc<str>`——"app-core 零 GPUI"必须机器可查（`cargo tree -p aline-app-core -i gpui` 为空），类型里出现 gpui 类型即自相矛盾；转换只发生在 aline-ui。（v1-architecture §8.1–8.2）
- **D-2026-10-26-03 · 插件接口两层制**：① 内置原生层（编译期进程内）元素级 `BlockRenderer`（experimental，永不跨 wasm 边界）；② 外部资产层（wasm/进程外）`AssetRenderer → RenderDesc` 纯数据。原 architecture-guide §14.2"一律返回 desc"与 v1-architecture §9.1 的 AnyElement 签名冲突，裁决为两层制并双向统一。（architecture-guide §14.2、v1-architecture §9.1）
- **D-2026-10-26-04 · commit 预算分档**：`< 10ms（≤32KiB 典型）/ < 50ms（≤128KiB 即 AT_ONCE 上限档）`——pulldown 单解析 128KiB 实测 ~20ms，"commit < 10ms"单值预算与自家测量自相矛盾。AT_ONCE=128KiB 阈值不变。（engine-design §2.10、v1-architecture §11）
- **D-2026-10-26-05 · dirty 用 undo 栈深判定，不用 stamp**：`dirty = history.undo 栈深 != saved_at || open 有未提交修改`。undo/redo 是精确逆补丁，栈深回到保存值 ⟺ 内容与已保存字节一致（O(1)）；stamp 会把"打字后撤销回原文"误判为 dirty。保存动作隐式封口当前打字合并组。（engine-design §2.9、v1-architecture §6.3/§15.2、architecture-guide §5.7）
- **D-2026-10-26-06 · UNDO_GROUP=900ms + 补丁合并规则定形**：打字合并窗口统一 900ms（ElyMark 实证；废止 architecture-guide 原 300ms/Zed 引法）。合并规则：仅"相邻的单区间插入/删除接续"可并入既有步（区间与 inverse 对应延长）；替换、跨块、命令、commit、IME 组合结束一律新开一步（IME 组合过程整体一步）。`UndoStep` 增 `selections_before/after`（撤销/重做各恢复对应选区）；字节预算 trim 时清空 redo。（engine-design §2.2、v1-architecture §6.3、architecture-guide §3.1.4）
- **D-2026-10-26-07 · Escape 统一为提交**：块打开态与表格单元格同语义——不设"取消"手势（取消式 Escape 不可恢复地丢字；提交式 + 补丁撤销零丢失），误操作走撤销。（engine-design §2.4/§2.7、v1-architecture §6.2/§6.6）
- **D-2026-10-26-08 · aline-buffer 与 aline-source 解耦**：依赖链末端是 aline-buffer，source 独立为旁路 crate（文件 I/O 由 ui/app 直接编排）——Buffer 不需要文件层，解耦后 buffer 可纯内存测试。（AGENTS.md 铁律 1、architecture-guide §1.1、v1-architecture §1.2、engine-design §2.1）
- **D-2026-10-26-09 · 保存/崩溃恢复先 flush 打开块**：自动保存与恢复槽落盘前自动 commit 打开块（单步撤销）——dirty 判定、自动保存、恢复槽看到同一份文本，open 里的未提交编辑不会丢；恢复槽随文件正常关闭而清理。（engine-design §2.9、v1-architecture §3/§6.2）
- **D-2026-10-26-10 · 预处理改写器跳过代码**：flow/tex/front 三个改写器的扫描必须跳过围栏代码块、缩进代码块、行内代码 span 与 HTML 块——代码里的字面 `$$`、`\(` 永不被触碰，否则即数据损坏；各带专项规格测试。（engine-design §2.3.1/§2.11）
- **D-2026-10-26-11 · Win/Linux 单实例**：锁文件 + 命名管道/Unix domain socket 参数转发，消除双进程并发写 `session.json` 与同一文档互覆；macOS 系统天然单实例；M5 三平台实测。（v1-architecture §2.2/§14）
- **D-2026-10-26-12 · 混合行尾语义明示**：逐字节保真覆盖"未编辑文件"与"统一行尾文件编辑后其余字节不变"；混合行尾文件一旦编辑，少数派行尾（含未编辑行）按多数派重写——继承 Ely/Typora 的有意决策，保真套件按此断言；逐行保留列为后续可选增强。（engine-design §2.9/§2.11、v1-architecture §12）

### 2026-10-26 · HTML 支持进 V1（ElyMark 逆向后裁决）

- **D-2026-10-26-13 · HTML 支持按 ElyMark 语义进 V1**：背景——原 v1-features 把块级 HTML 真渲染列为"后续迭代"，但逆向 ElyMark 源码后发现其 HTML 能力并非浏览器式渲染：行内是「标签→样式」降维，块级是**浏览器式文本提取**（标签消费、可见文字保留、块内行内样式与实体/emoji 照常生效、注释丢弃、块级标签换行、空白折叠、过滤标签的标签本身按字面文本显示）。用户 2026-10-26 明确要求 HTML 进一期。裁决：**V1 完全对齐 ElyMark 语义**——① 行内 HTML：`u/b/strong/i/em/s/del/strike/code/kbd/mark/a` → `Style` → `StyledText::with_highlights`；HTML5 嵌套修复（`reopens` 重开 formatting elements、`breaks` 32 个块级标签、`closes_p` 隐式闭合、`<a>` 互斥、`<a>` 无 `href` 时不计链接样式）；`entities` 实体解码（命名/无分号 legacy/十进制/十六进制/C1 重映射）；GFM tag filter（`script/style/iframe/title/textarea/xmp/noembed/noframes/plaintext` 的标签本身按字面文本显示）；② HTML 块：`html_block_text` 浏览器式文本提取 + `owed` 延迟空白机制 + `LineMap` 双向映射（`Escapes::Html` 解码实体，解码出的每个字符都指向实体 `&` 的字节）；显示树落为 `Node::Paragraph`（**无 `Node::Html` 变体**）；点击走通用 `Open` 路径显示源码、commit 单区间补丁（单步撤销）+ 写回校验；③ `EditRefusal::Html` 有两条触发路径，**都读 mdast**：块命令走 `block.rs` 的 `refusal()`（`block.rs:12` 为 `use crate::markdown::mdast::Node;`，HTML 块命中 `Node::Html(_)`），行内格式走 `format.rs` 的交叠特判（`held` / `touches(true)`）；显示树里 HTML 块是 `Node::Paragraph`，那只影响渲染与落点，不影响拒绝判定——实现时不要把两处判定搞混。**否决**完整 HTML5 真渲染（需 WebView 或自建盒模型，违背"纯 GPUI、无 WebView"铁律且成本高）；**保留**"上编辑下渲染"双视图为后续迭代方向（engine-design §2.17.9）。落点：v1-features §5（三条明细）、engine-design §1.3.1 支持矩阵 + §1.10 机制逆向 + §2.7 特殊块 + §2.12 依赖（`entities = "1"`）+ §2.13 M4 + §2.17 实施方案、v1-architecture §1.3/§6.7/§12/§13。

同批文档卫生修复（非争议裁决，随批落地）：`TaskToggle` 参数 `item_range → box_at`；Command 枚举与 v1-architecture §6.3 对齐并声明单一来源；`Mode` 标注 V1 仅 Visual|Source；外部修改检测统一 mtime+size（非哈希）；notify 标注 V2 预留；`directories`/`rfd` 补登记；architecture-guide §4.3 引用修正（engine-design §1.4.2/§2.2，原 §2.6 为渲染层）；StringBuffer/CodeBuffer 命名分层注释；`FindState` 增 `stamp` + `MatchesInvalidated` 事件（废止"(块 id, 块内 range) 稳定引用"）；单块超大击穿虚拟化列为已知限制；Formatter 待定项补架构预留与决策截止（M3）。

---

## 常设决定（不设截止日期，翻案需新裁决）

| 决定 | 依据 |
|---|---|
| 解析器锁定 pulldown-cmark（0.13），缺失语法用带字节映射的预处理改写器补齐，禁止引入第二个 markdown 解析器 | Ely ADR-0041（markdown-rs 慢 ~90 倍实测） |
| gpui 全工作区单一来源精确锁版：`gpui = { package = "gpui-pre", version = "=0.3.9" }`，vendor 平台补丁，升级是显式动作 | Ely ADR-0032（两个来源的 gpui 类型不兼容） |
| Windows 10 与 Intel Mac 为必须支持目标（非"新系统优先"） | 用户决策 2026-10-26 |
| 源文本是唯一事实来源；文本永不重建，编辑永远是区间补丁；存储逐字节保真（未编辑原样写回，编辑过只重放 BOM/CRLF 多数派） | engine-design 第 2 部分不变量、AGENTS.md 铁律 2/4 |
