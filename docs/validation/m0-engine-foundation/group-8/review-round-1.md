# 组 8 独立复核报告（Round 1）

**角色声明：** 本文档由独立 reviewer 撰写。我不是实现者，也不替实现者改代码或改期望。我的结论来自当前 git HEAD、代码、测试、日志和 git 对象本身，不承认 step-8.*/report.md、README 或 tasks.md 的勾选状态作为验收依据。

**基准：** HEAD `1dd0174`；组 8 实现与证据提交 `96c6573`；本次复核时间窗为 `2026-10-31T12:54:14Z–13:20:12Z`（UTC）。

**约束条件（用户指令）：**
- 只读窗口：除新建本文件及 `review-round-1-evidence/` 下证据外，禁止修改源码、测试、期望、tasks.md、既有证据；禁止 git add/commit/push；禁止改守卫。
- 不得改期望迁就实现。
- safety 门禁：只核对 17/19 与 `tests/common/mod.rs` 误判的既有证据是否如实记录；不得声称门禁通过；组 8 结论必须把该阻塞列为外部未决项。
- 8.5 回填准备（只准备，不落盘到正文）：不要修改这些文档。
- 最终回复简短结论 + 报告路径；不宣称 V-07/V-08/AC/M0 出口通过；不勾 tasks。

---

## 1. 方法与环境、命令与证据

### 1.1 环境快照

- **平台/Docker：** macOS arm64（无容器）
- **rustc：** 1.98.1 (48a229cea 2026-09-01)
- **cargo：** 1.98.1 (797e8a9bc 2026-08-05)
- **HEAD：** `1dd0174ca8fc4aa975209ee5d6c3169c36695fad`
- **目标 commit：** `96c65732bf69630c56dd6e72feb2505dac59abb5`（组 8 impl+evidence 提交）
- **环境变量：** `CARGO_HOME=<cargo-home>`, `RUSTUP_TOOLCHAIN=1.98.1`, `CARGO_NET_OFFLINE=true`
- **工作树状态：** `crates/aline-ui/src/document/*` 与 `tests/*.rs` 自 `96c6573` 后未改动；但有其他修改（见 §2-R1-F3）。

证据文件与 UTC/exit（全部在 `docs/validation/m0-engine-foundation/group-8/review-round-1-evidence/`）：

| 文件 | 内容摘要 | UTC | exit |
| --- | --- | --- | --- |
| `environment.log` | rustc/cargo/HEAD 快照 | 12:54:14Z | — |
| `commands.log` | 定向测试命令与定时 | 12:54:19–12:54:26Z | 0 |
| `review-document.log` | `cargo test -p aline-ui --test document` | 12:54:19Z | 0 |
| `review-document_typing.log` | `--test document_typing` | 12:54:20Z | 0 |
| `review-document_composition.log` | `--test document_composition` | 12:54:20Z | 0 |
| `review-document_results.log` | `--test document_results` | 12:54:20Z | 0 |
| `review-aline-ui-full.log` | 全 crate aligned 测试 | 12:54:26Z | 0 |
| `dependency-metadata.log` | cargo metadata：8 workspace 成员，依赖图 | 13:13:29Z | 0 |
| `engine-gpui-check.log` | `aline-buffer/edit/markdown/plugin/source/app-core` gpui_lines=0 | 13:13:40Z | 0 |
| `review-fmt.log` | `cargo fmt --all --check` | 12:59:45Z | 0 |
| `review-clippy.log` | `cargo clippy --workspace` | 12:59:46Z | 0 |
| `review-safety-head-guard.log` | `git show HEAD:scripts/check_safety.py` 复跑，4 条 failure | 13:07:40Z | 1 |
| `review-safety-current-worktree.log` | worktree guard 复跑，0 条 failure | 13:07:42Z | 0 |
| `safety-guard-worktree-modification.log` | blob mismatch / mtime / diff 记录 | 13:07:44Z | — |
| `review-literal-filters.log` | `publication_wait` `input_group` `document` 字面过滤匹配数 | 13:15:19Z | 0 |
| `review-literal-filters-extra.log` | `composition`/`typing`/`results` 字面过滤匹配数 | 13:20:12Z | 0 |

