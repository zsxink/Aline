# R02 run-1 报告：F-03 RewriteMap 多阶段来源复合

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。工具链 1.98.1，`CARGO_NET_OFFLINE=true`。
所有权范围：`crates/aline-markdown/src/pre/rewrite.rs`、`crates/aline-markdown/tests/rewrite.rs`、本批次证据目录。

## 1. 根因

旧 `compose` 对**每一个第二层片段**调用一次 `self.to_source(segment.source)`：

```rust
for segment in &next.segments {
    let mapped = self.to_source(segment.source.clone())?;
    segments.push(RewriteSegment { source: mapped.source, ... });
}
```

`to_source` 会把落在查询区间内的所有第一层片段**并成一个**源码区间（取最强精确性）。
于是第一层里「Aligned 1..3（`"bc"`→`"X"`）」与「Exact 3..6（`"def"`）」被并成 `1..6`，
第二层再问 `0..0` 的空锚点时也拿到 `1..6`——细粒度边界被并集化抹平。这就是 F-03。

`pre-fix-tests.txt`（exit 101）记录两条新用例先失败：
`compose_with_identity_rewrite_preserves_every_first_level_segment` 得到
`left [(0..3, 0..4, Synthetic)]`（三片被并成一片）、
`compose_pairing_rules_for_empty_ranges_are_explicit` 得到 `left [(0..1, 0..1, Aligned)]`（丢了隐藏片）。

## 2. 复合算法

不再对整段调 `to_source`，改为**按两层片段边界求交**，逐片保留第一层来源。空区间规则显式化：

- 第一层 `rewritten` 为空的隐藏片段：锚点落在第二层哪个片段，就随该片段投影到第二副本对应点；
  未被第二层覆盖则丢弃。
- 第二层 `source` 为空的合成片段：锚定到第一层片段在源码中的对应点，合成内容不伪造非空来源。
- 第二层片段非 `Exact` 且 `rewritten` 非空时，第二层无法表达逐字节对应，该片段内部交集
  合并为一片：来源取并集，精确性取较强者。
- 第二层 `rewritten` 为空（隐藏片段）时，逐片保留第一层来源，不合并。

新增辅助：`middle_anchor`（空来源锚定）、`middle_owner`（第二副本点归属）、
`rewritten_point`（源码点→第二副本点，区分 `middle.start` / `middle.end`）、
`project_source` / `project_rewritten` / `source_point`（区间与点投影）。
排序键 `(rewritten, rank, 边界点)`：`rank 0` 是零宽锚点片、`rank 1` 是内容片，
保证同起点时锚点在内容之前，重写端单调。

## 3. 逐条断言对照

| 目标 | 证据 |
|---|---|
| 恒等复合保留每片 | `compose_with_identity_rewrite_preserves_every_first_level_segment`：三片逐项相同，`rewritten_len=4` |
| Exact / Aligned / Hidden / Synthetic 正确 | `compose_pairing_rules_for_empty_ranges_are_explicit`、`compose_anchors_second_level_empty_sources_without_union` |
| 跨片段来源 / 锚点正确 | 上两条共 14 条 `to_source` 断言（端点、跨片并集、空锚点） |
| 隐藏片不并入非 Exact 合并片 | `compose_keeps_hidden_anchor_outside_non_exact_merged_piece`：复合 = `[(0..3,0..1,Aligned),(1..2,1..1,Hidden)]` |

## 4. 既有断言冲突（已按主 Agent 裁决处理）

`maps_compose_through_two_rewrite_levels` 的 `to_source(0..0)` 原断言 `1..6` 改为 `1..3 Aligned`
并加注释；`to_source(0..4) → 1..6 Aligned` **未动**。

理由：旧的 `1..6` 只有把 `"X"` 所在的 `Aligned 1..3` 与后续 `Exact 3..6` **并成一片**才可能，
那正是 F-03 的并集化行为；与「恒等复合保留每片」不可兼得。这不是放宽断言，是纠正一条
把缺陷固化成期望的旧断言。另一条既有用例 `control_bytes_do_not_collide_with_synthetic_markers`
未改动。

## 5. 主 Agent 追问的两点

1. **合并片 `rewritten` 取整段 `next_segment.rewritten` 是否过宽**：第一层片段由
   `apply_with_cancel` 连续铺满 `[0, rewritten_len)`（gap 也补 Exact 段），无空洞；
   空 `rewritten` 片长度为 0，不覆盖非空 `next.source` 的字节，故交集必然铺满 `next.source`，
   整段重写区间与交集一致。若将来出现空洞路径，精度已是 `Aligned`，不伪装逐字节对应。
2. **第一层隐藏空片落在第二层非 Exact 片内是否重叠 / 次序错乱**：不会。隐藏片先 `continue`，
   不并入并集，只产出零宽锚点片；非 Exact 片段上 `rewritten_point` 只返回片段起点或终点，
   不返回内部点，故与合并片无共享字节；`middle_owner` 保证同一隐藏点只归属唯一第二层片段。

**主 Agent 复核备注**：上述第 2 点的「无共享字节」只针对**重写端**。源码端该用例的合并片
声明 `0..3`、隐藏片声明 `1..2`，二者在源码上**确实重叠**。我核过 `to_source(0..1) → 0..3 Aligned`
与 `to_source(1..1) → 1..2 Hidden` 在该结构下语义自洽（合并片给全跨度并集，零宽片标记被隐藏的
子区间），且 `RewriteMap` 并未声明「片段源码互不重叠」的不变式。但「语义自洽」不等于
「这是应然设计」——该点留给独立 reviewer 裁决，不由实现方或主 Agent 定案。

## 6. 验证

- 终版重跑（`post-fix-tests.txt` 的 FINAL CODE RERUN 段，`04:16:57Z`）：exit 0，全 crate
  **112 passed / 0 failed**，`rewrite` **13/13** 全绿。
- 指纹与代码一致：`rewrite.rs` `ef9b46ac…bd4bc397` / mtime `12:12:44`；
  `tests/rewrite.rs` `767895fe…1f5e7e15` / mtime `12:16:39`。
- `cargo fmt -p aline-markdown -- --check` exit 0；`cargo clippy -p aline-markdown --all-targets
  --locked --offline -- -D warnings` exit 0（均 `04:17:03Z`）。
- 生产代码无 `unwrap(` / `expect(` / `panic!` / `unreachable!` / `assert!`；测试无 `.unwrap()` / `.expect(`。

## 7. 剩余问题

- 未跑 workspace 全量门禁（其他 crate 正被并行 worker 修改），R14 统一验证时重跑。
- 空范围查询的「可见片段优先」是既有 `anchor_to_source` 语义，本次未改动，已在测试注释中标注。
- 源码端重叠结构（见 §5 复核备注）待独立 reviewer 裁决。
