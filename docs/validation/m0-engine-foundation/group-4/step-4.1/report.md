# 任务 4.1 解码金样与预算先验

日期：2026-10-31（UTC 03:08 起运行门禁）。状态：独立金样、解码实现与测试已落地，独立复核与 UTF-16 裁决已处理，三条直接门禁与统一入口均通过；任务 4.1 已可勾选。本记录只覆盖解码，不表示 encode、往返、Undo、原子写盘或 V-01～V-09/M0 出口通过。

## 1. 范围与规范依据

- 任务原文：`openspec/changes/m0-engine-foundation/tasks.md` 4.1，要求先建独立 bytes→保留 CR/LF 的源码/元数据→bytes 金样，再实现预算先验与解码；覆盖空、BOM、正文 U+FEFF、LF、CRLF、混合双方多数及平票、孤立 CR、无尾换行、有效 `%PDF`/`PK`，以及非法 UTF-8、UTF-16 BOM、截断多字节与输入超限。
- 语义来源：`docs/engine-design.md` §5.1～5.3 的 2026-10-31 已批准原始行尾表示，`openspec/changes/m0-engine-foundation/specs/engine/source/spec.md` 的“有效编码与存储来源”，以及 `openspec/CODE_STANDARDS.md` §6.2 的证据边界要求。
- 本任务不修改 `docs/engine-design.md` 等已确认规范文本；引擎 §5、架构 §5.1、V-06/V-07 的回填归 4.5。
- 3.3 记录中的“`decode` 仅契约未验证”是 3.3 复核时点的状态；自本任务起 `decode` 已有实现与测试，历史记录不追溯改写。

## 2. 独立金样

文件：`docs/validation/m0-engine-foundation/group-4/step-4.1/decode-golden.v1.txt`（SHA-256 `f876654a1355c65e5aade1d727110b9c92f243a3129d1cabc9b03c3c180572c1`）。

表由人工按上述规范逐项手写，不从实现输出反推；测试只读取该表。共 32 个用例，其中 21 个成功、11 个错误。字段为：

```text
name | input_hex | decode_budget | expected | source_hex | bom | crlf_pairs | unpaired_lf | line_ending | unchanged_encode_hex | error
```

覆盖分组：

| 分组 | 用例 |
|---|---|
| 空/BOM/正文 U+FEFF | `empty`、`bom-only`、`header-bom`、`body-bom-without-header`、`body-bom-after-header`（仅剥离一个头部 BOM，第二个 U+FEFF 留作正文） |
| 行尾表示 | `lf-only`、`crlf-only`、`bom-with-crlf`、`mixed-crlf-majority`、`mixed-lf-majority`、`mixed-tie`、`crlf-majority-with-isolated-cr` |
| 孤立 CR 与 EOF | `isolated-cr`、`trailing-cr`、`isolated-cr-and-lf`、`isolated-cr-before-crlf-lf-majority`（孤立 CR 紧邻 CRLF 单元时仍按一个 CRLF 对计数，为 4.2 的 LF 策略编码例外预留）、`no-trailing-newline` |
| 有效签名与多字节 | `pdf-prefix`、`pk-prefix`、`unicode-4byte-crlf`、`utf16le-no-bom-valid-utf8`（无 BOM 的 UTF-16LE 字节恰好是有效 UTF-8，按已裁决口径接受为 UTF-8 文本） |
| 编码错误 | `utf16le-bom`、`utf16be-bom`、`invalid-utf8`、`lone-continuation`、`truncated-3byte`、`truncated-4byte`、`bom-then-invalid` |
| 预算先验 | `over-budget`、`over-budget-invalid`（先于 UTF-8 拒绝）、`over-budget-utf16`（先于 UTF-16 识别拒绝）、`bom-over-budget` |

`unchanged_encode_hex` 是“未修改时 encode 必须逐字节返回原 bytes”的 4.2 预期。4.1 尚未实现 encode；金样加载时检查该列对成功用例与输入逐字节一致，作为夹具内部一致性和后续任务入口，不把它写成已通过的原 bytes 往返。加载时同时绑定用例总数为 31、拒绝重复名称，并校验 err 行的源码、BOM、行尾计数、行尾策略与未改 bytes 列取空值或规范零值，避免删行、改名或未用列自相矛盾静默通过。

独立算术核对：`independent-check.py` 只读金样文本，用 Python 标准库重算字段数、字节长度、UTF-8 有效性、仅剥一个 BOM、CRLF 对数、未配对 LF 数、多数派/平票、未改 bytes 列，以及错误用例的预算先验、UTF-16 BOM 前缀与 UTF-8 错误偏移/长度；不调用 Rust 实现。2026-10-31T03:08:06Z 实际运行退出 0，输出 `checked=32 ok=21 err=11`（`independent-check.command.log`、`independent-check.stdout.log`）。除初次手写 `pk-prefix` 预算少算 1 字节（已按实际 8 字节修正）外无其他不一致。该核对是夹具自检，不是实现证据。

