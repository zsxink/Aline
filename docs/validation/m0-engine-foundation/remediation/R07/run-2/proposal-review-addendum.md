# R07b 提案独立复审

结论：**APPROVE_WITH_CONCERNS**。原审查两条 P2 在提案层闭合，可提交用户裁决；不再有阻塞提案的问题。不代表依赖/helper 已获实施许可或 F-10 已关闭。

- 第 11 行补齐 `derive_more-impl`，与前轮独立 `cargo tree` 结果一致。
- 第 20–22 行把主查询、`PreContext` 和重复补给纳入累计预算，固定 4096 字节/每块至多 256 字节，规定类型化拒绝、不得伪报偏移和独立期望测试。锁定库 `src/grapheme.rs:398–417` 支持按请求端点补给前文，`:517–535` 可继续请求更早上下文；该 API 允许适配层在调用前停止超预算补给，无须修改第三方库。
- 第 13、26 行已写明 `U+094D U+0915` 的 17/18 边界 `[0,3,6]`/`[0,6]`，及 V-07 若要求 Unicode18/当前 UAX 时必须重新准入；新 Unicode 主版本亦须复审。与[Rev.47 GB9c](https://www.unicode.org/reports/tr29/tr29-47.html#GB9c)、[Rev.49 GB9c](https://www.unicode.org/reports/tr29/tr29-49.html#GB9c) 一致。

独立探针与结果见 [api-probe.rs](review-evidence/api-probe.rs)、[原始输出](review-evidence/api-probe.stdout.log)。使用现有锁定 rlib 编译、运行均 exit 0；Unicode17 金样通过，短 RI 补给成功，长 RI 经 15 次上下文补给累计 3968 字节后拒绝下一次超预算补给，未返回偏移。探针仅证明公开 API 可消费及预算方案可行，不验证尚未实现的 Aline helper。

实施注意：4096 是每次公开调用所交字节的累计计费值，不能宣传为库内部精确访存量或延迟上限。实现须在每次 `is_boundary`/移动/`provide_context` 调用前计费，保持 UTF-8 端点及 `PreContext` 请求端点有效，补齐提案所列长簇、双向移动和非边界拒绝测试，并取性能证据。

用户仍须先批准精确 `=1.13.3` 的唯一新引擎边、Unicode17 差异和预算拒绝语义；之后按开发流程 §2–3 更新 change、复审实施设计，再实施。建议接纳本提案作为待决材料；当前不建议合并依赖或 helper 代码。
