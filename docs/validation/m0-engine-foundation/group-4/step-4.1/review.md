# 任务 4.1 独立复核记录

复核日期：2026-10-31（UTC 02:42～02:55 左右）。复核者为独立只读 Explore 子代理（agent `a5e0d6aae17d38dac`，68 次工具调用，约 758 秒），未修改任何文件。复核基线 HEAD `6731ff2d57e78916e659ea95e517ea598053501b`，工作区包含本任务尚未提交的实现、测试、金样与证据。

复核结论：阻塞 0 项、应修 2 项、建议 5 项。应修第 1 项是规范/设计文本与实现的 UTF-16 口径差异；用户于 2026-10-31 裁决选择收窄规范为“带 UTF-16 BOM”，该项已落实。其余各项已处理。

## 1. 逐项处置

| # | 级别 | 发现 | 处置 | 证据 |
|---|---|---|---|---|
| 1 | 应修（已裁决 A） | change spec 的“非法与超限拒收”场景写“输入为 UTF-16 … 返回可区别的编码/预算错误”（`openspec/changes/m0-engine-foundation/specs/engine/source/spec.md:20`），引擎 §5.1 写“UTF-16、未知编码、截断多字节序列返回 DecodeError”（`docs/engine-design.md:103`）；实现只拒绝以 `FF FE`/`FE FF` 开头的 UTF-16 BOM，无 BOM 且字节本身是有效 UTF-8 的 UTF-16LE（如 `41 00 42 00`）会被接受为源码 `"A\0B\0"`。任务 4.1 原文只要求“UTF-16 BOM”。 | 用户裁决 A：把 change spec 场景与引擎 §5.1 收窄为“以 UTF-16 BOM 开头”，与任务原文和实现一致；新增 `utf16le-no-bom-valid-utf8` 金样固定无 BOM 的有效 UTF-8 字节按 UTF-8 接受；报告 §5 记录该限制。 | `openspec/changes/m0-engine-foundation/specs/engine/source/spec.md:20`；`docs/engine-design.md:103`；`decode-golden.v1.txt`；`report.md` §5 |
| 2 | 应修 | 金样测试只拒绝零用例，不绑定 31 用例集合；错误行未用列可自相矛盾而静默通过。 | 已修：加载时断言总数 31、拒绝重复名称；err 行的源码/BOM/行尾计数/行尾策略/未改 bytes 列必须为空或规范零值；`unchanged_encode == input` 的一致性检查移入加载阶段并注明不是 encode 证据。 | `crates/aline-source/tests/decode_golden.rs:17`、`:65`、`:68`、`:76`、`:124` |
| 3 | 建议 | `valid_up_to` 字段文档写“首个非法字节的位置”，但截断输入的偏移是不完整序列起点（lead byte 本身合法）。 | 已修：字段文档改为“第一个无法构成有效 UTF-8 的字节位置；截断时指向不完整序列的起点”。 | `crates/aline-source/src/decode.rs:68` |
| 4 | 建议 | 报告称 31 个用例，但覆盖分组表只列 30 个名称，漏 `bom-with-crlf`。 | 已修：把 `bom-with-crlf` 加入“行尾表示”组。 | `report.md:27` |
| 5 | 建议 | `unchanged_encode == input` 是金样自洽断言，不是 encode 证据；留在主测试消息中可能误读。 | 已修：移入 `load_cases`，错误消息明确“金样内部一致性…这不是 encode 证据”；报告 §2 同步说明。 | `decode_golden.rs:68`；`report.md` §2 |
| 6 | 建议 | 两层 `StorageBudget` 未在证据中说明和覆盖：构造期超限返回 `StorageError`，decode 期返回 `DecodeError`；金样快照用 `usize::MAX`，未覆盖构造期超限。 | 已修：新增 `snapshot_and_decode_budgets_are_distinct_layers`，覆盖构造期超限、等号边界与 decode 期超限；报告 §3 说明两层职责。 | `decode_golden.rs:326`；`report.md` §3 |
| 7 | 建议 | 统一入口的 `public-api` 项只是夹具 self-test；报告未像 3.3 那样限定。 | 已修：报告注明“夹具的正/反例编译 self-test，不是对仓库实际公开面的扫描”。 | `report.md:82` |

## 2. UTF-16 裁决与落实

