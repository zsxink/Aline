# R02b 恢复验收独立复核

裁决：**APPROVE_WITH_CONCERNS**。建议合并本批；R02 review §E 的 F-01（空来源合成片获非空 source）可关闭。没有发现本批实现的阻断问题。唯一建议为低优先级永久测试覆盖补齐，不扩大为 M0、R14 或三平台验收通过。

复核者未参与 `9602486` 实现及恢复测试编写。范围为该提交的 `source_point` 修复、当前 `tests/rewrite.rs` 新增测试，以及 recovery 三份证据。未修改生产代码、测试、旧证据或任务状态。适用依据：tasks.md §4 R02、R02 review §E、引擎 §6.3/7.1 与 E-T04/E-T08、CODE_STANDARDS §2/4/5/6 及 engineering-governance 的独立可信证据要求。

## 实现与独立预期

`src/pre/rewrite.rs:207–214` 的第二层空来源路径，经 `middle_anchor:306` 唯一调用 `source_point:633`。非 Exact 返回端点零宽区间；Exact 的逐字节平移未改，非空来源复合未改。现有具体函数足够，无新增状态、依赖、公开 API、权限或副作用。文件仍围绕重写构造与映射一个职责，本次小改动无需机械拆分；未来扩展预算/取消仍归相同映射边界独立处理。

我按输入来源及既有端点规则重新计算，未用 compose 输出生成预期：

- 原 `aXYdef → axydef` 的 Aligned `1..3`：副本起点 1 回原点 1，内部点 2 只能取末点 3；故两处新插入分别为 `1..1`、`3..3 Synthetic`。左右可见片按相交拆分，永久测试的全部元组正确。
- `ab → a` 的 Hidden `1..2 → 1..1`：末尾 fallback 落在 Hidden 的起点，取原点 1。新插入为 `1..1 Synthetic`，没有伪造 `1..2` 来源。
- 新增 `ab → axyb`：`xy` 全部来源为原点 1。再在副本点 1/2/3 插入，结果都取 `1..1`；点 3 由后继 Exact 接管。三份完整元组的来源、重写长度及 precision 均与手算一致。

原报告 §3 明说早期期望参考探针输出，不能据此追认为独立预期。上述为本轮非作者重新计算的新证据；新增测试使用字面量而非运行时自生成，计算结果正确。旧报告“需三层叠加才覆盖 Synthetic”的说法不准确：本轮两次 apply 加一次 compose 已触发该路径；恢复交接已按实际覆盖描述。

## 可操作建议

**C-01〔低，测试覆盖〕**，位置 `crates/aline-markdown/tests/rewrite.rs:540`（尤其 `:577` 的点 3 案例）。该案例的 Synthetic 终点由后继 Exact 接管，不能验证第一层末尾 Synthetic 经 `middle_anchor` fallback 的路径；现有 Aligned 回归也没有第一层 Aligned 位于文件末尾的端点案例。触发条件分别为原空串插入 `xy` 后于副本末尾 2 再插入，以及 `aXYZ → axy` 后于末尾 3 插入。影响是这些分支的未来回归不能由本批永久测试发现，**当前实现结果正确**。建议把 `review-probe.rs` 的 `synthetic-eof`、`aligned-eof` 案例加入永久测试，预期分别 `0..0`、`4..4 Synthetic`。本轮探针已独立补验，故不阻断合并。

## 实跑证据与限制

- `review-checks.jsonl`：`cargo test -p aline-markdown --locked --offline` **127 passed / 0 failed**（rewrite 15）；Clippy 同包 `--all-targets --locked --offline -- -D warnings`、fmt 同包 `--check` 全 exit 0。
- `review-probe.rs` + 同日志：外部 consumer 手写 **17** 个锚点，覆盖 Aligned 起点/内部/EOF、删除后带不同源/副本起点的 Exact、全隐藏、Synthetic 起点/内部/EOF、UTF-8 全部合法端点。全部通过，所有输出端点均验证原文字符边界。首次 rustc 因依赖搜索路径误指 debug 根而 exit 1，改为 debug/deps 后通过；失败与纠正均原样保留。
- recovery 日志可正常解析：补测前 126、补测后 127；最终 before/after 指纹一致，与当前源/测试一致。生产 SHA-256 `aa730fb81caa167826de781cc896676e8bd489c225427a76e109a3dd57c2f522`，测试 `5fa6bd02d1998c2dd74b4f8842ba2c10b1adb1437c321a167b6eba0559341a59`，Cargo.lock `998b8b9178c129cb3d7b915054b9d007dee72eb4f4dd7d99671876fdd3dabe69`。我的全部运行前后这三份输入也一致。
- `9602486` 中七份原 run-1 证据逐字节未变；当前生产源码亦与该提交相同。原修复前失败记录保持其原作者/时间身份，没有追造历史。
- 未复跑 workspace 全门禁（由 R14 统一）；未验证真实 UI、Windows/Linux。R02 已登记的 compose 预算/取消与空 map 边界不在本批范围，本结论不关闭它们。
