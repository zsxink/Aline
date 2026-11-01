# R04 派发契约：F-02 edit→DocCore 受检消费（普通/IME 提交不再绕过 plan/validate）

基线 HEAD：以派发时 `git rev-parse HEAD` 为准（**必须晚于 R01 的 `79c3b54`**，R01 已改 `doc_core.rs`；不要再基于 `2f1dcea` 复述旧行号）。仓库根 `<repo>`。前置 R01（已完成）、R02、R03（markdown 侧）已落地；**不得写 `crates/aline-markdown/**`、`crates/aline-edit/**`、`crates/aline-buffer/**` 的任何源码**。

## 文件所有权（只有你可写这些文件）

- `crates/aline-ui/Cargo.toml`（新增 `aline-edit.workspace = true`；已有 `aline-markdown` 依赖但当前无消费者，可按需使用）
- `crates/aline-ui/src/document/doc_core.rs`
- `crates/aline-ui/src/document/doc_core/protocols.rs`
- `crates/aline-ui/src/document/**` 下你新建的模块（如 `checked_commit.rs`）与 `mod.rs` 的模块声明
- `crates/aline-ui/src/lib.rs`（仅导出面）
- `crates/aline-ui/tests/**`（新建集成测试优先）
- `docs/validation/m0-engine-foundation/remediation/R04/run-1/` 下你自己新建的证据文件

不得 `git reset`、不得回退 R01 或他人改动。若判定必须改 `aline-edit`/`aline-markdown` 才能闭环，**不要动手**：登记 `待裁决` 并说明最小缺口。

## 失败事实（F-02，阻断，维度 D1/D4/D6）

原独立复核探针（`<tmp>/aline-m0-independent-review.pTFf8u/C/repro.rs`，实测原文）：

```
protect mode=Visual
protect type=Ok(())
protect commit=Ok(Committed { revision: DocumentRevision(1) })
protect actual="---\nt: Yx\n---\n"
```

即默认 Visual 模式、文本 `"---\nt: x\n---\n"`、在偏移 7 输入 `Y` 并提交：front matter 属受保护区间，却被直接写入，且 revision 前进。根因：`DocCore::commit_typing_group` 与 `composition_commit` 各自直接
`CheckedBatch::new(buffer.snapshot(), vec![(range, draft)], self.patch_budget())` → `buffer.apply(...)`，
**UI 从未消费 `aline-edit` 的 `plan_change`/`validate_change`**；`EditContext` 的 Boolean 字段也没有任何宿主侧来源。

## 目标（分两个子批，同一 finding 归属，出口不降）

### R04a：普通输入与 IME 提交走受检消费链（闭环 F-02 原始复现）

1. 普通组提交（`commit_typing_group`）与 IME 提交（`composition_commit`）改为：
   - 用当前一致快照（`current_snapshot`）与**已采纳的 `Parsed`** 构造 `EditContext`（`Visual`/`Source` 由 `self.mode` 映射；`pending_input`/`composition_active` 由真实状态给出）；
   - 以 `EditCommand::Replace { edits: [SourceEdit(range, expected, draft)] }` 调 `plan_change`，再 `validate_change`；
   - 只有 `PlanOutcome::Changed` 且 `validate_change` 成功后才 `buffer.apply(plan.batch(), …)`；
   - `PlanOutcome::Unchanged` 走既有 `Unchanged` 语义（不记步、不前进 revision）。
2. 拒绝必须**相对调用前状态零副作用**：草稿、选区、版本、历史、redo 分支、快照身份全部不变。被拒时返回可区分的新 `DocError` 变体（建议携带 `EditRefusal`），并在文档注释写明「拒绝不提交草稿」。
3. **`Parsed` 的持有与采纳**：`DocCore` 目前没有 `Parsed`。最小方案是新增「采纳派生结果」入口：复用既有 `accept_derived_result`（`InputIdentity` 已绑定 document/working version/snapshot id/config generation/request id）校验通过后**保存**该 `Parsed`，并在任何使快照身份失效的路径（草稿增删、提交、undo/redo、reload、`advance_parse_config`、关块/开块、`switch_mode` 实际切换）**作废**它。校验必须包含 `parsed.snapshot().id() == 当前快照 id`（`plan_change` 的 `bind_parsed` 也会再验一次，不要依赖单层）。
   - 无已采纳 `Parsed` 时怎么办，必须给出明确选择并写进 `report.md`：`Err` 拒绝（宿主须先派生解析）**或**在提交点做一次受预算约束的解析。**禁止**每键全文解析——若选后者，必须证明它只在提交点发生（900ms 组边界/IME 提交），不在 `type_text`/`composition_update` 路径。
4. **不得引入第二份可写全文或第二历史**：`Parsed` 只持不可变快照与派生数据；写入仍只有 `StringBuffer::apply` 一次原子发布。
5. **预算口径**：`DocCore::patch_budget()` 是 `PatchBudget::new(1, …)`（只允许 1 个补丁）。普通/IME 提交是单区间补丁，`plan_change` 会用调用方注入的 `EditBudget`，**不要**把 `plan_change` 的结果再塞进 `patch_budget()` 二次校验；`apply_change` 侧也不得用这个单补丁上限去约束命令计划（Source 多补丁 `BulkReplace` 会被误拒）。`EditBudget` 的三项（patch/text/parse）如何从 `DocLimits` 与 `ParseBudget::m0()` 派生，必须写进 `report.md` 并说明为何不新增公开配置面。

### R04b：公开 `apply_change` 单一写入口（命令路径）

