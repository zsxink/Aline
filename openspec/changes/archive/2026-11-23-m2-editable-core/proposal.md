# Proposal

## Why

M1 只读阶段已按决定 §43 在 macOS arm64 范围收口，M2 入口由此解锁；当前仍没有可供日常使用的编辑闭环。M2 需要把 M0 的无头文档/补丁/历史原语和 M1 原生阅读界面连成可编辑单文档，并完成安全输入、搜索、保存与恢复，验收仍以 PRD、V-07/V-08 和架构 §13 为准，不能由任务完成替代平台证据。

## What Changes

- 以单文档可编辑闭环为阶段目标：可视化单块草稿与源码编辑共用源码、映射、历史和当前快照；提交只产生受检区间补丁，拒绝不改变状态。
- 覆盖基础剪切/复制/粘贴、撤销/重做、双模式位置恢复和源码查找替换；HTML、脚注/目录/Front matter 等既有保护边界继续生效。
- 将 V-08 身份/事务方案审查列为首个实现门槛。在通过前不接入保存/恢复写路径，也不解锁现有 W4/W5 的生产接线；之后分别由 `qmd-w4-persistence-recovery` 与 `qmd-w5-native-ime-input` 承载细化实现和证据，本 change 负责依赖顺序与 M2 阶段收口。
- M2 出口逐项核对 PRD 对应 AC、V-07/V-08 和三平台要求；明确记录 macOS、Windows、Linux、Intel Mac 的实际覆盖与未测缺口。

### Non-Goals

不纳入 M3 格式化/结构命令/表格/HTML 编辑，不纳入 M4 插件与完整资源迁移，不纳入 M5 标签、多窗口、工作区和文件树；不改变 PRD 已确认行为、坐标/保真铁律、依赖准入或平台保证。阶段伞变更不复制 W4/W5 的详细设计，也不预先判定 V 项通过。

## Capabilities

### New Capabilities

- `ui/editable-document`：M2 单文档基础编辑、双模式、历史、查找替换及保护/拒绝行为。

### Modified Capabilities

- 无。现有 `engine/document-core`、`engine/edit`、`engine/source` 的契约继续作为实现边界；保存/恢复和原生 IME 的细化 delta 由各自 W4/W5 change 维护。

## Impact

预期影响 `aline-buffer`、`aline-markdown`、`aline-edit`、`aline-ui`，以及必要的 `aline-source` 写入消费与 app/UI 原生输入接线；现有八 crate 依赖方向不变，不新增 crate、运行时依赖或 unsafe。适用 `openspec/CODE_STANDARDS.md` §2～6、架构 R-3～8/10/15～17、Rust S-5/7/10～14 与 RS-02；每个跨模块坐标转换经过映射，UI/文件副作用保留唯一所有者。

阶段范围依据：PRD §5.2～5.5、§8 AC-02～08/10；架构 §5.2～5.4、§6、§13；开发流程 §1～3；决定 §43；验证计划 V-07/V-08。
