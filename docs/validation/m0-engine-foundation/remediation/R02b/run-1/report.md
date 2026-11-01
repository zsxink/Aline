# R02b run-1 报告：F-01 残留（空来源合成片获非空 source）

基线 HEAD：`3b10ac042d102b155adb841b0b726ff5db970f79`。工具链 1.98.1，`CARGO_NET_OFFLINE=true`。
所有权范围：`crates/aline-markdown/src/pre/rewrite.rs`、`crates/aline-markdown/tests/rewrite.rs`、本批次证据目录。
实现者：主 Agent。**本批未经独立复核前不得标为完成**（作者不能自任复核）。

## 1. 缺陷事实（来自 R02 复核 §E，本批独立复现）

`RewriteMap::compose` 文档承诺「合成内容不伪造非空来源」（`rewrite.rs:188-193`），但第二层
**空来源**片段锚定到第一层 **Aligned / Hidden** 片段时，`middle_anchor` → `source_point`
返回的是该片段的**整段来源**：

```rust
// 修复前
MappingPrecision::Aligned | MappingPrecision::Hidden | MappingPrecision::Synthetic => {
    Ok(segment.source.clone())
}
```

于是复合结果里出现 `(1..3, 1..2, Synthetic)` —— 一个 2 字节的合成内容声称占据 2 字节源码。

`pre-fix.txt`（exit 101）是归档探针在修复前源码（`rewrite.rs` sha256 `ef9b46ac…bd4bc397`）上的
原始输出，与 R02 复核 `probe/f01-synthetic-nonempty-source.log` 的 `COMPOSED@1` 逐元组一致。

## 2. 根因与修复

`source_point` 有两个调用点，都在 `middle_anchor`（`rewrite.rs:313`、`:320`），只服务**空来源**
第二层片段。改为对非 `Exact` 片段返回**零宽端点锚点**：

```rust
MappingPrecision::Aligned | MappingPrecision::Hidden | MappingPrecision::Synthetic => {
    let anchor = if point == segment.rewritten.start {
        segment.source.start
    } else {
        segment.source.end
    };
    Ok(anchor..anchor)
}
```

端点方向沿用同文件 `rewritten_point`（`:595-624`）的既有约定：点在片段起点取 `start`，
否则取 `end`，以保持隐藏来源与相邻内容的先后关系。`Exact` 分支逐字节平移、本来就返回
零宽点，未改动。

修复后同一份探针（未改一字）输出（`post-fix.txt` §B）：

| 锚定位置 | 修复前 | 修复后 |
|---|---|---|
| Aligned 片起点（副本点 1） | `SYNTHETIC@1 = [(1..3, 1..2)]` | `[(1..1, 1..2)]` |
| Aligned 片内部（副本点 2） | 非空（复核 log） | `[(3..3, 2..3)]` |
| 第一层 Hidden 片终点 | 未覆盖 | `[(1..1, 1..2)]` |

## 3. 新增回归用例

`crates/aline-markdown/tests/rewrite.rs::compose_zero_source_pieces_anchor_to_empty_points_only`
（14 条 `to_source`/片段断言 + 三处全覆盖的不变式循环）：

| 场景 | 断言 |
|---|---|
| Aligned 起点 | 复合 = `[(0..1,0..1,Exact),(1..1,1..2,Synthetic),(1..3,2..4,Aligned),(3..6,4..7,Exact)]`；`to_source(1..2) → 1..1 Synthetic` |
| Aligned 内部 | 复合 = `[(0..1,0..1,Exact),(1..3,1..2,Aligned),(3..3,2..3,Synthetic),(1..3,3..4,Aligned),(3..6,4..7,Exact)]`；`to_source(2..3) → 3..3 Synthetic` |
| 第一层 Hidden | 复合 = `[(0..1,0..1,Exact),(1..1,1..2,Synthetic)]`；`to_source(1..2) → 1..1 Synthetic` |
| 全局不变式 | 三份复合结果里**所有** `Synthetic` 片段 `source_range().is_empty()` |

