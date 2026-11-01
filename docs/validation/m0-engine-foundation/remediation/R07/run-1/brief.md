# R07 派发契约：F-10 字素边界与标量边界分开验收

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。仓库根 `/Users/xian/Project/editor/Aline`。前置：R03 已完成（`ReusedBlock` 受检化，不影响本批写入范围）。

## 文件所有权（只有你可写这些文件）

- `crates/aline-markdown/tests/mapping.rs`（新增测试；既有测试只允许不改断言的情况下重排，不得放宽）
- `docs/validation/m0-engine-foundation/remediation/R07/run-1/` 下你自己新建的证据文件

不得写其它文件（尤其不要动 `crates/aline-markdown/src/**`、`crates/aline-buffer/**`、`crates/aline-edit/**`、任何 `Cargo.toml`、任何 `scripts/**`）。不得 `git reset`、不得回退他人改动。

## 失败事实（F-10，严重，维度 D3）

`openspec/changes/m0-engine-foundation/specs/engine/markdown/spec.md` Scenario「重复文字与字素」：

> WHEN 处理重复空白、中文、组合附加符和 emoji ZWJ
> THEN 依据构建来源定位，**字素移动边界与标量边界分开验证**，不用字符相等回搜猜来源

现状（我已独立复验）：

- `rg -n -e grapheme -e 字素 crates` 无命中；仓内没有任何字素边界概念。
- `crates/aline-markdown/tests/mapping.rs:495` `zwj_family_emoji_stays_exact_before_hidden_newline` 与 `:608` `tab_and_combining_character_keep_utf8_boundaries` 只断言了来源片段、UTF-8 标量边界拒绝（`drawn_offset(2)` → `NotCharBoundary`）与 UTF-16 代理对/映射；**没有**任何「哪些偏移是用户字符边界、哪些只是标量边界」的断言。
- 关键区别：`e\u{301}` 的标量边界是 `{0,1,3}`，字素簇边界是 `{0,3}`——`drawn_offset(1)` 现在**成功**，但 1 不是用户字符边界。这个区别当前完全没有被验收。

## 目标（在 M0 纯边界层面把两者分开验收）

1. **独立手写字素边界表**：新增一个表驱动测试（建议命名 `grapheme_and_scalar_boundaries_verified_separately`），至少覆盖 6 类簇，每类都写死字符串、字节长度、**标量边界集合**、**字素簇边界集合**（两套都手写，不调用任何实现生成）：
   - 组合附加符：`e\u{301}`（以及至少一个双附加符，如 `e\u{301}\u{302}`）；
   - ZWJ 家族：`👨\u{200d}👩\u{200d}👧`；
   - 肤色修饰：`👍\U{1F3FD}`；
   - 区域旗帜：`🇨🇳`（两个 regional indicator）；
   - 变体选择符：`❤\u{FE0F}`；
   - 中文与重复空白：至少一组 `中文字`、`a  b`（这几类标量与字素边界相同，作为对照，说明「相等」也是被断言的结果而不是假设）；
   - CRLF：若可构造，断言 `\r\n` 在映射层的处理（`parse` 不规范化 CR/LF；不确定就跳过并在报告里写明为什么）。
   每类都要给出**推导依据**（UAX #29 的哪条规则，用中文注释写清），并在 `report.md` 里逐行列出「字符串 / 字节 / 标量边界 / 字素边界 / 依据」。
2. **分开断言，不许合并**：
   - 断言映射层实际接受的边界集合**恰好等于**该类的标量边界集合（用 `src_offset`/`drawn_offset` 逐偏移探测，结果收集成集合再比较，不要逐点散断言而漏掉多余接受）。
   - 断言手写字素集合是标量集合的子集，且**两者的差集非空**（对 `e\u{301}` 这类），并逐个列出差集元素（例如 `e\u{301}` 的差集 = `{1}`）；对「相等」的类（中文/重复空白）断言差集为空。
   - 断言「字素移动」在该夹具上的结果：给定手写字素集合，从任一簇起点移到下一个簇边界只经过集合内偏移；用纯数据断言（例如构造 `Vec<usize>` 的移动轨迹并比较），**不要**新增任何 UI 或输入处理。
3. **不得用字符串包含 ZWJ 冒充覆盖**：不能只断言「含 ZWJ 的字符串片段是 Exact」；必须落到具体字节边界表。
4. **明确未实现部分**：仓内**没有**字素分段实现（`docs/engine-design.md:246` 只说「建议映射服务提供字素边界适配」，`design.md:44` 规定「Unicode 分段依赖准入」且金样由 V-07 收口）。因此：
   - 本批**不得**新增 Unicode 分段依赖、不得手写完整 UAX #29 实现、不得新增公共 API。
   - 若你判断「必须有引擎级字素适配器才算完成该 scenario」，不要自行实现：在 `handoff.md` 的「待裁决」写明理由与范围冲突（需准入 + V-07 金样），status 返回 `DONE_WITH_CONCERNS`，把已完成的部分照常交付。
5. 既有测试（`mapping_goldens_tsv_matches_parser_output`、`zwj_family_emoji_stays_exact_before_hidden_newline`、`tab_and_combining_character_keep_utf8_boundaries`、`repeated_text_keeps_distinct_segments_and_canonical_anchors` 等）断言不得减少或放宽。
6. 测试不得 `unwrap()/expect()`（clippy 在 engine crate 也覆盖 tests）；用 `?` 与显式 `ok_or`。
7. 全部 Cargo 命令 `--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。工具缺失/离线失败记「未核实」。
8. 提交（Conventional Commits，scope `aline-markdown`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R07/run-1/`）

- `pre-fix.txt`（新增测试在未加字素断言时的失败/通过状态说明；若本批只是补测试，写明「无代码改动，无修复前后差异」并给出实际运行）
- `post-fix.txt`（`cargo test -p aline-markdown --locked --offline` 全量原始输出）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（逐类边界表与 UAX #29 依据、差集、未实现部分、对 V-07 的依赖）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、一行测试小结、覆盖的簇类数、concerns、证据路径。不要贴长日志。