1. 按 `openspec/changes/m0-engine-foundation/design.md:25`（「`apply_change` 是文档写入口；负责候选状态最终重验并发布，不在 render/每键做全量解析或全文哈希」）与 `:61`（公开接口最小集合含「`DocCore` 的 `apply_change`」），新增 `DocCore::apply_change`：
   - 入参是 `aline-edit` 的 `ChangePlan`（M0 的 `Change` 形态）或等价受检类型；
   - UI 侧**复验会话身份**：计划绑定的 document/working version/before snapshot/投影/选区/解析配置代次与 DocCore 当前状态逐项比对，然后 `validate_change(plan, 当前快照, 已采纳 Parsed, 当前选区, 当前 EditContext)`，成功才原子发布；
   - 发布内容 = 补丁批次 + 行索引 + after 选区 + 正逆历史 + 新 revision（由 `buffer.apply` 一次完成），并更新保持打开块的基线与范围；
   - 低层失败不得泄漏半成品。
2. **契约冲突处理**：`tasks.md:84` 的验收文字写「待定组/组合/redo存在时拒绝」。其中「待定组」「组合」与 `plan_change`/`validate_change` 的 `precheck` 一致；「redo 存在时拒绝」**与** `docs/engine-design.md` §9.3／design.md 输入协议 §5（「实际新编辑才截 redo」）**可能冲突**。不要凭本表授予例外：先在 `report.md` 引用原文给出判断——若契约不支持「redo 存在即拒绝」，按契约实现（新编辑正常截 redo）并把该条登记 `待裁决`，附最小复现与代价；若契约确实要求拒绝，给出出处行号。

## 必须有的真实 DocCore 测试（不得只测 Boolean `EditContext`）

每条都先在 `report.md` 写手写期望，再跑：

1. **保护拒绝（F-02 原样）**：`"---\nt: x\n---\n"`、Visual、偏移 7 输入 `Y` 并 `commit_open_block` → 拒绝；`buffer_text()` 仍为原文；revision/历史/draft/选区/快照身份零变化。修复前必须实测通过（原为 `t: Yx`）。
2. **HTML 边界拒绝**：Visual 下在行内 HTML/样式区间内提交（用 `Parsed::protected()`/`styled()` 实取区间，不硬编码猜测）。
3. **Source 允许**：`switch_mode(Source)` 后同一位置提交**成功**，文本确实改变，且只记 1 步；随后 `switch_mode(Visual)` 往返不产生额外历史步。
4. **IME 一致**：`composition_start/update/commit` 在保护区间内被同样拒绝（零副作用、组合状态按契约保留或明确结束，写清取舍），在 Source 下成功。
5. **待定组/组合/redo 存在时的行为**逐条实测并写明：命令路径（R04b）在有待定普通输入组时、组合活动时各返回什么；redo 分支存在时实际结果是什么（与第 2 点的裁决一致）。
6. **拒绝零副作用**：用既有 `common::assert_rejected_without_change` 或等价捕获比较完整状态（文本/快照文本/draft/版本/undo/redo/节点数/组合/等待）。
7. **每键不解析**：断言 `type_text`/`composition_update` 不触发解析（可用计数探针或结构说明 + 只读检查），并在 `report.md` 给出依据。

## 适用规范（先读，再动手）

- `openspec/changes/m0-engine-foundation/design.md` 第 25、61 行与「Commit and Input Protocol」§1–6。
- `docs/engine-design.md` §8.1–8.3（尤其 §8.2 第 4–6 步「原子应用」「重解析」）与 §9.1/§9.3。
- `AGENTS.md` 铁律 1（依赖方向：`aline-ui` 可向下消费 `aline-edit` 的公开 API）、铁律 3 与 RS-02（跨 crate 坐标受检）、铁律「引擎 crate 禁止 `unwrap/expect/panic!/unreachable!/assert!`」（`aline-ui` 同样受 workspace clippy deny）。
- `docs/architecture-guide.md` 命令事务/缓存失效条款。

## 不得越界

- 不得改 `aline-edit`/`aline-markdown`/`aline-buffer` 的源码或公开契约；需要改公共契约/已确认策略时按 `docs/development-process.md` 走流程并登记 `待裁决`。
- 不得在 render/每键路径做全量解析或全文哈希；不得引入第二份可写全文、第二 History、第二套坐标转换。
- 生产代码不得新增 `unwrap/expect/panic!/unreachable!/assert!`；测试不得 `unwrap()`，用 `?` 与显式错误。
- 既有 `crates/aline-ui/tests/**` 断言不得放宽或删除；R01 的 5 条回归必须继续通过。

## 工具链与提交

全部 Cargo 命令 `--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`；`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全绿。工具缺失/离线失败记「未核实」，不联网安装。新增 `aline-edit` 依赖需要改 `Cargo.lock`——必须 `cargo check --offline` 生成并**在证据里单独登记**为「初始 lock 更新（非 `--locked` 检查）」，其余命令仍 `--locked`。提交（Conventional Commits，scope `aline-ui`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R04/run-1/`）

- `pre-fix.txt`（修复前用当前源码重跑 F-02 探针的真实 argv/cwd/时间/exit/原始 stdout，必须复现 `t: Yx`）
- `post-fix.txt`（修复后同探针原始输出 + `cargo test -p aline-ui` 与 `cargo test --workspace` 原始输出）
- `commands.txt`（每条 argv、cwd、起止 UTC、exit code；lock 更新单列）
- `design.md`（最小方案：公开边界、唯一写入口、`Parsed` 采纳/作废矩阵、预算派生、被否决的替代方案与代价、验证方式）
- `report.md`（逐条断言对照、契约裁决（含 redo 条）、未覆盖项）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status（DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED）、提交哈希、一行测试小结、新增/变更的公开 API 清单、契约裁决结论（redo 条）、concerns、证据路径。不要贴长日志。
