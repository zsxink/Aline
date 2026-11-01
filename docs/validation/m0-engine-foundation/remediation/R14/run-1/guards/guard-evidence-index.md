# R14 守卫证据索引

R14 不改守卫实现；守卫实现缺陷须另行修复。本索引将最终工作树的正常运行与既有受控违规证据并列，不能用 self-test 代替正常范围检查。

## 最终工作树正常范围

| 守卫 | 命令/证据 | 结论 |
|---|---|---|
| engineering/dependency | `gate/unified-captured/dependency.command.json`、`dependency.stderr.log` | **未核实**：指定离线隔离 Cargo home 缺 `rust-embed`，metadata JSON 无法生成；失败不是规则正确拒绝的证据。隔离 self-test exit 0。 |
| lint | `gate/unified-captured/lint.command.json`、`lint.stdout.log` | 通过，exit 0。隔离 self-test exit 0。 |
| safety | `gate/unified-captured/safety.command.json`、`safety.stdout.log` | 通过，exit 0；归档 89 文件扫描结论及变异仍见 R06。隔离 self-test exit 0。 |
| public API | `gate/unified-captured/public-api.command.json`、`public-api.stderr.log`、`public-api/summary.json` | **未核实**：隔离 lock 生成失败（同一离线索引缺 `rust-embed`），8 个用例均不算通过。独立 self-test exit 1。 |

## 违规拒绝与恢复

- lint 正常范围/真实 Cargo 诊断/违规隔离用例及删除违规后恢复：`../../../R05c/run-2/report.md`、`../../../R05c/run-2/review-evidence/normal-guard.stderr.log`、`../../../R05c/run-2/review-evidence/mutation-summary.json`、`../../../R05c/run-2/review-evidence/independent-guard-real-rust.stdout.log`、`../../../R05c/run-2/review-evidence/independent-guard-real-clippy.stdout.log`。共 9 个定向变异；具体预期路径/规则见 mutation JSON 与 stderr。复核见 `../../../R05c/run-2/review.md`。
- safety 正常范围、65/65 self-test、3 种规则变异明确捕获、删除变异恢复及空目标/失效目标探针：`../../../R06/run-2/review-evidence/normal.stdout.json`、`../../../R06/run-2/review-evidence/self-test-summary.json`、`../../../R06/run-2/review-evidence/mutation-A-allow-external-symlinks.stdout.json`、`../../../R06/run-2/review-evidence/mutation-B-ignore-missing-target.stdout.json`、`../../../R06/run-2/review-evidence/mutation-C-duplicate-dangling.stdout.json`、`../../../R06/run-2/review-evidence/real-cargo-no-target.stderr.log`。复核见 `../../../R06/run-2/review.md`。D4 `targets=[]` 显式拒绝仍为限制，不写成已关闭。
- R14 新增的实际自测原始输出：`../gate/04-engineering-self-test.*`、`../gate/05-lints-self-test.*`、`../gate/06-safety-self-test.*`；public API 失败为 `../gate/07-public-api-self-test.*`。

## 判定

历史 R05/R06 证据证明相应规则变异被拒绝并有恢复证据；R14 新环境下 lint/safety 正常通过，engineering 与 public API 因隔离离线缓存不完整而未核实。故不能宣称四类守卫本轮全部正常检查通过，M0 仍未闭合。
