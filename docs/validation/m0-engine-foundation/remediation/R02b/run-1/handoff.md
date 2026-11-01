# R02b handoff

- 批次：R02b / run-1（F-01 残留：第二层空来源片获非空 source）
- 状态：**待独立复核**（由主 Agent 实现，作者不得自任复核）
- 基线 HEAD：`3b10ac042d102b155adb841b0b726ff5db970f79`
- 工具链：cargo/rustc 1.98.1，`CARGO_NET_OFFLINE=true`

## 代码改动（两文件）

- `crates/aline-markdown/src/pre/rewrite.rs`
  - 仅改 `source_point` 的非 `Exact` 分支：`Ok(segment.source.clone())` →
    端点零宽锚点 `anchor..anchor`（点在片起点取 `source.start`，否则 `source.end`）；
    文档注释补明该约束与 `compose` 的「合成内容不伪造非空来源」关系。
  - sha256 `aa730fb81caa167826de781cc896676e8bd489c225427a76e109a3dd57c2f522`
- `crates/aline-markdown/tests/rewrite.rs`
  - 新增 `compose_zero_source_pieces_anchor_to_empty_points_only`（+133 行）：
    Aligned 起点 / Aligned 内部 / 第一层 Hidden 三处锚定，期望值手写；
    并断言三份复合结果里所有 `Synthetic` 片段 source 为空。
  - sha256 `c6387fec712ca19dac66782adcbadba7604192d908655bf4553e76da5ada35d2`

## 证据

`docs/validation/m0-engine-foundation/remediation/R02b/run-1/`

- `brief.md`（含 R02 复核 §E 原文引文）
- `pre-fix.txt` + `pre-fix-probe.rs`（exit 101，`SYNTHETIC@1 = [(1..3, 1..2)]`）
- `post-fix.txt`（§A–§H，各段起止 UTC/argv/环境/退出码/原始输出；末尾 `PASSED_TOTAL` 勘误）
- `commands.txt`、`report.md`、本文件

## 一行测试小结

`cargo test -p aline-markdown` 12 目标全 ok，**126 passed / 0 failed**；
`rewrite` 目标 14/14；`clippy -D warnings` exit 0；`fmt --check` exit 0。

## 用例有效性

把 `src/pre/rewrite.rs` 临时还原为 HEAD 版（`ef9b46ac…`），同一终版用例 exit 101，
失败行正是 `(1..3, 1..2, Synthetic)` vs `(1..1, 1..2, Synthetic)`；取证后还原并核 sha256 一致。

## concerns（交复核者裁决）

1. **端点方向是既有约定而非新语义**：复用 `rewritten_point` 的「起点取 start、否则取 end」。
   若复核认为 Aligned 内部点应锚到 `source.start`，属语义变更，须另行裁决——本批未擅自改。
2. **源码端零宽片与合并片可重叠**：`Aligned 内部` 用例的复合结果含
   `(1..3, 1..2, Aligned)` 与 `(3..3, 2..3, Synthetic)`，以及 `(1..3, 1..2)`/`(1..3, 3..4)`
   两片声明同一源码区间——这是 R02 已裁决的「无源码互不重叠不变式」的延续，本批未改变它。
3. 第一层为 `Synthetic` 时无行为差异（其 source 本就为零宽），故未单独造三层夹具。
4. 未跑 workspace 全量门禁（留 R14）；`compose` 无生产调用者，影响面仅测试。
5. 本批修复**不构成** M0 出口通过，V-01～V-09 不因此升级。
