# AGENTS.md

本文件为所有 AI 编码代理（Claude Code / Codex / …）提供本仓库的工作指引。产品范围以 `docs/v1-prd.md` 为准；架构和实现遵守本文件的铁律及已确认架构规范；验证状态以 `docs/v1-validation-plan.md` 为准。`docs/README.md` 区分现行规范、调研、归档与原型；同级资料无法裁决的冲突列入待确认，不按日期或历史措辞自行取舍。后续编码与评审同时必读 [OpenSpec 代码与架构执行规范](openspec/CODE_STANDARDS.md)及其[工程治理规格](openspec/specs/engineering-governance/spec.md)和受影响的现行专项规范，局部修复同样适用；三者职责不相等：本文铁律、已确认 PRD 与专项规范继续各自负责，OpenSpec 提供工程基线并承载拟议变更，不能覆盖已确认决定，无法解释的冲突列入待确认并暂停依赖该决定的实现。

## 项目是什么

Aline —— Typora 式 Markdown 编辑器（Rust + GPUI，无 WebView），macOS / Windows / Linux 三端。技术路线继承 ElyMark 验证过的方案：**源文本是唯一事实来源 → 解析为携带"绘制文本 ↔ 源码字节"双向映射的展示树 → 按块虚拟化渲染 → 同一时刻最多一个块"打开"编辑 → 提交时对源文本做单区间补丁（单步撤销）→ 未编辑字节原样写回**。

当前状态：**尚无应用实现；现行规范见 `docs/README.md`。调研、原型、历史归档和维护审计材料不自动取得规范效力。原 `docs_bak/`、`reports/` 已经独立复核并按用户授权删除；历史证据与复查入口见 [删除准备维护审计](docs/maintenance/deletion-readiness/README.md)**。

## 文档地图（动手前必读）

| 文档 | 内容 | 何时读 |
|---|---|---|
| [开发流程](docs/development-process.md) | OpenSpec 变更、单项实现顺序、M0–M6 协作与阶段交接 | 开始实施或推进阶段前 |
| [OpenSpec 代码与架构执行规范](openspec/CODE_STANDARDS.md) | 必须遵循的工程基线：模块/接口大小、状态所有权、设计模式选用、Rust 约束与可检查交付 | 任何编码、重构和评审前 |
| [OpenSpec 工程治理规格](openspec/specs/engineering-governance/spec.md) | 工程要求及正常/拒绝/失败验收场景，与现行 R/S 规则配套 | 规划变更、实施与交付核对时 |
| [docs/engine-design.md](docs/engine-design.md) | 当前引擎契约：保真与补丁、解析、坐标映射（§7）、输入组/IME、有限 HTML（§11）、依赖准入（§18.2）；历史逐章处置见 §18.1 | 理解和实现纯逻辑机制前 |
| [docs/README.md](docs/README.md) | 当前权威关系、文档状态与阅读顺序 | 动手前先读 |
| [docs/v1-prd.md](docs/v1-prd.md) | V1 产品范围、非目标、行为与 AC-01～AC-20 | 判断一个功能该不该做 |
| [docs/v1-validation-plan.md](docs/v1-validation-plan.md) | V-01～V-09 实验、样本与阶段依赖，当前均未执行 | 判断技术和验收是否已有证据 |
| [docs/plugin-system.md](docs/plugin-system.md) | V1 内置插件的产品行为、设置归属、启停与架构边界 | 设计或实现插件功能和插件设置前 |
| [docs/ui-spec.md](docs/ui-spec.md) | 产品界面规范：布局/交互/视觉基线、图片只读 tab、菜单/设置/主题、键盘与中英文验收；UI-01～UI-06 已确认，原问题见 [UI 确认归档](docs/archive/2026-10-29-ui-decisions.md)，V 项仍未执行 | 实现或评审任何面向用户的界面 |
| [docs/theme-package-spec.md](docs/theme-package-spec.md) | 自定义主题包格式、可配置样式字段、继承规则和导入校验 | 设计或实现主题包字段与主题加载前 |
| [docs/v1-architecture.md](docs/v1-architecture.md) | 实施蓝图：crate 划分、模块骨架、各功能组实现方案、里程碑 M0–M6（历史蓝图处置见 §15） | 实现任何功能前 |
| [docs/rust-style-guide.md](docs/rust-style-guide.md) | Rust 代码规范：强制/建议/待验证、S/C 规则检查、工具链、坐标分型、错误/panic/unsafe、测试与提交；RS-01～RS-05/ED-01 已确认，原问题见 [确认归档](docs/archive/2026-10-29-rust-style-decisions.md) | 写任何 Rust 代码前 |
| [docs/architecture-guide.md](docs/architecture-guide.md) | 架构规范（【强制】/【建议】）：分层依赖、数据流真源、坐标系边界、命令事务、并发调度、缓存失效、主题/设置传播、插件与持久化边界 | 做架构层决策、动分层/依赖/传播机制前 |
| [docs/zed-notes.md](docs/zed-notes.md) | Zed逐主题复核与完整历史证据入口；旧“照搬”标签不作当前决定，主线与gpui-pre有API代差 | UI / GPUI 实现时仅作机制线索，须核对锁定源码及 V 验证 |
| [docs/decisions.md](docs/decisions.md) | 决策时间线 + 常设决定清单（含 2026-10-26 复核裁决） | 争议选型查先例 |
| [docs/wordcraft-study.md](docs/wordcraft-study.md) | WordCraft（storytold/wordcraft @ 0.11.0）调研：手写 markdown 解析的抗退化工程、性能案例库、保真方法论、敌意输入防御；原文实际 21 条提案，W22～W24 缺项，WC-01～WC-07 已确认A，技术验证仍未执行 | 实现自研扫描 pass、文件层/保存、安全上限、formatter 时对照 |

