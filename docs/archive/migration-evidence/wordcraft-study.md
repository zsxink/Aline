# wordcraft-study 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/wordcraft-study.md`；原文件 37209 bytes，SHA-256 `9bc8058419c77cff4f48083bc3725be6225c06d5757e55d40644f59ff0a56b9a`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L367 · 完整原始研究：工程证据/21条W原案/旧案例/完整附录；历史提案不自动采纳

原片段 SHA-256：`9bc8058419c77cff4f48083bc3725be6225c06d5757e55d40644f59ff0a56b9a`。

~~~~~~~~markdown
# WordCraft 调研：Markdown 解析、性能与保真 —— 及 Aline 改进方案

> 调研对象：`storytold/wordcraft` @ 0.11.0（2026-10-28 本地快照，`/Users/xian/Project/book/wordcraft`，下文引用均为该快照的 `文件:行号`）。
> 定位：**分析报告 + 候选提案清单**。按 [architecture-guide.md](./architecture-guide.md) §18.2，参考项目的新结论**不自动**改变规范——本文第 5 部分每条提案标注【落点】【验收】【决策需求】，采纳一条落一条：`decisions.md` 记一条，回写管辖文档。
> 配套阅读：[engine-design.md](./engine-design.md)（机制之"为什么"）、[v1-architecture.md](./v1-architecture.md)（怎么建）、[architecture-guide.md](./architecture-guide.md)（边界纪律）。

---

## 0. TL;DR

WordCraft 是纯 Rust 的 Word 处理器（docx 为主，~85.6k 行、15 个 crate，`docs/architecture.md:11-34`），markdown 只是它众多导入/导出格式之一。它**手写了整个 markdown 解析器**（不用任何库，`crates/formats/src/markdown/` 共 ~1.6k 行），转换成内部文档模型——是**有损转换路线**，与 Aline"源文本唯一事实来源"的路线根本不同。**路线不学，工程学全收**：它的行内解析抗二次方设计（扫描预算 + 索引记忆化）、导出"读回恒等"转义纪律、"丢弃必须留痕"（Dropped 账本）、敌意输入预算全家桶、原子保存全细节（权限保留/失败不落盘）、性能回归案例库（每个优化挂 issue 号 + 数字 + 测试），全部可直接移植到 Aline 的对应环节。第 5 部分共 24 条提案：6 条 P0、10 条 P1、8 条 P2，其中 3 条需要先落决策记录。

---

## 1. WordCraft 是什么，与 Aline 的可比性

### 1.1 项目概况

- 纯 Rust word 处理器：读写 `.docx/.doc/.odt/.rtf/.html/.md/.tex`，egui 渲染，macOS/Windows/Linux/BSD/Web 四端。~85.6k 行，15 crate，分层由 `cargo xtask layers` 机器强制（`docs/architecture.md:11-14`）。
- 质量门槛：821 个 `#[test]` + 10 个 property-test 块；`hostile_params_never_panic` 把全部 422 条命令灌垃圾参数；`cargo xtask ci` = fmt + clippy `-D warnings` + 测试 + 资产署名 + 分层 + wasm 构建（`docs/architecture.md:53-57`）。
- 保真方法论成文档：`docs/file-format-parity.md` 给每个格式标"保真 % × 用户流量权重 × 修满小时数"，并有"地板只升不降"的棘轮测试（`docs/architecture.md:58-59`）。

### 1.2 路线差异：先说清楚什么不能学

| | WordCraft | Aline |
|---|---|---|
| 编辑对象 | 内部文档模型（样式化段落/表格树） | 源文本本身（唯一事实来源） |
| markdown 的地位 | 导入/导出的一种**有损转换** | 唯一格式，解析树只是源文本的投影 |
| 字节映射 | 无（转换即失真，不追踪来源字节） | sources 双向映射（七条保真不变量，engine-design §1.5） |
| 解析器 | 手写 CommonMark 子集 | 锁定 pulldown-cmark + 带字节映射的预处理改写器（常设决定） |
| 撤销 | 全量结构快照（Arc 共享便宜） | 区间补丁栈 |

它的 markdown 解析不需要保真——`import` 直接把 `#` 变成 Heading 样式、`**` 变成 bold，源标记被消费掉。因此**它的解析架构整体不可照搬**（Aline 禁止第二个解析器、禁止"从 AST 反序列化出 markdown"）。但它为了在敌意输入与真实世界文件上活下来所积累的每一项工程细节，都正好是 Aline 自研部分（改写器、映射、alike、HTML 栈、markdown_hl）需要的同款防御。

---

## 2. Markdown 解析实现分析

### 2.1 总体路线

- 两个文件承载全部逻辑：`blocks.rs`（673 行，行级块解析）+ `inlines.rs`（570 行，行内 token 化 + CommonMark 强调算法）。模块自述："a CommonMark subset plus GitHub tables and strikethrough"（`markdown.rs:1`）。
- 管线：整篇文本 → 按行预处理（剥 BOM、去 `\r`、行首 tab 按 4 列制展开成空格，`blocks.rs:142-152`）→ 先摘出链接引用定义（`take_definitions`）→ `parse_blocks` 递归下降产 `model::Flow` → `model::to_doc` 转内部模型。
- 行内：`tokenize` 把文本扫成 `Node` 流（Text / Inline / Delim 三种），`emit` 跑 CommonMark "process emphasis" 分隔符栈算法摊平成带格式的 span（`inlines.rs:214-570`）。

