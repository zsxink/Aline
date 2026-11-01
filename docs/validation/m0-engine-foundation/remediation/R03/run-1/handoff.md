# R03 handoff（F-04 复用重定位的受检坐标边界）

## 交付物
- 代码：`crates/aline-markdown/src/reuse.rs`、`src/lib.rs`（新增 `ReuseTranslateError` 导出）、`tests/reuse.rs`（18 → 23 条，无删除、无放宽）
- 证据：`docs/validation/m0-engine-foundation/remediation/R03/run-1/`

## 一句话结论
`ReusedBlock` 不再持有裸 `Range<usize>`，改存两个带 `SnapshotId` 的 `SrcRange`；`translate` 入口只收旧快照绑定的 `SrcOffset`，出口只经 `TextSnapshot::src_offset` 产出新快照绑定的 `SrcOffset`，四步固定顺序校验、无 clamp。

## 关键裁决
1. **闭区间右端保留**。`translate(old_range.end)` 必须继续可用（`assert_all_reused_pairs_match` 依赖），语义由新增的 `translate_accepts_in_block_endpoints_and_is_byte_exact`（遍历 `start..=end` 逐字节断言）钉死。
2. **不留兼容层**。旧裸 API 无生产消费者，直接收紧签名。
3. **`ReuseTranslateError` 只派生 `Debug`**。`InvalidNewOffset` 携带 `aline_buffer::BufferError`，后者无 `PartialEq`/`Copy`；测试用 `matches!` 判定变体。这是有意的取舍，不是遗漏。

## 验证（全部实跑，工具链 1.98.1，`--locked --offline`）
- `cargo test -p aline-markdown --test reuse` → 23 passed
- `cargo test -p aline-markdown` → 12 段 `test result: ok`，exit 0
- `cargo clippy -p aline-markdown --all-targets -- -D warnings` → exit 0
- `cargo fmt --check` → 先 1，apply 后 0
- `python3 scripts/check_lints.py` → exit 0（回归，本批未改脚本）
- `python3 scripts/check_public_api.py` → 隔离空缓存下 exit 1；**真实 `CARGO_HOME=$HOME/.cargo` 下 8/8 符合预期、exit 0**

## 遗留 / 交接
- **待独立复核**：本批由主 Agent 实现（两个 Sub-agent 名额被 R02/R05 reviewer 占用），必须由非作者复核。复核重点：四步校验顺序是否真的不可绕过、闭区间右端是否有第二处隐含依赖、23 条用例有无「照实现写测试」。
- **`check_public_api.py` 的环境依赖**：隔离 home 的 `registry/` 只有 `CACHEDIR.TAG`，任何需要解析依赖的 `cargo metadata` 都会 exit 101。建议 R12/R14 前统一缓存来源，不要逐批绕开。
- **`check_workspace.py` 的 `dependency` 子门禁**同因失败（`all-metadata.stdout.log` 为空）。其余子门禁全 0。
- 未改动：`scripts/**`、`aline-ui/**`、`pre/rewrite.rs`（属 R02 所有权）。
- 本批未提交前的 `docs/validation/.../group-2/safety-results/` 脏文件与 R03 无关，未触碰。
