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

### 5. 本轮裁决落定（2026-11-09-cjk-03，按用户授权的建议）

用户授权「裁决的部分按照你的建议来，最优的建议考虑的以后的功能迭代」。完整裁决、
逐项影响与未决项见 `logs/validation/m1-single-window-readonly/2026-11-09-cjk-03/cjk-linebreak/decision/recommended-ruling.md`
与 `decision/minimal-interface-proposal.diff`（提案，未应用、未编译、未激活、未批准）。

**接口裁决（已落定）**：受限回退选 A1 + A4 受检组合——有合法候选则在合法字形边界
软折行；无合法候选不再硬断报成功，保留原文与可达入口并报告 `NoLegalBreak`；单簇过宽
则整簇移下一行并报告 `ClusterWiderThanLine`。**不选 A2 溢出、不选 A3 保留硬断**，
两者都与 spec delta 的 MUST NOT 冲突，自行放宽属硬边界禁止项。A3 的「显示受限提示」
单独列为后续 change 待裁决项，本 change 不接线。

**最小公开面（面向 M2 caret/IME、M3 表格/clamp、M4 双向/竖排、M5 多窗与 GPUI
升级重打）**：锁定 GPUI 补丁的公开面只新增两个条目——`#[non_exhaustive] WrapLimit`
与 `TextLayout::wrap_limit() -> Option<WrapLimit>`。`Option` 区分「未测量/不可判定」与
「已测量且无受限」，去掉提案里的 `unwrap_or_default()`；`WrappedLineLayout.limit` 保持
`pub(crate)`；`compute_wrap_boundaries` 私有返回类型可变；**拒绝** `Boundary.limit` 与
`truncate_wrapped_line` 公开三元组，改用私有 `truncate_wrapped_line_with_limit` +
二元组薄封装，使既有调用点零改动。B6 Aline 提示接线本 change 不做；B7 真实 CoreText
簇索引形态保持开放，需 3.2 真实 run 证据。

**来源准入（结论：暂不批准）**：不批准申请代 / `hashes.json` 代 / 磁盘代任何一代作为
可编译身份，暂不允许 3.2 编译。三代 diff 与文件哈希互不相同、磁盘代含需要
`test-support` 依赖的 `[[test]] cjk_wrap`、两处未登记隔离编译尝试、`make_hashes.py`
复跑配方失效、`repo_state` 与当前 HEAD 不一致；按「冲突列入待确认并暂停依赖该决定的
实现」处理。收敛要求（单一自洽代 + 全部文件哈希 + 重算七场景指纹 + 移除测试目标 +
修复复跑配方 + 独立评审 + 具体批准）见裁决文件 §二。范围仍限核心包、Apache-2.0、
五引擎隔离、七场景、最小例外；不扩整包 allowlist、三包 vendor、许可证泛化或 GPUI
升级，W1 隔离图许可不套用。

**规范处置**：spec delta **未改**——既有 requirement 与「不存在合法断点或字形超过宽度」
Scenario 已要求「有限时间内产生可检查的受限/失败结果并保留原文」，A1 + 类型化状态
正好满足，无需放宽。按「若规范因此变化，先更新工件并重新评审」：规范未变化，故只
同步 proposal/design/tasks 并重跑 `openspec validate --strict`；公共 API 变更仍按
AGENTS.md 路由安排独立复审（只安排、未执行）。

**本轮边界**：未改生产 `Cargo.toml` / `Cargo.lock` / 策略资产，未用 `[patch]` 或本地
来源构建，未激活 patched GPUI，未启用 test-support，未重新生成候选代（避免第四代身份），
未新增平台诊断钩子，未取得真实 shaping / 像素 / 命中证据，未勾 3.2 / 5.x。

### 6. H1 裁决落定（2026-11-09-cjk-04，方案 A + R1）

用户「按照你的推荐来」落定独立评审高 finding H1：**不改公开面、不改已批准字节身份**，
以受限 opt-in 钩子在真实 Window 的 prepaint 之后读取已有公开 `TextLayout::wrap_limit()`；
`wrap_line` 残留按 R1 登记为 crate 内可检查的产品公开面边界。依据：
`StyledText::layout()` 公开、`PreparedLayout` 已持有 `TextLayout`，H1 缺的是一次真实
窗口实测而非缺接口。方案 B 改变全部字节身份、C 因禁止 `test-support` 不可行、
D 需放宽 spec delta，均不采用。**spec delta 未改、未放宽**；
完整裁决见 `../2026-11-09-cjk-04/cjk-linebreak/decision/h1-ruling.md` 与[决定 §31](../../../docs/decisions.md)。

本轮只在 logs 隔离副本交付：纯记录规则（23/23 正反例、`forbid(unsafe_code)`、无 I/O）、
opt-in 钩子（四环境变量 gate、唯一状态来源、capture 不写盘、flush `create_new`、
未来 `#[non_exhaustive]` 变体拒绝）、在 patched GPUI 上 `cargo check` 0 告警与
8/8 运行验证、准入守卫 13 项反例。已批准 `minimal.diff` / 8 哈希逐字节一致，
生产源码 / 清单 / 锁 / 策略资产零变更。**H1 未清零**：真实 macOS arm64 窗口的
`Some(NoLegalBreak)` / `Some(ClusterWiderThanLine)` 记录未取得（5.1 首次授权范围），
钩子未接线到 prepaint 回调与后台 flush，新增钩子的独立复审待执行。
H2、B6、M1、5.1 仍待裁决 / 授权；任务仍 2/12。

## Risks / Trade-offs

- [没有合法断点] → 独立失败用例先行，禁止硬断伪通过；必要受检接口差异先审阅。
- [字形簇被拆开或 wrapped-truncation 漏修] → 覆盖三个入口、ZWJ/组合字符/连字/截断，比较真实 cluster 边界和完整原文。
- [本地来源影响整个 GPUI 闭包] → 固定原始归档与最小 diff，完整 metadata/tree/guard 逐边审计，不改包版本或全局 allowlist。
- [字体回退被错误归因] → 记录 actual run/advance；缺少名称只记 opaque id，不据此接受中文斜体限制或直接合成。
- [旧布局沿用] → 宽度/字体/文档变化与关闭替换继续复验身份，真实边界两侧命中/模式往返核对字节与历史。

## Migration Plan

先完成独立金样、隔离最小补丁与精确准入材料，取得批准后再接最小来源替换；随后在 macOS arm64 Release 重测六组合与真实点击/锚点/源码/历史，并跑完整工程门禁和独立复审。仅 macOS arm64 为本轮实测要求，不改 V1 三平台范围。

不迁移用户数据。回滚恢复原受审来源/锁/指纹并重新构建、重建派生布局，不改原文件、源码、revision 或历史；回滚不算能力通过。正式补丁保留最小维护清单，运行原文留 logs，阶段摘要回填 V-04/V-07/V-09。
