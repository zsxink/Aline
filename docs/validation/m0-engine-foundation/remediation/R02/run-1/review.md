# R02 独立复核裁决（F-03 RewriteMap 多阶段来源复合）

- 复核对象：`0820e8e`，证据 `docs/validation/m0-engine-foundation/remediation/R02/run-1/`
- 复核者：独立 reviewer（未参与 R02 实现），主 Agent 另行逐条复核
- 裁决：**APPROVE_WITH_CONCERNS**（无阻断于本批目标；1 项既有缺陷升级为后续批次输入）

## A. 规格合规
- 达标：`compose` 不再逐片段调用会并集化的 `to_source`（`crates/aline-markdown/src/pre/rewrite.rs:197-303`）；生产代码 `unwrap/expect/panic/unreachable/assert` 计数 0；工具链 1.98.1，复跑均 `--locked --offline`；提交符合 Conventional Commits + scope `aline-markdown`。
- **部分达标**：`pre-fix-tests.txt` 只覆盖 4 条新用例中的 2 条（`compose_pairing_rules_for_empty_ranges_are_explicit`、`compose_with_identity_rewrite_preserves_every_first_level_segment`），另 2 条随实现后才加入，因此没有「修复前失败」的原始记录。**主 Agent 已复核**：`pre-fix-tests.txt` 确为 `11 tests` / `2 FAILED`。
- **部分达标**：commit message 称「新增 3 条用例」，`git show 0820e8e -- tests/rewrite.rs` 实为 **4** 条 `#[test]`。**主 Agent 已复核**：`grep -c "^+#\[test\]"` = 4。属提交信息计数错误，不是实现问题；按 §5 追加勘误，原文不改。
- 达标：既有断言改动理由成立（见 C）；`control_bytes_do_not_collide_with_synthetic_markers` 在 diff 中 0 命中。

## B. 独立复跑（全部实跑）
- `cargo test -p aline-markdown --test rewrite --locked --offline` → 13 passed / 0 failed，exit 0
- `cargo test -p aline-markdown --locked --offline` → 112 passed / 0 failed，exit 0
- `cargo fmt --check` → exit 0；`cargo clippy -p aline-markdown --all-targets -- -D warnings` → exit 0
- `git worktree` 在基线 `2f1dcea` 上跑 4 条新用例：全部失败，exit 101（如 `[(0..3,0..4,Synthetic)]`）→ **F-03 独立复现成立**；worktree 已删除，main 未动。
- 复核时工作树有 R03 的并行改动（`src/lib.rs`、`src/reuse.rs`、`tests/reuse.rs`），R02 两文件 sha256 与提交一致，不影响结论。

## C. 反「照测试写实现」
- 旧期望 `1..6` 只可能来自旧 `compose` 把 `Aligned 1..3` 与 `Exact 3..6` 并成单片；新期望 `1..3` 正是包含点 0 的 Aligned 片，符合既有 anchor 语义。改动是**纠正固化了缺陷的断言**，不是放宽。
- 4 条新用例期望值均为手写常量，且在基线 `2f1dcea` 上全部失败，非抄实现。

## D. 疑点 C 裁决（tasks.md 强制要求的三问）
1. **是否存在「源码片段互不重叠」不变式？** 否。`rewrite.rs:51-97` 未作此声明；重叠是合并片取并集的必然结果，零宽 `Hidden` 片标出被隐藏的子区间。
2. **重叠是否会导致重复计数？** 否。零宽片不参与非空 `to_source` 并集；唯一消费者 `crates/aline-markdown/src/build.rs:413` 只用 `to_source`。
3. **用例是否在固化缺陷？** 否。用例合理。

## E. 其他发现
- **[严重] F-01 残留（既有缺陷，非 R02 引入）**：第二层空来源（`Synthetic`）片段锚定到第一层 `Aligned`/`Hidden` 片段时，返回**非空 source**，与 `rewrite.rs:188-193` 文档「合成内容不伪造非空来源」冲突；`middle_anchor`（`:306-323`）与 `source_point`（`:627-644`）是成因。
  **主 Agent 已独立复现**（探针源码与原始输出存 `run-1/probe/f01-synthetic-nonempty-source.log`）：第一层 `[(0..1,0..1,Exact),(1..3,1..3,Aligned),(3..6,3..6,Exact)]`，第二层在副本点 1 插入零来源 `Synthetic`，复合结果含 `(1..3, 1..2, Synthetic)`；点 2 得 `(1..3, 2..3, Synthetic)`。**结论成立**。该缺陷在基线同样存在，R02 未引入、也未修复，新用例未覆盖。
- **[一般] F-03**：`compose` 无取消/预算，`O(n·m)` 在 1M×1M 最坏可长时间阻塞；当前生产无调用者。
- **[一般] F-04**：`compose(空 map, identity(0))` 返回 `Unmapped`（既有边界，未修）。溢出/下溢逐处有 `checked_sub` 与前置守卫，排序键保证重写端单调且锚点在内容前。

## 主 Agent 裁决
- 本批**目标达成**：F-03 的并集化抹平已被按两层片段边界求交替代，且基线复现失败、修复后通过。
- **[严重] F-01 残留**移交：它是 R02 的所有权范围（`pre/rewrite.rs` 空来源锚点），但属于**既有缺陷**而非本批回归。按「批次完成 ≠ finding 关闭」，登记为 **R02b 待办**，在 R04/R10 之前处理（R04 依赖 R02 的映射语义）。
- 两处「部分达标」为记录精度问题，已按 §5 追加勘误，不阻断。
