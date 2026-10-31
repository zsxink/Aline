# 6.6 记录：预算、敌意输入与协作式取消

日期：2026-10-31。状态：`cargo test -p aline-markdown --test hostile` 通过；扫描步数阈值由仓外探针二分实测。本文只记录保护性上限与终止行为，不宣称 V-06 通过，也不把任何数值当发布限额。

## 1. 可注入预算

`ParseBudget` 八个维度均可注入，`ParseBudget::m0()` 只是测试与无头调用用的保守上限：

| 维度 | `m0()` 值 | 超限出口（已按复核 N-F2-3 更正） |
|---|---|---|
| 源码字节 | 16 MiB | 解析前拒绝（`ParseError::Source`/`BudgetExceeded`） |
| 重写文本字节 | 32 MiB | `Err(ParseError::BudgetExceeded)`（`parse.rs` 重写分支），不返回部分改写文本 |
| 映射片段 | 1,000,000 | `BuildError::BudgetExceeded` → 入口整源回退（`RawFallback` 路径，诊断 `BudgetExceeded`） |
| 扫描步数 | 4,000,000 | 构树失败 → 入口整源回退，不是“该区原文投影” |
| 嵌套 | 256 | 构树失败 → 入口整源回退 |
| 节点 | 1,000,000 | 构树失败 → 入口整源回退 |
| HTML 栈深 | 256 | 构树失败 → 入口整源回退，不是“该区原文投影” |
| 投影索引点 | 4,000,000 | `Err(ParseError::Mapping(MappingError::BudgetExceeded))`，不是整源回退 |

**为什么构树类维度只能整源回退**：扫描步数/HTML 栈深超限发生在已经隐藏过标签的事件之后，只放弃当前事件会吞掉先前事件的源码，所以构树失败让入口整体回到原文视图（`build.rs` 的 `html()` 分支注释已写明），保证不截断。

**`max_mapping_points` 的语义（复核 N-F2-3）**：该值同时传给 `MappingBudget::new(budget.max_rewritten_bytes(), budget.max_mapping_points())` 的索引点参数**与**片段数参数，因此在 `projection.rs` 里也充当投影片段数上限；一个文档可能仅因片段数超过它而触发 `Mapping(BudgetExceeded)`。`ParseStats::mapping_points` 只统计绘制字符数加一，不反映片段数维度。测试 `[a](u)\n`（`stats_points=2`、`segments=3`，上限 2 失败、3 通过）固定了这一点。

这些数值是可注入的保护上限，**不是 V-06 发布限额**；V-06 要冻结的端到端时延、峰值内存与文件/资产上限仍未测量。

## 2. 协作式取消

- `CancelCheck` trait 定义低成本 `is_cancelled()`；`CancelToken` 用 `Arc<AtomicBool>`（`Relaxed`）实现可克隆令牌。
- `parse_with_cancel` 在解析入口、重写编辑之间、Front matter 检测前后、每个事件前、构树冲刷/收尾循环间轮询；取消返回 `ParseError::Cancelled` 且不产出树。
- 不带取消参数的兼容入口用 `NeverCancel`，默认路径行为不变（`uncancelled_token_preserves_default_parse_result`）。

测试用计数式 `CancelCheck` 保证确定性，不依赖真实 sleep 或墙钟。

## 3. 敌意输入与实测步数

`tests/hostile.rs` 在修复后为 19 个用例。下表前 15 个为 6.6 首轮与 round-1 遗留项 F2-3 的用例；末 4 个为 round-2 复核 N-F2-2/N-F2-3 的处置用例：

| 用例 | 验证 |
|---|---|
| `deep_nesting_terminates_with_raw_fallback` | 超深嵌套在有限步内整源回退 |
| `deep_nesting_within_budget_keeps_full_text` | 预算内深嵌套不截断 |
| `repeated_unclosed_tags_fall_back_to_whole_source_without_truncation` | 重复未闭合标签不截断原文 |
| `repeated_unclosed_quotes_stay_linear_and_keep_source` | 重复未闭合引号保持线性并保留源 |
| `huge_paragraph_is_not_truncated` | 巨段不截断 |
| `huge_table_is_not_truncated_within_budget` | 预算内巨表不截断 |
| `injected_node_budget_terminates_before_publishing_partial_tree` | 节点预算注入后不发布部分树 |
| `source_budget_is_checked_before_reading` | 源码预算在读取前检查 |
| `scan_steps_are_measured_by_the_budget_threshold` | 未过滤样本：扫描步数由统计与二分阈值交叉验证 |
| `rewritten_budget_failure_returns_budget_error_not_partial_text` | 重写预算失败返回错误而非部分文本 |
| `pre_cancelled_parse_returns_cancelled_without_tree` | 预取消不产树 |
| `cooperative_cancel_stops_deep_nesting_without_hanging` | 取消中断深嵌套，不挂起 |
| `cooperative_cancel_stops_huge_input_without_hanging` | 取消中断巨型输入，不挂起 |
| `uncancelled_token_preserves_default_parse_result` | 未取消令牌不改变默认结果 |
| `mapping_point_budget_is_counted_in_stats_and_enforced` | 投影索引点计入统计并可强制报错 |
| `filtered_html_scan_steps_match_the_real_scanner` | 过滤样本 5/5、3/3、3/3：统计与真实扫描阈值一致（N-F2-2） |
| `unfiltered_html_scan_steps_match_the_real_scanner` | 未过滤对照 8/8（N-F2-2） |
| `scan_step_budget_failure_falls_back_to_whole_source` | 扫描步数超限整源回退，统计值保留（N-F2-2） |
| `mapping_point_budget_also_caps_projection_segments` | `[a](u)\n` 的 2 点/3 片段与 `max_mapping_points` 双重角色（N-F2-3） |

