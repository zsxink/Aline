# Aline V1 实现前验证清单

更新日期：2026-11-03
需求依据：[V1 PRD 0.9](./v1-prd.md)  
评审记录：[产品问题批阅归档](./archive/2026-10-28-v1-open-questions.md) · [架构问题确认归档](./archive/2026-10-29-v1-architecture-decisions.md) · [Rust 与引擎问题确认归档](./archive/2026-10-29-rust-style-decisions.md) · [UI 问题确认归档](./archive/2026-10-29-ui-decisions.md) · [插件问题确认归档](./archive/2026-10-29-plugin-decisions.md) · [主题决定归档](./archive/2026-10-29-theme-decisions.md)

本清单承接本轮已同意的验证方案，以及需要在技术设计中细化的行为契约。WC-01～WC-06 已确认并回写产品边界，完整依据见[迁移审计决定归档](./archive/2026-10-30-migration-decisions.md)；此确认不表示任何实验已通过。问题批阅、RS-01～RS-05、ED-01、UI-01～UI-06、PL-01～PL-03 及 TH-01～TH-10 的建议确认已回写归档，当前无待确认产品选择；下列实验、产物和用例尚未完成，不能标成已验证。主题字段与产品选择确认不代表 schema 数值、GPUI/字体/透明度能力或资源限额实验已冻结。无需为已决定的产品方向重复询问，但实验结果若迫使缩减范围，须记录影响并取得新的产品决定。

以下 V 项均**未整项完成**；M0 已执行的工程与无头基础分项在对应条目和结果表中单列，不能由局部结果推断整项通过。涉及格式、默认数值或边界尚未固化的地方，先制作可检查的方案或样本，再回填 PRD 及专门规范。正式 V1 发布前，所有发布阻塞项必须完成。