### 2.2 块级解析：不认识的一律保留

递归下降（容器块递归，`MAX_NEST = 32` 深度上限，`blocks.rs:5`），覆盖 ATX/setext 标题、段落软硬换行、引用（lazy continuation）、围栏/缩进代码、主题线、嵌套列表（`marker_space` 按列宽算内容缩进）、GFM 管道表格（`split_row` 处理 `\|` 转义与行尾反斜杠奇偶，`blocks.rs:274-306`）。

最值得注意的两个语义决策：

1. **引用定义只从"块的起点"摘除**（`blocks.rs:92-103` 注释整段论证）：CommonMark §4.7 规定定义不能打断段落，所以"段落中间的定义形行"是作者正文，一个字符都不能动——"deleting a line that was part of a paragraph would lose it silently"。摘除 pass 对围栏的开关判定与块解析器用**同一套规则**（`blocks.rs:110-115`），否则把代码行误判为围栏就会误删正文（配套测试 `a_definition_shaped_line_that_is_not_a_definition_stays_in_the_document`，`markdown/tests.rs:87-115`，六个边界场景）。
2. **不识别的构造原样保留为可见文本**：脚注定义 `[^1]: ...` 不是链接定义，留在正文（`blocks.rs:61-64`）；理解不了的东西宁可当文本显示，不静默吞掉。

这两条与 Aline"非规范语法就地保留 + 命令拒绝制"（engine-design §1.5 不变量 3/4）是同一哲学的两种表述，可作为佐证引用。

### 2.3 行内解析：CommonMark 强调算法 + 三件反退化工程

`emit` 的强调配对是教科书实现：分隔符栈 + `bottom[字符][闭合长度%3][可开]` 三维数组记忆化（`inlines.rs:443`），这是 CommonMark 参考实现 cmark 的已知 O(n) 优化，避免每对强调向栈底全扫。另有两处防退化注释写得很明白：

- 分隔符字面量继承"前一个文本节点的格式"，用 `last_text` 增量跟踪而不是每个分隔符向前搜索——"quadratic on a paragraph of delimiters"（`inlines.rs:536-537`）。

**三件反退化工程**（对 Aline 自研扫描逻辑最有参考价值的部分）：

1. **反引号 run 索引**：预扫描一遍建 `Backticks(HashMap<run长度, Vec<起始位置>>)`，闭合 code span 变成 `partition_point` 二分查找（`inlines.rs:85-109`）。注释点名动机：对每个 code span、每对链接括号都重扫全文找闭合 run，是超线性的。
2. **括号配对记忆化**：`matching_bracket` 的结果（含"配不上 = None"）记进 `known: HashMap<起点, Option<终点>>`；后续链接扫描命中同一 `[` 直接查表（`inlines.rs:45-80`）。**连"失败"也记忆**——失败扫描同样昂贵。
3. **总扫描预算**：`scan_budget = 文本长度 × 8 + 8192` 字节；每次链接/图片前瞻扫描按实际读过的字节数扣减，预算耗尽后剩余的 `[` 一律按纯文本处理（`inlines.rs:220, 297-309`）。注释点名威胁模型："Each unmatched `[` would otherwise rescan the rest of the text, which is quadratic on hostile input"——一篇全是未闭合 `[` 的文档，复杂度从 O(n²) 压回 O(n)。

防御上限全家桶（行内）：`MAX_TAG = 4096`（`within()` 截断搜索窗口且**不切字符边界**，`inlines.rs:419-429`）；实体解码限定 33 字节窗口；图片仅 `data:` URI 入图，其余留 alt 文本；导出侧链接目的地先过 `is_safe_href`（`export.rs:151`）。

### 2.4 导出：以"读回恒等"为准绳的转义纪律

WordCraft 导出 markdown 时的转义不是"看起来对"，而是每条都有**读回恒等**的精确判据：

- `link_target` 的文档原话："`l` as a Markdown link destination that **reads back as exactly `l`**"（`export.rs:201-221`）。Markdown 读方会解码的东西（反斜杠转义、实体引用 `&amp;`）写回时补 `\`；`<`/`>` 必转义——注释点名安全动机："deleting `<` or `>` could turn an accepted relative target into `javascript:…`"。
- 围栏代码块的围栏长度 = **内容里最长反引号 run + 1**，注释记录历史 bug：曾经 `.min(64)` 封顶，内容含 64+ 个反引号时内容行被读回成闭合围栏（`export.rs:325-334`）。
- `escape_line_start`：行首防块结构注入（`#`/`>`/`- `/`=`/数字`.`/主题线），行首缩进截到 3 空格——4 空格会变成缩进代码块（`export.rs:26-56`）；列表项再补一层"弹点 + 短横线会拼成主题线"的防护（`export.rs:364-366`）。
- 强调前把空白移出标记文本（`**bold **` → `**bold** `），保证标记正确闭合嵌套（`export.rs:99-140`）。

这套纪律对 Aline 的直接意义：**v1 不导出，但 formatter 是待定项**（v1-features 头部，决策截止 M3）——formatter 本质是"全文读回恒等的重写"，`link_target`/`code_span`/`escape_line_start` 的规则表可以几乎逐条搬进 formatter 的规格测试。