### 1.2 测试计数验证

- **定向测试：** document=22，document_typing=25，document_composition=25，document_results=17，public_boundaries=2；总计 89 例覆盖步骤 8.1–8.4。
- **全对齐运行：** `cargo test -p aline-ui` 结果为 unit=1, document=22, document_composition=25, document_results=17, document_typing=25, public_boundaries=2, doc-tests=0；全部 pass。
- **字面过滤命令：**
  - `cargo test -p aline-ui document` → 2 例（case_8_4_03_rejects_other_document_identity + input_identity_rejects_wrong_document_or_working_version），其余 22+25+25 被 filter out。
  - `cargo test -p aline-ui publication_wait` → 0 例在所有 6 个 test targets 中。
  - `cargo test -p aline-ui input_group` → 0 例。
  - `cargo test -p aline-ui composition` → 8 例匹配（均位于 document_composition.rs）。
  - `cargo test -p aline-ui typing` → 9 例匹配（分散在 document_typing/public_boundaries等）。
  - `cargo test -p aline-ui results` → 0 例。

任务字面要求未被字面满足；实际覆盖来自 `--test document*` 目标级命令。

### 1.3 依赖方向

- **workspace 成员：** 8 个（aline, aline-app-core, aline-buffer, aline-edit, aline-markdown, aline-plugin, aline-source, aline-ui）。
- **五引擎 + app-core：** aline-buffer, aline-edit, aline-markdown, aline-plugin, aline-source, aline-app-core；`gpui_hits=0` 在所有 `cargo tree` 输出中。
- **aline-ui 依赖：** aline-app-core, aline-buffer, aline-edit, aline-markdown, aline-plugin, aline-source, gpui-pre (=0.3.9)。无第九 crate；符合 task 8.1。

---

## 2. 发现（R1-F…）

### R1-F1 [高/阻断] **8.1 资源可达引用完全缺失**

- **问题：** `CurrentSnapshot` 仅含 `document`, `version`, `text`, `selection`；所有 crates 无资源/资产引用类型；无测试覆盖。spec R1/S1 的 "MUST 包含所需资源引用" 与 task 8.1 显式项均未实现。
- **影响：** 8.1 不能以 M0 范围闭合；若架构决定此项移至 M2，需要 spec/tasks 书面修订与裁决记录。
- **证据指针：** `snapshot.rs` 行 19–32（CurrentSnapshot struct）；全仓库 grep `resource\|Resource\|资源` 无文档相关类型；test 目录无 resource 引用断言。

### R1-F2 [中] **8.4 InputIdentity 缺少解析配置代次与 request id**

- **问题：** `InputIdentity::new(document, version, snapshot)` 字段仅有三者；task 8.4「错配置/旧 request」无法构造身份信封；tests `case_8_4_07`/`08` 仅占位状态列，未调用 `accept_derived_result`；TSV 结果列为“待定”。
- **影响：** 8.4 实质未验证 "error config" 和 "old request" 拒绝场景；需后续添加 identity fields 并补充测试。
- **证据指针：** `identity.rs` 行 136–167；tests 8.4-07/08。

### R1-F3 [中] **安全门禁外部未决（worktree modification observed）**

- **现状：** HEAD 守卫 blob `3578d761c739a29fa543590af625c2de2c3f176d`；我复跑得出 4 条 `test-module-resolution` failure（tests/document*.rs），exit 1，与原 summary.json 一致。但复核期间 worktree `scripts/check_safety.py` 被外部修改（blob `3c02a35...`，mtime 13:04:08Z），随 group-9/step-9.1/safety-guard-fix 目录存在（未跟踪）。modified guard 复跑 0 失败；group-9 self-test 50/50 pass（`summary.json`）。
- **影响：** README `“守卫未修改”` 仅在其时点成立（11:17Z）；当前 worktree 不成立。组 8 证据链（17/19）对 HEAD guard 仍属真实；不得据此宣称组 8 门禁通过。该事项为外部未决项。
- **证据指针：** `review-safety-head-guard.log`, `review-safety-current-worktree.log`, `safety-guard-worktree-modification.log`; group-9 目录。

