# R12 独立复核

复核 Agent：`r12_independent_review`（未参与 R12 实现）；范围：脚本锁定口径、argv 记录字段、AST 审计覆盖、命令原始证据及工程守卫报告勘误。仅只读 Python/文件核验；未运行 Cargo。

## 结论

**APPROVE，建议接受 R12。** 原三项问题均已关闭，未发现新的阻断问题。

## 核验结果

- 缺失 manifest metadata 负例已带 `--locked`；报告与 append-only 勘误准确说明它测试缺失 manifest，而非 lock 缺失。
- AST 文件级豁免已删除。独立重算：25 个调用点、16/16 依赖检查均 locked/offline、0 缺锁、3 个初始生成例外；在内存 AST 中去掉正向 metadata 的锁参数会被判为“未修”。
- 补充归档 77 条实际 Cargo 记录的字段均与 argv 一致；public API 记录为 8 次初始生成、8 次锁定检查。engineering self-test 97/97、public API 8/8 的原始 summary 通过。
- 最终 5 项源码指纹全部匹配；旧审计、旧报告和失败记录均保留。最终指纹链接指向 `primary-final-source-identity.json`。

## 范围限制

本复核不替代 R14 workspace 全量门禁或 M0 出口裁决。
