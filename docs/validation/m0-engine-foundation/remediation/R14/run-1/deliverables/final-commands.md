# R14 最终命令与测试差异

仓库：`<repo>`。实际 HEAD：`96024867f7ae949d7328232b05d93ed5bc6033d9`。独立门禁环境覆盖：`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`、`PYTHONDONTWRITEBYTECODE=1`；九项命令的 JSON 同时记录 argv、cwd、UTC 起止、exit 与实际覆盖环境。该组命令未设置 `CARGO_HOME`，Cargo 使用主机默认 `<cargo-home>`。

## 九项门禁

| # | 命令 | exit | 原始证据 |
|---|---|---:|---|
| 1 | `cargo fmt --all --check` | 0 | `../gate/01-fmt.command.json`、`.stdout.log`、`.stderr.log` |
| 2 | `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 0 | `../gate/02-clippy.*` |
| 3 | `cargo test --workspace --locked --offline` | 0 | `../gate/03-test.*` |
| 4 | `python3 scripts/check_engineering.py --self-test` | 0 | `../gate/04-engineering-self-test.*` |
| 5 | `python3 scripts/check_lints.py --self-test` | 0 | `../gate/05-lints-self-test.*` |
| 6 | `python3 scripts/check_safety.py --self-test` | 0 | `../gate/06-safety-self-test.*` |
| 7 | `python3 scripts/check_public_api.py --self-test` | **1** | `../gate/07-public-api-self-test.*`；脚本选择准入登记的隔离 `CARGO_HOME`，离线 registry index 缺少 `rust-embed`，8 个编译案例的 lock 均未生成；不可算作代码违规已拒绝或通过。 |
| 8 | `openspec validate m0-engine-foundation --strict --no-interactive` | 0 | `../gate/08-openspec.*` |
| 9 | `git diff --check` | 0 | `../gate/09-diff-check.*` |

独立全 workspace test 汇总：48 个 harness summary，361 passed、0 failed、0 ignored。旧最终门禁的原始基线为 312 passed（`../../../../group-9/step-9.1/workspace-gates-after-cleanup/test.stdout.log`）。按当前与旧 stdout 中实际 `test <name> ... ok` 名称多重集逐项比较：新增 49、删除 0、同名改写 0；这只能说明测试名的集合差，不声称历史语义“改名”不存在。49 个新增名称如下：

```text
compose_anchors_second_level_empty_sources_without_union
compose_keeps_hidden_anchor_outside_non_exact_merged_piece
compose_pairing_rules_for_empty_ranges_are_explicit
compose_synthetic_source_anchors_stay_empty_at_each_boundary
compose_with_identity_rewrite_preserves_every_first_level_segment
compose_zero_source_pieces_anchor_to_empty_points_only
crossing_golden_cases
document::doc_core::checked_commit_tests::command_history_budget_failure_keeps_whole_owner_state
document::doc_core::checked_commit_tests::command_inline_format_consumes_same_projection_and_records_one_step
document::doc_core::checked_commit_tests::command_precheck_refuses_real_pending_or_composition_with_redo_untouched
document::doc_core::checked_commit_tests::command_refuses_stale_identity_selection_or_projection_without_state_change
document::doc_core::checked_commit_tests::command_source_multi_patch_is_not_restricted_by_input_patch_budget
document::doc_core::checked_commit_tests::command_success_is_one_atomic_step_and_invalidates_old_redo
document::doc_core::checked_commit_tests::command_wrong_mode_rejects_before_publish_and_preserves_redo
document::doc_core::checked_commit_tests::legal_input_publish_clears_redo_only_when_committed
document::doc_core::checked_commit_tests::no_change_or_cancelled_composition_preserves_redo_and_history_steps
document::doc_core::checked_commit_tests::protected_pending_group_cannot_bypass_checked_commit_at_any_boundary
document::doc_core::checked_commit_tests::source_typing_and_ime_allow_protected_and_hidden_source
document::doc_core::checked_commit_tests::visual_hidden_marker_creation_retains_existing_replace_refusal
document::doc_core::checked_commit_tests::visual_html_input_rejection_uses_parsed_boundaries_and_keeps_state
document::doc_core::checked_commit_tests::visual_ime_protection_rejection_preserves_complete_state
document::doc_core::checked_commit_tests::visual_typing_and_ime_use_exact_mapped_visible_insertion
document::doc_core::checked_commit_tests::visual_typing_protection_rejection_preserves_complete_state
dump_structure_checks_ignore_sensitive_words_in_drawn_text
e3_basic_fixture_dump_matches_independent_expectation
e3_blocks_and_gaps_partition_source_without_overlap
e3_drawn_text_matches_visible_source_and_length
e3_hidden_aligned_and_synthetic_segments_are_present
e3_protected_ranges_match_independent_expectation
e3_segments_match_independent_table_one_by_one
e3_source_identity_is_byte_derived_not_snapshot_identity
f04_translate_must_reject_half_char_old_offset
grapheme_and_scalar_boundaries_verified_separately
grapheme_goldens_move_both_directions_and_reject_scalar_interior
long_clusters_and_ri_context_refuse_within_supply_budget
mapping::grapheme::tests::supplied_bytes_are_charged_before_every_call_and_include_retries
old_and_partial_projections_are_rejected
pending_input_commit_invalidates_redo_branch_without_empty_step
pending_redo_restores_collapsed_selection
pending_redo_restores_directional_selection
pending_redo_restores_forward_selection
redo_must_not_discard_pending_input
scalar_utf16_and_grapheme_boundaries_are_distinct
translate_accepts_in_block_endpoints_and_is_byte_exact
translate_rejects_outside_old_block
translate_rejects_wrong_new_snapshot
translate_rejects_wrong_old_snapshot
unicode17_indic_version_delta_is_explicit
visual_navigation_maps_back_only_through_projection
```

## 统一入口与正常范围检查

`scripts/check_workspace.py` 的 CodeGraph 读取确认实际 CLI 为 `--cargo-home`、`--output-dir`。按入口契约使用准入登记缓存 `<tmp>/aline-m0-admission-k4_p5j6a/cargo-home`，命令为：

```text
python3 scripts/check_workspace.py --cargo-home <上述隔离缓存> --output-dir docs/validation/m0-engine-foundation/remediation/R14/run-1/gate/unified-captured
```

顶层实际 argv/cwd/UTC 起止/exit/覆盖环境及 stdout/stderr 在 `../gate/unified-captured/launcher.command.json`、`launcher.stdout.log`、`launcher.stderr.log`。入口返回 1。子命令为 19 项并逐条保留 JSON、stdout、stderr：toolchain/version/config、fmt 均 exit 0；clippy/test 均 exit 101；正常依赖检查 exit 1；engineering/lint/safety 三个 self-test、lint/safety 正常检查均 exit 0；public API self-test exit 1。Cargo clippy/test/dependency/public API 失败原因为指定离线缓存缺 `rust-embed`，不是源代码错误诊断；故本次对该缓存上的构建、依赖闭包与公开 API 为**未核实**。

另有脚本直接正常范围检查：lint exit 0、safety exit 0；engineering 及 public API normal 均因同一离线缓存缺包/metadata 为空而未核实。详情见 `../gate/unified-captured/`。公开 API 独立 self-test 默认选择同一 admission cache，失败记录在 `../gate/07-public-api-self-test.*` 与系统临时目录原始输出线索；R14 未联网安装或升级依赖。catalog 本轮通过实际 `cargo test --workspace` 发现并执行，包含 app-core 正反例及 UI 消费测试；未据此宣称完整 UI catalog 行为验收。

OpenSpec 严格校验、格式、workspace Clippy/test 与 `git diff --check` 的九条原始结果都在 `../gate/`。CodeGraph 发现 240 个新增证据文件和 4 个修改脚本输入，已同步；`../gate/10-codegraph-sync.*`、`11-codegraph-status.*` 记录二次 sync `Already up to date` 与 status `Index is up to date`。

## 独立引擎范围复核命令

reviewer 独立运行 `cargo test -p aline-buffer -p aline-markdown -p aline-edit -p aline-ui --locked --offline --quiet`，Rust 1.98.1、offline、`CARGO_HOME=<cargo-home>`，exit 0，321 passed。reviewer 原始输出和复核范围随后由主 Agent记录在 `../reviews/`；这 321 项与 workspace 361 项有重叠，不相加。