### 2.5 丢失显式化：Dropped 账本

导入器凡是超出上限丢数据，必须记账：`Dropped::{Font, Link, RunStrings, StyleName}` → `log::warn` 一条人类可读消息（"import: links longer than 8192 bytes are dropped; their text is kept"），**每类每文档只报一次**（thread-local 位图，`begin_import` 复位，`model/types.rs:47-84`）。#3873 评审语被写成注释："no limit drops data without saying so"。

这正是 architecture-guide §16.2"出错就暴露，禁止静默兜底"在解析层的落地形状，且比"每项一条日志"多想了一步：**按类去重**，敌意输入不会刷爆日志。

### 2.6 对照结论

| WordCraft 做法 | Aline 采纳方式 |
|---|---|
| 手写解析器 | ❌ 不采纳（铁律 5：锁定 pulldown-cmark） |
| 扫描预算 / run 索引 / 失败记忆化 | ✅ 模式移植到自研扫描 pass（§5.1-W1） |
| 不认识即保留为文本 | ✅ 已是同哲学，作先例锚点引用 |
| Dropped 账本 | ✅ 移植为"降级账本"事件族（§5.2-W2） |
| 读回恒等转义纪律 | ✅ 预留给 formatter 规格（§5.3-W3） |
| MAX_NEST/MAX_TAG/实体窗口 | ✅ 对应上限表进自研 HTML 栈/改写器（§5.4-W4） |

---

## 3. 性能架构分析

### 3.1 数据面：Arc 写时复制 + 快照 undo + panic 恢复

- 文档块以 `Arc<Block>` 存储，编辑走 `Arc::make_mut` 写时复制（`crates/doc/src/document.rs:104,229,249`）；撤销 = 整文档结构快照，但 Arc 共享让快照便宜（`docs/architecture.md:42`）。栈深控制：快照只带最近 `SNAPSHOT_HEAD = 4` 步 undo（`crates/engine/src/snapshot.rs:35-36`）。
- **never-crash 标准**：`Session::run` 捕获逃逸 panic 并恢复文档（`docs/architecture.md:42-43`）；`EditSnapshot`（文档 + 选区 + undo/redo 栈头 + IME 组合态 + dirty 位）在危险命令前拍照、失败整体恢复（`snapshot.rs:9-33`）。
- 字节表示影响恢复 I/O 数倍的实际教训：媒体字节 serde 默认按数字数组序列化，27 MB 图片造出 99 MB 恢复副本（"6 倍"），改 base64 字符串后回到 ~1.3 倍（`crates/doc/src/byte_store.rs:1-8`，#586）。

### 3.2 渲染面：增量重排与页级光栅缓存

- 布局**增量**："an edit relayouts only what changed (**1.4 ms on a 188-page document**, measured 2026-10-06)"（`docs/architecture.md:45-46`）——块/段落级增量重排的正确性与量级，有第三方实测背书，与 Aline `alike + splice` 同粒度。
- `Session` 持有 layout cache（`docs/architecture.md:27`）；render 层每页光栅结果缓存、像素对齐（`docs/architecture.md:47`）——渲染结果缓存到"页"这一级。
- 保存性能案例：32,000 个 content controls 保存 4 秒 → 十分之一秒，病根是"每段落在全集合里找自己的标记"与"两两比对找冲突"，改为一次 lookup + 一趟 sweep，**写入结果不变**（`docs/file-format-parity.md:130-134`，#13，有专门回归测试 `content_control_save_cost.rs`）。

### 3.3 I/O 面：原子保存、后台恢复、非阻塞读取

- **原子保存全细节**（`save_path_with`，`docs/file-format-parity.md:180-183`）：先在内存完成序列化/加密，再建**独占**（`O_EXCL`）同目录临时文件 → 写 → fsync → rename 替换；Unix 上临时文件以 `0600` 起步、替换已有文件时继承原文件基础权限；失败时**绝不 in-place 写**，原文件字节不动，错误文案指引"另存为/给权限"；NAS/GVFS 这类无 Unix mode 的挂载容错（#3003）；目录本身不 fsync 明示为已知限制。清理只删本次临时文件，best-effort。
- **崩溃恢复在 worker 线程**：主线程每帧 `recovery_tick` 收割结果 + `wake` 回调驱动（`crates/engine/src/recovery.rs:25-64`）——周期性恢复副本落盘不占 UI 线程；恢复列表可逐个选择恢复/丢弃，密码操作前先清恢复副本（`recovery.rs:169`）。
- **大文件读取**：引擎用 `O_NONBLOCK` 检查式读文件（#3666，Cargo.toml libc 依赖注释），网络盘上不卡 UI。

### 3.4 性能文化：案例库

每个优化都挂 issue 号 + 数字 + 回归测试，且**病根模式可归类**：

