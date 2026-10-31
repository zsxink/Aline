# 6.3 记录：块/Gap 分区与无头 dump

日期：2026-10-31。状态：`dump_tree` 示例与块/Gap 金样测试通过。本文只记录无头分区语义与人工审查结果，不宣称 UI 渲染、V-07、AC-09 或 M0 出口通过。

## 1. 分区语义

- 顶层块与 Gap 必须无缝覆盖源码：`block.start..block.end` 与 `gap.start..gap.end` 交替且无重叠、无空洞。
- 块尾不含块后的分隔换行；分隔换行全部计入 Gap，Gap 内容只允许 `\r\n`/`\n` 空白。
- 空文档零块零 Gap。
- 连续多空行、前导/尾随空行都计入 Gap。

## 2. dump 入口

示例 `crates/aline-markdown/examples/dump_tree.rs` 按任务要求以 `cargo run -p aline-markdown --example dump_tree -- <file>` 调用，输出配置代次、features 位、源/绘制字节数、每个块与 Gap、保护范围与诊断。示例不调用 `unwrap`/`expect`/`panic`。

`basic.md` 的实际输出（`step-6.3/dump-tree.stdout.log`，stderr 为空）：

```
config 1 features 0x00000c1e
source 319
drawn 197
block 0..19 FrontMatter
gap 19..21
block 21..29 Heading
gap 29..31
block 31..118 Paragraph
gap 118..120
block 120..141 BlockQuote
gap 141..143
block 143..200 CodeBlock
gap 200..202
block 202..237 Table
gap 237..239
block 239..258 Paragraph
gap 258..260
block 260..278 FootnoteDefinition
gap 278..280
block 280..294 Math
gap 294..296
block 296..319 Paragraph
protected 0..19 FrontMatter
protected 251..255 FootnoteReference
protected 260..278 FootnoteDefinition
```

10 个块、9 个 Gap，区间连续覆盖 `0..319`（`basic.md` 的字节数），块尾与 Gap 边界不重叠。

## 3. 人工对独立金样审查

人工把上述 dump 与 `crates/aline-markdown/tests/fixtures/m0/basic.blocks.tsv`（实现前提交的手工金样）逐行比对：块种类、起止字节、Gap 位置全部一致。金样本身由 `python3` 逐行读取原始字节产生，不依赖 Rust 解析器；`verify_goldens.py` 再用纯标准库复算，输出 0 个问题（`step-6.8/goldens-independent.stdout.log`）。

需要说明的边界：

- 顶层 `Math` 块（`280..294`）是 M0 的块分类，不代表已实现公式渲染。
- `basic.md` 无尾换行，因此最后一个块 `296..319` 后没有 Gap。
- `protected 251..255 FootnoteReference` 是脚注引用的子区间，未扩到相邻正文；这证明“脚注引用与普通正文相邻时仅保护子区间”。

## 4. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:13Z | `cargo run -q -p aline-markdown --locked --offline --example dump_tree -- crates/aline-markdown/tests/fixtures/m0/basic.md` | 0 | `step-6.3/dump-tree.stdout.log` / `.stderr.log` |
| 2026-10-31T08:48:11Z | `cargo test -p aline-markdown --locked --offline --test spec` | 0 | `step-6.1/targeted-tests/spec.stdout.log` |

## 5. 未覆盖

- dump 是调试入口，不是公开 API；格式稳定只服务于本组证据，不作产品契约。
- 未渲染、未布局、未测量命中；这些属 V-07 与 UI 层。
- 仅一个夹具做人工 dump 审查；其余 6 个夹具靠 `spec.blocks.tsv` 与测试断言覆盖。
