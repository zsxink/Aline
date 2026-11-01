# 任务 3.2 独立复核与统一入口接线

日期：2026-10-31（UTC 02:19～02:22）。状态：独立复核通过，`scripts/check_public_api.py` 已接入 `scripts/check_workspace.py`；任务 3.2 可勾选。范围仍只限 tuple 补丁底层形状、受检坐标/身份与恒等映射边界；不包含 apply/history、复杂 Markdown 解析/映射、HTML/实体、DocCore，也不声明 V-01～V-09 或 M0 出口通过。

## 复核结论

- `aline-buffer::CheckedBatch` 私有持有 `before_id`、`before_revision`、`Vec<(Range<usize>, String)>` 原形与受检 `output_len`；公开面只有 `patches() -> &[(Range<usize>, String)]`，没有可变视图或绕过版本的裸 tuple 通道。构造检查全部端点、UTF-8、升序、重叠、同点多插入、数量/插入/输出预算及长度溢出；`validate` 先拒绝版本不符，再拒绝快照不符，不因文本相同放行另一快照。该类型仍不 apply、不写盘、不持历史。
- `aline-markdown::Projection` 的 `source_at` / `drawn_at` / `source_from_utf16` / `utf16_from_source` / `aligned` 全部在映射叶模块内完成身份校验与受检运算；恒等投影也走同一入口，调用方没有机会写局部差或自建 UTF-16 表。代理对中间、半 UTF-8、越界、局部投影外、错投影、错快照均返回类型化错误。
- `Mapped<T>` 携带来源 `SrcRange`、`MappingPrecision` 与方向偏好；本阶段恒等路径只产生 `Exact`，`Aligned`/`Hidden`/`Synthetic` 的复杂来源关系留第 6 组，不能把本记录当作复杂映射完成证据。
- `aline-ui::InputIdentity` 覆盖错文档、错工作版本、同文本新 `SnapshotId` 与代次耗尽；`DocumentId`/`WorkingVersion` 按架构 §5.1 属 UI 所有权，因此错文档/错版本用例落在 UI 外部测试，而非 buffer/markdown 重复实现。
- 未发现 type alias 充当分型、公开可变 `CheckedBatch`、调用方手算 `at - range.start` 或额外 UTF-16 表；`SnapshotId`/`ProjectionId`/`SrcOffset` 字段私有，编译夹具实际拒绝伪造。

## 环境与隔离缓存

- 主机：macOS 27.0.1（build 26A434），Darwin 27.0.0，`arm64`。记录见 `environment/sw-vers.*`、`environment/uname.*`。
- 工具链：Rust/Cargo 1.98.1、rustfmt 1.9.0、Clippy 0.1.98、edition 2024；`rustfmt --print-config current` 实际输出 `max_width = 100`、`edition = "2024"`。见 `workspace-gates/rustc-version.*`、`cargo-version.*`、`fmt-version.*`、`clippy-version.*`、`fmt-effective-config.*`。
- 隔离 `CARGO_HOME`：`<cargo-home>`。复核时发现 `admission-2026-10-30/isolation.json` 登记的临时目录已被系统清理（路径不存在）；本次以 APFS clone 从既有 `~/.cargo` 建立新的隔离缓存，并执行 `cargo fetch --locked` 补齐 lock 中已存在但缓存缺失的包。实际补齐 16 个包，其中 11 个在缓存中完全缺失：`arbitrary 1.5.0`、`cc 1.10.0`、`fontconfig-parser 0.5.8`、`js-sys 0.3.107`、`roxmltree 0.20.0`、`which 8.0.6`、`windows 0.62.2`、`windows-collections 0.3.2`、`windows-future 0.3.2`、`windows-numerics 0.3.1`、`windows-threading 0.2.1`。
- `cargo fetch --locked` 不修改 `Cargo.lock`、不新增依赖，版本与校验和由 lock 固定；后续全部门禁仍以 `CARGO_NET_OFFLINE=true` 执行。`environment/offline-fetch-check` 在补齐后再次离线执行，退出 0 且无下载，证明缓存完整。
- 首次统一入口在补齐前实际失败：`dependency` 因离线缺少 `arbitrary v1.5.0` 退出 1，整体退出 1。原始记录保留在 `workspace-gates-missing-cache-failure/`，未计为通过；补齐后重跑全绿。该失败是环境缓存缺口，不是代码或规则失败。

## 实际测试发现与结果

`targeted-tests/` 保存每条命令的 argv、cwd、UTC 时间、环境覆盖、退出码及完整 stdout/stderr。`cargo test -p <crate> -- --list` 的实际发现如下（均为退出 0）：

