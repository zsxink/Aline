# AGENTS.md

本文件为所有 AI 编码代理（Claude Code / Codex / …）提供本仓库的工作指引。产品范围以 `docs/v1-prd.md` 为准；架构和实现遵守本文件的铁律及已确认架构规范；验证状态以 `docs/v1-validation-plan.md` 为准。`docs/README.md` 区分现行规范、调研、归档与原型；同级资料无法裁决的冲突列入待确认，不按日期或历史措辞自行取舍。后续编码与评审同时必读 [OpenSpec 代码与架构执行规范](openspec/CODE_STANDARDS.md)及其[工程治理规格](openspec/specs/engineering-governance/spec.md)和受影响的现行专项规范，局部修复同样适用；三者职责不相等：本文铁律、已确认 PRD 与专项规范继续各自负责，OpenSpec 提供工程基线并承载拟议变更，不能覆盖已确认决定，无法解释的冲突列入待确认并暂停依赖该决定的实现。

## 项目是什么

Aline —— Typora 式 Markdown 编辑器（Rust + GPUI，无 WebView），macOS / Windows / Linux 三端。技术路线继承 ElyMark 验证过的方案：**源文本是唯一事实来源 → 解析为携带"绘制文本 ↔ 源码字节"双向映射的展示树 → 按块虚拟化渲染 → 同一时刻最多一个块"打开"编辑 → 提交时对源文本做单区间补丁（单步撤销）→ 未编辑字节原样写回**。

当前状态：**已有八成员 workspace、引擎与无头宿主基础，以及 M1 的原生单窗口只读界面（打开/渲染/点击映射/滚动，已在 macOS arm64 取证）；尚无编辑、保存/重开、插件运行时与发布产物。M0 出口已由用户 2026-11-02 验收裁决通过（验收豁免；逐包人工复核与 E2/E4/E5/E7 整项等未执行项见验证计划，不得写成已验证）；V-01～V-09 均未整项通过，已执行与未测的分项见 `docs/v1-validation-plan.md`。现行规范见 `docs/README.md`，验证状态见 `docs/v1-validation-plan.md`。调研、原型与历史材料不自动取得规范效力；旧过程报告和维护审计已按用户要求清理，必要决定见 `docs/decisions.md`，不恢复整套目录。**

## 文档地图（动手前必读）

| 文档 | 内容 | 何时读 |
|---|---|---|
| [开发流程](docs/development-process.md) | OpenSpec 变更、单项实现顺序、M0–M6 协作与阶段交接 | 开始实施或推进阶段前 |
| [OpenSpec 代码与架构执行规范](openspec/CODE_STANDARDS.md) | 必须遵循的工程基线：模块/接口大小、状态所有权、设计模式选用、Rust 约束与可检查交付 | 任何编码、重构和评审前 |
| [OpenSpec 工程治理规格](openspec/specs/engineering-governance/spec.md) | 工程要求及正常/拒绝/失败验收场景，与现行 R/S 规则配套 | 规划变更、实施与交付核对时 |
| [docs/engine-design.md](docs/engine-design.md) | 当前引擎契约：保真与补丁、解析、坐标映射（§7）、输入组/IME、有限 HTML（§11）、依赖准入（§18.2）；历史逐章处置见 §18.1 | 理解和实现纯逻辑机制前 |
| [docs/README.md](docs/README.md) | 当前权威关系、文档状态与阅读顺序 | 动手前先读 |
| [docs/v1-prd.md](docs/v1-prd.md) | V1 产品范围、非目标、行为与 AC-01～AC-20 | 判断一个功能该不该做 |
| [docs/v1-validation-plan.md](docs/v1-validation-plan.md) | V-01～V-09 实验、样本与阶段依赖；均未整项通过，已执行与未测的分项以本文档为准 | 判断技术和验收是否已有证据 |
| [docs/plugin-system.md](docs/plugin-system.md) | V1 内置插件的产品行为、设置归属、启停与架构边界 | 设计或实现插件功能和插件设置前 |
| [docs/ui-spec.md](docs/ui-spec.md) | 产品界面规范：布局/交互/视觉基线、图片只读 tab、菜单/设置/主题、键盘与中文界面验收（决定 §24：人工/原生取证只做中文，双语 catalog 自动校验保留）；§4.1 原型布局对照（实现参照）；UI-01～UI-06 已确认，原问题见 [UI 确认归档](docs/archive/2026-10-29-ui-decisions.md)，V 项均未整项通过 | 实现或评审任何面向用户的界面 |
| [docs/theme-package-spec.md](docs/theme-package-spec.md) | 自定义主题包格式、可配置样式字段、继承规则和导入校验 | 设计或实现主题包字段与主题加载前 |
| [docs/v1-architecture.md](docs/v1-architecture.md) | 实施蓝图：crate 划分、模块骨架、各功能组实现方案、里程碑 M0–M6（历史蓝图处置见 §15） | 实现任何功能前 |
| [docs/rust-style-guide.md](docs/rust-style-guide.md) | Rust 代码规范：强制/建议/待验证、S/C 规则检查、工具链、坐标分型、错误/panic/unsafe、测试与提交；RS-01～RS-05/ED-01 已确认，原问题见 [确认归档](docs/archive/2026-10-29-rust-style-decisions.md) | 写任何 Rust 代码前 |
| [docs/architecture-guide.md](docs/architecture-guide.md) | 架构规范（【强制】/【建议】）：分层依赖、数据流真源、坐标系边界、命令事务、并发调度、缓存失效、主题/设置传播、插件与持久化边界 | 做架构层决策、动分层/依赖/传播机制前 |
| [docs/zed-notes.md](docs/zed-notes.md) | Zed逐主题复核与完整历史证据入口；旧“照搬”标签不作当前决定，主线与gpui-pre有API代差 | UI / GPUI 实现时仅作机制线索，须核对锁定源码及 V 验证 |
| [docs/decisions.md](docs/decisions.md) | 决策时间线 + 常设决定清单（含 2026-10-26 复核裁决） | 争议选型查先例 |
| [docs/wordcraft-study.md](docs/wordcraft-study.md) | WordCraft（storytold/wordcraft @ 0.11.0）调研：手写 markdown 解析的抗退化工程、性能案例库、保真方法论、敌意输入防御；原文实际 21 条提案，W22～W24 缺项，WC-01～WC-07 已确认A，技术验证仍未执行 | 实现自研扫描 pass、文件层/保存、安全上限、formatter 时对照 |
| [docs/quiet-md-study.md](docs/quiet-md-study.md) | quiet-md 工程实践调研与 Aline 采纳方案（研究文档，非权威）：fuzz/不变量、性能报告、保存/恢复、IME、语义 token、中文排版、离屏证据；QMD-01 已把方法回写规范，治理 delta 已并入 change `m1-single-window-readonly`，W1～W7 已分别建立独立 change，W4/W5 保留 M2 前置（与 WordCraft W22～W24 不共用编号空间） | 做引擎测试、保存恢复、字体/主题或补强验证证据时对照；实施前按开发流程逐项建档 |

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

