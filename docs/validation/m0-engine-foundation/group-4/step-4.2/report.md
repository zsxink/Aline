# 任务 4.2 原 bytes 通道与多数派编码

日期：2026-10-31（UTC 03:32 起重跑门禁）。状态：独立金样、编码实现与测试已落地；三条直接门禁与统一入口均通过；独立复核的 1 项阻塞与 1 项应修已处理（见 `review.md`）。本记录只覆盖 encode 与本地字符串级逆补丁，不表示原子写盘、保存编排、Undo 历史或 V-01～V-09/M0 出口通过。

## 1. 范围与规范依据

- 任务原文：`openspec/changes/m0-engine-foundation/tasks.md` 4.2，要求实现原 bytes 通道与改后 BOM/多数派编码，按已批准语义扫描 CRLF/LF 行尾单元（含编辑新拼接 CRLF），未配对 CR 保留；验证 CR-X-LF 删除 X、边界插入/删除/粘贴的冻结源码精确值、编码、再次 decode 限定行尾等价与逆补丁精确恢复；不隐式整理 Buffer、不用存储等价吞掉历史或拒绝正常输入。
- 语义来源：`openspec/changes/m0-engine-foundation/specs/engine/source/spec.md` 的“保真编码与内容身份”，以及 `docs/engine-design.md` §5.1～5.3 的 2026-10-31 已批准原始行尾表示。4.1 已冻结 LF 策略下孤立 CR 紧邻 CRLF 的保留例外（`cr-x-lf-delete-preserve-exception`），本任务实现该例外。
- 2026-10-31 复核后用户批准的补充裁决：基线无头部 BOM 而冻结源码以 U+FEFF 开头时，改后编码补写一个头部 BOM 保护该正文字符，预算计入，成功保存后的新基线记为有 BOM；不删除源码首字符，也不拒绝该正常编辑。该裁决已写入 change spec 场景与 `docs/engine-design.md` §5.2。
- 3.3 冻结接口：`encode` 接收冻结编辑源码、存储基线与 `StorageBudget`，只产生完整待写 bytes；内容相等证据必须绑定文本与基线身份，拒绝调用方自报的 `unchanged: bool`。`encode` 不写盘、不改 Buffer/History/基线。
- 本任务不实现文件 I/O、原子替换、保存成功后的新基线发布或历史集成；这些分别归 4.3/4.4。

## 2. 独立金样

文件：`docs/validation/m0-engine-foundation/group-4/step-4.2/encode-golden.v1.txt`（SHA-256 `6ecef245a55f424dec99e31e468159ffa7a147cb8b589f3e893842d301ca94ec`）。

表由人工按规范手写，不从实现输出反推；字段为：

```text
name | input_hex | frozen_hex | budget | expected | output_hex | reused_original | redecode_hex | error
```

共 38 个用例：34 个成功、4 个预算错误。覆盖：

| 分组 | 用例 |
|---|---|
| 原 bytes 通道 | `empty`、`plain-lf-unchanged`、`bom-crlf-unchanged`、`mixed-unchanged`、`body-bom-unchanged`、`utf16le-no-bom-valid-utf8-unchanged`、`lone-cr-unchanged`、`no-trailing-newline-unchanged` |
| LF 多数派编码 | `lf-policy-converts-crlf`、`tie-changed`、`cr-x-lf-delete-lf-policy`、`paste-crlf-into-lf`、`bom-changed-lf-policy` |
| CRLF 多数派编码 | `crlf-policy-unpaired-lf`、`crlf-policy-paste-lf`、`crlf-policy-isolated-cr`、`bom-crlf-changed` |
| 边界与孤立 CR | `cr-x-lf-delete-preserve-exception`、`boundary-insert-lf`、`boundary-delete-cr`、`boundary-delete-lf`、`paste-isolated-cr-into-crlf` |
| 保护性 BOM | `leading-feff-changed-no-bom`、`leading-feff-inserted`、`leading-feff-changed-with-bom`、`leading-feff-over-budget`（错误行） |
| 行尾守卫对抗 | `guard-cr-run-one`、`guard-cr-run-two`、`guard-crlf-then-crlf`、`guard-crlf-then-lone-crlf` |
| 空文档、NUL 与多字节 | `empty-to-text`、`text-to-empty`、`nul-changed`、`utf8-multibyte-changed`、`budget-exact-boundary` |
| 预算先验 | `unchanged-over-budget`、`expansion-over-budget`、`changed-over-budget` |

