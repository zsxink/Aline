# Design

## Context

见 proposal；现有四 crate 的金样/拒绝测试各自验证单一场景，尚无共同可复跑的连续操作驱动。现有 `reuse` 是全量解析后的候选过滤与受检重定位，不存在可调用的独立增量解析器，故差分验证候选块的重定位后来源及展示，不能虚称测试了不存在的增量路径。

## Goals / Non-Goals

**Goals:** 每个随机步骤都有独立检查，错误立即返回并包含复现上下文；默认量可常规 CI 运行，扩量只补充证据。

**Non-Goals:** 不改解析、编辑、存储生产实现；不实现 M2 保存替换/崩溃恢复、不启动窗口或验证真实输入法；不消除 M0 豁免缺口。

## Decisions

- 使用具体函数和测试本地状态。测试驱动在各 crate 的 `tests/support/mod.rs` 保持相同实现，仅 std 依赖，使用普通 `mod support`；既有 safety 守卫不接受 `#[path]`，不绕过或扩大守卫。不建立新 crate/trait/全局 RNG。重复约百行驱动的代价小于引入测试依赖边；RNG、操作轨迹和字符串 oracle 都归测试栈所有，不产生运行时 API。
- 默认 seed 从固定非零常量起，16 seeds × 100 steps，环境变量 `ALINE_FUZZ_SEED`（十进制）、`ALINE_FUZZ_SEEDS`、`ALINE_FUZZ_STEPS` 和 `ALINE_FUZZ_TRACE` 可覆盖；非法/零/超限配置失败关闭，拒绝悄悄退默认。诊断记录完整操作前缀，失败可重放；纯输入性质使用逐段删除最小化，保证仍失败的输入不被 helper 修复。
- buffer 用独立字符串/历史栈 oracle 检查合法替换、undo/redo、reload、保存身份、非法批次零副作用；每步比较 bytes、选区、revision 和历史。锚点只检查来源边界与 affinity，不把实体映射强当双射。
- markdown 生成语法种子文档及 UTF-8 边界局部替换，重复解析检查稳定性；块+Gap 精确覆盖、映射源/绘制边界有效、精确映射往返、保护范围和 reuse 受检重定位逐项验证。失败使用相同性质函数最小化输入。
- edit 随机合法选区和替换/格式命令，核对计划及验证阶段不修改输入，旧快照/选区/投影必须拒绝；Visual 的保护区命中明确拒绝，Source 解除的仅产品保护。新计划的所有结果都显式匹配：无变化核对等值替换或已存在格式标记，拒绝限定为与命令/模式和实际保护范围对应的原因；fresh 输入的身份/坐标/内部错误不能被静默接受。只读能力属于 DocCore，不伪造 edit 本身拥有 ReadOnly mode。
- source 随机合法/坏 bytes 先 decode，独立核对解码只剥离一个头部 BOM，未编辑 encode 字节等同；编辑重开必须与编辑输入只存在引擎 §5.2 允许的 CRLF/LF 表示差异，不折叠孤立 CR 或正文 BOM，再核对编码/重开幂等且原基线不变。使用现有 cancellation、预算和已存在目标制造受检失败，真实磁盘状态由隔离测试目录持有；不存在的 M2 replace/crash hooks 不假定已实现。
- 负例仅在 `ALINE_FUZZ_INJECT=byte-loss` 时删除实际结果观测值的末字节，由正常比较断言非零退出；不得直接返回人为失败绕过断言。正常 CI 不设置。注入不改变生产代码，不将用户输入 panic 当测试通过。buffer/edit 输出的最小片段仅用于这个观测损坏性质，完整有状态复现仍使用 seed、步数与操作序列；Markdown/source 对可独立重放的纯输入运行相同性质缩减。
- 复用随机检查除源码端点重定位外，还比较种类、保护、样式及可见映射与全量新投影。Hidden 只检查合法来源和精确性，不强设局部可见位置：邻接表格的 Synthetic 分隔符可以把锚点放在无可见字符块的边界，完整投影的全局绘制落点不是隐藏块自身的可见坐标。该边界不豁免可见映射、源码分区或来源有效性检查。

## Risks / Trade-offs

- [有限语法/命令集合] → 与已有金样互补，报告生成集合及盲区；扩量不能推导覆盖完整。
- [oracle 同源] → buffer 用独立字节模型、source 原始 bytes、markdown 明确分区/边界与全量参考，不复制实现作为预期。
- [失败最小化代价] → 只在失败发生时运行；有状态历史失败保留完整操作序列及当步最小片段，不把仅片段称为完整可复现序列。
- [磁盘测试平台差异] → create-only 仅验证当前平台，绝不等价于三平台 M2 替换或断电持久性。

## Migration Plan

无数据迁移或公开接口变更；删除测试文件即可回退。主 Agent 复核测试和证据后更新现行验证计划，原始日志不入库。

## Validation Commands

默认入口为 `cargo test -p aline-buffer -p aline-markdown -p aline-edit -p aline-source --test fuzz_invariants --locked --offline --no-fail-fast`，执行固定起始 seed `1511464998`（`0x5a17_2026`）的 16×100 步。单次复跑设置 `ALINE_FUZZ_SEED=<seed> ALINE_FUZZ_SEEDS=1 ALINE_FUZZ_STEPS=<失败步号加一>`；`ALINE_FUZZ_TRACE=1` 输出每一步操作。扩量设置 `ALINE_FUZZ_SEEDS=64 ALINE_FUZZ_STEPS=500`，不会改动默认 CI 时间预算。注入负例在同命令加 `ALINE_FUZZ_INJECT=byte-loss ALINE_FUZZ_SEEDS=1`，使用 `--no-fail-fast` 确保四个 crate 都执行，预期 exit 101；移除注入后同 seed 需 exit 0。原始命令、失败轨迹与两次重复错误上下文核对留 logs，不作为公开产品界面或跨平台证据。
