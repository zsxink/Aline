# R02 handoff

- 批次：R02 / run-1（F-03 RewriteMap 多阶段来源复合）
- 状态：DONE（未提交，按协调者要求停手，由主 Agent 串行提交）
- 基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`

## 代码改动

- `crates/aline-markdown/src/pre/rewrite.rs`
  - sha256 `ef9b46acc686d6a3a0fdc87f8be8794ca62225eb452a9997c62b795bbd4bc397`
  - mtime `2026-11-01T12:12:44`（本地时区）
- `crates/aline-markdown/tests/rewrite.rs`
  - sha256 `767895fe403ee582954ca663c859c2ac0da16c71edf83a1f2de2f5361f5e7e15`
  - mtime `2026-11-01T12:16:39`

## 证据

`docs/validation/m0-engine-foundation/remediation/R02/run-1/`

- `brief.md`、`pre-fix-tests.txt`（exit 101，9 passed / 2 failed）、`post-fix-tests.txt`、`commands.txt`、`report.md`、本文件

## 一行测试小结

`cargo test -p aline-markdown --test rewrite --locked --offline` → 13 passed / 0 failed；
全 crate 112 passed / 0 failed；`fmt --check` exit 0；`clippy -D warnings` exit 0。

## 结论

`compose` 从「对每个第二层片段调 `to_source`（会把第一层片段并成一片）」改为「按两层片段边界
求交、逐片保留第一层来源」，空区间规则显式化（隐藏片锚定、合成片锚定、非 Exact 片内合并）。
F-03 的并集化抹平已消除。

## concerns

1. 报告类 `.md` 被工具策略禁止由 worker 落盘，`report.md` / 本文件由主 Agent 补齐。
2. 既有断言 `maps_compose_through_two_rewrite_levels` 的 `to_source(0..0)` 由 `1..6` 改为
   `1..3 Aligned`——旧值只有并集化才可能，属纠正固化了缺陷的断言，非放宽。详见 `report.md` §4。
3. 合并片在**源码端**可与其内隐藏零宽片声明区间重叠（`compose_keeps_hidden_anchor_outside_non_exact_merged_piece`：
   `0..3` 与 `1..2`）。`RewriteMap` 未声明源码互不重叠不变式，语义自洽，但应然性留给独立 reviewer 裁决。
4. 未跑 workspace 全量门禁（并行 worker 正在改其他 crate），留待 R14。
5. 空范围查询「可见片段优先」是既有 `anchor_to_source` 语义，本次未改。
