# Proposal

## Why

QMD-01 W7 要求工程检查只有一个入口，性能报告和失败必须可追溯。现有 `scripts/check_workspace.py` 已顺序执行 Rust、依赖、lint、安全和 API 门禁，但未运行 M1 证据汇总器自测与 W6 语义色守卫；本变更在 M1 补齐接线，依据开发流程 §3.1、架构规范 R-11-01、Rust S-14-03 与 quiet-md 调研 W7。

## What Changes

- 复用现有 workspace 入口，加入 M1 汇总器、收集器自测、UI token 与 catalog 字面用法检查及反例。
- 对新增入口继续执行输出目录拒绝验证，失败保留 stdout/stderr 与逐条退出码。
- 核对性能报告字段、catalog 自动验证、启动失败可见性与文档单一来源；未取得的真实性能或平台证据保持未完成。

非目标：新检查总入口、新依赖、vendor 补丁、panic 恢复策略、编辑/IME、发布门槛冻结，以及把合成自测记为 V-05 成绩。用户 2026-11-08 已授权逐项建立并实施 W1–W7；本 change 独立于 M1 主 change。

## Capabilities

### New Capabilities

无。现行工程规则的工具接线，使用 `skip_specs: true`。

### Modified Capabilities

无。不改变产品或现行验收契约。

## Impact

影响 `scripts/check_workspace.py`、`scripts/check_validation_output.py`、`scripts/check_public_api.py`、`scripts/safety_lex.py`、UI 字面用法检查器和本 change；不改 Cargo/生产源码/API。公共 API 守卫使用显式提供的隔离缓存，未提供时才查询登记缓存，失效缓存仍失败关闭。遵循 CODE_STANDARDS §2～6、R-11/R-17、S-2/S-7/S-14。验收为实际检查与正反例、统一入口执行结果、OpenSpec 严格校验；真实性能由 M1 7.1/V-05 取证，跨平台未测栏保留。风险为新增脚本未完整接线或错误后续检查被跳过，采用显式检查清单及失败继续执行验证。

**2026-11-12 用户裁决（决定 §42，勾选 2.2 伞项，不改写上文）：** 2.2a / 2.2b / 2.2c 均已勾选；真实初窗失败收据在锁定 `gpui-pre 0.3.9` 的 macOS 后端下结构不可达（`Window::new` 唯一可失败点为 `platform.open_window`，`MacPlatform::open_window` 唯一 `Err` 出口为 `WindowKind::AnchoredPopup`，Aline 使用 `WindowKind::Normal`），§35 其六受控实验为负结果。2.2 按「静态边界 + 显式缺口」勾选；缺口保留，不写成平台通过，V-09 与 M1 出口仍未整项通过。工程门禁清单同期加入 startup 4.3 的两个探针自测（授权与细节见 startup change tasks 4.3 与决定 §42），全量 37 项 exit 0，证据 `logs/validation/m1-single-window-readonly/2026-11-12-startup-4_3/`。
