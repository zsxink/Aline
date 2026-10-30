# 删除准备维护审计

2026-10-30（Asia/Shanghai）。这里是维护审计与复查证据，**不是产品规范或技术实验结果**。产品范围、实现约束及技术状态仍分别由 PRD、AGENTS/已确认专项、验证计划负责。审计阶段保留两个原目录；随后用户明确授权，现已删除，见[执行记录](./deletion-execution.md)。原审计报告保留当时结论。

后续用户又要求采用新应用图标并删除旧图，见[图标采用记录](../../decisions.md#12-2026-10-30-应用图标采用与旧图删除)。本目录 `check.py` 已支持[原型清单](../../prototype/inventory.json)中两份旧 PNG 删除、index 三处引用替换的精确授权记录；其余历史哈希保护不变。既有报告、基线及检查 JSON 保留当时快照，不改写成这次变更后的运行结果；当前状态应重新执行下方只读检查。

## 阅读入口

- [最终报告与三种场景判定](./report.md)
- [起始完整清单](./baseline-inventory.json)、[含用途/忽略规则的清单](./inventory.csv)、[范围数量](./scope-counts.json)
- [86文件统一处置表](./file-disposition.csv)
- [旧文档内容复核](./migration-content.md)、[逐主题/原行片段账本](./migration-topics.csv)、[11篇文档处置](./document-disposition.csv)
- [现行规范复核](./spec-review.md)、[89项编号需求追踪](./spec-trace.csv)、[20项AC原文追踪](./acceptance-trace.csv)
- [原型与资产复核](./prototype-review.md)、[48项逐文件表](./prototype-disposition.csv)
- [reports复核](./reports-review.md)、[26文件处置](./reports-disposition.csv)、[精选历史证据及解包方法](./prior-evidence/README.md)
- [依赖分类与三种闭包](./dependency-review.md)、[冻结历史链接的场景记录](./historical-link-scenarios.json)、[消费者链接修订记录](./link-rewrites.json)
- [当前运行汇总](./run-summary.json)、[当前状态检查](./check-current.json)、[A](./check-A.json)、[B](./check-B.json)、[C](./check-C.json)、[原型结果](./prototype-check.json)、[原型无来源模拟](./prototype-without-sources.json)
- [删除执行记录](./deletion-execution.md)、[执行JSON](./deletion-execution.json)、[原件仍在时的删除前检查](./pre-deletion-check.json)、[删除前运行汇总](./pre-deletion-run-summary.json)
- [环境版本](./environment.json)、[起始Git状态](./initial-git-status.txt)、[根入口](./root-entrypoints.json)、[本轮现行文件修订清单](./changed-files.json)、[工具禁止读原目录的结果](./tool-independence.json)

初审/收尾的旧“通过”“未确认”等只保存历史语境，不作为本轮迁移完成的证明。不要运行隔离原文中的旧生成命令来更新当前规范或历史原型。

## 可复现检查

在仓库根目录执行，不需要安装新的依赖：

```sh
python3 docs/maintenance/deletion-readiness/check.py
python3 docs/maintenance/deletion-readiness/check.py docs_bak
python3 docs/maintenance/deletion-readiness/check.py reports
python3 docs/maintenance/deletion-readiness/check.py docs_bak reports
python3 docs/prototype/tools/verify-reference.py
```

`check.py` 参数只模拟路径不可见；不会移动目录。脚本不导入 reports 工具，必需输入都在 docs 内。原目录可见时核对原件/原行及起始文件集合，缺少原件时跳过对应的可选溯源核对，仍核归档整文件/各围栏片段哈希、精选原文和保留结构。输出的 `source_checked=false` 或 `null` 是跳过来源比对，不是对已不存在原件的证明。A/B/C 检查不能代替逐主题人工归宿判断。

完整原文/历史扫描的重建读取只需 docs 内归档；其中 source 路径、旧引用和哈希都是保留的历史数据，不被按路径动态加载。旧工具链的持续运行用途已退役，现行复查由这两个工具承担；它们不会重建旧报告的时态或证明旧手工结论。

如需重新生成机器检查附件：

```sh
python3 docs/maintenance/deletion-readiness/run-checks.py
```

该编排器捕获完整 stdout 后才回写本目录的结果，避免把正在检查的 JSON 自身截断。不要直接将输出重定向到已被检查器扫描的同名 JSON。原型工具会临时绑定本机随机端口，逐文件请求并关闭服务；不执行原型 JS 或历史预览生成器。另有禁止读取不可见目录的 pathlib mock，完整可复制命令与实际结果见[原型复核](./prototype-review.md#本轮实际执行与不可见模拟)。

本轮归档真值与逐文件处置表是经审阅的维护输入，不由检查器猜测或自动生成“已迁移”判断。起始 inventory 也不能用当前树覆盖，否则会丢失保护基线。新增审计时应建新的有日期快照。

## 方法边界

静态 Markdown 链接/锚点解析、JSON/XML/Python/JS 语法、哈希和本机 HTTP 只证明相应局部断言。没有执行浏览器完整交互、真实离线演练、主题导入/原生字体、GPUI/IME、保存故障、实际导出阅读器、性能与 V-01～V-09/M0 实验；没有核实外部源码行号/commit、素材作者或许可证。PROTO-01/03 的事实缺口仍保留，已确认策略不重新开题。