### R1-F4 [低] **任务字面过滤命令与实际覆盖不匹配**

- **观察：** `cargo test -p aline-ui document` 只跑 2 例，不运行 tests/document.rs 的 22 例；`publication_wait`/`input_group` 字面过滤匹配 0。任务 8.1/8.2/8.3 的字面命名要求未被证据覆盖；实际覆盖来自 `--test document*` 目标级运行。
- **影响：** 证据链真实（commands.log 同时记录了 `--test` 命令与字面 filter），但任务字面要求的“named document/input_group 测试”与“publication_wait 过滤测试”未能直接执行。
- **证据指针：** `review-literal-filters.log`。

### R1-F5 [中] **模式切换与自动保存/恢复只有协议记录，无入口**

- **8.2：** `DocCore` 无模式切换入口；`case_8_2_23` 仅断言 state 不变且不调用任何 switch，outcome/error 列为“待定”（T-8-15）。
- **8.3：** `PublicationKind` 只有 Save/SaveAs/Export；无 Autosave 种类；`case_8_3_23` 占位；restore read via `current_snapshot` tested by `case_8_3_24`，但“恢复保护独立”仅有 protocol-level 行为。
- **影响：** 任务字面要求不能被断言实现通过；只能记为协议留证。

### R1-F6 [低] **“所有拒绝比较完整状态”不完全**

- **问题：** `capture` 函数不含 `open_block_range`、`snapshot_id`、`wait` handle value、`next_wait_id`；因此 rejection comparison 并非字面完整状态。部分用例另行断言 `snapshot_id`（如 8.4-10/11/13），但 `open_block_range` 仅在闭块态断言 none。
- **影响：** 若未来出现 block.range 变化的拒绝场景，可能不被 capture 检测。

### R1-F7 [低] **Reload 节点 source 不可观察**

- **观察：** `aline-buffer::HistoryStep::source()` 公开，但 DocCore 无历史步访问器；`case_8_4_13` 仅通过 node_count/Undo/Redo 间接验证独立边界；dg 值列为“待定”（T-8-13）。
- **影响：** M0 可接受的功能性局限，非功能缺陷。

### R1-F8 [低] **8.4-17 只断言无入口，未验证跨层回执分类契约**

- **观察：** DocCore 无写盘回执 entry；`aline-source` 有 `AtomicFailureKind`/`Written` 分类协议（已实现但未在组 8 层面消费）。
- **影响：** 任务“实际写盘回执分类协议保留”在 source 层有基础，但 DocCore 侧无测试验证。

### R1-F9 [低] **证据链时间序注记**

- **事实：** 19 门禁组长复核在 11:17Z 运行；实现提交于 12:45Z；提交后重跑的是 fmt/clippy/test（commit-gates），未重跑 safety；定向测试日志 11:13Z 与 commands.log 一致；未发现伪造/回填迹象。
- **说明：** 19 门禁早于提交；提交后未进行完整的 19 gate run。该时间序不影响组 8 证据的真实性记录。

### R1-F10/R1-F11/R1-F12 [低] **三处差异（详见第 3 节）**

- F2 开放块区间差异（expectation erratum?）、SnapshotId 复用（draft-only transitions）、Reload source unobservability — 均为已知且记录的差异，裁决结论见第 3 节。

---

## 3. 三处差异裁决

### Diff-1: F2 开放块区间（0..4 vs 4..4）

- **规范假设：** `step-8.1/expectations.md` F2 称 `new("abcd\n", (4,4))` 打开块为 `0..4`；F4 称 `new("中\n", (3,3))` 打开块为 `3..3`。
- **实现行为：** `DocCore::new` 将打开块范围设为 `selection.range()`（排序后的 anchor/head），故 `(4,4)` → `4..4`，`(3,3)` → `3..3`。F4 符合预期；F2 的 `0..4` 与自身 precondition（选区 4:4）不一致。
- **TSV：** 状态表没有 open-block-range 列；22 个测试用例均无断言 `open_block_range()` 的预期值。
- **裁决：** 实现选择 `range = selection.range()` 是语义一致的；F2 条目疑为期望笔误（expectation erratum）。但因 review 权限限制不得修改 expectations，必须由架构/规范负责人裁决是否确认该 erratum 及初始范围语义（建议明确为“初始打开范围等于选区范围”）。此差异不影响 22 项测试通过结果，但不封闭 8.1 的期望对齐问题。

