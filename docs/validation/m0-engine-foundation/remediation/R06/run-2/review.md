**APPROVE_WITH_CONCERNS，建议合并 R06b。** 独立复核；未参与实施或原审查。脚本与 `15502eb` 逐字节一致。

- **D1 关闭**：`scripts/check_safety.py:188–223` 按真实路径拒绝逃逸。自建目录/文件链接均 exit 1 / `external-target-path`；成员内目录链接落点含 unwrap，实报 `engine-unwrap`。
- **D2 关闭**：65/65 self-test 通过。独立撤销链接边界、target 存在性、去重检查，均使对应 case 变红；悬空 target 恰报 1 条 `target-path-missing`。原缺失断言已有效覆盖。
- **D3 关闭**：`report.md` §8.1 和 handoff 追加勘误明确撤回旧 §5 第 4 条；旧新扫描中的唯一 `src/build.rs` 均为 `engine-production`，撤回准确。
- **D4 保留（低，非阻断）**：`scripts/check_safety.py:164` 对 metadata `targets=[]` 不显式拒绝；Cargo 替身返回该结构时仍 exit 0、扫 8 文件，可能丢失真实入口验证。真实 Cargo 对无 target 包 exit 101，属已登记限制；建议拒绝空 targets 并补反例。

仓库 exit 0、89 文件，与 `60ace78` 的 `(file, kind)` 双向差集为空。输出隔离在 `/tmp`；原 safety-results、脚本与原报告前后 SHA-256 一致。命令、自测、变异与可复跑探针均在 [证据目录](review-evidence/)，见 [汇总](review-evidence/facts.json)。
