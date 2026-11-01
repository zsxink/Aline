# 5.1 记录：正逆补丁金样、StringBuffer 与行索引

日期：2026-10-31。状态：正逆补丁/行索引金样先于实现手写完成，`StringBuffer` 原子批次已实现，定向测试与全 crate 测试通过。本文只记录组 5 低层 buffer 原语，不宣称 V-07、M0 出口或现有文件保存通过。

## 1. 先行金样

金样位于 `step-5.1/goldens/`，生成/复核脚本为 `generate_goldens.py`，说明见 `goldens/README.md`。

| 项目 | 数量 | 来源与独立核对 |
|---|---:|---|
| 手写正逆补丁案例 | 14 | 直接按引擎 §9.1 与 buffer spec 写期望；覆盖 `abcXYZ → aQQc!`、空插入、删除、相邻/同点逆范围合并、多字节字符、CRLF 字节、no-op |
| 固定 seed 随机案例 | 120 | Python 标准库 `random.Random(20261031)`；生成器用独立逐段拼接函数重算 after 与逆补丁，并断言逆补丁恢复 before；seed 与生成日志已保存 |
| 行索引手写案例 | 11 | 空文本、无换行、LF/CRLF/孤立 CR、连续空行、多字节字符、行数预算拒绝 |

随机部分没有引入 Rust 随机/属性测试依赖，也不是实现的唯一正确性来源。`aline-buffer` 的 manifest 仍无依赖（审计见 5.3）。因为未使用随机库，本步没有“随机库最小失败样本”；若后续引入 proptest 等库，须先按仓库规则完成依赖准入。

`forward-inverse.tsv` 的坐标均为 UTF-8 字节、半开区间；正补丁使用 before 坐标，逆补丁使用 after 坐标。行索引内容范围不含末尾 LF，CRLF 的 CR 一并排除；孤立 CR 不是换行。

## 2. 实现

- `StringBuffer` 单一持有 `TextSnapshot`、`SrcSelection`、`LineIndex` 与 `History`（`crates/aline-buffer/src/string_buffer.rs:69`）。
- `apply` 先 `CheckedBatch::validate` 复核基线，再剔除内容相同的替换/空文本插入；空计划返回 `Unchanged`，不推进 revision、不产生历史、不清 redo。
- 非空计划先物化 after 文本、构造 after 坐标逆补丁、重建行索引、推进 `DocumentRevision`、校验 after 选区与历史预算；全部成功后才替换快照并提交历史（`string_buffer.rs:143` 起）。
- `line_index.rs` 按 LF 建立行起点，CRLF 不作为额外换行；`line_of_offset` 把换行字节归入当前行，文本末尾归入最后一行。
- 公开 `AppliedBatch::inverse_patches` 供独立复核/显式回滚。

## 3. 定向证据

环境：`CARGO_HOME=<cargo-home>`，`RUSTUP_TOOLCHAIN=1.98.1`，`CARGO_NET_OFFLINE=true`；macOS arm64，`rustc 1.98.1 (48a229cea 2026-09-01)`。完整环境/git 状态见 `../evidence/environment.stdout.log`、`../evidence/git-state.stdout.log`。

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T06:24:21Z | `cargo test -p aline-buffer --locked --offline patch` | 0 | `targeted-tests/patch.stdout.log` / `.stderr.log` |

该名称过滤命中 3 个测试，而不是只命中金样测试：

- `forward_inverse_patch_goldens_round_trip_after_coordinates`：断言金样总数 134，覆盖 134 例正逆补丁、after 选区、无变化分支、正补丁历史只保留实际变化、逆补丁独立回放。
- `anchors_transform_multiple_patches_by_cumulative_delta`：名称含 `patch` 被过滤一并选中。
- `history_handles_keep_trimmed_patch_resources_alive`：名称含 `patch` 被过滤一并选中。

三者均通过；不能把“过滤后 3 个测试通过”误写成“只有 1 个金样测试”或“覆盖了全部 patch 相关路径”。行索引金样由同一 target 的另一测试 `line_index_goldens_match_handwritten_boundaries` 执行，11 例通过。

全 crate 证据见 5.4：29 个测试全部通过，0 失败。

## 4. 未覆盖与边界

- 未使用 proptest/quickcheck 等随机库；120 例随机金样只说明该固定 seed 样本内往返成立。
- 未做真实大文本性能/内存测量，`TextBudget`/`LineIndexBudget` 只是注入上限。
- 未验证命令规划、IME、UI 选区映射、文件保存重开；这些不属于 5.1。
- 未证明其他 OS/target；本步只在当前 macOS 主机运行。

## 5. 独立复核

本节不写复核结论。独立复核者应检查：金样是否确在实现前手写、生成器是否独立于 Rust 实现、134 与 11 的计数是否与日志一致、`patch` 过滤是否被如实解释、是否有未准入依赖。
