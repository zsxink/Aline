# Tasks

任务按实际实施与验证状态逐项标记；用户已授权 apply，不包含 commit/push/archive。M0 的 2026-11-02 用户验收豁免继续有效，证据缺口保留。**纯机制 1～3 已完成无头部分；4.1 的后端候选、精确许可证/metadata 路径与 GPUI 指纹均获具体裁决，准入策略、7 场景真实图及完整自测已通过。4.2 的锁定离线 app build 与 macOS 原生空窗显示/退出已验证；4.3 两种系统首选语言真实启动已验证；4.4 文件打开/取消/关闭真实操作已接通。5.1 变高虚拟阅读和精确 1 MiB 双视图首尾滚动已取得 macOS 实测证据。5.2 命中、5.3 锚点恢复/预算边界、内容与导航/字体视图尚未完成。**

Requirement 引用缩写仅用于追踪，不另建编号规格：

| 缩写 | capability / Requirement |
|---|---|
| U1 | ui/readonly-document / 原生单窗只读文档生命周期 |
| U2 | ui/readonly-document / 两种只读视图共享源码位置 |
| U3 | ui/readonly-document / 变高块和源码行的可交互虚拟阅读 |
| U4 | ui/readonly-document / 点击命中只产生受检源码定位 |
| U5 | ui/readonly-document / Markdown 有限 HTML 与保护区展示 |
| U6 | ui/readonly-document / 大纲目录脚注主动导航 |
| U7 | ui/readonly-document / 普通代码行号与双语键盘基线 |
| E1 | engine/markdown / 只读结构展示与局部命中保留统一来源 |
| E2 | engine/markdown / 导航索引为纯数据且绑定源快照（M1 修改标签等价配对） |
| D1 | engine/document-core / 只读会话不依赖打开编辑块 |
| G1 | workspace/engineering-gates / M1 只读出口按分项和平台交接 |

所有命令在仓库根执行。Cargo 使用已登记工具链和已核对的隔离缓存，不自动安装/下载；需要时在命令前设置 `CARGO_HOME=<cargo-cache>`（占位符，证据中统一归一化）。原始 stdout/stderr、命令/环境、截图/基准及审查原文统一放 `logs/validation/m1-single-window-readonly/<run-id>/<group>/`，下文简写 `<evidence>/<group>/`；`<evidence>` 必须解析到上述 logs/validation 下的唯一运行目录。路径统一经 `scripts/evidence_paths.py` 处理。当前交接如需只维护 `tmp/m1-single-window-readonly-resume.md` 一份不追踪文件。

新测试 target、脚本名称和参数在下文标为“拟新增”；先实现该任务的入口再运行，不能把计划命令记为现成能力。组内先落失败测试/独立预期再实现，随后更新对应公共契约文档；最终组只负责跨组复核。

## 1. Fixtures and Contract Cases

关联 U1～U7/E1/G1，AC-01/09 阅读分项，V-03/V-07；CODE_STANDARDS §2～4/6，R-4/12，S-5/12。

- [x] 1.1 在 `crates/aline-markdown/tests/fixtures/` 建五组真实 Markdown 及独立展示/来源预期：基础块/行内，嵌套列表/引用/Alerts，代码/表格，公式/Mermaid 原文，图片引用/有限 HTML；包含转义管道、实体、BOM/CRLF/孤立 CR/无尾换行、空文档和灰区。先写消费这些预期的测试；验收 `cargo test -p aline-markdown --test readonly_presentation`（拟新增）确实读取每组而非孤立存夹具；证据 `<evidence>/fixtures/`。
- [x] 1.2 用最小样例区分 SoftBreak/HardBreak/布局软折行、HTML void/未知/交叉嵌套/过滤集、未闭合 Front matter、目录大小写/重复标题、脚注缺失/重复/大小写/嵌套目标；记录现状与依据 PRD/引擎契约得出的独立预期及待冻结项。解析器已配对而导航空目标的大小写脚注必须列为失败回归，关联 E2，不能当正常缺失；验证现有 `cargo test -p aline-markdown --test html --test navigation --test mapping --test protection`。其他不能裁决语义列入 proposal 待确认并暂停依赖项，不将现状复制成金样。SoftBreak 可见换行、非支持 void 原标签、未知容器整段原文已于 2026-11-03 由用户裁决，新增 `html` 失败回归并实现后转绿；更新 `docs/engine-design.md` §7/11 及 V-07 夹具。其余 V-07 项仍不宣称完整通过。
- [x] 1.3 建可复跑固定 seed/内容复杂度的 32/128 KiB、1/4 MiB、>4 MiB、许多短块/单巨段/长行/巨表/密集实体样本生成器及大小/首尾锚点独立预期；稳定生成器/小金样放 `scripts/fixtures/` 或对应 tests，生成大副本放 logs。验收拟新增 `python3 scripts/generate_m1_fixtures.py --output-dir <evidence>/fixtures-generated` 重复生成得到同内容与已知大小，失败非零，不能依赖旧 logs 作为输入；回填 V-05 样本说明。

