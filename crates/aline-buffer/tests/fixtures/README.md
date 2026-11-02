# 正逆补丁与行索引金样

日期：2026-10-31。本目录的金样在实现 `StringBuffer` 之前按引擎 §9.1 与 buffer spec 手写；随机部分只用 Python 标准库 `random.Random(20261031)`，不新增 Rust 依赖，也不把生成器输出当作唯一正确性来源。`generate_goldens.py` 会先用独立的逐段拼接函数复核下表的正逆结果，再输出 TSV；实现测试只读取 TSV，不调用生成器。

坐标均为 UTF-8 字节、半开区间，正补丁用 before 坐标，逆补丁用 after 坐标。`after_anchor/after_head` 是应用后的原始字节位置，尚未绑定快照；测试必须经 `TextSnapshot::src_offset` 构造选区。

## 手写案例

| id | before | 正补丁 | after | 逆补丁 | before 选区 | after 选区 | 说明 |
|---|---|---|---|---|---|---|---|
| hand-01 | `abcXYZ` | `1..2→QQ`、`3..6→!` | `aQQc!` | `1..3→b`、`4..5→XYZ` | `(6,2)` | `(5,1)` | 引擎 §9.1 例子；逆坐标计入前项长度差，选区方向保留 |
| hand-02 | `a` | `1..1→b` | `ab` | `1..2→""` | `(1,1)` | `(2,2)` | 末端插入，逆为删除 |
| hand-03 | `ab` | `0..1→""` | `b` | `0..0→a` | `(0,2)` | `(1,0)` | 首字符删除，逆为零宽插入 |
| hand-04 | `abc` | `0..1→""`、`1..2→""` | `c` | `0..0→ab` | `(3,3)` | `(1,1)` | 相邻删除在 after 同点；逆补丁必须合并，不能出现同点歧义插入 |
| hand-05 | `ab` | `0..1→X`、`1..1→Y` | `XYb` | `0..2→a` | `(2,1)` | `(3,1)` | 替换与边界插入的逆范围相接，合并后恢复原文 |
| hand-06 | `甲a乙` | `3..4→🙂` | `甲🙂乙` | `3..7→a` | `(7,3)` | `(10,3)` | 多字节替换；端点仍为 UTF-8 边界 |
| hand-07 | `a\r\nb` | `1..3→\n` | `a\nb` | `1..2→\r\n` | `(4,3)` | `(3,2)` | 行尾字节按普通字节区间编辑，不做规范化 |
| hand-08 | `0123456789` | `1..3→AB`、`6..8→""` | `0AB34589` | `1..3→12`、`6..6→67` | `(10,0)` | `(8,2)` | 多补丁；第二逆范围计入前项净长度不变 |
| hand-09 | `abc` | `0..0→甲` | `甲abc` | `0..3→""` | `(3,0)` | `(6,3)` | 空区间插入按协议生效，不是 no-op |
| hand-10 | `abc` | `1..2→b` | `abc` | 空 | `(2,1)` | `(2,1)` | 内容相同替换不产生历史步，状态不变 |
| hand-11 | `abc` | `1..1→""` | `abc` | 空 | `(1,1)` | `(1,1)` | 空文本插入为 no-op，状态不变 |
| hand-12 | `abc` | `0..0→""`、`1..2→B` | `aBc` | `1..2→b` | `(3,2)` | `(3,2)` | no-op 补丁被剔除，只记录实际补丁 |
| hand-13 | `abcd` | `1..2→X`、`2..3→""` | `aXd` | `1..2→bc` | `(4,1)` | `(3,1)` | 相接的替换/删除逆范围合并 |
| hand-14 | `abcdef` | `1..2→X`、`4..5→Y` | `aXcdYf` | `1..2→b`、`4..5→e` | `(6,2)` | `(6,2)` | 有间隔的多补丁保持分离 |

## 随机案例

`random-*` 由固定种子生成，覆盖 ASCII、中文、emoji、CRLF、LF、Tab 与空替换；每个案例仍由生成器独立计算 after 与逆补丁，并断言逆补丁能恢复 before。随机库只用于生成金样，不进入 `aline-buffer` 依赖。

## 行索引金样

`line-index.tsv` 手写覆盖空文本、无换行、LF 末尾、CRLF、孤立 CR、连续换行、多字节字符与行数预算拒绝。行内容范围不含末尾 `\n`，若其前紧邻 `\r` 也一并排除；孤立 `\r` 不是换行。`line_of_offset` 对换行字节归属当前行，文本末尾归属最后一行。

## 文件格式

`forward-inverse.tsv` 为制表符分隔，字段依次为：

`case_id、before_hex、patches、after_hex、inverse、before_anchor、before_head、after_anchor、after_head、expect`

- 文本与替换文本为 UTF-8 bytes 的小写十六进制；空串为空字段。
- 补丁为 `start:end:replacement_hex`，多个用逗号分隔；空列表为 `-`。
- `expect` 为 `changed` 或 `unchanged`；`unchanged` 案例的 inverse 为空。
- `#` 开头行是注释。

`line-index.tsv` 字段依次为：

`case_id、text_hex、max_lines、line_count、ranges、offsets、expect`

- `ranges` 为 `start:end` 列表，`offsets` 为 `offset:line` 列表，多个用逗号分隔，空列表为 `-`。
- `expect` 为 `ok` 或 `line_count_exceeded`。
