# R04 / F-02 实施交接报告

状态：实现完成，待主Agent最终验证与独立reviewer；未commit，工作所有权已交回。未修改R09/R11/任务表/进度/M1事项。

## 精确变更文件

- Cargo.lock：仅aline-ui依赖数组新增aline-edit。
- crates/aline-ui/Cargo.toml：声明已有workspace aline-edit依赖。
- crates/aline-ui/src/document/doc_core.rs：新增DocError::Edit；普通/IME提交改走publish_input；901ms/非相邻旧组提交复用统一组提交；声明新模块及测试。
- crates/aline-ui/src/document/doc_core/checked_commit.rs：普通/IME同一plan→validate消费者；公开apply_change命令入口，即时复验借用的身份/Parsed/计划。
- crates/aline-ui/src/document/doc_core/checked_commit_tests.rs：16条真实宿主回归。

设计、唯一状态所有者、拒绝顺序、预算和代价见design.md。diff原文见handoff-tracked-diff、handoff-new-consumer-diff、handoff-new-tests-diff.stdout.log；两个no-index diff的exit1表示存在新增文件差异，非校验失败。

## 验收对照

| 场景 | 手写期望 | 实际 |
| --- | --- | --- |
| F-02 front matter偏移7普通Y提交 | 拒绝，保留草稿/选区/Buffer/版本/历史 | 先红后绿 |
| 同点IME start/update/commit，活动保存等待和快照 | 拒绝保留组合、等待与完整状态 | 通过 |
| 从Parsed保护/样式范围取得HTML点，普通/IME | 两条路径同类HtmlBoundary，零副作用 | 通过 |
| Source下保护源码、新Markdown标记、HTML原文 | 成功一步历史，模式往返不加步，undo恢复字节 | 通过 |
| 已格式化正文的可见普通/IME输入 | 经source_at的最窄空区间，隐藏标记不改，一次输入组一步 | 通过 |
| Visual新增隐藏标记 | 沿既有Replace拒绝，Source允许 | 按主Agent确认范围通过 |
| 保护草稿的901ms、组合开启、离块、切模式提交 | 所有间接提交不绕过权限，拒绝完整状态不变 | 通过 |
| 实际pending或组合+redo并存时apply_change | precheck拒绝，不先提交/封口、组合不被结束、redo保留 | 通过 |
| 命令文档/工作版本/配置/请求/投影/选区/模式失配、历史预算失败 | 零副作用拒绝 | 通过 |
| Source多补丁、格式两个标记补丁 | 不套单输入patch1限制，一次提交一步历史 | 通过 |
| 合法普通/IME/命令实际新发布 | 清旧redo，无空历史步 | 通过 |
| 无变化IME/组合取消 | 保留旧redo和全部历史步骤，不推进revision | 通过 |

两个真实宿主保护验收的修复前日志为real-host-red-tests-corrected.*（2 failed，实际写入成功）；此前real-host-red-tests.*为测试探针首次无法Debug StringBuffer的编译错误，完整保留。隔离probe.rs的原样输出见pre-fix.txt，普通和IME确实都曾写出t:Yx。早期协议调查及局部source_at对照保留，最终范围由主Agent明确后恢复实施，不将早期NEEDS_CONTEXT当最终状态。

## 命令与退出码

全部原始argv/cwd/env/UTC/exit在*.command.json与commands.txt；stdout/stderr不删失败输出。

| 记录标签 | 命令/范围 | exit与结果 |
| --- | --- | --- |
| initial-lock-update | cargo check -p aline-ui --all-targets --offline | 0；唯一未带--locked的正式仓库命令，仅登记内部边lock更新 |
| checked-host-tests | cargo test -p aline-ui --lib checked_commit_tests --locked --offline | 0，14 passed（追加2例前） |
| final-crate-tests | cargo test -p aline-ui -p aline-edit --locked --offline | 0，154 passed（最后追加2例前） |
| final-host-tests | cargo test -p aline-ui --lib checked_commit_tests --locked --offline | 0，最终16 passed |
| final-crate-clippy | cargo clippy -p aline-ui -p aline-edit --all-targets --locked --offline -- -D warnings | 0（最后追加2例与私有可见性收紧前） |
| baseline-fmt | cargo fmt -p aline-ui -p aline-edit --check | 0（基线） |
| final-fmt-apply | cargo fmt -p aline-ui | 0，最终源码已格式化 |
| initial-lock-update | cargo check -p aline-ui --all-targets --offline | 0，消费者接线后构建通过 |
| baseline-check | cargo check -p aline-ui -p aline-edit --all-targets --locked --offline | 0（基线） |
| handoff-codegraph-sync | codegraph sync | 0，Already up to date |
| handoff-final-clippy | cargo clippy -p aline-ui --all-targets --locked --offline -- -D warnings | 0，最后16例之后 |
| handoff-final-fmt | cargo fmt -p aline-ui --check | 0，最后16例之后 |
| handoff-final-check | cargo check -p aline-ui --all-targets --locked --offline | 0，最后16例之后 |

主Agent要求立即交回后已停止源码变更，并依补充要求只跑最终只读验证：16例专项于08:41:58–08:41:59 UTC通过；随后最终源码 clippy于08:44:18–08:44:19 UTC exit0、fmt --check于08:44:18 UTC exit0、check于08:44:19 UTC exit0。对应handoff-final-clippy/fmt/check.*。最后16例之后未重复完整ui+edit测试；最近完整154例与最新16例分开记录。未执行全workspace门禁，留R14。

## 明确边界与独立审查重点

1. 未测试真实GPUI/平台IME、三平台、性能和UI调度；组/IME发布点同步解析不构成V-05通过。相邻每键路径未新增parse；旧组边界仍需发布时解析。
2. public apply_change接收InputIdentity+Parsed+ChangePlan并即时复验，不持久保存Parsed；M0只接受ParseConfig::m0。未来有效配置所有者/异步采纳沿现有身份协议演进。
3. 完整可观察状态比较涵盖DocCore所有字段、Buffer文本/快照ID/选区/行索引及公开全部HistoryStep。StringBuffer无Debug，私有History的pending/last_input_at未增加测试接口；本组DocCore每节点一实际步骤、发布立即seal，代码拒绝路径均在apply/seal之前，需reviewer核对内部封口状态。
4. 组合取消/无变化与实际发布清redo已实测；首次未提交输入清redo时点T-8-17未提升为产品裁决，仅保留现有实现。
5. 原有apply后版本推进、baseline冲突/重试/模式切换时序未扩大本轮协议；相关现有回归通过，交主Agent继续复核。
6. Visual新增隐藏标记拒绝与HTML边界拒绝遵循既有edit契约和主Agent本轮明确范围，Source原文编辑通过；未改edit公共协议和HTML权限规则。

环境：Rust1.98.1，offline，实际缓存`<cargo-home>`。首次admission隔离缓存缺pulldown-cmark的失败日志保留；未下载或安装。新消费边仅path内部依赖。
