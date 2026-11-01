# R08 / F-11 删除、粘贴和 HTML 跨界负例

## 结果

新增 `crates/aline-edit/tests/crossing.rs`，逐行加载本目录手工编写的 `goldens/edit-goldens.tsv`，共 14 行：空删除 5 例、粘贴 6 例、HTML 3 例。前一轮在 `refusals.rs` 添加的 4 个表驱动拒绝场景及全部迭代日志均保留。未修改生产代码、UI 或 scripts。

各拒绝用例在调用 `plan_change` 前记录完整状态：text、anchor/head（保留反向选择）、revision、undo_depth、redo_depth、history nodes。拒绝后逐字段相等，并执行 redo 验证原分支可用，核对 Q 节点和选区方向。零长度空替换按 `Unchanged(ReplacementIdentical)` 验收，也比较状态且确认 redo 未丢失。允许变更用手算 TSV 比较单条 patch、group reason、target selection，随后提交实际 CheckedBatch 并核对文本及一步 undo 历史。

## 契约勘误与端点语义

原 brief 要求保护区间左右相邻粘贴均允许。主 Agent 核对现行权威后确认这与 `docs/engine-design.md` §4.2 以及 `aline-edit/src/permissions.rs:11,41–43` 冲突：触及边界也算触及保护范围，零长度插入位于左端或右端时均拒绝。本轮在 `brief.md` 追加勘误，不改生产逻辑；分别加入左端 7..7、右端 11..11 的 `ProtectedRange/FootnoteReference` 拒绝负例。保护范围外一字节的 6..6 插入仍通过，Source 模式同一位置可改原文。该裁决是依现行规范处置，不是待用户裁决。

## Parsed 实际范围

测试从每行对应的 `Parsed` 实际读取保护范围，并以手写 TSV 独立预期交叉核对；HTML 情形同时汇集 `protected_ranges(Html)` 与 `styled_ranges`。`parsed-range-capture.stderr.log` 原始探针输出：脚注引用 protected=`7..11`；HTML protected 开标签=`2..5`、styled 文本=`5..6`、protected 闭标签=`6..10`。命令中的 offset 仅是手算输入，不用于代替解析结果。

## 逐行金样推导

所有字符串在 TSV 用 `\n` 表示换行，偏移均为 UTF-8 字节。

| id | 手算命令范围 → 期望 | 结果 / 目标 |
|---|---|---|
| del_body | `alpha beta gamma\n` 删除 `6..11`（`beta `）→ patch `6..11 => ""` | Changed；结果 `alpha gamma\n`；折叠于 6 |
| del_cross_one_byte | 脚注夹具删除 `6..8`（` [`, 仅跨入首个保护字节） | Visual 拒绝，FootnoteReference `7..11` |
| del_footnote_visual | 删除完整 `7..11 => ""` | Visual 拒绝，`7..11` |
| del_footnote_source | 同一 `7..11` 删除 | Source Changed；目标 7；单步历史 |
| del_zero_empty | `abc\n` 的 `1..1 => ""` | `Unchanged(ReplacementIdentical)`；无空历史步 |
| paste_left_boundary | 在实取保护起点 7 插入 P（`7..7`） | Visual 拒绝，`7..11` |
| paste_left | 起点左侧一字节 `6..6 => P` | patch `6..6 => P`；目标 7 |
| paste_cover | 覆盖 `7..11`，expected `[^1]`，替换 pasted | Visual 拒绝，`7..11` |
| paste_source | Source 在右边界 `11..11 => P` | patch `11..11 => P`；目标 12 |
| paste_multiline | `ab cd\n` 在 2 插入 `X\nY` | patch `2..2 => X\nY`；目标 5；结果 `abX\nY cd\n` |
| html_right | `a <b>x</b> c\n` 格式 `6..7`（右端切入闭标签） | HtmlBoundary `6..10` |
| html_whole | 格式 `1..11` 包住 `<b>x</b>` 全元素 | HtmlBoundary `2..5` |
| html_tag_inner | 将开标签 `<b>` 内 `3..4` 的 b 改为 i | HtmlBoundary `2..5` |
| paste_right | 在实取保护终点 11 插入 P（`11..11`） | Visual 拒绝，`7..11` |

右侧边界 Visual 粘贴原先独立预期 Changed 的失败日志保存在 `crossing-sixth-run.*` 与 `crossing-after-right-last.*`；收到规范核对后按闭区间语义更新 TSV 为拒绝，不覆盖失败证据。早期 test harness/TSV 编码尝试日志也全部保留。`del_body` 迭代细节见下节；早期 TSV 没有保留副本，不能把当前金样倒写成修前事实。