## 2. Pure Presentation and Mapping

依赖 1.1/1.2 的已裁决预期；关联 E1/U3～U7，AC-09 展示/AC-20 行号分项，V-07；R-2/4/12，S-3/5/7/12/14。

- [x] 2.1 在既有 pulldown-cmark 构建链增加只读嵌套语义/Markdown 样式、Alerts 与表格行列/对齐及代码正文行数据，保持全文来源/保护/块+Gap；不新增解析器或依赖。验收 `cargo test -p aline-markdown --test readonly_presentation --test spec --test dump --test html --test protection`，检查既有金样差异不被盲目接受；证据 `<evidence>/presentation/`。
- [x] 2.2 先写局部窗口/Source 行投影的独立命中金样，再实现映射层受检局部↔全文适配，覆盖实体一对多、Hidden/Synthetic、CRLF/孤立 CR、tab、代码缩进、中文/ZWJ/组合字符、半字符/越界/旧身份拒绝。验收 `cargo test -p aline-markdown --test readonly_mapping`（拟新增）及 `--test mapping --test grapheme`；调用点不得手算块首/行首；独立复审发现的空窗口锚点 affinity 与内嵌 Hidden 查找问题已由最小失败回归修复。主 Agent 复跑 `readonly_mapping/mapping/grapheme` 共 33 例及完整 `aline-markdown`（185 例）通过，API reviewer 清除两项 finding；证据 `<evidence>/mapping/` 与 `<evidence>/review/`。
- [x] 2.3 为新增结构数据接预算/取消/raw fallback 和复用差分：深嵌套/巨表/巨段边界、取消无部分树、旧绝对节点范围失效、导航文字来源与目标独立。验收 `cargo test -p aline-markdown --test hostile --test reuse --test readonly_presentation`，比较新结构/样式/来源/保护，不只比较 kind/text；证据 `<evidence>/presentation-failure/`。
- [x] 2.4 补新公开只读 API 的中文 rustdoc、实际 UI 消费边界和类型/错误说明，同步 `docs/engine-design.md` §3/6/7/11 与架构 §4/5 的已实现状态，保留待冻结项；复核 CODE_STANDARDS §3～4 的职责/最窄可见性。验收 `cargo test -p aline-markdown --doc` 与 `python3 scripts/check_public_api.py` 12 项通过；已回填实现、无头限制及 V-07 待冻结边界，独立 API 审查通过。证据 `<evidence>/presentation-api/` 与 `<evidence>/review/`。
- [x] 2.5 按 E2 在同一解析链修复已识别脚注的标签等价配对，保持重复目标源码顺序/顶层范围/预算/复用拒绝，不新增匹配库。先写 `[^Note]`/`[^note]:`、解析器认可的非 ASCII 等价标签、真正未定义、重复及预算最小失败用例，再修复；验收 `cargo test -p aline-markdown --test navigation --test reuse --test hostile`，不能仅将旧现状测试改预期。回填引擎 §12.1/V-07 的实际修复与未覆盖边界，证据 `<evidence>/navigation-core/`。

## 3. Headless Readonly Lifecycle

