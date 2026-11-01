# R00 恢复基线与运行记录

运行编号：R00/run-1（2026-11-01，北京时间约 10:09）

## 已做事项

1. 记录实际基线：`git rev-parse HEAD` = `2f1dcea46d5998f4e2ba213e73c11de166f943c4`，与任务表登记的原复核基线一致（见 `baseline-head.txt`）。
2. 记录起止工作树状态：`git status --porcelain` 仅有本次新增的两处未跟踪目录
   （`docs/maintenance/m0-remediation/`、`docs/validation/m0-engine-foundation/remediation/`），
   没有对受跟踪文件的改动（见 `baseline-status.txt`）。没有 reset、没有覆盖用户改动。
3. 记录工具链与环境：`rustc 1.98.1`、`cargo 1.98.1`、host `aarch64-apple-darwin`（见 `toolchain.txt`、`env.txt`）。
4. 建立新证据根目录 `docs/validation/m0-engine-foundation/remediation/`，按批次/运行编号分目录，不覆盖既有 `group-1..9` 历史证据。
5. 核对既有整改：除本任务表外，工作树中没有其它进行中的整改产物；原复核临时目录 `<tmp>/aline-m0-independent-review.pTFf8u/` 仍存在，
   其 `C/independent_tests.rs`、`C/repro.rs`、`C/rewrite_repro.rs`、`root/reuse_probe.rs` 已在本轮原样重读并记录（见任务表 §4）。
6. 明确不把原 tasks.md 的 46 条已勾选当通过：原任务勾选保持不动，最终在 R14 重新裁决。

## 未做/限制

- 本轮未运行任何 Cargo 门禁；R00 只做恢复与登记，实际验证由各批次与 R14 提供。
- `/tmp` 原报告仍可用但属本机临时路径，其内容已摘要进本任务表 §4，缺失时可重新复现。

## 下一步

按依赖推进 R01（aline-ui 文档 Redo/选区）与 R02（aline-markdown 多阶段改写复合），两者文件不重叠可并行。
</content>
