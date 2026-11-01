# R05c 交接

- 修改仅 `scripts/check_lints.py`；本目录为新增证据。
- D1：保留通用展平撞名检查，随后严格验证两个 workspace lint 策略的真实元组路径；孤立引号点分策略键拒绝且定位文件/行/键/双方路径。
- D2：按已知源文件前缀解析行号，支持空格和冒号；完整元组路径断言更严格。
- 12 个表路径自测（含原 4 个）与原 parser/member 自测全通过；完整含空格目录 self-test 通过；9 个定向变异全变红。
- Python AST/py_compile、正常仓库检查、Cargo fmt、diff check、CodeGraph sync/status 通过。
- 新增直接 Cargo 对照先离线生成 lock，再锁定离线；最终八条 fresh 命令各用独立 target，混合错误配置 0，真实 clippy/rust 策略对违规源码 101，`-vv` 参数与诊断吻合。
- 初次真实 rust 控制共用 target 得到意外 0，原日志保留并弃作有效控制；独立 target 重跑正确。具体见 report。
- 旧 R05c/run-1/pre-fix.txt 原样保留；未改任务表/progress/R05b review，未回退其它 Agent 改动。
- 生产文件所有权交回主 Agent；请独立核对 diff/原始输出并安排审查。
- 本轮未重复 workspace clippy/test（Python 局部修复；R14 最终门禁统一执行），未验证其它平台；现有深度/RecursionError 自测边界及 self-test 仅显式 offline 的 Cargo 调用保持原范围。
- 详细证据：[report.md](report.md)、pre/post 命令三件套、fresh 命令三件套、mutation-summary.json、pre/post-self-test/。
