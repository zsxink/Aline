# Aline 文档入口

当前已有八成员 Cargo workspace、无头引擎与工程守卫；2026-11-02 用户验收裁决将 M0 出口记为通过（验收豁免，非证据性验证；逐包人工复核、E2/E4/E5/E7 整项、E3 全量、E6 未测项与 V-01～V-09 仍未执行，见[验证计划](./v1-validation-plan.md)）。M1 已在 2026-11-03 授权 apply 后进入实施，当前进度与未测项以 `openspec/changes/m1-single-window-readonly/tasks.md` 与[验证计划](./v1-validation-plan.md)为准。实现任务见已归档的[M0 OpenSpec tasks](../openspec/changes/archive/2026-11-02-m0-engine-foundation/tasks.md)。

PRD定义产品结果，AGENTS铁律及已确认架构/专项约束实现，验证计划负责证据状态，decisions负责决定追踪。OpenSpec承载工程基线和拟议变更，不能覆盖已确认决定；资料无法按职责解释的冲突列入待确认，不按日期或目录自行裁决。

## 开发必读

| 入口 | 作用 |
|---|---|
| [AGENTS.md](../AGENTS.md) | 架构铁律、Agent路由、构建检查与工作规则 |
| [开发流程](./development-process.md) | M0–M6、OpenSpec、实施顺序；§5规定提交边界 |
| [执行规范](../openspec/CODE_STANDARDS.md)与[工程治理规格](../openspec/specs/engineering-governance/spec.md) | 所有实现/重构/评审的工程基线 |
| [V1 PRD](./v1-prd.md) | 产品范围、非目标及AC-01～AC-20 |
| [验证计划](./v1-validation-plan.md) | V-01～V-09、阶段/平台验证摘要和未完成项 |
| [技术架构](./v1-architecture.md) | 八crate职责、公共边界和M0–M6出口 |
| [引擎设计](./engine-design.md) | 保真/补丁、解析/映射、输入组/IME、有限HTML与依赖准入 |
| [架构规范](./architecture-guide.md) | R/K规则、唯一状态所有者、事务/异步/缓存与持久化边界 |
| [Rust规范](./rust-style-guide.md) | S/C规则、工具链、类型/错误、安全、测试与提交 |
| [UI规范](./ui-spec.md) | 布局/交互、菜单/键盘、主题/设置、中英文适配 |
| [插件规范](./plugin-system.md) | 内置提供者、启停/配置、公共命令与失败回退 |
| [主题包规范](./theme-package-spec.md) | schema 1格式、继承/导入/校验与资源安全 |
| [决策记录](./decisions.md) | 已确认决定、替代关系及回写入口 |

阅读顺序：AGENTS与开发流程 → PRD/验证计划 → 受影响架构/专项 → 对应OpenSpec工件。UI/插件/主题任务同时阅读各自规范。已确认决定不重新包装成未决问题；尚未验证的实现/数值/API不视为冻结成绩。

## 可复现验证入口

- 单元/集成测试：`crates/*/tests/`及源码旁测试；固定输入/独立预期在各crate的 `tests/fixtures/`。
- 工程门禁与反例：`scripts/check_workspace.py`、`check_engineering.py`、`check_lints.py`、`check_safety.py`、`check_public_api.py`、`check_validation_output.py`、`check_ui_catalog.py`、`check_ui_tokens.py`，输入在 `scripts/fixtures/`。
- UI 文案守卫检查直接 UI 字面串与翻译调用的字面首参 key（变量/数组间接 key 仍须人工复核）；宏、动态串与别名仍须人工复核，双语键和占位符一致性由 `aline-app-core` 测试覆盖。颜色守卫检查组件生产源码的直接字面色，不代替真实截图。
- Rust提交仍执行AGENTS列出的fmt/clippy/test；统一入口需要已安装工具链及已核对的隔离Cargo缓存，不自动下载。依赖图解析不代表跨平台构建。
- 验证运行输出放 `logs/validation/`，换会话只维护一份 `tmp/<任务>-resume.md`，两者不追踪。保留规则见[开发流程 §5](./development-process.md#5-仓库产物与提交边界)。本地输出不得成为复跑必要输入。

## 研究、历史与资源

- [Zed复核](./zed-notes.md)、[WordCraft调研](./wordcraft-study.md)、[Splat调研](./splat-study.md)、[quiet-md调研与采纳方案](./quiet-md-study.md)是参考研究，不能替代锁定源码核对与Aline实测。quiet-md 的 W0～W7 原为工作分解；2026-11-08 用户要求继续逐项实施，W1～W7 已各有独立 change（见[开发流程 §3.1](./development-process.md#31-研究采纳与工程实践落点2026-11-05)），W4/W5 保留 M2 前置；其不改变产品契约的工程/验证方法已按[决定 QMD-01](./decisions.md#25-2026-11-05-quiet-md-工程实践采纳qmd-01)回写现行规范，并由 OpenSpec change [`m1-single-window-readonly`](../openspec/changes/m1-single-window-readonly/proposal.md) 记录治理 delta，不代表已实施或已验证。
- [确认归档](./archive/)及[迁移历史证据](./archive/migration-evidence/README.md)保存已确认决定与必要研究来源，不自动取得现行规范效力。
- [历史原型](./prototype/README.md)为用户已确认长期保留的参考，且是原生 UI 的布局与控件邻接参照（[UI 规范 §4.1](./ui-spec.md#41-原型布局对照实现参照)）；除图标删除与 2026-11-03 语言切换两次记录在案的授权变更外不改历史字节；PROTO-01/03来源/许可事实仍待补证，原型主题不是正式schema 1包。
- [应用图标](./assets/app-icon/README.md)、[素材来源](./assets/provenance.md)、[架构总览](./assets/v1-architecture.svg)、[模块目录](./assets/v1-modules.svg)分别负责正式资源、来源与规划视图。

2026-11-01已按用户要求清理旧过程报告/审计目录，必要决定与状态已迁入现有入口，见[收敛决定](./decisions.md#18-2026-11-01-仓库过程产物收敛)。现行文档中的旧报告引用标为历史并指向清理前Git版本；它们只说明当时分项，不能证明当前工作树或M0/V出口。原 `docs_bak/`、`reports/` 也已授权删除；不为恢复过程痕迹重新生成目录。
