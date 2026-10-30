# 开发前独立终审附件

2026-10-30。本目录是维护审计，**不是产品规范或 V 实验结果**。结论适用于报告记录的工作树；不创建应用工程，不授权绕过后续阶段门禁。

- [终审报告](./report.md)：结论、发现/处理/依据、实际检查及未完成门禁。
- [实际覆盖](./coverage.md)：全文/定位/机器检查/未覆盖的边界。
- [需求/AC/V/阶段追踪](./traceability.md)与[89需求逐项表](./requirement-trace.csv)。
- [M0 具体任务顺序](./m0-task-order.md)：每项目标、crate、前置、不变量、验证、完成与回填。
- [起始 Git 状态](./initial-git-status.txt)、[基线文件身份](./baseline-files.json)、[本轮变化与并行变更](./changes.json)。
- [起始文档检查](./initial-document-check.json)、[修后文档检查](./document-check.json)、[原型检查](./prototype-check.json)、[JS/SVG/范围补检](./additional-checks.json)、[当前外链](./external-links.json)、[运行摘要](./check-summary.json)。

复查在仓库根目录运行：

```sh
python3 docs/maintenance/pre-development-review/run-checks.py
python3 docs/maintenance/pre-development-review/run-checks.py --network
```

[本轮运行器](./run-checks.py)只写本目录结果，调用现有两个核对器后捕获完整 stdout 再写，避免截断自身 JSON；网络检查可选，读取现行顶层文档及新增图标说明外链，不上传文件。不会覆盖旧删除审计输出，不执行 legacy 生成器、应用或 Cargo。网络失败单列，不自动改链接。

两个旧目录实际不存在，原件比对跳过属预期；保留归档哈希仍核对。静态 Markdown 正则/JSON/XML/JS语法/HTTP 回读不是浏览器或原生实验，旧源码/作者/许可也不因此认证。人工复查应从逐项需求行为出发重读对应正文，不以编号集合相同代替语义检查。V-01～V-09 与 M0 工具实验仍需未来实际取证回填原规范。