期望值全部手写常量，来源是探针原始输出与 `middle_anchor`/`source_point` 的端点约定，不是抄实现。

**用例有效性证据**（`post-fix.txt` §F/§H1）：把 `src/pre/rewrite.rs` 临时还原为 HEAD 版
（sha256 `ef9b46ac…`），同一终版用例 exit 101，失败信息正是
`left [(0..1,0..1,Exact),(1..3,1..2,Synthetic),…]` vs `right [(0..1,0..1,Exact),(1..1,1..2,Synthetic),…]`。
取证后立即还原并核 sha256 `aa730fb8…d57c2f522`，与 §A 一致。

## 4. 对 Requirement / Scenario 与消费者的影响

- 影响面：`RewriteMap::compose` 的**空来源**分支。`compose` 在生产代码中**无调用者**
  （`grep '.compose('` 只命中 `tests/rewrite.rs`），故不影响 `parse` / `build` / dump。
- 与 R02 已交付语义一致：非空第二层片段、`Exact` 平移、隐藏片不并入合并片的规则均未改动；
  R02 的 13 条既有用例全部继续通过（本批终版 `rewrite` 目标 14/14 全绿）。
- 不改 `anchor_to_source` 的「可见片段优先」语义（R02 已在测试注释中标注，本批未触及）。
- **不构成 M0 验收通过**：V-01～V-09 未因此轮修复升级为通过。

## 5. 验证（主 Agent 实测）

| 项 | 结果 | 位置 |
|---|---|---|
| 归档探针修复前 | exit 101，`SYNTHETIC@1 = [(1..3, 1..2)]` | `pre-fix.txt` |
| 归档探针修复后（原样重跑） | exit 0，三处全空区间 | `post-fix.txt` §B |
| 终版用例 + 修复前源码 | exit 101 | `post-fix.txt` §F、§H1 |
| 全 crate 测试（终版） | exit 0，12 目标全 ok，**126 passed / 0 failed** | `post-fix.txt` §H2 |
| `clippy --all-targets -D warnings` | exit 0 | `post-fix.txt` §H3 |
| `fmt --check` | exit 0 | `post-fix.txt` §H3 |
| 生产代码 `unwrap/expect/panic!/unreachable!/assert!` | 未新增 | 见 §2 diff |

代码身份（终版）：`src/pre/rewrite.rs` `aa730fb81caa167826de781cc896676e8bd489c225427a76e109a3dd57c2f522`；
`tests/rewrite.rs` `c6387fec712ca19dac66782adcbadba7604192d908655bf4553e76da5ada35d2`。

## 6. 未覆盖 / 残留

- 未跑 workspace 全量门禁（留 R14）。
- 第二层空来源片段锚定到第一层 **Synthetic** 片段的分支：第一层 Synthetic 片的 `source`
  按定义为空，修复前后都返回 `x..x`（`source.start == source.end`），**无行为差异**，
  故未为其单独构造夹具。若要覆盖需三层叠加（`apply` 只在「空来源 + 非空替换」产 Synthetic），
  本批未做，登记为已知未覆盖但无风险的分支。
- 未处理 R02 复核的 F-03（`compose` 无取消/预算）与 F-04（空 map 边界），二者仍登记在 R02。
- 本报告由实现者（主 Agent）撰写，不得作为独立复核结论。

## 7. 证据清单

- `brief.md` — 派发契约（含 R02 复核 §E 原文引文）
- `pre-fix.txt` — 修复前探针原始输出（exit 101）
- `pre-fix-probe.rs` — 探针源码（取证后已从 `tests/` 删除）
- `post-fix.txt` — §A 指纹、§B 修复后探针、§C 全包、§D clippy、§E fmt、§F/§H1 前源码失败、
  §G/§H2 终版重跑、§H3 clippy/fmt，含各段起止 UTC 与退出码；末尾含 `PASSED_TOTAL` 勘误
- `commands.txt` — 命令、环境与身份记录
- `report.md` — 本文件
- `handoff.md` — 交接摘要
