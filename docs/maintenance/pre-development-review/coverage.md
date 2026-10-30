# 实际阅读覆盖与证据边界

日期：2026-10-30（Asia/Shanghai）。以下是本轮主审与只读专项复核的合并覆盖，不是引用此前审计的覆盖声明。主审负责产品/阶段、专项结果复验和全部修订；专项分别核引擎事务、UI/插件/主题、Rust/研究/决定、资产/工具/删除后证据。各专项未写入文件。

## 完整正文阅读

大文件按行段/章节拆读；合并输出发生截断后补读缺段。下列行数为修订前快照，定位以章节为准。

| 范围 | 本轮实际覆盖 |
|---|---|
| 根入口 | AGENTS.md、CLAUDE.md、docs/README.md 全文；实际目录与 git status；.gitignore、.impeccable/config.json |
| 产品/验证/架构 | v1-prd.md 1–400、v1-validation-plan.md 1–117、v1-architecture.md 1–843 全文 |
| 引擎/工程 | engine-design.md 1–617、architecture-guide.md 1–470、rust-style-guide.md 1–467 全文 |
| UI/插件/主题 | ui-spec.md 1–588、plugin-system.md 1–411、theme-package-spec.md 1–655 全文，包括字段、示例、错误/拒绝和测试矩阵 |
| 决定与研究 | decisions.md 1–525、wordcraft-study.md 1–253、zed-notes.md 1–62 全文 |
| 八份确认归档 | 2026-10-28-v1-open-questions.md 277 行；2026-10-29-v1-architecture-decisions.md 71 行；plugin-extensibility.md 31 行；rust-style-decisions.md 126 行；ui-decisions.md 128 行；plugin-decisions.md 63 行；theme-decisions.md 141 行；2026-10-30-migration-decisions.md 194 行。核对采纳结论和保留原问题，未把旧待确认当新阻塞 |
| 迁移原文/索引 | migration-evidence/README.md 31 行和 index.json 424 行；旧 Rust 240 行、WordCraft 379 行、Zed 252 行、架构规范证据 203 行全文。旧引擎按本轮争议和现行引用定位补读，范围见下表，不声称旧引擎 1266 行全文阅读 |
| 原型说明 | prototype/README.md、review.md、inventory.md、open-questions.md 全文；legacy/README.md 与历史生成器全文 |
| 删除复核仍影响开发的材料 | deletion-readiness/README.md、report.md、spec-review.md、dependency-review.md、prototype-review.md、migration-content.md、reports-review.md、deletion-execution.md，以及 prior-evidence/README.md、prior-review.md 全文。其“通过”声明与本轮结果分开 |
| 资产说明 | assets/README.md、provenance.md；审计期间出现的 app-icon/README.md 补读全文。旧 PNG 身份重新核对；新图标产物只核说明、集合与文件身份，没有重做其加工或平台验收 |
| 工具源代码 | deletion-readiness/check.py 308 行、run-checks.py 39 行；prototype/tools/verify-reference.py 111 行、legacy/tools/generate-theme-previews.py 84 行全文；本轮新增 run-checks.py 逐段复验 |

## 定位阅读、机器遍历与未覆盖

| 对象 | 实际方法与限制 |
|---|---|
| 旧引擎/旧架构原证据 | 旧架构规范全文核到映射双射、drop_open、栈深 dirty 与调度原文；旧引擎补读保全文件 928–1183（摘录原行 L1328–1352 依赖表、L1587–1630 Ely 索引、L1427–1435 平台输入风险、L1444–1586 HTML 实施/限制/失败测试/决定/后续研究），另按身份/分层定位。WordCraft 旧 W8 通用 catch 原文亦已读；结合当前替代规则核对，不恢复为 API。未逐行重审全部旧引擎或所有外部源码 |
| migration-evidence 其余选段 | 索引/用途/引用闭包与保留片段哈希重新检查；对无本轮争议部分未逐段人工重读。不把文件存在/哈希相同写成语义全量认证 |
| prototype/legacy 的 HTML/CSS/JS | HTML 的样式/脚本加载顺序，theme-library.js 的索引→manifest→配置/预览 fetch，存储调用及 URL 路径针对性阅读；机器逐个读取 45 文件做哈希/HTTP/结构核对，6 JS 另做语法检查。未人工逐行重读全部脚本/样式，没有执行浏览器完整交互或真实断网演练 |
| 两份架构 SVG | XML 解析后读取全部 title/desc/metadata/text 文本，核对八 crate、六提供者、未来外部边界及“逻辑箭头非 Cargo 依赖”图例；核查外部资源属性。Quick Look 生成缩略图并查看，但缩略图出现方形裁切，未将其作为完整画布视觉 QA。图中旧版本 metadata 是制作时来源，不自动声明当前能力 |
| inventory/处置/历史扫描 JSON/CSV | 结构遍历、身份/哈希与当前目标检查；本轮需求 CSV 独立逐项编写。旧 spec-trace/acceptance-trace 的数量检查不作本轮语义依据。backup-references.tsv 是旧字符串快照，仅头尾和相关项定位阅读 |
| prior-evidence 历史初审/收尾/331主题原报告 | 读取外层入口及当前独立复核的相关发现，保全哈希重新检查；未全文人工重读所有围栏内旧报告。用户明确这些不是权威，未据此决定开发基线 |
| 本地参考项目 | 本轮不执行或认证 ElyMark/MarkFlow/Zed/WordCraft 源码/API/性能/许可证。引用的开发方法可独立理解；后续使用某源码机制须核对真实锁定版本 |
| 外链 | 现行顶层文档外链重取页面/HTTP与锚点；Apple/Microsoft 新图标参考仅检查可达。历史围栏、W3C namespace、原型 GitHub 点击导航分类处理，不是启动依赖；未把远端页面可达当权利证明 |

## 工作树归属

[起始状态](./initial-git-status.txt)含 AGENTS/CLAUDE 修改、已授权旧目录删除、整个 docs 未跟踪；本轮未 stage、commit、恢复或重建删除目录。[基线文件身份](./baseline-files.json)用于本轮逐文件对照，不能因为 docs 未跟踪而把全部 docs 归为本轮新写。

审计期间 assets/app-icon 与 assets/README.md 出现外部并行变更，本轮只补读、保留并在最终文件对比中单列，未覆盖或归功于本审计。文档修订由主审集中完成。最终差异及其归属见 changes.json；若此后工作树再变化，应重新运行检查并复核结论适用的文件身份。

## 不可推定通过的检查

未执行 Cargo fmt/clippy/test（无 workspace），未建立 Cargo/依赖/CI 配置；未做 GPUI/IME、真实原生菜单/文件关联、字体隔离/加载/透明、图表公式/图片解码、真实导出阅读器、保存/恢复故障注入、性能/资源限额、Windows/Linux 设备验证或第三方许可调查。V-01～V-09 与 M0 工具实验均未由本轮通过；修订文字和核对历史字节不替代它们。
