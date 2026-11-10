# Proposal

## Why

W2 在 macOS arm64 Release 的 320/480/768 正文逻辑宽 × Visual/Source 六组合中真实复现软折行后的闭标点行首；短夹具 `scripts/fixtures/m1-cjk-punctuation-minimal.md` 也复现。2026-11-09 用户选择在 M1 内继续准备最小修复：优先评估仅 gpui-pre 核心断点补丁，中文斜体先取得实际 fallback run 证据；这不是具体 vendor 补丁或新来源的准入批准。

## What Changes

- 只修中文自动换行的禁行首/禁行尾候选点，保留原始文字、StyledText/TextLayout 及统一像素命中链；不往源码或绘制文字插入换行、零宽字符或 NBSP。
- 先验证锁定 gpui-pre 0.3.9 核心的最小补丁候选，覆盖普通 shaped-line wrapping、LineWrapper 的普通与 wrapped-truncation 路径，形成可审阅 diff/来源/许可证/精确依赖图后再申请准入；不预设 vendor 三个包。
- 提供独立合法断点金样及无合法断点/超宽字素的失败证据，保留已有预算、原文可达和零写入约束，不以新硬断或裁切冒充修复。
- 中文斜体仅补 requested Font、实际 shaped run/advance 的受限诊断；Source 明确请求 Menlo，按原样显示 Markdown 标记。合成斜体、平滑改变和字体替换不在本变更中实施。

## Capabilities

### New Capabilities

- `ui/cjk-line-layout`：只读视图的中文软折行合法性、字形边界与同一文本/命中几何约束。现行主规格没有 UI capability；M1 的 `ui/readonly-document` 仍在独立进行中变更，本 capability 不复制其打开/导航/生命周期要求。

### Modified Capabilities

无。工程治理主规格的唯一真源、受检坐标、依赖准入和独立证据要求保持不变。

## Impact

目标阶段 M1；关联 PRD EDIT-01/04、MD-01、AC-01/09 展示分项、V-04/V-07/V-09，UI §3.2/§6、引擎 §7、架构规范 R-4/R-9/R-10/R-11/R-17、Rust S-5/S-10/S-12/S-14 及 CODE_STANDARDS §2～6。拟影响锁定 GPUI 核心的 text_system/line_layout.rs、line_wrapper.rs 及必要私有断点 helper；Aline 侧仅最小消费/验证和依赖来源登记，不新增引擎依赖、crate、线程、全局状态或解析器。

范围外：编辑/IME/保存（M2）、主题包与完整字体机制（M4）、合成斜体、整元素 skew、整篇字体替换、默认接受已知限制、GPUI 升级及生产 test-support 激活。本轮平台只要求 macOS arm64；保留 V1 三平台范围与其他平台未测状态。

风险为极窄/仅标点/超宽字素时无合法候选，以及与 shaping cluster、截断和缓存的交互。不能把未确认的降级/溢出策略写成批准事实；具体补丁必须先通过独立金样、完整图审计和独立评审。验收需重测六组合、断点两侧真实像素命中/源码往返、原 bytes/内存源码/revision/草稿/历史不变及完整工程门禁；旧 W2 观测只证明缺陷存在，不是修复成绩。

## 本轮裁决（2026-11-09-cjk-03，按用户授权的建议）

用户授权按「面向以后功能迭代的最优方案」落定两项裁决。完整记录见
`logs/validation/m1-single-window-readonly/2026-11-09-cjk-03/cjk-linebreak/decision/recommended-ruling.md`。

- 接口：受限回退选 A1 + A4 受检组合（整簇移下一行 / 无合法候选不硬断报成功 /
  单簇过宽报告受限），以 `#[non_exhaustive] WrapLimit` 承载可检查的受限结果；
  **最小公开面**只新增 `WrapLimit` 与 `TextLayout::wrap_limit() -> Option<WrapLimit>`，
  拒绝 `Boundary.limit` 与公开三元组。不选溢出、不选保留硬断（与 spec delta MUST NOT 冲突）。
  用户可见提示（B6）本 change 不接线，列为后续 change 待裁决项；这不改变本提案的
  目标 / 非目标与产品范围。
- 来源准入：结论为**暂不批准**任何一代候选身份、**暂不允许 3.2 编译**；原因是三代
  diff / 文件哈希漂移、测试目标引入 `test-support` 依赖、两处未登记隔离编译尝试，
  按「冲突列入待确认并暂停依赖该决定的实现」处理。收敛后仍需独立评审与具体批准。
- 规范：spec delta **未改**；既有 requirement / Scenario 已要求「可检查的受限结果并
  保留原文」，无需放宽。本轮未改生产清单/锁/策略资产，未构建/激活 patched GPUI，
  未启用 test-support，未勾任何需具体批准的任务（仍 2/12），未提交/推送/归档/改 HEAD。
- 保留：M0 验收豁免、M1 未过出口、原五样本主动导航缺失造成的 7.3 阻塞、
  W4/W5 的 M2 前置、V1 三平台范围、只 macOS arm64 实测。
