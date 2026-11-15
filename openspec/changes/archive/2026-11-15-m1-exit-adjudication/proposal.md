# Proposal

## Why

M1「单文档只读」的实现任务已于 2026-11-13 按 2026-11-03 apply 授权完成并归档，但阶段出口一直没有单独裁决：`docs/v1-architecture.md` §13、`docs/development-process.md` §1 与 `docs/v1-validation-plan.md` 仍写「M1 出口未整项通过」，而 M1 出口行要求的 macOS 首轮冒烟、AC-01/AC-09 阅读/字节分项、1 MiB 真实滚动、Win10/Intel 目标保留均已有可检查证据。状态长期悬空会让 M2 无法按 `docs/development-process.md` §1 的「M1 通过」交接，也会让 W4/W5 永远停在「等待 M1 出口」的模糊前置。

本 change 只做阶段出口核对、裁决与文档回填，不新增产品行为、代码、依赖或平台证据。

## What Changes

- 以 `docs/v1-architecture.md` §13 M1 出口行为唯一判据，逐项对照 2026-11-11 的 13 行 evidence 核对与本轮复读的机器可读产物，作出「M1 出口在 macOS arm64 范围收口」的裁决。
- 明确 V-01/V-04/V-07/V-09 的整项状态不变：它们不是 M1 出口判据，分别归属 M5/M6、M4/M6、M3、M4～M6；M1 只核对各自在 M1 范围内的分项。
- 明确保留项：M0 2026-11-02 验收豁免缺口、Windows 10/Intel Mac/Linux、真实 IME/焦点/候选窗、运行中字体重排、性能门槛冻结、视觉候选数值冻结、代码块真实横向滚动交互。
- 把裁决写入 `docs/decisions.md`，并回填架构 §13、验证计划、开发流程、AGENTS、README 与 `docs/README.md` 的当前状态入口；历史日期段落不改写。

### Non-Goals

不把任何 V 项或 AC-15/16 标成整项通过；不关闭 M0 豁免缺口；不宣称跨平台、真实 IME、辅助功能或发布门禁通过；不冻结性能门槛或骨架候选数值；不开始 M2 实现，不启用 W4/W5 接线，不新增依赖或 unsafe。

## Capabilities

### New Capabilities

无。本 change 只裁决既有阶段的出口状态，`.openspec.yaml` 使用 `skip_specs: true`；不改变任何产品/引擎/工程能力要求。

### Modified Capabilities

无。

## Impact

影响 `docs/decisions.md`、`docs/v1-architecture.md` §13、`docs/v1-validation-plan.md`、`docs/development-process.md` §1/§3.1、`AGENTS.md`、`README.md` 与 `docs/README.md` 的状态措辞；不影响代码、Cargo 清单/锁、依赖图、catalog 或平台配置。适用 CODE_STANDARDS §2～6 的文档/证据边界要求，以及 AGENTS 铁律 2/3/4/6；本裁决不触碰文本保真、坐标或引擎实现。

## Acceptance Evidence

- 2026-11-11 的 13 行核对：`logs/validation/m1-single-window-readonly/2026-11-11-82-handoff/handoff/proposal-13-crosscheck.md`。
- 五样本七步字节/只读分项：`logs/validation/m1-single-window-readonly/2026-11-11-73-real5/`（`acceptance/summary.json` `passed: true`、`failures: []`；5 例 `steps` 折叠为七步，`disk_bytes_equal`/`source_bytes_equal` 均为 true、无草稿、历史为空、revision 0/0）。
- 1 MiB 真实滚动：`logs/validation/m1-single-window-readonly/2026-11-11-82-scroll/`（`result.json` `passed: true`；滚动比例 0.0 → 0.0027 → 0.0018 → 1.0 → 0.0，213 次滚轮 + 2 次键盘事件）。
- V-05 首轮矩阵：`logs/validation/m1-single-window-readonly/2026-11-10-performance-01/performance/summary.json`（`passed: true`、`failures: []`、`commit=1893d4ac…`）。
- V-09 十四项窗口基线：`logs/validation/m1-single-window-readonly/2026-11-06-continue-06/window-baseline/summary.json`（`passed: true`、`failures: []`、14/14）。
- 交付复核修复：`logs/validation/m1-single-window-readonly/2026-11-13-18/`（无头回归与门禁；不增加平台证据）。

## Risks

- 出口裁决容易被读成能力通过或发布通过；回填必须同时保留「无一 V 项整项通过」「M0 豁免未关闭」「未测平台保留」「候选数值未冻结」四项边界。
- M1 只读面仍有未验证交互（代码块真实横向滚动）；本裁决将其列为 M2 入口待办而非 M1 出口阻塞，若后续确认是缺陷，必须在 M2 首次开放可编辑/指针驻留时修复，不能以代码路径核对代替。
- 若用户认为任一保留项应作为 M1 出口阻塞，本裁决可直接被新的用户裁决替代，不改写本轮历史记录。