### Diff-2: SnapshotId 复用（pending-group undo 前后）

- **观察：** 撤销待定草稿组不触碰 Buffer；Buffer 的 `TextSnapshot` id 重复使用；WorkingVersion 的 dg 前进，使旧身份因 `StaleWorkingVersion` 被拒。
- **影响：** 功能上正确（rejected via version check before snapshot check）；但 `SnapshotId` 不再唯一标识一个 WorkingVersion（同一 buffer content 对应多个 versions）。需 spec 澄清：SnapshotId 是 content identity 还是 version identity？建议记录为 M0 可接受的局限性。
- **裁决：** 不构成 functional failure；需 spec 澄清以避免消费者仅凭 SnapshotId 做缓存决策。

### Diff-3: Reload node source unobservable

- **观察：** `HistoryStep::source=Reload` 不在 DocCore 公开 API 暴露；测试通过 node_count/undo/redo 间接验证。
- **裁决：** acceptable M0 observability limitation；非 functional defect；若任务要求 source 断言，则记录为未实现的 accessor。

---

## 4. 逐任务结论表（8.1–8.4）

| 任务 | 结论 | M0 范围说明 | 主要缺口 | 证据/差异指针 |
| --- | --- | --- | --- | --- |
| 8.1 | **部分通过（不可闭合）** | unique Buffer/History/OpenBlock 真实现；snapshot draft/version/selection/不封口/压缩/redo 清理基本验证；baseline conflict/no open block char boundary 通过 | **资源可达引用缺失（spec MUST + task 显式项）**；open block range expectation 不一致（Diff-1） | R1-F1, R1-F10; `snapshot.rs`, `doc_core.rs new`, 22 tests |
| 8.2 | **部分通过** | 900ms ≤ boundary (899/900 merge, 901 split) 真实现；leave/escape/submit/close separation 真实现；undo pending priority/redo selection 真实现；snapshot interleave not seal/compress 真实现 | **模式切换无入口，仅协议记录（T-8-15）**；字面 named `input_group` 测试缺失（R1-F4） | R1-F3, R1-F4, R1-F5; 25 tests |
| 8.3 | **部分通过** | composition start/update/commit/cancel ImeCommit independent node cancel restores + dg advances; begin_publication_wait/request_cancel_wait/finish_publication protocol works; publish wait zero side effect (cancel request) true; restore read independent (same snapshot id during composition) 真实现 | **autosave 无入口，protocol only（T-8-12）**；字面 `publication_wait` filter matches 0（R1-F4） | R1-F4, R1-F5; 25 tests |
| 8.4 | **部分通过** | wrong doc/version/old request-by-dg/closed target/undo same text after/draft baseline conflict/stale projection reload independent boundary 真实现；reject without dropping_open preserved | **InputIdentity 缺 config generation + request id（T-8-10）**；full state comparison lacks range/snapshot id（R1-F6）；reload source unobservable（R1-F7） | R1-F2, R1-F6, R1-F7, R1-F8; 17 tests |

备注：8.5 backfill preparation 见第 5 节。

---

## 5. 8.5 回填准备

### 5.1 engine-design 插入位置建议

- **§8.1（约 256 行）**：在“四种状态各自的含义”段落后，增加：

```
**2026-10-31 M0 实现记录（组 8）：** `DocCore` 持有唯一的 `StringBuffer`（含 `Buffer` 与 `History`）、至多一个 `OpenBlock` 及 `DraftSlot` 草稿槽；`snapshot_current` 冻结最新草稿/组合候选与当前输入构成逻辑全文，返回 `CurrentSnapshot`（含 document/version/text/selection）；快照不提交/封口历史、不清 redo、不压缩；草稿基线冲突保留草稿与选区并返回 `DraftBaselineConflict`，不 `drop_open()`。资源可达引用在 CurrentSnapshot 中尚未实现（待裁决是否移 M2）。
```