依赖 2 的受检边界；关联 D1/U1/U2/U4/G1，AC-01 字节分项，V-07；R-3/5/6/8，S-3/5/7/12/13。

- [x] 3.1 先建不开块初始化/快照/文档级选区/模式测试，再扩 DocCore 只读构造和能力拒绝；保持既有 `new` 自动开块行为。覆盖全部写入口族、非法/旧导航零副作用，比较源码、revision、选区、mode、草稿/组合、undo/redo/节点数与请求资格。验收 `cargo test -p aline-ui --test readonly_document`（拟新增）与现有 `--test document --test document_typing --test document_composition`；证据 `<evidence>/readonly-core/`。
- [x] 3.2 增加文档级和编辑投影两类结果目标复验，区分会话关闭与无开块；测试同 SnapshotId 不同工作版本、模式往返、旧配置/请求、关闭/替换后晚到及 ID 耗尽拒绝，不放宽现有编辑 `ClosedTarget`。验收 `cargo test -p aline-ui --test readonly_document --test document_results --test document_followups`；证据 `<evidence>/readonly-results/`。
- [x] 3.3 通过小文件读取/选择适配边界先写无窗口 Loading/Ready 候选发布测试，再实现只读 session：A/B 乱序、取消、首次/替换失败、权限/无效 UTF-8/多路径/扩展名/预算拒绝，读入增长上限、关闭后结算/释放；拒绝特殊文件时不得被 FIFO `open` 无限阻塞，文档文本预算必须先于全文复制/核心构造执行。主 Agent 在隔离 headless probe 直接编译共享 `aline-ui` 源文件并执行 `readonly_open` 15 passed、FIFO 路径/替换与 inode 替换各 1 passed、Clippy 通过；`aline-source decode_golden` 3 passed。标准 workspace target 被 `gpui-pre-apple` 缺少 Metal 工具阻断，故记录等价源目标验证及限制；证据 `<evidence>/readonly-open/` 与 `<evidence>/root-readonly-probe/`。
- [x] 3.4 文档化只读会话/DocCore 公共边界及结果目标语义，更新引擎 §8/架构 §5 和相应中文 rustdoc，清理被替代的私有试验路径/导出/fake（保留有用故障注入）。只读会话 API 错误/身份/生命周期 rustdoc 和 M1 无头边界已写明；`cargo test --doc` 在隔离共享源码 probe 通过，`check_public_api.py` 12 项通过，`codegraph sync/status` 索引已更新并确认引用状态。标准 workspace target 同样受缺少 Metal 工具阻断，未将其记为原生或全 workspace 文档验证；证据 `<evidence>/readonly-api/` 与 `<evidence>/root-readonly-probe/`。

## 4. Native Window and Open Action

依赖 1～3 的纯机制先过；关联 U1/U7/G1，AC-01/15 阅读/主窗分项，V-01/V-09；R-1/6/8/13，S-2/11/13/15。