## del_body 失败迭代核对

现存证据能确认的命令/观察如下（时间和 argv 以各自 `.command.json` 为准）：

| 迭代 | 可核输入 | 实际观察 |
|---|---|---|
| `crossing-third-run`（10:32:13 UTC，exit 101） | 命令是 `cargo test -p aline-edit --test crossing --locked --offline`。旧 TSV 行未保全，stdout/stderr 不含 case 名或命令参数，故 source、range、expected/replacement、selection 均不可追认。 | stdout 的唯一失败为 `RereadFailed { reason: VisibleTextChanged }`。不能仅凭该日志定位具体测试行。 |
| `crossing-fourth-run`（10:32:32 UTC，exit 101） | 同一 argv。日志明确报 `del_body: planning refused with RereadFailed`，但没有打印该行输入字段；旧 TSV 行未保全，source、range、expected/replacement、selection 仍不可追认。 | 只确认 `del_body` 计划拒绝及拒绝大类 `RereadFailed`；该轮日志不能确认具体 `RereadFailure` 子类。 |
| `crossing-fifth-run`（10:32:49 UTC，exit 101） | 旧 TSV 仍未保全，故不能从命令日志复核该轮 `del_body` 的全部输入字段。 | 失败已推进到后面的 `del_zero_empty` 断言，说明按当时测试的顺序，之前的 `del_body` 未再导致失败；不能据此单独证明旧行各字段值。 |

当前最终金样的 `del_body` 行是唯一保全的输入：source=`"alpha beta gamma\n"`，selection=`6..11`，Replace range=`6..11`，expected=`"beta "`，replacement=`""`；手算删除中间的 `beta ` 后为 `alpha gamma\n`，目标选区 `6..6`。此行由最终 `crossing_golden_cases` 加载并通过。我的工作记忆是早期方案使用段尾删除，但没有旧 TSV/diff 留档，不能据此证明其精确 source/range/selection/oracle；因此段尾尝试的精确输入不可追认，记忆不作为历史证据。

改为中部删除可能是为避免段尾空白在候选文本解析/显示时被规范化，从而触发 `VisibleTextChanged`；这是结合最终前后文本和失败类别作出的事后推断，现存记录没有保存当时的决策说明，不能作为已证实根因。特此把“harness/TSV 调试”拆分为已证实观察与未保全输入，不再把推断写成事实。

## 最终验证

原始 `final-edit-*` 与 `final-fmt-check` 记录只保存 argv/cwd/time/exit，没有保存进程环境；其中 `--locked --offline` 可从 argv 核实，原文曾写的 `CARGO_NET_OFFLINE` 与工具链版本当时未被机器记录。为闭合该证据缺口，新增 `env-final-*` 记录并显式覆盖 `CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`：实际 `cargo --version` 与 `rustc --version` 均为 1.98.1；同一环境下包级测试 exit 0（crossing 1、goldens 6、protection 4、refusals 16），all-target Clippy、fmt 与 diff-check 均 exit 0。每条命令 JSON 记录 argv、cwd、UTC 时间、exit、环境覆盖与生效环境，原始 stdout/stderr 配对保留；索引见 `commands.txt` 与 `post-fix.txt`。没有跑 `cargo test --workspace` 或 workspace 全门禁，留 R14。

受测 HEAD `96024867f7ae949d7328232b05d93ed5bc6033d9`；最终源码 SHA-256：`crossing.rs` `5c26aef4bda9a8b1afee8b1e9f947a1d03edec5957782d9c820389054b54b68c`，`refusals.rs` `0abf16b447c5f73066008d71869a45be152ddc14f153927454ffa7241423a070`；TSV `bb42911b2cceca7e9b0eee03f2599c19f7f303b85f75b05c08e5bbf0d7fc4be9`。最终命令未提交改动。

区间诊断命令在临时测试源码中增加了一条 `eprintln!` 以保全完整 `Parsed` 范围输出，随后移除该诊断行；诊断时 `crossing.rs` SHA-256 为 `d3e66dcfd9b3e5c0fa8be92663889de085c3fc570a2fdae1bccf26ecc1a8646d`。最终回归源码 SHA-256 为正文记录的 `5c26aef4bda9a8b1afee8b1e9f947a1d03edec5957782d9c820389054b54b68c`。