关键预期：

- `boundary-delete-cr`：冻结源码删除 CR 后为 `"A\nB"`，CRLF 策略重放为 `"A\r\nB"`，与输入 bytes 恰好相同，但 `reused_original=0`——原 bytes 通道由内容精确相等决定，不由结果字节或历史选择。
- `cr-x-lf-delete-preserve-exception`：输入 `A\rX\r\nB\nC\nD` 删除 X 后为 `A\r\r\nB\nC\nD`，LF 策略下若把 CRLF 单元编码为单个 LF，会与前一个孤立 CR 拼成新的 CRLF 而丢失未配对 CR；金样要求逐字节保留 `\r\r\n`。
- `cr-x-lf-delete-lf-policy`：输入 `A\rX\nB` 删除 X 后为 `A\r\nB`，LF 策略保存重开为 `"A\nB"`；只声称行尾表示等价，不声称 String 相同。
- `leading-feff-changed-no-bom`：输入 `A␣B`（正文 U+FEFF、无头部 BOM）删除 `A` 后冻结源码为 `␣B`；编码补写一个头部 BOM，输出 `BOM + ␣B`，再解码仍为 `␣B`，不丢失该字符。
- `leading-feff-changed-with-bom`：基线本有 BOM 且源码正文以 U+FEFF 开头时只写一个头部 BOM，不重复写两个。
- `leading-feff-over-budget`：保护性 BOM 计入预算，输出 6 字节、预算 5 时先验拒绝。
- `guard-*`：对 CR 连串紧邻 CRLF 的形状（`\r\r\r\n`、`\r\n\r\r\n`、`\r\r\n\r\n`）固定保留例外，避免参考实现与实现共享同一错误邻接判定而无人发现。
- `expansion-over-budget`：BOM + CRLF 重放后完整待写 bytes 为 13 字节，预算 12 时必须先验拒绝，`needed=13`。

独立复核脚本 `independent-check.py` 只读金样并用 Python 标准库重算：原始解码事实、原 bytes 通道、参考编码器（含孤立 CR 保留例外与保护性 BOM）、再解码、限定行尾等价、BOM 重放/保护与预算错误。2026-10-31T03:31:27Z 实际运行退出 0，输出 `checked=38 ok=34 err=4`（`independent-check.command.log`、`independent-check.stdout.log`）。该核对是夹具自检，不替代 Rust 实现证据。

## 3. 实现

新增 `crates/aline-source/src/baseline.rs`：

- `SourceBaseline` 持有原 bytes 的 `StorageSnapshot`、对应精确冻结源码与 `StorageMetadata`；字段私有，只能由 crate 内解码或后续受检写盘事实创建。`source()` 返回解码所得精确文本，不是按行尾策略重建的副本。
- 类型文档固定不变量：`source` 必须恰为 `storage.bytes()` 的解码结果，`metadata` 必须是该次解码事实；4.3/4.4 增加成功写盘基线时也必须从实际写出 bytes 解码得到。
- `DecodedSource` 改为持有 `SourceBaseline`，保留 `storage()`/`source()`/`metadata()`，新增 `baseline()`/`into_baseline()`。

新增 `crates/aline-source/src/encode.rs`：

