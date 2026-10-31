# 6.1 记录：夹具、显式解析配置与字面代码区避让

日期：2026-10-31。状态：夹具先于实现建立并已提交（HEAD `99867bf`）；`cargo test -p aline-markdown --test spec` 通过。本文只记录无头解析层证据，不宣称 UI、V-03、V-07、AC-09 或 M0 出口通过。

## 1. 夹具先于实现

夹具与手工金样在提交 `99867bf2c5775aae4e8ed910713496d4ff84323c`（`test(aline-markdown): 固定组 6 夹具与块金样`）中以**仅夹具**的形式加入，该提交不含任何 `src/` 实现。文件清单与区间约定见 `crates/aline-markdown/tests/fixtures/m0/README.md`。

| 文件 | 覆盖点 |
|---|---|
| `basic.md` | front matter、标题、段落、强调/删除线/行内代码/链接、引用、列表、围栏代码、表格、脚注定义与引用、公式块、emoji shortcode、无尾换行 |
| `basic.blocks.tsv` | `basic.md` 的 10 块/9 间隙手工金样（19 个区间） |
| `front_matter_unclosed.md` | 未闭合 `---` 不吞全文 |
| `front_matter_plus.md` | `+++` 成对 front matter |
| `literal.md` | 围栏代码内的 `---`、`[toc]`、`[^1]`、`<script>` 按字面处理 |
| `gfm.md` | GFM 表格、删除线、自动链接、任务列表、GitHub Alert |
| `md11.md` | front matter、`[toc]`、脚注引用/定义、内联 HTML 保护 |
| `nested.md` | 嵌套列表与嵌套引用 |
| `spec.blocks.tsv` | 上述 6 个夹具的 38 个区间手工金样 |

金样不是由 `aline-markdown` 自身生成，也不得在实现后反向覆盖；独立校验脚本 `verify_goldens.py`（只用标准库，`python3 -I` 运行）按字节逐行重算区间，输出“6 个夹具、38 个区间、0 个问题”，见 `step-6.8/goldens-independent.stdout.log`。

## 2. 显式解析配置

Cargo 侧锁定 `pulldown-cmark = "=0.13.6"` 且 `default-features = false`，不引入第二解析器。运行期配置为 `ParseConfig { version, features }`：

- `ParseFeatures` 为位集合：`TABLES`、`FOOTNOTES`、`STRIKETHROUGH`、`TASKLISTS`、`MATH`、`GFM`，M0 全开，位值 `0x00000c1e`，配置代次 1。
- `options()` 显式映射到 `Options::ENABLE_TABLES/ENABLE_FOOTNOTES/ENABLE_STRIKETHROUGH/ENABLE_TASKLISTS/ENABLE_MATH/ENABLE_GFM`。
- 代次 0 在解析入口被拒绝，防止“未配置”被当成默认配置。
- 未启用原生 metadata 选项：Front matter 由本 crate 的前置改写识别。smart punctuation 与 heading attributes 保持关闭。

依据记录已回填到 `docs/engine-design.md` §6.2（2026-10-31 M0 解析配置记录）。

## 3. 覆盖的行为

`tests/spec.rs` 共 10 个用例：

| 用例 | 验证 |
|---|---|
| `fixture_blocks_and_gaps_match_handwritten_goldens` | 6 个夹具的块/Gap 与 `spec.blocks.tsv` 逐项一致 |
| `basic_blocks_and_gaps_match_handwritten_goldens` | `basic.md` 与 `basic.blocks.tsv` 逐项一致 |
| `literal_code_block_avoids_front_matter_toc_footnote_and_html` | 围栏代码内的 `---`/`[toc]`/`[^1]`/`<script>` 不触发 Front matter、目录、脚注与 HTML |
| `unclosed_front_matter_is_not_swallowed` | 未闭合 `---` 只作普通内容 |
| `front_matter_plus_and_yaml_are_paired` | `---` 与 `+++` 两种成对标记 |
| `gfm_features_are_enabled_explicitly` | 表格/删除线/任务列表/自动链接由显式位开启，不依赖默认 |
| `dump_contains_config_blocks_gaps_source_protection_and_diagnostics` | dump 输出含配置代次/features/块/Gap/源长/保护/诊断 |
| `empty_document_has_no_blocks_or_gaps` | 空文档零块零 Gap |
| `multiple_blank_lines_are_gaps_between_paragraphs` | `a\n\n\n\nb\n` 的连续空行全在 Gap |
| `leading_and_trailing_blank_lines_are_gaps` | 前导/尾随空行计入 Gap |

后两个用例由复核遗留项 F3-2 补入，并在同一文件加了 `assert_seamless`：块与 Gap 无缝覆盖源码、块尾不含分隔换行。

公式与 Mermaid 只识别来源：`tests/diagnostics.rs` 的 `mermaid_fence_is_source_only_code_block_without_render_backend` 断言 Mermaid 围栏是唯一 `CodeBlock`、块范围切片等于围栏原文、无诊断，并对 `BlockKind` 做穷尽匹配确认不存在渲染种类。这不宣称公式/图表渲染能力。

## 4. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:11Z | `cargo test -p aline-markdown --locked --offline --test spec` | 0 | `step-6.1/targeted-tests/spec.stdout.log`（8 passed） |
| 2026-10-31T08:48:11Z | `cargo test -p aline-markdown --locked --offline -- --list` | 0 | `evidence/tests-list.stdout.log` |
| 2026-10-31T08:48:11Z | `git rev-parse HEAD; git status --porcelain` | 0 | `evidence/git-state.stdout.log` |

`08:48` 的记录是**修复前**快照（`spec` 为 8 例）。F3-2 补入多空行与前后导空行两例后，`spec` 变为 10 例，修复后证据见 `step-6.8/targeted-tests-post-fix/`（脚本 `run-post-fix-tests.sh`）。

## 5. 未覆盖

- 真实 UI 渲染、灰色只读区呈现与源码模式切换未实现。
- 本组不测量性能；解析耗时与峰值内存属 V-05/V-06。
- 仅 macOS arm64 + Rust 1.98.1；未在其他 OS/target 运行。
- `[toc]` 只作为来源识别与保护范围，目录生成与导航未实现。