原始冲突：

- 任务 4.1 原文：`openspec/changes/m0-engine-foundation/tasks.md:39` 只列“UTF-16 BOM”。
- 原 change spec：`openspec/changes/m0-engine-foundation/specs/engine/source/spec.md:20` 写“输入为 UTF-16、非法或截断 UTF-8，或超过注入的输入预算 … 返回可区别的编码/预算错误”。
- 原引擎规范：`docs/engine-design.md:103` 写“UTF-16、未知编码、截断多字节序列返回 DecodeError”。
- 实现：`crates/aline-source/src/decode.rs:122`～`:131` 只按 `FF FE`/`FE FF` 前缀返回 `Utf16Bom`；无 BOM 的 UTF-16LE `41 00 42 00` 是有效 UTF-8，会被接受。

用户于 2026-10-31 裁决选择 A（收窄规范为 BOM），已落实：

- change spec 场景改为“输入以 UTF-16 BOM 开头、为非法或截断 UTF-8，或超过注入的输入预算”。
- 引擎 §5.1 改为“以 UTF-16 BOM 开头的输入、未知编码、截断多字节序列返回 DecodeError”，并注明无 BOM 的 UTF-16 若字节本身是有效 UTF-8 会按 UTF-8 接受，M0 不新增启发式检测。
- `Utf16Bom` 文档改为“仅 BOM 前缀可区别，无 BOM 的 UTF-16 按 UTF-8 验证结果处理”。
- 金样新增 `utf16le-no-bom-valid-utf8`（`41 00 42 00`，4 字节，成功接受，源码字节不变），固定该限制；用例总数 32（21 ok、11 err）。

未采纳：B（无 BOM 检测）需要未批准的启发式与误拒边界；C（仅记录限制）会保留规范与实现的口径差异。裁决前报告只声称任务 4.1 原文的 UTF-16 BOM 行为；裁决后规范、实现、金样三者一致。

## 3. 复核已核对部分

复核者逐项复算并确认：

- 复核时 31 行金样（20 ok、11 err）的字节长度、UTF-8 有效性、仅剥一个头部 BOM、源码列、CRLF/未配对 LF 计数、多数派/平票、未改 bytes 列均正确；`%PDF`/`PK` 有效前缀不被拒收，正文 U+FEFF、第二个 BOM、孤立/末尾 CR、无尾换行保留。裁决 A 后新增 `utf16le-no-bom-valid-utf8`，总数变为 32（21 ok、11 err），由重跑门禁与独立夹具复核覆盖。
- 预算先于 UTF-8/UTF-16：`over-budget-invalid`、`over-budget-utf16` 覆盖；`bom-then-invalid` 的 `valid_up_to=3` 是原始存储字节偏移；截断用例 `error_len=None`。
- `DecodedSource` 持有输入快照 clone，`BaselineId` 身份不变；decode 无 I/O、无线程/全局状态，不修改输入、不返回部分源码。
- `decode.rs` 无 `unwrap`/`expect`/`panic!`/`unreachable!`；无新增依赖；fmt/clippy/test 门禁通过。
- 报告未超出证据声称 encode、往返、Undo、原子写盘或 V-01～V-09/M0 出口通过。

## 4. 复核后门禁

处理全部 7 项后重跑（2026-10-31T03:08:08Z 起，HEAD 不变）：

- `cargo test -p aline-source --locked --offline`：5 passed、0 failed；`-- --list` 发现 3 个解码/预算测试与 2 个既有存储测试。
- `cargo test --workspace --locked --offline`：23 passed、0 failed。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`：exit 0。
- `python3 scripts/check_workspace.py --cargo-home ... --output-dir .../workspace-gates`：全部检查 exit 0。
- 独立夹具复核 `python3 independent-check.py`：`checked=32 ok=21 err=11`，exit 0（2026-10-31T03:08:06Z）。

证据见 `gates/commands.log`、`gates/*.stdout.log`、`workspace-gates/` 与 `independent-check.command.log`。

## 5. 结论

实现与测试无阻塞行为缺陷；第 1～7 项均已处理，其中第 1 项按用户裁决 A 收窄规范并新增金样，第 2～7 项直接修复。重跑门禁全部通过，任务 4.1 可以勾选并进入第 4.2 组。
