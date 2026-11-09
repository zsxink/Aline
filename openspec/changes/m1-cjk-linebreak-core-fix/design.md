# Design

## Context

动机见 proposal。W2 真实六组合及短样本已确认软折行行首闭标点；Source 请求 Menlo，Visual 的 italic 已传至 FontStyle::Italic。中文直立并不证明 Aline 丢失样式，实际 fallback run/advance 尚未取得。

已核对锁定 gpui-pre 0.3.9：StyledText→TextSystem::shape_text→layout_wrapped_line→LineLayout::compute_wrap_boundaries（text_system/line_layout.rs）。该函数约 232～265 行按 word-char/最后候选点或无候选硬断。line_wrapper.rs 中普通 wrap_line 和 truncate_wrapped_line（约 431 行）另有候选点逻辑；仅修改 is_word_char 会遗漏开标点和无候选硬断，也不能覆盖 shaped-line 主路径。

用户已批准准备修复方向；具体本地来源、补丁、完整依赖图及任何新增平台钩子仍待精确审计。W1 的 test-support 例外仅限其隔离场景，不自动批准本变更的生产特性或 vendor。

## Goals / Non-Goals

**Goals:** 保留当前单一 TextLayout 几何和原文，最小化修正核心断点策略；用独立金样与真实字形验证两文件内的三个入口；补实际字体 run 取证以区分斜体请求、真实 face 与回退。

**Non-Goals:** 不写第二套 UI 分行/命中引擎，不改源码或 projection 字符，不引入新字体、解析器或运行时；不升级 GPUI、不预设 vendor Apple/macOS 三包；不实现 glyph 合成斜体、平滑切换、M2 输入或 M4 主题。

## Decisions

### 1. 在既有核心布局内使用一个私有纯规则

问题是候选断点缺少中文禁头尾限制，而不是源码解析。最简单方案是在候选点选择和无候选回退共同使用合法边界判断，消费已有文本/相邻字符及调用方的真实 glyph cluster 信息。优先将纯规则放在核心 text_system 的私有连贯 helper，供 shaped-line、普通 LineWrapper 与 wrapped-truncation 入口共用；不新增 trait、service、全局状态或跨 crate 公开 API。

候选禁行首集合由 W2 已有闭标点样本和独立金样明确（起始包含 `，。、！？；：）】》」』`），禁行尾包含 `（【《「『`；补充引号/其他括号时逐项提供独立预期。只约束自动软折行，不改写原文起始/显式换行。纯字符边界不替代实际 glyph/字素边界，主路径必须拒绝同一 cluster 内候选。LineWrapper 按字符迭代且不是同一 shaping 视图，验证其合法性约束，不强求两个算法产生完全相同的字节断点。

无合法候选不能继续执行原有硬断后报成功。先以仅标点、超宽 cluster、极窄宽度固定失败回归，验证既有预算/受限反馈能否承接；若保持现有公开接口无法给出诚实受限结果，暂停该候选，提交最小受检接口差异与影响审阅，不静默选择裁切、溢出或放宽规范。

替代方案是 Aline 用公开 shape_line/ShapedLine::split_at 自建行元素，但 StyledText/TextLayout 不开放注入 wrap boundaries；这会新增命中、caret、选区和未来 IME 几何所有者，因此当前不选。不得向文本插入 NBSP/零宽字符规避问题。

### 2. 所有者、依赖与写入口

| 模块 | 责任及所有者 | 输入/输出与边界 |
|---|---|---|
| GPUI 核心纯断点 helper | 无可变状态，只判合法软折行候选 | 只读相邻文本/cluster 信息→布尔或受检候选，不读文件/窗口 |
| 两个现有核心布局模块 | 继续拥有真实 glyph/行宽与包装/截断派生结果 | 使用同一规则，保留既有预算、有限工作和结果失败语义 |
| aline-ui reader/readonly_hit/layout | 保持唯一当前 TextLayout、映射与布局身份 | 只做最小消费/错误适配；不持第二套可编辑全文或坐标转换 |
| 验证/准入脚本 | 拥有隔离候选来源、固定 diff/图/源码身份及运行输出 | 正式受审归档→logs 内候选→可审阅证据；准入前不得激活生产来源 |

八 crate 方向、五引擎 GPUI 隔离不变；源码与历史无新写入口。布局缓存仍绑定文档/投影、宽度、缩放、字体和换行环境；替换二进制会重建缓存，不借补丁新增跨进程缓存。旧点击/异步恢复沿原身份复验。helper 的模块拆分由两个现有消费者共享同一纯规则决定，不按行数机械拆分。

### 3. 先形成最小来源补丁，再申请生产准入

从与 Cargo.lock checksum 一致的原始 core 归档创建 logs 内副本；保留完整原始许可证/来源与最小 diff/文件哈希，不修改全局 Cargo 缓存。首先只评估核心包，按真实 Cargo metadata/tree 验证同版本 Apple/macOS 依赖能否统一使用该核心；若要求连带来源变化，列出新增差异重新审阅，不自动扩大为三包 vendor。

申请必须含原始归档、补丁及修改后文件哈希、许可证、完整图的所有入边/target/features、五引擎隔离与本机选包。既有 guard 应先拒绝未登记本地来源；独立审查后取得具体批准才按最小范围接入。新增守卫例外配正例、最小违规 diff/锁/旁路反例。自有新增逻辑不用 unsafe，不借外部原有 unsafe 扩大 Aline 例外。

### 4. 字体观察与合成机制分开

在受限 opt-in 诊断中，用公开 shape_line、font_id_for_index、x_for_index 等可用入口，绑定二进制/字体代次/样本/模式/宽度，记录请求字体、实际 shaped run 的 opaque FontId 与 advance。公开 API 不能解析 face 名称时保持未知，不以墨迹宽或字体列表猜回退名；如必须加平台诊断钩子，另列最小接口与准入审阅。

诊断不进入用户默认流程、不在 render 热路径写盘，不输出无关文档内容。斜体先比较 normal/bold/italic/bold-italic 的实际 run；本变更不在 paint 末尾 skew 全元素，也不为未知 face 加合成身份。Source 不应用 Markdown 粗斜体。

## Risks / Trade-offs

- [没有合法断点] → 独立失败用例先行，禁止硬断伪通过；必要受检接口差异先审阅。
- [字形簇被拆开或 wrapped-truncation 漏修] → 覆盖三个入口、ZWJ/组合字符/连字/截断，比较真实 cluster 边界和完整原文。
- [本地来源影响整个 GPUI 闭包] → 固定原始归档与最小 diff，完整 metadata/tree/guard 逐边审计，不改包版本或全局 allowlist。
- [字体回退被错误归因] → 记录 actual run/advance；缺少名称只记 opaque id，不据此接受中文斜体限制或直接合成。
- [旧布局沿用] → 宽度/字体/文档变化与关闭替换继续复验身份，真实边界两侧命中/模式往返核对字节与历史。

## Migration Plan

先完成独立金样、隔离最小补丁与精确准入材料，取得批准后再接最小来源替换；随后在 macOS arm64 Release 重测六组合与真实点击/锚点/源码/历史，并跑完整工程门禁和独立复审。仅 macOS arm64 为本轮实测要求，不改 V1 三平台范围。

不迁移用户数据。回滚恢复原受审来源/锁/指纹并重新构建、重建派生布局，不改原文件、源码、revision 或历史；回滚不算能力通过。正式补丁保留最小维护清单，运行原文留 logs，阶段摘要回填 V-04/V-07/V-09。
