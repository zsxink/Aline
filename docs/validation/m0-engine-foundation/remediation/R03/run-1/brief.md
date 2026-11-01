# R03 派发契约：F-04 复用重定位的受检坐标边界

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `/Users/xian/Project/editor/Aline`。

## 文件所有权（只有你可写这些文件）

- `crates/aline-markdown/src/reuse.rs`
- `crates/aline-markdown/src/lib.rs`（仅当导出面需要随 API 变化调整时）
- `crates/aline-markdown/tests/reuse.rs`
- `docs/validation/m0-engine-foundation/remediation/R03/run-1/` 下你自己新建的证据文件

不得写其它文件（尤其不要动 `crates/aline-markdown/src/pre/rewrite.rs`、`crates/aline-ui/**`、任何 `scripts/**`）。不得 `git reset`、不得回退他人改动。

## 失败（必须先写出失败回归，再修）

**F-04（阻断）**：`ReusedBlock::translate(&self, old_offset: usize) -> Option<usize>` 以及 `old_range()/new_range()` 都跨公开边界交出裸 `usize`，不携快照身份、不验证 UTF-8。原复核探针（原文，可作回归起点）：

```rust
use aline_buffer::{TextSnapshot,TextBudget};
use aline_markdown::{parse,reuse,ParseConfig,ParseBudget};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let old=TextSnapshot::new("😀\n".to_string(),TextBudget::new(1024))?;
 let new=TextSnapshot::new("x\n\n😀\n".to_string(),TextBudget::new(1024))?;
 let a=parse(&old,ParseConfig::m0(),ParseBudget::m0())?;
 let b=parse(&new,ParseConfig::m0(),ParseBudget::m0())?;
 let reused=reuse(&a,&b);let p=reused.reused().first().ok_or("no reused")?;
 println!("old_range={:?} new_range={:?}",p.old_range(),p.new_range());
 println!("old.src_offset(1)={:?}",old.src_offset(1));
 println!("pair.translate(1)={:?}",p.translate(1));
 if let Some(off)=p.translate(1){println!("new.src_offset(result)={:?}",new.src_offset(off));}
 Ok(())
}
```

原实测：`old.src_offset(1)=Err(NotCharBoundary { offset: 1 })`，但 `pair.translate(1)=Some(4)`，而 `new.src_offset(4)=Err(NotCharBoundary { offset: 4 })`。即公开 API 接受了一个本快照明确非法的旧端点，并返回一个同样非法的新端点。

## 适用规范（先读，再动手）

- `AGENTS.md` 铁律 3 与 RS-02 段：跨模块/跨 crate 的源码偏移与范围必须 newtype/受检封装；模块内部单坐标可用明确命名的 `usize`；表示边界处受检转换；不新增反向依赖。
- `docs/rust-style-guide.md` S-5-01 与 RS-02 条目（`type SrcOff = usize` 只是别名，不能阻止混用）。
- `docs/engine-design.md` §7 坐标段与 §199 行附近关于 `src_off` 的定义。

## 目标

1. **API 受检化**（RS-02，不给调用方留裸整数捷径）：
   - 入口只接受绑定旧快照的 `aline_buffer::SrcOffset`；出口只给绑定新快照的 `aline_buffer::SrcOffset`，失败走 `Result`（可新增专用错误类型，也可复用既有错误类型，但要能区分「错旧快照 / 错新快照 / 半字符 / 越界 / 块外」）。
   - 校验顺序与失败原因要显式：先验旧端点所属快照，再验它在本块旧范围内的合法性，再验新旧范围身份，最后用新快照构造并校验新端点（含 UTF-8 边界与长度）。不得 clamp、不得静默修正。
   - `old_range()` / `new_range()` 的裸 `&Range<usize>` 访问面同样受检化：改为携带快照身份的 `SrcRange`（或等价的受检类型），不得同时保留裸范围捷径。
   - 具体签名与类型归属由你依现有分层决定（`ReusedBlock` 目前只存 `Range<usize>`，需要新增快照身份；`SnapshotId` 已实现 `Eq/PartialEq`，`ReuseOutcome` 的 `Eq/PartialEq` 派生不得因此丢失）。
2. **清理无消费者的原始公开转换**：全仓只有 `crates/aline-markdown/tests/reuse.rs` 消费 `translate` 与两个 range 访问器（无生产消费者）。不要为了「保住 API」而保留裸入口，也不要预先安装没有消费者的新 API 形状（只加本次契约需要的受检入口）。
3. **先写失败回归**：在 `crates/aline-markdown/tests/reuse.rs` 写与 F-04 探针等价的测试（`😀\n` / `x\n\n😀\n` 同一夹具），断言旧端点 1 被拒、新端点 4 不可被构造。它在修复前必须**失败**（原 API 下表现为「被接受」）。
4. **补负例**（每项独立预期，先手写期望再跑）：
   - 旧快照错（拿另一 `TextSnapshot` 的 `SrcOffset` 喂给该 pair）；
   - 新快照错（用错的新快照请求转换）；
   - 旧端点半字符（emoji 内部偏移）；
   - 块外端点（`old_range.start - 1` 若存在、`old_range.end + 1`、`usize::MAX`）；
   - 合法端点正例：`old_range.start` → `new_range.start`、`old_range.end` → `new_range.end`，以及块内中间端点逐字节对应。
   - 若 `translate` 允许 `old_range.end`（原实现允许，`assert_all_reused_pairs_match` 依赖它），保留该语义并在报告里明确写出「闭区间右端」的取舍理由；改语义就要同时更新既有断言并在 handoff 单列。
5. 既有测试 `old_absolute_ranges_fail_against_new_snapshot`、`assert_all_reused_pairs_match` 及全部 reuse 测试不得减少或放宽；`crates/aline-markdown/tests/reuse.rs` 的改动只应是适配新受检 API 和新增负例。
6. 不得新增 `unwrap()/expect()/panic!/unreachable!/assert!` 到生产代码（clippy 已 deny）；测试里也不得 `unwrap()`，用 `?` 与显式错误。
7. 全部 Cargo 命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`，`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」，不要联网安装。
8. 提交（Conventional Commits，scope `aline-markdown`）。注意 `scripts/check_public_api.py` 的既有正例不得因本次导出面变化而失效——若你的改动让它需要同步（它不在你的所有权内），在报告里单列，不要越权改脚本。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R03/run-1/`）

- `pre-fix.txt`（修复前 F-04 回归的实际 argv/cwd/时间/exit code/原始输出，确认它先失败）
- `post-fix.txt`（修复后 `cargo test -p aline-markdown --locked --offline` 全量原始输出）
- `commands.txt`（每条 argv、cwd、起止时间、exit code）
- `report.md`（根因、API 设计（签名与校验顺序）、逐条断言对照表、闭区间右端取舍、残留问题、是否影响 `check_public_api`）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status（DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED）、提交哈希、一行测试小结、concerns、证据路径。不要贴长日志。