## 架构铁律（任何代码不得绕过）

完整展开与逐条判定依据（三参考项目先例锚定）见 [docs/architecture-guide.md](docs/architecture-guide.md)，以下为摘要：

1. **crate 依赖严格单向**：`app → aline-ui → aline-plugin → aline-edit → aline-markdown → aline-buffer`；`aline-source` 独立（buffer 不依赖它，文件 I/O 由 ui/app 直接编排）；`aline-app-core`（theme / i18n / settings）零 GPUI，谁都能依赖。引擎 crate（source / buffer / markdown / edit / app-core）**禁止依赖 gpui**，全部纯逻辑、可无头测试。
2. **文本永不重建，编辑永远是区间补丁**（`Vec<(Range<usize>, String)>`），块区间排除尾部换行。没有任何"从 AST 反序列化出 markdown"的路径。
3. **源文本字节偏移是全系统唯一持久坐标系**。三种坐标（源码 `src_off` / 绘制 `drawn_off` / UTF-16 `utf16_off`）之间的每一次转换必须经过映射层（`Inline::source_at` / `drawn_at` / `aligned`），禁止调用点手工加减偏移；偏移变量禁止裸名 `pos` / `idx`。RS-02 已确认：跨模块/跨 crate 的上述偏移和范围使用 newtype/受检封装，模块内部单坐标可明确命名 `usize`；第 2 条补丁表示保持不变，在表示边界受检转换，不新增反向依赖。
4. **存储保真**：未编辑 → 原文件字节原样写回；编辑过 → 只重放 BOM / CRLF 多数派。逐字节保真测试是发布门禁。
5. **解析器锁定 pulldown-cmark**（选型已确认；历史项目实测不是 Aline 性能成绩），缺失语法用带字节映射的预处理改写器补齐；**禁止引入第二个 markdown 解析器**。
6. **引擎 crate 禁 `unwrap()`（含测试/示例/基准）、用户输入永不 panic、零线程零全局状态**；新机制先有测试再做 UI，不变量优先补 `debug_assert!` 而不是注释。RS-03 已确认：引擎生产代码不用 `expect` / `panic!` / `unreachable!` / 发布期 `assert!`，使用受检错误/拒绝；测试预期断言允许，测试夹具/UI 初始化的例外须逐处证明并最小记录，不能放宽输入安全或引擎 unwrap 禁令。
7. **所有面向用户字符串从第一天走 `t("key")`**（zh-CN + en TOML catalog，rust-embed 打包），禁止硬编码；引擎层错误一律是枚举，翻译只发生在 UI 层。
8. **注释与文档用中文，标识符一律英文**；TODO 格式 `// TODO(xian): 说明`；提交遵循 Conventional Commits，单 crate scope 用完整 crate 名（如 `feat(aline-markdown): ...`）。RS-05 已确认：纯文档用 `docs`，公共工程配置/规则用 `workspace`，应用包用实际 package 名；跨 crate 必要原子变更用主责 crate scope，正文列其它影响范围。非 crate scope 限现有约定，新增须记录，不省略 scope 或强行拆散原子变更。
9. **自有代码默认 `unsafe_code = "forbid"`**（RS-04）；真实平台需要时另提最小例外方案，说明必要性、有效配置、保留其它 lint 的方式，经具体裁决、独立安全评审和验证后实施。本次基线确认不自动批准未来 unsafe/unsafe impl，细则见 Rust 规范第 10 节。