| 模式 | 案例 | 病根 → 修法 |
|---|---|---|
| 每项做全集合扫描 | #13（4s → 0.x s） | 每段落全集合找标记 → lookup + 单趟 sweep |
| 每字符做一次解析 | #3692/#3708 | 格式化/链接 run 按字符读 → 按变更读一次 |
| 每引用复制大字符串 | #3713 | 目的地逐引用复制 → 共享 `Arc<str>`，加 64 MiB 总预算 |
| 逐文档全量重查 | #3411 | 多 relationship 文档每次查全表 → 从已解析主文档顺带取 |
| 字节表示选错 | #586 | 数字数组 → base64，恢复副本 6 倍 → 1.3 倍 |
| 二次方扫描 | markdown scan_budget | 未闭合 `[` 重扫全文 → 总预算 + 记忆化 |

### 3.5 对 Aline 的映射

Aline 已有对应物且大部分已定案：块级增量（alike/splice）、补丁式 undo（内存 O(编辑量)，比 Arc 快照更优）、异步资产 + 内容哈希缓存、变高虚拟化、原子写、保存串行化。WordCraft 证明了两件事：**Aline 选的粒度是对的**（1.4ms/188 页的同粒度实测），以及 Aline 计划里还有五个空白：① 崩溃恢复槽位写未指定线程（10s 一次的全文本写盘，大文档会卡帧）；② `atomic_write` 未规定权限保留与"失败不碰原文件"的完整语义；③ 大文件读取未考虑网络盘阻塞；④ 命令层无 panic 兜底恢复；⑤ 没有"一次输入只重绘一次"之外的实测文化锚点。逐条见 §5。

---

## 4. 保真与健壮性分析

### 4.1 保真方法论：量化、透传、棘轮

- **量化**：每格式保真 %（按"典型真实文件存活的内容与格式份额"估计）× 用户流量权重 → 总盘 ~60%，并自我声明"DOCX 保真从未在真实语料上验证过"（`docs/file-format-parity.md:10-13`）。诚实标注未验证范围，是这份文档最值得学的姿态。
- **保不住就透传**（"Preserve what we can't render"，beta 判据之一，`file-format-parity.md:188-193`）：native ink、OLE 对象、chart/SmartArt、自定义 XML store、嵌入字体——读不了就整块保留、原样写回；未建模的 `settings.xml` 项**合并进原文件**而非重新生成（`file-format-parity.md:114`）；未编辑的 glossary parts **逐字节写回**（`file-format-parity.md:122`）。
- **棘轮**："a test keeps the catalog percentage above a floor that **only rises**"（`docs/architecture.md:58-59`）——保真覆盖只增不减，防止回归悄悄吃掉已有用例。
- 格式判定按内容不按扩展名：Word/RTF 签名嗅探，扩展名说谎也按内容打开；二进制文件拒收（#569/#2853）。

对 Aline 的映射：Aline 的字节级源文本保真**严格强于**这套（未编辑字节原样写回），百分比体系无意义；可移植的是两个方法：**保真测试地板棘轮**（用例清单只增不减）与**打开时内容嗅探拒收二进制**。

### 4.2 敌意输入防御：预算全家桶

WordCraft 对每种格式都定了显式上限并在注释里写威胁模型：

- 表格数据（xlsx/ods 合并名单，`file-format-parity.md:63`）：64 MiB/part、100,000 行、1,000 列、2M 单元格、单格 32,767 字符、单 sheet 64 MiB 文本——最后一条点名防御"共享字符串在每格重复导致内存爆炸"（#1287）。
- 二进制 .doc：扇区表长于文件本身→解析前拒收（#3642）；alt 文本/链接/图形共享 64 MiB 拷贝预算（#3709-#3710）；图片解码共享 512 MiB 预算、别名条目只解码一次（#3711-#3712）；DIB 转 PNG 时字节上限同时用作解码器分配上限（防解压炸弹）。
- zip 包：失败的 zip 条目计入大小限额；大小写变体 part 名视为一个（`file-format-parity.md:124`）。
- markdown 面的上限见 §2.3/§2.5（MAX_NEST 32、MAX_TAG 4096、实体窗口 33 字节、MAX_LINK 8 KiB、MAX_RUN_STRINGS 64 MiB）。

### 4.3 数据安全

- 原子写全细节（§3.3）之外：密码类型 `Debug` 打码、drop 时 zeroize（`crates/engine/src/io.rs:66`）。
- 恢复永不自动覆盖磁盘文件（Aline 已有同规）；恢复副本可逐文件丢弃；加密类操作前先清恢复副本。

### 4.4 对 Aline 的映射

Aline 的攻击面比 WordCraft 小得多（无 zip、无图片包、无宏），真正的面是：`decode`（UTF-16 拒收已有）、三个预处理改写器、pulldown、HTML 标签栈与实体解码、`html_block_text`、图片解码（`PictureAsset`：本地/`file:`/`data:`）、syntect 高亮。其中**图片解码是唯一真正的解压炸弹面**（恶意 PNG 声明巨大尺寸），Aline 计划里没有预算条款。上限表、敌意语料测试、panic 兜底见 §5.4。

---

## 5. 方案：让 Aline 更快、更强、保真、更安全、更好用

编号 `W1–W24`（WordCraft 借鉴）。每条含【证据】【落点】【验收】【决策需求】。优先级：**P0** = M0–M2 引擎地基期必须落；**P1** = 随对应里程碑；**P2** = M6 或之后。落地时按 architecture-guide §18.2 在 `decisions.md` 记录并回写管辖文档；未标注"需决策"的直接并入既有条款的修订。

### 5.1 更快

