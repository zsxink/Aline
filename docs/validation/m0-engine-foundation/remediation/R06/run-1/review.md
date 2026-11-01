# R06 独立复核（run-1）

复核者未参与实现；以下证据均由本轮独立构造，未采信 `report.md` / `handoff.md` 的结论。

## A 结论

**APPROVE_WITH_CONCERNS** —— F-09 / F-15 的目标缺陷确已修复，真实仓库扫描集合与 53 项既有断言无放宽；但「成员内符号链接指向目录外一律拒绝」的声明被证伪（目录符号链接仍静默漏扫），且 `report.md` §5 第 4 条与事实不符。

## B 逐问题独立结论与自建证据

### Q1 F-09 真修复

自建 8 成员夹具 `<tmp>/r06review.qtrajS/fx-lib`（`crates/aline-buffer/Cargo.toml` 写 `[lib] path = "../../outside.rs"`，`outside.rs` 含 `v.unwrap()`）。旧脚本由 `git show 340152c:scripts/check_safety.py` 导出，同目录同时导出 `safety_lex.py`：

- 旧：exit 0，`failures=[]`，`scanned=8`，`outside.rs` 不在 `scanned`；
- 新：exit 1，唯一规则 `external-target-path`，detail `lib .../crates/aline-buffer/../../outside.rs`，outside 未被扫描。

`build = "../../outside-build.rs"` 夹具重复：旧 exit 0 / `failures=[]`；新 exit 1 / `external-target-path`，detail 为 `custom-build .../outside-build.rs`。两条具体路径均 fail-closed（只拒绝、不受审，符合契约二选一）。

### Q2 F-15 真修复

新脚本（三项旧脚本均 exit 0、`failures=[]`）：

| `--root` | exit | 规则 / detail |
|---|---|---|
| 不存在路径 | 1 | `root-missing` |
| 目录无 Cargo.toml | 1 | `root-not-workspace`（"root has no Cargo.toml"） |
| 成员清单目录 | 1 | `root-not-workspace`（`workspace_root=.../fx-lib root_is_package=True`） |
| `members = []` | 1 | `empty-scan` + 8×`missing-member` |

### Q3 是否放宽既有断言

- 独立运行：旧脚本（`340152c`）自测 53 项全过；新脚本 60 项全过。旧 53 个 name 全部存在；逐项比较 expected_rules，**只有**题述两例从 `['member-package-identity']` 变为 `['member-package-identity','missing-member']`；diff 中无其它 case 元组被删除。
- 独立复算 `missing-member`：实际夹具的 `crates/` 下无 `aline-source`，`aline-ui` 清单包名为 `aline-source`；实测 cargo 对同名双包直接 exit 101（`two packages named 'aline-source'`），故规范目录不可能再承载第二个 `aline-source`。该夹具下 `missing-member` 是必然结论。
- 判定：**加强**（期望规则集变大，无规则被删或削弱），非放宽。但夹具已由单 crate 改为完整八成员，一例同时绑定两个真实缺陷；建议后续拆出独立 `missing-member` 用例（非阻断）。

### Q4 扫描范围无缩小

`--root .`：旧/新均 exit 0、`failures=0`、`scanned=89`；`(file, kind)` 对称差集为空；八成员与 4 种 kind 均覆盖。无缩小、无 kind 漂移。

### Q5 断言有效性

5 处自设变异全部变红并命中预期（C 节）；另发现 2 处**存活变异**，说明两处新规则无 case 覆盖。变异副本均置于 `scripts/_rev_m*.py`，跑后立即删除，`git status scripts/` 为空。

### Q6 fail-closed 闭环

- 代码：所有失败路径只 append failure，`main` 以 `1 if result['failures'] else 0` 退出；`except (OSError, ValueError, KeyError)` 统一返回 exit 1，未见吞掉后 exit 0 的分支。
- 实测：cargo shim 退出 42 → exit 1 `workspace-metadata-failure`；shim 输出非 JSON 且退出 0 → exit 1 同规则；真实坏 workspace → exit 1 同规则。
- `targets` 为空：代码无显式守卫；shim 返回合法 JSON 且 `targets=[]` 时脚本 exit 0、`scanned=8`（外部 target 重新不可见）。真实 cargo 对无 target 包直接失败（"no targets specified in the manifest"），故当前不可经真实 cargo 触发，列为纵深防御缺口。

### Q7 未覆盖范围是否诚实

- §5 第 1、2、3、5、6 条属实。第 5 条实测：空 `CARGO_HOME` 下 metadata 与整脚本均 exit 0、`scanned=89`；第 6 条：提交无 `*.rs`/`*.toml` 改动，`safety_lex.py` 0 行改动，`commands.txt` 无 cargo test/build。
- 第 4 条**不属实**：仓库唯一 `build.rs` 是 `crates/aline-markdown/src/build.rs`，旧脚本 rglob 已扫，新旧均为 `engine-production`；成员内 build.rs 不存在「范围扩大」。
- 未声明残留：见 D1、D2、D4。除这些外，未发现相对旧脚本的新增行为盲区。

