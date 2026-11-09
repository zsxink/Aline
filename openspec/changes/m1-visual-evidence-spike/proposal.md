# Proposal

## Why

按 QMD-01 W1（`docs/quiet-md-study.md` §W1、`docs/development-process.md` §3.1），M1 需要先确认锁定 GPUI 的离屏截图是否有可准入的真实实现，才能决定是否补充视觉回归。用户于 2026-11-08 明确要求逐项建档和实施；本 change 独立于 M1 核心真实窗口验收。

## What Changes

- 提供可复跑的 W1 能力和依赖预检：核对锁定归档身份，读取真实离屏/截图接口，采集候选 `test-support` 图，复用现有工程准入策略判断。
- 默认锁定离线预检不能取得候选图时，可显式使用 `--audit-candidate` 在 logs 内复制清单/锁，联网形成候选锁后再锁定离线复采，为具体准入评审准备差异；不修改正式清单/锁/策略，不构建或激活候选生产 feature。
- 准入失败即明确 no-go；接口存在不算运行成功。通过具体准入后才执行真实 macOS 主线程离屏窗口、像素尺寸/非空及连续两次稳定性实验。
- 2026-11-09 用户已按决定 §27 批准审计材料绑定的精确隔离 macOS arm64 图。提供固定锁/批准描述和 `run_w1_offscreen.py` 作为离线可复跑入口，完整图漂移即拒绝，生产默认特性、锁和正式准入基线不变。
- 不改变产品、生产依赖、公共窗口 API 或 M1 验收；不引入 `screen-capture`、直连 `image` 或未批准依赖。离屏不证明真实窗口 chrome、焦点、IME、锁屏、多显示器或跨平台。

## Capabilities

### New Capabilities

无。本次是候选能力调查和可复跑验证工具，复用现行工程约束，不新增产品或工程规格要求，故 `skip_specs: true`。

### Modified Capabilities

无。依据 `docs/v1-architecture.md` §3.2/§13、`docs/engine-design.md` §18.2 及现行 engineering-governance 的依赖/可信证据要求执行。

## Impact

新增 `scripts/probe_m1_visual_evidence.py`；以现有 Cargo 锁和工程策略为正式输入，不修改正式 crate、Cargo 清单、锁文件或准入策略。隔离审计的候选清单、metadata 目标桩和运行副本留 logs；用户具体批准后，固定批准锁/图描述与最小场景源码保存于 `scripts/fixtures/w1-offscreen/`，由 `scripts/run_w1_offscreen.py` 离线复跑，不属于生产依赖批准。遵守 CODE_STANDARDS §3～6、R-11/R-17、S-12/S-14。原始命令、候选图、源码核对和 no-go/go 结论放 `logs/validation/m1-single-window-readonly/<run-id>/w1/`，主 Agent 将必要摘要回填现有验证计划。V-09/M1 出口与 M0 豁免缺口不因 W1 关闭。
