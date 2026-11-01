# R02b 派发契约：F-01 残留（空来源合成片获非空 source）

- 批次：R02b / run-1（R02 复核移交的既有缺陷，须先于 R04/R10）
- 基线 HEAD：`3b10ac042d102b155adb841b0b726ff5db970f79`
- 仓库根：`/Users/xian/Project/editor/Aline`；工具链 1.98.1，`CARGO_NET_OFFLINE=true`
- 实现者：主 Agent（无空闲 worker；并发上限内已有一个复核 agent 在跑）
- 审查要求：**必须由未参与本批实现的 reviewer 复核**（主 Agent 是作者，不能自任复核）

## 1. 问题（R02 复核 §E 原文移交）

> **[严重] F-01 残留（既有缺陷，非 R02 引入）**：第二层空来源（`Synthetic`）片段锚定到第一层
> `Aligned`/`Hidden` 片段时，返回**非空 source**，与 `rewrite.rs:188-193` 文档「合成内容不伪造
> 非空来源」冲突；`middle_anchor`（`:306-323`）与 `source_point`（`:627-644`）是成因。

复核实测（`R02/run-1/probe/f01-synthetic-nonempty-source.log`）：

- 第一层 `[(0..1,0..1,Exact),(1..3,1..3,Aligned),(3..6,3..6,Exact)]`
- 第二层在副本点 1 插入零来源 `Synthetic` → 复合含 `(1..3, 1..2, Synthetic)`（应为 `1..1`）
- 点 2（落在 Aligned 片内部）→ 复合含 `(1..3, 2..3, Synthetic)`

## 2. 文件所有权

- `crates/aline-markdown/src/pre/rewrite.rs`
- `crates/aline-markdown/tests/rewrite.rs`
- `docs/validation/m0-engine-foundation/remediation/R02b/run-1/` 下本批新建的证据文件

不得写其它文件；不得 `git reset`；不得回退他人改动。

## 3. 必须满足

1. 先落**修复前失败**证据（探针原始输出 + 新回归用例在旧源码上失败）。
2. 修复后，第二层空来源片段锚定到**任何**第一层精确性（Exact / Aligned / Hidden / Synthetic）
   都返回**零宽**源码锚点。
3. 不得改变 `Exact` 的逐字节平移语义；不得改变非空第二层片段的复合结果。
4. 新增回归用例覆盖 Aligned 起点、Aligned 内部、Hidden 三种锚定位置，期望值手写。
5. 全 crate 测试、clippy `-D warnings`、fmt `--check` 均 exit 0；生产代码不新增
   `unwrap/expect/panic!/unreachable!/assert!`。
6. 证据含 argv/cwd/环境/起止 UTC/退出码/原始 stdout-stderr、输入与代码身份（sha256）。

## 4. 不在本批范围

- 不处理 R02 复核的 F-03（`compose` 无取消/预算）与 F-04（空 map 边界）。
- 不跑 workspace 全量门禁（留 R14）。
- 不改 `anchor_to_source` 的「可见片段优先」语义。
- 不因本批修复宣称 M0 出口或 V-01～V-09 通过。