**W1（P0）自研扫描 pass 的反退化三件套：预算、索引、失败记忆化**
【证据】§2.3：scan_budget（`inlines.rs:220`）、Backticks run 索引（`inlines.rs:85-109`）、known 失败记忆化（`inlines.rs:45-80`）、`last_text`（`inlines.rs:536-537`）。
【落点】pulldown 本身线性且久经沙场，风险全在自研部分，逐个过一遍：`pre/flow.rs`、`pre/tex.rs`、`pre/front.rs`（改写器扫描）、`markdown_hl.rs`（逐行着色，凡需找 code span 闭合处用 run 索引；凡前瞻搜索处定预算上限）、`tree/html_block.rs` 的 `html_block_text`（`</tag>` 搜索窗口截断，学 `within()` 不切字符边界）、`tree/html.rs` 嵌套修复、`parse.rs` 的 `alike`。
【验收】每个 pass 配两条测试：`linear(source)` 渐进守卫（4 倍输入 ≤ 8 倍耗时，architecture-guide §17.3 已有形状）+ 敌意语料（全篇未闭合 `[`/巨量嵌套 `*`/超长实体，断言线性时间与正确降级为文本）。
【决策需求】无，属 §12.2 的自然延伸；修订 architecture-guide §12 追加一条"自研扫描 pass 必须有预算或索引佐证 + linear 守卫测试"。

**W2（P0）基准语料进 CI，把"测量文化"变成门禁**
【证据】§3.2：wordcraft 每个性能结论都有日期与数字（1.4ms/188 页；#13 的 4s→0.x s）。
【落点】v1-architecture §11 已列基线 bench（解析 ≤50ms/128KiB、commit 分档、alike <5ms），补三个场景：**1 MiB 单块**（§2.14 已知限制的盯防）、**一次输入只重绘一次**（architecture-guide §11.1 的 `draws_once`）、**滚动 1 MiB 文档帧时间**。
【验收】bench 进 CI，回归 >10% 判失败（§11.2 已有条款，本条是把它从"计划"变"已建"）。
【决策需求】无。

**W3（P1）崩溃恢复槽位写移出 UI 线程：worker + 每帧收割**
【证据】§3.3：`recovery.rs:25-64` 的 worker 线程 + `recovery_tick` 每帧收割 + wake 回调模式。
【落点】v1-architecture §3 只写"每 10s 写槽位"，未指定线程。恢复副本是全文写盘（可能 1–4 MiB），在 UI 线程就是周期性卡帧。
【验收】gpui 测试：模拟 dirty 文档 + 恢复写入期间打字，帧时间不超预算；槽位内容与 flush 后文本逐字节一致（既有 flush 语义测试不变）。
【决策需求】无，修订 v1-architecture §3 补一句"槽位写在后台线程，结果经代次校验回主线程（architecture-guide §3.3）"。

**W4（P1）大文件/网络盘读取防阻塞**
【证据】§3.3：wordcraft 引擎用 `O_NONBLOCK` 检查式读取（#3666），网络盘不卡 UI。
【落点】`aline-source::decode`。V1 简化版：`open` 动作全部放后台任务 + 代次校验（≥`AT_ONCE` 的文件本来就该异步打开）；`O_NONBLOCK` 预检列为 V2 增强（涉及 libc，避免 V1 引依赖争议）。
【验收】FakeFs 测试：慢速读取期间 UI 可交互、打开完成后 `buffer.stamp()` 校验。
【决策需求】无。

**W5（P2）块渲染测量缓存的佐证**
【证据】§3.2：wordcraft layout cache + 每页光栅缓存；Aline 对应物是 §7.1 缓存键 = 内容 hash + 环境的 shape 缓存。
【落点】无新工作——architecture-guide §7.1 已定案；此条仅把 wordcraft 实测收益（页级缓存使 CPU 光栅可行）记入 §11 作为正面佐证，供实现时决定缓存深度（文本行 shape 至少要缓存）。
【决策需求】无。

### 5.2 更强

**W6（P0）降级账本（Dropped 模式）**
【证据】§2.5：`Dropped` 枚举 + 每类每文档一条 warn + 消息含上限值与保留策略（"links longer than 8192 bytes are dropped; their text is kept"）。
【落点】Aline 的既有降级点统一收口：高亮 >64KiB 降级（v1-architecture §7.2）、图片加载失败占位、改写器跳过、单块超限降级。形状：`DocCore` 持有本轮"降级事件"去重集合（注意：wordcraft 用 thread-local 全局，Aline 铁律禁全局状态——改为随解析会话传递的显式结构），UI 汇总为状态栏一次性提示（i18n key `ui.degraded.*`），log 逐条。
【验收】测试：构造超限输入，断言"结果降级 + 账本恰好一条该类事件 + 文档其余部分不受影响"。
【决策需求】无；修订 v1-architecture §7.2 与 architecture-guide §16.2 追加"降级必须留痕且按类去重"。

