# R07 handoff

- 按已批准 Unicode 17/精确依赖/4096 字节预算实现字素映射查询及独立测试；主报告见 `report.md`。
- 修改：workspace 与 `aline-markdown` manifests/lock、mapping 导出与 projection helper、新 `grapheme.rs` / `tests/grapheme.rs`；回填 engine-design、v1-architecture、V-07。
- 当前验证：相关三 crate 291 passed；相关 clippy、全量 fmt-check、engineering guard exit 0。依赖图新边限定为 `aline-markdown → unicode-segmentation 1.13.3`。原始命令和失败保留于 `implementation-evidence/` 与 `dependency-check-rerun/`。
- **协议偏差：**首轮 engineering guard 缺少离线缓存包；随后误执行 `cargo fetch --locked`，下载记录为 `14-cargo-fetch.*`。复跑守卫通过时使用预热缓存的 `--locked --offline`，不宣称冷缓存离线验证。主报告已完整披露。
- 尚待：独立代码 reviewer 审查；之后 R10 可依赖 R07。未验证 UI/IME/系统字素/跨平台实际编译/性能，V-07 与 F-10 未关闭。
