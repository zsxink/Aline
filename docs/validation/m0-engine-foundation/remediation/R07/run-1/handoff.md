# R07 handoff（F-10 字素边界与标量边界分开验收）

## 交付物
- 代码：`crates/aline-markdown/tests/mapping.rs`（+293 行，16 → 17 条用例；**仅测试**，`src/**` 零改动）
- 证据：`docs/validation/m0-engine-foundation/remediation/R07/run-1/`
  （`pre-fix.txt`、`post-fix.txt`、`commands.txt`、`self-proof.txt`、`report.md`）

## 一句话结论
新增表驱动用例 `grapheme_and_scalar_boundaries_verified_separately`，用 9 行**手写**夹具把
「映射层接受集合 = 标量边界」与「用户字符边界 = 字素簇边界」分开断言，差集逐元素列出，
对照组（中文 / 重复空白 / CRLF）断言差集为空——两套边界不再混为一谈。

## 关键裁决
1. **不实现字素分段**。仓内无 UAX #29 实现、无分段依赖；契约亦明令本批不得新增依赖、不得手写完整
   UAX #29、不得新增公共 API。夹具里的两套边界是**测试常量**，不是可复用实现。
2. **CRLF 不跳过**。实测 `parse` 保留快照原文，但绘制层把 `\r\n` 折成单个 `\n`、`\r` 归入
   `Aligned`（`drawn 1..2 / src 1..3`），故该类字素边界等于绘制标量边界，作为第 9 行正常断言。
3. **「相等」也断言**。对照组不靠「假设相等」，而是断言「差集为空 ⟺ 两套集合相等」。

## 验证（全部实跑，工具链 1.98.1，`--locked --offline`，`CARGO_NET_OFFLINE=true`）
- `cargo test -p aline-markdown --test mapping` → 17 passed / 0 failed，exit 0
- `cargo test -p aline-markdown` → 12 段 `test result: ok`，合计 118 passed / 0 failed，exit 0
- `cargo clippy -p aline-markdown --all-targets -- -D warnings` → exit 0
- `cargo fmt --check` → 先 1，`cargo fmt -p aline-markdown` 后 exit 0
- 自证（`self-proof.txt`）：三处临时变异（标量侧、字素侧、ZWJ 簇）均变红 exit 101，还原后 sha256 与变异前一致

## 待裁决（需准入 + V-07 金样，本批未自行实现）
- 本批交付的是**验收分离**，不是**引擎级字素适配**。若判定「必须有引擎级字素适配器（返回
  字素簇边界供移动/选中使用）」才算该 Scenario 完全达成，则需：
  1. 走 Unicode 分段依赖准入（`docs/design.md:44`）；
  2. 由 V-07 金样收口（`docs/engine-design.md:246` 仅「建议映射服务提供字素边界适配」）。
  范围冲突：本批文件所有权仅限 `tests/mapping.rs`，实现适配器必然改动 `src/mapping/**`。
- 因此 status 记为 **DONE_WITH_CONCERNS**：目标（分开验收）已达成，引擎级适配未做且不应在本批做。

## 遗留 / 交接
- **待独立复核**：本批由主 Agent实现（两个 Sub-agent 名额被 R05b / R06 占用），必须由非作者复核。
  复核重点：9 行手写常量是否真的手写（尤其 ZWJ 的 `{0,4,7,11,14,18}`、旗帜/肤色的 `{0,4,8}`）、
  第 3 步「接受集合恰等于标量集合」是否真能捕捉多余接受、三处变异是否各自独立变红。
- **既有 16 条用例零删改**（diff 仅新增 293 行、0 行删除）；`mapping_goldens_tsv_matches_parser_output`
  等未放宽。
- 未触碰：`crates/aline-markdown/src/**`、`crates/aline-buffer/**`、`crates/aline-edit/**`、
  任何 `Cargo.toml`、任何 `scripts/**`；`docs/validation/.../group-2/safety-results/` 既有脏文件与 R07 无关。
- 临时探针 `crates/aline-markdown/tests/zz_probe_crlf.rs` 已删除，未进入提交。
