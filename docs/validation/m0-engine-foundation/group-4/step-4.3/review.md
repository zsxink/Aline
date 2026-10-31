# 4.3 独立只读复核

2026-10-31 独立 reviewer（未参与实现或修改文件）复核了 PRD、引擎、架构、change 文档与 4.3 夹具/测试/API；未发现未解决阻塞项。复核者独立报告实际运行 `cargo test -p aline-source roundtrip`（退出 0，新增 7 项及既有 guard 1 项）、`independent-check.py`（退出 0，16 例）和 `check-api-fixtures.py`（正例成功；bool E0061、旧 EncodedBytes E0308；脚本退出 0）。完整门禁由主 Agent 执行，证据见 `report.md` 和 `gates/`。

复核初次提出两项文字问题：孤立 CR 守卫应说明 CRLF 紧跟在孤立 CR **之后**；change design 不应把存储行尾等价证据作为原 bytes 通道的替代条件。主 Agent 已同步修订，复核者再审确认关闭。

复核确认宿主仅为测试模拟，成功以实际 bytes 建立基线、失败保留旧基线和输入、较新输入不误报保存、基线替代后 Undo 不恢复旧 bytes、保护性 BOM 成功后 `has_bom=true`；行尾等价不放行精确源码原 bytes。编译反例由单独脚本执行，不属于 roundtrip 过滤测试。不能据此声称文件 I/O、保存队列、真实 Undo 历史或 M0 出口通过。