- [x] 4.1 提交并完成最小平台后端准入裁决：核对锁定 GPUI 的 `with_platform`/原生后端缺口、候选类型、来源/版本/许可证/依赖闭包/features/目标/unsafe 所属及系统语言来源。用户已批准 `gpui-pre-macos@0.3.9` + `sys-locale@0.3.2`，三个精确 MPL 包例外、精确 `libloading@0.8.9` ISC metadata-only 路径、wasi/wasip2 登记路径与新增9条 wasip2 边，以及7个 M1 GPUI 组合图指纹。完整工程自测117/117、路径并集14/14、许可证回归13/13通过；真实准入 all、all-features 与五目标场景均通过；五个 desktop target 的 dwrote/libloading tree 均未选中。证据 `<evidence>/native-admission/` 与 `<evidence>/root-admission/real-after-libloading/`。限制：当前环境无 `xcrun metal`，最小原生 build 尚未通过；该限制归 4.2/平台验证，不伪称窗口已接通。
- [x] 4.2 仅在4.1已获准后，按批准的精确范围完成后端准入接线，再用锁定 GPUI `with_platform` 在 ui 装配主循环/一个空窗口并保持 app 薄入口；核对实际原生构建与 Metal 工具，不安装/升级绕过。验收 `cargo build -p aline --locked --offline`、`cargo run -p aline --locked --offline` 后真实 macOS 显示/关闭单窗，以及 `python3 scripts/check_engineering.py --ledger <evidence>/native-admission/ledger.json --output-dir <evidence>/native-admission`；摘要回填 V-01，明确仅 macOS 首轮、Intel/Win10/Linux 未测。2026-11-03 Xcode 27.2/Metal Toolchain 可用后，锁定离线构建通过；真实窗口截图确认白色画布和本地化空状态，退出后运行正常返回。证据 `<evidence>/native-window/` 与 `<evidence>/native-build-run-after-metal/`；仍未覆盖文档选择/加载，这属于4.4。
- [x] 4.3 为已开放标题/无文档/加载/只读/错误/action 新增 zh-CN/en catalog，按4.1查明的真实系统首选语言来源接 Locale 与最小配置；检测失败回退可见，不假定 GPUI 有 locale API。测试缺 key/插值漂移和未知语言回退，所有错误枚举在 UI 翻译。验收 `cargo test -p aline-app-core`、`cargo test -p aline-ui catalog`、两语言真实启动与默认系统语言检测；系统首选语言从 `zh-Hans-CN` 切到 `en-US` 实测窗口英文，再恢复 `zh-Hans-CN` 并实测中文；三项 Cargo 测试通过，证据 `<evidence>/catalog/`，回填 UI §14/V-09 已测边界。
- [x] 4.4 接 GPUI 路径选择到同一打开 action（按钮/文件菜单/Cmd+O），关闭文档/系统关闭归统一只读生命周期；覆盖取消焦点返回、读取失败/替换/关闭晚到，用户写入不可用。验收 `cargo test -p aline-ui --test readonly_open` 和真实 macOS 选择 `.md`/`.MD`、取消、多次打开、关闭；协调器/只读打开回归、当前离线 UI Clippy/app build 通过，真实按钮/Cmd+O、`.md`/`.MD`、取消后文档与焦点保持、关闭文档回到同窗空态已实测；打开 1 MiB 夹具也成功，证据 `<evidence>/native-open/` 与 `<evidence>/virtualization/`，回填 UI §4/10/12 的阶段接线及 V-01/V-09，不建关联/拖放/CLI/多窗。

## 5. Virtual Views and Hit Testing

依赖 2～4；关联 U2～U4/E1/D1，AC-01/20 字节/换行分项、AC-16 首轮基准贡献，V-05/V-07/V-09；R-3/4/7/8/10，S-5/8/14。

- [x] 5.1 接变高块 `list` 与 Source 逻辑行虚拟列表、同内容列宽/外侧滚动槽位，建立带文档/投影/宽度/缩放/字体/换行键的有界高度缓存与源码滚动锚点。先测重排/虚拟复用/过期高度不污染/空闲无刷新环，再实现；验收 `cargo test -p aline-ui --test readonly_layout`（5 例）和真实 1 MiB 两视图滚到首尾，记录实例量与缓存规模。macOS arm64 debug 窗口以精确 1,048,576 字节样例实测：Visual 42,800 项、当前帧渲染 10～13 项，Source 42,801 行、当前帧渲染 16 项；两模式高度缓存均未超过 256，首尾截图显示首段/尾部标记，证据 `<evidence>/virtualization/`。这不是 Release 基准或性能门槛，V-05 仍待测。
- [ ] 5.2 接锁定 TextLayout 像素→局部 DrawnOffset→映射层→核心导航，覆盖 Exact/Aligned/Hidden/Synthetic、实体两侧/多字符、重复文本/空白、中文/emoji/组合字符、软折行/Gap、零可见 HTML、旧点击拒绝。验收 `cargo test -p aline-ui --test readonly_hit`（拟新增）与实际像素点击→切 Source 核对预期锚点；断言 `has_open_block=false`、零 bytes/revision/history，证据 `<evidence>/hit/`，回填 V-07 真实命中分项。
- [ ] 5.3 接双视图切换/可取消主动定位及换宽/字体重排恢复，避免同像素复用和陈旧布局；巨块/长行/巨表布局前预算，>4 MiB 默认 Source 原因提示。验收 `cargo test -p aline-ui --test readonly_layout --test readonly_document` 的过期/取消/预算矩阵及真实长文切换；安全回退和 1 MiB 可视化成功分列，证据 `<evidence>/view-switch/`。
- [ ] 5.4 更新 `docs/ui-spec.md` §6 与 `docs/v1-architecture.md` §5/9/10 的已实现虚拟化/命中边界和布局候选说明，补 V-05/V-07/V-09 分项；验收 `git diff --check`、引用检查及 CodeGraph 调用链确认 render 不做全篇解析/I/O/统计/物化、调用方不自建映射，不将候选数值写成发布门槛；证据 `<evidence>/layout-review/`。

