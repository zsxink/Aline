# R11 reviewer 检查结果（摘要）

reviewer 通过 CodeGraph 读取 `run-1/collect.py` 当前带行号源码；所有下面只读检查 exit 0。实际 Python heredoc 原始 stdout/stderr 未由 reviewer 保存，本页是其审查返回摘要，不冒充原始日志。R11/run-3 的修复复核须重新保全完整 argv/output。

| 检查 | 实际方法 | 结果 |
|---|---|---|
| 17项新命令记录 | Python解析 command.json 和匹配 stdout/stderr | 17×3=51文件；字段、时间、cwd、env、argv与label一致；全 exit 0，stderr空 |
| 旧记录缺口 | 递归读105 command JSON并对照 `legacy-gaps.json` | 105份、14缺字段准确；4份 safety 缺 cwd/起止，10份 tree 缺起止；另12条 environment 无 cwd/时间 |
| 候选身份 | 两份 Cargo.lock 对照 manifest、identity、ledger、新摘录 | version/source/checksum均相符，SHA256 `34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0` |
| 现存脱敏 | 对 R11/run-1 定向扫描，不输出匹配值 | 三个硬件标识和 uname host 已为 `[REDACTED]`；未发现残留；README历史host另存仍存在 |
| collector回放 | 纯内存OUT与模拟subprocess调用真实 `collect.main()` | 15 cases、target cases 0、未知label exit 0；4类假敏感值原样写入mock日志并覆盖占位符，真实子进程/文件写入为0 |
| 规范差异 | `rg` 阅读架构/引擎准入文字 | 确认报告的“regex禁令仍成立”与精确路径例外冲突 |

复核结论来源：未参与原采集的 R11 reviewer 在主 Agent 会话返回完整 finding 与结果。主 Agent 将其转写入 `review.md`，未补造原始 stdout/stderr。原始新采集件继续保存在 `run-1/`；不会用后续修复追认历史过程。
