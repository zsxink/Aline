# decisions 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/decisions.md`；原文件 14569 bytes，SHA-256 `70bd18f88393d329264d9df6bedc6bb5916875ff83529fa57042fef095dbcccb`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L8 · 旧权威/编号背景（较新日期优先不再适用）

原片段 SHA-256：`45b0c77468f359ab67e57d8227e310ea17acd1481a891f93d9c68ca124f352d6`。

~~~~~~~~markdown
# Aline 决策记录

本文件按 [architecture-guide.md](./architecture-guide.md) §18 的纪律维护：**凡有争议的选型，必须落一条决策记录（含背景、选项、裁决、后果）**，先例可查、不靠记忆。规则与 `docs/` 冲突时以各技术文档为准；本文件与它们冲突时，以较新日期的裁决为准并回写文档。

格式：`D-YYYY-MM-DD-NN`，一条一裁决，时间倒序（新在上）。

---

~~~~~~~~

## L20–L24 · 旧技术复核批次背景

原片段 SHA-256：`53cb5be21e5524ca1afc4c61e4aa9b1a790f3b7d9b1bfe76dfb8c06fa626c55b`。

~~~~~~~~markdown
## 决策时间线

### 2026-10-26 · 技术方案复核裁决（全面复核四份文档后批量修订）

背景：M0 动工前对 engine-design / v1-architecture / architecture-guide / v1-features 做整体复核，发现 6 处设计矛盾（H）、7 处边界缺失（M）、9 处文档漂移（L）。以下为裁决及落点。
~~~~~~~~

## L47–L54 · 四条旧常设决定；21条D原文已逐条保存在现行decisions §4

原片段 SHA-256：`eadeb51140aefc50fb13a11c41398633699efdc7d687341c4580691aecc8630d`。

~~~~~~~~markdown
## 常设决定（不设截止日期，翻案需新裁决）

| 决定 | 依据 |
|---|---|
| 解析器锁定 pulldown-cmark（0.13），缺失语法用带字节映射的预处理改写器补齐，禁止引入第二个 markdown 解析器 | Ely ADR-0041（markdown-rs 慢 ~90 倍实测） |
| gpui 全工作区单一来源精确锁版：`gpui = { package = "gpui-pre", version = "=0.3.9" }`，vendor 平台补丁，升级是显式动作 | Ely ADR-0032（两个来源的 gpui 类型不兼容） |
| Windows 10 与 Intel Mac 为必须支持目标（非"新系统优先"） | 用户决策 2026-10-26 |
| 源文本是唯一事实来源；文本永不重建，编辑永远是区间补丁；存储逐字节保真（未编辑原样写回，编辑过只重放 BOM/CRLF 多数派） | engine-design 第 2 部分不变量、AGENTS.md 铁律 2/4 |
~~~~~~~~
