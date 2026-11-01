# R11 run-3 交接

状态：整改实现与本机验证已完成，**待新的 reviewer 独立审查**；本文件不是复审通过声明。

入口：[补记](./report-addendum.md)、[修正版采集器](./collect.py)、[安全自测](./test_collect.py)、[结构验证](./validation.json)。

新证据位于 `commands/`（17 命令×3 文件，全部 exit 0），完整检查日志位于 `checks/`。新采集窗口为 2026-11-01T08:32:46.881873Z～08:32:47.589533Z。旧 run-1 窗口终点更正为 04:39:25Z，只依据旧文件，不补造执行来源。四个既有环境覆盖值补见 `legacy-env-overrides-addendum.json`。

run-1 采集器保留原不安全版本供审计，不能复跑；`run-1/commands` 和 `legacy-gaps.json` 未修改。run-1 报告仅追加明确勘误。README 仅 hostname 最小替换，原值不在任何新产物或输出中保留；无原值证据见 `readme-redaction.json`。前后/新工具 hash 见 `legacy-hashes-before.json` 与 `hashes.json`，差异见 `collector.diff`、`checks/authorized-diff.stdout.log`（旧 hostname 亦脱敏）。

复审重点：脱敏发生于写盘前；两条 target 查询在默认 case 中；未知 label 在写入前非零拒绝；默认唯一目录与显式目录拒绝覆盖；失败保全；四个环境值与旧源原值一致；现行 GPUI 精确例外表述准确。运行 `python3 -I -B .../test_collect.py` 只用模拟子进程，不应执行真实设备命令。

不重新运行旧 collect.py。若要重跑新真实采集，显式给一个未存在的新目录，或使用默认唯一目录，不能指向已有 `commands/`。不将新采集追认为旧采集；不改变原历史缺项、跨平台/远端 CI 缺证和 M0/V 状态。不改依赖、不构建、不重新准入。

本轮代码与证据的写入所有权交回主 Agent，后续复审材料由新的 reviewer 另存，不覆盖本轮附件。