**W7（P1）`hostile_params_never_panic` 的 Aline 版**
【证据】§1.1：wordcraft 把全部 422 条命令灌垃圾参数跑一遍（`docs/architecture.md:55-56`）。
【落点】`aline-edit::run` 是纯函数（v1-architecture §6.3），天然好测：对每个 `Command` 变体生成越界区间、空选区、巨串、跨块、非法 UTF-8 边界等参数，断言返回 `Result`（`Ok` 或 `EditRefusal`），永不 panic。
【验收】测试循环穷举 Command 变体（新命令忘加即测试漏覆盖——用 `strum` 或手写穷尽 match 保证）；进 M3 命令框架门禁。
【决策需求】无；写进 v1-architecture §12 功能组 4 门禁行。

**W8（P1）命令层 panic 兜底：catch + 快照恢复**
【证据】§3.1：`Session::run` 捕获逃逸 panic 恢复文档（never-crash 标准）；`EditSnapshot` 在危险命令前拍照（含选区、undo 栈头、IME 态）。
【落点】Aline 铁律 6 是"引擎 crate 用户输入永不 panic"——本条不削弱它，是在 **UI 编排层**加最后一道保险：`DocCore::apply` 前持有 `text_before`（§6.3 的 reread 已有同款），命令应用包 `catch_unwind`，panic 时回滚 buffer 到 `text_before`、丢弃 open 块、发 `EditRefusal::Internal` → 错误面板 + log。
【验收】测试：注入 panic 的假命令，断言文本逐字节不变、undo 栈不半途、错误面板出现。
【决策需求】**需决策**（与铁律 6 的边界表述相关：铁律管引擎 crate 的确定性，UI 层兜底是编排责任）——记 `decisions.md` 一条，回写 architecture-guide §16.3。

**W9（P2）打开时内容嗅探，拒收二进制**
【证据】§4.1：wordcraft 按签名而非扩展名判定（#2853/#569），二进制拒收并给出人话。
【落点】`aline-source::decode`：UTF-8 解码失败即明确报错（已有），补一步：文件头为已知二进制签名（`%PDF`、`PK\x03\x04`、`\x89PNG` 等）时给专属错误文案（"这是二进制文件"而非"无效 UTF-8"）。
【验收】测试：`.md` 后缀的 docx/png 文件打开 → 专属错误；i18n key 落 `engine.*`。
【决策需求】无。

### 5.3 更保真

**W10（P0）保真测试地板棘轮**
【证据】§4.1："a floor that only rises"（`docs/architecture.md:58-59`）。
【落点】engine-design §2.11 的保真套件（未编辑逐字节、BOM/CRLF/孤立 CR/无尾换行/模式切换零字节/保存-加载-保存幂等/open flush/undo 回保存点 dirty=false）建立后，加一个**清单持有测试**：用例名列表文件 + 测试断言每个名字都存在于套件中；删用例必改清单 → review 可见。
【验收】CI：清单测试绿 = 地板未降。
【决策需求】无；写进 v1-architecture §12 功能组 1 门禁行。

**W11（P1）`atomic_write` 补全语义：权限保留、独占创建、失败不碰原文件**
【证据】§3.3：序列化先行；临时文件 `O_EXCL` 独占；Unix `0600` 起步、替换时继承原文件 mode；失败绝不 in-place；无 mode 的挂载容错（#3003）；macOS 隐私拒绝时文案指引另存为。
【落点】v1-architecture §3 的 `atomic_write` 当前只写"同目录临时文件 → fsync → rename；失败留 .tmp"。补全为：① 编码（`Source::encode`）完成并拿到全部字节后才触碰文件系统；② 临时文件独占创建；③ 目标存在 → 继承其权限，新建 → 默认 mode 减 umask；④ rename 失败/写失败 → 删临时文件、原文件字节不动；⑤ 设权限失败分两类：不支持（NAS）继续保存，其他错误中止。
【验收】测试（wordcraft `safe_save.rs` 同款场景表）：占用名、符号链接、只读目录、无 mode 文件系统（FakeFs 模拟）、连续两次保存竞争；断言失败路径原文件逐字节不变。
【决策需求】无；直接修订 v1-architecture §3 + engine-design §2.9。

**W12（P2）读回恒等转义规则表（formatter 决策的输入）**
【证据】§2.4：`link_target`"读回恒等"判据、`code_span` 围栏 = 最长 run + 1（含 `.min(64)` 历史 bug）、`escape_line_start` 行首注入防护与 3 空格缩进上限、空白移出强调。
【落点】v1-features formatter 待定项（决策截止 M3）：若立项，规格测试直接以本规则表为金样；若不立项，此表留档备用（未来任何"生成 markdown 文本"的功能——导出、复制为 markdown、链接弹窗生成 URL——同样适用）。
【决策需求】随 formatter 决策一并落。

**W13（P2）"不认识即保留"先例锚点**
【证据】§2.2：`take_definitions` 的块起点规则与围栏同源判定；脚注定义留正文；注释明言"误删 = 静默丢失"。
【落点】无新工作——Aline 不变量 3（非规范语法就地保留）与拒绝制已覆盖；把 wordcraft 的六个边界场景测试（`markdown/tests.rs:87-115`）改写为 Aline 改写器测试的补充用例（改写器跳过代码的姊妹场景："定义形行在段落中间"）。
【决策需求】无。

### 5.4 更安全

