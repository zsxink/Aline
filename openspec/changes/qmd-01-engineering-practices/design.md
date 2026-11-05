# Design

## Context

动机、范围与证据边界见 `proposal.md`。本变更只处理工程/验证方法的治理记录，不实现运行时行为。现行规范已由 QMD-01 回写，但缺少与之对应的 OpenSpec change；同时回写中存在来源过度归因、fuzz 字段不一致、E-T20 阶段口径偏早、保存序列重复和 `.bak` 表述过强等问题，需要在本变更内一并校正。

现行权威关系不变：`docs/` 专项规范约束实现，`openspec/specs/` 保存主规格，`openspec/changes/` 保存拟议变更，研究文档只作证据索引。本变更的 delta 只进入 `engineering-governance`，不触碰产品 capability。

## Goals / Non-Goals

**Goals:** 用一个可校验的工程治理 change 记录 QMD-01 的方法采纳；让每条方法与来源、适用阶段和验证门一一对应；保证后续 W 项仍需独立立项；消除回写文档之间的字段和口径冲突。

**Non-Goals:** 不定义新的运行时状态、模块、API 或数据结构；不引入依赖、feature 或测试夹具；不测量性能、不执行 fuzz、不做 IME/平台/离屏实验；不修改 M1 change；不归档主规格。

## Decisions

### D1：一个治理 change，而不是八个 W 项 change

QMD-01 的方法采纳是同一次决定，来源、效力和边界共享；拆成八个 change 会重复同一套背景与边界。因此本 change 只记录“方法已采纳、W1～W7 未立项”，各 W 项在实施时再建独立 change。这样既满足 QMD-01 对验收方式变更的建档要求，也不把工作分解误当成批准。

### D2：现行 docs 是规范来源，delta 只补治理约束

方法正文继续留在 `docs/` 对应章节；本 change 的 delta 只写工程治理必须满足的结果与证据边界，不复制整段规范。这样做避免同一要求出现两个措辞版本，也符合“研究文档不是第二权威、归档不是第二份规范”的既有规则。

### D3：来源分为三类，不把 Aline 追加写成 quiet-md 采纳

QMD-01 方法表增加来源列，逐项区分：

- quiet-md 已有且可迁移的机制（确定性 fuzz 骨架、报告结构、原子写骨架）；
- Aline 既有规则（1 秒自动保存、10 秒恢复、冷/热与停顿/内存字段、GPUI 坐标约束）；
- Aline 追加或补强（最小输入、完整操作日志、样本 SHA-256/峰值内存、平台受检替换、失焦显式清理、中文排版测量门）。

没有证据的 quiet-md 行为标注“未确认”或“仅方向参考”，不写成已观察事实。

### D4：证据字段以一处为规范，其余引用

fuzz 失败证据统一为 `seed、步号、操作日志、最小输入`；E-T20、R-17-03、S-12-06、V-07/§14 与架构 §10.1 使用同一字段集，不再出现“最近操作序列”与“操作日志”两种口径。性能字段继续以 R-11-01/S-14-03 与 V-05 报告模板为准。

### D5：阶段口径按“W3 立项后”而非 M0

E-T20 的随机不变量要求来自 2026-11-05 方法采纳，M0 已按验收豁免结束；写成“M0 起”会追溯改变已裁决的 M0 范围。统一改为“M1 起（W3 立项后），M2/M3 扩展”，并明确不追溯改变 M0 验收。

### D6：`.bak` 写成“不采用文档旁副本作为恢复手段”

quiet-md 的 `.bak` 只用于 JSON 索引，不用于文档；Aline 的约束是恢复快照写应用管理的独立目标，不以文档旁 `.bak`/`.corrupt-*` 作为恢复手段。这样保留设计边界，又不新增“用户可见文件永不出现”的产品级承诺。

### D7：IME 清理写明等待语义

“显式清理”不等于 Blur 到达就立即 unmark/cancel。规范补充：先等待平台组合完成结果，等待期间保留原上下文，不抢焦点、不丢草稿；只有确认/取消结果或明确的本地结束条件才清理。坐标映射继续按字节/UTF-16/字素三种投影分离。

## Module and State Ownership

本变更没有运行时模块、状态所有者或数据流。若未来 W 项实现：

- 保存/恢复状态机归 `aline-ui` 的文档会话与持久化编排，`aline-source` 只提供受检 I/O 原语；
- fuzz 生成器与不变量检查归对应引擎 crate 的测试，不进入生产路径；
- IME 组合状态归 `aline-ui` 的唯一输入状态机；
- 语义 token 归主题模型与 UI 消费层；
- 离屏视觉测试归 `aline-ui` 的 feature-gated 测试目标。

以上只是归属边界，不是本 change 的实现承诺。

## Simplest Solution

只新增一个 OpenSpec change 与一个 delta 文件，同步 `CODE_STANDARDS.md` 的引用和检查表，修正现有文档的少量口径。不新增脚本、不新增自动门禁、不新增文档目录；验证使用现有 `openspec validate`、链接/锚点检查与 `git diff --check`。

## Failure and Cancellation Semantics

不适用：本变更没有运行时可取消操作。文档校验失败时保留工作树现状并修正文本，不通过删除要求或降低措辞来“通过”；OpenSpec 结构校验失败时按 CLI 诊断修格式，不伪造通过记录。

## Compatibility and Migration

无数据、API 或依赖兼容问题。已写入的 docs 不需要迁移；本 change 只补治理入口并修正表述。主规格在本 change 归档时才同步 delta，当前不把未实施门禁写成已启用。M1 change 与 M0 归档不受影响。

## Verification

1. `openspec validate qmd-01-engineering-practices --strict --no-interactive`；
2. `openspec validate --all --strict --no-interactive`（若 CLI 支持全量）；
3. 文档链接与锚点检查：QMD-01、开发流程 §3.1、各规范回写位置可跳转；
4. `git diff --check` 与绝对路径扫描；
5. `git diff --stat` 确认无 `Cargo.toml`/`Cargo.lock`/`crates/`/`app/` 改动，无主规格改动；
6. 人工核对同一字段、阶段和 `.bak` 表述在各文档一致。

以上只证明文档与规格结构，不证明任何 V/AC、W 项或平台能力。

## Trade-offs

- 一个 change 承载多条方法，审阅面较宽；换来来源和边界只写一次，W 项不会被误认为已批准。
- delta 不复制方法正文，读者需跳转 `docs/`；换来单一措辞来源，减少漂移。
- 保留强制措辞会增加后续交付记录成本；这是用户选择 A 的明确要求，且 W 项仍可按证据范围裁剪适用项。
