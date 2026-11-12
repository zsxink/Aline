# Design

## Context

见 proposal 的 Why。现有 `FontEnvironment` 已拥有启动字体激活与失败状态、有效字体代次；reader 消费该快照，布局缓存拥有字体代次键。V-04 已记录 2026-11-07 macOS 字体实验，但没有 W2 六组合测量。

## Goals / Non-Goals

Goals：提供可复跑的独立夹具和严格六组合观测完整性核对，定位当前实际换行路径，再由真实结果决定后续。

Non-Goals：不改变生产 renderer、坐标、字体、行距或依赖。当前缺陷若需 vendor/预折行/接受限制，先提供实测及最小失败输入，再单独裁决。

## Decisions

使用一个 Markdown 夹具和一个普通 Python 核对函数。夹具包含禁行首/行尾集合、重复临界文本、四种字形与中英文/emoji/组合字符。核对器拥有六组合清单与证据模板；真实观测由原生窗口操作填写，截图与原始日志仍归本轮 evidence 目录，输出经过 evidence_paths。无需 trait、服务或生产公开 API。

测量宽度指正文实际可绘制宽度，不是窗口宽度；窗口尺寸、缩放另记。Source 中 Markdown 标记本身可见，观察应描述实际逻辑行的视觉折行，不能把 Visual 的无标记文本直接当 Source 金样。模板保留每一组合的测量/观察空值；缺值和缺截图导致非零退出。自动核对只证明记录完整与文件身份匹配，不证明字形正确或能力验收。

renderer 换行路径从锁定源码与 Aline 调用链取证；不把 quiet-md 的两个历史实现直接当 Aline 当前调用路径。对字体回退未取得实际字体 run 的情况明确记“肉眼观察，字体 run 未取证”，不能断言具体回退字体。

## 本轮收尾裁决（2026-11-12，按决定 §35）

用户裁决见[决定 §35](../../../docs/decisions.md)。换行侧 M1 接受已知限制、vendor 推
M2；斜体侧不提出合成斜体；CJK 回退侧不新增平台诊断钩子、face 身份记为显式未知并留
M4；平滑侧不做正文开/关对照（属 M4 渲染路径）。

这不改变本设计的核对口径：`evidence_complete` 只证明记录完整，`capability_passed`
保持 `null`；2.1c / 2.2b / 2.2c 仍不勾选，2.2c 在 M2 重新评估前保持未决。

**2026-11-12 追加（决定 §36 与平滑裁决撤销，不改写本节）：** 3.2e 已按完整生产准入链
落地，「vendor 推 M2」只描述 §35 裁决时的状态；平滑侧的「不做正文开/关对照」已被
撤销，真实开/关对照已取得。`evidence_complete` / `capability_passed` 口径与 2.1c /
2.2b / 2.2c 未勾选状态不变。

**2026-11-12 六组合平滑对照（不改写上段）：** 2.1c 已按真实窗口 A/B 勾选，覆盖
320/480/768 × Visual/Source 六组合（HEAD `159725ee`、`app_sha256=56323a4c…`）：
应用偏好域 `AppleFontSmoothing=0`（关）与未设置（系统默认，本机为开）各启动一次，
六组合正文 ROI 变化 31,197–66,735 像素、墨量 +16.25%～+19.22%，偏好已恢复。上段
「2.1c 未勾选状态不变」为当时状态，历史保留。`evidence_complete` 只证明记录完整、
`capability_passed` 保持 `null`；2.2b / 2.2c 仍未勾选，2.1 伞项因观测集仍是
2026-11-09 的「未执行平滑 A/B」记录而未勾，重建观测集属跨轮组合、需裁决。证据
`logs/validation/m1-single-window-readonly/2026-11-12-smoothing-six-combos/`。

**2026-11-12 2.1 伞项勾选（跨轮组合，不改写上段）：** 用户接受跨轮证据组合；新观测集
`logs/validation/m1-single-window-readonly/2026-11-12-2_1-cross-round/observations.json`
将主截图与换行/行高/斜体（2026-11-09 `b7fbafaa…`）、回退 run 与位置间隔
（2026-11-11-cjk-06 `c87400a9…`，logs 隔离 patched Release）、平滑六组合 A/B
（HEAD `159725ee`、app `56323a4c…`）组合为完整六组合记录，核对入口返回
`evidence_complete: true`。设计口径不变：该布尔值只证明记录/文件身份完整，
`capability_passed` 保持 `null`；`line_boundaries` 是 3.2e 修复前状态，当前构建
未重测；face 身份与 shaped advance 未取证；2.2b / 2.2c 仍未勾，V-04 未整项通过。

## Risks / Trade-offs

- 人工测量存在误差 → 记录 px/pt、scale、截图、方法，保留原始值与不确定性。
- 记录完整不等于能力正确 → 报告独立 `evidence_complete`，不给 V-04 整项通过布尔值。
- 新构建会使旧截图失效 → 运行身份与夹具 SHA-256 必填，历史结果只作历史引用。
- 未能取得真实六组合 → 保留真实测量任务未完成；不填合成数据。

## Migration Plan

只新增验证资产；删除新增文件即可回退，不影响生产状态。所有运行副本留 logs，原始夹具为可复跑资产。