| crate | 测试目标 | 实际用例 |
|---|---|---|
| aline-buffer | `batch::tests` | `output_length_arithmetic_refuses_overflow_and_underflow` |
| aline-buffer | `text::tests` | `composed_length_overflow_is_checked_before_allocation` |
| aline-buffer | `tests/checked_batch.rs` | `checked_tuple_batch_keeps_original_shape_and_context`；`checked_tuple_batch_rejects_invalid_endpoints_order_and_budgets` |
| aline-buffer | `tests/public_boundaries.rs` | `single_overlay_shares_base_and_only_materializes_when_requested`；`source_coordinates_keep_snapshot_identity_and_selection_direction` |
| aline-markdown | `tests/identity_mapping.rs` | `identity_mapping_converts_local_global_and_utf16_through_one_service`；`identity_mapping_refuses_foreign_projection_snapshot_and_invalid_units` |
| aline-markdown | `tests/public_boundaries.rs` | `immutable_projection_can_be_carried_by_future_host_tasks`；`projection_coordinates_are_typed_and_bound_to_one_instance` |
| aline-ui | `catalog_tests` | `application_name_consumes_embedded_catalog` |
| aline-ui | `tests/public_boundaries.rs` | `input_identity_can_be_carried_by_future_host_tasks`；`input_identity_rejects_wrong_document_or_working_version` |

定向测试：buffer 6 passed、markdown 4 passed、ui 3 passed，全部 0 failed。全 workspace `cargo test --workspace` 实际 18 passed、0 failed（含 app-core 5 项），见 `direct-gates/test.stdout.log`。

任务 3.2 的负例覆盖对应关系：空区间/末端/反向选区由 buffer 与 markdown 外部测试覆盖；半 UTF-8、UTF-16 中间代理对、越界/溢出由 buffer/markdown 测试覆盖；错文档/错版本由 UI 信封测试覆盖；错投影、错快照由 markdown 测试覆盖；类型混传与私有字段伪造由编译夹具覆盖。

## 编译正反例

`scripts/check_public_api.py --self-test` 为每条夹具生成独立 Cargo workspace，先 `cargo generate-lockfile --offline`，再 `cargo check --locked --offline`；所有夹具显式声明 `[lints.rust] unsafe_code="forbid"`，非法夹具只编译、不运行。独立重跑 `compile-fixtures-independent/` 与统一入口内 `workspace-gates/public-api/` 均 8/8 符合预期：

| 夹具 | 预期 | 实际 |
|---|---|---|
| `positive`：三坐标转换、`StorageRange`、tuple 只读消费 | exit 0 | exit 0 |
| `drawn-is-not-source` | exit 101，E0308、`SrcOffset`、`DrawnOffset` | 匹配 |
| `drawn-is-not-utf16` | exit 101，E0308、`Utf16Offset`、`DrawnOffset` | 匹配 |
| `source-is-not-storage` | exit 101，E0308、`StorageRange`、`SrcRange` | 匹配 |
| `private-source-fields` | exit 101，E0451、`private` | 匹配 |
| `private-snapshot-id` | exit 101，E0423、`private` | 匹配 |
| `private-projection-id` | exit 101，E0423、`private` | 匹配 |
| `tuple-view-is-immutable` | exit 101，E0596、`cannot borrow` | 匹配 |

八条夹具的 lock 生成均退出 0；没有任何一条把 lock/依赖编译失败当作边界通过。每条命令、环境、UTC 时间与完整诊断见两个目录下的 `*.command.json` / `*.log`，汇总见各自 `summary.json`。

## 门禁与接线

| 检查 | 结果 | 证据 |
|---|---|---|
| `cargo fmt --all --check` | exit 0 | `direct-gates/fmt.*` |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | `direct-gates/clippy.*` |
| `cargo test --workspace` | exit 0，18 passed | `direct-gates/test.*` |
| `python3 scripts/check_workspace.py --cargo-home <隔离缓存> --output-dir <本目录>` | exit 0，19 项全部 0；`summary.json.passed=true` | `workspace-gates/` |
| 统一入口内的 `public-api` | exit 0，8/8 | `workspace-gates/public-api/summary.json` |

接线变更只有 `scripts/check_workspace.py` 的 `checks` 列表新增：

```python
('public-api', [sys.executable, str(ROOT / 'scripts/check_public_api.py'),
                '--self-test', '--output-dir', str(output / 'public-api')]),
```

`.github/workflows/m0.yml` 已直接调用 `python3 scripts/check_workspace.py --output-dir ...`，因此新增检查自动进入 CI 路径，工作流本身无需改动。远端 CI 仍未执行、runner 仍未验证，本记录不把它写成 CI 通过。

## 未覆盖与下一步

未覆盖：`CheckedBatch` 的 apply/逆补丁/历史（第 5 组）、复杂 Markdown 解析/改写/HTML/实体与 `Aligned`/`Hidden`/`Synthetic` 来源关系（第 6 组）、`plan_change`/`validate_change`（第 7 组）、DocCore 草稿/900ms/IME/发布等待（第 8 组）、真实窗口/输入法/保存/平台与 V-01～V-09（后续任务）。本记录只支持任务 3.2 勾选，不支持 M0 出口结论。

下一步按组内顺序进入 3.3：冻结公共接口、错误类别、唯一写入口与相等证据来源，并追踪实际公开消费者。
