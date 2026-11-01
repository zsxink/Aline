# 组 6 第二轮独立复核报告（review-round-2）

日期：2026-10-31（UTC）。复核者：独立复核（非实现者）。范围：OpenSpec change `m0-engine-foundation`，组 6（`aline-markdown`，任务 6.1–6.8）的 round-1 遗留项处置与回归。本文件是本次复核唯一写入仓库的文件；未修改任何代码、测试、金样或其它文档，也不在其它文件写复核结论。

## 1. 快照与方法

- **HEAD**：`99867bf2c5775aae4e8ed910713496d4ff84323c`（`main`）。复核开始与结束时均为该提交，期间未变。
- **读取窗口**：2026-10-31 约 09:15Z 至 10:02Z（结束时 `date -u` 为 `Sat Oct 31 10:01:38 UTC 2026`）。窗口内实现者仍在写入组 6 证据：`step-6.8/report.md` 生成于 09:46:01Z，工作区门禁运行于 09:42:36Z–09:43:04Z，回填哈希日志生成于 09:45:51Z。本报告读取的是这些写入完成后的工作区；晚于读取窗口的追加不在覆盖范围。
- **工作区**：`git status --porcelain` 显示组 6 证据整体仍未跟踪（`?? docs/validation/m0-engine-foundation/group-6/`）。HEAD 未变，因此本轮复核的不是一个不可变提交快照，而是工作区最终态。
- **环境**：macOS Darwin 27.2.0 arm64；`rustc 1.98.1`；`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。
- **方法**：只读代码、测试与证据；用上述隔离环境重跑 `cargo test -p aline-markdown --locked --offline` 与定向测试；在 `/tmp` 自建探针 crate 与独立 Python 脚本重算金样。实现者报告中的计数与结论不作为证据，均以代码、日志与自跑结果为准。

本轮实际执行（关键）：

```sh
CARGO_HOME=<cargo-home> \
RUSTUP_TOOLCHAIN=1.98.1 CARGO_NET_OFFLINE=true \
cargo test -p aline-markdown --locked --offline
# 98 passed, 0 failed（与 6.8 证据一致）

# 定向：spec 10、rewrite 9、mapping 16、html 20、protection 6、
#       hostile 15、reuse 13、diagnostics 5，均 0 failed

python3 -I <tmp>/verify_mapping_goldens.py \
  docs/validation/m0-engine-foundation/group-6/step-6.4/goldens/mapping-goldens.tsv
