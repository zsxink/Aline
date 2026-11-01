# R10 独立复核

复核 Agent：`r10_evidence_review`（未参与 R10 实现）；复核范围：独立推导工具、逐行比较、历史保全、命令记录与报告勘误。复核过程只读，未运行 Cargo。

## 结论

**APPROVE_WITH_CONCERNS，建议接受 R10。** 未发现有证据的阻断问题。

## 核验事实

- 独立工具只依赖 `std` 与 `pulldown-cmark`，仅读取夹具原字节；不调用 Aline API、不读旧期望、不执行被测程序。完整依赖树与源码一致。
- 独立复核 51/51 行完全相等；输入夹具 319 字节及 SHA-256 与记录一致。49 个片段、197 个绘制字节结论成立。
- 旧报告只追加 14 行；原前 77 行与 HEAD 及保存的 before 副本相同。19 个历史文件的 hash/size/mtime 与 54 项当前源码输入指纹无变化。
- 原始日志证实独立工具 4 项测试、Clippy 与格式复验通过；命令记录包含离线环境及工具链。首次 fmt 参数遗漏已披露并保留，后续完整 flags 复验通过。
- 勘误撤回旧探针的“独立推导”定性，正确区分独立算法、第二宿主和实现输出，没有追认旧 TSV 来源或先行时间。

## 保留限制

独立复算只覆盖当前 `basic.md`；它复用 `pulldown-cmark`，且实现者读过既有材料。因此不构成盲写期望、第三方解析器正确性证明或 V-07/M0 出口通过。

## 复核原始摘要

```text
raw comparison 51/51 equal; fixture 319 bytes/SHA exact
original report matches HEAD prefix
historical_files_checked=19; current_input_hashes_checked=54
appended_lines=14; heading_line=79; diff_bytes=0; issues=[]
```

本结论只覆盖 R10 的独立证据与历史勘误，不替代 R14 全量出口审查。