## 3. 实现

新增 `crates/aline-source/src/decode.rs`，由 `crates/aline-source/src/lib.rs:6` 导出：

- `decode(&StorageSnapshot, StorageBudget) -> Result<DecodedSource, DecodeError>`（`decode.rs:112`）。
- `DecodedSource` 持有 `storage()`、`source()`、`metadata()`；存储视图是输入快照的 clone，`BaselineId` 身份相同。
- `StorageMetadata`（`decode.rs:22`）记录 `has_bom`、`crlf_pairs`、`unpaired_lf`、`line_ending`。
- `LineEnding::{Lf, Crlf}`（`decode.rs:13`）。
- `DecodeError::{BudgetExceeded, Utf16Bom, InvalidUtf8}`（`decode.rs:53`），三类可区别；`InvalidUtf8` 的 `valid_up_to` 相对原始存储字节（含头部 BOM），`error_len: None` 表示序列在输入末尾截断。

处理顺序与语义：

1. 先比较输入字节数与注入预算；超限直接 `BudgetExceeded`，不进入编码识别。
2. 原始字节以 `FF FE`/`FE FF` 开头时返回 `Utf16Bom`，区分大小端；不尝试转码。
3. 对完整原始字节执行 `std::str::from_utf8`；失败返回 `InvalidUtf8`，不做有损替换。
4. 只剥离一个头部 UTF-8 BOM；第二个 BOM 或其他正文 U+FEFF 保留在源码中。
5. 扫描源码字节：完整 CRLF 计一个 CRLF 对，不属于 CRLF 的 LF 计未配对 LF，孤立 CR 不计数但逐字节保留；`crlf_pairs > unpaired_lf` 选 CRLF，否则（含平票与零）选 LF。
6. 不写盘、不修改输入快照、不持有 Buffer/History/UI 类型。

预算分两层：`StorageSnapshot::new` 在接管宿主字节时按构造预算拒绝，返回 `StorageError::BudgetExceeded`；`decode` 再按注入预算先于编码识别拒绝，返回 `DecodeError::BudgetExceeded`。金样用 `usize::MAX` 构造快照，以单独验证 decode 预算；`snapshot_and_decode_budgets_are_distinct_layers` 覆盖构造期超限、等号边界与 decode 期超限。

实现不依赖文件系统、线程或全局状态，新增公开项均有文档；`decode.rs` 不使用 `unwrap`/`expect`/`panic!`/`unreachable!`。

## 4. 验证

环境与时间：2026-10-31T03:08:08Z 起，macOS Darwin 27.0.0 arm64，`rustc 1.98.1`、`cargo 1.98.1`，`CARGO_HOME=<cargo-home>`，离线 Cargo。证据见 `gates/commands.log` 与 `workspace-gates/*.command.json`。

直接门禁（`gates/run-direct-gates.sh`）：

| 命令 | 退出码 | 结果 |
|---|---|---|
| `cargo test -p aline-source --locked --offline` | 0 | 5 passed、0 failed（本任务新增 3 个集成测试 + 既有 2 个） |
| `cargo test -p aline-source --locked --offline -- --list` | 0 | 发现 3 个解码/预算测试与 2 个既有存储测试 |
| `cargo fmt --all --check` | 0 | 无差异 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 无警告 |
| `cargo test --workspace --locked --offline` | 0 | 23 passed、0 failed |

实际发现的三个解码测试：

- `decode_golden_cases_match_independent_expectations`（`crates/aline-source/tests/decode_golden.rs:230`）：加载时绑定 32 个用例、拒绝重复名称并校验未用列，然后逐行检查源码字节、BOM、CRLF 对数、未配对 LF 数、行尾策略、错误类别/偏移、存储基本身份与输入字节不变。
- `snapshot_and_decode_budgets_are_distinct_layers`（`decode_golden.rs:326`）：分别验证 `StorageSnapshot::new` 构造期超限返回 `StorageError::BudgetExceeded`、等号边界接受，以及 `decode` 注入预算不足返回 `DecodeError::BudgetExceeded`，说明两层预算不混淆。
- `decode_does_not_write_the_original_file`（`decode_golden.rs:354`）：在 Cargo 提供的 target 临时目录写入非法 UTF-8 与有效 BOM+CRLF 两个文件，解码后逐字节重读，确认原文件未变化；非法输入不返回源码，有效输入返回保留 CRLF 的源码。

