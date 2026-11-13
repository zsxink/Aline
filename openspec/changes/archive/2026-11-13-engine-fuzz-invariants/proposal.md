# Proposal

## Why

QMD-01 W3 及引擎设计 §14 E-T20、架构规范 R-17-03、Rust S-12-06 已要求确定性随机不变量，但四个引擎尚无持续操作序列的专用入口。本 change 在 M1 增加测试证据，补强 SAFE-03、EDIT-06、MD-11 / AC-02～06/09 / V-07/08，不改变产品行为或把随机测试当作平台验收。

## What Changes

- 为 buffer、markdown、edit、source 分别增加无新依赖的随机测试，共享仅测试使用的确定性 RNG 与复现配置。
- 默认 16 个固定 seed × 100 步；显式环境变量可复跑或扩量。每步核对独立不变量，失败留下 seed、步号、操作序列与输入；独立最小化入口保留仍能触发失败的最小输入，不用规范化隐藏损失。
- 以一次故意损坏测试观测值的负例确认断言确实失败，恢复后同 seed 通过；原始输出只留 logs。
- 不新增引擎生产代码、依赖、保存协调器、真实 IME 或 GPUI；source 仅测现有 create-only 原语，不把它称为 M2 受检替换/崩溃恢复闭环。DocCore 与 M2/M3 扩展仍另按阶段实施。

## Capabilities

### New Capabilities

无。纯测试补强，`.openspec.yaml` 使用 `skip_specs: true`；既有 R-17-03/S-12-06/E-T20 契约不变。

### Modified Capabilities

无。

## Impact

仅四 crate 的 `tests/` 和测试共享辅助文件；遵守 CODE_STANDARDS §2～6，状态由每个测试本地拥有，不改公开 API、生产依赖边或运行时行为。风险是生成器覆盖有限及 oracle 自证循环，故采用独立字节模型、全量解析差分、显式拒绝/金样以及负例证明；随机通过不构成 V-07/08 或 M0 豁免缺口已验证。

验收证据：四 crate `--test fuzz_invariants`、适用 Clippy/fmt、负例非零后同 seed 通过、扩量 seed 输出与工程守卫；均在 `logs/validation/m1-single-window-readonly/<run-id>/w3/`，路径经既有 `scripts/evidence_paths.py` 归一化。授权来自用户本轮要求逐项建立并实施 W1～W7，不改变已确认阶段前置。