当前仓库已有 8 成员 Cargo workspace：`app`、`crates/aline-app-core`、`crates/aline-source`、`crates/aline-buffer`、`crates/aline-markdown`、`crates/aline-edit`、`crates/aline-plugin`、`crates/aline-ui`。Rust 提交前必跑以下门禁（规范出处：rust-style-guide §2）：

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

- 跑单个测试：`cargo test -p <crate> <test_name>`（测试命名 `被测行为_场景`，如 `edit_single_patch_creates_one_undo_step`）。
- RS-01 已确认：默认 lint + 逐项启用铁律/高价值 lint，`pedantic` 按项引入，不整组开启；保留 `-D warnings`，不全局放行坐标转换。根 `Cargo.toml` 已配置 `[workspace.lints]`，8 个成员均继承；工程/lint/安全/公共 API 守卫入口及自测见 `scripts/check_engineering.py`、`scripts/check_lints.py`、`scripts/check_safety.py`、`scripts/check_public_api.py`。`.github/workflows/m0.yml` 已配置 `workflow_dispatch` 和推送 `main` 时运行；远端 Actions 实际运行状态未核实。此配置状态不表示 M0 总验收完成，状态见 `docs/v1-validation-plan.md`。
- gpui 锁死快照版本（`gpui-pre`），升级是显式动作；V1 不引入 notify / regex / tokio 等运行时与额外依赖。2026-10-30 已批准的例外仅为架构 §3.2 登记的 GPUI 0.3.9→regex 精确路径与非桌面 WASI p2 绑定路径；2026-11-02 用户另批准 M1 macOS 后端候选 `gpui-pre-macos@0.3.9`、`sys-locale@0.3.2`、三个精确 MPL 包例外（`cbindgen@0.28.0`、`option-ext@0.2.0`、`dwrote@0.11.5`）、`libloading@0.8.9` 的精确 ISC metadata-only 完整路径例外、`wasi@0.11.1` 与 `wasip2@1.0.4` 原列及新增的精确 metadata 路径边，并批准七场景 M1 GPUI 组合图指纹。`wasip2`、`dwrote` 与 `libloading` 可按各自精确约束出现在 metadata-only，desktop target tree 不得选中。守卫已按逐节点身份/checksum/边/target 更新，真实 all 与七目标场景通过，合成自测通过；这不代表 Metal build/窗口/跨平台构建已验证。Aline 自有直接依赖、五引擎及其它路径继续禁止。按来源/版本/校验和/边/target/features 审计，不全包或前缀放行（当前状态见 engine-design §18.2 和 v1-architecture §3.2；历史依赖表不是预装许可）。

