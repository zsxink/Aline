# 任务 2.4 独立审查（2026-10-31）

结论：**当前最小双语 catalog 实现通过独立审查，可由主 Agent 完成任务 2.4；无未解决的已确认行为或公共 API 问题。** 审查不覆盖完整 UI 文案、系统语言、设置持久化、后续 M 阶段或任何 V 项。

## 范围与边界

实际路径为 `aline-app-core/src/i18n.rs`，lib.rs 只导出 Locale、Translator、CatalogError。公开消费路径为 app→aline-ui::application_name→Translator::embedded→t("app.name")→GPUI SharedString；应用仅以 ExitCode 结束，不开窗口或平台服务。构造坏 catalog 的辅助方法保持私有，没有为测试导出动态加载或注册框架。

locale、两份 catalog 均由实例持有；切换语言只修改该实例，未见线程、锁、可变运行期全局或全局 locale。资源通过已准入 rust-embed debug-embed 编译内嵌，正例验证 Cow::Borrowed；没有运行期文件加载。

模板 find 得到的是 ASCII 花括号的 UTF-8 字节边界，局部切片和加一因此有明确证明。参数集合严格匹配模板需求，重复参数拒绝，重复占位符可复用同一值，非 ASCII 参数按原样输出。键集合相等先验成立后再按对应键查另一语言；这些是 catalog 内部字符串运算，不是文档坐标调用点手算。

## 修正及验证

- 原 UTF-8/TOML 错误均被转成 InvalidToml(String)，丢失来源和分支。现分别保存 Utf8Error、toml::de::Error，并通过 Error::source 返回原错误。
- schema 错误已改为 EmptyCatalog、InvalidKey、InvalidValueType 等结构化枚举原因；Display 为技术诊断，UI 未直接显示引擎错误文案。
- 语言隔离测试改为 Result 返回及 ?，实例构造失败不能绕过全部断言；测试交叉切换两侧，另一实例仍维持原语言结果。重复占位符及中文参数值已有明确预期。

独立实际命令：`cargo test -p aline-app-core --locked --offline`，退出码 **0**，**5 passed**。完整参数、cwd、环境覆盖、时间、退出码、输出 hash 与日志见 `catalog-review-test.command.json` 及 stdout/stderr。测试逐项包含实际嵌入、键/占位符集合不一致、未知键、缺/多/重复参数、坏 TOML、无效 UTF-8、空 catalog、非法键/值及错误来源链。

实现者的 `catalog-evidence/` 另记录 fmt、三消费包 Clippy、debug/release app-core、UI 消费与 app 运行结果；它们不替代本次独立源码审查，也不外推跨平台支持。所审文件 SHA-256 见 `catalog-review-files.json`。

## 限制

本机仅 macOS aarch64。Windows、Linux、Intel Mac、完整界面语言传播、系统 locale、真实窗口、真实 IME、保存与性能均未验证。当前真实 catalog 仅 app.name，两个语言的品牌名都为 Aline；其余插值及隔离预期来自明确标记的单元夹具，未添加无人消费的产品文案。M0 完整出口仍待后续各任务与任务 9.6 独立审查。
