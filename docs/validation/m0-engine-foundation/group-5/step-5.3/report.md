# 5.3 记录：锚点 affinity、选区变换与低层边界审计

日期：2026-10-31。状态：`crates/aline-buffer/tests/anchors.rs` 的 5 个集成测试通过；只读审计显示 `aline-buffer` 不解析、不写盘、不使用 `unsafe`/`libc`/哈希库，也无依赖。本文不宣称文档坐标与存储坐标已统一审计，不宣称 V-07 通过。

## 1. 锚点与选区语义

测试覆盖：

- 替换 `aXYZb` 的 `1..4→Q`：替换外偏移按长度差平移；替换起点 Before→1、After→2；替换内部 2/3 失效；末尾 5→3。
- 插入 `abcd` 的 `2..2→X`：位置 2 的 Before→2、After→3；位置 3→4、4→5，映射结果必须是 after 的 UTF-8 边界。
- 同起点“插入 + 替换”`2..2→X`、`2..4→Y`：after 为 `abXY`；位置 2 Before→2、After→4；位置 3 失效；位置 4→4，证明按累计 delta 而不是逐补丁偶然顺序计算。
- 多补丁 `1..3→AB`、`6..8→""`：前缀/中段/删除点/末尾按累计 delta 平移，删除内部失效。
- 方向性选区：anchor/head 反向时保持方向；任一端点失效则整个变换返回 `None`，不做部分有效结果。
- 外来快照锚点或过期批次返回 `SnapshotMismatch`。

实现入口为 `transform_anchor`/`transform_selection`（`crates/aline-buffer/src/anchors.rs`），公开类型 `AnchorTransform`、`SelectionTransform`、`Affinity` 由 `lib.rs` re-export。

## 2. 操作来源与单调 revision

操作来源和单调 revision 的行为由 5.4 的 `history_records_source_selections_and_monotonic_revisions` 验证：历史步骤记录 `Typing`/`Replace` 来源、before/after 选区和 before/after revision；apply→undo→redo→undo→redo 的 revision 严格递增，Undo/Redo 不回退代次。5.3 的锚点测试不重复断言这两项，避免把测试名称当作覆盖。

## 3. 低层边界只读审计

脚本 `run-audit.sh` 只读扫描源码、manifest、`cargo tree`、所有权声明和发布顺序，日志为 `audit.stdout.log` / `audit.stderr.log`，退出 0。

| 检查 | 实际结果 |
|---|---|
| I/O 与 `unsafe`/`libc` | `std::fs`、`std::io`、`File::`、`OpenOptions`、`read_to_`、`write_all`、`unsafe`、`libc` 均无命中 |
| 解析依赖/符号 | 仅 `text.rs:149` 文档注释出现“markdown”字样；无 `pulldown`/`comrak`/`serde` |
| 哈希/指纹 | 无 `DefaultHasher`、`std::hash`、`Hasher`、`sha*`、`md5`、`blake` 命中 |
| 依赖清单 | `crates/aline-buffer/Cargo.toml` 无 `[dependencies]`；`cargo tree -p aline-buffer --edges normal --locked --offline` 只列自身 |
| 唯一持有 | `StringBuffer` 私有字段为 `snapshot`、`selection`、`line_index`、`history`；没有第二份文本或历史容器 |
| 共享点 | `HistoryStep` 的正/逆补丁为 `Arc<Vec<...>>`，apply 构造后以 `Arc::clone` 共享给结果与历史；`HistoryStepHandle` 持有 `Arc<HistoryStep>` |
| 发布顺序 | apply/undo/redo/reload 均先构造 after 文本/索引/步骤并调用 `prepare_record`，再赋值 `snapshot`，最后提交历史 |

结论限于 `aline-buffer` 自身：它不解析 Markdown、不写盘、不创建线程/计时器。审计没有检查 UI/编辑层是否会误用这些原语，也没有做跨 crate 坐标混用审计。

## 4. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T06:24:21Z | `cargo test -p aline-buffer --locked --offline anchors` | 0 | `targeted-tests/anchors.stdout.log` / `.stderr.log` |
| 2026-10-31T06:24:22Z | `sh step-5.3/run-audit.sh` | 0 | `audit.stdout.log` / `audit.stderr.log` |
| 2026-10-31T07:09:07Z | `sh step-5.3/run-audit.sh`（复核 F3-3 后 `set -eu`） | 0 | `audit-after-f3-3.stdout.log` / `.stderr.log` |

锚点过滤运行 5 个测试，`5 passed; 0 failed`；同一过滤还列出其他 target 的 0 匹配结果，日志保留原样。复核指出原脚本仅 `set -u`，关键命令失败可能被后续输出掩盖；本次只把脚本改为 `set -eu` 并以新日志重跑，未改写原日志，也未修改 buffer 代码。新日志退出 0、stderr 为空；结果与 06:24:22Z 的审计一致。

## 5. 未覆盖与边界

- 未做属性测试或随机锚点压力测试。
- 未验证跨层 `SrcOffset`/`StorageRange`/UTF-16 偏移混用；本审计只覆盖 buffer crate。
- 未验证 UI 实际使用 affinity 的策略，也未验证 IME 组合期锚点稳定性。
- 未运行其他 OS/target；当前证据只代表 macOS arm64。
- 未验证真实性能或大历史下的 `Arc` 生命周期开销。

## 6. 独立复核

本节不写复核结论。独立复核者应检查：锚点期望是否独立于实现、同点插入/替换的累计 delta 是否算对、失效判断是否覆盖端点、审计脚本是否只读且扫描范围足够、`Arc` 共享是否确实避免复制补丁文本。