- `encode(frozen_source: &str, baseline: &SourceBaseline, budget: StorageBudget) -> Result<EncodedBytes, EncodeError>`。
- 相等证据由 source 在同一基线对象上逐字比较 `frozen_source == baseline.source()` 得出；API 不存在调用方可传入的 `unchanged: bool`。
- 相等时检查原 bytes 长度预算后逐字节返回基线 bytes；即使曾经编辑过或结果恰好相同，也不会把“已修改”误判为原 bytes 通道。
- 不等时扫描行尾单元：完整 CRLF 计一个单元，不属于 CRLF 的 LF 计一个单元，孤立 CR 原样输出；LF 策略把两类行尾单元编码为 LF，CRLF 策略编码为 CRLF；CRLF 策略下不把已有 CR 再扩成额外 CR。
- LF 策略的保留例外：CRLF 单元若紧跟源码中的孤立 CR，则保留 `\r\n`，避免重开时孤立 CR 与 LF 拼成新 CRLF 而丢失未配对 CR。
- `emits_header_bom`：基线原有 BOM 时重放；基线无 BOM 但冻结源码以 U+FEFF 开头时补写一个头部 BOM，保护该正文字符；两种情形合计只写一个 BOM。
- 先统计完整待写长度（正文 + 头部 BOM），用 `checked_add` 防溢出，再与预算比较，最后分配并写入；`EncodeError::{BudgetExceeded, LengthOverflow}` 可区别。`LengthOverflow` 是防御性分支：CRLF 策略下正文最坏约为源码两倍，正常可分配输入不会触发。
- `EncodedBytes` 提供 `bytes()`、`baseline_id()`、`reused_original()`；`reused_original` 只反映 source 内部相等比较，不是调用方声明。

3.3 错误类别映射：预算对应 `BudgetExceeded`，编码长度失败对应防御性的 `LengthOverflow`；“基线或相等证据不匹配”因 API 内部比较而按构造不可达。该映射已写入模块文档。

新增公开项：`SourceBaseline`、`encode`、`EncodedBytes`、`EncodeError`。实现无 GPUI、无线程、无全局状态、无新增依赖，不使用 `unwrap`/`expect`/`panic!`/`unreachable!`。

## 4. 验证

环境与时间：2026-10-31T03:32:32Z 起，macOS Darwin 27.0.0 arm64，`rustc 1.98.1`、`cargo 1.98.1`，`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`，离线 Cargo。证据见 `gates/commands.log` 与 `workspace-gates/*.command.json`。

直接门禁（`gates/run-direct-gates.sh`）：

| 命令 | 退出码 | 结果 |
|---|---|---|
| `cargo test -p aline-source --locked --offline` | 0 | 11 passed、0 failed（3 decode + 6 encode + 2 public-boundary） |
| `cargo test -p aline-source --locked --offline -- --list` | 0 | 发现全部 11 个测试 |
| `cargo test -p aline-source --locked --offline encode` | 0 | 2 passed（名称含 `encode` 的用例；其余 4 个编码测试由全量运行覆盖） |
| `cargo fmt --all --check` | 0 | 无差异 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 无警告 |
| `cargo test --workspace --locked --offline` | 0 | 29 passed、0 failed |

六个编码测试：

- `encode_golden_cases_match_independent_expectations`（`crates/aline-source/tests/encode_golden.rs`）：加载时绑定 34 个成功、4 个错误用例并拒绝重复名称；逐行检查编码字节、原 bytes 通道判定、基线身份、再次 decode 源码、限定行尾等价（把 CRLF/未配对 LF 归一为 LF、孤立 CR 保留）、BOM 重放/保护（`input` 有 BOM 或改后冻结源码以 U+FEFF 开头），以及基线原 bytes 未被修改。
- `budget_boundary_and_categories_are_distinct`：验证预算等号边界接受、超出返回 `EncodeError::BudgetExceeded`，并覆盖原 bytes 通道也被预算约束。
- `encoded_bytes_carry_their_own_baseline_identity`：两个内容相同但身份不同的基线各自返回自己的基线身份，不共享缓存或跨基线复用 bytes。
- `inverse_patches_restore_exact_source_and_original_bytes`：对 CR-X-LF 删除 X、孤立 CR 紧邻 CRLF 的保留例外、边界插入 LF、边界删除 CRLF 中的 LF、粘贴混合行尾、行首正文 U+FEFF、删除 CRLF 中的 LF（结果 bytes 等于输入但未走原通道）、粘贴孤立 CR 到 CRLF 文档、CRLF 策略未配对 LF 九个场景，先断言冻结源码精确值，再检查编码、再次 decode 的限定行尾等价，最后用字符串级逆补丁恢复并确认重编码命中原 bytes 通道且逐字节等于输入。
- `guard_rule_roundtrips_all_small_cr_lf_sources`：属性检查，对 `{A, CR, LF}` 长度不超过 6 的全部 1093 个串、LF 与 CRLF 两种策略共 2186 次 encode→decode 往返，验证限定行尾等价；不依赖参考编码器的同一邻接判定，直接暴露守卫规则反例。
- `public_encoding_types_are_send_and_sync`：静态断言公开编码类型可跨线程传递；这不表示引擎启动线程，引擎仍无线程与运行时全局状态。未增加 compile-fail 测试（不引入新依赖），外部不可构造性由 `SourceBaseline::new` 的 `pub(crate)` 可见性保证。