# cases=6 problems=0
```

## 2. 六项核验范围结论

| 核验范围 | 结论 |
|---|---|
| 1. 有限 HTML：过滤集、顺序、空白折叠、块级合成换行、实体与回归 | **成立**（F2-1 要求的修复真实落地并有测试）；理由性说明与未支持标签对称性另见 F3 |
| 2. 复用：容器前缀剥离、O(lines)/不分配、定义/围栏拒绝、过度拒绝、差分比较 | **部分成立**（单行容器用例成立；多行引用定义标签仍绕过，且可产生可见文本不一致的复用） |
| 3. 预算/取消：公开 API、`parse_with_cancel`、`Cancelled`、`NeverCancel`、统计真实性 | **部分成立**（取消与 `mapping_points` 成立；过滤 HTML 的 `scan_steps` 不实；预算出口表 4 行错误） |
| 4. 金样消费者与独立重算 | **成立**（消费者逐项断言 6 例；独立 Python 重算 0 问题；TSV 早于消费者与复核且未改；手写来源不可证明另见 F3） |
| 5. 报告与文档诚实性（6.1–6.8、回填文本） | **部分成立**（回填为纯追加、未改规范句、明确不宣称 AC-09/V-06/V-07/M0 出口；但多处事实性描述不实或过宽，见 F2/F3） |
| 6. 证据完整性（日志、计数、时间戳、门禁） | **基本成立**（计数与时间顺序自洽，未发现伪造迹象）；但 F3-8 未成立：证据未提交、报告无提交 SHA |

## 3. round-1 遗留项逐项判定

| 项 | 判定 | 一句话依据 |
|---|---|---|
| F1-1（6.8 回填、夹具、组级报告） | **部分成立** | 回填与 6.1–6.8 报告已完成；组级 `group-6/report.md` 缺失，`docs/engine-design.md:179`、`:429` 两个链接悬空 |
| F2-1（有限 HTML） | **成立** | 过滤集补齐、空白折叠、块级合成换行、实体/零可见均落地；html 20、protection 6 通过；说明性遗留见 N-F3-1/N-F3-2 |
| F2-2（复用全局依赖） | **部分成立** | 单行容器内定义/围栏拒绝、过度拒绝用例通过；多行引用定义标签仍绕过，删除定义后可复用出与全量重解析不同的绘制文本 |
| F2-3（取消/预算/统计） | **部分成立** | 取消 API 与用例、`mapping_points` 成立；过滤 HTML 的 `scan_steps` 与真实阈值不一致，预算出口表 4 行错误，`max_mapping_points` 兼作片段上限 |
| F3-1（映射金样消费者） | **成立** | `tests/mapping.rs:450-492` 逐项消费 6 例；独立重算 0 问题；TSV 08:52:17Z 早于消费者 09:31:07Z 与 review 09:15:27Z，mtime 未改 |
| F3-2（多空行/dump） | **成立** | spec 10 例含多空行、首尾空行；金样校验 6 个 spec 夹具/38 区间/0 问题（另 basic 夹具 19 区间，合计 7 夹具/57 区间） |
| F3-3（RawFallback 诊断） | **成立** | diagnostics 5 例；整源回退保护整源；公开 `parse` 不可达已如实记录 |
| F3-4（选项记录、Mermaid、合成围栏） | **成立** | 合成围栏与 Mermaid 仅来源有测试；`ParseConfig`/`ParseFeatures`（`0x00000c1e`、代次 1）写入引擎 §6.2 |
| F3-5（HTML 金样覆盖） | **成立** | html 20 例覆盖实体、过滤嵌套、零可见；探针复现 |
| F3-6（复用差分/覆盖） | **部分成立** | 测试辅助已比较映射/保护/样式；生产 `same_block` 仍只比 kind+text；多行定义缺口见 N-F2-1 |
| F3-7（环境日志） | **成立** | 逐变量 `printenv` 日志保留首次失败尝试，随后显式 `export` 得真实值；平台缺口已记录 |
| F3-8（证据提交） | **未成立** | HEAD 未变、组 6 证据全部未跟踪、6.8 报告无提交 SHA；验收要求的 `git show --stat` 不成立 |

### 3.1 F1-1：回填真实，组级报告缺失

**已成立的部分**：

- 回填为纯追加：`git diff --stat docs/` 为 4 个文件、22 行新增、0 行删除；`git diff --check` 退出 0。回填前后 SHA-256 与 6.8 报告一致（`engine-design.md` `279780bb…`→`e60101de…`；`v1-validation-plan.md` `239d6ca4…`→`5ad602af…`；`v1-prd.md` `ef660179…`→`47962a6f…`；`v1-architecture.md` `16eae13e…`→`0e94cca4…`；`rust-style-guide.md` 未改 `6d94f86c…`）。`step-6.8/backfill-hashes.log` 生成于 09:45:51Z，前后哈希与工作区实际值一致。
- 回填文本未改写既有规范句、AC/V/TA/ED/PA 状态或链接（0 删除）；新增段落均带“2026-10-31”日期，并明确不宣称 AC-09/V-06/V-07/M0 出口通过。`docs/v1-prd.md:402` 明确写“AC-09 未通过”。
- `step-6.1`–`step-6.8` 报告均存在；6.8 报告 92 行，含门禁、定向计数、环境与缺口记录。

**未成立的部分**：

- `docs/validation/m0-engine-foundation/group-6/` 下没有组级 `report.md`（`ls` 可见 `review.md`、`follow-ups.md`、`step-6.*`、脚本与 `evidence/`，无 `report.md`）。
- `grep -rn "group-6/report.md" docs/` 只有 `docs/engine-design.md:179` 与 `:429` 两处，均指向不存在的文件。这是可点击的悬空链接，不是措辞问题。
- `step-6.8/report.md:91` 写“独立复核见组级 `review.md` 与 `review-round-2.md`”，但组级报告本身仍缺失；该句不能替代 follow-ups F1-1 要求的“写组级报告”。

因此 F1-1 只能判**部分成立**；缺失的组级报告与悬空链接构成新的 F1（N-F1-1），6.8 仍阻塞。

### 3.2 F2-1：有限 HTML 修复成立

**过滤集与顺序**：`crates/aline-markdown/src/tree/html.rs:736-755` 的过滤集包含 `script/style/iframe/title/textarea/xmp/noembed/noframes/plaintext`，并保留 `object/embed/form/button/svg/math`；过滤分支在 `:316` 先于 `br` 与 void 判定，未闭合时把剩余源码整体字面输出（不吞源码）。`is_void_tag`（约 `:727-733`）为 `img/hr/input/meta/link/source/area/base/col`，与过滤集不相交。

**空白折叠**：HTML 文本上下文连续空白折叠为一个空格；探针 `<b>a   b</b>` 绘制为 `"a b"`，折叠空格为 `Aligned` 片段并覆盖被消费的 `4..7` 源码。测试 `html` 20 例通过。

**块级合成换行**：探针 `<div>a</div><div>b</div>` 绘制为 `"a\nb\n"`（修复前为 `"ab\n"`）；块级标签边界产生 `Synthetic` `\n`，锚点为标签起点，构树侧去重；`<br>` 行为不变。

**实体与零可见**：`&NotEqualTilde;`、`&#x1F600;` 等解码为 `Aligned` 并回指完整实体源码；`<b></b>`、`<i> </i>` 不绘制文本、无样式，源码由 Hidden/Protected 覆盖。测试覆盖。

