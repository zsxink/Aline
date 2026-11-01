# R07b 独立提案审查

结论：**REQUEST_CHANGES**。范围合理，但尚不足作为实施许可；补齐下列两项后再裁决、更新 change 并复审。

已独立复核：`Cargo.lock:2791–2794` 与本机 `.crate` 的 SHA-256 均匹配提案；`cargo tree -i unicode-segmentation --locked --offline -e all` 的完整链为 `unicode-segmentation 1.13.3 → convert_case 0.10.0 → derive_more-impl 2.1.1 → derive_more 2.1.1 → gpui-pre 0.3.9 → aline-ui → aline`。锁定包 `Cargo.toml.orig:3–18`、`src/tables.rs:17`、`src/lib.rs:50` 证实版本、MSRV、许可、Unicode 17、无运行时依赖及禁 unsafe；包路径为 `<cargo-home>/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-segmentation-1.13.3/`。

1. **P2，提案第 20 行：预算遗漏跨簇上下文。**长 RI 串的每簇仅两个标量，但新 cursor 的反向查询/边界验证可回扫整串；包 `src/grapheme.rs:517–535` 直接循环此前所有 RI。仅“过长簇”触发预算不足以约束按键成本。应明确 `GraphemeCursor` 的有界分块、`PreContext` 处理及累计扫描计费，预算耗尽类型化拒绝；补长 RI、Indic、长簇及非边界输入测试。
2. **P2，第 13、24 行：版本裁决缺具体差异与退出条件。**[Unicode 17 GB9c](https://www.unicode.org/reports/tr29/tr29-47.html#GB9c) 要求此前有 Consonant，[Unicode 18 GB9c](https://www.unicode.org/reports/tr29/tr29-49.html#GB9c) 已移除该前提。例如 `U+094D U+0915` 的 UTF-8 边界从 `[0,3,6]` 变为 `[0,6]`。应将此双版本金样及升级触发条件写实，避免用户批准不明确的“已知差异”。

用户须先裁决：是否接受仅 `aline-markdown` 新直接边、精确 `=1.13.3`/Unicode 17 基线，以及上述差异和 V-07 升级条件；否则保留 F-10 未关闭。此为新依赖，符合 `engine-design §7.3/18.2` 的方向，但仍须按 `development-process §2–3`、`CODE_STANDARDS §6.1` 更新 change；workspace 集中版本遵守 `v1-architecture §3.2`。身份分型、投影归属和删除另验边界合理。不建议合并相应依赖/helper 实施；提案可作为待决材料保留。
