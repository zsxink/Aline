# Proposal

## Why

2026-11-05 用户要求独立复核 [quiet-md 调研与采纳方案](../../../docs/quiet-md-study.md)，并把其中可迁移的工程/验证方法整合进 Aline 的开发、架构、引擎、Rust 与 UI 规范。方法已按[决定 QMD-01](../../../docs/decisions.md#25-2026-11-05-quiet-md-工程实践采纳qmd-01)回写到现行文档，但该决定同时规定：改变验收方式（如 V-04/V-05/V-09 增加夹具或报告字段）仍须先建 OpenSpec change。建立本变更前，`openspec/changes/` 没有对应工件，构成“方法已写入规范、治理入口缺失”的缺口。

本变更补齐该入口：把 QMD-01 的方法采纳、来源边界与 W0～W7 立项门写成工程治理增量规格，并同步 `openspec/CODE_STANDARDS.md`。它只追认已完成的文档回写，不实施任何 W1～W7 代码、依赖、夹具或平台验证。

## What Changes

- 在 `engineering-governance` 增量规格中记录 QMD-01 的方法采纳，包括来源标注、效力边界和“研究文档不是第二权威”的约束。
- 把以下方法写成可验收要求：随机不变量测试的确定性/复现证据、性能报告字段与门槛归属、保存与恢复的故障安全分离、IME 显式清理与坐标边界、语义 token/中文排版测量门/离屏视觉证据的边界。
- 规定 W0～W7 必须逐项建立独立 OpenSpec change，依赖准入、夹具、验收方式或跨模块契约变化先评审再写代码。
- 同步 `openspec/CODE_STANDARDS.md` 的检查与交付说明，引用本变更与 QMD-01，不复制 quiet-md 的实现或数值。
- 修正回写中的来源表述、fuzz 证据字段、阶段口径与重复段落，使各文档对同一方法给出一致要求。

### Non-Goals

不新增产品行为、PRD/AC、crate、依赖、feature、生产代码、测试夹具或平台证据；不执行 V-01～V-09；不把 quiet-md 的数值、测试覆盖、vendor 补丁或 GUI 行为写成 Aline 已验证事实；不批准 W1 的 `test-support` 准入、W2 的 vendor/fork 顺序、W3 的 fuzz 规模、W4/W5 的阶段或 W6 的 token 时点；不替代 M1 当前 change 的范围与状态；不归档本变更。

## Capabilities

### New Capabilities

无。本变更只补充既有工程治理能力，不新增产品 capability。

### Modified Capabilities

- `engineering-governance`：新增验证方法采纳、随机/性能证据、保存/恢复、IME/坐标、UI token/中文排版/离屏证据及 W0～W7 立项门要求；不改变既有“实现必须遵循当前工程基线”“测试和交付必须有独立可信证据”等主规格要求。

## Impact

文档与 OpenSpec 工件：`docs/decisions.md` §25、`docs/development-process.md` §3.1、`docs/v1-architecture.md` §7.3/§7.4/§10.1/§10.2/§13/§14、`docs/architecture-guide.md` R-11-01/R-15-13/R-17-03、`docs/engine-design.md` §10/§14、`docs/rust-style-guide.md` §12/§17、`docs/ui-spec.md` §3.2/§6/§13、`docs/v1-validation-plan.md` V-04/V-05/V-07/V-08/V-09、`docs/quiet-md-study.md`、`AGENTS.md`、`docs/README.md`、`openspec/CODE_STANDARDS.md` 与本 change。

不涉及 `app`、`aline-ui`、`aline-source`、`aline-buffer`、`aline-markdown`、`aline-edit`、`aline-plugin`、`aline-app-core` 的代码、Cargo 配置、依赖图或公共 API；不改变 M1 change 的任务勾选与验收结论。若后续 W 项改变依赖、验收方式、跨模块契约或阶段范围，必须另建 change 并按开发流程 §2 重新评审。

### Acceptance Evidence

| 可检查结果 | 证据 |
|---|---|
| 方法采纳可追踪 | 本 proposal/design/tasks 与 QMD-01、开发流程 §3.1 的双向链接 |
| 增量规格结构有效 | `openspec validate qmd-01-engineering-practices --strict --no-interactive` 通过；全量校验另行记录 |
| 文档一致 | 同一 fuzz 字段、阶段口径、保存/恢复与 `.bak` 表述在架构、引擎、Rust、验证计划中一致 |
| 来源不过度归因 | QMD-01 的方法表区分 quiet-md 已有、Aline 既有与 Aline 追加；研究文档的推论与观察保持区分 |
| 无实现越界 | `git diff --stat` 不出现 Cargo、`crates/`、`app/` 或主规格改动；未声称任何 V/AC 通过 |

### Risks

- **过度采纳**：把 quiet-md 的实现细节当成 Aline 规范。缓解：逐项标注来源，数值、覆盖与平台能力只作待核对线索。
- **要求过重**：fuzz 最小输入、性能报告字段或 IME 核对点增加交付成本。缓解：字段只在适用 V 项执行，默认规模在 W3 立项时按 CI 预算确定，方法不冻结门槛。
- **治理循环**：本变更本身也可能被误读为批准 W1～W7。缓解：proposal、design、tasks 与规格场景都明确“未立项、不写代码”。
- **文档漂移**：研究文档、决定与现行规范再次出现不同口径。缓解：以现行 `docs/` 为唯一规范来源，研究文档只作证据索引；本次修正重复与错误归因。

### Open Questions

无新增待确认问题。quiet-md 调研原有的 test-support 准入、vendor 顺序、M1 中文排版已知限制、token 时点、fuzz 规模、W4/W5 阶段与窗口构造重构仍保持待确认，由各自 W 项 change 裁决；本变更不自行决定。
