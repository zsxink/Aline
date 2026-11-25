# Proposal

## Why

M2 已按决定 §46 完成本机 macOS arm64 最小出口。M3 补齐日常 Markdown 编辑命令、核心表格、限定 HTML 原文编辑和手动全文格式化，对应 PRD §5.4、§5.6、FORMAT-01～02 与架构 §13。V-07 的命令、表头、整理及格式化金样先于实现冻结。

## What Changes

- 接入行内与段落格式命令、列表续行和缩进、任务框切换。
- 提供表格逐格编辑、结构命令、Markdown 复制及以目标单元格为准的右键操作。悬停增强归 M4。
- 支持 HTML 块原文编辑，守住 HTML 格式边界和 MD-11 灰区。
- 在编辑菜单提供格式化差异预览、确认、取消和一步撤销。
- 阶段出口取证仅在本机 macOS arm64，原生界面使用中文，记录实际缩放。Windows、Linux、macOS Intel 与本机未覆盖环境留待后续。正式 V1 三平台同发适用。

## Capabilities

### New Capabilities

### Modified Capabilities

- `engine/edit`：结构编辑、语义复读及格式化规划。
- `ui/editable-document`：命令入口、原文编辑、表格交互及差异确认。

## Impact

涉及 aline-markdown 的源范围、aline-edit 的计划与复读、aline-ui 的 DocCore 和原生入口、aline-app-core 的双语文案，以及应用菜单接线。按 CODE_STANDARDS §2～6 和工程治理规格核查唯一所有者、受检坐标、事务和证据。主要风险是重写不可见字节、陈旧目标或预览误提交、输入组与命令历史合并。以独立源码金样及本机真实窗口证据核查 AC-04、AC-08、AC-09、AC-17 对应分项。