**回归**：`cargo test -p aline-markdown --locked --offline` 98 通过；html 20、protection 6；无既有断言回归（`entities_inside_html_keep_aligned_mapping` 的 `styled.len()` 由 3 改 5 有注释说明，drawn 与实体片段断言未变）。

**遗留（不影响 F2-1 成立，但须记录）**：

- “过滤判定先于 void 判定”对 `input/link/meta/base` 是 **no-op**：过滤集与 void 集不相交，这些标签从未进入过滤分支。`tree/html.rs:330` 的注释“避免 input/link/meta/base 抢占字面路径”与 `step-6.5/report.md:18` 的同一说法不成立。实际行为仍是 void 分支整体隐藏，与处置表“非过滤集 void 标签不擅自改、作为 V-07 待定项记录”一致（`tree/html.rs:340-343`、`step-6.5/report.md` §8）。见 N-F3-1。
- 未支持的非 void 标签不对称：探针 `<foo>a   b</foo>` 绘制为 `"<foo>a   b"`——开标签按字面可见、闭标签隐藏、内部空白不折叠；没有测试或 V-07 记录。见 N-F3-2。

### 3.3 F2-2：单行容器成立，多行引用定义仍漏

**已成立的部分**：

- `reuse.rs:266` 起的 `strip_container_prefixes` 逐行剥离最多常量个 `>`/列表前缀，返回 `&str` 切片，循环次数与行数线性，不分配；`has_reference_definition`（`:243`）与 `has_fence_line`（`:252`）在剥离后识别。
- 单行容器用例成立：探针 `> [x]: /u` 返回 `refusal=Some(ReferenceDefinition)`、`reused=0`；块引用/列表内单行定义与围栏的拒绝测试通过；过度拒绝用例 `plain_block_quote_text_still_reuses` 通过（普通块引用文字仍复用）。
- 测试辅助的差分比较已加入块种类、重定位后范围、投影片段来源/精确性、保护范围、样式范围；跨块 Aligned/Synthetic 部分裁剪在辅助比较中报错（`tests/reuse.rs:471-474`）。

**未成立的部分（新 F2）**：