- **§8.3（约 286 行）**：在事件转换表格之后增加：

```
**组 8 IME/发布等待纯状态记录：** 组合体状态机完成 start/update/commit/cancel；确认产生单步 `ImeCommit`，取消恢复组合前文本及选区且不空步；组合期显式发布建立 Pending 等待，finish 要求组合已结束才取快照；取消发布请求不结束组合、不改输入/历史/版本；自动保存调度与恢复保护无入口（协议留证）。
```

- **§9.3（组 5/7 记录后）**：增加：

```
**组 8 reload 独立边界：** `reload` 入口提供 Buffer.reload 重载模拟；Changed→after_commit revision 推进 + clear undone_typing/cache；Unchanged→不变；历史节点 source=Reload 不可观察但节点计数/undo/redo 行为验证独立边界（不合并 Typing 节点）。
```

- **§10.1（IME 表后）**：增加：

```
**组 8 IME 状态机：** update 隐式开始组合时已有待定组会先提交封口该组；candidate 更新替代不立即写 Buffer；confirm/dispatch ImeCommit 写入 Buffer；cancel 恢复组合前文本/选区；platform natural end 由宿主调用 finish_publication 完成，测试模拟而非真实平台。
```

- **§17 ED-01（实现与验收 bullet）**：更新 M2 计划备注。

- **E-T06/E-T09/E-T14 记录行（约 527 行）**：增加 组 8 覆盖率注记。

### 5.2 v1-architecture 插入位置建议

- **§5.1 状态所有权表（约 457 行）**：OpenBlock 行后加注 `DocCore` 唯一性已在组 8 落地（但 snapshot_current 资源引用尚未实现）。
- **§5.2（提交关块分离，约 460 行）**：增加组 8 实现注记。
- **§5.4（历史/保存点/IME，约 486 行）**：组 5/7 记录后增加组 8 注记（900ms、IME、reload independent boundary）。
- **§7.1 snapshot_current 行（约 546 行）**：从“仅 InputIdentity 身份信封；内容快照仅契约（第 8 组）”改为“Group 8 已落地 CurrentSnapshot 含 document/version/text/selection；资源可达引用未实现”；并在其后加 `2026-10-31 组 8 后状态补充` 段落。

### 5.3 v1-validation-plan 插入位置建议

- **V-07（约 105 行）**：在组 7 paragraph 后加：

```
**组 8 编辑输入与组合等待基础（M0）**：block open/commit 分组、IDE 状态机、ED-01 显式发布等待、Undo/Redo 交错已落实于无头测试；真实键盘/焦点/IME 候选窗/平台回调、自动保存、磁盘 IO、恢复/临时资源清单未实测。
```

- **V-08（约 138 行）**：加 M2 身份与保存设计准备注记。

### 5.4 可证明 facts & 未测清单

**facts provable by code/tests：**
- single StringBuffer/History/OpenBlock/DraftSlot in DocCore (§5.1);
- snapshot includes latest draft via TextSnapshot::with_overlay; does not commit/seal/redo-cleanup (§8.1);
- 900ms merging rule: <= boundary implemented and tested (8.2-01..03);
- commit_close separation (leave_block, escape, commit_open_block);
- undo pending first / redo restores after-selection (8.2-15..19);
- composition updates candidate in overlay; ImeCommit produces single History node; cancel restores pre-composition state + advances dg; publication wait kinds/status/cancel behavior (8.3-01..22);
- identity rejection order: closed_target → document → version → snapshot; stale_result via version or snapshot mismatch; baseline_conflict retains draft/select (8.4-01..12,10,11);
- reload forms independent history boundary (node counts change, undo/redo behaviors) (8.4-13..16);
- dependency graph: 5 engines + app-core have zero gpui usage, seven members total.