**W14（P0）markdown 面上限表（写进文档的常量）**
【证据】§2.3/§4.2：MAX_NEST 32、MAX_TAG 4096（`within()` 截断）、实体窗口 33 字节、MAX_LINK 8 KiB、MAX_FONT_NAME 256、MAX_RUN_STRINGS 64 MiB；sheet/doc 的每维预算表。
【落点】Aline 自研面的对应常量表（`pub const`，单一来源，architecture-guide §3.4）：HTML 行内标签栈深度上限（§6.7 嵌套修复——嵌套修复算法本身要有深度断言）、HTML 块标签搜索窗口、`html_block_text` 处理的单块字节上限（超限降级纯文本显示）、实体名长（已定 32 字节，D-2026-10-26-13）、`data:` URI 大小上限（图片解码前）。每个常量旁注释威胁模型。
【验收】敌意语料测试逐常量一条（超限 → 降级 + 账本，§5.2-W6）。
【决策需求】无；修订 architecture-guide §12 追加"上限表"条款。

**W15（P0）图片解码预算（唯一的解压炸弹面）**
【证据】§4.2：wordcraft DIB 处理——字节上限同时用作解码器分配上限（#1334/#1554 相关）；图片解码共享预算、别名条目解码一次。
【落点】`render/assets.rs` 的 `PictureAsset`：`image` crate 解码前按文件头读取声明尺寸，超限（如单边 > 8192 px 或解压后 > 256 MiB）拒收 → 占位条 + 账本；`image::Limits` 显式设置；同 hash 只解码一次（内容哈希缓存天然覆盖）。
【验收】测试：构造声明超大尺寸的最小 PNG（可手工造），断言拒收且不 OOM。
【决策需求】无。

**W16（P1）敌意语料固定集 + proptest**
【证据】§1.1/§4.2：821 测试含 hostile/malformed 专项（docx `malformed.rs` 8 项、`hostile_params_never_panic`）；markdown 测试对边界场景的穷举风格（`markdown/tests.rs:87-115`）。
【落点】`aline-markdown/tests/`：固定敌意语料文件（截断 UTF-8、裸控制字符、10k 层嵌套、超长单行、BOM+混合行尾、伪表格、未闭合围栏……）逐个喂 `decode → 改写器 → pulldown → 展示树` 全管线，断言不变量（块区间拼接 == 全文、UTF-8 合法、无 panic）；proptest 随机字节串同断言。已有计划（engine-design §2.11 第 2/8 条）的基础上，把 wordcraft 语料风格并入。
【验收】进 M0 门禁。
【决策需求】无。

**W17（P1）恢复副本的编码成本校准**
【证据】§3.1 #586：字节表示使恢复副本放大 6 倍（27 MB → 99 MB）。
【落点】Aline 恢复槽写纯 markdown 文本（无媒体），天然安全；此条是把教训写成验收：恢复槽写入耗时按 4 MiB 文档（`LARGEST`）校准 < 10ms（后台线程，W3），槽位大小 ≈ 文本大小。
【验收】bench 一条。
【决策需求】无。

### 5.5 更好用

**W18（P1）崩溃恢复的逐文件选择与明确丢弃**
【证据】§3.3：`recovery_documents()` 列表、`discard_recovery_copy` 逐个丢弃、`recovery_wait/recovery_ready/recovery_busy` 状态细分（后台恢复的 UI 状态反馈）。
【落点】v1-architecture §3 的"下次启动发现槽位 → 提示恢复"细化为：列表展示（文件名 + 保存时间 + 与磁盘版本的差异提示）→ 逐项恢复/丢弃/全部丢弃；恢复动作进行中显示状态而非冻结。
【验收】gpui 集成测试：崩溃模拟（写槽位后强退）→ 重启 → 列表 → 恢复内容只进内存。
【决策需求】无；修订 v1-architecture §3。

**W19（P2）保存失败的"给出口"文案**
【证据】§3.3：macOS 隐私保护/只读目录场景，错误文案指名"用另存为到其他目录，或给 App 授权"（#2138）。
【落点】保存错误 → toast/对话框按错误类别给行动指引（i18n key `engine.save_failed_readonly` 等）；联动 SaveIntent 三选（v1-architecture §15.1）。
【决策需求】无。

**W20（P2）默认应用注册礼仪：永不自动抢默认**
【证据】§1.1 对照：wordcraft 安装器只注册"打开方式"候选，抢默认必须用户在设置页点显式按钮（`docs/file-format-parity.md:144` 表）。
【落点】v1-architecture §2.2 的三平台注册任务补一句交互约定：注册为候选 handler，设置页提供"设为默认 .md 应用"按钮。
【决策需求】无。

**W21（P2）测试即截图（ui_shot 模式）**
【证据】§1.1：README 全部截图由 `cargo run -p wordcraft-ui-egui --example ui_shot` 离屏渲染自产（`README.md:37`）。
【落点】Aline 已有 `ALINE_SMOKE` 钩子（v1-architecture §15.4）；扩展一个 `--screenshot` 输出 PNG 的示例，README 首图自产 + 外观零位移测试（1px 断言）可复用其渲染管线。
【决策需求】无；M6 收尾项。

### 5.6 明确不采纳（防止过度工程）

