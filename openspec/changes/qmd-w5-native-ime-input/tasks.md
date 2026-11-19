# Tasks

## 1. 前置和锁定平台能力

关联全部 requirements、CODE_STANDARDS §2～6、R-4/8、V-01/07。`<evidence>` 为 `logs/validation/m1-single-window-readonly/<run-id>`；当前建档不代表实现或平台通过。

- [x] 1.1 核对 M1 7.1/7.3/8.2 出口及 M2 可编辑块/Source 投影消费者，在本 design 标明当前身份与写入口。验收：M1 出口有证据、M2 输入前置可审查，未满足前不启用 M1 原生写入。
- [x] 1.2 对锁定 GPUI 0.3.9 取得 handler 查询/替换/marked/unmark、平台自然结束与候选矩形坐标的源码锚点并做最小 macOS 能力 spike；不引入未准入 feature/依赖或 unsafe。验收：实际 callback 序列与单位可检查，明确 Blur/unmark 是否产生平台结果，不能用模拟确认绕过；记录 `logs/validation/qmd-w5/platform-spike/`，能力不足保留具体阻塞。2026-11-16 macOS 27.2.1 arm64 / 微信输入法探针摘要和事件见 `logs/validation/qmd-w5/platform-spike/native-ime-elgr1j_k/summary.md`；观察到候选确认/取消及点击路径的 blur 前替换，未观察到 `unmark_text`，窗口失活和候选窗实显仍未测。

## 2. 输入投影与无头机制

关联原生输入与受检坐标 requirements，S-5/7/12、R-3～5、RS-02。

- [x] 2.1 先建独立 Unicode/旧身份金样，再补 M2 可编辑输入投影的 UTF-16↔源码/布局字节受检适配。验收：UTF-16、源码和绘制范围均由绑定输入 identity/projection 的映射层转换；独立金样覆盖半代理对、半 UTF-8、ZWJ、组合符、CJK 标点、CRLF、Hidden/Aligned/Synthetic 阅读投影拒绝、跨块范围拒绝和旧 identity/projection 无副作用；同步引擎 §7/10 实际 API 边界。
  - `crates/aline-ui/src/document/native_input_coordinates.rs` 覆盖输入投影转换、越界与跨块拒绝、阅读态 Hidden/Aligned/Synthetic 投影拒绝，以及旧文档/请求/配置/模式/工作版本和旧投影拒绝；失败检查前后比较文档版本、选区、草稿、撤销/重做深度及缓冲区文本。`crates/aline-markdown/tests/grapheme.rs` 提供手写 Unicode17 边界金样；新增局部 identity 源码范围用完整快照上下文判定字素，并拒绝越出 Exact 范围。验证：`cargo test -p aline-ui --lib native_input_coordinates --locked --offline`、`cargo test -p aline-markdown --test grapheme --locked --offline`、`cargo fmt --all --check` 均通过；OpenSpec strict、`git diff --check` 和 CodeGraph sync/status 通过。引擎 §7/10 已同步该 API 边界。真实 GPUI/IME 回调和平台字符行为仍属于后续任务，不由本项无头测试推定通过。
- [x] 2.2 先建普通输入/候选更新替换确认取消、重入/重复/晚到回调的无头表驱动测试，再接 DocCore 统一输入。验收：`cargo test -p aline-ui --test native_input_protocol` 与 `cargo test -p aline-ui --test document_typing --test document_composition --test readonly_document`，确认独立一步、取消保前态、900ms 分组与只读拒绝完整状态不变。新增 `native_input_protocol` 5 项协议测试覆盖候选范围、重复确认/取消、发布等待重入、取消等待后的迟到回调与自然取消取快照；该 target 不证明 GPUI/macOS callback 行为。

## 3. 原生接线与组合生命周期

关联 marked/焦点、候选窗、发布等待 requirements，R-6/8/10/16、S-11/13/14。

- [ ] 3.1 在 UI 实现最小锁定 handler 和当前可编辑块/Source 消费，真实用户输入全部走受检入口；渲染不持第二份可提交全文/候选。验收拟新增 `cargo test -p aline-ui --test native_input_handler` 的假平台契约和真实 macOS 普通输入/删除/选区烟测，错误经双语 catalog，记录新公开项实际消费方。
  - 已接 GPUI handler、普通输入/删除/方向键/选区与 DocCore 受检入口；单测覆盖旧行回调拒绝、同文 IME 确认后选区一致、Source 插入换行后续输入、候选离开当前 reader 行时自然结束，以及替换草稿删空仍提交删除。生产文档会话默认仍只读；无真实窗口普通输入/删除/选择证据。异步候选准备/发布新依赖 M2 design §7，仍在实现前置，本项未完成。
