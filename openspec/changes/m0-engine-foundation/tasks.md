# Tasks

本 change 已由用户授权启动 apply，实际状态以任务勾选及证据为准；M0 全程不创建窗口。每组是审查关口，组内任务可分次完成；前置失败暂停依赖任务。2026-10-30 用户在隔离审计和草案交付后指示完成第 1 组，批准现行架构 §3.2 的两项 GPUI 精确例外及相关规范同步；没有其它禁令豁免。E1～E7 对应 proposal 出口表，requirement/scenario 名对应本 change 的 delta。

证据统一记录：提交/工具版本、OS/CPU/设备、输入夹具路径与独立预期来源、完整命令/退出码/关键输出、实际/人工/未执行分类、反例失败原因及恢复后结果、未覆盖平台和链接。拟存放 `docs/validation/m0-engine-foundation/`（apply 时按实际证据创建），并从既有验证计划相应分项链接；这不是新增规范或 V 编号。共享 fixtures 必须由具体 crate 测试调用并出现在测试发现清单中。下列测试过滤名和守卫文件名是拟建入口，实施时记录实际名称，不将不存在的入口当已运行。每组 Rust 变更提交前执行第 2 组列出的完整三条门禁，不以局部测试替代。

## 1. 兼容目标与工具链及依赖候选登记

范围：全八 crate，重点 GPUI 闭包；前置：用户批准本方案并启动 apply。关联 workspace/engineering-gates「候选与闭包准入」，E1/E6；R-1、S-2-02/13-06/15，K-01/K-08，RS-01/04/05。

- [x] 1.1 建 V-01 兼容目标/设备表：登记 macOS（含 Intel）、Windows 10、Linux CPU/系统候选/显示环境和取证设备缺口；人工核对验证顺序为先 macOS 后 Windows/Linux，表中无“已支持”推断。
- [x] 1.2 在隔离候选目录验证 Rust/Cargo/edition/rustfmt/Clippy/gpui-pre 组合，保存 rustc -Vv、cargo -V、cargo fmt --version、cargo clippy --version、候选构建日志及源码/补丁身份；MSRV 若声明单独测试最低版本，正例可复现，最小不兼容配置须有诊断，不把旧版本或 edition 候选写成锁定事实。
- [x] 1.3 建逐依赖准入账本：用途/所属 crate/候选精确版本/来源/许可证/features/target/dev/build/同类替代；用 cargo metadata --format-version 1 及 cargo tree --target all -e all 保留真实包/别名/传递路径，再分别解析实际目标与 feature 组合，不能以一次 all-features 并集掩盖互斥组合；按架构 §3.2 核对 GPUI 精确例外及全部引入路径，正例合规组合通过，反例经别名或目标路径引入未获例外的 notify/regex/tokio/Wasmtime/WASI、第二解析器，以及同包旁路/版本或校验和/target 漂移必须失败；解析失败记阻塞而非通过。
- [x] 1.4 人工审查准入证据和五引擎闭包；第 1 组审查隔离候选闭包及拟用依赖归属，第 2 组仍须复核实际五引擎闭包，不能把尚未创建的成员写成已验证。无合规 GPUI 候选时列精确路径、最小替代和待裁决影响，暂停步骤 2 依赖接线。回填架构 §3.2、引擎 §18.2、Rust §2/13/19 与 V-01 目标栏，并同步本次已批准例外涉及的 AGENTS、架构规范 R-1-05、Rust §15、CODE_STANDARDS/工程治理及 change 工件，只记录实际分项，不标 V-01 通过；核对提交 scope 遵守 RS-05。

## 2. 八 crate 骨架、工程门禁与 catalog

范围：app 与七 aline crate，app-core/i18n；前置：1 准入组合合规。关联 workspace/engineering-gates 全部 requirement，E1；R-1/2/13/16、S-2/7/10/11/12/13/15，K-01/K-06/K-08。