| WordCraft 做法 | 不采纳理由 |
|---|---|
| 手写 markdown 解析器（§2.1） | 铁律 5：锁定 pulldown-cmark；它能手写恰因无字节保真负担 |
| Arc 快照式 undo（§3.1） | Aline 补丁式 O(编辑量) 更优；wordcraft 自己也需 `SNAPSHOT_HEAD=4` 控栈深 |
| 保真百分比 × 权重体系（§4.1） | Aline 目标是字节级恒等，"60% 保真"没有定义；只取地板棘轮方法（W10） |
| 422 命令 + MCP + CLI 全家桶（§1.1） | V1 范围外；Command 单表架构已预留同款扩展路径 |
| egui 即时模式的具体对策（页光栅等） | GPUI 保留模式渲染管线不同；只取"缓存布局结果"的通用结论（W5） |
| thread-local 全局（Dropped 位图、REPORTED） | 铁律 6：引擎 crate 零全局状态；改随会话传递（W6 已注明） |

### 5.7 采纳路径与里程碑归位

| 提案 | 优先级 | 归位里程碑 | 落点文档 |
|---|---|---|---|
| W1 反退化三件套 | P0 | M0（改写器/markdown_hl 实现期） | architecture-guide §12 新增条款 |
| W2 基准进 CI | P0 | M1 首个 bench 起持续 | v1-architecture §11 |
| W10 保真棘轮 | P0 | M2（保真套件建成时） | v1-architecture §12 |
| W14 上限表 | P0 | M0（HTML/改写器实现期） | architecture-guide §12 |
| W15 图片解码预算 | P0 | M4（资产管线） | v1-architecture §7.2 |
| W6 降级账本 | P0 | M2 起随各降级点 | v1-architecture §7.2、architecture-guide §16.2 |
| W3 恢复线程化 | P1 | M5（崩溃恢复收尾） | v1-architecture §3 |
| W4 读取防阻塞 | P1 | M1（文件层） | v1-architecture §3 |
| W7 命令注入测试 | P1 | M3（命令框架） | v1-architecture §12 |
| W8 panic 兜底 | P1 | M2（需先决策） | decisions.md 新条目 |
| W11 atomic_write 补全 | P1 | M1（文件层） | v1-architecture §3、engine-design §2.9 |
| W14/W16 敌意语料 | P1 | M0 门禁 | aline-markdown tests |
| W17 恢复副本校准 | P1 | M5 | bench |
| W18 恢复 UX | P1 | M5 | v1-architecture §3 |
| W9 内容嗅探 | P2 | M1 | aline-source |
| W12 转义规则表 | P2 | M3 formatter 决策时 | v1-features、decisions.md |
| W13 先例锚点用例 | P2 | M0 测试期 | aline-markdown tests |
| W19 保存失败文案 | P2 | M5 | i18n catalog |
| W20 默认应用礼仪 | P2 | M5 打包 | v1-architecture §2.2 |
| W21 ui_shot | P2 | M6 | ALINE_SMOKE 扩展 |

三条需先落决策记录的：**W8**（UI 层 panic 兜底与铁律 6 的边界）、**W12**（随 formatter 立项裁决）、以及 W11 若 V1 决定不继承权限保留语义（建议继承，成本一次函数）。

---

## 附录 A：WordCraft 证据索引

| 主题 | 位置 |
|---|---|
| 分层/测试/门禁总览 | `docs/architecture.md:11-59` |
| markdown 模块自述 | `crates/formats/src/markdown.rs:1-10` |
| MAX_NEST / 块解析 / 列表 / 表格 | `crates/formats/src/markdown/blocks.rs:5,142-152,274-306,389-673` |
| 定义摘除的块起点规则 | `crates/formats/src/markdown/blocks.rs:92-139`；测试 `markdown/tests.rs:87-115` |
| Backticks 索引 / known 记忆化 / scan_budget | `crates/formats/src/markdown/inlines.rs:85-109,45-80,220,297-309` |
| emit 强调算法 / bottom 数组 / last_text | `crates/formats/src/markdown/inlines.rs:440-570` |
| MAX_TAG / within 截断 | `crates/formats/src/markdown/inlines.rs:419-429` |
| 导出转义纪律 | `crates/formats/src/markdown/export.rs:9-56,66-81,151,201-221,325-334,364-366` |
| 上限常量 / Dropped 账本 | `crates/formats/src/model/types.rs:9-84` |
| Arc 写时复制编辑 | `crates/doc/src/document.rs:104,229,249` |
| EditSnapshot / SNAPSHOT_HEAD | `crates/engine/src/snapshot.rs:9-40` |
| 恢复 worker + tick | `crates/engine/src/recovery.rs:25-64,169` |
| 原子保存细节 | `docs/file-format-parity.md:180-183`；`crates/engine/src/io.rs:776` |
| 密码打码/zeroize | `crates/engine/src/io.rs:66` |
| 媒体 base64（#586） | `crates/doc/src/byte_store.rs:1-8` |
| 预算全家桶（sheet/doc/zip） | `docs/file-format-parity.md:52,63,124` |
| #13 保存性能案例 | `docs/file-format-parity.md:130-134` |
| 保真方法论/透传/棘轮 | `docs/file-format-parity.md:10-13,114,122,186-193`；`docs/architecture.md:58-59` |
| 增量重排实测（1.4ms/188 页） | `docs/architecture.md:45-46` |
| 版本与规模 | 根 `Cargo.toml`（0.11.0）；`docs/architecture.md:11` |
~~~~~~~~