- 多行引用定义标签绕过检测。`has_reference_definition` 是逐行判断：`[foo` 行不含 `]:`，续行 `bar]: /u` 不以 `[` 开头，因此整体不命中。
- 删除定义的反例产生**可见文本不一致**的复用：旧源 `"[foo\nbar]: /u\n\n[foo bar]\n"` 的旧块绘制为 `"foo bar"`（引用已解析）；新源 `"[foo bar]\n"` 全量解析绘制为 `"[foo bar]"`。`reuse` 返回 `refusal=None`，并配对旧 `15..24` → 新 `0..9`（Paragraph）。若消费者按旧块的投影/样式翻译，会显示 `"foo bar"` 而非 `"[foo bar]"`——这不是“文本未损坏”的旧描述，而是可见文本错误。
- 改目标的反例同样不拒绝：`[foo\nbar]: http://x` → `[foo\nbar]: javascript:x` 返回 `refusal=None`、`reused=1`。虽然 M0 公开 API 不暴露 href、可见文本相同，但这违背“引用定义目标变化必须拒绝”的全局依赖契约。
- 行内链接目标变化 `[a](/u)`→`[a](/v)` 不触发拒绝；改变块因 block text 不同而不配对，其它块仍复用。`docs/engine-design.md:429` 的“含链接目标变化的文档因整体拒绝复用而不可比”对行内链接不成立（见 N-F3-7）。
- 现有 reuse 13 例没有多行标签用例；follow-ups F2-2 的验收“改目标/删除定义/围栏内容变化必须拒绝”（MUST）对多行标签未满足。

因此 F2-2 判**部分成立**；残留为 N-F2-1，须在 M0 出口前关闭。

### 3.4 F2-3：取消成立，统计与出口表不实

**已成立的部分**：

- 公开取消接口：`budget.rs` 的 `CancelCheck`/`CancelToken`（`Arc<AtomicBool>`，`Relaxed`）与 `parse_with_cancel`；取消在入口、重写编辑间、Front matter 前后、每个事件前、构树/冲刷/收尾循环间轮询；取消返回 `ParseError::Cancelled` 且不产出树。
- `NeverCancel` 默认路径等价：`tests/hostile.rs:273` 的 `uncancelled_token_preserves_default_parse_result` 通过；预取消、深嵌套取消、巨型输入取消用例通过（hostile 15）。
- `ParseStats::mapping_points`：`parse.rs:307` 与 `:344` 均为 `drawn.chars().count().saturating_add(1)`；`"abc\n"` 统计为 4，与 `projection.rs:660-690` 每字符一个点加起点一致。`mapping_point_budget_is_counted_in_stats_and_enforced`（`tests/hostile.rs:287` 起）通过。
- 第三方 `pulldown-cmark` 内部长调用不可中断已在 6.6 §4、引擎 §12.3、6.8 §6 如实记录，未冒充已解决。

**未成立的部分（新 F2）**：

- `stats.scan_steps` 对过滤 HTML 不是实际扫描步数。`build.rs:123-125` 对每个 `Event::Html|InlineHtml` 调用 `budget.rs:182-209` 的 `html_scan_steps(raw)` 复刻；该复刻缺少过滤分支，而真实扫描器 `tree/html.rs:240-265` 在过滤标签内把“剩余到闭标签”作为一步消费。探针结果：
  - `<script>\n<b>x</b>\n</script>\n`：`stats=8`，二分得到的真实最小阈值 `5`；
  - `<script><b>x</b></script>\n`：`stats=6`，阈值 `3`；
  - 未过滤对照 `<div>\n<b>x</b>\n</div>\n`：`8 == 8`；40 个 `<b>x</b>`：`80 == 80`。
  因此 `tests/hostile.rs:171-211` 的“统计 vs 二分阈值”交叉验证只覆盖未过滤样本；`step-6.6/report.md:63,68`、`docs/engine-design.md:461`、`docs/v1-validation-plan.md:85` 的“记录实际扫描步数/交叉一致”表述过宽或对过滤 HTML 不实。
