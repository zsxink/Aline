# R03 / F-04 复用重定位的受检坐标边界 — 报告

- 批次：R03（finding F-04，阻断级）
- 证据目录：`docs/validation/m0-engine-foundation/remediation/R03/run-1/`
- 代码改动：`crates/aline-markdown/src/reuse.rs`、`crates/aline-markdown/src/lib.rs`（导出面）、`crates/aline-markdown/tests/reuse.rs`
- 执行方式：主 Agent 直接实现（R02/R05 的独立 reviewer 占满两个 Sub-agent 名额），**因此本批尚缺一名非作者的独立复核**，见 `handoff.md` 的交接项。

## 1. 根因

F-04 不是「快照身份没带下来」，而是**带下来了又被丢掉**：

- `mdast::Block` 一直持有 `range: SrcRange`（带 `SnapshotId`），`Block::range()` 返回 `&SrcRange`；
- `reuse::pair()` 却用 `old_block.range().byte_range()` 把它降级成裸 `Range<usize>` 再存进 `ReusedBlock`；
- `ReusedBlock::translate(old: usize) -> Option<usize>` 全程只做 `new_start + (old - old_start)` 的裸算术，既不复验旧端点属于哪个快照，也不复验结果在新快照里是不是 UTF-8 边界。

复现（brief 夹具，旧 `"😀\n"` / 新 `"x\n\n😀\n"`）：旧端点 `1` 在旧快照里是**半字符**，`old.src_offset(1)` 返回 `NotCharBoundary`；但旧 `translate(1)` 返回 `Some(4)`，而 `new.src_offset(4)` 同样返回 `NotCharBoundary`。也就是说**入口收得进一个旧快照自己都拒绝的坐标，出口吐出一个新快照自己也不接受的坐标**，两端都没有受检。

实测见 `pre-fix.txt`（exit 101，`left: Some(4), right: None`）与 `pre-fix.command.txt`（argv/cwd/起止 UTC/exit 101）。

## 2. API 设计（签名与校验顺序）

`ReusedBlock` 不再存裸范围，改存两个 `SrcRange`：

```rust
pub struct ReusedBlock {
    old_index: usize,
    new_index: usize,
    old_range: SrcRange,   // 携带旧 SnapshotId
    new_range: SrcRange,   // 携带新 SnapshotId
    kind: BlockKind,
}
```

- `old_range()` / `new_range()` 返回 `&SrcRange`，**不保留任何裸 `Range<usize>` 捷径**；
- 不额外存快照字段：身份本来就在 `SrcRange` 里，多存一份只会带来不一致的可能。

重定位入口：

```rust
pub fn translate(
    &self,
    new_snapshot: &TextSnapshot,
    old: &SrcOffset,
) -> Result<SrcOffset, ReuseTranslateError>
```

校验顺序**固定**，每一步有独立错误，任一步失败即返回，**不 clamp、不回退到最近合法位置**：

| 步骤 | 检查 | 错误变体 |
| --- | --- | --- |
| 1 | `old.snapshot_id()` 必须等于本配对旧范围的身份 | `WrongOldSnapshot` |
| 2 | `old.byte_offset()` 必须落在旧块范围内，**右端点按闭区间允许** | `OutsideOldBlock` |
| 3 | 传入的 `new_snapshot` 必须与本配对新范围同一身份 | `WrongNewSnapshot` |
| 4 | 用 `new_snapshot.src_offset(new_byte)` 构造并复验（越界 / 半字符 / 长度溢出） | `InvalidNewOffset(BufferError)` |

`ReuseTranslateError` 只派生 `Debug`，`InvalidNewOffset` 直接携带 `aline_buffer::BufferError`，保留「越界 / 半字符 / 长度溢出」的原始区分。`BufferError` 不实现 `PartialEq`/`Copy`（`Allocation` 变体持有 `TryReserveError`），所以本枚举不能派生 `Eq`/`PartialEq`；测试统一用 `matches!` 按变体判定，不做相等比较。这个取舍写进了枚举的文档注释。

## 3. 逐条断言对照表（旧 → 新，强度不降）

`tests/reuse.rs` 由 18 条增至 23 条，**没有删除任何一条，也没有放宽任何断言**。

| 旧断言 | 新断言 |
| --- | --- |
| `pair.old_range() == 3..7` | `pair.old_range().byte_range() == 3..7` |
| `pair.translate(x) == Some(y)` | `pair.translate(new.snapshot(), &old.snapshot().src_offset(x)?)?.byte_offset() == y` |
| `block_signature` 内 `translate(offset)?` | 先 `src_offset(offset)?` 再 `pair.translate(new_snapshot, &old_offset)?`，多一道受检构造 |
| （无） | `f04_translate_must_reject_half_char_old_offset`：断言旧端点 1 不可构造、块起点为正例、旧 API 的 `Some(4)` 通道已不存在 |
| （无） | `translate_rejects_wrong_old_snapshot` |
| （无） | `translate_rejects_wrong_new_snapshot` |
| （无） | `translate_rejects_outside_old_block` |
| （无） | `translate_accepts_in_block_endpoints_and_is_byte_exact`：遍历**闭区间** `start..=end`，逐字节断言对应关系 |

