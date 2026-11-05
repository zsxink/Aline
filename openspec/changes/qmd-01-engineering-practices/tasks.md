# Tasks

本 change 只记录 QMD-01 的工程/验证方法采纳并校正文档口径，不实施 W1～W7。任务勾选只表示文档/规格动作完成，不表示任何 V/AC、平台能力或 W 项通过。commit/push/archive 需用户另行明确指令。

## 1. 建档与来源校正

- [x] 1.1 创建 `openspec/changes/qmd-01-engineering-practices/`，写入 `.openspec.yaml`（`schema: spec-driven`，`created: 2026-11-06`）、proposal、design 与本 tasks；在 proposal 中写明“只追认文档回写、不实施 W 项”。
- [x] 1.2 在 QMD-01 方法表增加来源列，逐项区分 quiet-md 已有、Aline 既有、Aline 追加/补强；把无证据的 quiet-md 行为标为“未确认/仅方向参考”，删除“采纳 quiet-md 的冷/热”等错误归因。
- [x] 1.3 修正研究文档的事实与重复：保存基线/锁失败语义、flush 与退出路径、派生顺序、表格裁剪范围、主题 `allow(dead_code)`、Markdown 粘贴、GPUI glyph 版本限定、`native_window` 路径、E01～E12 重复索引。

## 2. 统一证据字段与阶段口径

- [x] 2.1 把 fuzz 失败证据统一为 `seed、步号、操作日志、最小输入`：`docs/engine-design.md` E-T20 与 §14 段落、`docs/v1-architecture.md` §10.1、`docs/v1-validation-plan.md` V-07、`docs/rust-style-guide.md` §12.2。
- [x] 2.2 把 E-T20/架构 §10.1 的阶段改为“M1 起（W3 立项后），M2/M3 扩展”，并注明不追溯改变 M0 验收。
- [x] 2.3 在 `docs/ui-spec.md` §6 与 `docs/decisions.md` QMD-01.4 补“先等待平台组合完成结果，等待期间保留原上下文”；`docs/engine-design.md` 已含该语义，保持。

## 3. 保存/恢复与文档一致性

- [x] 3.1 合并 `docs/v1-architecture.md` §7.3 重复的保存序列，保留一条“至少核对”清单并保留目录同步/权限/Windows 占用边界；R-15-13 同步同一口径。
- [x] 3.2 把“不产生用户可见 `.bak`/`.corrupt-*`”改为“恢复快照写应用管理的独立目标，不以文档旁 `.bak`/`.corrupt-*` 作为恢复手段”，同步架构规范、架构 §7.3/§7.4、V-08 与 QMD-01.3；研究文档的不采纳清单保持原意。
- [x] 3.3 补 `S-12-06` 到 Rust 规范 §17 的 C-03/C-04 检查组；给 `docs/v1-architecture.md` 状态行加“截至 2026-11-05”限定；统一 QMD-01 与开发流程的 W4/W5 阶段措辞。

## 4. OpenSpec 与工程基线同步

- [x] 4.1 新增 `specs/engineering-governance/spec.md` delta，覆盖来源/效力、随机与性能证据、保存恢复、IME/坐标、UI token/中文排版/离屏证据、W0～W7 立项门。
- [x] 4.2 同步 `openspec/CODE_STANDARDS.md` §6：引用 QMD-01/本 change，增加方法证据行，明确未建立门禁不得写成已启用。
- [x] 4.3 在 `docs/decisions.md` QMD-01.2 效力段、`docs/development-process.md` §3.1、`docs/README.md` 与 `AGENTS.md` 标注本 change 是治理入口、研究文档非权威、W0～W7 未立项。

## 5. 校验

- [x] 5.1 运行 `openspec validate qmd-01-engineering-practices --strict --no-interactive`，再运行全量校验；记录实际输出，失败则修正格式而非删除要求。
- [x] 5.2 运行文档链接/锚点检查、`git diff --check` 与绝对路径扫描；确认无 `Cargo.toml`/`Cargo.lock`/`crates/`/`app/`/主规格改动。
- [x] 5.3 人工核对 fuzz 字段、阶段、保存序列与 `.bak` 措辞在各文档一致；确认未声称任何 V/AC 或 W 项通过。

## 6. 后续（不属于本 change）

- [ ] 6.1 W1 离屏视觉证据：先做依赖准入与能力 spike，另建 change。
- [ ] 6.2 W2 中文排版与字体、W3 随机不变量、W4 保存/恢复、W5 IME、W6 语义 token、W7 工程门禁：各自按范围另建 change，未批准不写代码。

## 7. 执行记录（2026-11-06）

- `openspec validate qmd-01-engineering-practices --strict --no-interactive` → `Change 'qmd-01-engineering-practices' is valid`。
- `openspec validate --all --strict --no-interactive` → `9 passed, 0 failed`。
- 文档链接/锚点检查 → `checked_links=643`、`LINKS_OK`。
- `git diff --check` → 无输出（通过）。
- 绝对本地路径扫描（用户目录、主目录、Windows 用户目录、file URL 前缀）→ 无命中。
- `git diff --stat` → 仅 `AGENTS.md`、`docs/`、`openspec/CODE_STANDARDS.md` 与新增 change 工件；无 Cargo、`crates/`、`app/` 或主规格改动。

以上只证明文档与规格结构；未运行 Cargo 测试、未执行 V 项、未取得平台证据。原始命令输出按开发流程 §5.1 留在未跟踪的 `logs/validation/qmd-01-engineering-practices/2026-11-06-run-01/`。