- 预算出口表 4 行错误（`step-6.6/report.md:12,14,17,18`）：
  - 重写文本字节：表写“回退原文 + 预算诊断”，实际 `parse.rs:246` 返回 `Err(ParseError::BudgetExceeded)`；`tests/hostile.rs:214-227` 正是断言该错误。
  - 扫描步数、HTML 栈深：表写“该区原文投影”，实际 `tree/html.rs:255`/标签栈错误经 `build.rs:270-296` 返回 `BuildError`，入口整体 `raw_fallback`（整源回退），不是该区投影。
  - 投影索引点：表写“整源回退”，实际 `parse.rs:305-314` 将 `Projection::from_segments` 的 `MappingError::BudgetExceeded` 映射为 `Err(ParseError::Mapping(BudgetExceeded))`；`tests/hostile.rs:287` 起的测试也断言该错误，而非回退。
  - `docs/engine-design.md:461` 的“`max_mapping_points` 已由注入用例验证可触发回退”因此不实；“可触发错误”才是实际行为。
- `max_mapping_points` 兼作片段数上限：`projection.rs:245` 用 `segments.len() > budget.max_points` 拒绝；`MappingBudget::new(budget.max_rewritten_bytes(), budget.max_mapping_points())`（`parse.rs:305`）把 `max_mapping_points` 同时用于索引点上限与片段数上限，绘制字节上限则复用 `max_rewritten_bytes`。探针 `[a](u)\n`：`stats_points=2`、`segments=3`，`max_mapping_points=2` 时返回 `Err(Mapping(BudgetExceeded))`，`3` 时通过——即“投影索引点”维度可因片段数超限而触发，且错误类型与表不符。该语义未在文档中说明。见 N-F2-3。

因此 F2-3 判**部分成立**；残留为 N-F2-2/N-F2-3，须在 M0 出口前关闭。

### 3.5 F3-1：映射金样消费者与独立重算成立

- 消费者：`tests/mapping.rs:450-492` 的 `mapping_goldens_tsv_matches_parser_output` 用 `include_str!` 读取 `mapping-goldens.tsv`，断言 6 个用例的 drawn、每个 segment 的 drawn/source 范围与精度，以及每个 anchor 经 `drawn_offset`/`source_at` 的结果；不是只做文件存在性检查。
- 独立重算：本轮在 `<tmp>/verify_mapping_goldens.py` 用 Python 独立解析 TSV，按文档模型检查：segment 连续铺满 drawn、Exact 两侧字节相等、Aligned 两侧非空、Hidden 无绘制/有来源、Synthetic 有绘制/无来源、源码切片为合法 UTF-8，以及 `segment_for_drawn` + affinity 的锚点规则（跳过 Hidden；Exact 为 start+delta；Aligned/Hidden 按 Before/After 取 start/end；Synthetic 取 source start）。结果 `cases=6 problems=0`，退出 0。该脚本不依赖 `aline-markdown`。
- 来源：TSV 生成时间 08:52:17Z，消费者 `tests/mapping.rs` 生成 09:31:07Z，round-1 `review.md` 生成 09:15:27Z；TSV 的 mtime 仍为 08:52:17Z，未在复核后被回填或改写。`step-6.4/goldens-check/` 的探针日志与 TSV 一致。
- 限制（N-F3-5）：6.4 报告称 TSV 为“实现前手写”，但 08:48 已存在修复前解析器，08:52 的 TSV 不能排除由该解析器生成；只能证明“先于消费者与 round-1 复核且未改”，不能证明“手写”。内容本身经独立模型复算无误，故 F3-1 仍判成立，但报告措辞应限定。

### 3.6 F3-2 至 F3-7：其余遗留项

