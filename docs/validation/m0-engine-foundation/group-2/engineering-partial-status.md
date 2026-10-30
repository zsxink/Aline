# 任务 2.2 中途状态（2026-10-31）

因 source 表示契约需用户裁决，当前按主 Agent 要求收敛。任务 2.2 **未完成、未勾选**，不能将本目录或检查器存在视为正式门禁通过。

## 当前实现

- `scripts/engineering_dependencies.py` 消费八成员 metadata、tree 和 lock；已补直接及中间包的内部方向检查，以及 metadata-only 自有禁库旁路检查。
- `scripts/check_engineering.py` 建立七种组合的同步 Cargo 采集入口，检查命令退出码、输出 hash、目标/feature 标签及 normal/dev/build 范围。七种组合为 all-target、五个登记桌面 triple 及当前 workspace 的 all-features；当前八成员没有自有 feature，未来新增组合须登记，不能仅以全特征并集代替。
- `scripts/engineering_policy.json` 保持 `reviewed=false`。GPUI 正常图会拒绝，等待正式基线审查；`--self-test` 明确非零并报告尚未完成，避免虚假通过。
- `scripts/engineering_audit_baseline.py` 生成逐项历史→正式子图差异，不自动批准。差异在 `engineering-baseline-diff.json`；all-target 包/身份/来源/边未变，`toml_datetime` 因已准入的 toml 统一 feature 增加 serde。桌面候选与正式配置的 GPUI feature 不同，具体移除后端与边逐项保留。

## 实际执行

`engineering-partial-results/` 的每项包含完整 argv、cwd、时间、退出码、stdout/stderr hash 和原始输出：

1. `python3 scripts/engineering_regression.py`：退出码 0。六个明确标记的合成图局部回归符合预期，覆盖三条直接非法方向、两条中间包非法方向与 UI→source 正例。它们来自实际 metadata 的合成变异，**不是实际 Cargo 解析的违规清单**。
2. `python3 scripts/check_engineering.py --self-test`：退出码 1，诊断任务 2.2 完整自测尚未完成。此为拒绝未完工自测的实际结果，不能记作完整守卫反例通过。
3. Python AST 语法检查：退出码 0，仅证明当前脚本可解析。

## 尚未执行或未验证

- 当前新入口的正常图端到端运行、完整隔离 Cargo 正反例、自测 CLI 的完整实现、恢复正常图重跑：未执行/未验证。
- GPUI 正式基线批准、完整来源/checksum/target/features 漂移及所有禁库反例：未完成。
- 五个桌面 triple 仅有此前 Cargo 图解析日志，不能推断跨平台构建、原生窗口或平台支持。
- 本次未运行 fmt/Clippy/workspace test；尚未接 CI、catalog 或 source/buffer 功能。

后续先完成基线独立复核和完整自测，再由主 Agent 检查、运行正式入口后决定是否勾选 2.2。不得因本报告将 E1 或 V-01～V-09 改为通过。
