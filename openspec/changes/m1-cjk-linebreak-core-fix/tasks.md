# Tasks

## 1. 独立断点契约与失败样例（ui/cjk-line-layout；R-4/R-17、S-5/S-12）

- [x] 1.1 固定 W2 短输入与 320/480/768 六组合的独立合法/禁止软折行金样，区分自动折行与原文开头/显式换行，加入中英/开闭括号/引号/ZWJ/组合字符/连字/仅标点/超宽 cluster；先写确实消费预期的失败测试，不复制当前断点作真值。验收确定性无依赖 Rust 测试在原规则下按目标反例失败、正确普通文本正例通过，证据规范化至 logs，并在 V-04/V-07 记录覆盖范围。
  - 证据：`logs/validation/m1-single-window-readonly/2026-11-09-cjk-01/cjk-linebreak/golden/`（12 文本、22 用例）；oracle 现状失败 6 个禁则用例 + 1 个拆簇用例、普通英文与显式换行正例通过，见 `commands/oracle-run.log`。
- [ ] 1.2 明确三个入口的无合法候选与工作上限契约，复跑极窄/长标点用例证明有限终止、原文可达和真实受限反馈。若现有接口无法表达，不实现静默溢出/裁切/硬断，给出最小受检接口 diff 和对应规范冲突后先取得具体裁决；验收正/拒绝场景及接口审阅记录齐全。
  - **阻塞**：有限终止与原文可达成立，但真实受限结果不成立；已产出 `contract.md`、`isolated-diff/interface-proposal.diff`、`isolated-diff/interface-impact.md`，等待接口裁决，未勾选。

## 2. 最小核心补丁候选（CODE_STANDARDS §3～4；R-9/R-10/R-17）

- [x] 2.1 编写私有合法断点纯规则，以无新依赖的同源测试先验证 UTF-8/cluster 边界、禁头尾集合、正常英语/空白及无候选拒绝；验收 `rustc --test` 或等价独立测试的正/最小违规案例全部通过，新增源码保持 forbid unsafe，不为该算法增加 trait/service/crate。
  - 证据：`rule-tests/cjk_soft_break.rs` + `rule-tests/rule_tests.rs`，13 个测试全部通过（`commands/rule-build.log`、`commands/rule-run.log`），源码 `forbid(unsafe_code)`。
- [ ] 2.2 从锁 checksum 一致的 gpui-pre 0.3.9 原始归档，在 logs 隔离副本形成最小 diff，将同一规则接入 shaped-line wrapping、LineWrapper 普通包装与 wrapped-truncation；保留 StyledText/TextLayout 几何、不改 buffer/projection 字符。验收原始归档/修改文件/diff 的哈希、三个真实消费点及无重复替代路径可复核；具体来源获准前不构建或激活修改后的 GPUI。
  - **部分完成、阻塞**：隔离副本、`minimal.diff`、`hashes.json`、`integration-notes.md` 已就绪（归档 checksum 一致，三入口接入，rustfmt 通过）；无合法候选分支仍是原硬断 + TODO，等待 1.2 接口裁决，未勾选。未构建/未激活。

## 3. 精确来源准入与核心集成（AGENTS 依赖规则；R-1/R-17、S-2/S-10）

- [ ] 3.1 形成核心本地来源的完整候选 metadata/tree、原归档与补丁绑定、许可证、全部入边/target/features/本机选包、五引擎隔离与最小性说明；独立评审后取得具体批准。验证未批准来源、checksum/diff/边或附加特性漂移均由现有守卫失败关闭，不套用 W1 隔离图许可；若需连带 Apple/macOS 来源，先重新审阅范围。
  - **材料完成、未获批准**：`admission/admission-request.md`、`policy-draft.json`、`candidate-summary.json`、`candidate-Cargo.lock`、`guard-rejection/`；现行门禁实际拒绝 `原生路径精确身份/来源/校验和漂移: gpui-pre@0.3.9`。未独立评审、未取得具体批准，未勾选。
- [ ] 3.2 获准后仅接入最小来源与精确策略资产，编译实际核心补丁并执行三个入口的真实布局/截断/cluster 回归；验收相应核心测试及原 Aline 映射/命中测试，清理试验替代路径，逐项说明与旧独立金样差异，不能凭纯 helper 测试宣称 TextLayout 集成通过。
- [ ] 3.3 更新既有依赖入口与最小补丁维护清单（原版本/来源、差异、复跑/回滚/升级重打、范围及未知项），执行实际准入守卫和最小违例自测；文档不写本机绝对路径或把本地来源例外扩成整包/全局许可。

## 4. 字体 run 事实取证（W2；V-04；R-9/R-11/R-13）

- [ ] 4.1 在受限 opt-in 验证入口记录 normal/bold/italic/bold-italic 的 requested Font、实际 shaped run FontId/可得 face 与真实 advance，并绑定样本/宽度/模式/字体代次/二进制；Source 明确 Menlo 普通标记显示。验证缺身份/缺真实 run 失败、opaque id 不能伪造字体名、默认 render 不新增写盘/全量工作，执行对应正反例与原字体/布局测试。
- [ ] 4.2 用真实 macOS arm64 中英、标点、emoji/组合字符样本解释 CJK 直立与真实 run 的关系，回填 V-04/UI 和 W2 的已证/未知边界。只有确有证据再提出合成斜体/平台钩子的独立最小方案；验收原始 run/advance/截图可核对，不借整元素 skew、换全文字体或未知墨迹宽宣称修复。

## 5. 原生回归与完整交付（V-04/V-07/V-09；R-11/R-17、S-14）

- [ ] 5.1 用锁定 Release 工件在 macOS arm64 重测 320/480/768 正文逻辑宽 × 两视图，记录每个真实断点/行高及受限场景；在新断点两侧做真实像素点击、两视图往返、换宽/字体失效与源码锚点恢复。验收图片/布局/run/映射身份一致，原 bytes/内存源码/revision/草稿/历史不变；原 M1 五个保真样本七步契约保持，不以本组替代缺失跳转。
- [ ] 5.2 执行 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、适用核心测试、统一工程/准入/安全守卫与正反例、OpenSpec strict，并安排补丁/布局/命中独立复审。验收实际命令与身份/失败记录齐全，重要 finding 清零，CodeGraph 同步且最新；依赖图解析不记为其它平台实测。
- [ ] 5.3 将真实结果、最小失败输入、准入/补丁维护与未测项回填 UI/架构/引擎依赖入口和 V-04/V-07/V-09，更新 W2/新 change tasks，保留 M0 豁免、M1 未过出口及 M2 前置；验收链接/路径/diff 检查和逐项证据核对，不提交、推送或归档。