## 工作方式

- **规范是实现约束（不改变上述权威分工）**：按 `openspec/CODE_STANDARDS.md` 和工程治理规格检查职责、依赖、唯一状态所有者及可测试边界；模块行数仅提示复核，不机械拆文件或 crate。相关 change 的 design 必须解释模式解决的问题、最简单可行方案、代价和验证；允许直接使用具体类型/函数，禁止无需求的模式包装。工程规则调整同步本文入口、OpenSpec 与相关现行文档，不能以局部 allow 或 design 绕过铁律。
- **不写本机绝对路径**：文档、证据与脚本只写仓库内相对路径或 `<repo>`、`<tmp>`、`<home>` 等占位符，不记录本机用户目录、临时目录或仓库外磁盘位置；生成证据的脚本统一经 `scripts/evidence_paths.py` 归一化，读回时用 `resolve_path` 还原。shebang 的解释器行是唯一例外。
- **W1 隔离验证例外（2026-11-09）**：用户批准决定 §27 的精确 `gpui_macos=[font-kit,test-support]` 测试图，仅限 logs 内 macOS arm64 spike；生产清单/锁/features 不变。精确图、五条分支、72 个新增 metadata 包及 9 包变化以固定批准资产核对，漂移重新审计，不作全包或许可证泛化许可。`option-ext@0.2.0` 的已批准精确路径可在 macOS tree 选中；`dwrote/libloading` 仍不得被 desktop tree 选中。
- **提交可复现资产，运行产物留本地**：按 [开发流程 §5](docs/development-process.md#5-仓库产物与提交边界)保留代码、单元/集成测试、独立金样、最小失败输入和可复跑脚本；命令记录、stdout/stderr、临时夹具及 Agent 逐轮 report/review 等写 `logs/validation/...`，不因是 `.md` 就提交。换会话只在 `tmp/<任务>-resume.md` 维护一份不追踪的当前交接；必要决定回写规范，平台/手工验证摘要回填验证计划。旧过程材料按需查 Git，不重建报告目录或改写历史。
- **按里程碑推进**（v1-architecture 第 13 章）：M0 引擎地基 → M1 单文档只读 → M2 可编辑核心（可日用）→ M3 命令与表格 → M4 插件与主题 → M5 应用壳 → M6 收尾。阶段可形成内部预览，验收按三平台分别取证；先 macOS 后 Windows/Linux 验证，正式 V1 三平台同发（PRD §9、V-01）。
- 新增依赖先对照 engine-design §18.2 和 v1-architecture §3.2 的准入规则，同类功能不得引入第二个库。2026-11-02 起 `scripts/check_engineering.py` 默认执行逐依赖准入检查（用途分组/选包/许可证），传 `--ledger <path>` 时另存逐包账本，失败关闭；实际执行与限制见架构 §3.2 R14 记录，不把策略核对写成法律意见或跨平台构建证明。
- 实现期对照：ElyMark 完整历史关键文件索引已保全于 `docs/archive/migration-evidence/engine-design.md`，当前入口见 engine-design §18.3；Zed 克隆放在仓库外的参考目录，不随本仓库分发（gpui 新架构与锁定的 gpui-pre 存在 API 代差，机制线索参考 docs/zed-notes.md §0及其完整历史证据，具体 API 及性能结论须重新取证）。Aline 全部自写实现，外部源码只作对照参考。
- 外观与交互参照 MarkFlow（[`MarkFlow`](https://github.com/zsxink/MarkFlow)）：设置模型、导出主题、图片粘贴、自动保存/外部修改检测等功能语义从它迁移（迁语义不迁代码）。其 2026-10-25 性能诊断（128KB 文档卡顿三病根，issue #458/#459/#460）已并入 architecture-guide §11 作为反面实证；方向研究报告全文在 MarkFlow 仓库 `docs/superpowers/specs/2026-10-26-aline-gpui-rebuild-design.md`。
- **原生 UI 布局与控件邻接对照历史原型**：`docs/prototype/legacy/` 的 HTML/CSS 是原生界面的布局参照（区域构成、自上而下顺序、控件邻接、图标语义与交互意图），逐区域映射与冲突裁决见 [UI 规范 §4.1](docs/ui-spec.md#41-原型布局对照实现参照)。原型决定“怎么摆”，PRD 与已确认规范决定“有什么”，冲突时以 PRD/已确认规范为准；不从原型反推新功能、键位、默认值或运行时主题格式，也不把浏览器行为当原生能力证明。参照不等于拷贝：不把 `legacy/` 复制进应用 `src/` 或 `resources/themes/`。
