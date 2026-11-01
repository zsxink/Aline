# R05 run-1 报告：F-08 lint 继承必须真实解析

基线 HEAD：`c7232ed`（R05 起于 R00 之后）。工具链 1.98.1，`CARGO_NET_OFFLINE=true`，Python 3.9.6。
所有权：`scripts/check_lints.py` + 本批次证据目录。

## 1. 根因

旧 `tables()` 是逐行状态机：见到 `[...]` 就当表头，见到 `key = value` 就按正则入表，
完全无视 TOML 词法。夹具 `lint-decoy/crates/aline-buffer/Cargo.toml` 的

```toml
[package]
description = """
[lints]
workspace = true
[package.metadata.decoy]
"""
name="aline-buffer"
```

`description` 的多行基本字符串内部三行被当成真实语法，于是凭空造出 `lints` 表并填入
`workspace = true` → 守卫 fail-open（`pre-fix.txt`：decoy exit 0）。

Cargo 实测（`cargo-control.txt`）：伪继承 exit 0、真实继承 exit 101
（`missing documentation for the crate`）。差异恰好被旧守卫漏判。

## 2. 解析器方案

内置递归下降 TOML 解析器，仅标准库（Python 3.9.6 无 `tomllib`，环境无 `tomli`/`toml`，未联网）。

- `_TomlParser`：表头 / 键 / 值 / 字符串 / 数字 / 日期 / 数组 / 内联表
- `parse_toml(text, source)`：出错一律 `CheckError`（`文件:行号: 原因`）
- `tables(path)`：保持原签名，解析后展平为 `{"a.b.c": table}`
- `_strict_equal`：区分 bool/int，防 `True == 1` 混过
- `audit_member_manifest`：fail-closed——`lints` 缺失、类型错、键集非 `{"workspace"}`、
  值非 `True` 全部拒绝

覆盖：`[table]` / `[[array-of-tables]]` / 点分表头、裸键 / 引号键 / 点分键、基本字符串与全部转义、
`"""` 与 `'''` 多行（含续行与 4/5 引号、拒 6 引号）、注释、布尔、十/十六/八/二进制整数
（含 `_` 与 i64 范围）、浮点含 inf/nan、日期时间（校验月日时分秒与偏移）、多行数组 / 嵌套数组 /
尾随逗号、内联表。

拒绝：BOM、裸 CR、控制字符、未知转义、`\u` 位数不足、代理区、未闭合 / 跨行字符串、
重复键 / 表、点分键冲突、内联表被表头扩展、**点分键已定义表再用表头定义**、
普通表与数组表同名、内联表尾随逗号、表头缺 `]`、行尾残留、非 ASCII 裸键。

## 3. 逐条断言对照

| 目标 | 证据 |
|---|---|
| 真实解析 | `parser_cases` 通过（定稿值见 §7） |
| 无外部依赖 | 仅标准库 |
| fail-closed 语义 | 全部拒绝路径抛 `CheckError` |
| 伪继承拒 / 真实继承过 / Cargo 对照 | `f08-decoy-guard` exit 1、`f08-real-guard` exit 0、`f08-decoy-cargo-control` exit 0、`f08-real-cargo-control` exit 101 含 `missing documentation` |
| 正常仓库不回归、self-test 不缩水 | 原有 30 条断言全保留，总数 30 → 56+ |

TDD 顺序：`pre-fix.txt`（decoy exit 0 误判）→ 改脚本 → `post-fix.txt`。

## 4. 额外交叉验证

- 语料回归：仓库内 9464 个 `Cargo.toml` / `Cargo.lock`，0 解析失败。
- 差分测试：离线构建 Cargo 同版本 `toml 1.1.8+spec-1.1.0`（本地 registry 缓存）作对照，
  81 条构造中判定一致 79 条，差异 2 条（内联表尾随逗号、BOM）**均为 fail-closed**。
- 差分中修掉一处 **fail-open**：`a.b = 1` 后写 `[a]`，Cargo 报 duplicate key 而初版放行；
  已加 `state="dotted"` 标记修正（`check_lints.py:245` 标记、`:303` 拒绝）。

## 5. 未覆盖的 TOML 构造（12 条）

**有意偏离（更严，fail-closed）：**
1. 内联表尾随逗号（1.1 接受，我们拒）
2. BOM（1.1 接受，我们拒）

**未覆盖：**
3. TOML 1.1 `\e` / `\x41` 转义（按 1.0 拒绝，会过度拒绝；实测 `a = "\x41"` → `未知转义序列 \x`）
4. 1.1 宽松异构数组（不检查，与参考一致但无自检断言）
5. 多行字符串裸 CR（统一 `\r\n → \n`，参考保留）
6. 可省略秒的时间 / 日期时间（我们拒，fail-closed；实测 `time = 07:32` → `无法识别的 TOML 值`）
7. 小数秒位数上限不限制
8. Unicode 裸键拒绝与参考一致但无断言
9. `+0x1` / `0x_1` 前缀符号与下划线位置无专门断言
10. 未做 Unicode noncharacter 检查
11. 表 / 数组表深层语义边界只固化 2 条，其余组合未断言
12. 注释非 ASCII / 超长行无专门断言

## 6. 结论

F-08 修复且方向正确。残余风险仅第 3、6 条会**过度拒绝**、不会误放行。

## 7. 定稿与勘误

原 `handoff.md` 记录 `sha256 f9f64124…` / `mtime 12:16:54Z` 为写 evidence 时的中间态；
定稿为 `497923a5…` / `12:19:41Z`，其间收尾了 dotted-key 修正。原述「已纳入自检
`reject-dotted-key-then-header`」在落盘时并不成立（`PARSER_CASES` 实为 22 条、无此用例），
已补上该用例，`parser_cases` 22 → 23。勘误均按 §2 追加，未改写原文。
