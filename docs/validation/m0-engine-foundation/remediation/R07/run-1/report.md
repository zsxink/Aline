# R07 / F-10：字素簇边界与 UTF-8 标量边界分开验收

- 批次：R07（依赖 R03，已完成 `128954e`）
- 基线 HEAD：`74b876e`
- 实现者：主 Agent（两个 Sub-agent 名额当时被 R05b / R06 占用）
- 代码改动：**仅测试**。`crates/aline-markdown/tests/mapping.rs` +293 行，16 → 17 条用例；`crates/aline-markdown/src/**` 零改动。

## 1. 失败事实与根因

`openspec/changes/m0-engine-foundation/specs/engine/markdown/spec.md` Scenario「重复文字与字素」要求
「依据构建来源定位，**字素移动边界与标量边界分开验证**，不用字符相等回搜猜来源」。

基线状态（`pre-fix.txt` 为原始命令与输出）：

| 检查 | 基线结果 |
|---|---|
| `git show HEAD:tests/mapping.rs \| grep -c -e grapheme -e 字素` | `0` |
| `grep -rn -e grapheme -e 字素 crates/aline-markdown/src/` | 无命中 |
| `tests/mapping.rs` 用例数 | 16 |

也就是说：映射层只校验 UTF-8 标量边界（`drawn_offset` → `points` 二分查找，非标量边界返回
`NotCharBoundary`），**从未区分**「标量边界」与「用户字符（字素簇）边界」。关键反例：
`e\u{301}` 的标量边界是 `{0,1,3}`，字素簇边界是 `{0,3}`；`drawn_offset(1)` 现在**成功**，
而 1 不是用户字符边界。这个区别此前完全没有被验收。

本批不改实现，只补验收载体，因此「无修复前后行为差异」——`pre-fix.txt` 已写明这一点并给出实际运行。

## 2. 新增用例：`grapheme_and_scalar_boundaries_verified_separately`

表驱动，9 行夹具（≥6 类簇要求已满足），**两套边界集合全部手写**，不调用任何实现生成：

| # | 类 | 源码（`\n` 为尾部换行） | 绘制 | 字节 | 标量边界（手写） | 字素边界（手写） | 差集 | 依据 |
|---|---|---|---|---|---|---|---|---|
| 1 | 组合附加符 | `e\u{301}\n` | `e\u{301}` | 3 | `{0,1,3}` | `{0,3}` | `{1}` | UAX #29 GB9：`U+0301` 的 GCB=Extend，不得在基字与 Extend 之间断 |
| 2 | 双组合附加符 | `e\u{301}\u{302}\n` | `e\u{301}\u{302}` | 5 | `{0,1,3,5}` | `{0,5}` | `{1,3}` | GB9：连续 Extend 全部并入基字，链式不断开 |
| 3 | ZWJ 家族 | `👨\u{200D}👩\u{200D}👧\n` | 同源码 | 18 | `{0,4,7,11,14,18}` | `{0,18}` | `{4,7,11,14}` | GB11：`ExtPict × ZWJ × ExtPict`，ZWJ 及其两侧 emoji 同簇 |
| 4 | 肤色修饰 | `👍\U{1F3FD}\n` | 同源码 | 8 | `{0,4,8}` | `{0,8}` | `{4}` | GB9：`U+1F3FD` 的 GCB=Extend（Emoji_Modifier） |
| 5 | 区域旗帜 | `🇨🇳\n` | 同源码 | 8 | `{0,4,8}` | `{0,8}` | `{4}` | GB12/GB13：两个 regional indicator 成对合为一簇 |
| 6 | 变体选择符 | `❤\u{FE0F}\n` | `❤\u{FE0F}` | 6 | `{0,3,6}` | `{0,6}` | `{3}` | GB9：`U+FE0F` 的 GCB=Extend |
| 7 | 中文（对照） | `中文字\n` | `中文字` | 9 | `{0,3,6,9}` | `{0,3,6,9}` | `{}` | GB999 任意：每个 CJK 表意字自成一簇 |
| 8 | 重复空白（对照） | `a  b\n` | `a  b` | 4 | `{0,1,2,3,4}` | `{0,1,2,3,4}` | `{}` | GB999：普通空格之间允许字素断开，每个空格自成一簇 |
| 9 | CRLF | `a\r\nb\n` | `a\nb` | 3 | `{0,1,2,3}` | `{0,1,2,3}` | `{}` | GB3/GB4：CR 与 LF 之间不断；映射层把 `\r\n` 折成单个 `\n` |

CRLF **可构造**（不是跳过）：实测 `parse` 不把 `\r\n` 规范化为快照，但绘制层把它折成单个 `\n`，
`\r` 归入 `Aligned` 片段（`drawn 1..2 / src 1..3`）。因此该类字素边界等于绘制标量边界，是
「CRLF 属同一字素簇（GB3）」与「映射层折叠」两者叠加的结果，已作为第 9 行断言。

## 3. 断言分工（分开，不合并）

