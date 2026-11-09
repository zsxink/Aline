# Proposal

## Why

QMD-01 W6 要求组件消费语义色名；当前 M1 表面色集中在 `shell.rs`，而阅读器链接、滚动条和透明边框仍散落字面量。按 UI 规范 §3.1～3.2、开发流程 §3.1 与 quiet-md 调研 W6，在 M1 进行保持色值和行为的局部重构。

## What Changes

- 建立私有原始色阶与语义 token 模块，颜色保留当前精确值，注明 UI/原型/实验来源。
- shell、阅读器和窗口组件统一引用语义名，移除已替代的常量定义。
- 增加可复跑的颜色字面量检查及正例/最小违规反例，防止后续组件绕过 token。
- 用自动色值回归与真实窗口重构前后截图互补；截图缺失时保留验收未完成。

非目标：M4 主题包、主题加载/传播、暗色算法、视觉调整、schema/继承规则变化、冻结候选数值和完整 V-09。用户 2026-11-08 已授权 W1–W7 分别建档并实施；本 change 不合并进 M1 主 change。

## Capabilities

### New Capabilities

无。纯内部重构及已有 UI §3.2 规则的工具化，使用 `skip_specs: true`。

### Modified Capabilities

无。不改变产品行为或现行规格要求。

## Impact

影响 `aline-ui/native_window/{tokens,shell,readonly_reader}.rs`、`native_window.rs` 与专属守卫；不改公开 API、Cargo 依赖、引擎、主题包模型或源码坐标/历史。遵循 CODE_STANDARDS §2～6、架构 R-13/R-17 与 Rust S-2/S-7/S-14。风险为误用 RGB/RGBA 或丢失 alpha；验收逐值比较、适用 Rust 门禁、守卫正反例及真实窗口截图。平台结果只适用于实际取证的 macOS，Windows/Linux 保留未测。
