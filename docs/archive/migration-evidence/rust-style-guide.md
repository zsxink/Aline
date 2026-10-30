# rust-style-guide 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/rust-style-guide.md`；原文件 28041 bytes，SHA-256 `417c8d1d68d45c367a0d3cac4868f60250e0dd8c420dc9b0c7a15c75efbb3383`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L15 · 原规范身份

原片段 SHA-256：`c1fe0449c92c04266e6c221434234b30773615cf65e3d4026ddec7eb21930f17`。

~~~~~~~~markdown
# Aline Rust 代码规范

> 版本：v0.1（2026-10-26）
> 适用范围：本仓库全部 Rust 代码（`crates/*` 与 `app`）。
> 配套文档：[engine-design.md](./engine-design.md)（引擎机制与不变量）、[v1-architecture.md](./v1-architecture.md)（crate 划分与骨架）、[architecture-guide.md](./architecture-guide.md)（架构边界与 spec 约束）、[zed-notes.md](./zed-notes.md)（Zed 源码实证笔记，§18 的依据）。

**执行方式**：规则分两级——

- **【强制】**：能被 rustfmt / clippy / CI 拦截的一律工具执行，code review 不再争论；其余在 review 中把关，不通过不合并。
- **【建议】**：工具查不了的（抽象时机、命名语义、注释质量），review 时执行。

整体姿态：**以官方与社区共识为底座**（rustfmt 默认、clippy、Rust API Guidelines），本规范只补充两类内容——工具管不到的约定，以及编辑器这个领域（字节偏移映射、增量解析、GPUI）特有的规则。参考基准见 §17。

---

~~~~~~~~

## L117–L145 · 旧坐标/类型规则原始片段

原片段 SHA-256：`df7e5eb1bb7961a1cd24af2e7c86ff780cabd90087445ab165cc99618fcbcf1f`。

~~~~~~~~markdown
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
~~~~~~~~

## L151–L175 · 旧错误/拒绝边界原文

原片段 SHA-256：`181c2d2e91a58fba4c5a54b0262be472a6d056daa2b985dfbb9e46499bd15498`。

~~~~~~~~markdown
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

~~~~~~~~

## L204–L216 · 旧unsafe/注释规则

原片段 SHA-256：`52a412b5c44194480911a354d20a776ce60ee04aa31747500e27b8c81e5b7d3e`。

~~~~~~~~markdown
## 11. unsafe【强制】

- workspace 级 `unsafe_code = "forbid"`（见附录 B）。当前技术栈（gpui-pre、pulldown-cmark、syntect 全为纯 Rust）没有 unsafe 需求。
- 未来若某 crate 确需 unsafe（如平台栈互操作）：该 crate 单独降为 `deny`，unsafe 集中到独立模块（如 `src/sys.rs`），每一块 unsafe 上方写 `// SAFETY: <为何满足前置条件>`，并在 PR 中单独说明。其余 crate 保持 forbid。

## 12. 文档与注释

- 【强制】引擎 crate 的公共 API（`pub` 项）必须有 `///` 文档注释；复杂算法（`aligned` 反向匹配、`RewriteMap`、`alike` 配对）在实现文件顶部先写一段 `//!` 思路说明再写代码。`aline-ui`/`app` 的公共项同样要求，内部项不作硬性要求。
- 文档注释中的示例进 doctest（` ```rust ` 默认会被 `cargo test` 执行），保证示例不过期。
- 【强制】注释解释 **why**（为什么这么映射、为什么跳过、不变量依据），不复述 what。坏注释：`// 遍历块`；好注释：`// 尾部换行不进区间，保证块间空行永不落入编辑范围（engine-design.md §1.5 #2）`。
- 【强制】注释与文档用**中文**，标识符一律**英文**。中文注释能显著降低本项目领域术语（保真、折叠、改写器）的歧义成本。
- 【强制】TODO 统一格式 `// TODO(xian): 说明`，不留无主 TODO；过时注释随手删——git 历史负责记忆，注释掉的代码块一律删除不保留。

~~~~~~~~

## L284–L401 · 参考项目证据/完整索引及旧rustfmt/lint/CI样例

原片段 SHA-256：`93bede3bb9451328ea16373f5f9c1e3511da382d38a9d3205b5a4f8c5b822dfd`。

~~~~~~~~markdown
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
~~~~~~~~