### Q8 自测不再污染仓库

- 代码：`safety-fixtures` 仅被读取 `cases.json`；所有夹具/fake root 写入 `tempfile.mkdtemp`。
- 实测：将模块 `ROOT` 重定向到临时树后，以**省略 `--output-dir`** 跑 `self_test`；运行后 fixture 目录只剩 `cases.json`，结果文件全部落在临时树 `safety-results`。
- 仓库 `git status -- safety-fixtures` 为空，无本会话新文件。默认 `--output-dir` 仍写受跟踪 `safety-results`，旧版亦然，属证据输出设计，不是夹具回归。

## C 变异记录（自设，非 report §3）

| 变异 | 结果 |
|---|---|
| M1 `inside()` 恒返回 True | exit1；红：`external-lib-target-path`、`external-build-script-path` |
| M2 删除 tests/ 集成 crate 根判定 | exit1；红：`integration-test-crate-root`、`integration-nested-module-resolution`、`integration-sibling-file-is-own-root`、`integration-old-wrong-location-is-rejected` |
| M3 `test_files \|= test_targets`（去掉生产优先） | exit1；红：`production-consumer-wins`、`binary-and-lib-shared` |
| M4 `package is None` 改为静默 continue | exit1；红：`member-not-in-workspace` |
| M5 所有 `mod` 声明都按 cfg(test) 处理 | exit1；红：`filename-is-not-test-scope`、`nested-module-does-not-authorize-sibling`、`production-consumer-wins`、`binary-and-lib-shared` |
| M6 静默跳过 `target-path-missing` | **存活**：exit0，无 case 变红 |
| M7 静默跳过符号链接文件拒绝分支 | **存活**：exit0，无 case 变红 |

M2–M5 与 report §3 四处无对应；M1 与报告第一处效果同域，但改的是 `inside()` 实现而非检查条件。

## D 发现的缺陷

1. **中：目录符号链接逃逸仍 fail-open，且与代码/报告声明矛盾。** 自建夹具：`crates/aline-buffer/src/linkdir -> <tmp>/.../outside-dir`，`outside-dir/mod.rs` 含 `v.unwrap()`，`lib.rs` 写 `mod linkdir;`。新脚本 exit 0、`failures=[]`、`scanned=8`，outside 文件未被扫描；`rustc 1.98.1 --edition=2024` 实际编译了它（warning 指向 `src/linkdir/mod.rs:2`）。原因：Python 3.9 `Path.rglob('**')` 不下降目录符号链接，拒绝分支只覆盖「被枚举到的文件符号链接」；`mod` 解析虽找到 `linkdir/mod.rs`，但 `production_targets` 从未加入扫描集合。旧脚本同样 exit 0（非本提交引入的回归），但本提交的代码注释与 §2.1 表格声称「成员内符号链接指向目录外 → external-target-path 拒绝」，该声明不成立，§5 也未登记。
2. **低：两处新规则无自测断言。** M6/M7 存活证明 `target-path-missing` 与符号链接文件拒绝均无 case 覆盖；自测中没有任何符号链接夹具。
3. **低（文档）：`report.md` §5 第 4 条错误**，见 Q7。
4. **低（纵深防御）：`targets=[]` 无显式守卫**，仅依赖 cargo 自身拒绝无 target 包。

## E 残留与需主 Agent 裁决项

- D1 是否阻断：我倾向登记为已知限制并开后续项，不阻断本批——F-09 契约针对 Cargo target/build script，具体路径确已修复，且该逃逸旧版已存在、非回归；但必须删除或修正过强声明。
- D2：建议补 `target-path-missing` 与符号链接（文件/目录）自测，否则这些分支今后可静默退化。
- D3：合并前修正 `report.md` §5 第 4 条。
- Q3 夹具合并两缺陷：可选拆分，非阻断。
- 非阻断：`self_test` 的 `tempfile.mkdtemp` 从不清理（临时目录泄漏）。

## F 未覆盖范围复核

- 报告 §5 六条：1 属实（只拒绝不受审，实测 outside 不在 `scanned`）；2 属实（`--no-deps` 依赖树、过程宏/别名、`include!`、非 .rs 模块入口确未覆盖）；3 属实（成员清单当根被 `root-not-workspace` 拒绝，不向上找根）；4 不属实（见 D3）；5 属实（空 `CARGO_HOME` 实测通过）；6 属实（无 Rust 改动）。
- 未声明的残留：D1 目录符号链接；D2 两规则无断言；D4 `targets=[]` 依赖 cargo 自身拒绝。
- 未发现相对旧脚本的新增行为盲区：真实仓库新旧 `scanned` 89 条完全一致，新增 target 校验只会更严。

（取证临时目录 `<tmp>/r06review.qtrajS` 已按要求清理。）
