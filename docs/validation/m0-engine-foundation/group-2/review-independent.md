# 任务 2.2 独立中途审查（2026-10-31）

审查范围：正式八成员依赖图模型、采集入口、GPUI 精确例外及已有基线文件。只新增本组 `review-*` 证据，不修改实现、任务或正式 policy。任务 2.2 未完成，不建议将当前工程门禁作为已完成状态合入；不判 E1 或任何 V 项通过。

## 实际复核

完整命令、时间和退出码在 `review-recheck.command.json`，输出在 `review-recheck.stdout.json`；脚本为 `review-recheck.py`。复核只变异内存图，非真实 Cargo 违规清单。`policy.reviewed=true` 只设置于进程内副本，以检查待批准指纹；正式文件仍为 false，**未批准正式基线**。

- all、macos-arm、macos-intel、linux-x64、linux-arm：当前内存图校验通过。它们不是平台构建或支持证据。
- windows-x64：校验失败，tree 选包的 `libc@0.2.191` 在 Windows filter-platform metadata 的选包子图中无法由八成员根到达。
- all-features：缺少对应 tree，未执行。
- 总脚本退出码 1，不能记成正常门禁通过。
- 一次初版复核脚本误以为正式图仍有旧 wasi 包，实际不存在而触发 StopIteration；已改为当前实际 wasip2，重新执行的原始日志及退出码如上。该初版失败不属于 Cargo 或产品失败。

## 发现、修复及未决项

1. **P1，已修复并定向重验**：`engineering_dependencies.py::check_structure` 原本只核对成员直接内部依赖，允许 `aline-buffer → review-bridge → aline-source`。初次独立内存图复现确实返回 PASS；实现者已增加每成员完整可达内部方向检查。当前重验明确拒绝并打印两条实际边。需在完整自测补真实 Cargo 中间包夹具，现有定向重验不能替代任务规定的完整自测。
2. **P1，潜在盲区已加固，未完成该分支完整独立负例**：原本 `full - selected - gpui_full` 无法检查已有 GPUI metadata-only 节点被自有另一路径引入的情形。实现者增加 `full_outside - selected` 检查；本次独立重验实际选中的 `wasip2` 自有旁路被拒绝。未把该结果写成 metadata-only 分支负例已执行。
3. **P2，采集标签检查已补，尚未独立负例验证**：离线图入口原本仅核命令名、退出码和输出 hash，不核 scenario 对应 target/features；实现者已补目标/feature/边集合参数校验。完整误标签负例未执行。
4. **P1，未解决**：`engineering_dependencies.py::validate` 对正式 windows-x64 基线拒绝，诊断为 metadata/tree 选包可达性不一致。触发输入见上述 Windows metadata/tree，差集只有 `libc@0.2.191`。影响是当前默认七组合采集即使获得这些成功 Cargo 结果也不能通过门禁。需要区分目标与宿主 build/proc 依赖的实际解析关系后修图模型，并补正反例；不能直接删除 libc 或把 Cargo 图不一致写成通过。原因尚未定位，不作平台能力结论。

## 准入差异与限制

`engineering-baseline-diff.json` 显示 all-target 子图只增加已准入 toml 导致的 toml_datetime serde feature；桌面配置关闭旧候选的 font-kit/windows-manifest 特性。当前未发现新的禁库例外需求，但正式 policy 仍未获批准，完整来源/checksum/边/target/features 漂移反例未执行，不能仅凭差异报告关闭准入。

按主 Agent 要求因 source 契约裁决收敛，本次未继续扩量测试。完整 `--self-test` 尚未实现、实际入口正常图端到端运行及恢复正常图均未验证；应保留任务 2.2 未勾选。
