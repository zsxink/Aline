# R08 派发契约：F-11 删除/粘贴/HTML 跨界的独立负例

基线 HEAD：派发时 `git rev-parse HEAD`（须晚于 R04 的提交）。仓库根 `/Users/xian/Project/editor/Aline`。前置 R04 已落地（DocCore 已实际消费 `plan_change`/`validate_change`）。

## 文件所有权（只有你可写这些文件）

- `crates/aline-edit/tests/protection.rs`、`crates/aline-edit/tests/refusals.rs`（仅追加用例）
- `crates/aline-edit/tests/crossing.rs`（新建，推荐；表驱动负例集中在此）
- `docs/validation/m0-engine-foundation/remediation/R08/run-1/`（含你新建的 TSV 金样与证据）

**不得写** `crates/aline-edit/src/**`（本批只补覆盖，不改实现——若判定实现有缺陷，登记 `待裁决` 并给出最小复现，不要自己改引擎）。不得写 `crates/aline-ui/**`、`crates/aline-markdown/**`、`scripts/**`。不得 `git reset`、不得回退他人改动。

**关键**：`docs/validation/m0-engine-foundation/group-7/step-7.1/goldens/edit-goldens.tsv` 是**历史金样**，不得改写或追加。你的新金样放在 `docs/validation/m0-engine-foundation/remediation/R08/run-1/goldens/edit-goldens.tsv`，由新测试文件加载。

## 失败事实（F-11，严重，维度 D3/D7）

`docs/validation/m0-engine-foundation/group-7/step-7.3/report.md` 第 10、28、31 行自认的缺口，经 round-2 复核（`review-round-2.md:49`、`:117`）判定成立：

- **MD-11 粘贴无独立用例**：粘贴被论证为「同一 `Replace` 路径」，因此没有独立断言。
- **HTML 跨界只有左端一条**：`refuse_format_html` 仅覆盖 5..6 触及 2..5 保护区间的**右端**；右端越界、跨整个 HTML 元素的格式命令无独立金样。
- **空删除**（`Replace` 替换为空串，即删除）没有与「有内容替换」分开的独立用例。

这三项都是「scenario 被登记但未独立验证」，不是实现缺陷的证据——本批目标是把它们变成**可失败的独立断言**。

## 目标

1. **空删除独立用例**：`EditCommand::Replace` 把选区内容替换为 `""`，在下列位置各一例，每例独立断言：
   - 正文内正常删除 → `Changed`，补丁与目标选区按字节规则手算写死；
   - 删除恰好跨入受保护区一个字节 → `ProtectedRange` 拒绝，且拒绝前后完整状态不变；
   - 删除整个受保护区间 → 拒绝（Visual）；`Source` 模式下 → 允许且只记 1 步历史。
   - 删除零长度（`range` 为空、替换为空）→ 判定结果并写明依据（`Unchanged` 还是拒绝），不要含糊。
2. **独立粘贴用例**：把「粘贴」建模为一次 `Replace`，但要**独立成例**，不得复用删除/替换断言：
   - 在受保护区间左/右相邻处粘贴（不越界）→ 允许；
   - 粘贴内容**覆盖**受保护区 → 拒绝，`ProtectedRange`；
   - 粘贴到 `Source` 模式同一位置 → 允许；
   - 剪贴板多行文本（含 `\n`）粘贴 → 断言补丁范围与目标选区，不得只断言「成功」。
3. **HTML 跨界补全**：
   - 格式命令（`ToggleInlineFormat` Strong/Strike）触及 HTML 保护区间**右端** → 拒绝，断言 `HtmlBoundary` 携带的 `range` 精确值（不是只断言错误类别）；
   - 格式命令**跨整个 HTML 元素**（选择区间完全包住 `<b>x</b>`）→ 拒绝并断言 range；
   - 编辑恰好**落在** HTML 标签内部（`<b` 与 `b>` 之间）→ 拒绝；
   - 每个用例都用 `Parsed` 实取保护区间，**不得硬编码猜测偏移**；在 `report.md` 写出该夹具的实际 `protected()`/`styled()` 区间作为依据。
4. **比较真实完整状态**：每条拒绝用例都用完整状态快照比较（`text / anchor / head / revision / undo_depth / redo_depth / nodes`），沿用 `refusals.rs` 既有 `capture()` 形状。不得只断言 `is_err()`。
5. **表驱动 + 独立预期**：新金样 TSV 的每一行预期（补丁区间、目标选区、拒绝类别与 range）必须由你按字节手算写死，**不得先跑实现再把输出抄进 TSV**。`report.md` 给出每行的推导（源字节区间 → 期望补丁 → 期望选区）。
6. **不得回归**：`cargo test -p aline-edit` 全部既有用例（`protection` 4、`refusals` 16、`goldens`）保持通过且断言不放宽；历史 TSV 一个字节不动。

## 适用规范

- `docs/engine-design.md` §7（保护/HTML 边界、Source 放行）、§9.3（拒绝不产生空步、不无故清 redo）。
- `openspec/changes/m0-engine-foundation/design.md:86`（「拒绝相对调用前完整状态零副作用…无变化不记步、不清 redo」）与 `:100`（非法输入保留文本/草稿/组合/选区/undo/redo）。
- `docs/validation/m0-engine-foundation/group-7/step-7.3/report.md`（现行覆盖与未覆盖清单）。
- `AGENTS.md` 铁律 3、RS-02；workspace clippy deny（测试同样不得 `unwrap/expect/panic!/assert!`——注意 `assert_eq!` 属 `assert!` 族，**仓库既有测试已大量使用**；本批沿用既有测试风格，不因该 deny 而放弃断言，但**不得新增 `unwrap()/expect()`**）。

## 约束

- 全部 Cargo 命令 `--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`；工具缺失/离线失败记「未核实」，不联网安装。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全绿。
- 提交（Conventional Commits，scope `aline-edit`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R08/run-1/`）

- `pre-fix.txt`（新增用例在**不改实现**前提下的实际运行结果：哪些通过、哪些失败；F-11 的缺口是覆盖缺失，所以「全部通过」也是合法结论，但必须逐条列出并说明缺口本来的风险）
- `post-fix.txt`（最终 `cargo test -p aline-edit --locked --offline` 与 `cargo test --workspace` 原始输出）
- `goldens/edit-goldens.tsv`（手写推导的新金样）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（逐用例覆盖表、夹具实取区间、每行金样推导、未覆盖项、若发现实现缺陷的 `待裁决` 登记）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status（DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED）、提交哈希、一行测试小结、新增用例数（按空删除/粘贴/HTML 三类分列）、是否发现实现缺陷、concerns、证据路径。不要贴长日志。