## 构建与检查

当前仓库仅有文档。M0 建立 Cargo workspace 后，提交前必跑（规范出处：rust-style-guide §2）：

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

- 跑单个测试：`cargo test -p <crate> <test_name>`（测试命名 `被测行为_场景`，如 `edit_single_patch_creates_one_undo_step`）。
- RS-01 已确认：默认 lint + 逐项启用铁律/高价值 lint，`pedantic` 按项引入，不整组开启；保留 `-D warnings`，不全局放行坐标转换。具体 lint 级别、作用范围、成员继承及正反样例在 M0 验证登记，当前尚无配置或 CI。
- gpui 锁死快照版本（`gpui-pre`），升级是显式动作；V1 不引入 notify / regex / tokio 等运行时与额外依赖（当前准入见 engine-design §18.2 和 v1-architecture §3.2；历史依赖表不是预装许可）。

## 工作方式

- **规范是实现约束（不改变上述权威分工）**：按 `openspec/CODE_STANDARDS.md` 和工程治理规格检查职责、依赖、唯一状态所有者及可测试边界；模块行数仅提示复核，不机械拆文件或 crate。相关 change 的 design 必须解释模式解决的问题、最简单可行方案、代价和验证；允许直接使用具体类型/函数，禁止无需求的模式包装。工程规则调整同步本文入口、OpenSpec 与相关现行文档，不能以局部 allow 或 design 绕过铁律。
- **按里程碑推进**（v1-architecture 第 13 章）：M0 引擎地基 → M1 单文档只读 → M2 可编辑核心（可日用）→ M3 命令与表格 → M4 插件与主题 → M5 应用壳 → M6 收尾。阶段可形成内部预览，验收按三平台分别取证；先 macOS 后 Windows/Linux 验证，正式 V1 三平台同发（PRD §9、V-01）。
- 新增依赖先对照 engine-design §18.2 和 v1-architecture §3.2 的准入规则，同类功能不得引入第二个库。
- 实现期对照：ElyMark 完整历史关键文件索引已保全于 `docs/archive/migration-evidence/engine-design.md`，当前入口见 engine-design §18.3；Zed 克隆在 `/Users/xian/Project/book/zed`（gpui 新架构与锁定的 gpui-pre 存在 API 代差，机制线索参考 docs/zed-notes.md §0及其完整历史证据，具体 API 及性能结论须重新取证）。Aline 全部自写实现，外部源码只作对照参考。
- 外观与交互参照 MarkFlow（`/Users/xian/Project/book/MarkFlow`）：设置模型、导出主题、图片粘贴、自动保存/外部修改检测等功能语义从它迁移（迁语义不迁代码）。其 2026-10-25 性能诊断（128KB 文档卡顿三病根，issue #458/#459/#460）已并入 architecture-guide §11 作为反面实证；方向研究报告全文在 MarkFlow 仓库 `docs/superpowers/specs/2026-10-26-aline-gpui-rebuild-design.md`。