- **F3-2（多空行/dump）成立**：spec 10 例，含 `multiple_blank_lines_are_gaps_between_paragraphs`、`leading_and_trailing_blank_lines_are_gaps` 与无缝覆盖断言；金样独立校验输出 6 个夹具、38 个区间、0 问题。
- **F3-3（RawFallback）成立**：diagnostics 5 例；`raw_fallback` 保护整源（`tests/protection.rs` 的 `raw_fallback_protects_the_whole_source`）；公开 `parse` 入口的实际预算诊断是 `BudgetExceeded`、`RawFallback` 分支不可达，已在 6.6 §4 与 6.8 §6 如实记录，未伪装成已执行。
- **F3-4（选项/Mermaid/合成围栏）成立**：`synthetic_fence_maps_to_empty_source_anchor_not_real_source` 验证合成片段的 `0..0` Synthetic 锚点；Mermaid 仅识别来源；引擎 §6.2 记录 `ParseConfig`/`ParseFeatures`（位值 `0x00000c1e`、代次 1）、Front matter 前置改写、未启用原生 metadata/smart punctuation/heading attributes。
- **F3-5（HTML 金样）成立**：html 20 例覆盖实体（十进制/十六进制/多字符）、过滤嵌套、零可见；探针复现。
- **F3-6（复用差分/覆盖）部分成立**：测试辅助已比较映射来源/精确性、保护与样式；但生产 `reuse.rs:183-190` 的 `same_block` 只比较 `kind` 与 `block_text`，文档/报告中的“差分比较”若被读成生产路径比较则不成立；多行定义缺口见 N-F2-1。
- **F3-7（环境日志）成立**：`step-6.8/environment-per-variable.stdout.log`（09:23:35Z）逐变量显示 `CARGO_HOME`、`RUSTUP_TOOLCHAIN`、`CARGO_NET_OFFLINE` 的真实值；首次 `printenv` 多参数显示“未设置”的失败尝试被保留，随后显式 `export` 复跑；`uname`/`rustc` 与 macOS arm64 记录一致；Windows/Linux/CI 未覆盖已写入 6.8 §5。

### 3.7 F3-8：证据仍未提交

- HEAD 仍为 `99867bf2c5775aae4e8ed910713496d4ff84323c`；`git status --porcelain` 显示 `?? docs/validation/m0-engine-foundation/group-6/`，整个组 6 证据目录未跟踪。
- `step-6.8/report.md` 没有提交 SHA；全文只写日期、门禁时间、日志路径与“独立复核见 review.md/review-round-2.md”，不满足 follow-ups F3-8 的“报告注明提交 SHA 与证据快照时点”与 `git show --stat` 验收。
- 证据在复核窗口内仍被追加（6.8 报告 09:46:01Z），与 round-1 F3-8 指出的“复核期间仍有追加”性质相同。故 F3-8 判**未成立**。

## 4. 新发现（含 round-1 未闭合残留）

| ID | 级别 | 位置 | 问题 | 处置要求 |
|---|---|---|---|---|
| N-F1-1 | **F1** | `docs/validation/m0-engine-foundation/group-6/`（缺 `report.md`）；`docs/engine-design.md:179,429` | 组级报告缺失；两处链接指向不存在的文件，文档不可核对 | 在勾选 6.8 前补组级报告并消除悬空链接 |
| N-F2-1 | **F2** | `crates/aline-markdown/src/reuse.rs:243-252`；`tests/reuse.rs` | 多行引用定义标签绕过全局依赖检测；删除定义后可复用出与全量重解析不同的可见文本（`"foo bar"` vs `"[foo bar]"`） | 识别跨行标签或保守拒绝；补“改目标/删除定义/围栏内容变化”多行反例；M0 出口前 |
| N-F2-2 | **F2** | `crates/aline-markdown/src/budget.rs:182-209`；`build.rs:123-125`；`tree/html.rs:240-265` | 过滤 HTML 的 `scan_steps` 与真实阈值不一致（8 vs 5、6 vs 3）；交叉验证只覆盖未过滤样本 | 复刻补过滤分支或由扫描器直接回报；补过滤样本交叉验证；修正 6.6/引擎/验证计划措辞 |
| N-F2-3 | **F2** | `step-6.6/report.md:12,14,17,18`；`parse.rs:246,305-314`；`build.rs:270-296`；`projection.rs:245`；`engine-design.md:461` | 预算出口表 4 行错误；`max_mapping_points` 返回 `Err` 而非回退，且兼作片段数上限 | 修正表与文档；明确或拆分 `max_mapping_points` 语义；M0 出口前 |
| N-F3-1 | F3 | `tree/html.rs:330`；`step-6.5/report.md:18` | “过滤先于 void”对 `input/link/meta/base` 是 no-op（集合不相交），注释/报告理由不成立 | 更正注释与报告；行为本身符合处置表 |
| N-F3-2 | F3 | `tree/html.rs:364` 与闭标签分支 | 未支持非 void 标签开标签字面可见、闭标签隐藏、内部空白不折叠；无测试/文档 | 补测试并记入 V-07，或统一处理 |
| N-F3-3 | F3 | `reuse.rs:183-190`；`step-6.7/report.md:11`；`engine-design.md:429` | 生产 `same_block` 只比 kind+text；差分比较只在测试辅助；文档措辞易被读成生产已比较 | 明确“测试比较/生产不比较”，或补生产差分 |
| N-F3-4 | F3 | `step-6.2/report.md:31`；`tests/hostile.rs:214-227` | 报告称两个测试都断言“整段原文 + 诊断”；hostile 测试实际断言 `Err(BudgetExceeded)` | 更正报告 |
| N-F3-5 | F3 | `step-6.4/report.md`；`mapping-goldens.tsv` | “实现前手写”不可证明（08:48 已有预修复解析器，TSV 08:52） | 改为“先于消费者/复核且未改”，不写不可证的手写来源 |
| N-F3-6 | F3 | `docs/v1-architecture.md:553` | “超限整源回退而不截断”过宽；源码/重写/投影索引点超限返回 `Err` | 限定为实际回退的构树/预算维度，或改为“不发布部分结果” |
| N-F3-7 | F3 | `docs/engine-design.md:429` | “含链接目标变化的文档因整体拒绝复用而不可比”对行内链接不成立（无拒绝、未改块不配对、其它块复用） | 改为“引用定义目标变化（单行）拒绝；行内链接目标不在 M0 比较范围” |