新增 5 条用例中的期望值全部是手写常量，并在修复前树上实测失败（`pre-fix.txt`），不是照实现抄写。

## 4. 闭区间右端的取舍

brief 明确要求「保留闭区间右端语义，或显式重新裁决并同步更新断言 + handoff 说明」。

**裁决：保留闭区间。** 理由：`assert_all_reused_pairs_match` 依赖 `translate(old_range.end)` 能取到 `new_range.end`——块范围的右端点在「空块 / 零宽块」语义下是合法查询点，不是越界。这个语义现在由 `translate_accepts_in_block_endpoints_and_is_byte_exact` 的 `..=` 遍历逐字节钉死，不再只靠一个用例隐含。

## 5. 消费者与影响面

- 生产消费者：**无**。全仓 `crates/` 与 `app/` 搜索，旧裸 API 的调用点只有 `tests/reuse.rs` 和 `src/lib.rs` 的再导出，因此按 brief「不要为了保住 API 而保留裸入口」收紧签名，而不是留兼容层。
- 导出面变化：`lib.rs` 增加 `ReuseTranslateError` 一条导出，属新增、无破坏。
- 受影响 Requirement/Scenario：RS-02（源码字节偏移为唯一持久坐标系；跨模块/跨 crate 的偏移必须受检）。本批把 `aline-markdown` 内部唯一的裸偏移转换通道封掉了。

## 6. 验证结果（全部实跑）

| 命令 | exit | 证据 |
| --- | --- | --- |
| `cargo test -p aline-markdown --test reuse --locked --offline` | 0，23 passed | `post-fix.txt` 节选 |
| `cargo test -p aline-markdown --locked --offline` | 0，12 段 `test result: ok` | `post-fix.txt` |
| `cargo clippy -p aline-markdown --all-targets --locked --offline -- -D warnings` | 0 | `clippy.txt` |
| `cargo fmt --check` | 先 1（4 处格式差异）→ apply → 再 0 | `fmt.txt`、`fmt-apply.txt`、`fmt-check-after.txt` |
| `python3 scripts/check_lints.py` | 0 | 本批未改 lint 脚本，仅作回归 |
| `python3 scripts/check_public_api.py --output-dir …` | 见第 7 节 | `gates/` |

工具链固定 `1.98.1`，Cargo 一律 `--locked --offline`。

## 7. `check_public_api.py`：一次失败与最终归属（重要）

首次按仓库默认环境（`CARGO_HOME` 指向 admission 隔离 home）运行时 **exit 1，`summary.json` 全部 8 例 `"passed": false`**。这不是 API 变更造成的回归，证据链如下：

1. `public-api-check/positive-lock.stderr.log`：`error: no matching package named pulldown-cmark found` / `note: offline mode (via --offline)` / `exit_code: 101`。
2. 该隔离 home 的 `registry/` **只有 `CACHEDIR.TAG`**（`gates/offline-attribution.txt`），即空缓存；同一 home 下 `cargo metadata --locked --offline` 同样 exit 101，报 `no matching package named rust-embed found`——**连元数据解析都过不去**，与任何源码/API 无关。
3. 换用真实 `CARGO_HOME=$HOME/.cargo`（仍 `--offline`、仍 `--locked`）后，**8 例全部「符合预期」，exit 0**（`gates/public-api-real-cargo-home.log` + `.command.txt`）。

**归属结论**：脚本本身没有因本次导出面变化而需要同步（脚本内不含 `reuse` / `ReusedBlock` 引用）；失败纯粹是「隔离空缓存 + 离线」的环境限制。按 brief 的「工具缺失/离线失败记『未核实』，不要联网安装」，此处**既不记为本批失败，也不记为本批通过**——记录为**环境受限、在真实缓存下已通过**。

同一限制也解释了 `scripts/check_workspace.py` 的 `dependency` 子门禁 exit 1：其 `dependency-graphs/all-metadata.stdout.log` 为空，stderr 为 `依赖门禁拒绝: JSON 输入缺失或解析失败 … Expecting value: line 1 column 1 (char 0)`，根因是上游 `cargo metadata` 在该 home 下失败。`check_workspace.py` 的其余子门禁（lint / safety / 三个 self-test）全部 exit 0。

## 8. 残留问题

- **本批缺独立复核**：由主 Agent 实现，按 tasks.md §3 必须由非作者复核，已登记为待办。
- `scripts/check_public_api.py` 在隔离空缓存下不可用，属工程环境问题（与 R11 记录的 admission 隔离取证同源），建议在 R12/R14 前统一处理缓存来源，而不是逐批绕开。
- `translate` 的第 2 步与第 4 步都可能返回与块外相关的错误（`OutsideOldBlock` / `InvalidNewOffset`）。`checked_sub` 失败在通过第 2 步后实际不可达，保留它只为彻底排除算术回绕，不构成第二套语义。
