# R11 首轮独立复核

结论：**REQUEST_CHANGES；暂不建议合并 R11 交付。** reviewer 未参与原采集；只读检查，不修改任何文件。运行基线为当前工作树的 `run-1/` 产物。复核原始消息由主 Agent 据其结论落盘；reviewer 本轮没有将 stdout/stderr 写入仓库，故下列为结果记录而非伪称的逐字原始命令日志。实施修复与重新采证见后续运行目录。

## Findings

1. **P1：默认 collector 复跑可覆盖已脱敏日志并重新落盘敏感标识。** `run-1/collect.py:34–51,67–69,78–83` 在唯一写入口将 `subprocess` 原始 stdout/stderr 直接写文件；模拟真实 `main()` 时，硬件序列号、Hardware UUID、Provisioning UDID、uname hostname 均原样进入内存日志并覆盖预置 `[REDACTED]`，退出 0；模拟不启动真实子进程、不写实际文件。默认 `python3 -I collect.py` 和选择硬件/uname labels 均受影响。与 `report.md` 的“落盘前打码、复跑不覆盖”说法相反。
2. **P2：可复跑脚本无法生成已交付的17条命令记录。** 其 `CASES` 只有15条；缺两个 rustup target 查询。传入缺少 label 时生成0条仍退出0。落盘的两条记录本身完整，但原报告将17条全部归因于此脚本不可复核。
3. **P2：报告把 GPUI 0.3.9 的 regex 全闭包说成当前准入障碍。** `run-1/report.md:102–103` 与现行 [架构 §3.2](../../../../../v1-architecture.md#32-栈与依赖的状态) 和 [引擎 §18.2](../../../../../engine-design.md#182-依赖准入与未解决实现风险) 的已批准精确路径例外矛盾；应标为当时历史状态，并引用当前例外；本轮并未重新验证准入或平台构建。
4. **P2：legacy-gaps 未保留四个 safety 命令已有 `env_overrides` 原值。** 其中包含 `CARGO_HOME`、`CARGO_TARGET_DIR`、`RUSTUP_TOOLCHAIN`，与“既有字段原值”的约定不符。缺少的 cwd/时间必须继续保持未知，不能补造。
5. **P3：采集窗口结束时间不准确。** 报告和 handoff 写至 `04:36:48Z`，实际两个 target 命令记录结束于 `04:39:25Z`。

## 已核实事项

- `commands/` 有17组 JSON/stdout/stderr（51文件）；必需字段、UTC、cwd、env、argv、文件身份相互一致，17条 exit 均为0且 stderr空。该一致性不证明历史上由所称脚本执行。
- 105份旧 command JSON 中缺字段精确为14份：4个 safety 记录缺 cwd/起止，10个 tree 记录缺起止；文件集合和已摘录字段值吻合。12条 `environment-commands.json` 均无 cwd/起止。
- 候选 `gpui-pre =0.3.9` 的 repo lock、candidate lock、旧 identity checksum 三方相同：`34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0`；source 一致，manifest/ledger 的版本、license、Zed revision 与摘录一致。未重新下载或重算归档。
- `run-1/commands` 的硬件日志三处及 uname 主机名一处已脱敏；对该目录查 UUID/未打码 serial/UDID/旧 hostname 未发现残留。此结论不扩展到仓库其它历史。
- README 第46行仍保留旧 host；第120–122行有勘误，但没有移除旧值。跨平台/CI/旧时点缺项仍正确标记未验证；未声称历史缺项恢复或三平台构建完成。

具体检查清单与退出状态摘要在 `review-evidence/checks.md`。需修 collector 并作安全新采集；历史输入按追加勘误处理，旧日志保留。
