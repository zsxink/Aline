# 组 9 独立复核记录

本记录由主 Agent 根据未参与实现或证据编制的独立 reviewer 的原始反馈整理；reviewer 只读，未写证据文件。复核基准为 `16f8e30` 后未提交的工作树。

## 发现与修复

- 9.3 原将 E3/E5 标签错位；现已按 proposal 将预算、取消与非法输入归 E2，展示树 dump 与源码范围归 E3，覆盖和映射归 E5。reviewer 已复核修正。
- 9.2 原声称 plugin 有实际 API 消费者；现已如实修正。进一步审查发现 ui→plugin、plugin→edit、plugin→app-core 三条无代码消费者的依赖边，已删除并保留 plugin workspace 成员；锁文件更新后重跑完整门禁并刷新证据。
- 复判时 `git diff --check` 报 `crates/aline-plugin/Cargo.toml:9: new blank line at EOF`，exit 2。随后主 Agent 移除本次删除依赖段时新引入的尾部空行；此事实不改写复判时的结果。

## 清理后复判

reviewer 确认三条边在 manifest、锁文件与最新 metadata 中消失，plugin 仍是八成员之一；正式原始汇总为 19/19 exit 0、312 passed，77 条依赖记录、53 个安全用例及 8 个 API 编译例均符合预期。复判时 74 项指纹匹配、9.2–9.5 的 112 个本地链接存在；文件尾空行随后修正，相关指纹需以最终刷新结果为准。E1–E7 的来源与范围一致，9.1–9.5 可闭合；9.6 仍须文档回填、结构校验与 CodeGraph 状态核对。

reviewer 检查唯一状态所有者、资源持有、关闭/模式失效及溢出原子性，未发现其他可操作问题。其于 2026-10-31T15:00:19Z–15:00:20Z 实际运行 14 个 `document_followups` 用例，报告 exit 0；原始 stdout/stderr 未独立落盘，此处是观察记录而非完整测试证据。

本审查不证明真实 UI/IME、现有文件保存、恢复、资源迁移、其他平台，亦不宣称 V 项或 M0 出口整项通过。
