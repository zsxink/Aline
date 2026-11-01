# R11 run-3 独立复审

结论：**APPROVE；建议合并本次 R11 整改。** 未发现有证据且需要修复的问题。该结论仅覆盖 R11 run-3 交付，不代表 M0、V 验证、三平台或依赖重新准入通过。

本 reviewer 未参与实现，仅新增本报告和 `review-evidence/`。未运行 run-1 采集器、真实设备命令、Cargo 构建或重新准入；只用 fake runner 复测。审查先执行 CodeGraph，再核对当前代码。隐私检查只输出布尔值和命中计数，原始 hostname/serial/UUID/UDID 未写入本次审查产物。

## 独立验证结果

- **写盘前脱敏与异常路径**：`run-3/collect.py:48–65,87–108` 先在内存发现双流标识，再在首个文件打开前脱敏；捕获的子进程 OSError 只记录类型和 errno。原有 7 项 fake 自测全部通过，覆盖命令非零与 OSError。额外在 `write_new` 调用入口拦截全部 51 次写入，检查模拟敏感值命中为 0，并验证“stderr 提供标识、stdout 重复标识”的反向发现和普通硬件字段保留。证据：[自测](./review-evidence/selftests.stderr.log)、[写入边界](./review-evidence/write-boundary.stdout.log)。
- **拒绝覆盖**：`collect.py:68–84,112–134` 默认时间戳加随机目录，显式目录必须不存在；旧 run-1 及其子目录、未知 label 在执行前拒绝。7 项自测验证默认两次目录唯一、显式旧目录内容保全、run-1 路径和未知 label 拒绝；额外验证文件独占打开拒绝已有文件，旧字节不变。
- **17×3 新采集**：独立固定 17 项预期 argv 与实现逐项一致，含两个 target 查询；51 文件集合精确，17 份 id/argv/cwd/env/UTC/exit 与文件名配对，全部 exit 0、stderr 空。逐条时序落在包装执行区间内，包装 stdout 与 17 份 exit 记录逐行一致。新窗口为 2026-11-01T08:32:46.881873Z～08:32:47.589533Z。证据：[独立核对](./review-evidence/independent-check.stdout.log)。
- **历史保全**：`legacy-hashes-before.json` 完整覆盖 run-1 的 59 个文件；58 个非报告文件（含 51 个 command/log 文件、旧 collector 和 legacy-gaps）保持哈希；报告追加节之前的完整原始字节与原哈希一致。`hashes.json` 的 10 项当前哈希全部一致。旧 105 条记录仍有 14 条缺项，旧窗口终点独立计算为 04:39:25Z。
- **环境补记**：四个 safety 记录的源 SHA-256、id、完整 env_overrides 对源文件逐项相等；源文件缺失的 cwd/起止时间仍未补造，补记明确保留这些缺口。
- **README 最小替换**：不只相信替换计数；从 git HEAD 在内存取得旧 hostname，将当前 README 指定字段反向还原后，全文 SHA-256 与 `readme-redaction.json` 的 before 哈希一致；当前全文与 after 哈希一致，故其它字节保持原样。README 和 run-3 共 100 个文件中旧/当前 hostname 命中数为 0；新采集日志的未脱敏设备字段/UUID 命中数为 0。原值没有进入输出或落盘证据。
- **文字与契约**：人工对照现行架构 §3.2、引擎 §18.2，run-3 补记和 run-1 追加勘误准确限定已批准锁定 GPUI regex 路径及非桌面 WASI p2 条件边，继续保留 Aline 自有直接依赖、五引擎和其它路径禁令、身份/来源/校验和/入边/target/features 限制；未扩大为全闭包许可，未宣称重新准入。旧报告错误和旧工具风险已有明确追加更正。
- **链接与规格**：15 个本地 Markdown 链接目标存在，run-3 JSON 与 Python AST 可解析。`openspec validate --all --strict --no-interactive`：2 passed / 0 failed。证据：[OpenSpec](./review-evidence/openspec.stdout.log)。

## 可复查证据与边界

`review-evidence/` 保存独立检查源脚本及每次检查的 `*.command.json`、`*.stdout.log`、`*.stderr.log`，命令记录包括 argv、cwd、UTC 起止和 exit。最终有效检查均 exit 0；审计包装器初次使用了上一级 cwd，CodeGraph 初始化检查曾 exit 1，纠正后重跑成功。初次日志被重跑覆盖，准确起止时间不可恢复，未补造，见 [设置勘误](./review-evidence/setup-correction.json)。CodeGraph 源码定位见 [记录](./review-evidence/codegraph.command.json)和[输出](./review-evidence/codegraph.stdout.log)。

历史哈希验证基于 run-3 留存的整改前快照；README 的替换前哈希另由 git HEAD 的旧字段在内存重建复核。采集元数据/输出一致性与 fake 拒绝测试不追认 run-1 的真实执行来源，也不替代真实设备重新采集或跨平台验证。磁盘中途失败可能保留部分新目录，补记已明确该目录不能当完整运行；本次未做真实磁盘故障注入。现有证据不支持新增阻塞问题。
