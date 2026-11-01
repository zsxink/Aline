# R07 OpenSpec 实施前独立审查

结论：**APPROVE**。未发现有证据且需修改的问题。建议合并本次 change 更新，按已批准范围推进任务 6.9；该结论仅覆盖设计/spec/tasks，不代表尚未编写的实现通过审查或 F-10 关闭。

审查对象：`openspec/changes/m0-engine-foundation/design.md:44,80–83`、`specs/engine/markdown/spec.md:42–50`、`tasks.md:67`；对照用户已批准的 [R07b 提案](../run-2/dependency-admission-proposal.md)及[提案复审](../run-2/proposal-review-addendum.md)。

核对结果：

- **依赖与架构：**精确 `unicode-segmentation =1.13.3`、归档 SHA-256、Unicode17 数据和唯一 `aline-markdown` 直接引擎边均明确；mapping 是唯一实现归属，禁止 GPUI/第三方公共类型泄漏、buffer 重复实现及源码手算偏移，未准入其它候选。遵守 `engine-design §7.3/18.2`、`v1-architecture §3.2` 和开发流程 §2–3。现有 Cargo 清单没有新增分段边；同期 lockfile 唯一变化为 `aline-ui → aline-edit`，不作为本次 R07 准入内容。
- **预算与失败：**design 第 82 行明确每查询累计供给 ≤4096 UTF-8 字节、每块 ≤256 字节，主动块、`PreContext`、重复供给均计费；预算未确定边界时类型化拒绝，不返回猜测偏移。已区别供给计费与第三方内部访存/CPU 保证。spec 对应场景和任务 6.9 一致；实现应在调用前检查下一次供给，确保不实际越过上限。
- **版本与 API：**U+094D U+0915 的 Unicode17 `[0,3,6]`/Unicode18 `[0,6]` 已落入设计和场景；V-07 若要求新版规则须重新准入。完整 Source identity/Visual 投影、强类型身份、非法 scalar/旧投影/非字素移动起点拒绝和删除保护另验均被保留。
- **验证闭环：**6.9 要求先建独立期望，覆盖短/长 RI、长组合簇、Indic 差异、双向移动、非字素起点/旧投影；登记依赖守卫唯一新边并回填引擎、架构及 V-07。tasks 开头仍要求每组 Rust 变更完整 fmt/clippy/test 和原始证据，未把 UI/IME/平台验收当作完成。

实际检查：`openspec validate m0-engine-foundation --strict`，exit 0，输出 `Change 'm0-engine-foundation' is valid`。本次未运行 Rust 测试，因为被审增量只有规划工件；既有库 API 可消费性和有限供给探针见 run-2 复审证据。未修改提案、现行规范、Cargo 或源码。
