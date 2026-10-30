# 既有迁移审计的精选历史证据

日期：2026-10-30。这里保存此前审计交付中仍有追溯价值的信息，**不是当前产品规范、本轮独立复核的证明、V 实验结果或删除授权**。本轮逐文件判断见[reports 复核](../reports-review.md)和[26文件处置表](../reports-disposition.csv)。不能因为旧报告自称“无剩余问题”而跳过本轮复核。

## 可独立查阅的证据

| 主题 | 独立目标 | 保存边界 |
|---|---|---|
| 旧功能清单的67项逐条历史对照 | [功能复核原文](./features-review.md) | 全文原字符隔离；当时PRD版本、建议与结论保持历史身份 |
| 确认前全量审计及未迁证据发现 | [初审报告原文](./initial-audit.md) | 全文保留；不能把其中“未确认/未迁入”当作现状 |
| 确认后回写与此前删除准备结论 | [收尾报告原文](./closure-report.md) | 全文保留；其“可进入删除评估”是待复核的历史判断 |
| 确认前60文件/331主题映射 | [CSV](./initial-file-mapping.csv)、[主题原表](./initial-topic-mapping.md) | 人工判断与细项原文全部保存；标题覆盖不是语义证明 |
| 确认后60文件/331主题映射 | [CSV](./closure-file-mapping.csv)、[主题原表](./closure-topic-mapping.md) | 包括泛化“已实施”声明，不能当作独立证明；本轮逐主题复核另见上层附件 |
| 确认前WC、PROTO、审计总表三份原问题来源 | [源快照JSON](./confirmed-question-sources.json) | 三个字符串原字节保存；不把后续采纳倒写进原问题 |
| 此前reviewer发现、修复及结论 | [审查记录原文](./prior-review.md) | 保留3个曾发现问题及范围，不为其无问题结论背书 |
| 三阶段逐条引用扫描、两个摘要、旧来源不可见检查输出 | [历史扫描集](./scan-history.json) | 含原始引用、源行、清单、结果、方法和限制，按对象去重；不是新检查结果 |
| 旧检查点身份 | [历史扫描集](./scan-history.json)的`historical_checkpoints` | 两份旧基线按指纹索引保留；不复制旧目录正文，不作为新工具输入 |
| 原件/归档哈希与消费者迁移表 | [manifest](./manifest.json)、[old-to-new](./old-to-new.json) | 前者核原文与目标；后者只定位9份精选原件及2个重复MD表的CSV替代 |

## 现行来源与复查方法

现行职责分别由[PRD](../../../v1-prd.md)、[验证计划](../../../v1-validation-plan.md)、[决定记录](../../../decisions.md)和对应专项承担；[WC/PROTO确认归档](../../../archive/2026-10-30-migration-decisions.md)保存确认事实；[旧文档原证据索引](../../../archive/migration-evidence/README.md)保存已选原文片段；[原型说明](../../../prototype/README.md)与[素材来源账本](../../../assets/provenance.md)承担历史运行副本及待补来源事实。以上链接是当前可用入口，围栏内旧相对链接只保留原字符，不会动态读取旧目录。

原件Markdown包装采用至少8个波浪号的围栏，内部全文逐字节保留。`manifest.json.records`的`mode=fenced_original`项给出原路径、原字节数、原哈希和目标哈希；取包装中第一个围栏开始标记下一行起的`source_bytes`字节即可恢复原文并验哈希。`mode=bytes`的CSV/JSON目标与原件逐字节相同。

历史扫描解包：对`scan-history.json.scans[phase].data`的每个字段递归处理；`{"$record": key}`取顶层`records[key]`原JSON对象，`{"$list": [...]}`递归还原数组，其他值不变。对象池里的记录已是原JSON，不再解释其中的字符串。六个阶段的字段、数组顺序与原JSON已逐项比较相等，原格式空白未保留。`historical_checkpoints.checkpoints[old_path].entries`按原顺序引用`historical_checkpoints.fingerprints`，还原两份旧基线，保留原字段名/值；记录过去哈希不能重建过去正文。`historical_inventory_identities.identity_by_path`补充旧76行清单的原用途/权威身份字符串，原路径/大小/哈希在after阶段scope中。

在仓库根目录运行本轮[独立检查工具](../check.py)：

```sh
python3 docs/maintenance/deletion-readiness/check.py
python3 docs/prototype/tools/verify-reference.py
```

旧`reports/…/check.py`与`build-ledgers.py`已经退役为本次审计的历史对象，不是上述命令的依赖，不运行其写入式生成器。新工具不重建旧报告的时态，也不将本机HTTP、哈希、路径模拟或旧JSON的`errors=[]`解释为浏览器完整交互、原生能力或V实验通过。外部网站、第三方源码行号、素材作者及许可事实不由本证据集证明。
