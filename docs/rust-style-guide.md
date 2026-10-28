# Aline Rust 代码规范

> 版本：v0.1（2026-10-26）
> 适用范围：本仓库全部 Rust 代码（`crates/*` 与 `app`）。
> 配套文档：[engine-design.md](./engine-design.md)（引擎机制与不变量）、[v1-architecture.md](./v1-architecture.md)（crate 划分与骨架）、[architecture-guide.md](./architecture-guide.md)（架构边界与 spec 约束）、[zed-notes.md](./zed-notes.md)（Zed 源码实证笔记，§18 的依据）。

**执行方式**：规则分两级——

- **【强制】**：能被 rustfmt / clippy / CI 拦截的一律工具执行，code review 不再争论；其余在 review 中把关，不通过不合并。
- **【建议】**：工具查不了的（抽象时机、命名语义、注释质量），review 时执行。

整体姿态：**以官方与社区共识为底座**（rustfmt 默认、clippy、Rust API Guidelines），本规范只补充两类内容——工具管不到的约定，以及编辑器这个领域（字节偏移映射、增量解析、GPUI）特有的规则。参考基准见 §17。

---

## 目录

1. [基本原则](#1-基本原则)
2. [工具链](#2-工具链强制)
3. [工程结构与模块组织](#3-工程结构与模块组织强制)
4. [命名](#4-命名强制)
5. [坐标系与类型设计](#5-坐标系与类型设计强制)
6. [格式](#6-格式强制)
7. [错误处理](#7-错误处理强制)
8. [所有权、借用与分配](#8-所有权借用与分配)
9. [Option / Result / match](#9--option--result--match)
10. [泛型与 trait](#10-泛型与-trait)
11. [unsafe](#11-unsafe强制)
12. [文档与注释](#12-文档与注释)
13. [测试](#13-测试强制)
14. [并发与 GPUI](#14-并发与-gpui强制)
15. [性能](#15-性能)
16. [依赖与提交](#16-依赖与提交)
17. [参考基准](#17-参考基准)
18. [参考项目实证条款](#18-参考项目实证条款)
- [附录 A：rustfmt.toml](#附录-arustfmttoml)
- [附录 B：workspace lints](#附录-bworkspace-lints)
- [附录 C：CI](#附录-cci)

---

## 1. 基本原则

1. **官方优先，不另起炉灶。** 与 rustfmt/clippy 默认行为冲突的规则一律不设；想"手工调整格式"时先怀疑自己。
2. **能自动化就不靠人眼。** 新增规则先问：clippy 有没有现成 lint？能进 CI 就进 CI。
3. **引擎与 UI 分治。** `aline-source / aline-buffer / aline-markdown / aline-edit / aline-app-core` 是零 GPUI 的纯逻辑层，标准最严（禁 unwrap、禁 panic、零日志零全局状态、proptest 必备）；`aline-ui`、`app` 允许少量 pragmatism（`expect` + 明确理由）。
4. **不变量高于实现。** 引擎的结构不变量（文本永不重建、编辑=区间补丁、块区间排除尾部换行、源码字节偏移是唯一坐标系）定义在 engine-design.md §1.5 / §2.4——**任何代码不得绕过**。怀疑不变量被破坏时，优先补 `debug_assert!` 而不是加注释。
5. **简单直接。** 出现第二个真实使用者之前不抽 trait；能用具体类型不用泛型；不做用不上的配置项（YAGNI）。

## 2. 工具链【强制】

| 项 | 约定 |
|---|---|
| Rust 版本 | 最新 stable，跟随官方发布节奏升级（应用项目，无 MSRV 负担） |
| edition | 2024（`workspace.package` 统一声明，各 crate 继承） |
| 格式化 | rustfmt，规则见附录 A；CI 跑 `cargo fmt --all --check` |
| Lint | clippy，配置见附录 B；CI 以 `-D warnings` 运行 |
| 测试 | `cargo test --workspace` 进 CI；性能敏感提交附 criterion 数据 |
| 依赖审计 | 定期 `cargo audit`（不必每次 CI） |

提交前本地最小检查（可做成 pre-commit hook）：

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

**警告不清零**：CI 里 `-D warnings` 意味着不允许"先合入再修"；临时豁免用 `#[allow(...)]` 并在旁边写一行原因，禁止裸 allow。

## 3. 工程结构与模块组织【强制】

### 3.1 crate 划分与依赖方向

以 [v1-architecture.md §1.2](./v1-architecture.md) 为准，依赖严格单向：

```
app(bin) → aline-ui → aline-plugin → aline-edit → aline-markdown → aline-buffer
                └── aline-app-core（theme / i18n / settings，零 GPUI，谁都能依赖）

aline-source 独立：buffer 不依赖它，文件 I/O 由 ui/app 直接编排（与 v1-architecture §1.2 一致）。
```

- **引擎 crate（source/buffer/markdown/edit/app-core）禁止依赖 gpui**，也禁止依赖彼此的私有项——依赖方向在 Cargo.toml 里体现，不得为了图方便加边。
- 公共 API 只有各 crate 的 `lib.rs` re-export 这一个出口；`pub use` 集中管理，形成稳定 API 面。
- `app` 是薄壳：窗口、菜单、计时器等编排，不写业务逻辑。

### 3.2 模块组织

- 模块目录统一用 `mod.rs` 风格（与 v1-architecture §1.3 骨架一致：`tree/{mod.rs, sources.rs}`），**全仓库只用一种风格，不混用**。
- 一个文件一个核心类型（及其私有辅助类型）；超过约 500 行考虑拆分（软限制，拆分标准是职责而不是行数）。
- 禁止通配导入 `use foo::*`；唯一例外是测试模块顶部的 `use super::*`。
- 文件内排序建议：`//!` 模块说明 → imports → 常量 → 类型定义 → impl → 自由函数 → `#[cfg(test)] mod tests`。

### 3.3 可见性

- 默认私有；确需跨模块用 `pub(crate)`；仅 API 面上的项用 `pub`。`pub` 是承诺，每加一个都过一遍"外部是否真的要用"。反面教材是 Ely：大量 API 锁死在 `pub(crate)`，外部项目无法依赖只能借鉴重写——Aline 的 `pub` 面就是跨 crate 契约（architecture-guide §1.2）。
- 结构体字段默认私有，需要只读暴露时提供方法（getter 规范见 §4）；确需公开字段（如 `Selection { anchor, head }` 这类纯数据）可以 `pub`，但不加 `pub` 的同时期待外部不改。

## 4. 命名【强制】

对齐 [Rust API Guidelines（C-CASE 系列）](https://rust-lang.github.io/api-guidelines/naming.html)，clippy 会拦截大小写违规，此处只列需要判断力的部分。

| 场景 | 规则 | 例子 |
|---|---|---|
| 转换方法 | 语义与开销三档：`as_`（零成本视图）/ `to_`（可能有成本）/ `into_`（消耗所有权）；可能失败加 `try_` 前缀并返回 Result | `as_str`、`to_vec`、`into_inner`、`try_decode` |
| getter | 名词，**不加 `get_` 前缀** | `fn len(&self)`、`fn sources(&self)` |
| 迭代器 | `iter()` / `iter_mut()` / `into_iter()` | `blocks.iter()` |
| 布尔谓词 | `is_` / `has_` / `can_` / `should_` 前缀 | `is_open`、`needs_full_reparse` |
| 事件回调 | `on_` 前缀 | `on_commit`、`on_block_click` |
| 构造 | `new` / `from_` / `with_`；多构造器时语义自明 | `Source::decode`、`History::with_budget` |
| 常量 | SCREAMING_SNAKE_CASE，语义完整，禁止魔法数字 | `const OVERDRAW_REM: f32 = 40.0;` |
| 类型别名 | UpperCamelCase；模块级 Result 别名统一形式 | `pub type Result<T, E = DecodeError> = std::result::Result<T, E>;` |

- **不无谓缩写**：`span`、`offset`、`range`、`selection`，不写 `sp`、`off`、`rng`。领域术语（`rebase`、`alike`、`reread`）一旦在 engine-design.md 里定名，代码里就用同一个词，文档与代码同语言。
- 类型名不使用空词（`Data`、`Info`、`Manager`、`Utils`）；领域名词优先：`Buffer`、`Parsed`、`FoldedInlineEditor`。
- 同一概念全仓库同名：块区间就是 `range`，绘制文本就是 `text`（`Inline.text`），源码偏移就是 `at`/`offset`——不要每层换一个近义词。

## 5. 坐标系与类型设计【强制】

编辑器正确性的最大来源就是坐标混乱，这条是本规范最重要的领域规则。

1. **源文本字节偏移是全系统唯一坐标系**。`Selection`、`Block.range`、补丁区间一律是源文本字节坐标。凡是"位置/偏移"语义的变量，**禁止裸名 `pos`、`idx`、`x`**，必须带坐标系前缀：

   | 坐标 | 命名 | 出现位置 |
   |---|---|---|
   | 源文本字节偏移 | `src_off` / `src_range` | 全系统 |
   | 绘制文本偏移 | `drawn_off` / `drawn_range` | 仅 aline-ui 的映射与折叠编辑器 |
   | UTF-16 偏移 | `utf16_off` | 仅 `EntityInputHandler` 边界（IME） |

   三种坐标之间的每一次转换都必须经过 `Inline::source_at` / `drawn_at` / `aligned`（或折叠编辑器中的等价映射层），**禁止在调用点手工加减偏移量**。

2. B+（折叠编辑器）落地时，三套坐标 **newtype 化**（`SrcOff`、`DrawnOff`、`Utf16Off`），让类型系统拦截混用；纯 UI 局部的布局计算除外。
3. 行列号（`line`/`column`）只在 `LineMap` 与 UI 展示层出现，不进入引擎 API。
4. **纯数据结构体**（`Selection`、`UndoStep`、`Change`）字段可公开；行为体（`Document`、`Open`）字段私有，状态变更全部走方法——不变量（如"块区间排除尾部换行"）由方法维持。
5. **派生策略**：新类型默认 `#[derive(Debug, Clone, PartialEq)]`；进比较器/HashMap 的补 `Eq`/`Hash`；公开 API 类型补 `Default`（如果语义成立）。不派生用不上的。
6. **布尔参数爆炸禁令**：跨模块函数禁止两个及以上 bool 参数，改用枚举或参数结构体：

   ```rust
   // ❌ 读调用点时真假难辨
   fn open(&mut self, ix: usize, reveal: bool, remember: bool);

   // ✅ 语义自明
   fn open(&mut self, ix: usize, landing: Landing);
   ```

## 6. 格式【强制】

- **格式全部交给 rustfmt**，review 时讨论空白/换行/引号一律视为无效意见；拿不准就让 fmt 决定。
- 行宽保持默认 100；确实放不下的表达式优先考虑提取变量/函数，而不是调宽行宽。
- 字符串字面量：普通文本用双引号；含大量转义的路径/正则用 `r"..."`。

## 7. 错误处理【强制】

### 7.1 分层策略

| 层 | 方式 |
|---|---|
| 引擎 crate（库性质） | `thiserror` 定义各 crate 的错误枚举；模块级 `pub type Result<T, E = XxxError>`；错误信息中携带足够定位的上下文（偏移量、块序号等） |
| `aline-ui` / `app`（应用性质） | 可用 `anyhow` 汇聚底层错误；需要用户决策的失败不走 Result，走事件（`Event::EditRefused` 等）或 UI 通知 |

### 7.2 红线

- **引擎 crate 禁止 `unwrap()`**（clippy `unwrap_used = "deny"`）。`expect("为何此刻一定成立")` 仅允许用于不变量断言，且消息必须写清理由，如 `expect("block ranges are sorted by apart(), see engine-design.md §2.5")`。
- **解析器对用户输入永不 panic**：畸形 markdown、非法 UTF-8、超界偏移一律返回 `Result` 或按普通文本降级处理，绝不能"这个文件会让编辑器崩"。
- 可能失败的 I/O、解码一律 `Result` + `?`；上层用 `.context()` / `.with_context()` 补语义（"在做什么时失败的"），不吞错误。
- 越界风险处用 `get()` 显式处理；程序不变量用 `assert!`（发布期保留）/ `debug_assert!`（仅调试期）表达，不要用注释表达断言。

### 7.3 拒绝制不是错误

命令框架的"拒绝"（`EditRefusal::{Code, Math, Html, FrontMatter, Changed}`）是**业务上的正常返回值**，不是 `Result::Err`。区分标准：

- `Result::Err` = 系统级失败（I/O、解码、内部不变量被破坏），通常需要日志或弹窗；
- `EditRefusal` = 可预见的"这个操作此刻不该做"，UI 静默提示即可。

两者不得混用——把拒绝塞进 Err 会让 `?` 传播路径失去区分度。

## 8. 所有权、借用与分配

- 【强制】参数传视图不传容器：`&str` 不 `&String`，`&[T]` 不 `&Vec<T>`，`Range<usize>` 不 `&Range<usize>`；需要所有权时用 `impl Into<T>`（如 `impl Into<SharedString>`），由调用方决定给什么。
- 【强制】**引用计数 clone 是免费的**：`SharedString`、`Entity<T>`、`Arc<Parsed>` 的 clone 随手用，不用传生命周期硬拧。**深拷贝（String/Vec 大对象）必须节俭**：热路径上出现 `String::from`/`to_string`/`clone()`（深拷贝）时，要么消除要么注释理由。
- 【强制】`aline-ui` 的每帧路径（`Render::render`、块渲染注册表 `draw()`）不得新建大 String/Vec；预格式化数据应在解析/commit 阶段算好存进 `Parsed`/`Block`。
- 【建议】集合能预估大小时 `with_capacity`；能返回迭代器/视图就不急着收集成 `Vec`；文本拼接在循环里用 `String::push_str`/`write!`，不用 `+` 链。
- 文本缓冲区的读写一律走 `Buffer` trait 的 API（`text`/`slice`/`edit`），UI 层禁止绕过 buffer 直接持有字符串副本——绕过即破坏"文本永不重建"不变量。

## 9. Option / Result / match

- 优先组合子与 `?`，避免 match 嵌套；"取不到就提前返回"用 `let ... else`：

  ```rust
  let Some(open) = self.open.as_mut() else { return; };
  ```

- 【强制】**会持续增加变体的内部枚举，match 禁止 `_` 通配兜底**——`Node`、`BlockKind`、`Command`、`EditRefusal`、`MarkdownMode` 等，新变体必须让编译器逼出所有调用点。例外：对外部库枚举（如 pulldown-cmark 的 `Tag`/`Event`）可以 `_ =>` 忽略不支持项。
- `if let` 链最多 3 层，再多改 match；match 分支体超过 4 行提取函数，保持分支表一眼可扫。
- 【建议】`unwrap_or_default()` 只用在"空值确实合法"的场景；"这里不该为空"的空值用 `expect("理由")` 表达，不要默默吞掉。

## 10. 泛型与 trait

- 【强制】抽象只在**已确定的替换点**做。项目里的正当先例：`Buffer` trait（v1 `StringBuffer`，v2 `ropey`）；`aline-plugin` 的 `Plugin`/`BlockRenderer`（插件边界）。除此之外，出现第二个真实使用者之前不抽 trait。
- API 参数尽量具体类型起步，确实需要多态再泛型化（`impl AsRef<str>` / `impl Into<SharedString>` 是允许的日常形式）。
- `dyn Trait` 只用于异构集合与插件注册表（`Vec<Box<dyn BlockRenderer>>`）；引擎热路径（解析、映射、undo）保持静态分发。
- 对外提供又不希望别人实现的 trait，做成 sealed trait（私有 supertrait）。
- 【建议】宏是最后手段：能用具类型/泛型/派生宏解决的，不手写声明宏或过程宏。

## 11. unsafe【强制】

- workspace 级 `unsafe_code = "forbid"`（见附录 B）。当前技术栈（gpui-pre、pulldown-cmark、syntect 全为纯 Rust）没有 unsafe 需求。
- 未来若某 crate 确需 unsafe（如平台栈互操作）：该 crate 单独降为 `deny`，unsafe 集中到独立模块（如 `src/sys.rs`），每一块 unsafe 上方写 `// SAFETY: <为何满足前置条件>`，并在 PR 中单独说明。其余 crate 保持 forbid。

## 12. 文档与注释

- 【强制】引擎 crate 的公共 API（`pub` 项）必须有 `///` 文档注释；复杂算法（`aligned` 反向匹配、`RewriteMap`、`alike` 配对）在实现文件顶部先写一段 `//!` 思路说明再写代码。`aline-ui`/`app` 的公共项同样要求，内部项不作硬性要求。
- 文档注释中的示例进 doctest（` ```rust ` 默认会被 `cargo test` 执行），保证示例不过期。
- 【强制】注释解释 **why**（为什么这么映射、为什么跳过、不变量依据），不复述 what。坏注释：`// 遍历块`；好注释：`// 尾部换行不进区间，保证块间空行永不落入编辑范围（engine-design.md §1.5 #2）`。
- 【强制】注释与文档用**中文**，标识符一律**英文**。中文注释能显著降低本项目领域术语（保真、折叠、改写器）的歧义成本。
- 【强制】TODO 统一格式 `// TODO(xian): 说明`，不留无主 TODO；过时注释随手删——git 历史负责记忆，注释掉的代码块一律删除不保留。

## 13. 测试【强制】

测试策略总纲见 [engine-design.md §2.11](./engine-design.md#211-测试策略发布门禁)（保真套件、映射往返、规格一致性、命令金样表、reread、IME 模拟、集成、模糊）。本节是代码组织层面的约定。

- **单元测试**与被测代码同文件，`#[cfg(test)] mod tests`；跨 crate 行为放 `tests/` 集成测试；gpui 交互用 `TestAppContext` 模拟事件（Ely/Zed 同款方法）。
- 【建议】时间/文件系统敏感的测试留确定性缝：时钟可注入，去抖时长在测试下归零（Zed FakeFs `Duration::ZERO` 模式——确定性执行器不自行推进时间），禁止在测试里 `sleep` 等真实时间。
- **命名**：`被测行为_场景`（英文，如 `edit_single_patch_creates_one_undo_step`、`source_at_within_escape_returns_content_offset`）。测试名就是行为说明书，禁止 `test1`、`test_bug_fix`。
- **快照测试**（insta）：解析树、渲染结果的回归用快照锁定，`cargo insta review` 人工审阅快照变更；快照文件与测试同目录提交。
- **属性测试**（proptest）覆盖核心不变量：`source_at(drawn_at(x)) == x` 往返一致、任意编辑序列后增量结果等于全量重解析、undo 逐字节还原。新增映射/缓冲区机制必须先补 proptest 再做 UI。
- **金样表**：命令框架每条 `Command` × 典型输入 × 选区 → 输出文本 + 新光标，以数据表驱动，拒绝用例（跨代码块、front matter）入表。
- **修 bug 先补一个失败的测试**，再修——每个 bug 留下一道回归防线。
- 测试互相独立，不共享可变状态；断言失败信息要能独立定位（带上下文偏移/输入片段）。

## 14. 并发与 GPUI【强制】

### 14.1 线程模型

- **引擎 crate 零线程假设**：不 spawn 线程、不持有锁、无全局状态（`static mut`、`lazy_static` 单例一律禁止），并发编排全部由 `aline-ui` 承担——这样引擎才能在单线程测试里全量验证。
- UI 层分工：主线程管状态与渲染；重活（全量重解析、高亮、数学/图表渲染）用 GPUI 的 `BackgroundExecutor`/`Task`/`Asset` 抛后台，**结果回主线程后先校验新鲜度再采纳**：

  ```
  spawn 后台任务（携带快照 stamp/text）→ 回主线程 → if buffer.stamp() 已变 → 丢弃
  ```

  这个"快照-验证-采纳"形状与 apart 协议一致，所有异步写回统一用它，禁止"后台算完直接改共享状态"。

### 14.2 锁与共享

- 优先所有权与消息传递（channel/事件），其次 `Mutex`；`RwLock` 仅在读多写少且证据明确时用。
- 锁内不做耗时操作（I/O、解析、渲染）、不回调外部代码、不获取第二把锁（避免锁序问题）。
- `Send`/`Sync` 约束按需最小化；后台任务捕获的数据显式 clone（引用计数 clone 免费，见 §8），不为了绕借用检查引入 `Arc<Mutex<>>` 大杂烩。

### 14.3 运行时

- **不引入 tokio 等完整 async 运行时**：GPUI 自带的 executor 与 `Task` 覆盖本项目全部异步需求（文件 I/O、资产加载、防抖解析）。
- 异步函数不得假设"一定有 executor"；纯计算保持同步函数，把 spawn 的决定留给调用方。

## 15. 性能

性能预算与手段见 [engine-design.md §2.10](./engine-design.md#210-性能策略)。代码层面的约定：

- 【建议】**先测量后优化**：用 criterion 基准（`benches/`）或火焰图定位热点，不做凭感觉的优化；性能敏感的 PR 附前后数据。
- 【强制】偏移量/行列数学防溢出：边界推进用 `saturating_*`/`checked_*`；release 配置开 `overflow-checks = true`（附录 B 的 profile 片段），别让 release 静默回绕。
- 【建议】`#[inline]` 不滥用（跨 crate 微小热函数才考虑）；`pub` API 的 `#[must_use]` 该加就加（clippy 会提示）。
- 【建议】解析/映射层避免 O(n²)：全文扫描类操作一躺过完，重复定位用 `partition_point`（二分）而不是线性找。

## 16. 依赖与提交

### 16.1 依赖

- 【强制】新增依赖先对照 engine-design.md §2.12 依赖清单：同类功能不得引入第二个库（尤其**不得引入第二个 markdown 解析器**——选型 pulldown-cmark 已由性能实测锁定，见 ADR 结论）。
- 【强制】gpui 锁死版本快照（`gpui-pre`，与 Ely 一致），升级是显式动作，vendor 目录收平台补丁。
- 【建议】新依赖 PR 中说明：用途、为什么不复用已有依赖、维护活跃度。

### 16.2 提交

- 【强制】Conventional Commits，scope 用 crate 名：

  ```
  feat(aline-markdown): 数学定界符改写器带字节映射
  fix(aline-edit): 表格整理操作拒绝跨表选区
  refactor(aline-buffer): History 打字合并改为字节预算 trim
  ```

  类型：`feat` / `fix` / `refactor` / `perf` / `test` / `docs` / `chore` / `build`。
- 【强制】一个提交一个意图；提交信息说清"为什么"，不必复述 diff；提交前跑 §2 的最小检查。

## 17. 参考基准

| 来源 | 用途 |
|---|---|
| [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) | 命名、文档、API 设计的逐条基准（C-* 编号可在 review 中直接引用） |
| [The Rustdoc Book](https://doc.rust-lang.org/rustdoc/) 与 std 文档风格 | 文档注释的段落组织（`# Errors`、`# Panics`） |
| [Clippy Lint 列表](https://rust-lang.github.io/rust-clippy/master/) | 判断某条约定是否已有现成 lint |
| [Google Rust Style Guide](https://google.github.io/rust-guide/) | 大厂实践：工具强制优先、克制的手写规则 |
| [Chromium Rust Guidelines](https://chromium.googlesource.com/chromium/src/+/main/styleguide/rust/) | 大型项目引入 Rust 的边界管理（unsafe、依赖、互操作） |
| [Zed](https://github.com/zed-industries/zed) | **GPUI 风格第一参考**：fluent builder（`div().flex().gap_2()`）、`SharedString`/`Entity` 用法、自定义 Element 范式、InputHandler 实现 |
| `/Users/xian/Project/book/zed`（本地全量克隆）+ [zed-notes.md](./zed-notes.md) | Zed 源码逐机制实证（file:line 已对抗式复核），本文 §18 与 architecture-guide 的依据 |
| `/Users/xian/Project/book/ElyOffice`、`/Users/xian/Project/book/Ely-GPUI-Components` + [engine-design.md](./engine-design.md) | Ely 逆向分析：选型依据（ADR-0041）、机制继承与架构教训的出处 |
| `/Users/xian/Project/book/MarkFlow` | 旧项目：外观/交互参照与功能语义迁移来源；性能反面实证（issue #458/#459/#460，见 architecture-guide §11） |
| [Helix](https://github.com/helix-editor/helix)、[rust-analyzer](https://github.com/rust-lang/rust-analyzer) | 编辑器/语言工具链的测试组织与增量架构 |
| [ripgrep](https://github.com/BurntSushi/ripgrep) | 错误处理与性能敏感 CLI 的范本 |

## 18. 参考项目实证条款

> 依据与出处：Zed 逐条 file:line 见 [zed-notes.md](./zed-notes.md)（2026-10-26 对抗式复核后定稿）；Ely 见 [engine-design.md](./engine-design.md)。本节只收**落到代码写法**的条款；架构层边界（分层/依赖/数据流/传播机制）见 [architecture-guide.md](./architecture-guide.md)。

### 18.1 来自 Ely（正向继承）

- 【强制】解析器缺失语法的扩展（数学块 `$$`、行内 `\(\)`）必须用**带字节映射的预处理改写器**实现，改写器产出的 `RewriteMap` 与映射层同等级别对待（进映射往返 proptest）——Ely 验证了这是补语法而不 fork 解析器的唯一干净路径。
- 【强制】命令实现走 apart 形状：输入快照 → 纯函数计算 → 应用前验证 → reread 校验；可预见的"此刻不该做"返回 `EditRefusal`，不 panic、不塞 `Result::Err`（§7.3）。
- 【建议】GPUI 交互测试用 `TestAppContext` + 模拟按键/IME 事件（Ely editor 测试同款），让 UI 行为也能无头回归。
- 【建议】数字定宽排版（字数统计、行号等场景）在容器层用 font features（`tnum`）解决，不做逐字符手工对齐——ElyOffice 的做法。

### 18.2 来自 Zed（源码实证）

- 【强制】GPUI 回调（observe/subscribe/事件）一律持 `WeakEntity`，不持有强引用；upgrade 失败静默退出是预期语义，不是错误。
- 【强制】异步三件套：`spawn(AsyncFnOnce(WeakEntity<T>, &mut AsyncApp))` 内 `background_spawn(重活).await`，再 `weak.update` 回主线程采纳；后台任务捕获的数据显式 clone（引用计数 clone 免费，§8）。
- 【强制】IME 四件套：`EntityInputHandler` 对外全 UTF-16，内部字节坐标；`text_input_configuration` 只在真变化时转发（重复重配会重启 IME 会话，Zed window.rs 源码注释原文）；组合期临时关闭自动配对/autoclose；marked text 用高亮（`InputComposition` key）表示，不引入独立 marked buffer。
- 【强制】内容寻址缓存：排版/高亮/资产按（内容 hash + 环境键）探缓存，命中零分配返回，miss 才物化（`shape_line_by_hash` 范式）；禁止按"帧数"或"时间"失效内容缓存。
- 【强制】分桶高亮的优先级 = key 枚举定义顺序（derive `Ord` + BTreeMap 升序叠色，Zed HighlightKey 实证并有源码注释背书）；禁止给每种高亮手写 priority 数字再互相调试对齐。
- 【建议】索引↔位置转换用 glyph 线性扫描（`x_for_index` / `index_for_x` / `closest_index_for_x` 形态），不为光标定位维护额外索引结构。
- 【建议】具名动作（action）命名对齐 Zed 实测规则：PascalCase 结构体 + `::` 命名空间前缀（`editor::MoveUp` 式），keymap 按名引用；菜单加速键直接从 keymap 派生，不单独维护。
- 【建议】渲染期组件从 global 现取主题（`cx.theme()` 模式），不把 token 存进组件字段——否则主题切换时要逐组件同步。

---

## 附录 A：rustfmt.toml

```toml
edition = "2024"

# 以下两项仅 nightly rustfmt 生效（stable 会忽略并警告）；影响 import 合并与排序分组
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

其余全部使用默认值；若未来想改，先在团队内（本仓库 = 提交者本人）说明理由再动。

## 附录 B：workspace lints

根 `Cargo.toml`：

```toml
[workspace.package]
edition = "2024"
rust-version = "1.85"

[workspace.lints.rust]
unsafe_code = "forbid"
rust_2018_idioms = "warn"
missing_docs = "warn"            # 引擎 crate 的 API 面要求文档化

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }

# —— pedantic 中对编辑器场景过于苛刻的，显式放行（偏移量数学大量 usize↔isize/截断） ——
cast_possible_truncation = "allow"
cast_sign_loss = "allow"
module_name_repetitions = "allow"

# —— 项目红线 ——
unwrap_used = "deny"
dbg_macro = "deny"
expect_used = "warn"
todo = "warn"
print_stdout = "warn"

[profile.release]
overflow-checks = true           # 偏移量数学溢出在发布版也 panic，而不是静默回绕
```

各 crate `Cargo.toml` 统一继承：

```toml
[lints]
workspace = true
```

引擎 crate 保持 `missing_docs`；`aline-ui`/`app` 如噪音过大，可在各自 crate 覆盖为 `missing_docs = "allow"`（覆盖需注释原因）。

## 附录 C：CI

`.github/workflows/ci.yml`：

```yaml
name: CI
on: [push, pull_request]

jobs:
  check:
    runs-on: macos-latest        # gpui 目标平台，与开发环境一致
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

后续接入 proptest/保真套件后无需改结构——它们就是普通 `#[test]`，`cargo test --workspace` 自然覆盖。