- [x] 2.1 建八成员最小 Cargo 骨架、统一依赖/精确 GPUI 来源及锁文件、登记工具链，app 不打开窗口；只建被当前任务消费的模块。用 cargo metadata --format-version 1 核八成员/真实包名，五引擎逐包无头构建/测试，验证 source/app-core 无反向依赖，保存实际命令和结果。
- [x] 2.2 建拟定 `scripts/check_engineering.py` 的 metadata 依赖守卫及隔离图夹具：正例允许主链向下/source 独立及架构 §3.2 的精确 GPUI 例外；最小负例分别 buffer→source、markdown→edit、plugin→ui、引擎经别名/中间包/dev/build/target/feature 泄漏 GPUI，以及未获精确例外的全闭包禁用依赖、同包旁路/身份校验和/target 漂移。第 1 组候选准入检查不得替代实际 workspace 守卫。运行 `python3 scripts/check_engineering.py --self-test`（实现时确认实际 CLI），断言每个反例非零且指明目标边；移除反例重跑正常图。说明未解析目标/feature 是缺口，检查器不能把 Cargo 报错当合规。
- [x] 2.3 配置默认 lint+逐项高价值项，验证每成员实际继承、自有 unsafe forbid、五引擎测试/示例/基准均禁 unwrap，生产禁 expect/panic!/unreachable!/发布 assert；在隔离最小样例逐一故意触发诊断、包括成员漏继承和非法坐标转换，debug/release 分列；正例测试预期断言/不可变实体常量可用，运行期 OnceLock/线程/全局 locale 须被守卫或人工路径审查发现。记录 lint 级别/作用域和宏/调用语义盲区，不整组开启 pedantic、不全局放行转换。
- [x] 2.4 在 app-core 建显式 locale 的 t(key)、zh-CN/en TOML 与 rust-embed 实际消费测试；`cargo test -p aline-app-core` 核对相同 key/插值正例及缺键、未知键、缺/多插值、损坏 TOML 反例按既定错误协议失败，实例切换语言互不污染。人工检查引擎错误枚举不携已翻译文案，UI 用户字符串消费 key。
- [x] 2.5 建实际检查入口和 CI 最小接线：逐条运行 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`，分别记录退出码/日志，前条失败后的命令不得记通过；另保存 metadata 守卫/catalog/lint 正反例结果。验证格式选项生效且无忽略警告。`--all-targets` 只代表当前目标，CI 未实际运行记未执行，不能沿用此规划的结构校验成绩。
- [x] 2.6 回填 Rust §2/17/19、架构 §4/10.3 及工程证据入口，写清检查器执行命令/依赖范围/人工核查步骤；人工复核公开面、线程/全局调用、异常豁免（本方案未授权任何 unsafe 例外），确认不预装无消费者后端。

## 3. 坐标、身份与公共输入输出边界冻结

范围：五引擎公开边界、UI document；前置：2 无头构建与守卫有效。关联六 capability 的版本/范围场景，E1/E2/E5/E7；R-2～4/6、S-5/7，K-01～03/K-08，RS-02/03。

- [x] 3.1 按 design/Public Boundaries 定义受检源码/绘制/UTF-16/存储范围与版本 home，记录 DocumentId/WorkingVersion/草稿代次/投影/磁盘身份消费签名及生成范围；用外部消费测试和依赖守卫证明无反向边，source 仅接文本视图/自有元数据，跨层不导入 UI 类型。
- [x] 3.2 保持 tuple 补丁底层形状并封装受检构造/转换；正例合法空区间、末端、反向选区通过；最小反例类型混传、半 UTF-8、UTF-16 中间代理对、越界/溢出、错文档/错版本/错投影失败。用 cargo test -p aline-buffer 和 -p aline-markdown 的外部 API 用例及编译失败夹具分别留证，不以 type alias 当分型。
- [x] 3.3 冻结 decode/encode/parse/映射/plan/validate/apply/snapshot 的输入输出、错误类别、唯一写入口及相等证据来源；人工追踪实际公开消费者，验证恒等转换也走映射、调用方不手算局部差或 UTF-16 表。回填 Rust §5、引擎 §3/4/7、架构 §4/5/7.1、V-07 接口基础记录，未实现接口只写契约未验证。

## 4. aline-source 字节与行尾往返

范围：decode/encode/metadata 及最小受检 I/O；前置：3 的存储/源码与相等证据语义。关联 engine/source 三 requirement，E2/E4；R-3/15、S-5/7/12，K-02/K-08（K-04 仅最小故障样例）。

- [x] 4.1 按 2026-10-31 已批准原始行尾表示，先建独立 bytes→保留 CR/LF 的源码/元数据→bytes 金样，再实现预算先验与解码：空/BOM/正文 U+FEFF/LF/CRLF/混合双方多数及平票/孤立 CR/无尾换行/有效 %PDF 与 PK；非法 UTF-8、UTF-16 BOM、截断多字节与输入超限受检失败，cargo test -p aline-source 解码用例确认无有损替换、无原文件写入。
- [x] 4.2 实现原 bytes 通道与改后 BOM/多数派编码；按已批准语义扫描 CRLF/LF 行尾单元（含编辑新拼接 CRLF），未配对 CR 保留；先验证 CR-X-LF 删除 X、边界插入/删除/粘贴，分别检查冻结源码精确值、编码、再次 decode 的限定行尾等价、逆补丁精确恢复；LF 策略下 `"\r\n"` 重开 `"\n"` 不声称 String 相同。正常操作须可表示，其余字符不丢；不得隐式整理 Buffer、用存储等价吞掉历史或拒绝正常输入。
- [x] 4.3 用 `cargo test -p aline-source roundtrip` 验原字节逐字节相同、局部修改限定范围、混合例外及 Undo 回原基线；反例无来源 unchanged bool/旧基线相等证据不能放行。模拟新基线替代后不恢复过期原 bytes，保存成功/失败仅按实际输入事实更新由宿主持有的基线。
- [x] 4.4 建最小受检文件原语及可控故障边界，正例完整写入、反例临时创建/写入/替换前失败保全目标；运行 `cargo test -p aline-source atomic` 并记录文件系统与故障步骤，不回退为截断写、不宣称完整 V-08 或全平台断电保证。
- [x] 4.5 回填引擎 §5、架构 §5.1、V-06/V-07 编码基础证据，记录边界表示取舍与未测权限/替换竞争，K-08 人工复核源码与磁盘偏移未混用。

## 5. aline-buffer 补丁、锚点与历史原语

范围：string_buffer/patch/anchors/history/revision；前置：3/4 表示和身份通过审查。关联 engine/buffer 两 requirement，E2/E4/E7；R-3～5、S-5/7/12，K-02/K-03，TA-01/02。

- [x] 5.1 先写手工正逆补丁金样，再建 StringBuffer/行索引与原子批次；以 abcXYZ→aQQc! 金样验证 after 逆坐标，随机合法批次/固定 seed 往返 bytes/选区，`cargo test -p aline-buffer patch` 记录最小失败样本（随机库先准入）。
- [x] 5.2 检查末项非法、重叠、逆序、同点歧义插入、半字符、旧基线、after 选区非法与容量溢出；反例均比较文本/行索引/选区/revision/undo/redo 零变化，合法空插入和无变化替换分别验证；预检完成前不得发布局部修改。
- [x] 5.3 建锚点 affinity 变换、正逆选区、操作来源和单调 revision，用 `cargo test -p aline-buffer anchors` 与 history 用例验证替换内锚点/前文变长、Undo/Redo 不退 revision；低层不解析或写盘，人工审查唯一持有关系。
- [x] 5.4 建历史节点/待定组/独立重载边界及预算入口，测保存 A→Undo→B 同栈深、历史裁剪、回基线相等证据待定、redo 分叉、逆补丁预算耗尽；重载 Undo/Redo 只改内存不退磁盘基线，资源可达接口不提前释放历史引用，不每键做全文哈希。用 history 过滤测试及全 crate 测试留证。
- [x] 5.5 回填引擎 §9、架构 §5.4、Rust §12、AC-01～04 基础贡献及 V-07 分项，注明 TA-02 真实自动保存暂停/外改提示尚属 M2，不能把历史原语测试当闭环完成。

## 6. aline-markdown 解析、块/Gap、映射与保护

范围：pre/build/tree/blocks/protection/budget；前置：3～5，解析器版本/features 准入。关联 engine/markdown 全部 requirement，E2/E3/E5；R-3/4/7/12/16、S-5/7/12/13，K-01/K-02/K-08（K-07 仅预算基础）。

- [x] 6.1 先建 CommonMark/GFM 与 MD-01～06 基础夹具，再接显式 Cargo features/运行期 Options；记录各语法走原生选项或必要 pass 的依据。`cargo test -p aline-markdown spec` 验字面代码区避让、配对文首 Front matter、未闭合不吞全文，公式/Mermaid 只识别来源不引渲染后端。
- [x] 6.2 建必要 RewriteMap 逐级复合/跨片段事件来源与构树，控制字节 U+0001～U+0008 碰撞、合成围栏、重叠改写及映射失败有原文回退反例；`cargo test -p aline-markdown rewrite` 比较独立源范围预期，不仅比较可见字符。
- [x] 6.3 建块/Gap 与无头 dump 入口（拟 `cargo run -p aline-markdown --example dump_tree -- tests/fixtures/m0/basic.md`）；空文档、嵌套/多空行/无尾换行覆盖测试确认顶层分区精确覆盖且块不含尾分隔。保存 dump 含版本/块/间隙/来源/保护/诊断并人工对独立金样审查，示例同样禁 unwrap。
- [x] 6.4 建 source_at/drawn_at/aligned、局部/全文和 UTF-16 统一映射；`cargo test -p aline-markdown mapping` 验转义、实体/多字符实体、中文/emoji/ZWJ/组合字符、隐藏/合成、重复文字/空白、SoftBreak/HardBreak/br 来源、选区方向；原样精确往返、规范锚点稳定/对齐幂等。最小非法半字符/代理对/旧投影失败，不能要求全源码双射或靠 clamp/backscan 修补。
- [x] 6.5 建有限 HTML 与保护子区间金样：支持标签、无 href、引号内 >、交叉嵌套、注释/空白、实体、过滤标签、不完整尾部；`cargo test -p aline-markdown html` 与 protection 过滤验证来源/文字/语义/精确性/保护；脚注引用与普通正文相邻时仅保护子区间，HTML 原文语义不因展示成段落消失。
- [x] 6.6 建扫描步数/嵌套/节点/改写/映射预算与取消检查；敌意深栈、重复未闭合 HTML/引号、巨段/巨表及超限反例在分配/循环中终止，保持源快照、不截断；`cargo test -p aline-markdown hostile` 记录注入预算/实际步数和第三方长调用限额盲区，数值不是 V-06 发布限额。
- [x] 6.7 建全量路径与最小头尾配对/重定位差分用例：前文插入、引用定义/脚注/围栏/配置变化，比较树/Gap/范围/映射/保护/导航；用 reuse 过滤测试记录实际复用命中与安全回退，旧绝对范围反例须失败，不以所有样例全回退冒充优化证明。
- [x] 6.8 回填引擎 §6/7/11/12、解析配置及 V-06/V-07 基础夹具；将 SoftBreak、有限 HTML 尚未固化金样逐项列实验结果/缺口，不变更 MD 口径或宣称公式图表能力/AC-09 完成。

## 7. aline-edit Change、拒绝与最小编辑协议

范围：change/check/refusal/代表性命令；前置：5/6 补丁、匹配版本 Parsed 和保护范围。关联 engine/edit 两 requirement，E2/E4；R-4/5/12、S-5/7/12，K-02/K-03。

- [x] 7.1 先写普通源码替换、可证明安全的行内格式 toggle 和一批多补丁的独立输入→补丁→选区金样，再实现纯 plan/validate、候选预演与应用前 reread；`cargo test -p aline-edit` 验实际消费方不获得可写真实 Buffer，保留隐藏源片段，重读按命令语义比较。
- [x] 7.2 验成功/无变化/错工作版本/错预期片段/错投影/越界/预算/重读失败；最小反例为可见文字相同但隐藏链接目标丢失，必须拒绝。`cargo test -p aline-edit refusal` 核完整状态不变，无空步/no redo 清除，计划变化后重验，不套旧范围。
- [x] 7.3 测普通正文与脚注引用交错、MD-11 跨区删除/替换/粘贴、HTML 触及/跨界格式拒绝、Source 修改及 HTML 原文修改允许；拒绝与独立 Source 定位分开，pending 组/组合/选区/undo/redo 均不被预检改变；为查找旧计划及多补丁一步历史保留可消费协议，不提前实现全量 M3 命令。
- [x] 7.4 回填引擎 §8/9/14、架构 §7.1/7.2、V-07/AC-04 基础覆盖及未覆盖列表/表格/格式化命令；人工审查 DocCore 以外无实际写入通道，记录 R/S 与场景覆盖。

## 8. 无窗口 DocCore 草稿、输入组与当前快照

范围：aline-ui/document 纯状态及 buffer/edit/app-core 接口；前置：3～7。关联 engine/document-core 全部 requirement 及 edit「待定输入时拒绝」，E4/E7；R-3～6/12/15、S-7/12/13，K-02/K-03/K-08（K-04 仅模拟采纳），TA-01/02、ED-01。

- [ ] 8.1 建 DocCore 唯一 Buffer/History/OpenBlock 及当前快照，先以事件测试验证含最新草稿/工作版本/源码选区/资源可达引用；`cargo test -p aline-ui document` 在无窗口环境运行，五引擎依赖仍无 GPUI。若测试 harness 不可用，使用同模块纯状态公共驱动入口并记录实际能力，不新增引擎 GPUI 或第九 crate。
- [ ] 8.2 用显式虚拟时间验证 900ms 前后边界、非相邻输入、同块多组提交仍开块、离块/Escape、Undo 优先待定组及 Redo 选区恢复；快照与继续输入交错时，取快照不封口、不压缩历史；切模式先等组合安全结束，再经统一协议处理剩余输入组，切换动作自身零字节/零额外历史步；用 named document/input_group 测试及独立状态表留证。
- [ ] 8.3 建组合/发布等待纯状态转移，模拟更新/替换/确认/取消、重复事件及取消后旧投影；确认独立一步、取消恢复前态且代次不复用；手动保存/另存为/导出只等待自然结束后取当前快照，取消请求零副作用，自动保存延后，恢复保护独立。运行 composition/publication_wait 过滤测试，标明真实候选窗/焦点/平台 callback 未执行。
- [ ] 8.4 用可控结果序列测试错文档/错配置/错 WorkingVersion/旧 request、关闭后返回、Undo 同文本后旧请求及草稿基线冲突；拒收旧解析/旧投影且保留输入，不能 drop_open。模拟重载独立历史与基线分离，实际写盘回执分类协议保留；所有拒绝比较完整状态，不提前实现文件协调器。
- [ ] 8.5 回填引擎 §8～10/17、架构 §5/7.1、V-07 基础分项及 V-08 身份/快照设计准备；逐项列真实输入/保存/恢复/资源迁移未测。评审确认 M2 可消费同一历史/状态协议，无子编辑器第二历史。

## 9. M0 出口复核与 M1 交接

范围：全 workspace 集成证据；前置：1～8 分项证据及失败已处理。关联 workspace/engineering-gates「逐项出口交接」、proposal E1～E7；R-17/18、S-2/12/19，K-01～03/K-06/K-08，K-04/05/07 完整实验保留后续。

- [ ] 9.1 完整重跑 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`，另运行实际依赖/安全/catalog 守卫与正反例，保存每条退出码、环境/feature/目标及日志；此前组已验证的领域测试不重复扩量，新增集成失败先修并重跑受影响检查。纯文档规划阶段这些命令均未执行。
- [ ] 9.2 建 E1 证据包：八成员 metadata/lockfile/候选准入与真实版本、依赖方向/GPUI/禁库反例、成员 lint 继承/unsafe/panic 作用域、catalog 键/插值、完整 Cargo 结果；人工审查闭包及文案来源，缺任一证据不判 E1 通过。
- [ ] 9.3 建 E2/E3/E5 证据包：四核心公共 API 无头测试发现清单、预算/取消与非法输入结果、展示树原始范围 dump、块+Gap 覆盖、来源/精确性与对齐映射金样；独立复核 dump 与源字节，声明实际覆盖平台及预算仍待测限额。
- [ ] 9.4 建 E4/E7 证据包：AC-01 原 bytes、AC-02 局部补丁/模式意图、AC-03 Unicode 与组合模拟/选区、AC-04 输入组/代表命令/拒绝全状态；TA-01 开块多组、快照不封口和 TA-02/ED-01 纯协议，逐条列未覆盖真实 UI/IME/完整结构命令，不修改 AC 原口径。
- [ ] 9.5 建 E6 及 M1 交接：回填 V-01 目标、V-06/V-07 基础分项与 M0 工具实验；V-01～09 不整项通过。M1 排入原生 macOS 冒烟、V-04 中文字体回退、V-05 基准、真实语法夹具；Windows/Linux 后续取证且 Win10/Intel 不丢；M2 前 V-08 身份/事务方案及最小安全闭环保留独立出口。
- [ ] 9.6 按 AGENTS 路由安排重要架构/API/安全/一致性的独立 reviewer，逐项核查所有者/写路径/模式代价/R/S/K 场景/平台缺口；回填架构 §13、引擎阶段出口、Rust §19、验证计划分项并检查交叉引用及 `openspec validate m0-engine-foundation --strict --no-interactive`。在真实代码新增后按开发流程执行 codegraph sync/status；记录仍阻塞项，全部 M0 出口有证据后才允许提出 M1 接线，不以任务关闭/archive 代替出口。