## 5. 证据完整性核验

- **定向与全 crate**：`commands-post-fix.log` 记录 2026-10-31T09:42:10Z–09:42:11Z 的命令，全部 `exit=0`；定向计数 spec 10、rewrite 9、mapping 16、html 20、protection 6、hostile 15、reuse 13、diagnostics 5；全 crate 日志的各测试二进制通过数为 5+15+20+2+16+6+2+13+9+10 = **98**，与 6.8 报告一致。本轮在隔离环境复跑全 crate 得 98 passed、0 failed，定向计数一致。
- **修复前对照**：`commands.log` 为 2026-10-31T08:48:11Z–08:48:13Z，全部 `exit=0`；计数 spec 8、rewrite 6、mapping 10、html 11、protection 6、hostile 10、reuse 7、全 crate 62。与修复后增量（spec +2、rewrite +3、mapping +6、html +9、hostile +5、reuse +6、diagnostics +5、全 crate +36）相符，未发现用后一次覆盖前一次。
- **工作区门禁**：`workspace-gates-post-fix/summary.json` 的 `passed=true`、19 项结果；各项 `exit_code` 均为 0；`cargo test --workspace` 为 187 passed、0 failed。修复前 `workspace-gates/` 为 19 项、134 passed。两次运行都保留。
- **金样**：`verify_goldens.py` 输出 6 个夹具、38 个区间、0 问题；本轮独立 Python 重算 6 个映射用例、0 问题；TSV 与 `goldens-check/` 探针日志一致。
- **时间戳与哈希**：回填哈希日志 09:45:51Z、门禁 09:42:36Z–09:43:04Z、6.8 报告 09:46:01Z、环境日志 09:23:35Z，顺序自洽；回填前后 SHA-256 与工作区实际文件一致。未发现伪造时间戳或伪造计数的迹象。
- **完整性缺口**：组 6 证据未提交（N-F1-1/F3-8）；6.8 报告无提交 SHA；文档中若干事实性描述不实（N-F2-2/N-F2-3、N-F3-1..N-F3-7）。这些是内容/流程缺口，不是日志计数造假。

## 6. 仍未冻结/未覆盖

### 6.1 V-07 待冻结（不得写成已完成）

- **SoftBreak 投影口径**：当前投影为带来源的换行，口径本身尚未冻结。
- **非过滤集 void 标签**（`input/link/meta/base` 等）：仍按“不支持标签 → 标签隐藏、无可见内容”处理，展示口径待 V-07。
- **空白折叠与块级标签合成换行的口径本身**：本组只记录当前行为与金样，口径待 V-07。
- **有限 HTML 完整性**：未支持非 void 标签的开/闭标签对称性、属性解析的完整 HTML 语法、过滤标签与块级标签交错的全部组合。
- **真实 UI 呈现**：灰色只读区、源码模式切换、真实命中/候选窗/焦点。
- **导航与生成**：`[toc]` 生成/导航、脚注编号/回链、链接 href 导航（M0 公开 API 不暴露 href）。
- **输入法/字素**：IME 确认/取消、系统字素边界、跨块命令。
- **跨块 Aligned/Synthetic 片段的逐对比较**：当前样本未触发，未实现。