统一入口（`python3 scripts/check_workspace.py --cargo-home ... --output-dir .../workspace-gates`）全部检查退出 0，包括 `fmt`、`clippy`、`test`、`dependency`、`lint`、`safety`、`public-api` 及其 self-test。其中 `public-api` 项是夹具的正/反例编译 self-test，不是对仓库实际公开面的扫描（同 group-3/step-3.3 记录的限定）。逐项命令、开始/结束 UTC 与退出码见 `workspace-gates/*.command.json`。

## 5. 未声称与边界

- 原子写盘、临时文件、失败保全与保存成功后基线发布属于 4.3/4.4；本任务只验证纯编码与测试内字符串级逆补丁，不声称 Undo 历史、保存编排或磁盘行为。
- 内存中的缓冲区、历史、工作版本仍未接入 encode；本任务不声称“未修改”判定已集成到 UI 或持久化路径。
- 逆补丁是测试内对 ASCII 编辑的区间恢复，不是 `aline-buffer` 的 `CheckedBatch`/History 实现；buffer 补丁与历史归第 5 组。
- `reused_original` 是 source 内部比较事实，不是调用方可构造的标志；本任务不提供无来源 bool 入口。
- `LengthOverflow` 未用真实 `&str` 构造测试（需接近 `usize::MAX` 的分配），只作为防御性分支记录；本任务的错误证据只覆盖 `BudgetExceeded`。
- 保护性 BOM 写盘后的新基线（`has_bom=true`）属 4.3/4.4 的基线发布；本任务只产出 bytes 并固定再解码结果。
- 仅在 macOS arm64 与锁定工具链上运行；未验证其他平台、权限、磁盘满、崩溃或断电行为。V-01/V-06/V-07/V-08 与 M0 出口均未据此判通过。

## 6. 证据清单

| 证据 | 路径 |
|---|---|
| 独立金样 | `encode-golden.v1.txt`（SHA-256 `6ecef245a55f424dec99e31e468159ffa7a147cb8b589f3e893842d301ca94ec`） |
| 夹具独立复核 | `independent-check.py`（SHA-256 `bc10ad939f626cfe93ceaff51b88f53d869f045511a41e6350b01b82652dad6f`）、`independent-check.command.log`、`independent-check.stdout.log` |
| 编码实现 | `crates/aline-source/src/encode.rs`（SHA-256 `49a9891bd0cb3462c5fe1e08d224b2da84dabd620e4863ae08b6f2c7b25f0227`） |
| 基线实现 | `crates/aline-source/src/baseline.rs`（SHA-256 `5ef1649150ce97605f0ffe1724a50c08c6f33e99ecb294cf849255d098e7d81d`） |
| 解码调整 | `crates/aline-source/src/decode.rs`（SHA-256 `b87b683b2a7e493414a324fb558b948512239688c3d25dd9b27ae99dc4404291`） |
| 导出 | `crates/aline-source/src/lib.rs`（SHA-256 `f463952ddc30044806d939802efec8986e5df674614cad62a00ac94187b108b7`） |
| 外部消费者测试 | `crates/aline-source/tests/encode_golden.rs`（SHA-256 `312c102394d22c9b40a45df1db87b7f78a52d20759299802dcd4fe8c2e4f7bb5`） |
| 独立复核记录 | `review.md` |
| 直接门禁 | `gates/commands.log`、`gates/*.stdout.log`、`gates/*.stderr.log` |
| 统一入口 | `workspace-gates/` 下逐命令 JSON 与日志 |

直接门禁运行时的 HEAD 为 `24e5d68`，工作区包含本任务的实现、测试、金样与证据；提交时勾选 `tasks.md` 4.2。
