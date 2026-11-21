# Tasks

## 1. 确定性驱动与 buffer（E-T03/06/20，R-17-03，S-12-06）

- [x] 1.1 建立仅测试的确定性 RNG、受检环境配置、完整操作轨迹与失败输入最小化；新增 buffer 随机补丁/历史/reload/保存点/拒绝测试，独立字符串 oracle 核对每步 bytes/选区/revision/历史；`cargo test -p aline-buffer --test fuzz_invariants` 通过并文档化复跑入口。

## 2. markdown 与 edit（E-T04/09/10/14/20）

- [x] 2.1 增加 Markdown 语法种子+随机 UTF-8 局部修改，对块+Gap、映射边界/精确往返、保护和全量解析/reuse 重定位做逐步验证；`cargo test -p aline-markdown --test fuzz_invariants` 通过，失败可最小化重跑。
- [x] 2.2 增加 edit 随机选区/命令，验证计划零副作用、过期身份/选区拒绝和 Visual 保护拒绝；`cargo test -p aline-edit --test fuzz_invariants` 通过并明确 DocCore 只读/IME 未覆盖边界。

## 3. source 与证据（E-T01/16/20，V-07/08）

- [x] 3.1 增加 source 随机 bytes、未编辑逐字节保真及编辑重开幂等；对现有 create-only 原语测试预算、取消、已存在目标失败保全；`cargo test -p aline-source --test fuzz_invariants` 通过。M2 受检替换与真实崩溃恢复不在已有原语中，不能写成已覆盖。
- [x] 3.2 故意注入一次观测字节损失，确认同 seed 非零且输出 seed/step/trace/最小输入；移除注入后通过，16×100 默认与扩量运行原始输出留 `<evidence>/w3/`。复核默认耗时，运行 fmt/适用 Clippy 与工程守卫；主 Agent 回填现行验证计划，保留平台/DocCore/M2 扩展缺口，不归档。

本轮核对（macOS arm64）：前四项实现及默认 16×100、扩量 64×500 已通过；总计 13 个测试，包含驱动最小化和 edit 拒绝/no-op oracle 的反例。独立审查发现的 edit 意外拒绝被静默接受问题已修复，独立 reviewer 已复跑 64×500 及 Hidden 边界 seed 并清除 finding。`design.md` 记录可复现命令与 Hidden/Synthetic 边界；不将无可见位置强作块局部绘制坐标。

证据根为 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/`：`w3/final-expanded.*`、`w3/post-review-default.*`、`w3/post-review-clippy.*`、`w3/final-fmt.*`；四 crate 的真实观测字节损失注入各自非零，两次 seed/step/操作/输入/最小片段完全相同见 `w3/final-negative-replay-check.json`，去除注入同 seed 恢复见 `w3/post-review-recovery.*`。安全守卫及其反例自测通过见 `w3/safety-stable.*`、`w3/safety-self-test.*`。早期隔离缓存缺包和新增错误 Hidden 假设导致的失败也原样保留，后续证据明确修正，不覆盖历史输出。

3.2 暂留未勾：上述注入、复跑、扩量、局部 fmt/Clippy 和安全守卫已完成，等待主 Agent 完成本轮全 workspace 门禁、现行验证计划回填及最终核对。仅 macOS arm64；DocCore/真实 IME/M2 受检替换与崩溃恢复均未由本 change 覆盖。

2026-11-09 主 Agent：workspace-final-02 的 29 项完整检查全部通过，已将 W3 具体不变量、默认/扩量/注入及独立审查结果回填 V-07；3.2 对应交付闭环完成。原有 M0 豁免、M1/真实 IME/M2 保存恢复缺口保持。
