# quiet-md 工程实践调研与 Aline 采纳方案

更新日期：2026-11-05 · 修订：2026-11-06（事实与来源校正） · 状态：调研与独立复核完成；工程/验证方法已按[决定 QMD-01](./decisions.md#25-2026-11-05-quiet-md-工程实践采纳qmd-01)回写现行规范，治理 delta 已并入 OpenSpec change [`m1-single-window-readonly`](../openspec/changes/m1-single-window-readonly/proposal.md)，W1～W7 已于 2026-11-08 分别建独立 change 并按用户指令继续实施，进度见[开发流程 §3.1](./development-process.md#31-研究采纳与工程实践落点2026-11-05)；本文调研本身不提供实现证据

研究交付（不含后续独立 change 的实现）：quiet-md 快照核对、独立复核、采纳矩阵、W0～W7 工作流、证据与决策门。无代码、依赖或实验产物。复核记录为调研过程，不构成独立入库证据。

**目标：** 把 quiet-md 中可迁移的工程实践落成 Aline 的可执行工作流，先补 M1 证据与引擎不变量，再服务 M2 保存/IME 与 M4 主题。

**方案架构：** quiet-md 只提供方法、失败样本与取舍记录；Aline 按“源文本唯一真值 + 三坐标映射 + 单区间补丁”重写，所有 W 项先过 OpenSpec/决定门。

**技术栈：** Rust 2024、gpui-pre 0.3.9、pulldown-cmark、现有八 crate；不新增生产依赖，测试依赖须走准入。

**依据：** [AGENTS.md](../AGENTS.md)、[PRD](./v1-prd.md)、[架构](./v1-architecture.md)、[验证计划](./v1-validation-plan.md)、[决策记录](./decisions.md)。

**全局约束：** 源文本唯一真值；引擎 crate 零 GPUI；引擎禁 unwrap/expect/panic/threads/global/unsafe；仅 pulldown-cmark；用户可见串走 `t("key")`；文档/证据不写绝对本地路径；原始输出只进未跟踪 `logs/validation/`；依赖身份/feature/边以 `scripts/*policy.json` 为准；验证只验中文（决定 §24）。

**复核重点：** 见 §3.4。

## 1. 用途、效力与处理状态

本文是参考研究与采纳方案，**不能覆盖** Aline 当前产品契约、架构铁律和技术路线。权威顺序仍是：

[AGENTS.md](../AGENTS.md) 强制约束 → [PRD](./v1-prd.md) 已确定范围/行为 → 已确认的架构、引擎与专项规范 → [验证计划](./v1-validation-plan.md) → [决策记录](./decisions.md) → 本文与其它研究笔记。

已采纳的规则以对应 Aline 规范为准；引用当前规范只能证明已有承接，不能证明 quiet-md 是决定来源，也不表示已经实现或验收。本文所有工作流默认是**建议采纳，待确认**；2026-11-05 决定 QMD-01 已把其中不改变产品契约的工程/验证方法回写现行规范，但每个 W 项的具体实施仍须按 W0 建 OpenSpec change。

| 状态 | 本文含义 |
|---|---|
| 采纳并改造 | 原则可迁移，但所有者、算法、数值或验收须按 Aline 约束重写 |
| 建议采纳，待确认 | 有明确收益的新增工作，尚未有决定依据 |
| 待验证 | 已授权方向下的能力/数值实验，结果可能为否 |
| 不采纳 | 与 Aline 强制约束或明确范围冲突，给出依据 |
| 仅供参考 | 工程先例或候选实现，不形成新规定 |
| Aline 已有 | 当前仓库已具备同等或更强机制，不需要引入 |

一条工作流可以包含不同状态的子项；主状态不能把候选算法、数值或新增 UI 一起升级为决定。

## 2. 调研对象、快照与证据限制

- 对象：`quiet_md` 0.1.5，本地快照 HEAD `7f269750a9c5689eb879dcb8e8a1a8b0e5a8e6a5`，工作树干净。
- 时间与方式：2026-11-05 只读复核；未运行 quiet-md 的测试、基准或 GUI，未核验远程 issue/CI，未修改任何文件。
- 许可：应用 MIT；`vendor/gpui` 为 Apache-2.0 的 gpui 0.2.2，`vendor/taffy` 为 0.9.0；两者都有本地补丁。
- 用户数据：quiet-md 默认文档库是真实用户数据；本次未读取、未修改、未启动应用。
- 证据分层：**直接观察**指本轮实际读取的源码/文档；**推论**是基于机制的判断；**Aline 建议**不自动成为决定。下文路径除特别说明外，均相对 quiet-md 快照根目录；Aline 路径相对本仓库根目录。

| 索引 | 范围与定位 |
|---|---|
| E01～E12 | 统一见 [§13 证据索引](#13-证据索引)；本节只记录快照与证据分层，不重复路径 |

## 3. 结论摘要

### 3.1 值得采用的五项

1. **离屏视觉证据**：gpui-pre 0.3.9 已提供真实 macOS 渲染的离屏窗口、模拟键鼠与截图接口，可作为 M1 真实窗口证据的补充，但必须先做能力 spike。
2. **引擎 fuzzer 方法**：随机操作 + 每步不变量 + 周期往返 + 失败保留 seed/step/操作日志；quiet-md 不保存最小输入、也不做最小化，Aline 另要求最小输入；quiet-md 用自写 xorshift，不需要新依赖。
3. **保存故障安全骨架**：临时文件→`sync_all`→`rename`、写前磁盘基线检查、失败保留未保存状态；按 Aline 的 SAFE-04/05/07/08 重写，不照搬 500ms 与 `.bak`。
4. **性能报告写法**：环境、源码、方法、P50/P95/最大值、原始日志、复现命令、明确的“不测量什么”；Aline 另补冷/热、样本 SHA-256、峰值内存等字段，直接补强 V-05。
5. **主题分层方向**：原始色阶 + 语义 token + 来源注释 + 覆盖测试；quiet-md 只部分做到（十六进制未完全集中），Aline 按设计要求补齐；不搬 quiet-md 的色值或自动暗色算法。

### 3.2 必须改造后才能采用的四项

- **撤销**：quiet-md 是整篇 `Document` 快照；Aline 使用补丁/历史组，不能复制其内存模型。
- **自动保存**：quiet-md 是 500ms；Aline SAFE-04 是停止输入 1 秒；恢复快照（SAFE-08，10 秒）与自动保存是两件事。
- **虚拟化缓存失效**：quiet-md 的 `hash_block` 对文本只取长度；Aline 已有 `LayoutKey`/映射/版本身份，不能退化为长度键。
- **本地化**：quiet-md 以中文原文为 key；Aline 已用 `t("key")` catalog，不能反向迁移。

### 3.3 明确不采用的项

- 文档模型（block 树、DocxXML、行内 U+FFFC）；DocxXML 是存储真值，Markdown 不是存储格式（还用于导入与粘贴解析）。
- 整篇 `Document` 快照撤销、结构操作全量 clone。
- 文本布局 key 只算长度、依赖“可见行每帧重测”兜底。
- 双主题双渲染、全局 bool 分派、视图层绕过 Core 直接改结构。
- 整体 vendor GPUI + 15 个补丁 + AppKit 视图注入。
- 主线程同步 fsync、窗口激活时全库重解析。
- 测试主动裁掉已知丢失（如单元格尾随空格）再宣称往返通过。
- 固定 30 天回收站、默认开启远程图片/自动更新/全局热键等产品默认值。

### 3.4 复核重点（Review Focus）

以下五类失败模式最可能在采纳过程中造成“看起来完成、实际不可用”，每个工作流都必须有对应测试或人工证据：

1. **把离屏截图当成真实窗口证据**：离屏渲染不证明窗口 chrome、焦点、IME、锁屏、多显示器或平台矩阵。
2. **中文避头尾补丁与上游升级脱节**：vendor 三个 gpui-pre 包后，升级会静默丢补丁或改变字形/布局。
3. **fuzzer 把真实损失规范化掉**：测试辅助函数不得掩盖源码字节变化、映射丢失或历史不一致。
4. **自动保存/恢复/外改互相覆盖**：不得用自动保存替代 10 秒恢复快照，不得在磁盘已变时静默覆盖。
5. **token 提取造成视觉漂移**：语义 token 只能重命名现有值，不能在重构中顺手改色或把候选数值冻结。

## 4. quiet-md 实现盘点

### 4.1 分层与编辑协议

quiet-md 是单进程 GPUI 应用：`crates/core`（`fdl_core`，约 1 万行）不依赖 GPUI，根 crate 是窗口、视图、渲染与菜单。一次编辑固定为：

`checkpoint(kind)` → Core 操作 → `Editor::changed()` → `Workspace::schedule_save`。

`checkpoint` 递增 `generation`、置 dirty、按条件记撤销快照；`changed` 做 `reveal_caret` 并发事件。这个“单一改动入口 + 派生缓存按 generation 失效”的纪律值得采用。

但文档自认存在**直接修改例外**：图片尺寸/裁剪、倒计时、代码块名、图标/封面、标题重命名、勾选/折叠等通过 `core.block_mut(id)` 在视图层直接写；`src/editor/image.rs` 还有直接 `core.doc.blocks.extend` 的结构修改。这是“单一写入口”被例外掏空的反例，不应复制。

### 4.2 渲染与虚拟化

页面是 GPUI `List`，条目为标题区、每个可见顶层 block、尾部点击区。`sync_items` 生成 item key，比较前后缀后只 `splice` 变化区间；`hash_block` 对文本只取长度，理由是百万字全文哈希每次按键约多 3ms，而可见行每帧会重测，屏幕外高度最多短暂过期。

这是明确的性能取舍，但它把正确性押在“可见行会重测”上；代码块 caption、Callout 颜色等字段也未进 key。Aline 的虚拟化已有 `LayoutKey`、映射身份、文档/版本/快照/投影/配置/模式/环境键，不能采用长度键。

### 4.3 撤销

- 每步是整篇 `Document` 快照，`VecDeque<Arc<Snapshot>>`；上限 500 步、约 256MiB、至少 20 步。
- 合并条件：同一 `EditKind`、非 `Other`、同一 block、距上次 <700ms；公式编辑不受 700ms 限制。
- 撤销/重做保留当前 icon 和折叠状态，不随历史回滚。

简单可靠，但结构操作每次 clone 全文，内存与长文档成本线性增长；这不适合 Aline 的补丁/历史组协议。

### 4.4 保存、恢复与外改

- `write_atomic`：同目录临时文件 → `write_all` → `sync_all` → `rename`；没有父目录 fsync。
- 文档保存前检查磁盘内容：等于上次已知磁盘内容（加载或上次保存）或等于本次新内容才写；否则拒绝覆盖。没有可用基线时冲突检查可能不执行。
- `.bak` 与 `.corrupt-*` **只用于 JSON 索引/回收站索引**，不用于 DocxXML 文档。
- 多实例：对 `.lock` 加咨询锁并重读索引；不是“合并文档”，文档冲突直接拒绝写。锁打开/加锁失败时保存仍继续写，重读失败沿用内存索引，因此不是完整的多实例保护。
- 自动保存：500ms 防抖；切换/退出先静默 flush，切换、关窗或 ⌘Q 失败才弹“重试/放弃修改”，窗口失活与系统注销失败不弹框；没有独立的 10 秒恢复快照。
- 保存是主线程同步写盘；`flush` 在写盘前对全篇做 `plain_text`/`reminders`/`doc_links` 派生，窗口每次激活都会重解析全库。

可迁移的是故障安全骨架与失败语义；不可迁移的是 500ms、主线程同步、JSON `.bak` 与“冲突后只能导出”的产品处理。

### 4.5 测试与证据

- Core 有单元测试与随机编辑 fuzzer；fuzzer 用手写 xorshift64*，默认 40 seeds × 300 步，每步检查模型不变量，每 25 步做存盘往返，失败打印 seed、步号、最近 12 步与整篇 XML；支持 `FDL_FUZZ_SEED/SEEDS/STEPS/TRACE`。
- 顶层随机分支实际 20 个（`rng.below(23)` 只是上界），生成块种类有限，不生成图片/文件/倒计时/Raw；round-trip 前会裁掉表格单元格中嵌套块前的空格/制表符。
- view/render/overlays/app/image/table/drag 等关键 UI 模块没有单元测试；UI 正确性主要靠手工脚本截图与像素对比；CI 只在 macOS 跑测试，Windows 仅编译。
- 这是“方法值得学、覆盖与规范化不可学”的典型。

### 4.6 UI、主题与本地化

- 主题分三层：原始色阶（`craft`/飞书）、语义 token、文档主题 CSS 覆盖；方向正确，但十六进制并未完全集中，逐行一致性测试只覆盖代码生成的两个主题。
- 双主题双渲染：约 600 行 `_feishu` 复制渲染，靠全局 bool 逐块分派；部分主题模块/字段仍带 `allow(dead_code)`，但模块实际在用，这是残留抑制而非死代码。这是反例。
- 本地化以中文原文为 key，`tr(zh,en)` 查英文；有源码扫描测试与用户内容豁免；没有 RTL/复数/数字格式。Aline 的 key catalog 更强，不应回退。
- 未发现 NSAccessibility 实现；无障碍是 quiet-md 的明显空白，不能作为参考。

### 4.7 vendor 与平台

- `vendor/gpui` 是 gpui 0.2.2 + 15 个补丁，`vendor/taffy` 是 0.9.0 + 1 个补丁；升级要手工重打。
- 补丁中与中文最相关的是：字体平滑、光学尺寸边界、KaTeX 字体放行、中文避头尾、显式行高、合成斜体、PostScript 重名（苹方）、离屏渲染。
- 平台能力大量 `cfg` 静默降级：热键/打印仅 macOS，Windows 用 `osascript` 的通知会静默失效；还有按类名注入 `NSGlassEffectView` 的原生视图层级。这些都不应复制。

### 4.8 文档与工程纪律

- `AGENTS.md` 与 `CLAUDE.md` 内容重复且自认“两份一起改”，但已经出现表述过宽/失准；单一来源比“两份同步”可靠。
- 明确不跑 rustfmt，靠周围风格；clippy 允许 7 项，包括 `type_complexity`/`too_many_arguments`/`large_enum_variant`。Aline 的 fmt + 严格 lint 不应放弃。
- `THIRD_PARTY.md` 自认不替代各组件许可文本；图标、字体、主题命名有额外许可限制。复用代码或资源必须单独做法务核对。

## 5. 与 Aline 的根本差异

| 维度 | quiet-md | Aline | 可迁移性 |
|---|---|---|---|
| 真值 | DocxXML block 树；Markdown 不是存储格式，用于导入/导出与粘贴解析 | Markdown 源文本是唯一真值 | 文档模型不可迁移 |
| 坐标 | 块内 UTF-8 字节 + UTF-16 平台边界 | `src_off` / `drawn_off` / `utf16_off` 三坐标 + 映射层 | 只可借鉴边界测试 |
| 编辑单位 | 整个文档树，最多一个块打开 | 最多一个打开块、单区间补丁 | 撤销/保存不可照搬 |
| 历史 | 整篇快照 | 补丁/历史组/保存点身份 | 方法可借鉴，模型不可 |
| 解析 | 自有 DocxXML；Markdown 导入 | pulldown-cmark 唯一解析器 | 不适用 |
| UI | vendor gpui 0.2.2 + 15 补丁 | 锁定 gpui-pre 0.3.9，无 vendor | 补丁只能按算法移植 |
| 本地化 | 中文原文作 key | `t("key")` 双语 catalog | Aline 更强 |
| 平台 | macOS 优先，Windows 部分静默降级 | 三平台目标 + 依赖准入 | 不学静默降级 |

结论：quiet-md 的价值在**小步交付、失败留证、协议固定、性能取舍写明**；不在其数据结构与产品语义。任何“看起来可以直接复制”的代码，先按上表判断是否跨过了真值/坐标/历史边界。

## 6. 独立复核记录

### 6.1 复核方式

本轮在首轮分析之后，另起多路无前序上下文的只读复核，各自独立读源码并给出文件:行号；复核为调研过程，不构成独立入库证据：

1. **事实核对**：逐条判定首轮结论“确认/部分正确/错误”，重点是夸大与遗漏。
2. **补充调研**：找架构分析容易忽略的工程实践（崩溃安全、窗口、搜索、无障碍、本地化、设置迁移、日志、打包、DX、产品细节）。
3. **文本/IME**：专看输入法、候选窗、UTF-16/字节/字素、命中与选区。
4. **反模式**：专找“看起来好、实际有隐藏代价”的做法，以及还债信号。

多路结果与首轮交叉后，下面这些说法被修正；方案以修正后的事实为准。

### 6.2 首轮结论修正

| 首轮说法 | 复核事实 | 对方案的影响 |
|---|---|---|
| fuzzer 有 23 类操作 | `rng.below(23)` 是上界；顶层 match 只有 20 个分支，细分命令更多 | 不把“23 类”写进覆盖结论；W3 明确按 Aline 语法重新设计生成器 |
| `.bak`/`.corrupt-*` 保护文档 | 只用于 JSON 索引/回收站；DocxXML 文档用原子写 + 基线检查 | W4 不把 `.bak` 当文档恢复方案 |
| 多实例会合并索引/文档 | 加锁后整体重读索引；文档冲突直接拒绝覆盖，没有合并 | W4 只借鉴“写前检查”，不宣称有多实例合并 |
| 切换/退出会提示未保存 | 先静默 flush；只有写盘失败才弹重试/放弃 | W4 的失败语义按“失败才阻塞”设计 |
| 撤销按通用 700ms 合并 | 还必须同 `EditKind`、非 `Other`、同 block；公式豁免 | 不把 700ms 当通用规则；Aline 仍是 900ms 输入组 |
| 主题十六进制已集中 | 多个 UI 模块仍有字面色；一致性测试只覆盖 craft/feishu | W6 用“先建 token + 守卫”而不是“已有集中” |
| `VisualTestAppContext` 可直接用于普通测试 | 受 macOS + `test-support` 门控；`MacPlatform` 在 gpui-pre-macos | W1 先做依赖准入与能力 spike，不假设开箱可用 |
| gpui-pre 缺字体平滑/光学边界/苹方去重 | 平滑与 font-kit 光栅边界已存在；苹方去重只在单次 `load_family` 内，PostScript 名仍是单值映射 | 这些降为“待验证”，不作为确定缺陷 |
| quiet-md 测试覆盖强 | 关键 UI 模块零单测；fuzzer 生成块有限；往返测试有已知丢失预处理 | W3 明确“不得用规范化掩盖损失” |
| vendor 是资产 | 15 个 GPUI 补丁 + 1 个 taffy 补丁，升级需手工重打 | W2 把 vendoring 设为高成本决策门 |

### 6.3 补充发现（首轮未覆盖）

- **崩溃安全**：原子写、坏 JSON 隔离、锁内重读、失败挂起动作形成完整损失路径；但文档没有独立的未保存缓冲区日志，防抖窗口内强杀仍会丢编辑。
- **窗口**：最小 480×480；只持久化尺寸，恢复时按显示器 clamp；关闭/退出才落盘；不存位置。
- **搜索**：真实 shaping 二分摘要、标题命中优先、替换后按长度差平移后续命中；小写改变字节长度时回退 raw find，偏移可能错。Aline 的单区间补丁可借鉴“后续偏移平移”，不能照抄其字节假设。
- **无障碍**：未发现 AX 树/Tab 焦点环/屏幕阅读器语义；这是空白，不是可借鉴项。
- **本地化**：源码扫描测试 + 用户内容豁免 + 诊断模板翻译值得学；中文 key 不值得学。
- **设置迁移**：新字段 `serde(default)`、`FORMAT` 版本更高只警告不写；损坏偏好静默回默认，无迁移链。Aline 保留 default + 版本标记，并另加隔离备份。
- **日志**：最先初始化、1MB 轮转、panic hook 强制 backtrace、每行 flush、Windows 用 MessageBox 显示日志路径；无脱敏/遥测。可借鉴“启动失败不静默”，不照搬实现。
- **打包**：`build.rs` 生成内嵌资源表、`--locked`、许可表生成、更新器验签/同 Team ID；Aline 只需 `--locked` + 许可生成 + 内嵌资源。
- **产品细节**：保存失败保留后续动作、删除前检查另一窗口是否在编辑、移动对话框隐藏自身子树；这些是产品语义，不自动成为 Aline 需求。

### 6.4 文本/IME 复核

quiet-md 的 IME 做法：`marked` 按块记录字节区间；`text_for_range`/`marked_text_range` 只服务头块；跨块选区上报折叠点；提交时把 marked 变选区再 `type_text`；候选窗用 `bounds_for_range` 取文本布局矩形。Aline 可借鉴的结构是“组合只属于一个打开块 + 投影层换算”，但有三处已知坑：

1. **失焦未显式 `unmarkText`**：quiet-md 只在 set_caret、鼠标、撤销时清理；未找到失焦清理路径，是否存在平台自动 unmark 未验证。Aline 必须在焦点丢失、块切换、窗口失活时显式取消/清理。
2. **候选窗忽略元素 frame**：`bounds_for_range` 只取文本布局坐标；滚动、内边距、虚拟列表位移会让候选窗偏移。Aline 必须把元素 frame、滚动偏移、窗口坐标一起换算。
3. **glyph 索引是字节不是字素**：在锁定 gpui-pre 0.3.9 的实现中，GPUI 的 glyph `index` 是 UTF-8 字节偏移；这不是上游文档契约，升级需回归。双击/光标定位可能落进组合符或 ZWJ emoji。Aline 的 UTF-16 端点与字素边界必须只经投影层，不得直接用 glyph 索引当字素索引。

另有两点值得直接纳入 M2 设计：组合确认单独成一步历史；组合取消不得写入草稿或磁盘。Aline 已有 `composition_start/update` 与延迟协议，应在 M2 接线时保留这些边界。

### 6.5 反模式复核

最值得记住的五条负向结论：

1. 单一写入口被“只改属性”的例外掏空，最终出现视图层 `blocks.extend` 改结构。
2. 共享几何靠上一帧 `Rc<RefCell<HashMap>>` 隐式协议，浮层按上一帧布局定位；这不是模块接口。
3. 关键视图零单测，把 UI 正确性交给手工截图和 macOS-only CI。
4. 性能取舍线性反噬：整篇快照、长度键、主线程 fsync、全库重解析，在更大文档/更慢盘/更大库上会放大。
5. 把 vendor 和原生注入当资产：15 个补丁、NSView 层级注入、跨平台 `cfg` 静默降级，都是长期维护债。

## 7. 采纳矩阵

状态缩写：**改**=采纳并改造；**议**=建议采纳待确认；**验**=待验证；**否**=不采纳；**参**=仅供参考；**有**=Aline 已有。

| 编号 | 实践 | quiet-md 证据 | Aline 承接 | 状态 | 工作流 |
|---|---|---|---|---|---|
| Q01 | 临时文件→sync→rename | E02 | SAFE-04/07、V-08 | 改 | W4 |
| Q02 | 写前磁盘基线检查 | E02 | SAFE-07 | 改 | W4 |
| Q03 | JSON `.bak`/坏文件隔离 | E02 | 无对应；恢复存储 | 否（文档层） | W4 |
| Q04 | `.lock` + 锁内重读 | E02 | M5 多实例 | 参 | W4 |
| Q05 | 500ms 自动保存 | E03 | SAFE-04 为 1s | 否（数值） | W4 |
| Q06 | 保存中/失败状态 | E03 | SAFE-05 | 改 | W4 |
| Q07 | 切换/退出静默 flush，失败才阻塞 | E03 | SAFE-04/07 | 改 | W4 |
| Q08 | 10 秒恢复快照 | 无 | SAFE-08 | 议（自建） | W4 |
| Q09 | 整篇快照撤销 | E04 | 补丁/历史组 | 否 | W3/W4 |
| Q10 | 700ms 合并 | E04 | 900ms 输入组 | 否（数值） | W4 |
| Q11 | 每步不变量 + 周期往返 | E05 | M0 测试体系 | 改 | W3 |
| Q12 | 手写 xorshift，无新依赖 | E05 | 无 dev-dependency | 议 | W3 |
| Q13 | 失败留 seed/step/操作日志 | E05 | 可复跑证据 | 改 | W3 |
| Q14 | `FDL_FUZZ_*` 环境变量 | E05 | 复跑入口 | 改 | W3 |
| Q15 | `hash_block` 文本只取长度 | E06 | `LayoutKey` 已更强 | 否 | W6 |
| Q16 | 列表高度估算/滚动条 | E07 | 已有 ListState | 参 | W1 |
| Q17 | 离屏窗口/模拟键鼠/截图 | E11 | V-09 证据补强 | 验 | W1 |
| Q18 | 截图像素对比 | E10 | 视觉回归 | 验 | W1 |
| Q19 | 中文避头尾 | E07/E09 | V-04 | 验 | W2 |
| Q20 | 合成斜体 14° | E07 | V-04 | 验 | W2 |
| Q21 | CJK 标点单独 PingFang run | E09 | V-04 | 验 | W2 |
| Q22 | 语义 token 分层 + 完整性测试 | E07 | V-04/M4 | 改 | W6 |
| Q23 | 性能报告字段/复现命令 | E10 | V-05 | 改 | W7 |
| Q24 | 本地化扫描测试 | 补充 | catalog 测试 | 改 | W7 |
| Q25 | 单一检查入口 | E10 | `check_workspace.py` | 有 | W7 |
| Q26 | AGENTS/CLAUDE 双份同步 | E10 | 单一来源 | 否 | W7 |
| Q27 | 不跑 rustfmt | E10 | fmt 门禁 | 否 | W7 |
| Q28 | vendor 补丁清单 | E07 | 若 W2 采用 | 议 | W2/W7 |
| Q29 | AppKit 视图注入 | E07 | 无 | 否 | W1/W7 |
| Q30 | 跨平台 osascript 静默降级 | E07 | 平台 trait | 否 | W5/W7 |
| Q31 | 无障碍语义 | 无 | UI 规范/M1 aria | 议（自建） | W7 |
| Q32 | 日志/panic hook | E08 | 启动失败可见 | 议 | W7 |

## 8. 工作流

每个工作流都不是一个巨型 PR；W0 要求按“可独立验收的交付物”拆成单独的 OpenSpec change。以下文件路径是拟新增/拟修改，实施前以对应 change 为准。

### W0 治理与决策门

**目标**：防止方案在没有决定记录的情况下变成事实规范。

- 把本文登记为参考研究，链接进 `docs/README.md` 的研究区；不把任何 W 项写成已批准。
- 每个工作流单独建 OpenSpec change，写清范围、非目标、验收证据与不做项；涉及产品行为/数值的，先入 `docs/decisions.md`。
- 依赖准入只通过 `scripts/engineering_policy.json` / `scripts/dependency_admission_policy.json` 与自测更新；禁止在 `Cargo.toml` 里静默加 feature。
- 任何复用 quiet-md 代码或移植 GPUI 补丁，先做许可/来源记录：quiet-md 应用是 MIT，GPUI 是 Apache-2.0；修改 Apache-2.0 文件要保留声明与修改说明。
- 证据规则沿用 AGENTS：原始输出只进未跟踪的 `logs/validation/`，文档不写绝对本地路径，不伪造截图/结果；离屏证据与真实窗口证据分开命名。

**验收**：`docs/README.md` 有本文链接；每个后续 change 有范围/非目标/证据表；依赖策略自测通过。

### W1 离屏视觉证据与截图回归

**目标**：判定 gpui-pre 0.3.9 的 `VisualTestAppContext` 能否在本机稳定渲染 Aline 根视图并截图；能则补 M1 证据，不能则记录阻塞，不硬推。

**为什么先做 spike**：gpui-pre 的离屏接口受 `#[cfg(all(target_os = "macos", any(test, feature = "test-support")))]` 门控；`test-support` 会带入 `proptest`、`futures-concurrency`、`postage`、`web-time` 与 `wayland`/`x11` feature。Aline 目前没有任何 dev-dependency，必须走准入。且 Rust 测试默认在工作线程跑，macOS/GPUI 可能要求主线程；不能用普通 `#[test]` 想当然。

**步骤**：

1. 在仓库外或 feature-gated 的一次性 spike 中，用锁定版本构造 `VisualTestAppContext`（headless `MacPlatform`），打开 -10000 离屏窗口，`capture_screenshot` 取像素；验证尺寸、非空、连续两次运行稳定。
2. 若可行，提交依赖准入申请：`gpui` 的 `test-support` feature、`gpui_macos` 平台实现、以及保存 PNG 所需的 `image`（或等价编码器）直连 dev-dependency；更新策略、指纹与 `--self-test`。
3. 把 `crates/aline-ui/src/native_window.rs` 中私有的窗口构造抽出可测试入口；不改生产行为，公共 API 守卫同步更新。
4. 新增 `crates/aline-ui/tests/visual.rs`，`harness = false`，在 `main` 中顺序执行用例，确保主线程；用 `[[test]] required-features = ["visual-tests"]` 控制，不进入默认 `cargo test`。
5. 用例优先覆盖可判定结构，而不是整图 golden：三段行顺序、内容列 880/900/600 档、空状态、打开夹具后的可见文本、已知像素命中→Source 锚点、双视图列宽一致、统计占位/零值。
6. 截图与差异图只写 `logs/validation/`；若要做像素回归，先定义容差、尺寸处理和失败输出，不把整图二进制当唯一断言。
7. 记录“不证明”：真实窗口 chrome、焦点环、系统 IME、锁屏、多显示器、Windows/Linux。

**验收**：spike 报告给出明确 go/no-go；go 时给出可复跑命令、截图与测试输出；no 时记录阻塞与回退到真实窗口证据。**不**把离屏截图写成 V-09 整项通过。

### W2 中文排版与字体能力

**目标**：用实测决定是否需要改 gpui-pre；不先复制 quiet-md 的补丁。

**事实**：gpui-pre 0.3.9 的 `LineWrapper::is_word_char` 已包含 ASCII 闭合标点（`! ) ] } " ” » …`）与 `, . : ;`，但没有全角中文标点；`line_layout.rs` 是第二份同逻辑实现。quiet-md 的做法是在两份算法里加 `is_no_break_before`/`is_no_break_after`，并自写 `wrap_spaced`；它本质是“取消断点”，找不到候选点时仍会硬断，仍可能让标点落到行首。

**步骤**：

1. 建中文排版夹具：`，。、！？；：）】》」』`、`（【《「『`、中英混排、emoji、组合字符；宽度 320/480/768；覆盖 Visual 与 Source。
2. 实测并记录：行首/行尾标点位置、行高变化、字体回退；确认 Aline 当前 renderer 走的是哪条换行路径；产出最小复现。
3. 决策门（四选一，记录理由）：
   - A. 向上游 gpui-pre 提补丁，升级锁定版本（维护成本最低，但要等上游）。
   - B. vendor `gpui-pre` + `gpui-pre-macos` + `gpui-pre-apple` 三个包，`[patch.crates-io]` + 补丁清单 + 升级重打脚本（能力最直接，治理成本最高）。
   - C. 在 Aline 布局层预折行/分段，再交给 GPUI；只有在映射、命中、选区、IME 都能保持时才可行。
   - D. M1 记录为已知限制，M4 再处理；V-04 只报告字体回退与加载，不宣称中文避头尾通过。
4. 合成斜体单独决策：中文强调/引用是否必须倾斜；14° 是 WebKit 合成角，不能默认照搬；若采用，评估 advance 不变导致的右侧伸出。
5. CJK 标点宽度单独验证：CoreText 回退可能挤压全角标点；若实测存在，优先用“按 run 指定 PingFang SC”的 UI 层方案，不急于 vendor。
6. 字体平滑单独决策：quiet-md 补丁主动开启 smoothing，而 Aline 原型 CSS 使用 `-webkit-font-smoothing: antialiased`，方向相反；以原型截图/墨量实测二选一。
7. 按 V-04 补字体注册失败、缺字族、布局失效、授权 TTF/OTF、损坏字体的无头/真实实验；与 M1 任务 6.4 拟新增的 `crates/aline-ui/tests/readonly_fonts.rs` 对齐，失败必须可见，不能静默。

**验收**：夹具与实测报告；决策记录；若 vendor，补丁清单、上游 diff 命令、升级检查与依赖准入指纹更新；V-04 只回填实际分项。

### W3 引擎不变量与模糊测试

**目标**：把 quiet-md 的 fuzzer 方法移植到 Aline 的“源文本唯一真值 + 三坐标映射”，不引入 `rand`/`proptest`。

**步骤**：

1. 在每个测试 crate 放一个约 30 行的 xorshift64* 测试 RNG（或共享 `tests/common/rng.rs`）；不新增生产依赖。
2. `aline-markdown`：生成合法/边界 Markdown（含保护区间、HTML、front matter、脚注、表格、嵌套、中文/emoji），逐步随机修改源文本；不变量：
   - 解析不 panic、不越界；预算触发是类型化错误。
   - `Projection` 的源码/绘制范围单调、可逆、覆盖可见文本；保护区不被映射为可编辑。
   - 增量投影 == 全量重解析（同一源文本）。
   - 无编辑时源码字节不变；重复解析结果一致。
3. `aline-buffer`：随机补丁/撤销/重做/锚点；不变量：正逆补丁互逆、revision 单调、历史裁剪、保存点身份、重载边界。
4. `aline-edit`：随机命令 + 随机选区；不变量：`plan_change` 不改输入、`validate_change` 拒绝过期快照/投影/选区、MD-11 保护、单区间补丁、只读模式零写入。
5. `aline-source`：随机 BOM/行尾/孤立 CR/无尾换行/非法 UTF-8/`%PDF`/`PK` 前缀；不变量：解码/编码/往返、原子写失败注入、崩溃窗口。
6. 复跑接口：环境变量 `ALINE_FUZZ_SEED/SEEDS/STEPS/TRACE`（命名可按仓库习惯调整）；失败输出 seed、步号、操作日志（最近操作序列）、最小源片段；默认规模小，夜间/手动跑大。
7. 禁止测试辅助函数“修正”已知损失；若发现真实丢失，列为缺陷，不把它写进 oracle。

**验收**：四个 crate 的 fuzz 测试通过；至少有一个“注入错误→测试失败→修复→通过”的回归样例，证明断言不是空转；大 seeds 输出留 `logs/validation/`。

### W4 保存、恢复与外改闭环（M2）

**目标**：按 SAFE-04/05/07/08 建 Aline 自己的保存语义；quiet-md 只提供失败清单。

**步骤**：

1. 写盘路径：同目录临时文件 → 写 → 文件 sync → rename；父目录 fsync 作为显式决策项；权限/磁盘满/rename 失败都要有可重试路径。
2. 基线：打开/保存时记录磁盘身份（内容指纹 + 大小/mtime 作为快速检查）；写前再查。
3. 外改：本地无修改→自动重载并保持可定位位置，重载可撤销；有冲突→暂停自动保存，展示基线/本地/磁盘差异，提供载入磁盘、确认覆盖、另存为、取消；不自动合并。
4. 保存状态：未修改/有修改/保存中/保存失败；只有对应版本写入成功才显示已保存；保存期间继续输入仍算有修改；撤销回已保存内容恢复已保存状态。
5. 自动保存：停止输入 1 秒后防抖；组合未结束延后；显式保存/另存为/导出按 ED-01 可取消等待；未命名文档不自动选路径。
6. 恢复：有修改的命名/未命名文档每 10 秒写恢复快照，含当前输入与临时图片引用；异常启动逐文档恢复/丢弃，不提供全部丢弃；恢复先入内存，不覆盖磁盘。
7. 故障注入：同 size/mtime 外改、目标再次变化、权限、磁盘满、部分写、崩溃在临时文件/rename 之间、恢复损坏、重复进程。
8. 不采用：文档旁 `.bak` 作为恢复手段、500ms 数值、主线程同步 fsync、冲突后只有“导出”一条路。

**验收**：V-08 故障矩阵；AC-05/06/10 相关链路；AC-07 的多标签/多窗口部分留在 M5。真实保存/重开/恢复证据不能由 create-only 原语或单元测试推定 M2 出口。

### W5 IME 与输入协议（M2）

**目标**：把 quiet-md 的 IME 失败教训前置到 M2 设计，避免“能打字但候选窗/取消/跨块出错”。

**步骤**：

1. 组合会话归属：marked 只保留在唯一打开块的槽；组合确认单独成一步历史；取消不写草稿、不改磁盘。
2. 坐标换算：内部只用字节/映射层；GPUI 文本端点用 UTF-16，glyph 索引是 UTF-8 字节偏移；光标/选区用字素簇；三者不得互相冒充；测试 emoji ZWJ、组合符、CJK 标点、CRLF。
3. 跨块：选区跨两块时上报折叠 range，`marked_text_range` 返回 None；输入不得把组合拆进多个块；若产品要求跨块输入，另立决定。
4. 候选窗：元素 frame + 文本布局 + 滚动偏移 + 窗口坐标一起换算；测试长文虚拟列表边缘、缩放、多显示器（能测则测）。
5. 清理：失焦、块切换、窗口失活、撤销/重做、鼠标按下都显式 `unmark`；不得残留 marked 下划线或旧组合。
6. 滚动/揭示：组合期间的 reveal 走现有 `Reveal`，不直接改列表偏移；测试候选窗跟随。

**验收**：无头组合事件测试 + 真实 macOS 中文输入法观测；V-07 输入分项。**不**由模拟事件代替真实 IME 全矩阵。

### W6 语义 token 与主题

**目标**：消除 `aline-ui` 里的散落十六进制，为 M4 主题打地基；只重命名，不改视觉。

**现状**：`shell.rs` 已有 `SURFACE/TEXT/SECONDARY/BORDER/ACCENT/ACCENT_SOFT/HOVER/NOTICE_SURFACE`，但 `readonly_reader.rs` 仍直接写 `0x0969DA`/`0xF6F8FA`/`0x8C959F`，`native_window.rs` 还有透明色字面量。原型 `docs/prototype/legacy/light.css` 已有 `--bg/--surface/--fg/--muted/--border/--accent/--accent-soft/--code-bg/--hover/--shadow`，以及 `--app-*`、`--md-*` 语义层。

**步骤**：

1. 新增 `crates/aline-ui/src/native_window/tokens.rs`（或 `aline-ui/src/theme/tokens.rs`），按原型变量建语义常量；每个值注明来源（原型行/已确认 UI 规范/实验候选）。
2. 组件只引用语义名；原始色阶与语义 token 分两层，避免再次混用。
3. 替换 `shell.rs`、`readonly_reader.rs`、`native_window.rs` 的字面色；滚动条 rail/thumb、链接、代码表面、focus ring 等一并归类。
4. 加一个测试/检查：`aline-ui/src` 除 token 模块与测试外不得出现颜色字面量；允许的例外逐条注释。
5. 用截图/像素对比确认重构前后一致；不顺手改色、不改间距、不冻结候选值。
6. 不建主题包 schema；M4 的正式主题仍走 `aline-app-core` 纯模型与 `docs/theme-package-spec.md`。不采用 quiet-md 的 OKLab 自动暗色或 craft/feishu 色值。

**验收**：token 守卫通过；M1 视觉分项截图无差异；V-09 只回填“token 化”分项，不宣称完整主题通过。

### W7 工程门禁、性能报告与文档单一来源

**目标**：采用 quiet-md 的“可复现报告”与“失败可见”，不复制其门禁分叉。

**步骤**：

1. 采用 quiet-md 报告的环境、源码 commit/锁定构建、方法/操作序列、计时边界、P50/P95/最大、原始日志、复现命令与明确“不测量什么”结构；按 Aline R-11-01 另补样本 SHA-256、冷/热、最长主线程停顿、峰值内存。用于 M1 任务 7.1，不复制其 1M 字 5ms 数字。
2. 不新增第二个 `check.sh`：Aline 的 `scripts/check_workspace.py` 已是统一入口；如要便捷别名，只能调用它，不得另建一套策略。
3. 若 W2 vendor：新增 `vendor/PATCHES.md` 风格的补丁清单（上游版本、补丁表、diff 命令、升级重打、已知风险），并更新依赖准入指纹。
4. 日志/panic hook：评估 UI 层“最后兜底”是否写未跟踪日志并在启动失败时可见；引擎仍禁止 panic；不照搬 quiet-md 对第三方 `catch_unwind` 的用法。
5. 本地化：确认现有 catalog 测试覆盖用户可见串；补一个源码扫描/键存在性检查，避免新增硬编码；不采用中文原文 key。
6. 文档：`AGENTS.md` 是单一来源，`CLAUDE.md` 只做入口指引；不复制两份正文。
7. 无障碍：把 aria label/焦点顺序纳入 W1 或独立小项；quiet-md 无可参考实现，按 Aline UI 规范自建。

**验收**：V-05 报告字段齐全；统一门禁仍通过；若 vendor 则补丁清单与准入自测通过；文档无重复正文。

## 9. 顺序、依赖与阶段映射

| 时点 | 可做 | 先决 | 不做什么 |
|---|---|---|---|
| M1 进行中 | W0 登记；W1 spike；W2 测量；W3 引擎 fuzz；W6 token（小步） | 用户批准对应 change | 不在 M1 开 M2 的保存/IME 实现 |
| M1 出口前 | W1 go/no-go；W2 决策；W7 性能模板 | M1 真实窗口证据仍以 macOS 实测为准 | 不用离屏截图替代真实窗口 |
| M2 | W4 保存/恢复/外改；W5 IME；W3 扩展 edit fuzz | V-08 身份/事务方案、TA-01/TA-02 | 不把 10 秒恢复并入自动保存 |
| M4 | W2 最终排版；W6 完整主题/字体能力 | V-04 格式与限额冻结 | 不把 M1 token 当正式主题包 |
| M5/M6 | W4 多窗口/路径；W7 跨平台性能 | M5 工作区模型 | 不用单平台结果推断三平台 |

依赖关系：

- W0 是所有工作流的门；未批准不写代码。
- W1 与 W2 可并行，但 W2 的实测可复用 W1 的离屏渲染；若 W1 失败，W2 回到真实窗口。
- W3 独立于 UI，可在 M1 期间先做。
- W4/W5 依赖 M2 的 V-08 与 TA-01/TA-02 设计，不在 M1 提前实现。
- W6 只依赖现有原型 token，可小步做；但不要阻塞 M1 关键路径。
- W7 持续；性能模板服务 M1 任务 7.1。

**建议的首批 change**（均需用户批准，不是本文已创建）：

1. `m1-visual-evidence-spike`（W1）：只做能力验证与准入申请，不改生产行为。
2. `m1-cjk-typography-measurement`（W2）：只做夹具/测量/决策记录，不 vendor。
3. `engine-fuzz-invariants`（W3）：只加测试与测试 RNG，不加生产依赖。
4. `m1-ui-semantic-tokens`（W6）：只做 token 化与守卫，不改视觉。

M2 再立 `m2-save-recovery` 与 `m2-ime-protocol`。

## 10. 验收与证据矩阵

| 工作流 | 自动检查 | 人工观测 | 证据位置 | 不证明 |
|---|---|---|---|---|
| W1 | feature-gated visual test；尺寸/结构/像素断言 | 截图、模拟点击/按键 | `logs/validation/.../visual/` | 真实窗口/IME/锁屏/平台矩阵 |
| W2 | 夹具渲染、行首行尾断言、字体加载负例 | 真实中文排版截图、候选窗 | `logs/validation/.../fonts/` | 完整主题/三平台字体 |
| W3 | `cargo test` fuzz + 大 seeds 手动 | 失败回归样例 | `logs/validation/.../fuzz/` | UI 正确性/性能 |
| W4 | V-08 故障注入、保存状态机测试 | 真实保存/重开/恢复 | `logs/validation/.../save/` | 多窗口/跨平台 |
| W5 | 组合事件/坐标/跨块测试 | 真实中文 IME | `logs/validation/.../ime/` | 所有输入法/平台 |
| W6 | token 守卫 + 截图对比 | 重构前后对比 | `logs/validation/.../tokens/` | 主题包/暗色/三方主题 |
| W7 | `check_workspace.py`、catalog 扫描 | 性能报告审阅 | `logs/validation/.../workspace/` | 发布性能门槛 |

规则：原始输出只进未跟踪目录；文档只写路径归一化后的相对位置；失败不勾任务；不把“测试通过”写成 V 项整项通过；离屏与真实窗口证据分目录、分标题。

## 11. 风险与未决问题

### 风险

- **R1 test-support 准入**：新增 `proptest`/`futures-concurrency`/`postage`/`web-time` 等传递依赖，可能改变测试构建时间与 feature 统一；未批准就做 W1 会破坏依赖准入。
- **R2 离屏能力不确定**：headless `MacPlatform`、主线程、Metal 离屏、截图权限/稳定性都可能在 spike 中失败；失败不是错误，硬推才是。
- **R3 vendor 升级债**：要同时 fork `gpui-pre`、`gpui-pre-macos`、`gpui-pre-apple`；15 个补丁的教训说明升级会持续收费。
- **R4 Aline 层预折行风险**：可能破坏映射、命中、选区、IME；只在 W2 的选项 C 且证明可行时采用。
- **R5 fuzz 生成器质量**：没有 property 库时，生成器可能只覆盖已知语法；用种子文档 + 回归样例 + 覆盖率观察补足，不把“跑了很多步”当覆盖证明。
- **R6 M2 范围膨胀**：W4/W5 都是安全闭环，不能为了赶 M1 证据提前做半套；也不要把它们塞进 M1 change。
- **R7 许可/来源**：复用 quiet-md 代码（MIT）与移植 GPUI 补丁（Apache-2.0）义务不同；需要保留声明/修改说明，不能只写“参考”。
- **R8 证据膨胀**：离屏截图、模拟事件、无头测试都不能替代真实窗口/IME/平台证据；命名与结论必须区分。

### 待确认问题

1. 是否批准 W1 的 `test-support` 依赖准入 spike（含 `image` 直连 dev-dependency）？
2. 是否接受 W2“先测量、后决定是否 vendor 三个 gpui-pre 包”的顺序？
3. 若 W2 实测确认中文避头尾缺陷，M1 是允许列为已知限制，还是必须 M1 内解决？
4. W6 token 化放在 M1 收尾还是 M1.5？它是否会挤占 M1 关键路径？
5. W3 默认 fuzz 规模与 CI 时间预算是多少（例如 16/32 seeds × 100/200 步）？
6. W4/W5 是否严格按 M2 计划，不在 M1 提前实现？
7. 是否允许为 W1 重构 `native_window` 的窗口构造入口（会触及公共 API 守卫）？

**2026-11-12 状态更新（追加，不改写上面各问）：** 第 2、3 问已由[决定 §35](./decisions.md#35-2026-11-12-m1-剩余七项裁决与受控失败实验)裁决——M1 接受中文避头尾已知限制，不在 M1 vendor 三个 gpui-pre 包，vendor 决策推到 M2 出现 caret/IME 后重新评估；W2 的「先测量、后决定」顺序本身已按此执行完毕。其余问题状态不变；本文仍是参考研究，裁决以决定记录与验证计划为准。

## 12. 执行交接

本文先作为参考研究落库；2026-11-05 已按决定 QMD-01 把方法回写现行规范，并由 OpenSpec change [`m1-single-window-readonly`](../openspec/changes/m1-single-window-readonly/proposal.md) 记录治理 delta；不改代码、不加依赖。建议下一步：

1. 用户审阅本文与 QMD-01 的回写范围，确认要授权哪些 W 项；未授权的保持“建议/待验证”。
2. 对授权项分别建 OpenSpec change，先做 W0 的范围/非目标/证据表，再做实现。
3. 第一批优先 W1 spike 与 W2 测量：它们决定 M1 证据能否补强、中文排版是否要动上游；W3 可并行；W6 只在不会挤占 M1 关键路径时做。
4. 每个 change 完成后按 Aline 的验证计划回填；不能用本文替代证据，也不能用 quiet-md 的结果替代 Aline 实测。

## 13. 证据索引

| 索引 | quiet-md 位置 | 用途 |
|---|---|---|
| E01 | `docs/ARCHITECTURE.md:3-64` | 分层、编辑流程、直接修改例外 |
| E02 | `crates/core/src/store.rs:203-271,315-345,392-405` | 原子写、JSON 备份、锁、基线 |
| E03 | `src/app.rs:836-851,870-936` | 自动保存、失败、切换/退出 |
| E04 | `crates/core/src/editor/core.rs:321-385,608-619` | 撤销上限/合并/保留字段 |
| E05 | `crates/core/src/editor/fuzz_tests.rs:28-45,105-219,520-590` | fuzzer 结构/不变量/复跑 |
| E06 | `src/editor/render.rs:91-129,1030-1107` | 虚拟化/长度键/重测 |
| E07 | `vendor/PATCHES.md:1-60` | 15 个 GPUI 补丁与 taffy 补丁 |
| E08 | `src/logging.rs:28-60`、`src/main.rs:80` | 日志/panic hook/窗口最小尺寸 |
| E09 | `src/editor/text.rs:1310-1364`、`src/ui.rs:540-613` | 自写避头尾/CJK 标点字体 |
| E10 | `docs/PERFORMANCE.md:1-47`、`AGENTS.md`、`.github/workflows/ci.yml` | 性能报告/纪律/CI |
| E11 | `gpui-pre-0.3.9/src/app/visual_test_context.rs:20-130,384-390`、`Cargo.toml:69-78` | 离屏窗口/模拟输入/截图/门控 |
| E12 | `gpui-pre-0.3.9/src/text_system/line_wrapper.rs:501-540`、`line_layout.rs:232` | ASCII 闭合标点已有、全角中文未覆盖 |

Aline 侧对应位置：`crates/aline-ui/src/native_window.rs`、`crates/aline-ui/src/native_window/`（`shell.rs`、`readonly_reader.rs`、`geometry.rs` 等）、`crates/aline-ui/src/document/`、`crates/aline-markdown/`、`crates/aline-buffer/`、`crates/aline-edit/`、`crates/aline-source/`、`scripts/check_workspace.py`、`docs/v1-validation-plan.md`、`docs/v1-prd.md`。

## 附录 A：不采纳清单（带理由）

| 做法 | 理由 |
|---|---|
| block 树/DocxXML/U+FFFC 文档模型 | 与 Aline“源文本唯一真值”冲突 |
| 整篇 Document 快照撤销 | 与补丁/历史组协议冲突，长文档内存线性增长 |
| `hash_block` 文本只取长度 | 会把屏幕外高度/内容缓存置于过期风险 |
| 双主题双渲染 + 全局 bool | 复制渲染与分派，维护成本高，易漂移 |
| 视图层 `block_mut`/`blocks.extend` | 绕过唯一写入口，破坏不变量 |
| 整体 vendor + 15 补丁 + AppKit 注入 | 升级/平台/法务长期债；Aline 只按需最小补丁 |
| 主线程同步 fsync + 全库重解析 | 大文档/慢盘/大库会卡输入与窗口 |
| 测试裁掉已知丢失 | 把缺陷变成 oracle，掩盖真实损失 |
| 中文原文作 i18n key | Aline 已有 key catalog，回退会丢类型/扫描能力 |
| 30 天固定回收站/默认远程图片等 | 产品语义不同，不能自动迁移 |

## 附录 B：与现有 Aline 任务的对应

| 本文工作流 | 现有 M1/M2 任务 | 关系 |
|---|---|---|
| W1 | 10.2/11.2/12.2/12.3/13.2/13.3/14.2/15.2 的真实窗口截图阻塞 | 补充证据手段，不替代真实窗口 |
| W2 | 6.4 字体/V-04 | 测量与决策前置，避免直接 vendor |
| W3 | 引擎 crate 测试/V-07 基础 | 新增不变量测试，不改产品行为 |
| W4 | M2 SAFE-04/05/07/08、V-08 | 按 M2 设计实施，不在 M1 提前 |
| W5 | M2 TA-01/IME、V-07 | 按 M2 设计实施 |
| W6 | 5.x/7.x 视觉基线、V-09 | 小步重构，服务后续主题 |
| W7 | 7.1 V-05、7.4 守卫、7.5 完整门禁 | 补报告模板与守卫，不另建门禁 |
