# AGENTS.md

本文件为所有 AI 编码代理（Claude Code / Codex / …）提供本仓库的工作指引。规则与 `docs/` 冲突时，以 `docs/` 为准。

## 项目是什么

Aline —— Typora 式 Markdown 编辑器（Rust + GPUI，无 WebView），macOS / Windows / Linux 三端。技术路线继承 ElyMark 验证过的方案：**源文本是唯一事实来源 → 解析为携带"绘制文本 ↔ 源码字节"双向映射的展示树 → 按块虚拟化渲染 → 同一时刻最多一个块"打开"编辑 → 提交时对源文本做单区间补丁（单步撤销）→ 未编辑字节原样写回**。

当前状态：**尚无代码，`docs/` 是唯一内容，也是唯一权威**。

## 文档地图（动手前必读）

| 文档 | 内容 | 何时读 |
|---|---|---|
| [docs/engine-design.md](docs/engine-design.md) | 引擎机制设计（ElyMark 逆向）：解析管线、sources 映射、七条保真不变量、折叠编辑器阶梯 A→B→B+→C、HTML 机制（§1.10）与实施方案（§2.17）、依赖清单 | 理解任何机制"为什么这样设计" |
| [docs/v1-features.md](docs/v1-features.md) | V1 功能清单，以及 V1 明确不做的范围 | 判断一个功能该不该做 |
| [docs/plugin-system.md](docs/plugin-system.md) | V1 内置插件的产品行为、设置归属、启停与架构边界 | 设计或实现插件功能和插件设置前 |
| [docs/ui-spec.md](docs/ui-spec.md) | UI 与交互验收规范：MarkFlow 对照范围、Aline 品牌、浅色主题、代码块细节和 Markdown 展示夹具 | 实现或评审任何面向用户的界面 |
| [docs/v1-architecture.md](docs/v1-architecture.md) | 实施蓝图：crate 划分、模块骨架、各功能组实现方案、里程碑 M0–M6（取代 engine-design §2.13） | 实现任何功能前 |
| [docs/rust-style-guide.md](docs/rust-style-guide.md) | Rust 代码规范（【强制】/【建议】两级）：工具链、命名、坐标系、错误处理、测试、提交格式、参考项目实证条款 | 写任何 Rust 代码前 |
| [docs/architecture-guide.md](docs/architecture-guide.md) | 架构规范（【强制】/【建议】）：分层依赖、数据流真源、坐标系边界、命令事务、并发调度、缓存失效、主题/设置传播、插件与持久化边界 | 做架构层决策、动分层/依赖/传播机制前 |
| [docs/zed-notes.md](docs/zed-notes.md) | Zed 源码实践笔记：GPUI 机制、逐条"照搬 / 改造 / 不采用"清单 | 实现 UI / GPUI 层前 |
| [docs/decisions.md](docs/decisions.md) | 决策时间线 + 常设决定清单（含 2026-10-26 复核裁决） | 争议选型查先例 |
| [docs/wordcraft-study.md](docs/wordcraft-study.md) | WordCraft（storytold/wordcraft @ 0.11.0）调研：手写 markdown 解析的抗退化工程、性能案例库、保真方法论、敌意输入防御 + 24 条借鉴提案（W1–W24，含里程碑归位） | 实现自研扫描 pass、文件层/保存、安全上限、formatter 时对照 |

## 架构铁律（任何代码不得绕过）

完整展开与逐条判定依据（三参考项目先例锚定）见 [docs/architecture-guide.md](docs/architecture-guide.md)，以下为摘要：

1. **crate 依赖严格单向**：`app → aline-ui → aline-plugin → aline-edit → aline-markdown → aline-buffer`；`aline-source` 独立（buffer 不依赖它，文件 I/O 由 ui/app 直接编排）；`aline-app-core`（theme / i18n / settings）零 GPUI，谁都能依赖。引擎 crate（source / buffer / markdown / edit / app-core）**禁止依赖 gpui**，全部纯逻辑、可无头测试。
2. **文本永不重建，编辑永远是区间补丁**（`Vec<(Range<usize>, String)>`），块区间排除尾部换行。没有任何"从 AST 反序列化出 markdown"的路径。
3. **源文本字节偏移是全系统唯一坐标系**。三种坐标（源码 `src_off` / 绘制 `drawn_off` / UTF-16 `utf16_off`）之间的每一次转换必须经过映射层（`Inline::source_at` / `drawn_at` / `aligned`），禁止调用点手工加减偏移；偏移变量禁止裸名 `pos` / `idx`。
4. **存储保真**：未编辑 → 原文件字节原样写回；编辑过 → 只重放 BOM / CRLF 多数派。逐字节保真测试是发布门禁。
5. **解析器锁定 pulldown-cmark**（性能实测已定），缺失语法用带字节映射的预处理改写器补齐；**禁止引入第二个 markdown 解析器**。
6. **引擎 crate 禁 `unwrap()`、用户输入永不 panic、零线程零全局状态**；新机制先有测试再做 UI，不变量优先补 `debug_assert!` 而不是注释。
7. **所有面向用户字符串从第一天走 `t("key")`**（zh-CN + en TOML catalog，rust-embed 打包），禁止硬编码；引擎层错误一律是枚举，翻译只发生在 UI 层。
8. **注释与文档用中文，标识符一律英文**；TODO 格式 `// TODO(xian): 说明`；提交遵循 Conventional Commits，scope 用 crate 名（如 `feat(aline-markdown): ...`）。

## 构建与检查

当前仓库仅有文档。M0 建立 Cargo workspace 后，提交前必跑（规范出处：rust-style-guide §2）：

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

- 跑单个测试：`cargo test -p <crate> <test_name>`（测试命名 `被测行为_场景`，如 `edit_single_patch_creates_one_undo_step`）。
- gpui 锁死快照版本（`gpui-pre`），升级是显式动作；V1 不引入 notify / regex / tokio 等运行时与额外依赖（完整清单 engine-design §2.12）。

## 工作方式

- **按里程碑推进**（v1-architecture 第 13 章）：M0 引擎地基 → M1 单文档只读 → M2 可编辑核心（可日用）→ M3 命令与表格 → M4 插件与主题 → M5 应用壳 → M6 收尾。每个 M 独立可发布，验收标准三平台分别过。
- 新增依赖先对照 engine-design §2.12 清单，同类功能不得引入第二个库。
- 实现期对照：ElyMark 关键文件索引在 engine-design.md 附录；Zed 克隆在 `/Users/xian/Project/book/zed`（gpui 新架构与锁定的 gpui-pre 存在 API 代差，机制级结论参考 zed-notes §0）。Aline 全部自写实现，外部源码只作对照参考。
- 外观与交互参照 MarkFlow（`/Users/xian/Project/book/MarkFlow`）：设置模型、导出主题、图片粘贴、自动保存/外部修改检测等功能语义从它迁移（迁语义不迁代码）。其 2026-10-25 性能诊断（128KB 文档卡顿三病根，issue #458/#459/#460）已并入 architecture-guide §11 作为反面实证；方向研究报告全文在 MarkFlow 仓库 `docs/superpowers/specs/2026-10-26-aline-gpui-rebuild-design.md`。
