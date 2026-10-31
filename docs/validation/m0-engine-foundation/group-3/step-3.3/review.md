# 任务 3.3 独立复核记录

日期：2026-10-31。复核者：独立只读 Explore 子代理，未参与本任务实现。复核对象：`report.md`、四份回填文档的实际 diff、相关公开 `lib.rs`、delta specs 与 `openspec/CODE_STANDARDS.md`。

结论：**阻塞 0 项**。未发现“仅契约”被写成已实现/通过，也未发现回填改动了已确认的契约文本。复核提出 7 项应修与 5 项建议，均已逐条核实并处理；处理后可勾选 3.3。

## 1. 应修项与处理

| # | 复核发现 | 独立核实 | 处理 |
|---|---|---|---|
| 1 | `trace/commands.log` 的 `rg consumer types` 不能生成 `consumer-types.stdout.log`，且脚本未登记该扫描；手工追加命令与输出不匹配 | 确认。旧输出 75 行含类型名与 `Projection::identity` 调用行，`rg -e 'consumer types'` 语义不可能产生它；脚本第 23 行会截断命令日志 | 在 `trace/run-trace.sh` 加入精确命令 `rg -n -e 'TextSnapshot\|StorageSnapshot\|CheckedBatch\|InputIdentity' crates app -g '*.rs'`，整脚本于 02:43Z 重跑；`commands.log` 由脚本重新生成，输出为新命令的 68 行，不再手工补录 |
| 2 | `docs/engine-design.md` §3 写“当前公开面只有……”，遗漏 app-core/UI 其他公开项及 buffer/markdown 的坐标、预算、错误类型 | 确认。与报告第 2 节及实际 `lib.rs` 导出不一致 | 改为“与本任务八接口相关的已落地公开面包括……”，补列 source/buffer/markdown/UI 相关类型，并注明完整公开面见报告第 2 节 |
| 3 | 报告 3.8 把“旧工作版本”“预算超限”写成 `snapshot_current` 错误类别，但输入无期望版本/预算，规范也未定义；遗漏 ED-01 发布等待 | 确认。`document-core` spec 只规定草稿基线冲突；`StaleWorkingVersion` 属 UI `InputIdentity` 身份层 | 错误类别收敛为草稿基线冲突，并注明工作版本身份校验属 UI 层、不新增未确认类别；输入/输出补充 ED-01 的“可取消地等待组合自然结束后取快照”，编排归第 8 组 |
| 4 | 报告 3.7 写“`BufferError` 全集”，实际枚举还含 `NestedOverlay`/`InvalidBacking` | 确认，见 `crates/aline-buffer/src/text.rs` | 改为“与 apply 相关的变体”，并说明 `NestedOverlay`/`InvalidBacking` 属快照/覆盖层构造边界，不是 apply 结果 |
| 5 | 报告把 grep 无命中写成“公开面不存在旁路和调用方手算”，超出扫描能证明的范围，违反 CODE_STANDARDS §6.2 | 确认。`SrcOffset::byte_offset`、`SrcRange::byte_range`、`StorageRange::byte_range`、`CheckedBatch::patches` 等原始访问器仍公开 | 报告、`engine-design.md` §7.1、`rust-style-guide.md` §5、V-07 统一改为“当前仓库扫描未发现……，但不能证明公开 API 无法被绕过”，并点名仍公开的访问器 |
| 6 | 多处“已落地的是/只有”清单不完整，遗漏 `DocumentRevision`、`WorkingVersion`、`CheckedBatch`、`TextSnapshot`、`StorageSnapshot` | 确认。报告第 2 节与状态段落口径不一致 | 将穷举措辞改为“包括”，补齐上述类型；报告第 6 节不再把不完整列表称为“已实现类型清单” |
| 7 | 报告把 UTF-16 过滤结果写成“过滤 mapping 后仅 `lib.rs:7`”，脚本实际同时过滤了 tests | 确认。`utf16-all.stdout.log` 另有 tests 命中 | 改为“过滤 mapping 与 tests 后仅 `crates/aline-markdown/src/lib.rs:7`” |

## 2. 建议项与处理

| # | 建议 | 处理 |
|---|---|---|
| 1 | 架构 §7.1 的 `plan_change`/`validate_change` 单元格易被读成 edit 层已实现 | 拆成“`plan_change` 与 edit 层 `validate_change` 均未实现（第 7 组）；buffer/markdown/UI 的 validate 三层已实现” |
| 2 | `public-api` 8/8 是 `--self-test` 夹具结果，不是仓库实际公开面检查；统一入口命令用了占位路径 | 报告改为“public-api self-test 8/8（夹具自测，不是对仓库实际公开面的检查）”，并写出实际 `CARGO_HOME`/`--output-dir` 路径，指向各 `*.command.json` |
| 3 | `encode` 的“编码不可表示”无规范出处，输入也未写预算 | 改为规范用语“编码失败”，输入补 `StorageBudget` |
| 4 | `TextSnapshot` 的“生产消费者只有……”未限定跨 crate，同 crate `batch.rs` 也消费它 | 改为“跨 crate 生产消费者只有……”，并注明 `batch.rs` 在 `CheckedBatch` 校验中消费同 crate 类型 |
| 5 | 报告称“可勾选”早于 `tasks.md` 勾选与 trace 重跑 | 已先完成全部修复、重跑 trace 与三条门禁，再勾选 `tasks.md` 3.3 并保留本记录链接 |

## 3. 复核后的验证

- `trace/run-trace.sh` 整脚本重跑（2026-10-31T02:43:40Z 起）：`consumer-types` 与其余扫描、workspace `--list`（20 个测试）、外部消费者（5 passed）全部按脚本生成；`coordinate-arithmetic-all`、`range-field-use` 仍为预期的无匹配 exit 1。
- 三条直接门禁重跑（2026-10-31T02:44:03Z 起）：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --locked --offline` 均 exit 0；测试 20 passed、0 failed。见 `gates/commands.log`。
- 本轮只改文档、证据脚本与报告，没有 Rust 代码或依赖变化；没有开窗口。复核不改变“未实现接口仅契约未验证”的结论，也不表示 V-01～V-09 或 M0 出口通过。