首 10 个用例为 6.6 首轮；其后 5 个（`scan_steps_are_measured…` 至 `mapping_point_budget_is_counted…`）由 round-1 遗留项 F2-3 补入；末 4 个由 round-2 复核 N-F2-2/N-F2-3 补入。修复前后计数与日志见 §5。

实测阈值：仓外探针 `step-6.6/scan-budget-probe-main.rs` 用 40 个 `<b>x</b>` 拼接的 321 字节 HTML 密集源二分得到：

```
source_bytes 321
scan_step_threshold 80
diagnostics_at_threshold Ok(0)
diagnostics_below Ok(1)
```

即最小可通过扫描步数为 80，`max_scan_steps=79` 时出现 1 条预算诊断。`ParseStats::scan_steps` 在测试中记录同一值，形成“统计值 vs 二分阈值”的交叉验证。

**过滤样本的补充交叉验证（复核 N-F2-2）**：修复前用临时探针 `probe_scan_steps` 记录 `filtered-newline`、`filtered-compact`、`filtered-nested` 的统计/阈值分别为 8/5、6/3、6/3；修复后为 5/5、3/3、3/3，未过滤对照 `unfiltered-div` 8/8 与 `repeated-b` 80/80 不变。探针输出保留在 `step-6.6/fix-n-f2-2-n-f2-3/probe-before.log` 与 `probe-after.log`；探针文件本身是临时 example，已于 10:21Z 删除。

## 4. 盲区（如实记录）

- **第三方长调用不可中断**：`pulldown-cmark` 内部事件生成、重写逐字节循环内部与投影构造本身无法中断；单个超长事件只能在结束后观察到取消。这是“无后台线程”前提下的已知限额，尚无终止策略。
- **扫描步数计数是扫描器的复刻，不是扫描器直接回报**：`html_scan_steps` 是 `tree/html.rs` 扫描循环的复刻（该循环在模块内私有）。round-2 复核 N-F2-2 指出原复刻缺少过滤分支，导致过滤样本统计 8/6/6 而真实阈值 5/3/3；修复后复刻复用真实扫描器的 `is_filtered_tag`/`filtered_close_end`，过滤标签内部同样把“剩余到闭标签”作为一步消费，并由 `HtmlStack::count_scan_steps` 按事件前的过滤状态计数。扫描语义变化仍须同步修改，现由过滤样本 5/5、3/3、3/3 与未过滤 8/8、80/80 交叉验证兜底。
- **预算注入的实际诊断种类是 `BudgetExceeded`，不是 `RawFallback`**：`parse.rs` 的 `raw_fallback` 只在非预算 `BuildError`/`RewriteError` 时用 `RawFallback`，公开 `parse` 入口目前无法触达该分支。`tests/diagnostics.rs` 因此按实际实现断言 `BudgetExceeded`，另用类型/范围/dump 名称契约用例固定 `RawFallback` 的枚举值，并明确注释不宣称该分支已执行。
- 未测量真实耗时、峰值内存与 1/4 MiB 规模；这些属 V-05/V-06。

## 5. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:13Z | `cargo test -p aline-markdown --locked --offline --test hostile` | 0 | `step-6.6/targeted-tests/hostile.stdout.log`（10 passed） |
| 2026-10-31T09:13:09Z | `cargo run -q`（扫描步数阈值探针） | 0 | `step-6.6/scan-budget-probe.stdout.log` / `.stderr.log` |
| 2026-10-31T10:23:36Z–10:23:37Z | `cargo test … --test hostile`、全 crate、clippy、fmt | 0 | `step-6.6/fix-n-f2-2-n-f2-3/targeted-hostile.stdout.log`（19 passed）等，见该目录 `commands.log` |
| 2026-10-31T10:27:35Z | `cargo fmt --all --check`、`cargo clippy -p aline-markdown --all-targets` | 0 / 0 | `leader-verify-round-2/fmt.stdout.log`、`clippy.stdout.log` |
| 2026-10-31T10:28:17Z | `python3 scripts/check_workspace.py …` | 0 | `leader-verify-round-2/workspace-gates/`（19 项，`cargo test --workspace` 204 passed） |

`08:48` 记录是修复前快照（10 例）。F2-3 补入 5 个用例后为 15 例，修复后证据见 `step-6.8/targeted-tests-post-fix/hostile.stdout.log`；round-2 复核 N-F2-2/N-F2-3 再补 4 个用例后为 19 例，证据见 `step-6.6/fix-n-f2-2-n-f2-3/` 与调度者复跑目录 `leader-verify-round-2/`。