用例内按 6 步顺序断言，每一步都是集合级比较而非散点断言：

1. `case.rule` 非空 —— 依据必须写明，否则两套边界只是数字对照。
2. 绘制文本与字节长度等于手写常量。
3. **接受集合恰等于标量集合**：`accepted_drawn_offsets()` 遍历 `0..=drawn.len()` 收集
   `drawn_offset` 成功者，整段 `Vec` 与手写 `scalar` 比较——多接受一个偏移也会红。
4. 手写字素集合 ⊂ 标量集合；`scalar \ grapheme` 计算后与手写 `diff` **逐元素**比较；
   并断言字素集合含 `0` 与绘制末端。
5. **字素移动是纯数据断言**：`grapheme_move_trajectory()` 与 `grapheme_move_reverse()`
   只读手写集合，产出 `Vec<usize>` 轨迹，正向必须恰等于字素集合、反向逆序后必须一致。
   未新增任何 UI 或输入处理。
6. 差集里的每个偏移必须**在标量层面被接受**、且**不得出现在移动轨迹**中——这正是两套边界的分野；
   并断言「差集为空 ⟺ 两套边界相等」，把对照组的「相等」也变成被断言的结果而非假设。

**没有**用「字符串包含 ZWJ」冒充覆盖：`zwj_family_emoji_stays_exact_before_hidden_newline` 原有的
片段级断言保留不动，本用例单独落到具体字节边界表。

## 4. 自证：新用例是否真的钉死两套边界（`self-proof.txt`）

临时变异手写常量 → 必须变红 → 已还原（还原后 sha256 与变异前一致）：

| 变异 | 期望 | 实际 |
|---|---|---|
| M1：把 case 1 的**标量**集合改成 `{0,3}` | 接受集合断言变红 | FAILED `left: [0,1,3] / right: [0,3]`，exit 101 |
| M2：把 case 1 的**字素**集合改成 `{0,1,3}` | 差集断言变红 | FAILED `left: [] / right: [1]`，exit 101 |
| M3：把 case 3 字素集合改成 `{0,7,18}` | 差集断言变红 | FAILED `left: [4,11,14] / right: [4,7,11,14]`，exit 101 |

即：标量侧与字素侧各有一个独立断言会红，两套边界不是同一份数据的两种写法。

## 5. 验证结果（`commands.txt` / `post-fix.txt` 为原始输出）

| 命令 | exit | 结果 |
|---|---|---|
| `cargo test -p aline-markdown --test mapping --locked --offline` | 0 | 17 passed / 0 failed |
| `cargo test -p aline-markdown --locked --offline` | 0 | 12 段 `test result: ok`，合计 **118 passed / 0 failed**（基线 117） |
| `cargo clippy -p aline-markdown --all-targets --locked --offline -- -D warnings` | 0 | 无告警（`rule` 字段已用断言消费，无 dead_code） |
| `cargo fmt --check` | 0 | 先 1，`cargo fmt -p aline-markdown` 后 0 |

工具链 `rustc 1.98.1 (48a229cea 2026-09-01)`，全部 `--locked --offline`，`CARGO_NET_OFFLINE=true`。
既有 16 条用例断言零删改（diff 仅 +293 行、0 行删除）。测试内无 `unwrap()/expect()`。

## 6. 未实现部分与对 V-07 的依赖

仓内**没有**字素分段实现，本批按契约也**没有**引入：

- 未新增任何 Unicode 分段依赖（`Cargo.toml` 零改动）；
- 未手写 UAX #29 分段算法——第 2 节的边界集合是**测试常量**，不是可复用的实现；
- 未新增公共 API（`src/**` 零改动）。

`docs/engine-design.md:246` 只说「建议映射服务提供字素边界适配」，`design.md:44` 规定
「Unicode 分段依赖准入」且金样由 V-07 收口。因此本批交付的是**验收分离**，不是**引擎级字素适配**。
是否需要引擎级适配器才能算该 Scenario 完全达成，留待准入 + V-07 金样裁决（见 `handoff.md`）。

## 7. 影响面

- 受影响 Requirement/Scenario：`engine/markdown` Scenario「重复文字与字素」（验收侧，非实现侧）。
- 消费者：无生产代码改动；仅 `aline-markdown` 测试。
- 未触碰：`crates/aline-markdown/src/**`、`crates/aline-buffer/**`、`crates/aline-edit/**`、
  任何 `Cargo.toml`、任何 `scripts/**`。
- 本批为补测试，`pre-fix.txt` 已注明「无代码改动，无修复前后差异」并附实际运行。

## 独立复核勘误（2026-11-01）

reviewer 指出旧版「UAX #29 不在空格之间断开」与 GB999 `Any ÷ Any` 相反。已更正表格与测试夹具的规则说明，边界常量未变；官方依据为 [UAX #29 GB999](https://www.unicode.org/reports/tr29/tr29-49.html#GB999)。复核结论仍限于本批纯数据验收，不证明引擎导航，见 [review.md](review.md)。
