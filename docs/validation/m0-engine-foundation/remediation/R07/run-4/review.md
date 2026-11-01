# R07 / F-10 独立代码复核

结论：**APPROVE**，范围限于 R07 纯映射字素机制；未发现有证据、需要阻止合并的代码缺陷。复核者：未参与实现的 `r07_impl_review` reviewer agent。只读审查源码及原始证据，未运行 Cargo、未改源码。

## 核对依据

- `crates/aline-markdown/src/mapping/grapheme.rs:99`：校验投影身份、scalar 端点和完整源码上下文。
- 同文件 `:151` 起：每次 cursor query 和 `provide_context` 前均计费；单 chunk/context ≤256 字节，共享累计预算 ≤4096 字节；超限时不调用游标、不返回偏移。分块边界为 UTF-8 边界。
- `crates/aline-markdown/tests/grapheme.rs`：手写字节边界预期覆盖 Unicode17、正反移动、非字素起点、旧/部分投影、长组合簇和长 RI 超限；第三方游标类型未进入公共 API。
- 本地 `unicode-segmentation 1.13.3` 源表确认 Unicode 17.0.0，准入 SHA 与记录一致。
- 原始证据：`R07/run-3/implementation-evidence/05-grapheme-test.*` 记录 6 passed；`09-root-related-tests.*` 汇总 291 passed、0 failed；`08-root-related-clippy.*`、`12-fmt-all.*` exit 0；`16-source-sha.stdout.log` 与当前源码/配置一致。

## 协议偏差的影响

`14-cargo-fetch.command.json` 与日志证实曾执行联网 `cargo fetch --locked`。首轮 `13-engineering-guard` 的离线失败保留；`15-engineering-guard` 及 `dependency-check-rerun/` 只证明预热缓存后的离线图审计。测试和 Clippy 在 fetch 前，故偏差不推翻字素实现证据；但不能声称遵守了原离线协议，也不能声称冷缓存离线复现已验证。

该结论不覆盖完整 workspace 门禁、真实键盘/删除、IME、系统字素、Unicode 18 行为、跨平台实际构建或性能。V-07 与 M0 出口仍未通过。