“无有损替换”由结构保证并由错误用例验证：所有非法/截断输入返回 `Err`，成功路径只接受 `from_utf8` 验证后的完整文本。“无原文件写入”由解码不持有任何 I/O 入口和上述真实文件重读测试共同证明；两者都不是权限、磁盘满或崩溃语义的证明。

统一入口（`python3 scripts/check_workspace.py --cargo-home ... --output-dir .../workspace-gates`）全部检查退出 0，包括 `fmt`、`clippy`、`test`、`dependency`、`lint`、`safety`、`public-api` 及其 self-test。其中 `public-api` 项是夹具的正/反例编译 self-test，不是对仓库实际公开面的扫描（同 group-3/step-3.3 记录的限定）。逐项命令、开始/结束 UTC 与退出码见 `workspace-gates/*.command.json`。

## 5. 未声称与边界

- encode、原 bytes 通道、多数派重编码、边界拼接、逆补丁与再次 decode 等价属于 4.2/4.3；本记录不声称已完成。`unchanged_encode_hex` 仅冻结预期。
- `isolated-cr-before-crlf-lf-majority` 冻结了一个 4.2 必须处理的编码边界：LF 策略下若把紧跟在孤立 CR 之后的 CRLF 单元直接编码为 LF，会与前一个孤立 CR 拼成新的 CRLF，导致未配对 CR 丢失；解码本身不受影响，本任务不实现该编码例外。
- 4.1 不解码文件以外的状态，不读路径、不检查权限、不写盘；原子写原语与故障边界归 4.4。
- UTF-16 只在存在 `FF FE`/`FE FF` BOM 时可区别地拒绝。无 BOM 的 UTF-16 若字节序列本身是有效 UTF-8（例如 ASCII 字符夹 NUL），在 M0 中会被当作 UTF-8 文本接受；规范未提供可靠的启发式检测，本任务不新增猜测式拒收。
- 2026-10-31 裁决 A：change spec 场景与引擎 §5.1 已收窄为“以 UTF-16 BOM 开头的输入”，与任务 4.1 原文和实现一致；`utf16le-no-bom-valid-utf8` 金样固定无 BOM 的有效 UTF-8 字节按 UTF-8 文本接受。裁决记录见 `review.md`。
- `%PDF`/`PK` 只作为有效 UTF-8 前缀测试，不用于拒收；签名启发式不属于解码路径。
- 仅在 macOS arm64 与锁定工具链上运行；未验证其他平台、权限、磁盘满、崩溃或断电行为。V-01/V-06/V-07/V-08 与 M0 出口均未据此判通过。

## 6. 证据清单

| 证据 | 路径 |
|---|---|
| 独立金样 | `decode-golden.v1.txt`（上列 SHA-256） |
| 夹具独立复核 | `independent-check.py`（SHA-256 `0ca0cbb67d2efc90721552cdb654a6825c85e09e53567604b3b8d2d1a846c667`）、`independent-check.command.log`、`independent-check.stdout.log` |
| 解码实现 | `crates/aline-source/src/decode.rs`（SHA-256 `3d4754f02260e8393fd76b1b960fc587630f7d725ffa228ca955deb214684d38`） |
| 外部消费者测试 | `crates/aline-source/tests/decode_golden.rs`（SHA-256 `e94b762c25da105f14fafe34b385dfc9726d62592c87080e9162acf465598a1f`） |
| 独立复核记录 | `review.md`（0 阻塞、2 应修、5 建议；UTF-16 按裁决 A 收窄规范并新增金样） |
| 直接门禁 | `gates/commands.log`、`gates/*.stdout.log`、`gates/*.stderr.log` |
| 统一入口 | `workspace-gates/` 下逐命令 JSON 与日志 |

直接门禁运行时的 HEAD 仍是 `6731ff2d57e78916e659ea95e517ea598053501b`，工作区包含本任务的实现、测试、金样、复核记录与规范裁决；复核与裁决已完成，提交时勾选 `tasks.md` 4.1。

## 5. 后续勘误（R13，2026-11-01）

本节为后续追加勘误，不改写上文的历史记录。上文“加载时同时绑定用例总数为 31”是报告文字错误：当前代码 `crates/aline-source/tests/decode_golden.rs` 的 `EXPECTED_CASE_COUNT` 为 32，金样 `decode-golden.v1.txt` 有 32 行用例，独立核对输出为 `checked=32 ok=21 err=11`。三项事实一致指向 32；实现与金样未发现数量差异。原报告的 31 保留作为历史文本，并由本段纠正。核对命令和原始输出见 `docs/validation/m0-engine-foundation/remediation/R13/run-1/`。