## 6. Content Navigation and Fonts

依赖 5，语义依赖 1.2；关联 U5～U7/E1，AC-09 展示/AC-15/20 主窗分项、V-03/V-04/V-07/V-09；R-4/8/10/12/13，S-5/11/12/14。

- [ ] 6.1 接所有基础结构/样式、表格只读行列、五类 Alerts、有限 HTML、灰色 Front matter/脚注/目录及未知/公式/Mermaid/图片引用原文回退；先测渲染覆盖与过滤内容无执行/网络副作用。验收 `cargo test -p aline-ui --test readonly_content`（拟新增）、五组真实夹具截图/操作，不能只 Source 展示通过；证据 `<evidence>/content/`，回填 V-03 仅基础夹具、V-07 展示分项及 UI §7。
- [ ] 6.2 接大纲层级/目录/脚注有效目标与回跳、键盘激活、无标题/缺失/歧义状态及源定位；不做反向跟随，不伪造未索引目标。验收 `cargo test -p aline-ui --test readonly_navigation`（拟新增）和重复标题/多引用回跳/替换文档后旧导航/取消等待定位真实操作，比较文本/历史不变；证据 `<evidence>/navigation/`，回填 UI §5.3/8 与 V-07/V-09，未冻结语义保留未通过。
- [ ] 6.3 接围栏/缩进统一普通代码 renderer、只读语言标识、常显行号与可访问横滚，不接高亮/语言修改；先测空行/空块/末尾换行/软折行编号再接布局。验收 `cargo test -p aline-ui --test readonly_code`（拟新增）、真实长行/中文/未知语言/主纵向滚动不困住；证据 `<evidence>/code/`，回填 UI §7.2/AC-20 仅行号分项，不宣称完整 AC。
- [ ] 6.4 用现有 GPUI font API 实现最小字体适配和有效字体代次，先写注册失败/缺字族/布局失效无头测试，再执行 V-04 中文回退和有授权静态 TTF/OTF 加载/损坏实验；实际观察中英标点/emoji、字体变化后的锚点/命中，不能只凭 add_fonts 返回成功。验收 `cargo test -p aline-ui --test readonly_fonts`（拟新增）和真实实验截图/字体来源与回退观测记录，失败可见且无字节变化；证据 `<evidence>/fonts/`，回填 V-04 与 UI §16，仅字体分项。

## 7. Baselines and Existing Gates

依赖 4～6；关联全部 U/E1/D1/G1，架构 §13 出口、AC-01/09/15/16/20 **分项**，V-01/04/05/07/09；CODE_STANDARDS §6，R-11/17，S-2/12/14/19。

