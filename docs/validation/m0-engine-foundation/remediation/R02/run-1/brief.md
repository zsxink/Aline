# R02 派发契约：F-03 RewriteMap 多阶段来源复合

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `<repo>`。

## 文件所有权（只有你可写这些文件）

- `crates/aline-markdown/src/pre/rewrite.rs`
- `crates/aline-markdown/tests/rewrite.rs`
- `docs/validation/m0-engine-foundation/remediation/R02/run-1/` 下你自己新建的证据文件

不得写其它文件。不得 `git reset`、不得回退他人改动。另一个 worker 正在并行改 `crates/aline-ui/**`，那不是你的范围。

## 失败（必须先写出失败回归，再修）

**F-03（阻断）**：`abc` 在偏移 1 插入 Synthetic `!` 得到 `a!bc`，再对 `a!bc` 复合 identity。原实测：第一层三片 `Exact 0..1 / Synthetic 1..1(空来源) / Exact 1..3` 被压成单片 `source 0..3 → rewritten 0..4 Synthetic`；`b` 从 `Exact 1..2` 变成 `Synthetic 0..3`。

原复核复现源（可原样用作回归起点）：

```rust
use aline_markdown::{Rewrite,RewriteEdit,ParseBudget,MappingPrecision};
fn main() -> Result<(),Box<dyn std::error::Error>> {
 let first=Rewrite::apply("abc",&[RewriteEdit::new(1..1,"!".into(),MappingPrecision::Synthetic)?],ParseBudget::m0())?;
 let identity=Rewrite::apply(first.text(),&[],ParseBudget::m0())?;
 let combined=first.map().compose(identity.map())?;
 println!("first={:?}",first.map().segments());
 println!("composed_identity={:?}",combined.segments());
 println!("b_before={:?}",first.map().to_source(2..3)?);
 println!("b_after={:?}",combined.to_source(2..3)?);
 Ok(())
}
```

## 现行契约

- `RewriteMap::compose(&self, next)` 的既有文档语义：`next` 把第二副本映射到第一副本，`self` 把第一副本映射回源码；返回值仍是「rewritten → source」映射，`rewritten_len` 取 `next.rewritten_len()`。
- `RewriteSegment`：`source` 为空表示合成内容（Synthetic），`rewritten` 为空表示隐藏源码（Hidden）；`Exact` 逐字节平移，`Aligned` 表示整段来源、无逐字节对应。
- 现有测试 `maps_compose_through_two_rewrite_levels` 与 `control_bytes_do_not_collide_with_synthetic_markers` 是有效既有行为，不得为通过新测试而改其断言（除非你能证明原断言与契约冲突，并在 handoff 里单列证据与理由）。
- `combine_precision` 的既有 rank 顺序（Exact < Aligned < Hidden < Synthetic，取较强者）是本轮已确认的语义，不要改。

## 要求

1. 先写失败回归：恒等复合必须保留每一片（片段数量、每片的 `source`/`rewritten` 区间、每片 `precision` 逐项比较），并单独断言 `to_source` 在跨片段范围、合成空来源锚点、各合法端点上的结果。预期必须由你按片段边界独立推导写死成常量，**不得调用 `compose`/同一实现来生成预期**。
2. 再实施最小修复：按两层片段边界求交集复合，而不是对每片调用会并集化的 `to_source`。注意空区间片段（Synthetic 的空 source、Hidden 的空 rewritten）的配对规则要显式定义并测试。
3. 不得新增 `unwrap()/expect()/panic!/unreachable!/assert!` 到生产代码；测试也不得 `unwrap()`。
4. 全部 Cargo 命令用 `--locked --offline`，`CARGO_NET_OFFLINE=true`，工具链 1.98.1。工具缺失/离线失败记「未核实」，不要联网安装。
5. 提交（Conventional Commits，scope 用 `aline-markdown`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R02/run-1/`）

- `pre-fix-tests.txt`（失败回归原始输出，确认先失败）
- `post-fix-tests.txt`（修复后 `cargo test -p aline-markdown` 全量）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（根因、复合算法、逐条断言对照、剩余问题）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、一行测试小结、concerns、证据路径。不要把长日志贴回来。
