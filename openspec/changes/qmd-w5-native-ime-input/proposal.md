# Proposal

## Why

QMD-01 W5 要将已确认输入组/组合协议接到真实原生输入，避免候选替换、焦点和投影坐标导致丢字。当前 DocCore 有无头组合与 ED-01 等待基础，M1 原生窗口仍只读，没有真实输入适配或候选窗证据（PRD EDIT-04～06、AC-03～05；引擎 §8/10、E-T15；V-01/07/08）。

用户 2026-11-08 已要求 W1～W7 分别立项实施，本 change 独立承载 W5；阶段仍为 M2，依赖 M1 出口与 M2 可编辑投影，不因 QMD 研究建议跳过安全前置（开发流程 §1/3.1、架构 §13、决定 §25/26）。

## What Changes

- 基于锁定 GPUI 接线单文档唯一打开块及 Source 输入，复用 DocCore 900ms 普通输入组和独立 IME 确认历史；拒绝、取消和旧回调不丢当前输入。
- 所有 UTF-16 平台端点、UTF-8 glyph 索引和字素操作经过声明投影的受检映射；候选窗按元素/文本/滚动/窗口坐标计算。
- 焦点借还、点击其他块、切模式与窗口失活显式协调 marked 生命周期，先等待平台结果并保留原上下文，不凭 Blur/Escape 猜确认或取消。
- 建无头协议和真实 macOS 中文输入法矩阵；Windows/Linux/Intel Mac 未测项单列，模拟事件不得替代平台证据。

### Non-Goals

不改变 TA-01、ED-01、MD-11，不新增跨块组合语义或第二份可编辑全文/历史；不在 M1 提前接编辑，不实现 M3 命令全集或 M4 导出消费者。保存执行属于独立 W4；W5 负责组合期发布门控及其与 W4 的消费契约。不升级 GPUI、不增加未准入依赖或自有 unsafe。

## Capabilities

### New Capabilities

- `ui/native-input`：原生文本/组合输入、平台坐标、marked 与候选窗、焦点和组合期发布等待。

### Modified Capabilities

无。现有 `engine/document-core` 的唯一草稿、分组与发布等待契约保持；本 change 增加实际 UI 消费，不为重复已有需求而改主规格。

## Impact

影响 `aline-ui/document` 的实际消费者、拟新增输入适配/可编辑块、锁定 GPUI handler 与双语反馈；若发现映射公共 API 缺口，先按现有 `aline-markdown` 受检类型扩展，不能在调用点手算偏移。适用 CODE_STANDARDS §2～6、R-3～6/8/10/16/17、S-5/7/10～14；引擎不持 GPUI 或任务，文档核心继续唯一拥有输入状态。

### Acceptance Evidence

真实中文输入法候选更新/替换/确认/取消、选中文字组合、借焦点/跨块/切模式、滚动折行和 DPI 候选窗定位、Undo/Redo、显式保存等待取消及自然结束后快照。无头独立金样覆盖非法/旧投影、ZWJ/组合符/代理对/CRLF。原始证据写 `logs/validation/m1-single-window-readonly/<run-id>/qmd-w5/`，经路径归一化，摘要回填 V-01/07/08；仅取中文界面，双语 catalog 自动校验保留。

### Risks

锁定 GPUI 平台 callback、自然结束时序、菜单借焦点及多屏/DPI 仍需实测。研究 W5 “事件一律 unmark”不得替代 QMD-01.4/ED-01 的等待和保全契约；平台能力不符时先保留上下文并明确拒绝/等待，任何新行为冲突先列明裁决。M1 和可编辑消费者未就绪时只做准备，不勾选真实平台任务。
