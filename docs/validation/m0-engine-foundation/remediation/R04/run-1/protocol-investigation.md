# R04 最小接线设计与既有协议缺口（待裁决）

## 结论与所有权

本轮仅新增诊断证据，未改应用、引擎或 Cargo 文件。`DocCore` 继续是唯一 Buffer（含 History）、OpenBlock 普通草稿/组合、选区和工作版本所有者；edit 只读取快照并返回受检计划。F-02 仍存在，不能记作修复。

`architecture-guide.md` §1.1 和 `CODE_STANDARDS.md` §2.1 都明确“主链可向下直接消费公开 API”；已有 workspace `aline-edit` path 定义，plugin crate 目前是无依赖、无导出的占位。故 UI 声明 `aline-edit.workspace = true` 符合现有依赖方向，不需要 plugin 中转。主 Agent 已确认此范围，但由于下述契约缺口本轮没有添加依赖。

## 原计划与拒绝顺序

普通组提交、非相邻/901ms 隐式组提交、IME 确认应统一在发布点从当前 Buffer 基线建立 Replace，按同一 ParseConfig 解析基线，再 `plan_change` → `validate_change`，成功才一次 `buffer.apply`。补丁是基线的唯一单区间，expected 从基线受检切片取得；草稿 after 选区不能冒充 before 选区。提交前的所有身份推进、预算、草稿基线和权限检查必须完成；失败不清缓存、不封口、不改草稿/组合/History。普通输入发布不属于“其它命令在活动输入中执行”，不能直接把 pending/composition 标成 true 交给现有命令预检而让每次正常提交必拒绝。

显式命令入口应先基于真实 DocCore 状态拒绝不适用命令：基线/打开状态 → pending → composition → 模式/身份/工作版本/快照/配置/投影/选区/片段 → 权限 → 候选重读/after 选区 → 原子发布。不允许为了让命令计划可执行先提交当前草稿。redo 本身不是拒绝条件；合法实际新编辑才截分支，无变化和组合取消不清分支。首次未提交输入截 redo 的 T-8-17 时点仍未定。

已有 `ChangePlan` 只绑定低层 revision/snapshot/projection/selection，没有 DocumentId、WorkingVersion 草稿代次和 UI 配置代次，因此 `apply_change` 所需 UI 信封应由 UI 层绑定并复验，不能反向引入 UI 类型。

## 既有 API 无法无回归覆盖输入

`EditCommand` 只有 Replace 与 ToggleInlineFormat。Visual Replace 的 `RereadContract::Visible` 要求“候选绘制串 = before 绘制串按字面 replacement 拼接”，且非纯空白隐藏源码序列完全相等。

实际 `abc` 在偏移 0 输入 `**hi**`：现有 DocCore 成功提交 `**hi**abc`；相同 Replace 在写前返回 `RereadFailed::HiddenSourceChanged`。这不是保护拒绝，而是用户输入合法 Markdown 创建了强调标记。ToggleInlineFormat 也不能表达任意新输入、换行和组合候选。

实际 `<b>x</b>` 偏移 3 输入 `Y`：DocCore 成功提交 `<b>Yx</b>`；Visual Replace 返回 `HtmlBoundary { range: 0..3 }`。现行引擎 §8.2/§11 与架构 R-5-07 明确受检 HTML 原文编辑允许；格式/块命令不能触及边界，两者语义不同。换行例 `abc` 偏移1插入 LF 可通过，不能声称所有换行都拒绝或所有输入都适用。

保留现有命令契约时，普通/IME 输入发布需要能表达“源码草稿变化、保留未编辑字节并检查保护/HTML身份”的独立受检语义，不能偷换为 Source，也不能为通过测试放宽 Visible 替换契约。此点需要先按开发流程裁决输入计划的最小公共协议、HTML 原文允许边界与重读不变量；本轮禁止新增该 API，故停在证据上。

## 最简方案、代价与替代方案

可行的下一步是先明确输入提交契约，再只给 edit 增加必要输入意图或计划入口，由 UI 发布普通/IME草稿和显式命令共用验证/原子应用链。代价是新增并验证一个受检语义及对应公开 API，不能作为纯 UI 内部接线隐藏。

不采用：所有 Visual 输入套现有 Replace（会拒绝合法 Markdown）；Visual 输入用 Source计划（绕过保护/HTML权限）；直接复制权限逻辑到 UI后继续裸 apply（无法证明消费同一 edit 验证链且产生双份规则）；新建 plugin透传层（当前没有业务中转需求）。

若后续批准，则解析仅在组提交/IME确认/显式命令点发生；不在 append/candidate update 逐键解析。现有 `DocLimits` 文本预算派生单输入 `PatchBudget(1, text.max_bytes, text.max_bytes)`；parse 使用现有 `ParseBudget::m0()`，显式命令独立使用其计划预算，不能塞回单补丁上限。同步解析预算和真实 UI 调度仍须 V-05；无头正确性不证明响应性能。无需持 Parsed 字段时可提交点解析；若持有采纳缓存须绑定完整输入/配置/请求身份，在草稿变化、提交、undo/redo/reload、模式/配置变化、开关块时作废。

## 追加：主 Agent 要求的局部映射对照

`local-anchor-probe.rs` 明确使用 `drawn_offset` → `source_at` → `SrcOffset` 建立空区间插入，再对Changed计划实际调用validate_change。

- before `**hi**`，绘制末端2经两种affinity都得到src4；`SourceEdit(4..4, "", "abc")` 的plan与validate都成功，候选绘制串`hiabc`。所以“已有Markdown中的普通可见文字编辑”**可以**由既有API表达，不需要diff/rebase猜测，也不应因整块替换失败而判接口不足。
- before `abc`，绘制0经两种affinity都得到src0；`SourceEdit(0..0, "", "**hi**")` 仍拒绝HiddenSourceChanged。原反例本来就使用此最小空区间，未替换整块。缺口仅指输入创建/改变Markdown标记的语义，不是宿主缺少局部range。
- 同一映射链的front matter绘制0→src0拒绝ProtectedRange；HTML绘制0→src3拒绝HtmlBoundary。

本轮已有DocCore普通TypingGroup本身保留明确range与draft，无需为纯插入组通过字符串diff推测范围；不能用“定位有歧义”解释上述新增标记拒绝。后续真实块级替换若只给整块字符串，则仍应保留平台映射得到的明确源意图，而不靠重复文本相等猜定位，但它不是本次已实测的阻断原因。