**real input/save/restore/resource migration untested：**
- real keyboard/focus/platform IME candidate window/marked text rendering (true IME event flow);
- real manual save/save-as/export, disk IO, write receipt consumption, file coordinator orchestration;
- autosave scheduling, 10s recovery records, temporary resource manifests;
- resource references in snapshot (not implemented at all);
- mode switch entry point & true view switching semantics;
- cross-block commands, formatting, find-replace;
- close-window & multi-window lifecycle;
- Windows/Linux cross-platform validation.

**M2 judgment：** 代码结构基础支持 M2 消费同一历史/状态协议，不存在第二子编辑器历史（单一 Buffer、单一 OpenBlock、单一 DraftSlot、单一 composition/wait state）。但 M2 仍需：
- 新增身份键：parse_config generation + request id 字段；
- snapshot 增加资源引用字段；
- host-wiring 的真实 save/restore/IO 连接；
- apply_change/command plan consumer 入口。

---

## 6. 未覆盖/未执行

1. **真实 IME 事件流**：候选窗/标记文本绘制/平台组合回调时序（pure state 模型除外）。
2. **真实保存/导出事务**：磁盘 IO、写盘回执、文件协调、权限复制。
3. **自动保存**：调度策略、外部冲突提示、恢复记录与临时资源清理。
4. **资源迁移**：图片关联、临时图片隔离、另存为资源复制、路径变更同步。
5. **模式切换**：DocCore 无入口；仅协议留证。
6. **跨块命令/查找替换/格式化**：未实现命令规划入口。
7. **多窗口/关闭生命周期**：UI Workspace 层级。
8. **平台广度**：目前 macOS arm64；Windows/Linux 未测。
9. **安全门禁 gate closure**：HEAD guard still blocks；worktree modified externally under option A；组 8 不宣称门禁通过。

---

## 7. 不宣称

- **不宣称** V-07、V-08、AC-M0 出口通过；
- **不勾** tasks.md 中的 8.1–8.4；
- **不提交** git changes（只允许创建本文件与证据目录）。

---

**附录：关键文件与测试一一对应**

- **8.1：** 22 TSV rows ↔ `tests/document.rs`: `case_8_1_01`…`22` (confirmed name-to-row mapping).
- **8.2：** 25 TSV rows ↔ `tests/document_typing.rs`: `case_8_2_01`…`25`.
- **8.3：** 25 TSV rows ↔ `tests/document_composition.rs`: `case_8_3_01`…`25`.
- **8.4：** 17 TSV rows ↔ `tests/document_results.rs`: `case_8_4_01`…`17`.

Column-by-column automated numeric verification was limited due to fixture-based test structures; name mapping is one-to-one for all 89 cases. Representative columns were manually sampled and confirmed.

---

**报告路径：** `<repo>/docs/validation/m0-engine-foundation/group-8/review-round-1.md`

**结论（最终回复）：**
- **8.1：部分通过（不可闭合）** — 资源可达引用缺失（spec MUST）; open block range expectation inconsistency needs owner adjudication.
- **8.2：部分通过** — 900ms grouping/leave/escape/undo-redo verified; mode switch only protocol entry.
- **8.3：部分通过** — composition state machine + publication waits verified; autosave/restore only protocol.
- **8.4：部分通过** — identity rejection (closed/target/baseline/conflict/projection) verified; missing config generation + request id; reload source unobservable; full-state comparison not literal complete.

**阻断项：** safety gate blocked by HEAD guard misjudgment (external unresolved; worktree modified outside my window under group-9 effort); **不得声称门禁通过**。

**8.5 回填：可做** — 提供具体 insertion points in engine-design §8–10/17, architecture §5/7.1, validation-plan V-07/V-08; list of facts provable and items untested as documented above.

**报告路径：** `docs/validation/m0-engine-foundation/group-8/review-round-1.md` (this file). Evidence files in `docs/validation/m0-engine-foundation/group-8/review-round-1-evidence/`.

---

*Review completed within user constraints; no modifications made to source/tests/expectations/tasks.md/git objects.*
