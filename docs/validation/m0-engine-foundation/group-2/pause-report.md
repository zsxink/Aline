# 第 2 组继续实施后的暂停记录（2026-10-31）

## 任务与实际状态

本轮继续任务 2.2：建立正式八成员 workspace 的依赖检查入口。任务尚未完成，未进入 2.3；tasks.md 未新增勾选，累计仍为 5/46。此前 2.1 的骨架构建与测试只验证空目标，不能作为 source/buffer/markdown/edit 功能证据。

本轮修复直接及中间包的内部非法依赖方向、metadata-only 自有禁库旁路，并增加采集记录的 target/feature 标签核验。新增检查入口、显式未批准策略、基线差异审计及局部回归脚本；文件清单和范围见 [中途状态](engineering-partial-status.md)。本轮没有修改 Cargo、crate、规范或任务完成状态，没有下载、安装、删除文件或提交。

## 实际命令与结果

完整参数、时间、退出码与输出分别在同名 command.json、stdout.log、stderr.log 中。

| 命令/记录 | 退出码 | 实际含义 |
|---|---|---|
| `python3 scripts/engineering_regression.py`；root-regression.* | 0 | 主 Agent 重跑六个局部合成图正反例，结果符合预期；不是实际 Cargo 违规清单的完整自测 |
| Python `compile` 语法检查；partial-syntax.* | 0 | 当前脚本语法可解析；不证明规则完整 |
| `python3 scripts/check_engineering.py --self-test`；engineering-partial-results/ | 1 | 完整自测尚未实现，明确拒绝；不记为门禁通过 |
| `codegraph sync`；pause-codegraph-sync.* | 0 | 同步新增/修改代码 |
| `codegraph status`；pause-codegraph-status.* | 0 | 实际输出 `Index is up to date` |
| `openspec validate m0-engine-foundation --strict --no-interactive`；pause-openspec-validate.* | 0 | 只证明 change 结构有效 |

本轮 fmt、Clippy、workspace test、完整 lint/catalog 自测、CI：未执行。未改变 Rust 源码，Python 局部检查不代替这些检查。

## 独立审查与未决项

独立 reviewer 用内存图变异确认两处漏洞修复后拒绝，结果见 review-recheck.stdout.json。审查为了验证图规则只在进程内对策略副本设置 reviewed=true，不代表正式基线获得批准；正式 engineering_policy.json 继续 reviewed=false。Windows 图出现 metadata/tree 选包可达性不一致，尚未定位；all-features 缺 tree，未验证。不得放宽规则掩盖差异，亦不得据桌面 triple 的图解析声明跨平台支持。

正常图端到端入口、完整隔离 Cargo 正反例、正式 GPUI 基线复核和恢复正常图重跑均未完成，因此任务 2.2 保持未勾选。第 2 组并未整体通过。

## 暂停原因与后续边界

source 表示的独立分析暴露 `CR X LF` 删除 X 后形成 CRLF 的契约歧义。简单 CRLF→LF 表示无法同时保证任意编辑后的严格源码字符往返与既定行尾编码。另一 raw-source 表示可实现，但必须先明确：新拼接的 CRLF 是否按一个行尾单元参与 LF/CRLF 等价，而不要求这两个源码字符在 LF 输出后仍逐字符相同。

这项含义尚未由用户裁决。本轮仅提出具体方案，不修改已确认契约，不实施依赖该决定的公共边界及 source/buffer。按用户的设计/契约停止条件等待确认；确认后先同步受影响的 proposal/specs/design/tasks，再继续实际实现与验证。所有 V-01～V-09、M0 出口和 AC 完整验收状态保持未完成。
