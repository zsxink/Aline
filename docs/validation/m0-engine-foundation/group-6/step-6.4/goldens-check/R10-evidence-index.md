# 2026-11-01 R10 证据身份补记

这是本轮新增索引；不覆盖同目录旧探针或日志。原 `probe-main.rs` 直接导入并调用
`aline_markdown::parse`，属于「同实现的第二宿主」，不能充当独立期望来源。

真正的本轮独立实现仅依赖第三方 `pulldown-cmark`，读取 R09 `basic.md` 原始字节，
见 [R10 报告](../../../remediation/R10/run-1/report.md)、
[独立源码](../../../remediation/R10/run-1/independent/src/main.rs)、
[独立原始输出](../../../remediation/R10/run-1/independent-output.txt)、
[逐行 diff](../../../remediation/R10/run-1/diff.txt)。

原报告末尾已[追加勘误](../report.md#6-勘误2026-11-01r10)。
不追认旧 TSV 的创作方式，不补造先行时间。
