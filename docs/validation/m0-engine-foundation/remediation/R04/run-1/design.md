# R04 文档所有者受检发布设计

状态：已实施，待主 Agent 复核与独立 reviewer；未commit。早期协议调查原样保留在protocol-investigation.md；主Agent明确本轮范围后继续，不以调查阶段暂停结论代表最终状态。

## 所有权与依赖

唯一状态所有者仍是DocCore：StringBuffer含唯一History，OpenBlock含普通输入或组合候选，工作版本/选区/等待/资源/请求身份均不复制。新checked_commit.rs只编排受检发布；没有第二可写全文、第二历史、线程或持久派生缓存。

architecture-guide §1.1、CODE_STANDARDS §2.1及本change design均明文允许主链向下直接消费公开API，plugin当前没有可用导出。UI仅声明已有workspace aline-edit path依赖，Cargo.lock仅增加aline-ui→aline-edit内部边，没有新增第三方包/版本。

## 两种发布上下文

普通输入/IME是唯一输入槽自身的发布：使用TypingGroup.range/CompositionState.range和before-selection在稳定Buffer基线上构造单条SourceEdit，expected为该基线受检切片，replacement为唯一草稿。不是把整块AST重建，也不做字符串diff或猜锚点。所有普通提交（含901ms/非相邻旧组边界）和IME确认共用publish_input：基线受检范围/选区→受预算解析→plan_change→validate_change→Buffer.apply。自身输入发布使用稳定基线的EditContext(mode)，不会因为其自身草稿活动而被当作“其它命令”拒绝；真实草稿和组合一直保留至发布成功。

公开命令边界为DocCore::apply_change(&InputIdentity,&Parsed,&ChangePlan,now)。它先校验打开块/草稿基线，再从真实状态生成context并按pending、composition顺序拒绝不适用命令；不提交或封口输入。之后复验DocCore身份（文档、WorkingVersion、快照、配置代次、请求代次）、M0解析配置，以及validate_change所检查的mode/revision/snapshot/projection/selection/片段/权限/候选重读。全通过才一次Buffer.apply，随后封口、推进版本和同步打开块。redo本身不拒绝，实际新发布才截分支。该入口为显式命令消费者，不暴露可写Buffer。

普通/IME发布点和命令发布点的Buffer.apply均只消费validate_change取得的批次；Undo/Redo/reload继续沿已有独立历史协议执行。原doc_core.rs中三条裸CheckedBatch→apply提交路径已移除。

## Parsed与预算

不新增持久Parsed字段。输入在组/IME发布点用当前Buffer基线解析；命令由宿主借用携带身份的只读Parsed，apply_change即时采纳并消费，按完整身份和投影复验，不留下需要多入口失效的缓存。现有快照缓存仍由DocCore拥有、成功内容发布时清空，拒绝不改缓存。

输入EditBudget使用现有DocLimits：patch=PatchBudget(1,text.max_bytes,text.max_bytes)，text=DocLimits.text；parse=ParseBudget::m0，源码限额取m0上限与DocLimits.text的较小值。显式命令直接消费自己的受检plan，不重新套单输入patch1限制，Source多补丁及两个标记格式命令实际测试通过。未新增公共预算配置面。

不在type_text的相邻append、composition_update的候选替换或selection读取中parse；start_group/start_composition若触发旧组提交，解析发生在既定组发布边界。源码唯一直接parse调用在publish_input，edit内部预演与校验会重新解析候选；无头同步执行不构成V-05 UI响应预算证据，真实UI调度留后续。

## 兼容范围与代价

主Agent本轮明确：Visual沿用既有Replace契约，对已可见文字的局部修改可成功；新增Markdown隐藏标记仍拒绝，Source合法。探针保留：**hi**绘制末端2→source_at→src4，插入abc可plan→validate；abc在src0插入**hi**拒绝HiddenSourceChanged。HTML边界同样沿既有Visual拒绝规则，Source原文修改保留。本轮没有修改edit/markdown保护或重读规则，也没有以Source计划绕开Visual保护。

最简单方案采用具体私有函数与现有计划类型；新增的公开面仅规范已有要求的apply_change及携带EditRefusal的DocError::Edit。代价是组发布点额外基线解析和两次候选验证，需V-05后续调度/性能证据；换取无缓存双真源和无额外公共edit协议。测试529行聚合权限/事务状态机责任，超过约500行人工复核信号不机械拆分；生产消费者138行，原DocCore减少重复提交代码。

## 验证

真实宿主回归先红后绿，16项覆盖保护/HTML、Source、精确映射输入、pending或组合与redo并存的预检、身份/配置/投影/选区/预算失败、格式/多补丁一步历史、合法提交截redo、无变化/取消保留。状态快照涵盖DocCore字段、Buffer快照/文本/选区/行索引/历史光标及所有公开HistoryStep。History内部封口/时间未新增读取API；DocCore每步立即封口且本组每节点单步骤，拒绝前无Buffer.apply/seal调用，交独立审查核对。