- [ ] 3.2 接菜单借焦点、点击其他块、模式切换、窗口失活、Undo/Redo 的组合门控与 marked 清理；等待平台自然结果保留原上下文。验收拟新增 `cargo test -p aline-ui --test native_input_lifecycle`，重复回调、关闭晚到、操作取消和旧目标拒绝无丢字/重复提交；本 design 固化实际时序，不能凭 Blur/Escape 推断结果。
  - 无头用例验证 Escape 不伪造结束组合、普通待定输入 Escape 后封口并保留已输入内容；普通组跨块重定向已有会话回归。菜单焦点借还、窗口失活、IME 组合跨虚拟滚动、取消/迟到回调的真实生命周期仍未测。
  - 2026-11-19 无头补强：组合期点击非输入阅读行改由行容器无条件反馈 `document.composition_pending`——组合推进版本后解析必然不 current，`HitTarget` 创建失败、行级命中回调不挂载，原 `open_visual_at` 分支不可达；输入行不拦截，`accept_reading_hit` 前置守卫只兜底旧帧竞态。新增 `cargo test -p aline-ui --test native_input_lifecycle`（4 项零副作用拒绝：模式切换 `Deferred`、Undo/Redo、组合期 `select`、取消后迟到确认/取消；另 1 项为组合提交后成功切换模式的路径，不是拒绝用例）。组合期同/跨块 `open_visual_at` 的零副作用断言在 `readonly_session/refresh.rs` 的文档层 API 契约测试中，不证明窗口点击路径。行容器与 API 契约测试均不能证明真实窗口反馈；菜单借焦、窗口失活与真实 IME 仍未测，未勾选。
- [ ] 3.3 接当前布局候选矩形与统一 Reveal，折行/滚动/DPI/字体代次使旧几何失效，不另设列表滚动所有者。验收拟新增 `cargo test -p aline-ui --test native_input_geometry` 的独立窗口/布局坐标预期和真实 macOS 长文虚拟列表边缘操作，候选窗跟随插入点；多屏缺设备保留未测。
- [ ] 3.4 将 ED-01 等待/取消与 W4 保存/另存为消费者联通，自动保存延后、恢复独立读取。验收拟新增 `cargo test -p aline-ui --test native_input_publication`，等待期间继续候选、取消请求、自然确认/取消、路径变化/关闭；W4 未接通时真实保存交错不得勾选；反馈中文且不抢候选焦点。
  - 2026-11-19 无头接线：组合自然结束（空候选 Mark 取消 / 其余确认）经 `composition_end` 判定后在 `settle_after_input` 结算 `SaveHost::Waiting`，继续原保存或首次关联；关联等待转 `Running`，迁移复制期取消与同句柄重复结算保持既有语义。纯函数与协调器级测试覆盖最新快照、确认/取消写回、组合期拒绝、复制期取消与迟到完成。`settle_after_input` 的窗口调用、真实 IME 自然确认/取消与真实保存交错仍未测，`native_input_publication` 目标未建，未勾选。

## 4. 真实 IME 验证与交付

- [ ] 4.1 执行真实 macOS 中文输入法矩阵：候选更新/替换/确认/取消、选中文字组合、菜单借焦点/点击其他块/切模式/失活、Undo/Redo、滚动折行/DPI 候选窗与保存等待取消。验收：输入法/系统/GPUI/显示身份、操作序列、真实截图/事件结果、内存源码与历史/草稿核查齐全；不记录敏感候选正文，模拟/离屏不代替真实证据；回填 V-01/07/08。
- [ ] 4.2 依 V-01 顺序补 Windows/Linux、Intel Mac 与多屏/缩放实际证据；无法取得的列明确平台限制，不由 macOS 或 all-targets 推导通过。验收：每个任务场景有适用平台成功证据或明确未测阻塞，V-01/07/架构 §13 状态一致，正式三平台边界保留。
- [ ] 4.3 安排输入状态/公共坐标/API 与生命周期独立审查，清理替代 handler/无消费者导出/fake 并 CodeGraph sync/status。验收：重要 finding 修复且主 Agent 复核，MD-11 所有已开放写入口/跨块/替换继续受保护；审查原文只留 `<evidence>/qmd-w5/review/`。
- [ ] 4.4 执行 workspace fmt/Clippy/tests 与 engineering/lints/safety/public-api 守卫，新增守卫包含正例和最小违规反例并核实八成员 lint 继承。验收命令：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全通过；原始输出经 evidence_paths 归一化，实际平台/feature 边界明确。
- [ ] 4.5 回填引擎 §10/UI §6/13/架构 §5/13 的实际接线与 V-01/07/08 证据状态，执行 `openspec validate qmd-w5-native-ime-input --strict --no-interactive`、全量严格校验、`git diff --check` 和引用/绝对路径检查。验收：真实证据缺失仍未完成，M0 豁免不被倒填，不提交/推送/归档。
  - 2026-11-17 收尾记录在 `logs/validation/m2-editable-core/2026-11-17-wrap-up.md`。全 workspace 工程门禁通过不表示真实 IME、候选定位、三平台或 M2 阶段出口通过。
