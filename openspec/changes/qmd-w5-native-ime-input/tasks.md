# Tasks

## 1. 前置和锁定平台能力

关联全部 requirements、CODE_STANDARDS §2～6、R-4/8、V-01/07。`<evidence>` 为 `logs/validation/m1-single-window-readonly/<run-id>`；当前建档不代表实现或平台通过。

- [ ] 1.1 核对 M1 7.1/7.3/8.2 出口及 M2 可编辑块/Source 投影消费者，在本 design 标明当前身份与写入口。验收：M1 出口有证据、M2 输入前置可审查，未满足前不启用 M1 原生写入。
- [ ] 1.2 对锁定 GPUI 0.3.9 取得 handler 查询/替换/marked/unmark、平台自然结束与候选矩形坐标的源码锚点并做最小 macOS 能力 spike；不引入未准入 feature/依赖或 unsafe。验收：实际 callback 序列与单位可检查，明确 Blur/unmark 是否产生平台结果，不能用模拟确认绕过；记录 `<evidence>/qmd-w5/platform-spike/`，能力不足保留具体阻塞。

## 2. 输入投影与无头机制

关联原生输入与受检坐标 requirements，S-5/7/12、R-3～5、RS-02。

- [ ] 2.1 先建独立 Unicode/旧身份金样，再补 M2 可编辑输入投影的 UTF-16↔源码/布局字节受检适配。验收拟新增 `cargo test -p aline-ui --test native_input_mapping` 及既有 markdown mapping/grapheme 测试，覆盖半代理对、半 UTF-8、ZWJ、组合符、CJK 标点、CRLF、隐藏/合成/跨块和旧投影零副作用；同步引擎 §7/10 实际 API 边界。
- [ ] 2.2 先建普通输入/候选更新替换确认取消、重入/重复/晚到回调的无头表驱动测试，再接 DocCore 统一输入。验收拟新增 `cargo test -p aline-ui --test native_input_protocol` 与 `cargo test -p aline-ui --test document_typing --test document_composition --test readonly_document`，确认独立一步、取消保前态、900ms 分组与只读拒绝完整状态不变。

## 3. 原生接线与组合生命周期

关联 marked/焦点、候选窗、发布等待 requirements，R-6/8/10/16、S-11/13/14。

- [ ] 3.1 在 UI 实现最小锁定 handler 和当前可编辑块/Source 消费，真实用户输入全部走受检入口；渲染不持第二份可提交全文/候选。验收拟新增 `cargo test -p aline-ui --test native_input_handler` 的假平台契约和真实 macOS 普通输入/删除/选区烟测，错误经双语 catalog，记录新公开项实际消费方。
- [ ] 3.2 接菜单借焦点、点击其他块、模式切换、窗口失活、Undo/Redo 的组合门控与 marked 清理；等待平台自然结果保留原上下文。验收拟新增 `cargo test -p aline-ui --test native_input_lifecycle`，重复回调、关闭晚到、操作取消和旧目标拒绝无丢字/重复提交；本 design 固化实际时序，不能凭 Blur/Escape 推断结果。
- [ ] 3.3 接当前布局候选矩形与统一 Reveal，折行/滚动/DPI/字体代次使旧几何失效，不另设列表滚动所有者。验收拟新增 `cargo test -p aline-ui --test native_input_geometry` 的独立窗口/布局坐标预期和真实 macOS 长文虚拟列表边缘操作，候选窗跟随插入点；多屏缺设备保留未测。
- [ ] 3.4 将 ED-01 等待/取消与 W4 保存/另存为消费者联通，自动保存延后、恢复独立读取。验收拟新增 `cargo test -p aline-ui --test native_input_publication`，等待期间继续候选、取消请求、自然确认/取消、路径变化/关闭；W4 未接通时真实保存交错不得勾选；反馈中文且不抢候选焦点。

## 4. 真实 IME 验证与交付

- [ ] 4.1 执行真实 macOS 中文输入法矩阵：候选更新/替换/确认/取消、选中文字组合、菜单借焦点/点击其他块/切模式/失活、Undo/Redo、滚动折行/DPI 候选窗与保存等待取消。验收：输入法/系统/GPUI/显示身份、操作序列、真实截图/事件结果、内存源码与历史/草稿核查齐全；不记录敏感候选正文，模拟/离屏不代替真实证据；回填 V-01/07/08。
- [ ] 4.2 依 V-01 顺序补 Windows/Linux、Intel Mac 与多屏/缩放实际证据；无法取得的列明确平台限制，不由 macOS 或 all-targets 推导通过。验收：每个任务场景有适用平台成功证据或明确未测阻塞，V-01/07/架构 §13 状态一致，正式三平台边界保留。
- [ ] 4.3 安排输入状态/公共坐标/API 与生命周期独立审查，清理替代 handler/无消费者导出/fake 并 CodeGraph sync/status。验收：重要 finding 修复且主 Agent 复核，MD-11 所有已开放写入口/跨块/替换继续受保护；审查原文只留 `<evidence>/qmd-w5/review/`。
- [ ] 4.4 执行 workspace fmt/Clippy/tests 与 engineering/lints/safety/public-api 守卫，新增守卫包含正例和最小违规反例并核实八成员 lint 继承。验收命令：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全通过；原始输出经 evidence_paths 归一化，实际平台/feature 边界明确。
- [ ] 4.5 回填引擎 §10/UI §6/13/架构 §5/13 的实际接线与 V-01/07/08 证据状态，执行 `openspec validate qmd-w5-native-ime-input --strict --no-interactive`、全量严格校验、`git diff --check` 和引用/绝对路径检查。验收：真实证据缺失仍未完成，M0 豁免不被倒填，不提交/推送/归档。