- [ ] 7.1 建可复跑的 M1 Release 测量入口（拟新增 `python3 scripts/run_m1_readonly.py --suite performance --output-dir <evidence>/performance`），先用独立时间序列正例验证分位数/样本数与阶段归属，错误/缺指标/取消/旧运行结果不得伪成功；真实执行 design §8 的尺寸/复杂度/冷热/至少20次方案，记录首屏、切模式、滚动、分段成本/排队、P50/P95、最长主线程停顿、峰值内存与虚拟实例。回填 V-05 首轮结果，明确没有编辑/输入/查找/资源时延成绩，不判完整 AC-16。
- [ ] 7.2 在真实 macOS 上执行 V-09 主窗无文档/加载/正常/失败/取消/回退、窄窗/高 DPI/长英文、打开与双视图/大纲/反馈键盘焦点矩阵；入口拟为 `python3 scripts/run_m1_readonly.py --suite window-baseline --output-dir <evidence>/window-baseline`，脚本输出检查清单并记录人工结果，缺少人工观测不能自动通过。回填 V-09 尺寸/默认/平台观测，AC-15/20 仅主窗/行号贡献；保留 Windows 10/Intel Mac/Linux 与完整设置/菜单/主题矩阵未测。
- [ ] 7.3 跨组执行阅读字节不变端到端：BOM/混合行尾/孤立 CR/空白/无尾换行，打开→滚动→命中→主动跳转→两视图往返→关闭，前后实际磁盘 bytes 和内存源码精确比较、无草稿/空历史，保真失败非零。验收拟新增 `python3 scripts/run_m1_readonly.py --suite readonly-bytes --output-dir <evidence>/readonly-bytes` 与 `cargo test -p aline-ui --test readonly_document`；回填 AC-01 阅读/字节和 AC-09 只读展示分项，保存/重开明确归 M2。
- [ ] 7.4 守卫如因公开面/原生 feature/catalog/证据入口变化需要更新，最小增量修改既有入口并先运行正例及最小违规反例：只读误开块/半字符或旧范围（领域测试）、漏继承成员/引擎间接 GPUI/硬编码或缺 catalog key/不归一化证据/缺测量字段（对应既有或新增检查），准确区分自动检查与人工盲区。验收受影响 `--self-test` 与 `python3 scripts/check_public_api.py --output-dir <evidence>/public-api`、`python3 scripts/check_validation_output.py --output-dir <evidence>/evidence-guard`；受审依赖漂移不得自动刷新通过，证据 `<evidence>/guards/`。
- [ ] 7.5 执行完整 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`，以及 `python3 scripts/check_workspace.py --cargo-home <cargo-cache> --output-dir <evidence>/workspace`；逐一检查八成员实际 lint 继承及工程/安全/公共 API/catalog 结果，不以 all-targets 代表三平台。原始输出经路径归一化留本地；失败不勾任务，证据 `<evidence>/workspace/`。

## 8. Independent Review and Phase Handoff

依赖全部已完成分项；关联 G1/全部能力、CODE_STANDARDS §2～6、R/K 与 S/C 检查组。这里复核跨组集成，不能代替前组应落的测试/文档。

- [ ] 8.1 按 AGENTS 路由对公共 API、只读能力拒绝、唯一状态所有者、后台取消/关闭采纳、HTML/字体安全和虚拟化做 Sol 独立审查，主 Agent 核对 findings 和修复后相关验证；检查替代路径/无消费者导出/依赖/fake 清理。验收无未处置阻塞 finding，执行 `codegraph sync`/`codegraph status` 并核对实际调用链，原审查留 `<evidence>/review/`，必要决定仅回填既有规范。
- [ ] 8.2 逐行核对 proposal 的 §13 对照表与实际证据，汇总前组已回填的架构 §13、验证计划 V-01/03/04/05/07/09（必要 V-06）及引擎/UI 公共契约；M0 豁免缺口、未测平台和 M2 前 V-08/保存闭环不得被关闭。验收每个 M1 必需出口均有可检查的成功证据或明确阻塞，1 MiB 真实可视化滚动不能空缺；若失败保持 M1 未完成，不把归档或 tasks 勾选当出口。
- [ ] 8.3 执行 `openspec validate m1-single-window-readonly --strict --no-interactive`、`openspec validate --all --strict --no-interactive`、`git diff --check` 和引用/绝对路径/变更范围检查；只汇报实际执行的命令、平台与限制。工件与实现一致且原始输出未入库后交付评审；本变更任务不包含 commit/push/archive，后续需用户另行指令。证据 `<evidence>/handoff/`。