### 6.2 其它未通过/未覆盖（不属 V-07）

- **V-06 未通过**：端到端时延、峰值内存、1/4 MiB 与巨型单段/表格、图片/公式资产、会话级降级汇总与 UI 呈现均未测量；`ParseBudget::m0()` 数值不是发布限额。
- **V-05 未通过**：复用未做局部解析优化、未测性能收益。
- **V-03 未通过**：公式定界符/支持清单、Alerts 五类提示块的产品呈现未冻结。
- **V-08**：权限与替换竞争不在本组。
- **平台/CI**：仅 macOS arm64；Windows/Linux/其他 target/CI 未覆盖。
- **第三方长调用盲区**：`pulldown-cmark` 内部事件生成、重写逐字节循环内部、投影构造本身不可中断；单个超长事件只能结束后观察取消。

## 7. 逐任务结论（6.1–6.8）

| 任务 | 结论 | 依据与遗留 |
|---|---|---|
| 6.1 夹具与显式 Options | **通过（M0 范围）** | 10 个夹具文件在 `99867bf` 中已固定；spec 10 例；无回归 |
| 6.2 RewriteMap 与失败回退 | **通过（M0 范围）** | rewrite 9 例；合成围栏/映射失败回退成立；报告 §3 对 rewritten-budget 测试的描述须按 N-F3-4 更正 |
| 6.3 块/Gap 与 dump | **通过（M0 范围）** | 7 夹具/57 区间（6 个 spec 夹具 38 区间 + basic 19 区间）；verify_goldens 0 问题；spec 10 例 |
| 6.4 映射与 UTF-16 | **通过（M0 范围）** | 消费者 6 例逐项断言；独立重算 0 问题；TSV 未改；手写来源措辞按 N-F3-5 限定 |
| 6.5 有限 HTML 与保护 | **通过（M0 范围）** | html 20、protection 6；过滤集/折叠/合成换行/实体/零可见成立；N-F3-1/N-F3-2 为说明性遗留；V-07 待定项已记录 |
| 6.6 预算、敌意输入与取消 | **通过（M0 范围）** | 取消 API/用例、`mapping_points` 成立；N-F2-2/N-F2-3 的统计与出口表问题须在 M0 出口前修复 |
| 6.7 复用与重定位差分 | **通过（M0 范围）** | 单行容器定义/围栏拒绝、过度拒绝、测试差分成立；N-F2-1 的多行定义复用缺口须在 M0 出口前修复 |
| 6.8 文档回填与 V-06/V-07 基础夹具 | **阻塞** | 回填本身成立（22 行新增/0 删除、哈希一致）；但 N-F1-1 组级报告缺失、两处链接悬空；F3-8 证据未提交、无提交 SHA |

## 8. 明确不声称

- 本报告**不判定 V-06、V-07、AC-09 或 M0 出口通过**；上述均未通过。表中的“通过（M0 范围）”只表示任务 6.x 的 M0 交付范围，不表示发布验收或 V-07 冻结。
- 本报告**不宣称有限 HTML 已完成**：非过滤集 void 标签、空白折叠/合成换行口径本身、未支持标签对称性与真实 UI 呈现仍未冻结。
- 本报告**不宣称复用全局依赖已完整**：多行引用定义标签是已证实的反例，且可产生可见文本不一致的复用。
- 本报告**不宣称预算统计已完全真实**：过滤 HTML 的 `scan_steps` 与真实阈值不一致；预算出口表有 4 行错误。
- 本报告**不宣称组 6 证据已提交**：HEAD 未变，组 6 证据仍未跟踪；6.8 报告无提交 SHA。
- 本报告不替代调度者对 `tasks.md` 的勾选决定；按本报告，6.8 仍阻塞，6.6/6.7 的 F2 残留须在 M0 出口前关闭。
