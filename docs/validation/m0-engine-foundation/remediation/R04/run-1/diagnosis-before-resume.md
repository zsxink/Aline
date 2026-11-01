# R04 / F-02 诊断与暂停记录

状态：NEEDS_CONTEXT（协议裁决前暂停实现）；不是 R04 完成，也不是 F-02 已关闭。源码及 manifest 零修改，未 commit。

## 手写验收期望与实际

| 用例 | 期望 | 本轮实际 |
| --- | --- | --- |
| 原样 front matter Visual 普通输入 `Y` 并提交 | 拒绝，Buffer/草稿/组合/selection/version/undo/redo/History全状态不变 | **失败复现**：写成 `t: Yx`，revision1，历史1步 |
| 同位置 IME 确认 | 同权限检查且拒绝保留组合与完整状态 | **失败复现**：同样写成 `t: Yx` |
| Visual 普通 Markdown输入 `**hi**` | 合法产生源码 `**hi**abc` | 宿主现有成功；相同 edit Visual Replace 拒绝 HiddenSourceChanged，证明不能直接套接 |
| `<b>x</b>` 原文偏移3插入Y | 受检原文编辑保留合法路径，格式/跨边界命令另行拒绝 | 宿主现有成功；Visual Replace 拒绝 HtmlBoundary |
| 普通正文 `abc` 偏移1插入Y/LF | 正常提交 | 两种既有 Replace 计划均成功；只用于界定缺口 |
| Source保护修改、命令pending/composition/redo预检、完整History零副作用、实际提交截redo、取消/无变化保留redo | 后续受检接线后逐项验证 | **未验证新行为**，实现按契约缺口暂停 |

`probe.rs` 是保留的完整可执行源；同一源在临时消费者 crate 使用四个仓库 path依赖。`probe-baseline-real-cache.*` 记录真实 DocCore及edit对照输出。`baseline-acceptance-tests.*` 中3例：两个保护拒绝验收测试按预期红灯，1个既有API缺口诊断测试绿灯。失败断言后未执行的状态比较不能冒充已证实零副作用。

## 根因和裁决边界

CodeGraph定位：DocCore `composition_commit`、`start_group` 的旧组提交、`commit_typing_group` 均直接构造 CheckedBatch→Buffer.apply。宿主没有消费 edit计划/校验。完整设计与替代方案见 design.md。

现行规范允许 UI直接依赖edit，plugin当前没有可用中转API；这一点主Agent已确认。真正阻塞是现有 Replace契约无法表达普通Markdown输入及HTML原文编辑。按本任务“既有API不能接入、需新增公共协议时停报”要求，不暗改edit、不把输入降级为Source。

redo不构成独立冲突：仅pending/组合中的不适用命令必须零副作用拒绝；合法实际新编辑截redo，无变化/取消保留。T-8-17不在本轮裁决范围。

## 环境及取证边界

工具链固定1.98.1、offline，无安装/下载。首次 admission已登记隔离CARGO_HOME缺pulldown-cmark而exit101，原日志 `probe-baseline.*` 保留；使用本机已有 `<cargo-home>` 缓存再次离线实跑成功，记录环境覆盖。临时消费者首次 `cargo run --offline` 仅生成其自身Cargo.lock（非仓库lock更新），后续测试使用 `--locked --offline`；仓库Cargo.lock未修改。

基线库test/clippy/fmt/check与CodeGraph记录见同目录命令日志；它们验证既有代码当前状态，不能证明未实现的R04通过。未执行全workspace门禁（留R14），未测试真实GPUI输入/三平台/性能调度。

## 基线检查结果

- `cargo test -p aline-ui -p aline-edit --locked --offline`：exit0，140 passed（不含隔离红灯probe）。
- `cargo clippy -p aline-ui -p aline-edit --all-targets --locked --offline -- -D warnings`：exit0。
- `cargo fmt -p aline-ui -p aline-edit --check`：exit0。
- `cargo check -p aline-ui -p aline-edit --all-targets --locked --offline`：见baseline-check.command.json。
- `git diff -- crates/aline-ui/Cargo.toml crates/aline-ui/src/document crates/aline-edit Cargo.lock`：空输出，确认本轮源文件未改。
- `codegraph sync`：exit0；before/after status原始输出分别留存。

## 局部锚点补充验收

主Agent要求区分“整块补丁失败”和“既有API表达能力”。`local-anchor-plan-validate.*` 已实际运行：`**hi**` 的绘制末端2经source_at映射src4，最小`(4..4, "abc")`成功plan→validate；front matter/HTML对照正确拒绝。`abc`的绘制0映射src0，最小`(0..0, "**hi**")`仍因HiddenSourceChanged拒绝。原反例也是局部空区间插入，不是整块Replace。结论收窄为：已有标记中的可见输入能表达；新增Markdown标记的合法源码输入需要不同于Visible Replace的语义。此处不需要diff/rebase，也没有以定位歧义夸大接口缺口。