**2026-11-03 验证取证语言范围：** 用户裁决以后验证不需要双语验证，只需要验证中文（见[决定 §24](./decisions.md#24-2026-11-03-验证取证只做中文)）。界面语言的人工/原生取证只做中文，不再切换系统首选语言复验英文；catalog 的 zh-CN/en 键与占位符自动校验、缺键/漂移反例继续保留。2026-11-03 已取得的英文启动记录保留为裁决前的事实，后续复跑只核对中文。本条不改变 APP-04 产品范围，也不表示任何 V 项整项通过。

## 当前 M0 状态与复跑入口

**2026-11-02 M1 apply 进度：** OpenSpec `m1-single-window-readonly` 已开始实施。夹具、Markdown 只读语义/映射/脚注目标、DocCore 只读边界与只读文件会话已完成无头分项；映射空窗口/Hidden 边界及 FIFO 阻塞/文本预算顺序修复有独立审查和回归。M0 豁免缺证仍保留。用户已批准原生后端候选 `gpui-pre-macos@0.3.9` + `sys-locale@0.3.2`、`cbindgen@0.28.0`/`option-ext@0.2.0`/`dwrote@0.11.5` 三个精确 MPL 例外、`libloading@0.8.9` 的精确 ISC metadata-only 完整路径例外、`wasi@0.11.1`/`wasip2@1.0.4` 原列及新增9条精确 metadata 边，以及七场景 M1 GPUI 组合图指纹。工程准入 all/all-features、macOS ARM/Intel、Windows x64、Linux x64/ARM 七场景已通过；完整自测117/117、path union14/14、许可证自测13/13通过，五个桌面 target tree 均不选中 dwrote/libloading。当前环境没有 `xcrun metal`，原生依赖编译不能越过 shader build；尚未创建/验证窗口、未执行系统语言 GUI 观测、未做 1 MiB 可视化滚动。Windows 10、Intel Mac、Linux 仍未做真实平台实验，不能声明 AC-16 或 V-01/V-05/V-07/V-09 整项通过。证据见 [M1 原生准入候选方案](../openspec/changes/m1-single-window-readonly/native-admission-proposal.md)及 `logs/validation/m1-single-window-readonly/2026-11-02-apply/`。

**2026-11-03 M1 原生入口进度：** 4.1 准入门已完成；`app` 现在读取系统首选语言并调用 `aline-ui::run_application`，macOS 原生入口和一个本地化空状态窗口构造已写入源码。锁定 GPUI 的静态复审确认初窗失败立即以状态码 1 退出、最后窗口关闭时退出。catalog 专项测试、Linux 目标 app 检查、`-D warnings` Clippy、UI 测试目标编译和完整 fmt 检查通过；完整准入/lint/safety/public API 守卫通过。完整 workspace Clippy 与测试仍被 `gpui-pre-apple` 的 Metal shader 构建阻断（缺少 `xcrun metal`），因此 macOS 专属源码尚未完成实际 Rust 编译，未运行原生窗口或观测系统语言。4.2/4.3 保持未完成；V-01、V-05、V-07、V-09 仍未整项通过，未取得 1 MiB 可视化滚动证据。原始输出见 `logs/validation/m1-single-window-readonly/2026-11-03-continue/`。

**2026-11-03 Xcode 环境复查：** 用户已安装 Xcode 27.2，但当前全局 developer directory 仍为 Command Line Tools。设置进程级 `DEVELOPER_DIR` 后可读取 Xcode 版本；`xcrun --find metal` 被未接受的 Apple Xcode/SDK license 拦截，Metal Toolchain 状态尚未确认。`xcode-select --switch` 要求 root 权限，本轮未切换全局设置。待用户在本机接受 Apple license 后，再以 `DEVELOPER_DIR` 重试，不把“已安装 Xcode”记为 Metal 编译器可用。检查输出见 `logs/validation/m1-single-window-readonly/2026-11-03-continue/xcode-detection/`。

**2026-11-03 Xcode/原生空窗复核（更新）：** 用户完成 Xcode 首次运行配置并安装 Metal Toolchain；当前 `xcode-select -p` 指向 Xcode 27.2，`xcrun --find metal` 和 `xcrun -sdk macosx metal --version` 均成功。`cargo fmt --all --check` 与 `cargo build -p aline --locked --offline` 通过；完整 `scripts/check_workspace.py` 汇总也通过。启动 `cargo run -p aline --locked --offline` 后，真实 macOS 单窗显示本地化空状态（本次系统首选语言为中文），退出后 Cargo 进程以状态码 0 结束。初次截图发现内容区域未设背景/文字色，已按现行 UI 颜色契约修复，并由后续真实截图复核。截图与原生构建输出见 `logs/validation/m1-single-window-readonly/2026-11-03-continue/native-window/`、`native-build-run-after-metal/`；环境探测历史见 `xcode-detection/`。这只完成原生入口/空窗分项；默认语言随系统的完整验证、打开文档、1 MiB 可视化滚动、V-01/V-09 整项和 AC-16 均未完成，Windows 10、Intel Mac、Linux 与 M0 豁免缺证仍保留。

**2026-11-03 M1 语言、打开与 1 MiB 虚拟阅读实测：** macOS arm64 / 27.2 的系统首选语言从实际全局 `AppleLanguages=zh-Hans-CN` 切为 `en-US` 后，分别全新启动最新离线 app 并截图核对英文、恢复 `zh-Hans-CN` 后核对中文；未使用 `LANG` 或应用测试注入。该英文切换属裁决前记录，按[决定 §24](./decisions.md#24-2026-11-03-验证取证只做中文)后续复跑只核对中文。`aline-app-core`、`aline-ui catalog`、`aline` 三组 Cargo 测试通过，截图、系统语言前后状态和启动输出见 `logs/validation/m1-single-window-readonly/2026-11-03-continue-02/catalog/`。真实原生窗口通过系统文件选择器打开生成器创建的精确 1,048,576 字节多短块夹具，在 Visual 与 Source 分别 Home/End 滚动；Visual 42,800 项、可见渲染项 10～13，Source 42,801 行、可见渲染项 16，高度缓存上限 256，首尾截图及调试度量见 `.../2026-11-03-continue-02/virtualization/`。这是 macOS debug 可视化滚动分项，不是 Release 性能基准，V-05 未通过；V-01/V-07/V-09 仍未整项通过，AC-01/09 仅能报告有证分项，不能宣称完整 AC-16。Windows 10、Intel Mac、Linux 和 M0 豁免缺证继续保留。

**2026-11-03 V-07 用户裁决的解析分项：** SoftBreak `a\nb` 保留带来源可见换行；有限 HTML 子集以外的 void 标签显示原标签；未支持非 void 容器按完整原文显示标签与内部空白。新增反例/绿测与独立 `.drawn` 预期后，`cargo test -p aline-markdown --locked --offline` 全部通过（17 个测试目标）；完整原始输出含早期金样冲突和修正结果见 `.../2026-11-03-continue-02/v07-contract/`。这不代表其他 V-07 语义、像素命中或原生 HTML 视觉样式通过。

2026-11-01 清理过程文档时保留的状态摘要：已有八成员 workspace、无头引擎/宿主与依赖/lint/安全/公共 API 守卫；**M0 出口未闭合，不能进入依赖该出口的 M1**。现有 OpenSpec 勾选不等于重新核实完成，继续以原任务与实际代码/测试逐项核对。旧过程目录已清理，历史报告引用仅说明当时事实。
- **2026-11-02 用户验收裁决：**用户明确指示“全部给通过”。据此，M0 任务 48/48、E1–E7 与 M1 门禁记为**用户验收通过**；1.3/1.4/9.2 的逐包替代/必要性人工复核、E2/E4/E5/E7 的整项验证、E3 的全量夹具、E6 的未测项、V-01～V-09 以及真实 IME/保存/跨平台仍未执行。该裁决是风险接受，不是证据补齐；本文件及历史段落中的“未闭合/未通过”保留为当时证据事实，不得改写成已执行或已验证。M1 未在本轮启动；提交、推送、OpenSpec 归档需另行明确指令。验收裁决回填后已复跑 OpenSpec strict：`run-21-openspec-acceptance/stdout.log` 2/2、exit 0；`git diff --check` exit 0。

- 2026-11-02 R14 前一轮：macOS arm64、登记 Rust 1.98.1。直接 Rust 三门禁及明确 warm-cache 的统一 `check_workspace.py` 曾通过；登记隔离 Cargo cache 缺 registry 源码/包缓存，旧轮结果见忽略路径 `logs/validation/m0-resume/run-1/`。此轮早于最新 6.7 测试改动，不单独作为最终代码状态证据。
- 2026-11-02 R14 最终复跑：macOS 27.2 arm64、登记 Rust/Cargo 1.98.1，`CARGO_NET_OFFLINE=true`。原始登记隔离 Cargo cache 在 run-06 完整入口 exit 1：fmt=0，Clippy/test=101（缺 `rust-embed`），dependency=1，public-api=1（8 个隔离编译夹具依赖无法解析）；工程/lint/safety 正常与自测为 0。随后仅将既有本机 warm cache 中 339 个经 lock checksum 核验的 registry archive 与 index 离线复制到登记隔离 Cargo home，未联网、安装/升级工具或改锁文件/指纹；run-07 对该已播种登记 cache 的统一入口 20 项均 exit 0。显式 warm cache 独立 run-06 入口也 exit 0。两种成功结果都不能改写原始空登记 cache 的失败事实；分别的命令、退出码、Cargo home 与限制见忽略路径 `logs/validation/m0-resume/run-20261102-06/` 与 `run-20261102-07/`。Rust 三门禁通过，workspace 48 个测试 binary 共 365 tests passed。该轮早于导航索引实现，最新代码状态的统一门禁见下文 run-19，不能把本轮的 365 写成最终计数。run-09 GPUI 0.3.9 / edition 2024 隔离候选在当前 macOS arm64 下 metadata/check/build/fmt/Clippy 均通过；不支持版本有 Cargo 解析拒绝诊断。tool 版本、工作目录映射、锁/源码身份见 `logs/validation/m0-resume/run-20261102-09/gpui-candidate/summary-final.json`。候选未声明 MSRV，Intel Mac/Windows/Linux 未构建。八成员 metadata 为 347 个 packages，依赖图包含七个目标解析场景；目标解析不代表跨平台 Cargo 构建。
- R14 原始 46 项（排除 R07 后增 6.9）为 **42 已核实、4 证据不足、0 与事实不符**；本轮新增解析 fixture 全树/Gap 分区、映射绘制文本、脚注回退保护范围/投影映射对照和标题源码锚点差分。成功复用样例不包含保护区间（相关语法会整体拒绝复用）。**2026-11-02 导航索引收口（任务 6.7/6.10）：** `Parsed::navigation()` 返回独立只读 `NavigationIndex`（Heading/Toc/FootnoteReference/FootnoteDefinition），20 例导航测试覆盖顶层标题级别/范围、TOC 来源与全部顶层标题目标、脚注双向目标、大小写差异、重复标签、嵌套边界、同快照、多标签预算、预算恰好等于/边界-1（T×H、R×D）与原文回退；30 例复用测试对真实命中只重定位并比较 `Heading` 条目，旧源码/目标绝对范围直接使用失败，嵌套 Heading 容器复用后无导航条目，嵌套 `[toc]` 容器可随容器复用但无条目，仅 FootnoteReference 与嵌套脚注定义分别拒绝；生产 `same_block` 仍只比较块种类与块文本。因此 6.7/6.10 可勾选，但不表示 V-05 或完整导航复用通过。1.2 当前 macOS GPUI 候选组合已在 run-09 复现并勾选；MSRV 未声明且跨平台构建未执行。1.3 仍缺逐包同类替代论证（现有替代结论为分支/引擎闭包级），1.4 的人工复核也明确未把分支级替代写成逐包结论；9.2/E1 因同一缺口仍不足。用户已裁决 T-8-17：有效非空普通输入草稿开始即清除旧 redo 分支；拒绝输入与 IME 取消分别保留分支，已有回归通过。独立复核发现的旧实现行为已由对应测试修复覆盖。OpenSpec 共 48 项（含 R07 的 6.9），当前 48 项勾选、0 项未完成；其中 1.3/1.4/9.2 为 2026-11-02 用户验收裁决通过，逐包人工复核未执行，保留为验收豁免而非证据性验证。勾选数不表示证据性验证完成。
- 2026-11-02 导航收口最终统一门禁：`logs/validation/m0-closeout-20261102/run-19-workspace-final/` 的 20 项入口均 exit 0（含 fmt、Clippy `--all-targets --locked --offline -D warnings`、test、工程/依赖准入、lint/safety/public API 正反例与输出路径检查）；`cargo test --workspace --locked --offline` 实跑 49 个测试 binary（含 doc-test）共 395 passed。`run-16-engineering-final/` 七场景 exit 0（metadata 347、选包 313），`run-14-admission-self-test/summary.json` 115/115，`run-19.../public-api/summary.json` 12 例通过。环境为 macOS 27.2 arm64、Rust/Cargo 1.98.1、登记隔离 Cargo home 与 `CARGO_NET_OFFLINE=true`；该 cache 已由经 lock checksum 核验的 archive/index 播种，不能写成空缓存或跨平台构建成功。OpenSpec strict 在文档回填后复跑：`run-20-openspec-final/stdout.log` 2/2 通过、exit 0（OpenSpec 1.13.2）；同轮 `git diff --check` exit 0，`Cargo.toml`/`Cargo.lock` 无改动。
- 2026-11-11 M1 7.3 统一门禁事故与修复：登记隔离 Cargo cache 已空（`registry` 仅 `CACHEDIR.TAG`，无 index/archive），`cargo metadata --offline --locked` 报 `no matching package named 'sys-locale' found`（该包由 2026-11-03 的 `328b07ec` 引入，非本轮改动；本轮 diff 不含任何 manifest/锁文件改动）。首次完整入口 `2026-11-11-73-workspace/` 因此 4 项转红：`clippy`/`test` exit=101、`dependency` exit=1、`public-api` exit=1（隔离 lock 生成失败）；`fmt` 与全部自测 exit=0。**该失败在 `git stash` 后的干净树上同样复现，属既有环境缺陷，与本轮改动无关。** 根因定位为两处：登记隔离 cache 无 registry 缓存；且本机 warm cache 中的 `sys-locale-0.3.2.crate` 归档损坏（16910 字节、SHA-256 `b1e99de5…b50d`，与 `Cargo.lock` 的 `8eab9a99…626c4` 不符，而 crates.io 上游归档逐字节匹配锁值）。修复方式沿用 2026-11-02 的既有做法：把本机 warm cache 中经锁 checksum 逐项核验的 427 个 registry archive 与 index 离线复制进登记隔离 Cargo home（426 个直接复制，`sys-locale` 用 checksum 匹配上游的归档），不联网安装/升级工具、不改锁文件或指纹、不预装 `registry/src`；同时以同一校验归档修复 warm cache 中的损坏副本。修复后 `2026-11-11-73-workspace-rerun/` 全部 35 项 exit 0，`cargo test --workspace --locked --offline` 60 套件全 ok、0 failed。原始失败记录与修复后记录分别保留，不以重跑覆盖失败事实。
- 矩阵源规格为六项 capability **16R/45S**，治理 delta **1R/2S**，合计 17R/47S；R07 在原 16R/44S 后增加 S36，6.9/S36 不计入原始 46 项任务。核对口径区分测试断言与 Scenario 全部 WHEN/THEN：S11 已覆盖待定 Undo、组合取消/旧身份失效及用户裁决的 redo 分叉分支；有效新输入截支、无效输入拒绝保支与空字符串无变化保支由不同断言核对，但组合状态仍是模拟状态，不代表原生 IME。S13 的旧解析/投影拒收有测试；写盘回执只在测试中检查 `Written.bytes()` 并调用 `into_baseline`，production host coordinator/实际快照结算未接线，仍为部分覆盖。E1/E2/E4/E5/E7 未闭合；E3 限 basic fixture；E6 仅登记未测项。V-01～V-09 仍无整项通过。 2026-11-02 用户验收裁决：E1–E7 与 M0 出口按现有证据记为验收通过；上述未闭合/限 basic/仅登记及 V-01～V-09 未整项通过的事实保留，不写成已执行或已验证。
- 原始行尾表示、Unicode17 精确准入与 GPUI 指纹批准已迁入[决策记录 §15～17](./decisions.md#15-2026-10-31-原始行尾表示与存储等价确认)，不再依赖临时报告维持契约。
- 单元/集成验证由 `crates/*/tests/` 和源码旁测试执行，固定输入及独立预期在 `tests/fixtures/`；门禁与正反例在 `scripts/`、`scripts/fixtures/`。统一复跑入口为 `python3 scripts/check_workspace.py --cargo-home <已核对的隔离缓存>`，脚本负责将运行输出写入 `logs/validation/`，不自动安装工具或下载依赖。
- 真实 GPUI/IME/焦点与窗口、Intel Mac/Windows10/Linux、远端 CI、已有文件替换/协调/多资源事务等仍未验证；Release 性能已由 2026-11-10 的 360 样本矩阵按分项完成（V-05 首轮，未冻结门槛）。以下 V-01～V-09 继续按各自完整出口执行，不由基础测试或分项记录推定整项通过。

## V-01 · 三平台兼容与发布矩阵

2026-10-31 M0 组 9 分项：当前 macOS arm64 的正式 workspace 统一门禁为 19/19 exit 0、312 passed，见[9.1 原始记录（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.1/report.md)。它只覆盖无窗口工程与纯状态基础；`--all-targets` 和目标依赖图解析不代表其他 OS/架构构建。**2026-11-11 现状更正：** M1 原生 macOS 冒烟已执行（真实单窗显示/退出、文件打开/取消/关闭、14 项主窗窗口矩阵，见 V-09 段），后续 Windows/Linux、Intel Mac/低系统版本与安装/真实输入仍未执行，**V-01 仍未整项通过**；完整交接见[E6（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.5/report.md)。

2026-11-03 增补：已在 `aline-ui` 源码中装配锁定 GPUI macOS 后端与单个初始空状态窗口，但 `cargo check -p aline --locked --offline`、workspace Clippy 和 workspace test 均在 `gpui-pre-apple` 的 Metal shader build 失败，错误为 `xcrun` 找不到 `metal`。这只证明宿主实现待平台编译，不是原生应用构建或窗口冒烟；macOS 真机打开/关闭及失败退出码仍待实际取证。

顺序固定：先验证 macOS，完成后再验证 Windows/Linux；正式 V1 三平台一起发布。Windows 10、Intel Mac 必须保留。

M0 兼容目标登记（2026-10-30，目标候选，不代表支持或验证通过）：

| 平台目标 | CPU / 系统候选 | 显示环境候选 | 可用取证设备与缺口 |
|---|---|---|---|
| macOS | Apple Silicon `aarch64`、Intel `x86_64`；最低系统待测候选 macOS 14，当前实际主机 27.0.1；最终下限未冻结 | macOS 原生显示环境 | 当前可用设备：MacBook Pro（Apple M5，macOS 27.0.1，`aarch64`），内建 Liquid Retina XDR 及外接 4096×2304 显示器；Intel Mac 和低系统版本取证环境尚未接入 |
| Windows 10 | `x86_64`，测试版本候选 22H2；最终最低 build 与 ARM64 是否纳入待确定 | Windows 原生显示环境 | 本次未接入 Windows 10 设备；安装、显示、输入法、系统集成均未取证 |
| Linux | `x86_64`、`aarch64`；发行版样本候选 Ubuntu 24.04 LTS、Debian 12，最终最低版本待测 | X11、Wayland 为待验证候选 | 本次未接入 Linux 设备/VM；发行版、显示会话与系统集成均未取证 |

目标与设备证据及命令见 [M0 引擎地基验证记录（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/README.md)。当前仅 macOS `aarch64` 主机环境已实际观察；上述其余 CPU/系统/显示值均为候选或缺口，不能据此推断平台支持。验证顺序为先 macOS，再 Windows/Linux；**V-01 仍未整项通过**（macOS 冒烟已按分项执行，Windows 10、Intel Mac、Linux 无任何运行时证据）。

第 1 组已取得隔离 GPUI 核心库的 macOS aarch64 构建和各目标依赖解析证据，见[候选审计（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/admission-2026-10-30/report.md)。系统版本候选用于安排后续取证，不是兼容测试结果；Intel Mac、Windows/Linux 的原生构建/运行、真实 IME 和安装分发均未执行，不能将 `cargo metadata`/`tree` 的成功称为平台已支持。

产物：最低系统版本、CPU 架构、Linux 发行版与显示环境、安装/卸载/签名或信任提示、系统菜单、文件关联、重复进程转交、文件路径与中文输入矩阵。增加 Windows AltGr/欧洲键盘布局与 Ctrl+Alt 快捷键交错输入样例，核实实际平台事件而不将旧稿的事件描述视作已验证事实（原历史线索见引擎证据 L1427～1435）。先确定实际测试设备和环境，不用尚未验证的系统泛化“支持三平台”。

时点：M0 确定兼容目标，M1 首轮 macOS 冒烟，M5～M6 按既定顺序完成全平台验证。对应 FILE-02～03、第 7.1 节。

应用图标输入按 [PRD §6.1](./v1-prd.md#61-主窗口与导航)、[UI §3.1](./ui-spec.md#31-应用身份与默认色)及[图标资源说明](./assets/app-icon/README.md)固定：macOS 使用 `aline.icns`，Windows 使用 `aline.ico`，应用内品牌展示与 Linux 使用透明 `aline.png`。验证实际应用包 / 可执行文件 / desktop 配置指向这些资源，在 Dock、Finder、Windows 桌面 / 任务栏及 Linux 启动器按实际可见尺寸检查，覆盖深浅背景、高 DPI 与安装 / 升级后的图标刷新。主体外透明，无原图白色画布、白边、二次圆角或重复留白；不打包原图、预览或旧品牌 PNG。现有文件级透明度与尺寸检查仅作为输入证据，不代表上述安装和显示验证已完成；**V-01 仍未整项通过**。

## V-02 · 导出保真矩阵与真实样本

路线已确定：PDF、`.docx`、HTML；公式/图表允许先用静态图像，须明确可编辑性边界。PDF 使用独立页面样式，HTML 明确离线资源策略，不以丢失复杂内容的纯文本代替导出完成。

产物：“语法 × 格式 × 插件状态”矩阵；标题、列表、代码、表格、图片、公式、Mermaid、HTML、脚注/目录/Front matter 的样本。明确 PDF 页边距默认值与跨页行为、Word 样式、HTML 资源打包/引用、主题与插件关闭后的输出、导出版本及取消/失败处理。真实产物需在对应阅读器打开检查。按 ED-01 增加组合期显式导出：候选更新期间不取对外发布快照，平台自然确认或取消后才取结束后的当前快照；取消等待请求不改输入/历史，不生成产物，结束后的实际产物不含未确认候选。

按已确认 PL-01，增加 Mermaid 开/关 × export 开/关矩阵：独立 SVG/PNG 与图像复制仅由 Mermaid 门控，关闭 export 不隐藏/取消独立图表命令和在途请求，关闭 Mermaid 则撤销相关图像能力。共享产物服务不能只绑定 export 生命周期。仅成功的可执行组合提供真实产物；禁用/拒绝/取消组合记录无新产物、原输出未改和对应状态。按 PL-02 增加关闭高亮使用内容主题普通代码表面及再启用恢复独立配置的样本，Word 已有显式代码样式选项及其他格式样式仍逐项验明，不擅自取消。

时点：M4 实现前冻结契约，M4～M6 验证实际产物。对应 EXPORT-01～06、AC-14。

## V-03 · 图片、公式、Mermaid 与协议支持矩阵

图片格式目标：PNG、JPG/JPEG、GIF、WebP、BMP、SVG，大小写不敏感；Markdown 为 `.md`（含 `.MD`）。图片只读 tab 的适配、1:1、缩放和平移语义已写入 PRD。需验证原生解码、GIF/WebP 动画能力和 SVG 安全处理，分别报告，不用“可打开文件”掩盖未支持的显示能力。

Mermaid 护眼模式使用亮色映射；固定主题不跟随应用，主题是受校验静态数据。产物包括首批图类型、公式语法、错误/超限输入夹具、主题导入格式、重名/删除/错误反馈、图像与源码复制契约。支持清单按实际覆盖率确定，失败保留源码。

按已确认 PL-01/PL-03，检验图表图像复制/输出的独立门控；图片预览关闭保留仍打开 tab 的路径、适应/手动模式与视口，再启用自动恢复。适应模式按当前视口重算且不放大小图；手动模式在当前文件/视口约束下恢复原缩放并约束平移。覆盖窗口/视口变化、图片尺寸/内容/路径变化、加载失败及旧任务晚到；资源变化/失败明确说明保留路径，不恢复已关闭或重启前 tabs。

补齐链接协议 allowlist、拒绝/提示行为、本地文件链接和剪贴板纯文本/图片/HTML/富文本行为；不因为剪贴板有 HTML 就引入新的富文本导入范围。

**2026-11-06 M1 内容夹具展示分项（基础夹具，非 V-03 整项通过）：** 在 macOS arm64 真实窗口（debug、中文界面）打开 `content-showcase.md` 与实体对照夹具，Visual 视图实际显示 Front matter 灰色块、两级目录、行内样式（加粗/斜体/删除线/行内代码/链接/有限 HTML/`<mark>`）、嵌套引用、五类 Alerts（中文名称加彩色左边线）、嵌套列表与任务框、三列对齐表格、围栏/缩进代码、公式与 Mermaid 原文回退、图片替代文字与脚注引用/定义；图片未被加载，HTML 未出现脚本行为。无头 `cargo test -p aline-ui --lib native_window::readonly_content` 20 passed。证据 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/content/`。**未执行：** 图片格式原生解码/动画/SVG 安全、Mermaid/公式渲染后端、链接协议 allowlist 与剪贴板矩阵、Source 视图内容与点击定位、字体/缩放变化。本分项不表示 V-03 整项通过。

时点：M1 建基本夹具，M4 前固化格式与支持契约。对应 MD-05、IMAGE-01～06、PLUGIN-01/04/07/09、PREVIEW-01～04。

## V-04 · 主题包与原生字体能力

先验证锁定环境的动态静态字体加载、中文回退、主题切换后缓存/布局刷新，以及三平台背景透明度。透明背景不支持可回退到同色不透明；字体无法激活不能静默忽略，须解决能力阻塞或记录新的产品决定。

产物：对照[主题包规范](./theme-package-spec.md)制作有效与恶意/损坏包样本，验证正式 schema 1、原型 schema 2/3 拒绝、封闭字段及唯一色源、GitHub 同职责同模式 → common → modes 深合并（数组替换、null 拒绝）、静态 PNG 预览、字体角色回退、ZIP 路径校验及授权。验证字体/透明能力与缓存刷新。验证包体/解压/字体限额，但限额数值须由实验确定，不得在本计划中臆造。验证同 ID 冲突拒绝、无应用内覆盖升级/删除、手工备份替换/删除并刷新、成功安装不自动应用、活动主题失效保留选择身份并回退说明、修复/重装/刷新/重启后显式重选、旧身份不得盖回失效期间新选择、固定代码模式保留及有效默认配色；受保护应用内 GitHub 根作为继承及最终安全回退，磁盘副本用户修改升级保留。主题导入失败不得破坏原主题。

按已确认 PL-02，验证高亮开启时两维跟随/固定、关闭时普通代码背景/文字/行号来自当前内容主题，以及关闭期间切换主题后再启用按保留配置解析当前值；固定维度不被覆盖，跟随维度使用当前应用设置，核心 Markdown 标记着色与控件保留。schema 1/字段/合并规则已按 TH-01～TH-10 确认；完整默认值、数值边界/预算及字体能力仍待本项实测。

**2026-11-05 中文排版与字体测量门（方法采纳，未执行）：**增加全角标点行首/行尾避头尾、标点挤压与字体回退、中英文/emoji 混排、合成斜体（粗/斜/粗斜）及 CJK 标点字体回退夹具，在锁定 gpui-pre 上记录实际换行、字形与回退结果。quiet-md 的 14° 等数值只作参考，不预设 Aline 值。若确认锁定 GPUI 缺陷，先记录能力缺口并决定是否 fork/vendor 三个 gpui-pre 包；vendor 方案另走依赖准入与补丁清单，不在本项直接实施。M1 是否允许把该缺口列为已知限制仍待用户裁决。

**2026-11-06 M1 字体分项（部分验证，真实窗口 + 无头）：** `FontEnvironment` 已接最小启动适配与有效字体代次：注册失败/缺字族保留此前字族与代次，失败映射到 `font.register_failed`/`font.family_missing` 界面提示；字族可见后才推进代次，`font_generation` 进入布局键并有无头 1→2 代次失效断言。`cargo test -p aline-ui --lib fonts::` 5 passed、`native_window::readonly_layout` 15 passed、全 lib 103 passed/1 ignored。真实窗口 `fonts.md` 的 Visual/Source 已核对中文标点、英文标点、Emoji（含 ZWJ 家庭/旗帜）、组合字符、韩文/日文与中英混排，状态栏 `186 字符 12 行`，无字节变化，证据 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/fonts/`。**未执行：** 有授权静态 TTF/OTF 的真实激活/损坏/缺字族实验与字体变化后的锚点/命中——已自制授权实验 TTF 并临时登记 `BUNDLED_FONTS`，应用启动后会话来锁（`CGSSessionScreenIsLocked=true`），AX 读取 `permission_denied`、截图失败，登记已还原并重建；注册失败/缺字族目前只有无头 FakeBackend 用例。未下载任何第三方字体。本分项不表示 V-04 或字体能力整项通过。

**2026-11-07 M1 字体分项补测（真实窗口，解锁后）：** 上一段的“未执行”项已完成。v1 自制字体虽 `add_fonts` 成功但未生效，根因是锁定 GPUI `gpui-pre-apple-0.3.9/src/text_system.rs` 的 `load_family` 跳过缺少 `m` 字形的字体；新增含 `m` 的 v2 夹具（SHA-256 `9ab780b5…e5e1`）后，真实窗口 A/B/C 显示为自制三角/方块/菱形，中文、中英标点、Emoji（含旗帜）与中英混合行逐字回退正常，初始与加载后均无错误提示。字体激活后 Source↔Visual 往返记录 `m1-anchor restored source=0 … glyph_y=122.5 viewport_y=82.5`，真实像素命中 `precision=Exact source_start=57 edge=false layout_text_matches_projection=true history=0 revision=0`，落在 CCCC 行源码范围 54..68 内。损坏字体初始空窗显示「字体数据无法注册；继续使用当前字体，文档与位置未改变。」，缺字族显示「字体已安装但所选字族不可用；继续使用当前字体，未激活新字体。」；打开文档会清空单槽反馈，故以初始空窗截图取证。实验后已还原 `BUNDLED_FONTS` 为空列表并重建（调试二进制 SHA-256 `45e0aaf8…b68b`），夹具 mtime/SHA 未变，`cargo test -p aline-ui --lib` 103 passed/1 ignored，`cargo fmt --check`/`git diff --check` 无输出，证据 `logs/validation/m1-single-window-readonly/2026-11-07-fonts/`。**边界：** M1 无运行中字体切换 UI，只覆盖启动时激活；仅 macOS arm64、自制字体、单一窗口尺寸，运行中字体重排、三平台、主题包字体与透明度矩阵仍未测。本分项不表示 V-04 或字体能力整项通过。

时点：M1 字体实验；M4 实现前冻结格式和实测限额，M6 完成发布矩阵。对应 THEME-04～09、AC-12～13。

**2026-11-08 QMD W1/W2 补充准备（未完成）：** 离屏 change `m1-visual-evidence-spike` 已核对锁定源码接口；候选 `test-support` 特征图在离线解析时缺 `futures-concurrency`，尚未完成依赖准入或真实离屏截图。中文排版 change `m1-cjk-typography-font-evidence` 已准备夹具与观测完整性检查，正文宽 320/480/768 × 两视图的真实测量和决策仍未完成；不据此判定 V-04 通过。

**2026-11-09 W1 精确隔离准入：** 用户批准仅 W1 验证使用 `gpui_macos=[font-kit,test-support]` 的精确候选图，锁 SHA-256 `6bb7d273edc39dc7153faf6417cac2c697364d0273926c198a06f74f0c94cf06`、macOS arm64 GPUI 指纹 `2dd7f852cdec117c5bd768d3eac1572253a0ffb0f12e9a8aca0402ca73f7f32f`。完整 metadata 507 包、本机选中 357 包；相对生产图新增 72 包（本机新增 23）、9 包 feature/边变化与五条新 GPUI 分支。新增归档 checksum、完整身份/入边/许可证及五引擎隔离已独立复核；不新增生产运行 feature、不替换生产锁或全局准入。`option-ext` 口径纠正见决定 §27。申请在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w1/isolated-audit-final/`，复核在同轮 `review/w1-admission-review.md`。获准不等于离屏运行成功，实际 spike 结果另记。

**2026-11-09 W1 真实离屏完成：** `scripts/run_w1_offscreen.py` 在 logs 独立八成员副本和从已核对归档重建的新缓存中，按固定 `scripts/fixtures/w1-offscreen/{Cargo.lock,approval.json}` 核对完整批准图及逐包准入，显式 `aarch64-apple-darwin` 构建并验证唯一 Cargo 工件/profile/lipo/二进制/源码/锁身份。两个独立进程各连续两次从真实 Metal drawable 读取 RGBA，均逻辑 640×400、scale=2、像素 1280×800，每份 4,096,000 bytes，四份 SHA-256 均为 `233c7ef4035f04f3ec495049a8dede6524713eccd2b278c764234df6a2bb2c00`，内容有两个色块与中文/ASCII 字形、非单色/非全透明。原始 RGBA/PPM 与无损预览保留，主 Agent 已视觉核验。fmt/Clippy（含此隔离场景 all-targets）通过；运行器正反例和独立复审完成。

实际成功记录为 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w1/runtime/real-03/summary.json`、`binary-receipt.json` 与 `process-*`；前两轮归档原始 checksum/新 lint 失败保留、不覆盖。此为 debug 的最小场景离屏能力，不是 Aline 产品 UI 全覆盖、真实屏幕呈现、点击/焦点/IME、缩放/锁屏或性能成绩。W1 的限定依赖和完整图绑定仅服务该验证；生产清单、锁、features 及全局策略均未改。能力 spike 完成不改变 V-04/V-09 整项未完成状态。

**2026-11-09 W2 六组合实测：** macOS arm64、锁定 GPUI 0.3.9、Release 中文窗口以正文宽 320/480/768 逻辑像素 × Visual/Source 完成六组合；真实窗口宽分别 372/532/960 pt，截图 scale=2，按 geometry 唯一列宽/槽位/内边距与截图边界交叉核对。六组普通正文连续墨迹行带间距均为 56 物理像素，即 28 逻辑像素，ROI/原始坐标在同轮 `w2/pixel-measurements.json`；完整性检查 `evidence_complete: true`、`capability_passed: null`。

截图确见禁行首闭标点（Visual 320 的 `？`、Visual 768 的 `》`/`；`），Visual 中文斜体直立而 English 倾斜；Source 按 Menlo 请求显示标记和普通文字，本来不渲染 Markdown 粗斜体。`scripts/fixtures/m1-cjk-punctuation-minimal.md` 在 320 正文宽真实复现行首 `？` 与中文/英文斜体差异。实际 fallback font run、glyph advance 与平滑开关 A/B 尚未取证，不从墨迹宽冒推这些数值。用户已选择 M1 内最小核心断点修复准备方向（决定 §28），独立 change `m1-cjk-linebreak-core-fix` 的规划工件已齐备；具体补丁/来源准入、实际字体 run 与修复实测未完成，未接受已知限制，不判 V-04 通过。原始六组合与最小样本截图在同轮 `gui/w2-*`，观测为 `w2/observations.json`。

**2026-11-09 CJK 断点准备覆盖（未通过）：** change `m1-cjk-linebreak-core-fix` 在当前 M1 分支准备独立金样 12 个文本 / 22 个用例（320/480/768），现状 oracle 复刻在 6 个禁则用例失败、1 个超宽簇用例拆簇；私有纯规则 13 个测试通过；锁定 `gpui-pre@0.3.9` 归档 checksum 一致，最小 diff 已接入三个真实折行入口且 `rustfmt --check` 通过。候选本地来源的 metadata/tree/入边/features/许可证/五引擎隔离材料已完成，但未获批准，现行 `check_engineering.py` 实际按“原生路径精确身份/来源/校验和漂移: gpui-pre@0.3.9”失败关闭。覆盖范围仅限合成度量下的断点选择、纯规则与隔离 diff；未编译、未激活、未做真实 shaping/像素/命中/截断或三平台测量，不改变 V-04 未通过状态。证据 `logs/validation/m1-single-window-readonly/2026-11-09-cjk-01/cjk-linebreak/`。

**2026-11-09 CJK 断点修复复核轮（未通过）：** `logs/validation/m1-single-window-readonly/2026-11-09-cjk-02/cjk-linebreak/` 复核 1.1 金样（12 文本 / 22 用例）、oracle「目标反例失败、普通正例通过」、2.1 的 13 个同源纯规则测试与 `forbid(unsafe_code)`，并给出 4.1/4.2 的「已证 / 未知」边界：W2 真实 macOS arm64 样本只证明 Visual 中文直立、English 倾斜与 Source 的 Menlo 普通标记显示；四种 requested Font 对应的实际 shaped run opaque `FontId`、可得 face 与真实 advance **未取证**，记录这些值所需的受限 opt-in 平台诊断钩子按指示只列最小接口与准入审阅后停下。独立复核发现候选本地来源的 diff / 文件哈希存在三代互不相同的身份，且文档称「未应用」的接口提案已落在磁盘隔离 diff 上，已列为待确认并暂停依赖该决定的实现。本轮**未构建、未激活、未批准、未编译**候选来源，未取得真实 shaping / 像素 / 命中 / 截断证据，不改变 V-04 未整项通过状态；合成斜体角度、标点挤压与平滑数值仍未冻结。

**2026-11-09 裁决落定（不改变 V-04 未通过状态）：** 用户授权按面向以后功能迭代的最优方案落定两项裁决，见[决定 §29](./decisions.md#29-2026-11-09-按授权落定的接口裁决与来源准入结论)与 `logs/validation/m1-single-window-readonly/2026-11-09-cjk-03/cjk-linebreak/decision/recommended-ruling.md`。接口选 A1+A4 受检组合与最小公开面（`#[non_exhaustive] WrapLimit` + `TextLayout::wrap_limit() -> Option<WrapLimit>`）；受限状态的用户可见提示另立后续 change，本 change 不接线；来源准入结论为**暂不批准**任何一代身份、**暂不允许 3.2 编译**，仍需单一自洽代 + 独立评审 + 具体批准。spec delta 未改、未放宽。本轮仍未构建/激活 patched GPUI、未启用 test-support、未取得任何真实 FontId / face / advance / 像素 / 命中 / 截断证据；4.1/4.2 的平台诊断钩子仍按指示停在最小接口与准入审阅阶段。合成斜体、整元素 skew、整篇字体替换、平滑数值仍未实现；V-04 仍未整项通过。

**2026-11-09-cjk-04 具体批准后的隔离编译（不改变 V-04 未通过状态）：** 用户按 `decision/concrete-approval-gate.md` 默认建议值给出具体批准，授权 3.2 **仅在 logs 隔离副本编译**，生产 `Cargo.toml` / `Cargo.lock` / 策略资产零变更。已收敛出 `single-regenerated-cohort`（归档 `34f61c1c…`、`minimal.diff` `aba1c841…`、全部 8 文件哈希）、重算七场景指纹并全部匹配、修复复跑配方、移除候选测试目标、把两处未登记编译尝试归一化为历史；现行守卫对候选 `[patch]` 图仍按精确身份失败关闭。隔离副本内 `cargo check -p aline-ui` 退出 0、三入口回归 13/13、原 `aline-ui` 映射/命中 111 passed / 1 ignored。**这些是合成度量下的编译与布局回归，不是真实 CoreText shaping / FontId / face / advance / 像素 / 命中证据**；`Some(NoLegalBreak)` / `Some(ClusterWiderThanLine)` 在最小公开面下只存在于 `pub(crate)` 层，外部 crate 不可读，需真实 Window（5.1）。独立评审已由两个独立 `codex` worker 执行，两份 verdict 均 `pass-with-findings`（两项高 finding 未清零，见 `review/independent-review-summary.md`），仍无 Release 六组合重测。证据见 `logs/validation/m1-single-window-readonly/2026-11-09-cjk-04/cjk-linebreak/`；V-04 仍未整项通过。

## V-05 · 性能设备、测量和发布门禁

按 V-01 顺序选择各发布平台代表设备，使用 Release 构建。准备 32/128 KiB、1/4 MiB 及巨型段落/表格、资源密集和错误输入夹具，分别记录冷/热缓存情况。

产物：打开、输入、块提交、查找、模式切换、滚动、图片/图表处理的 P50/P95、最长主线程停顿和内存报告。引擎处理与端到端输入到呈现分别计时；先有实际基准，再固化尚缺的打开耗时、帧率及内存门槛。PRD 的处理预算仍是目标，不是实测成绩。

由 [Splat 静态源码研究 §5](./splat-study.md#5-解析局部更新性能与异常输入)补充的实验问题（**未执行**）：紧凑映射、块复用、批量/分批派生刷新是否在保持同一来源关系的前提下降低实际成本。沿用上述规模，另加“许多短块/单个巨块”、密集实体/转义/emoji、远处引用定义变化、光标块与相邻相同块样本。分别计快照/坐标转换、解析、映射构建、布局、主线程应用、等待排队及最终呈现，记录分配和峰值内存；对优化分支与全量/未压缩路径做差分并记录优化命中率。正确性通过条件是块/源范围/映射精确性/保护/导航一致、复用后重新定位、占位与完成态同一投影文字和来源不变，且没有旧结果覆盖输入；性能结论按实际端到端报告评估，不复制 Splat 的 300/120 块或 16ms 参数，不把分批循环预算冒充巨块最长停顿保证。此实验不预先启用局部 Markdown 解析，不更改 PRD 性能门禁。

**2026-11-02 M1 样本生成器：** `scripts/generate_m1_fixtures.py --output-dir <evidence>/fixtures-generated` 以固定 seed 生成 32/128 KiB、1/4/5 MiB 五个尺寸样本，并为许多短块、单巨段、长行、巨表、密集实体各生成 128 KiB 复杂度样本；清单记录精确字节数、SHA-256 和首尾 32 字节锚点。本轮在两个独立输出目录重复运行，10 个文件字节和清单相同。大文件仅留在 `logs/validation/`。这只是可复跑输入准备，不是解析/布局/滚动性能测量；V-05 仍未执行。

**2026-11-05 报告字段（方法采纳，未执行）：**每次 V-05 运行至少记录环境与版本、源码 commit/锁定构建、样本 SHA-256、方法/操作序列、冷热缓存、计时起止、P50/P95、最大停顿、峰值内存、原始日志入口、复现命令及明确不测量项。引擎处理、组提交与端到端呈现分开计时；摘要数字须可追溯到机器可读原始输出。字段模板不冻结任何毫秒/内存门槛，门槛仍按本项实测与 PRD 目标冻结。

**2026-11-08 采集链复核（未完成性能测量）：** 汇总器与收集器补充交叉矩阵、运行身份、计时边界、逐样本指标和实际源码字节的失败关闭检查；探针尚未提供该收集契约、RSS、可信主线程停顿及冷热控制。已有单次 Release 探针烟测不是 20 次矩阵。`first_present` 记录 draw 提交后的 effect，不是屏幕扫描；事件循环间隔含调度抖动，不是精确主线程停顿。真实矩阵、P50/P95 与 V-05 判定仍缺证据。

**2026-11-06 M1 只读虚拟化分项（部分，未执行性能测量）：** `aline-ui` 已实现变高块/逻辑行双视图虚拟化、256 项高度缓存与布局键失效、1,000,000 行上限、64 KiB 单项映射窗口、>4 MiB 默认 Source；无头 `cargo test -p aline-ui --lib` 100 passed/1 ignored（命中 9、布局 15、内容 19、字体 5、阅读渲染 11；含 2026-11-06 复核与独立审查修复回归），1 MiB debug 首尾滚动见任务 5.1。**未执行：** Release 构建、20 次冷/热采样、P50/P95、最长主线程停顿、峰值内存、虚拟实例统计与真实滚动/切换序列（会话锁定阻塞）。**测量入口：** `scripts/run_m1_readonly.py` 已提供 performance/window-baseline/readonly-bytes 三套件与自测；performance 对错误、缺指标、取消、旧运行和不足 20 样本的记录一律非零，合成正例只验证入口逻辑，不是本轮成绩。本分项不构成 V-05 通过或 AC-16 判定。

**2026-11-06 M1 真实窗口滚动/切换补记（仍非性能测量）：** macOS arm64 debug 窗口（960×672）完成 1 MiB 夹具首尾/翻页/轨道滚动、第 24,407 行附近 Visual→Source→Visual 锚点恢复（source=597995）、900/600/900/960 换宽（`max_offset=1198024px` 保持、高度缓存重建、锚点恢复），以及 >4 MiB 默认 Source 分项：5,345,600 字节中文夹具默认 Source 并显示 `reading.large_source`，切 Visual 后提示清除、切回 Source 仍清除；恰好 4,194,304 字节夹具默认 Visual 且无提示。三个 131,072 字节复杂度夹具（长行/巨表/单个巨段）在 Visual 均显示布局预算提示，Source 均完整可读。原 5 MiB dense-entities 夹具因绘制字符数超过 `ParseBudget::m0().max_mapping_points()` 返回 `Mapping(BudgetExceeded)`，真实窗口以 `open_error.internal` 拒绝并保留旧文档；该行为是解析硬预算，不是视图降级，提示归入内部错误而非预算错误。证据 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/view-switch/`。**仍未执行：** Release 构建、20 次冷/热采样、P50/P95、最长主线程停顿、峰值内存、显示缩放变化与授权字体下的真实字体重排。本补记不构成 V-05/V-06 通过。

时点：M1 建基准，M6 冻结且通过全部发布门禁。对应第 7.2 节、AC-16。

**2026-11-09 M1 Release 部分性能矩阵（未通过 V-05）：** macOS arm64 的 9 类尺寸/复杂度 × 冷/热 ×20 次，共 360 个原生进程均成功采集，形成 126 个分段统计组。每个进程有 PID/起止/原始输出 SHA/事前 fixture SHA/构建工件/完整源码清单收据；墙钟只关联运行时间线，duration 与真实 Instant 起止 offset 同钟精确核对，固定量化反例及 10,000 次实际时钟不变量通过独立复审。冷条件仅为新进程且无同文档预热，启动空窗已触及字形/管线；未清 OS 文件缓存，事前哈希也已读文件。热条件为同进程完整预热后重开，不能称为磁盘热/冷隔离实验。

缓存身份补核：W1 新鲜归档门禁发现 `sys-locale@0.3.2` 旧缓存 raw `.crate` 为本地重打包（含 AppleDouble/Cargo 标记，raw SHA 不同），不能把该 raw 文件当官方归档。后续对 499 个 registry 包共 26,450 个官方归档普通文件与实际 metadata 指向的解包源码逐一核对，缺失/变化/额外源码均为 0（只排除 Cargo 标记）；sys-locale 的 15 个官方文件也完全等价。证据在同轮 `w1/runtime/source-audit/`，原缓存未覆盖。现有生产构建/360 次运行可归属相同依赖源码内容；后续 W1 运行仅在新缓存取锁 checksum 匹配的官方归档，不继承该重打包 raw 身份。

1 MiB 的 parse/mapping P50/P95：冷 27.58/34.84 ms，热 22.66/24.61 ms；首内容布局（render→子元素 prepaint）冷 4.04/6.26 ms，热 3.90/3.99 ms。这些不是端到端首屏呈现。模式切换/滚动到 draw 提交的观察与队列/reader-index 诊断均保留原始边界，不冒称屏幕呈现或精确 UI 停顿。全矩阵进程峰值 RSS 最大 1,041,940,480 bytes，由独立 worker 对唯一原生子进程的 getrusage 高水位取得；热条件包含预热，不是逐阶段峰值。

原始逐次收据、126 组 P50/P95 与方法在 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/performance/matrix-20/matrix-summary.json`。采样期间文档/tasks/W1 验证脚本有变更，变化清单和前后完整源码身份已保留；Aline 编译输入与运行二进制未改变，数据只归属于采样前冻结清单，不冒用为后续新代码成绩。标准入口仍 exit 1、`passed: false`：缺真正 first_present 和精确 main_thread_stall_ns，不把 draw effect/事件循环间隔或潜在 hang 的空表填成真值。System Trace/Metal Trace 仅取得能力与原生事件表，尚未把逐内容帧关联成可信指标；7.1 与完整 AC-16 保持未完成。

同轮 `performance/trace-bridge-smoke/` 的真实 Metal Trace 可按同 PID 的 command-buffer→frame-number/client surface→displayed-surfaces 表建立原生帧关系，但尚无 Aline 首内容帧到该关系的精确身份桥接。该烟测另记录主线程 Microhang 371,737,530 ns；当时并发 W1 重型构建且有 trace 开销，平台 hang 检测也有阈值，既不并入 360 次矩阵，也不称逐次精确最大停顿。锁定公开接口只返回 draw 提交/回调，profiler 的 Present 仍是 CPU 提交；后续需最小、安全且可审阅的内容帧/command-buffer/drawable 身份桥接及逐样本停顿观察，不能仅用墙钟就近配对。

**2026-11-10 M1 只读性能首轮 Release 矩阵（7.1 分项完成，未冻结门槛）：** macOS arm64（macOS 27.2，32 GiB，Xcode 27.2，rustc/cargo 1.98.1，`gpui-pre`/`gpui-pre-macos` 0.3.9），Release 构建 SHA-256 `06740b89…`，源码 commit `1893d4ac…`；9 类尺寸/复杂度 × 冷/热 × 20 次共 360 个原生进程、每样本 8 个阶段行，144 个（条件×冷热×阶段）组各 20 条成功样本；nearest-rank P50/P95（升序第 ceil(p/100·n) 个值，每组至少 20 样本）。标准入口 `python3 scripts/run_m1_readonly.py --suite performance --output-dir <evidence>/performance --require-matrix` exit 0、`passed: true`、`failures: []`（`summary.json`/`summary.md`）。

方法：`read_decode`/`snapshot`/`parse_mapping`/`main_thread_adopt` 为进程内同钟 Instant 边界；`first_layout` 为首内容 render 入口→根子元素 prepaint；`first_present` 改为 Metal System Trace 的 command buffer→CA client present request→displayed-surfaces-interval 显示链，取屏幕扫描开始（`external-trace-display-chain`），要求主帧 commit 身份、present 因果序与 1 ms trace 起始不确定度；`main_thread_stall_ns` 改为窗口内主线程 `individual_iteration` 扣除 `waiting_for_events` 后的最长连续忙碌段（`external-trace-runloop-busy`）。不再用 draw effect 或事件循环间隔充数。操作序列：每样本真实打开夹具，模式切换与 PageDown 滚动为程序化真实请求；冷=新进程且无同文档预热（不声称 OS 文件缓存冷，事前内容哈希已读取文件），热=同进程完整预热后重开（不声称磁盘热隔离）。计时起止：阶段 duration 与单调起止 offset 同钟，外部呈现链用 trace 相对时间并保留 1 ms 起始不确定度；逐样本原始边界在 `samples.jsonl`。

结果入口：`logs/validation/m1-single-window-readonly/2026-11-10-performance-01/performance/summary.md`（144 组全表）、`samples.jsonl`（2880 样本行 + 360 运行行）、`summary.json`。示例（32KiB 冷/热）：`first_present` P50/P95 64.209/77.297 ms 与 39.573/43.557 ms；`first_layout` 2.943/4.437 与 1.030/1.401 ms。1 MiB 冷 `parse_mapping` P50/P95 48.025/52.083 ms；4 MiB 冷 `first_present` 217.019/231.940 ms。全矩阵最长主线程停顿 93.479 ms；峰值 RSS 1,036,763,136 bytes（worker 唯一原生子进程 `getrusage` 高水位，热条件含预热，非逐阶段）；虚拟实例上限 29。fixture SHA-256：32KiB `d24324b2…`、128KiB `24eebfe0…`、1MiB `10e02e0b…`、4MiB `09f1ea66…`、>4MiB `ead464ec…`、many-short-blocks `24eebfe0…`、single-giant-block `bce86ce2…`、giant-table `fe3e6def…`、dense-entities `0873a671…`（精确字节数见 summary.md）。

证据代与补采：首轮 360 次尝试中 324 次成功；13 例由修正后的 trace 推导从失败证据恢复，9 例失败与 14 例不可用经两轮补采（21+2 条，新编号 20–22）替换；全部 324 条原收集记录用最终推导器统一重推。`rederive-report.json`、`samples-sources.json`、`samples.initial.jsonl`、`progress-reconciled.json` 记录来源目录与逐用例哈希；合并后 360 个启动 PID 唯一、每组 20 条，`check_validation_output.py` 12 项通过。补采与合并未修改 20 次下限、nearest-rank、阶段归属、运行身份/时钟不变量等拒绝规则。

明确不测：编辑/输入法/查找/资源后端时延不适用 M1，未测且不填零；Windows 10、Intel Mac、Linux 未测，不以 macOS arm64 推断；完整设置/菜单/主题矩阵未测；20 次是首轮采样方案，不是新增发布门槛。`first_present` 是屏幕扫描开始的下界，不声称像素已进入人眼；trace 录制本身有开销，阶段值与无 trace 运行不可直接混比。复现命令同上；补采与合并命令见 `tmp/m1-single-window-readonly-resume.md` 与 `rederive-report.json`。

判定：本分项只完成 7.1 的真实 Release 20 次测量与 V-05 字段回填，不冻结新性能门槛、不判完整 AC-16、不声称 M1 出口通过；8.2、7.3、V-09 整项、M0 豁免与 M2 前 V-08 保存闭环保持未关闭。

**2026-11-11 M1 1 MiB 真实窗口可视化滚动（8.2 第 6 行分项，非性能成绩）：** 此前 §13 第 6 行的 1 MiB 真实滚动记录是 2026-11-06 的 debug 构建，早于 2026-11-09 的渲染重写；2026-11-10 的 Release 性能矩阵虽覆盖 1 MiB，但那是程序化探针、无窗口截图，其口径自述「不代表屏幕呈现」。本轮在当前 Release 二进制上重取真实窗口证据：`python3 scripts/capture_m1_scroll_window.py --output-dir <evidence>/2026-11-11-82-scroll` exit 0、`passed: true`、`failures: []`。

夹具为 1,048,576 字节、17,772 个唯一编号段（`## Section NNNNNN`），末尾追加一段围栏代码块，块内是全篇唯一的 `ALINE_M1_END_OF_DOCUMENT`；夹具字节与 SHA-256（`b739ff30…`）固定在脚本内，被改动即拒绝。

位置证据有**两个互相独立的来源**。截图侧：从滚动条拇指在轨道中的位置读出滚动比例（应用按 `thumb_top = 轨道上沿 + (轨道高 − 拇指高) × 比例` 画拇指），并从正文区整行浅底的代码块认出末尾标记。实测顶部比例 0.0、真实滚轮两次后 0.0027、反向滚轮后 0.0018、滚到末尾比例 **1.0** 且末尾标记块出现在画面上（截图可见 `Section 017771`/`017772` 与 `ALINE_M1_END_OF_DOCUMENT`）、Home 回顶后比例 0.0。应用侧：文档关闭时应用把自己的回调序列落盘，本轮为 213 次 `scroll-wheel` + 2 次 `scroll-key`（End/Home），且 `has_draft=false`、`history_len=0`、`document_closed=true`、`disk_bytes_equal=true`、`source_bytes_equal=true`。

方法与边界：滚轮一次 12 页约产生 71 条回调事件（每次视口移动都会重新渲染并触发滚动回调），而应用的回调序列上限是 256 条，因此真实滚轮只投递两次、跨文档的长距离跳转交给应用自带的 `End`/`Home` 键（`ReadEnd`/`ReadStart`，走同一条滚动路径）；上限一旦被滚轮占满，后续事件会被丢弃并只留一条 `event-limit-exceeded`，本轮已显式拒绝该状态。截图读数只证明「可见内容变了」与「滚到了末尾」，**不**证明首屏呈现时延或帧率——那属上段 V-05 矩阵；本分项也不冻结任何性能门槛，不表示 V-05/V-06 整项通过。

## V-06 · 资源上限与可恢复降级

组 9 证据归集（2026-10-31）：预算/取消/非法输入基础已在最终 312 项 workspace 测试中执行，并按四核心公共 API 及原始输出建立[E2 发现清单（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.3/report.md)。本次未扩量性能/资源实验，注入预算与合作取消不冻结发布限额，也不证明第三方长调用可中断；V-06 未通过。

编码及行尾已确定：仅有效 UTF-8，保留 BOM；混合 LF/CRLF 编辑后多数派，平票 LF。>4 MiB 默认源码模式是视图降级，不等于无限制接受文件。

产物：文件、单段/单表、图片像素/解码内存、公式/图表源码及生成资产的上限与时限；主题 ZIP 包体/解压/字体资源也在 V-04/V-06 联合测量。触发后的明确错误、源码回退或打开拒绝路径；增加有效 UTF-8 但以 `%PDF`/`PK` 开头的 Markdown 样本，确认不因签名本身拒绝。按类去重的会话级降级汇总须有身份/版本/容量/生命周期，验证 UI 呈现、不持久化正文/IME 候选/敏感路径且不使用引擎 thread-local；汇总容量与 UI 布局由本项和 V-09 实测确定。验证不会截断原文、卡死或覆盖输入；超限规则加入格式与性能夹具。产品选择确认不预设或暗示任何资源限额数字。

时点：M0 固定编码用例，M1～M4 测量限额，M6 回归。对应 SAFE-01～03、第 7.2 节。

M0 编码基础证据（2026-10-31）：4.1～4.4 已固定解码/编码/往返与 create-only I/O 用例，回归入口为 `crates/aline-source/tests/`。已覆盖头部 BOM 分离、正文 U+FEFF、LF/CRLF/混合多数与平票、孤立 CR、无尾换行、有效 UTF-8 的 `%PDF`/`PK`、非法 UTF-8/UTF-16 BOM/截断/预算拒绝、原 bytes 通道、改后 BOM/多数派编码、逆补丁精确恢复、新基线替代、create-only 发布及写入/同步/竞争失败。未覆盖本项要求的文件/单段/单表/图片/公式资产上限与时限、会话级降级汇总及 UI 呈现、磁盘满/权限/断电/跨平台；这些不能由 M0 局部用例外推。V-06 仍未通过。

组 6 解析预算/敌意输入基础证据（2026-10-31）：`aline-markdown` 已把解析预算实现为八个可注入维度（源码字节、重写字节、映射片段、扫描步数、嵌套、节点、HTML 栈深、投影索引点），`ParseBudget::m0()` 的数值只是保护上限、**不是本项要冻结的发布限额**。`tests/hostile.rs` 覆盖深层嵌套、巨型输入、预算注入与预取消；`ParseStats` 记录实际扫描步数、投影索引点与预算触发次数，测试对 321 字节 HTML 密集源二分得到最小可通过扫描步数 80 并与统计值交叉一致（历史探针观察，当前回归见 `crates/aline-markdown/tests/hostile.rs`）。**复核 N-F2-2 限定**：该交叉一致原先只覆盖未过滤 HTML 样本；过滤标签（`script/style/...`）下统计值曾与真实阈值不符（8 vs 5、6 vs 3），修复后补了过滤样本交叉验证，回归见 `crates/aline-markdown/tests/hostile.rs`。协作式取消经 `CancelCheck`/`CancelToken` 注入，`parse_with_cancel` 在入口、重写编辑间、事件前与构树收尾循环轮询，取消返回 `ParseError::Cancelled` 且不产出树。未覆盖：真实端到端时延与峰值内存、1/4 MiB 与巨型单段/表格规模、图片/公式资产、会话级降级汇总与 UI 呈现、第三方 `pulldown-cmark` 内部长调用不可中断这一盲区、三平台。V-06 仍未通过。

**2026-11-02 导航预算分项：** `tests/navigation.rs` 以注入 `ParseBudget::max_nodes` 覆盖导航槽位 `entries + Σtargets` 的恰好等于与边界-1：10 个标题 + 10 个 Toc 的 T×H=100、10 个引用 + 10 个定义的 R×D 双向=200；超限触发整源 `RawFallback`、导航为空且 `budget_hits` 只增一次。该测试只证明纯数据索引的预检口径，不证明 `ParseBudget::m0()` 发布限额、真实峰值内存或端到端时延，V-06 仍未通过。

## V-07 · 编辑、格式化与只读语法夹具

组 9 证据归集（2026-10-31）：E3 的展示树 dump 与原始范围、E5 的块+Gap/来源/对齐映射见[9.3（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.3/report.md)，E4/E7 的 AC-01～04 基础与 TA-01/TA-02/ED-01 纯协议见[9.4（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.4/report.md)。最终门禁 312 passed 只证明当前发现并执行的基础；真实 UI 命中、灰色只读呈现、系统字素/IME、通用命令/跨块结构/格式化与尚未冻结的语法口径不因证据归集而通过。V-07 与 AC 整项仍未通过。

R07 字素边界基础（2026-11-01）：`aline-markdown` 提供绑定完整当前投影的 Unicode 17 `is/next/previous_grapheme_boundary`；区分 UTF-8 scalar 端点与扩展字素边界，覆盖 combining mark、RI、ZWJ family、Indic conjunct 版本金样、投影身份/完整上下文拒绝及扫描预算。每查询累计供给上限 4096 UTF-8 字节、每块 256 字节，超限拒绝。独立期望确认 Unicode 17 下 U+094D U+0915 为 `[0,3,6]`，Unicode 18 为 `[0,6]`；此差异已按用户准入接受。实现、测试和依赖准入原始记录见[R07（历史）](https://github.com/zsxink/Aline/tree/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/remediation/R07/run-3)。这是纯边界机制证据，不覆盖按键导航/删除、真实 UI/IME、Unicode 18 或三平台系统行为；V-07 与 AC 整项仍未通过。

已决定：基础跨块选择/复制/删除/粘贴；复杂结构命令可以拒绝并提供源码入口；表格复制优先 Markdown；缩进代码更改语言可转围栏并单步撤销。格式化仅菜单手动触发，全文差异确认后应用。

按已确认 TA-01 增加同块多个 900ms 输入组、IME 确认/取消、离块、Undo/Redo 与自动保存/恢复/导出交错用例；提交可保持块打开，离块不重记或压缩已有历史组，快照不改变分组。按 ED-01 增加组合期手动保存/另存为/导出的可取消等待，分别验证平台确认、平台取消和用户取消请求；等待及取快照不强制结束组合、不封口输入组或改历史，结束后读取的是当前文本而非请求发起时的候选。按 RS-02 验证跨模块/跨 crate 的坐标 newtype/受检范围与补丁原生表示的转换、非法端点拒绝及版本/投影身份。

M0 编码基础证据（2026-10-31）：行尾边界金样已固定 CR-X-LF 删除 X、边界插入/删除/粘贴、LF/CRLF 多数派与孤立 CR、再解码限定行尾等价及逆补丁精确恢复（回归见 `crates/aline-source/tests/`）。这些只服务编辑/格式化的编码基础，不含本项的块打开/提交分组、IME、跨块命令、格式化差异确认、只读保护与下方可视化机制矩阵。V-07 仍未通过；权限与替换竞争不在本项，归 V-08。

组 5 补丁/锚点/历史原语证据（2026-10-31）：`aline-buffer` 已用 134 例正逆补丁金样（含 `abcXYZ → aQQc!`）、11 例行索引金样和反例验证 after 逆坐标、合法空插入/无变化替换、锚点 affinity、900ms 输入组、保存点身份、重做分叉、裁剪与可撤销重载；回归入口为 `crates/aline-buffer/tests/`。这些仍是无头 buffer 原语，不含块打开/提交分组、IME 确认/取消、跨块命令、格式化差异确认、只读保护及下方可视化机制矩阵，不能据此判定 V-07 通过。

组 6 解析/映射/保护基础夹具证据（2026-10-31）：`aline-markdown` 已落地块/Gap 无缝覆盖金样（7 个夹具、57 个区间，含多空行、前导/尾随空行）、手写映射金样 6 例（实体、多字符实体、SoftBreak、硬换行、`<br>`、隐藏与合成片段混合；`tests/mapping.rs` 直接解析该 TSV 断言，不再是孤立证据文件），以及 ZWJ 家族 emoji、重复文字、连续空白、tab 与组合字符、UTF-16 局部投影的补充用例；回归入口为 `crates/aline-markdown/tests/mapping.rs` 及 `fixtures/`。保护范围覆盖 Front matter、脚注引用/定义、HTML 注释与处理指令、代码/围栏与有限 HTML 的隐藏子区间。复用拒绝已扩到块引用/列表容器内的引用定义与围栏行、配置代次变化与整源回退，差分比较加入映射来源/精确性、保护与样式范围（测试辅助；生产路径只比较块种类与块文本，`step-6.7`）。**未覆盖/待 V-07 冻结**：SoftBreak 当前投影为带来源的换行，该口径尚未冻结；有限 HTML 的空白折叠、块级标签合成换行、过滤集对 `title/textarea/xmp/noembed/noframes/plaintext` 的补全、非过滤集 void 标签（`input/link/meta/base`）展示口径、十进制/十六进制/多字符实体与“仅标签零可见文字”金样；真实 UI 命中、像素/候选窗/焦点、IME、系统字素与跨块命令；只读灰色展示仅引擎侧成立，未在 UI 验证。不能据此判定 AC-09 或 V-07 通过。

组 6 有限 HTML 修复证据（2026-10-31，F2-1）：上段列出的过滤集补全、空白折叠、块级合成换行与实体/零可见金样已落地并由 `tests/html.rs` 20 例覆盖（修复前 11 例），`tests/protection.rs` 6 例不变；回归入口为 `crates/aline-markdown/tests/html.rs`、`protection.rs`。**仍待冻结**：非过滤集 void 标签（`input/link/meta/base`）展示口径、空白折叠与块级换行口径本身、SoftBreak 口径；真实 UI 渲染与灰色只读呈现未验证。AC-09 与 V-07 仍未通过。

组 6 round-2 复核处置证据（2026-10-31）：有限 HTML 的**未支持非 void 标签**（如 `<foo>`）当前开标签字面可见、闭标签隐藏、内部空白不折叠（`<span>` 对照为两侧隐藏 + 折叠），已由 `tests/html.rs` 的 `unsupported_non_void_tags_record_the_v07_pending_asymmetry` 固定为现状记录（html 21 例），**该口径仍待 V-07 冻结、不表示已统一处理**。过滤 HTML 的扫描步数统计已与真实扫描器一致（过滤样本 5/5、3/3、3/3，未过滤 8/8、80/80），`max_mapping_points` 兼作投影片段数上限的语义已在 `budget.rs`/`parse.rs`/`projection.rs` 注明，预算出口表按实际出口更正（源码/重写字节与投影索引点返回错误，构树类维度整源回退）。跨行引用定义标签（`[foo\nbar]: /u`）现在整篇拒绝复用（当时 reuse 18 例；2026-11-02 导航收口后 `tests/reuse.rs` 为 30 例，新增用例见上文任务 6.7/6.10 记录）。回归入口为 `crates/aline-markdown/tests/html.rs`、`hostile.rs`、`reuse.rs`。**未覆盖不变**：真实 UI 命中/候选窗/焦点、IME、系统字素、跨块命令与灰色只读呈现；平台仅 macOS arm64。V-07 与 AC-09 仍未通过。

**复核 N-F3-5 限定（映射金样来源）**：“组 6 解析/映射/保护基础夹具证据”段落中“手写映射金样 6 例”的**来源不可证**——08:48 已存在修复前解析器、TSV 时间 08:52:17Z，只能证明该 TSV 先于消费者（09:31:07Z）与 round-1 复核（09:15:27Z）且未被改写，内容另经独立 Python 重算（6 例、0 问题）。

组 7 编辑计划/拒绝/最小命令证据（2026-10-31）：`aline-edit` 已落地纯 `plan_change`/`validate_change`，30 例手写金样（26 个 plan 用例 + 3 个 validate 拒绝 + 1 个 consume）先于实现建立，`tests/goldens.rs` 6 例直接解析该 TSV 断言，`tests/refusals.rs` 16 例（round-2 复核 N-F2-1 后由 15 例补至 16 例）在真实 `StringBuffer` 上比较文本/选区/revision/undo/redo/历史节点数，`tests/protection.rs` 4 例覆盖 front matter/`[toc]`/脚注引用的 Visual 拒绝与 Source 放行；命令面只有 `Replace` 与 `ToggleInlineFormat`（Strong/Strike）。回归入口为 `crates/aline-edit/tests/`。**已记录的勘误**：金样 `allow_source_footnote` 的目标选区写为 `7..7`，按统一字节规则应为 `8..8`（`4 + 4`），测试保留原值并引用 [step-7.1 报告（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-7/step-7.1/report.md) 第 3 节的书面解释，未回填金样。**已接受的语义限制**：隐藏语义比较跳过纯空白隐藏片段，且不比较片段边界与位置，不得据此声称“隐藏片段已完整保留”。**未覆盖**：块打开/提交分组与 900ms 真实计时、IME 确认/取消、离块与 Undo/Redo 交错、自动保存/恢复/导出交错、ED-01 的组合期等待/取消、列表/表格/任务框/代码语言/全文格式化命令、查找替换事务与不可见命中定位、MD-11 粘贴的独立用例（当前与跨区替换共用 `Replace` 路径）、HTML 右端越界与跨整个元素的格式命令、真实 UI/DocCore 消费者与灰色只读呈现、Windows/Linux 与其他 target。**V-07 仍未通过**，AC-04 也未通过。

组 8 输入与组合协议基础证据（2026-10-31）：无窗口 DocCore 已覆盖唯一草稿/最新快照、同块多个 900ms 输入组、提交/关块分离、待定组优先 Undo 与 Redo 选区、组合更新/替换/确认/取消、ED-01 显式发布等待/取消、自动保存组合期延后、独立恢复读取、草稿基线冲突及配置/request/关闭/模式变化后的旧结果拒收。宿主完整登记是资源集合齐备的前提，DocCore 和快照保守强持有登记引用，不自动发现或清理资源。最终局部 UI 原始日志为 109 passed（[实现及失败保留（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-8/followup-2026-10-31/report.md)、[回填与未测清单（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-8/step-8.5/report.md)）；这不替代 workspace 全量门禁。**仍未测/未实现**：真实键盘/焦点/候选窗/marked 绘制/平台 callback、真实双模式视图与选区映射、自动保存计时/目标队列、真实保存/恢复/导出及资源迁移、通用命令消费者与跨块/结构命令、Windows/Linux/Intel Mac。上方各组的未覆盖描述保留其时点；本段只补上述纯状态基础，V-07 与 AC-03/04/05/14 仍未通过。

**2026-11-06 M1 真实像素命中分项（5.2）：** 在 macOS arm64 真实窗口（debug、中文界面、960×672、scale 2）对内容/实体/字体夹具执行像素点击→映射→核心导航：Exact（普通文字与跨隐藏标记的可见文字）、Aligned（`&amp;`/`&lt;`，源码范围 22..27、28..32）、Synthetic（任务标记 `☐` 回指 `[ ]` 起点 508）均得到 `layout_text_matches_projection=true`、`open=false history=0 revision=0`；切 Source 后光标位于 `&amp;` 之前，与 `anchor=22` 一致。Hidden 片段不可由像素命中（`segment_for_drawn` 跳过零宽隐藏段），由无头用例覆盖，属设计行为；夹具 SHA-256 前后不变。该分项只覆盖只读命中/锚点与只读性，不表示 V-07 的编辑/格式化/IME/跨块命令或整项通过。证据见 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/hit/`。

产物：表格表头/整理的输入输出、跨结构选区命令边界、格式化风格和未知语法/HTML 保护样例；查找按源码、不可见命中定位、环回和退出焦点行为。

脚注、`[toc]`、Front matter 必须用灰色展示区，仅源码模式可编辑；基础展示/导航保留。补齐脚注编号/定义/回跳、目录大小写/重复标题锚点、Front matter 文首/BOM/闭合围栏样例，并验证跨块命令不会绕过只读保护。其他未知语法原文显示。

统计另立示例：中文、英文、标点、Markdown 标记、代码、空白、选区和组合字符；两模式口径一致，光标列不能用 UTF-8 字节计数冒充。

补充可视化机制回归矩阵（来源：[Splat §4～5](./splat-study.md#4-可视化编辑的实际机制)，**全部未执行**）。下表只细化已有契约；Splat 的测试源码不是本项目通过证据，未来若实验隐藏输入标记须单独立项。

| 验证问题 | 样本与方法 | 通过条件 |
|---|---|---|
| 阅读命中能否安全切为源码输入 | `**ab**` 末尾、链接显示文字/隐藏目标、`\*`、`&amp;`、多字符实体、空段落、中文/emoji；逐边界点击→开块→选区/输入→Undo | 约定锚点/affinity 稳定且带来源/精确性；输入按源码、不隐式转义；无非法 UTF-8/UTF-16 边界；不可见目标有源码入口 |
| 换行与结构边界是否混淆 | 段内 LF、两空格硬换行、反斜杠硬换行、HTML `<br>`、纯布局软折行，嵌套列表/引用/代码的 Enter/退格/Tab，跨块 Gap/atom 选区 | 文字/结构与来源各有金样；软折行不加源码 LF；删除/粘贴使用完整来源及保护规则；不自动带入富文本容器清理 |
| 字符与选区方向是否完整 | emoji 代理对、ZWJ、组合附加符、正反向拖选及跨块复制/删除；非法平台端点注入 | 用户字符不拆坏，anchor/head 方向保留；非法端点受检拒绝，拒绝前后文本/草稿/组合/选区/undo/redo 均不变 |
| 旧映射与输入能否竞争 | 暂停解析结果采纳，在源码变化后立即点击旧展示/输入，再乱序交付结果；组合更新/确认/取消与切块、切模式、菜单、发布等待交错 | 不用旧范围写入，不静默丢当前输入；保留草稿/组合并重新定位或进入冲突；TA-01/ED-01 分组与快照条件全部保持 |
| 表格纯导航是否引发格式化 | 不齐空格、转义管道、行内代码、引用/列表内表；已有格 Tab/Shift-Tab/上下移动，与末格追加行/显式整理分别记录 before/after | 纯导航 bytes/history 不变；结构/整理仅按显式命令的源片段补丁改变，一步撤销恢复原字节/选区；不从展示表序列化 |
| 局部优化是否漏全局语义 | 同文档前后修改引用定义、脚注、Front matter、未闭合围栏/HTML/列表；固定随机 seed，将候选结果与完整重算逐项比较，并记录回退 | 树/块/Gap/来源映射/保护/导航一致；复用位置更新；失败安全退全量，不能静默填末尾映射；确实命中优化的样本与全量回退样本分开报告 |

纯状态机金样先行；像素命中、候选窗、焦点和系统字素行为另在锁定 GPUI 与真实三平台环境验证，不能只靠标量/代理对单测宣称 IME 或导航完成。

时点：M0 解析夹具，M2 输入/选择/搜索边界，M3 命令和格式化；M4 前冻结统计口径。对应 EDIT-01～14、FIND-01～03、MD-11、FORMAT-01～02。

**接口基础记录（2026-10-31，任务 3.3）：** 八接口（decode/encode/parse/映射/plan/validate/apply/snapshot）的输入输出、错误类别、唯一写入口与相等证据来源已按已确认架构/引擎规范冻结；实际落地的包括 `TextSnapshot`/`StorageSnapshot`、受检坐标/身份、`CheckedBatch` 构造/校验、恒等 `Projection` 与 `InputIdentity`，decode/encode、完整 parse、edit 层 plan/validate、apply 与 `snapshot_current` 内容快照仍仅契约未验证。公开面追踪未发现映射 API 的 UI 生产消费者，当前仓库扫描也未发现映射外手算局部差或自建 UTF-16 表，但公开坐标访问器仍可被绕过；这不证明真实命中、IME、保存或本节任何用例通过。完整记录见[任务 3.3 记录（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-3/step-3.3/report.md)。本节上方“补充可视化机制回归矩阵”仍全部未执行。

**2026-11-02 M1 V-07 夹具分项：** `crates/aline-markdown/tests/fixtures/m1/README.md` 区分现状、独立预期和待冻结语义，覆盖 SoftBreak/HardBreak/布局软折行、HTML void/未知/交叉嵌套/过滤集、未闭合 Front matter、TOC 大小写与重复标题、脚注缺失/重复/大小写/含非 ASCII 字符标签。新增 `readonly_presentation` 实际消费五组展示预期并检验源码块+Gap 完整分区；导航回归先复现大小写标签目标丢失，再按 pulldown-cmark 0.13.6 的 ASCII-only `UniCase` 等价规则修复。HTML void 与未支持标签差异、SoftBreak 展示口径、真实 UI/像素命中仍未冻结或未执行。本分项不改变 V-07 未通过状态，也不宣称 AC-09 通过。

**2026-11-05 随机不变量补充（方法采纳，未执行）：**E-T20 的 fuzz 可作为 V-07 的补充证据：确定性 RNG 驱动 parse/buffer/edit/DocCore，每步检查不变量，周期保存往返或与全量权威路径差分，失败保留 seed、步号、操作日志与最小输入。fuzz 不替代本项的金样、真实 UI 命中、IME、跨块命令与格式化夹具；不能用测试辅助函数把已知语义丢失规范化后视为通过。

**2026-11-06 M1 只读命中分项（部分，真实像素未验证）：** 点击链已接 GPUI 文本布局局部绘制偏移 → `HitTarget`/`Projection::source_at`/`aligned` → 受检源码锚点，覆盖 Exact/Aligned/Hidden/Synthetic、实体两侧、重复文字/空白、中文/emoji/组合字符与软折行/Gap 的内联无头用例（`cargo test -p aline-ui --lib native_window::readonly_hit` 9 passed）；命中不开编辑块，debug 日志输出 `open`/`history`/`revision` 供核对。**未执行：** 真实像素点击→切 Source 核对锚点的平台序列（会话锁定阻塞）；不据此判定 V-07 或 AC-09 通过。

**2026-11-06 M1 内容展示分项（部分，真实窗口）：** Visual 视图对基础行内样式、嵌套列表/引用、五类 Alerts、三列对齐表格、任务框、有限 HTML、灰色 Front matter/脚注/目录、公式/Mermaid/图片原文回退的实际绘制已取得 macOS 真实窗口截图（`logs/validation/m1-single-window-readonly/2026-11-06-continue-06/content/`），无头内容测试 20 passed。**未执行：** 开块编辑与低对比标记、编辑/格式化命令、IME、跨块粘贴、逐类 Source 视图对照与真实点击定位；本分项不表示 V-07 或 AC-09 整项通过。

**2026-11-06 M1 只读导航夹具分项（部分，真实窗口）：** 目录重复标题、脚注引用/定义双向回跳、同一脚注多引用回跳、替换文档后旧导航与键盘激活已取得 macOS 真实窗口操作与截图：两个同名「重复标题」分别定位源码 17/4712；引用 41→定义 56、定义 74→引用 40、引用 49→定义 75、定义 93→引用 47；长文档引用 75→定义 8063、定义→首个引用 52；`navigation-nearby.md` 的 Tab 环包含两个灰色定义行，定义行 Enter 输出 `m1-navigate source=40 row=6 mode=Visual`；长→近→长替换后旧偏移不再生效，重新打开长文档后目录仍定位 4712；打开等待取消后位置/统计不变。所有命中日志 `history=0 revision=0`，状态栏字符/行数不变。空目录显示 `content.toc_empty`、悬空脚注引用无导航条目、重复标签保留全部目标由无头用例覆盖；挂起定位提示条只出现一帧，无法人工点击「取消定位」，取消语义由内联用例覆盖。**未执行：** 无标题文档空目录的真实截图与图片预览状态；本分项不表示 V-07 或 AC-09 整项通过。证据 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/navigation/`。

**2026-11-09 CJK 软折行语义覆盖（未通过）：** `m1-cjk-linebreak-core-fix` 的金样区分自动软折行与原文开头/显式换行豁免，覆盖中英混排、开闭括号、全角引号（待独立预期，只校验 UTF-8/簇）、ZWJ/旗帜/肤色、组合字符、拉丁连字、仅标点与超宽单簇；现状 oracle 复现行首/行尾禁则和簇内断点。无合法候选与超宽簇的“真实受限结果”契约尚未满足，接口提案待裁决；本轮未编译候选 GPUI、未做真实 shaping/像素命中/截断/IME。覆盖只说明缺陷可复现与规则/隔离 diff 形态，不改变 V-07 未通过状态。

**2026-11-09 W3 随机不变量闭环：** 四引擎 `engine-fuzz-invariants` 使用确定性 RNG/默认 16 seeds ×100 steps，并完成 64×500 扩量，13 项测试通过；每步层内不变量、source 精确解码/编码、buffer 拒绝不变性、edit 合法/预期拒绝及 no-op oracle、markdown 复用/样式/保护/可见映射差分均有落点。失败保留 seed/步号/操作日志/输入；四 crate 的真实字节损失注入各重复两次均预期失败，去注入后通过。独立审查发现的 edit 意外 Err/no-op 静默吞掉已修复；Hidden 全局绘制端点受相邻 Synthetic 锚点影响，不能当块局部双射，仍复验精确性/范围。审查 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/review/fuzz-final-review.md`、运行同轮 `w3/`，主 Agent 统一 workspace 29 项通过见 `workspace-final-02/summary.json`。随机测试不替代真实 IME、文件故障或 M2 保存恢复。

**2026-11-09 M1 原样本字节观测：** 同轮 `readonly-bytes-02/` 由真实原生窗口逐个打开 BOM、混合行尾、孤立 CR、无尾换行和纯空白五个原始样本，实际滚动输入、像素命中尝试、Visual→Source→Visual、关闭；导出各样本的 opened/source-before/source-after/stored/disk-after 独立字节，磁盘与精确源码均相等，revision 前后 0、无开块/草稿/组合、undo/redo/历史均 0。五个原样本均未完成主动跳转，不能以这些观测关闭 7.3；纯空白点击接受的是空白映射回调，不表述为命中可见字形。用户明确不接受配对夹具替代，原样本和七步标准保持。`observations.json` 的 `all_required_observed: false`、记录器 exit 1 为正确结果，不转换成标准 results.json 的通过记录。

**2026-11-11 样本形态裁决：** 用户裁决「修改原样本使其可跳转」——不引入配对的替代夹具，改为在每个原样本内补入 `# 标题` 与 `[toc]`，保留各自保真特征（BOM / 混合行尾 / 孤立 CR / 空白 / 无尾换行），使原样本自身可完成主动跳转。样本数量与保真维度不变；改动记于 `scripts/run_m1_readonly.py` 的 `BYTES_CASES`，形态回归见 `crates/aline-markdown/tests/readonly_bytes_samples.rs`。本条只记录样本形态裁决，7.3 的真实七步证据尚未取得，AC-01/AC-09 分项仍未回填。

**2026-11-11 M1 只读七步真实窗口分项（AC-01 字节 / AC-09 展示，回填分项）：** macOS arm64 真实原生窗口上，五个保真样本逐个走完打开→滚动→命中→主动跳转→两视图往返→关闭七步，由应用自身回调落盘逐步证据：每样本 `steps` 折叠后恰为 `open,scroll,hit,jump,visual_to_source,source_to_visual,close`，且 `disk_bytes_equal`、`source_bytes_equal` 均为 true、`has_draft=false`、`history_len=0`、`revision` 前后均为 0。标准入口 `python3 scripts/run_m1_readonly.py --suite readonly-bytes --results <evidence>/results.json --output-dir <evidence>/acceptance` exit 0、`passed: true`、`failures: []`；`cargo test -p aline-ui --test readonly_document` 9 项通过。**主动跳转为真实点击目录项**：截图中目录条目呈激活态；**命中为真实像素点击**（按截图暗像素聚类定位标题行中心），非程序化定位。证据 `logs/validation/m1-single-window-readonly/2026-11-11-73-real4/`（`driver/` 为逐动作命令、原始返回与截图及启动等待记录，`snapshots/case-001..005/` 为逐样本字节与状态）。**取证过程中修复的三个真实缺陷：**（一）打开面板会列出证据目录，无障碍树节点数超过工具遍历上限后「前往文件夹」面板被截断、路径输入框不可见，真实打开步骤必然失败——改为向当前焦点投递路径（`type-text`），并把驱动的动作留证移入独立子目录，使面板列举不再撑爆树；（二）点击窗口关闭按钮后窗口立即消失，工具回报 `window_not_found`，实为关闭成功，按失败计会误判一次正确收尾；（三）应用进程刚启动时虽已存在，却尚未登记进无障碍树，此刻投递只会得到 `app_not_found`，头几个样本因此在**打开文档之前**整批夭折——驱动改为投递第一个动作前先探测窗口是否可寻址，未可寻址则等待后重试，等待轮数留证于 `driver/startup-wait.json`（本轮 1 轮、0.289 秒）。**口径边界：** 本分项只回填 AC-01 的字节保真分项与 AC-09 的只读展示分项（真实窗口下打开、滚动、像素命中、目录主动跳转、两视图往返与关闭）；**保存/重开仍归 M2**，不因此关闭；V-07、AC-09、AC-16 整项与 M1 出口仍未通过。**2026-11-11 门禁收紧（独立评审发现的真实缺口）：** 评审实测出标准入口只读 `results.json` 这一**自述**文件、字段自洽即放行，从不回读原始证据——一份完全虚构的 `results.json`（run_id、pid、哈希全自编）能以 `passed: true`、`failures: []` 通过；另有驱动退出码不进标准入口、收据 `raw_sha256` 绑定空 `app.stdout`（哈希为公开常量 `e3b0c442…`）两处绑定失效。四项处置见[决定 §33](decisions.md#33-2026-11-11-只读字节门禁必须回读原始证据)：标准入口按 `evidence_root` 回读 `snapshots/case-*/` 重算原始字节哈希并重新折叠 `state.tsv` 的回调序列；驱动退出码经 `driver/driver-exit.json` 与收据交叉核对；`raw_sha256` 改绑每轮非空的 `driver.stdout` 并由门禁实际重算比对；文档事实更正（上一段称门禁 `final3` 的 33 项含驱动自测，实际 `final3` 不含，含该条目的是 34 项的 `final4`）。收紧后重新在真实窗口取证一轮（证据 `logs/validation/m1-single-window-readonly/2026-11-11-73-real5/`）：五个样本仍逐个走完七步，标准入口 exit 0、`passed: true`、`failures: []`；标准入口自测由 21 例增至 31 例，五处守卫各自以 mutation 验证「退回即失败」；收紧只判结构不判真伪，门禁证明的是「所声明的每一条都能在原始证据里找到对应」，证据本身可信度仍靠独立复核截图与操作日志。

**2026-11-09 CJK 断点修复复核轮（未通过）：** `logs/validation/m1-single-window-readonly/2026-11-09-cjk-02/cjk-linebreak/` 只复跑 1.1 / 2.1 的无依赖纯规则与合成度量金样，以及证据记录完整性正/负例；未做真实像素命中、新折行边界两侧点击或两视图往返，未编译或激活候选补丁（`未构建、未激活、未批准、未编译`）。接口裁决与精确来源准入待用户裁决，见该轮 `review/adjudication-questions.md`。原五个保真样本缺主动导航造成的 7.3 阻塞、M0 验收豁免与 W4/W5 的 M2 前置均保留；不改变 V-07 未整项通过状态，也不宣称 AC-09 通过。

**2026-11-09 裁决落定（不改变 V-07 未通过状态）：** 接口裁决已按用户授权的最小公开面方案落定、spec delta 未改；来源准入结论为暂不批准、暂不允许 3.2 编译（见[决定 §29](./decisions.md#29-2026-11-09-按授权落定的接口裁决与来源准入结论)）。真实 CoreText 簇索引形态与 `LineWrapper` 按字符迭代的残余风险仍开放，需 3.2 真实 run 证据；这直接关系 M2 的 IME / 组合字符，不得当已解决。本轮仍无真实像素命中、两视图往返或原 Aline 映射/命中回归，未构建/激活 patched GPUI，不改变 V-07 未整项通过状态，也不宣称 AC-09 通过。

**2026-11-09-cjk-04 隔离编译分项（不改变 V-07 未通过状态）：** 具体批准后仅在 logs 隔离副本编译 patched 核心，`cargo test -p aline-ui --lib` 111 passed / 1 ignored，确认统一像素命中链所依赖的原 Aline 映射/命中测试在该来源下仍通过；三入口回归 13/13（合成度量）。**未执行**真实像素点击、两视图往返、换宽/字体失效与源码锚点恢复，也未取得真实 CoreText 簇索引形态；typed 受限状态在最小公开面下外部不可读，需 5.1 真实 Window 证据。独立评审已完成（两份 `pass-with-findings`，高 finding 未清零）。见 `logs/validation/m1-single-window-readonly/2026-11-09-cjk-04/cjk-linebreak/3_2/` 与 `review/independent-review-request.md`；V-07 与 AC-09 仍不宣称通过。

## V-08 · 文件操作、临时图片与恢复事务

**M0 身份/快照设计准备（2026-10-31，组 8）：** 已有文档/工作版本/快照来源/配置/请求采纳身份、冻结当前输入与已登记资源、组合期显式等待/自动保存延后及独立恢复读取的无头基础。`SnapshotId` 不是版本或内容哈希；宿主须在引用生效前完整登记资源，M0 保守保留而不精确追踪/回收。source create-only 集成用例只证明宿主可保留实际 `Written` 及既有目标拒绝分类，即使派生输入过期/块关闭也不能丢弃已发生写盘事实；它未消费到 DocCore 保存点或真实文件协调器。**未执行**：现有文件安全替换及 metadata 保全、版本竞争/权限/断电、多资源事务、真实手动/自动保存、10 秒恢复与恢复图片可读、另存为迁移/清理、真实平台组合等待、Windows/Linux/Intel Mac。M2 前身份/事务方案及最小安全闭环仍为独立出口，V-08 未通过。证据见[8.5 回填记录（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-8/step-8.5/report.md)。

产物：文件树拖拽源/目标、目录移动、跨卷/外部拖放、冲突/取消/权限失败、回收站不可用、打开文件移动时的保存与关联行为。文档路径、图片预览和资源引用的同步需有实际样例；不能静默覆盖重名目标或破坏源文件。

按已确认 UI-05 验证“移动到…”键盘目标选择与拖动共用工作区内校验/影响提示，覆盖取消、无效目标、自身/后代、重名、权限失败及资源引用同步。按 UI-06 验证关闭工作区仅退出该窗口文件夹上下文，保留 Markdown/图片标签及输入/选区/历史/恢复材料，不触发逐 tab 询问，不影响其他窗口；待恢复根记为空，未随后使用新工作区时常规启动不恢复旧根。不借此重置侧栏布局字段，后续关闭标签仍按 FILE-08 保护。

未命名图片临时目录以文档身份隔离；首次保存/另存为按资源设置复制并更新引用。验证保存取消、复制部分失败、磁盘写入失败、恢复后未保存、路径变化、撤销仍引用临时图片、多次保存/另存为等情况。恢复记录和临时图片共同保留，清理遵循 PRD 状态表并验证不早删、不互删。

文件关联样例验证仅注册 Markdown 打开候选并提供系统选择指引，不出现应用内设默认按钮或自动更改 OS 默认。权限复制故障注入需区分明确 Unsupported/ENOTSUP 与其他权限错误；只有前者且临时文件创建/替换访问语义经平台证明安全时才继续，否则停止并保全原文（V-01/V-08）。异常恢复只逐文档恢复/丢弃，不出现全部丢弃动作。通用 UI 命令不新增 catch 隔离边界，验证依现有受检事务与拒绝零副作用契约。自动保存前版本检查与外部冲突、重复进程转交、逐 tab 保存询问均需集成用例；本地无修改自动重载，有冲突暂停自动保存并提示。按已确认 TA-02 验证自动重载与“载入磁盘”均可撤销：Undo/Redo 仅改内存，磁盘基线不回退，恢复旧内容正确判脏并提示、暂停自动保存且不覆盖磁盘；显式保存/另存为成功或重做回基线后恢复正常自动保存策略，重做回基线恢复未修改，后续保存仍检查外改版本。按 ED-01，手动保存/另存为在组合期进入可取消等待，平台确认/取消后再取当前快照；覆盖确认、磁盘版本与资源计划在实际执行时重新检查。覆盖“等待期间继续更新候选、取消请求、平台取消组合、路径/目标变化及关闭”的交错，确认不会提前写候选、丢输入或把取消报成保存成功。

时点：M2 前形成身份与保存设计；M2 出口验证单文档保存/自动保存、外改与可撤销重载、10 秒恢复及未命名临时图片的最小安全闭环，覆盖取消/失败与恢复图片可读；M4 完成完整资源迁移链路，M5 完成文件树与多窗口回归。最小闭环不能后置 M4/M5，完整矩阵也不能由 M2 最小用例代替。对应 FILE-02～09、SAFE-04～08、IMAGE-07～08、AC-05～07/10/15/18/20。

由 [Splat 文件链路差异](./splat-study.md#6-文件保存保真外改依赖与测试)补充反例（**未执行**）：外部文件在检查间隙被同尺寸改写、时间戳不变/不可读、刚完成自身保存后再外改，以及本地草稿/组合尚未写入 Buffer 时触发检查。使用受控文件服务/故障注入并辅以实际平台文件操作，记录内容基线、工作版本、检查及写盘顺序；通过条件是按当前工作内容判冲突、写前重验磁盘版本，不以 mtime/size 或通知缺失证明未变，不丢本地输入；重载依 TA-02 可撤销且不清旧历史，磁盘基线不随 Undo/Redo 回退。BOM、混合行尾多数/平票、无末尾换行另做字节对照，未修改仍返回原 bytes，不继承 Splat 的混合行尾保持策略。

**2026-11-05 保存/恢复分离核对点（方法采纳，未执行）：**验证 1 秒自动保存与 10 秒恢复快照是独立目标、独立状态：自动保存只写用户文档，恢复先入内存且不覆盖磁盘；恢复快照写应用管理的独立目标，不以文档旁 `.bak`/`.corrupt-*` 副本作为恢复手段。写入按同目录临时文件 → 写完 → 同步 → 平台受检替换 → 按平台同步目录/处理权限核对，写前检查磁盘基线；失败保留原文件、内存与恢复材料，恢复写失败单列诊断。原子替换/目录同步只按平台实测说明保证范围，不称为跨进程 CAS。M2 最小闭环仍须覆盖 SAFE-04/05/07/08 与本项既有故障矩阵。

## V-09 · 视觉尺寸、默认值与平台状态

**2026-11-03 M1 启动语言分项（历史，裁决前）：** `aline` 通过锁定 `sys-locale@0.3.2` 读取 macOS 首选语言列表首项，中文/英语选择双语内嵌 catalog；首项缺失或不支持时回退英语并在窗口显示说明。实际 macOS 首选项最初为 `zh-Hans-CN`；临时把系统 `AppleLanguages` 偏好改为 `en-US` 后，以当前用户默认启动参数运行原生 app，窗口标题为 “Aline”，空状态为 “Open a Markdown document to start reading.”，见 `logs/validation/m1-single-window-readonly/2026-11-03-continue-02/catalog-en/english-empty-window.png`。测试后恢复 `zh-Hans-CN`；`AppleLocale` 为 `zh_CN`，provider 依据首选语言列表而非 locale 字段。当前截图是在回退提示接线前取得；正常英语文案相同。catalog 正反例与启动语言映射/空列表回退测试已运行，当前新增 catalog keys 的专项复跑待记录。此分项不表示完整 V-09/AC-15 通过。按[决定 §24](./decisions.md#24-2026-11-03-验证取证只做中文)，后续该分项复跑只核对中文启动，英文观测不再作为必需步骤。

**2026-11-03 M1 主窗骨架分项（部分，非 V-09 整项通过）：** 按 UI §4.1 的原型对照实现工具栏行 44px、内容区 `flex_1`、状态栏行 28px 的三段主窗。内容列按 border-box 880（含左右各 56，文本宽 768），首末行垂直留白 40/100，窄窗 ≤900 取 28/30/80、≤600 取 20/24/60，正文 16px/行高 28（16×1.75）；这些仍是实验候选，未冻结。代码侧实际取得：`geometry.rs` 为唯一几何数值源（`rg` 仅一处定义），5 例几何边界、5 例统计口径、12 例只读布局、9 例只读文档测试通过，`cargo test -p aline-ui --lib` 61 passed/1 ignored；完整 workspace 门禁、`check_workspace.py` 与 engineering/lints/safety/public-api 检查全部通过，证据 `logs/validation/m1-single-window-readonly/2026-11-03-continue-04/`。工具栏只渲染品牌区与文档打开时的“编辑模式”组，状态栏只渲染字符数、行数、未就绪占位与模式标签，无文档态不显示统计。**未测边界：** 真实 macOS 窗口的三段顺序截图、模式按钮键盘焦点、统计 A→B 序列与未就绪/零值截图、双视图往返列宽对比、三档窄窗截图、中文界面裁切仍未取得；验证期间多数检查在会话锁定（`CGSSessionScreenIsLocked`）下进行，未尝试解锁或绕过。2026-11-03 会话解锁后仅补做 11.3：无文档态与打开 README 后的窗口品牌区截图、图标资源身份检查通过，证据 `logs/validation/m1-single-window-readonly/2026-11-03-continue-05/brand/`；该截图不替代其余 V-09 项。Windows 10/Intel Mac/Linux、显示缩放与完整主题/字体矩阵未测。本分项不表示 V-09、AC-15 或 AC-20 整项通过。

**2026-11-05 离屏视觉证据边界（方法采纳，未执行）：**若 W1 通过依赖准入与能力 spike，可用锁定 GPUI 的离屏窗口、模拟键鼠和截图补充 V-09；证据须与真实窗口截图分开命名、分目录，并记录 feature/平台/版本。离屏通过只说明离屏渲染路径，不证明真实窗口、IME、显示缩放、锁屏恢复或三平台行为；真实证据受阻时记录阻塞，不以离屏或原型截图替代。

**2026-11-06 M1 内容/导航/字体分项（部分，真实窗口）：** 只读内容行、表格单元格、五类 Alerts、灰色 Front matter/脚注/目录、公式/Mermaid/图片原文回退、目录与脚注主动导航、统一代码行号与常显行号、字体代次与失败反馈已接 GPUI 渲染；无头 `cargo test -p aline-ui --lib` 103 passed/1 ignored（内容 22、命中 9、布局 15、字体 5、阅读渲染 11；含 2026-11-06 复核、独立审查修复、脚注定义行键盘回跳与代码长行编号回归），内容测试在 6.1 复跑为 20 passed、脚注定义行回跳新增后 21 passed、代码长行编号回归后 22 passed。真实窗口已取得：五类内容夹具截图（Front matter/目录/行内样式/引用/列表/任务框/表格/代码/公式/Mermaid/图片/脚注回退）、目录两个同名「重复标题」分别跳转到源码偏移 17 与 4712（状态栏字符/行数不变）、脚注引用↔定义双向回跳与多引用回跳、替换文档后旧导航失效、完整 Tab 环（含两个灰色定义行）与定义行 Enter 回跳、打开等待取消后位置/统计不变；代码夹具另确认 `rust`/未知语言只读标识、行号含空行与两位数单行（修复行号栏宽度后）、长行不换行并在右缘截断、指针在代码块内时主纵向滚动不被困住，证据 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/content/`、`.../navigation/` 与 `.../code/`。**未执行：** 无标题文档空目录的真实截图、真实横向滚动交互（合成输入未使代码区水平位移；长行不换行/截断已观测，真实横滚留作残余边界）、中文回退/标点/emoji 观测、窄窗三档与双视图往返（会话锁定阻塞；双视图往返与窄窗三档后于 2026-11-06/07 补齐，见任务 13.2 与下段 13.3）；V-09 与 AC-15/20 仍只按分项记录，不整项通过。

**2026-11-07 M1 窄窗三档分项（真实窗口，非 V-09 整项通过）：** 在 960/900/600 × 672 三档窗口上打开 8085 B 长行夹具（与 `navigation-long.md` 逐字节相同，SHA-256 `7d687ddf…0352`），截图 `logs/validation/m1-single-window-readonly/2026-11-07-content-column/00～02-width*-long-line.png`，测量记录 `.../2026-11-06-continue-06/content-column/narrow-3widths.md`。正文左缘实测 90.5/32.5/20.5 pt，与 `content_metrics` 的 90/32/20 相差 0.5 pt（字形 bearing），档位内边距差 56/28/20 一致；600 pt 时长段落在「…来自导」后折行，第二行「航索引。」完整可见，首行右缘 553.5 pt < 正文右缘 568 pt，无横向溢出/裁切；900 与 960 为单行。1 MiB 夹具宽度调整中 `max_offset=1198024px` 不变、锚点 597947/597946/597898/597897 恢复（见 §5.3.3 证据）。回归 `cargo test -p aline-ui --lib --offline native_window::` → 69 passed/0 failed。**边界：** 仅 macOS arm64、scale 2、单一长行夹具与三档宽度；未覆盖显示缩放、主题包字体与跨平台；本分项不表示 V-09/AC-15/AC-16 整项通过。同次会话随后再次锁定（`CGSSessionScreenIsLocked=true`），7.2/15.2 的 14 项窗口基线矩阵未执行；停止前仅捕获无文档态 `logs/validation/m1-single-window-readonly/2026-11-06-continue-06/window-baseline/02-no-document.png`，锁定探测见同目录 `lock-state-2026-11-07.md`。

**2026-11-07 M1 主窗 V-09 十四项矩阵（真实窗口，非 V-09 整项通过）：** 会话解锁后按任务 7.2 执行 `python3 scripts/run_m1_readonly.py --suite window-baseline --output-dir logs/validation/m1-single-window-readonly/2026-11-06-continue-06/window-baseline`，exit 0（`passed: true`，14/14，`failures: []`；观测输入 `window-baseline/observations.json`，结果 `summary.json`/`summary.md`）。覆盖：无文档态（`03`）、5 MiB 加载帧（`12`，60 fps 抓帧第 12 帧「正在读取 Markdown 文档…」）、正常阅读（`05`，2857 字符/42 行）、非 UTF-8 读取失败（`04`/`24`）、打开取消（`11`，焦点归还打开按钮）、>4 MiB 回退源码（`13`，1778400 字符/10401 行与回退原因）、窄窗 900/600（`15`/`16`，实测正文左缘 32.5/20.5 pt，600 pt 折行后「航索引。」完整可见）、高 DPI scale 2 滚动与命中（`06` 与 `../hit/observations.md`）、长英文文件名（`17`，96 字符标题完整、状态栏 91 字符 7 行）、打开/双视图键盘（`07～10`，`m1-anchor restored source=636 row=4`）、大纲/脚注键盘（`18～22`，目录项 Enter `m1-navigate source=17 row=4`，定义行 Enter 回跳 `m1-navigate source=40 row=6`）、反馈键盘（`23`/`24`，错误条「文档不是有效的 UTF-8 文本。」、焦点环留在打开按钮）与中文界面（`catalog-chinese-2026-11-07.log`，`m1_window_catalog` 2 passed）。**边界：** 仅 macOS arm64、scale 2；AX 快照只暴露窗口与「打开 Markdown 文档…」按钮，提示条文本未作为 AX 静态文本暴露，不宣称辅助技术支持；Windows 10/Intel Mac/Linux 与完整设置/菜单/主题矩阵未测。本分项不表示 V-09、AC-15 或 AC-20 整项通过。

产物：主窗口、设置、菜单、对话框和图片预览 tab 的尺寸与状态基准；窗口最小尺寸、侧栏范围、字体/间距、键盘焦点与中文界面裁切检查（界面语言取证按[决定 §24](./decisions.md#24-2026-11-03-验证取证只做中文)只做中文；中英文混排、标点、emoji 与组合字符作为内容排版样本仍保留）。主题预览必须反映实际三模式；验证 outline/filled 两种图标风格的实际覆盖、描边参数与可辨认状态，并记录预览图与实际样式一致。主窗口结构与控件邻接按[UI 规范 §4.1 原型布局对照](./ui-spec.md#41-原型布局对照实现参照)实现，数值以原型取值为测量起点、再由本项实测冻结。数值和平台支持由样本实测确定。

按已确认 TA-03 增加常规启动单窗口恢复最后工作区根、侧栏显隐/宽度/文件或大纲视图的状态样例；不恢复 tabs，显式打开请求不因偏好恢复额外开窗，异常恢复提示独立验收。

按已确认 WC-01 验证 APP-06 降级摘要的中文界面呈现（英文文案由 catalog 自动校验覆盖，见[决定 §24](./decisions.md#24-2026-11-03-验证取证只做中文)）、键盘焦点可达性、按类别去重、会话/文档/版本归属、容量边界、关闭文档释放所属条目、关闭后旧任务晚到不得恢复条目、关闭会话释放集合，以及版本变化后旧结果失效且不回填；验证中不预设容量或布局数值。以上样例均未执行。

按已确认 UI-01～UI-06 增加主窗口内设置页及返回原编辑上下文、名称按钮应用主题/缩略图独立预览及各自焦点、已确认[菜单/工具栏/键位基线](./ui-spec.md#105-菜单与工具栏编排基线ui-04-已确认)的实际名称/触发/系统冲突（含 Windows AltGr/欧洲键盘输入不被 Ctrl+Alt 键位误吞）、“移动到…”键盘访问/取消/焦点归还、关闭工作区后标签保留与根偏好为空等样例。大纲验标题层级与主动跳转，不将正文滚动反向高亮/自动展开/揭示标题纳入 V1 门禁（UI-03）。尺寸、收纳与平台能力仍需实测，决定确认不等于本项通过。

按已确认 TH-01～TH-10 增加主题设置与目录状态样例：导入成功后仍保持当前应用主题，点击名称按钮才应用；缩略图/大图浏览不应用；普通同 ID 冲突可读且无覆盖升级/删除入口；手工替换或删除后刷新结果可见；主题失效时选择身份保留、当前有效默认渲染且有说明，修复后不自动应用，用户显式选择后才生效。加入 PNG 预览、outline/filled 样式可见性、有效与失败字体/透明状态样本。schema 结构/继承校验归 V-04，图标实际覆盖与视觉数值归本项；选择确认不等于通过。

按已确认 PL-01～PL-03 增加全窗口门控与配色/图片视口状态样例：Mermaid 独立图像操作不受 export 开关影响；关闭高亮切回当前内容主题普通代码表面，独立配置保留暂停、再启用恢复；图片预览再启用自动恢复仍打开 tab，分别检查适应模式重算及手动缩放/平移约束、文件变化/失败说明、关闭 tab 不复活及无常规重启 tabs 恢复。设置专属分区隐藏仍保配置，图片视口是 tab 状态；整个交错不改 Markdown 输入、选区或历史。

保留标签列表“全部关闭”下横线；V1 范围外入口隐藏，当前状态不可用置灰并说明。确认菜单/工具栏/快捷键名称、插件启停门控以及 PRD 默认值表一致。打字机、拼写检查和 PlantUML 无入口；自动换行和代码换行有真实行为。

时点：M1 主窗口基准，M4～M5 完整交互基准，M6 跨平台验收。对应 APP-01～06、第 5.13 节、第 6 节、AC-15/19～20。

**2026-11-09 CJK 断点修复复核轮（未通过）：** `logs/validation/m1-single-window-readonly/2026-11-09-cjk-02/cjk-linebreak/` 未产生新的六组合真实截图、窗口尺寸或视觉尺寸实测；W2 的 320/480/768 × Visual/Source 六组合仍是既有观测，不充当修复成绩，也不重测真实断点/行高。本轮候选本地来源**未构建、未激活、未批准、未编译**，接口裁决与精确来源准入待用户裁决（见该轮 `review/adjudication-questions.md`）。只 macOS arm64 为本轮范围；Windows 10 / Intel Mac / Linux、显示缩放与完整主题 / 字体矩阵仍未测，V-09 仍只按分项记录、不整项通过。

**2026-11-09 裁决落定（不改变 V-09 未通过状态）：** 受限状态的用户可见提示（显示形态 / `t("key")` 键与插值 / 呈现位置 / 与 WC-01 降级汇总的关系 / 编辑态与只读态差异）按面向 M2 / M5 的考虑**另立后续 change 待裁决**，本 change 不接线，见[决定 §29](./decisions.md#29-2026-11-09-按授权落定的接口裁决与来源准入结论)。本轮未产生新的六组合截图、窗口尺寸或视觉尺寸实测，未重测真实断点/行高，未构建/激活 patched GPUI；不冻结合成斜体角度或平滑数值，V-09 仍只按分项记录、不整项通过。

**2026-11-09-cjk-04 具体批准后的隔离编译（不改变 V-09 未通过状态）：** 用户具体批准只授权 logs 隔离副本的 3.2 编译，未授权 Release 六组合重测或真实窗口取证。本轮三入口回归按 `golden.md` 声明度量重放 320/480/768 宽度，但严格禁则断言只覆盖**受检纯 CJK 子集**（3 个纯中文文本 × 3 宽度 + 1 个禁则集为空样本），**不等于 `cases.tsv` 全量 22 用例禁则通过**；混排文本只做度量无关断言，且使用 `NoopTextSystem` / `ClusterTextSystem` 合成度量，不产生真实断点/行高/截图；第一次尝试用错度量（9.6px 统一前进量配 30px 宽）暴露的「禁则行尾 `《`」观察已如实保留在 `3_2/first-attempt.md`，未被写成通过。用户可见受限提示仍按裁决另立后续 change，本 change 不接线。V-09 仍只按分项记录、不整项通过。

## 结果记录

| 编号 | 当前状态 | 规范/实验/产物证据 | 受影响需求 | 结论及回写日期 |
|---|---|---|---|---|
| V-01 | M0 工程/目标登记已有分项；M1 macOS arm64 原生冒烟已按分项执行；Windows 10/Intel Mac/Linux 原生发布矩阵未完成 | [E1（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.2/report.md)、[E6/M1 交接（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.5/report.md) | FILE-02～03、第 7.1 节 | 未整项通过，2026-11-11 |
| V-02～V-05、V-09 | V-03/V-04/V-05/V-09 已按 M1 分项执行（基础夹具、字体分项、Release 360 样本矩阵、14 项主窗窗口矩阵）；完整产物/平台/性能/视觉实验仍未执行；ED-01 等纯协议不代替产物 | [未测与交接清单（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.5/report.md) | 见各条目 | 未整项通过，2026-11-11 |
| V-06～V-07 | 无头基础分项已执行；V-07 已补 2026-11-06 内容/导航/代码分项与 2026-11-11 五样本七步真实窗口分项；限额、真实输入/显示及完整语法命令未完成 | [E2/E3/E5（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.3/report.md)、[E4/E7（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.4/report.md) | SAFE-01～03、EDIT、MD、FORMAT 等，见各条目 | 未整项通过，2026-11-11 |
| V-08 | 身份/快照与 create-only 原语是设计准备；真实保存/恢复/资源事务未完成 | [M2 前独立出口（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-9/step-9.5/report.md#m2-前独立安全出口)、[8.5（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/validation/m0-engine-foundation/group-8/step-8.5/report.md) | FILE、SAFE、IMAGE 等，见本项 | 未整项通过，2026-10-31 |

完成标准：证据可检查、支持和拒绝边界明确、PRD/专门规范已同步、相关验收通过。范围决定不再重复询问；尚未确定的规范细节由方案和样本收口，不能仅通过归档动作标记验证完成。

**2026-11-09 W6/W7 交付分项：** 语义 token 原值/守卫通过，真实 Visual 字面值控制版与 token 版在相同构建条件、夹具、960×672 pt/2×中文窗口比较。控制版由同一快照反向展开 token 构建、不是历史二进制；初次正文/工具栏/状态栏区域像素差 0，原生标题和滚动条有差异，追加 End/Home 的抓图阅读位置不一致，不能据该对照宣称全窗零差。图、源清单、19 色核对及结果见 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w6-before/visual-comparison.json` 及同目录 manifest/method。本轮未覆盖 Source 视觉 A/B、所有语法配色或 M4 主题。W7 的 UI 直接文案/翻译字面首参守卫、正反例及各测量入口自测已接现有 workspace；变量/数组间接 key、宏与动态串仍需人工。真实初窗失败尚无平台收据，完整性能报告仍缺呈现/精确停顿，不因此关闭 W7 2.2/3.1。

**2026-11-09 统一门禁最终记录：** `workspace-final-03/summary.json` 共30项通过（新增W1离屏运行器自测）；包括 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 及真实依赖/lint/safety/public-api/catalog/token/输出路径和测量工具正反例。新 CJK 修复规划建档后 OpenSpec strict 16/16，记录在同轮 `final/openspec-strict.*`。独立复核已将 W2 2.1 退回未完成：六组合已证部分保留，回退/平滑未测不能以记录完整布尔值代替。仅 macOS arm64，不将目标图解析当跨平台实测。
