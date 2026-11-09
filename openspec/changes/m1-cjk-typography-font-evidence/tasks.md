# Tasks

## 1. 可复跑测量资产

- [x] 1.1 新增独立中文排版夹具，包含完整开闭标点、混排、emoji/组合字符和四种字形；核对文件与固定 SHA-256，生成六组合观测模板，缺记录保持失败。
- [x] 1.2 实现六组合完整性核对入口，验证正常完整记录、缺组合/重复/缺截图/身份漂移反例；执行 `python3 scripts/check_m1_cjk_typography.py --self-test`，输出规范化证据。
- [x] 1.3 核对当前 renderer 的实际换行调用链及锁定上游实现，记录证据到本轮 logs；不以历史 quiet-md 补丁当当前事实。

## 2. 真实测量与决策

- [ ] 2.1 在真实 macOS 原生窗口按正文宽 320/480/768 px × Visual/Source 六组合取得截图与换行/行高/回退/斜体/平滑测量，运行核对入口达到 `evidence_complete: true`；真实结果另报告，不据完整性写能力通过。
- [ ] 2.2 对发现的缺陷整理最小失败输入与实际能力结果，依 QMD-01.6 分别处理换行、斜体、CJK 回退与字体平滑的决策；如需 vendor/预折行/接受已知限制，先取得具体裁决，不勾选等待中的决定。
- [x] 2.3 仅将实际分项回填 V-04/UI §3.2，保留历史字体证据、跨平台/运行中字体与完整主题矩阵未测，执行 OpenSpec strict 与差异/路径检查。

当前状态（2026-11-08）：1.1～1.3 的证据在 `logs/validation/m1-single-window-readonly/2026-11-08-w2/`。检查器自测 2 项通过；无观测默认运行与仓库外输出目录均 exit 1，strict 校验与 diff 检查通过。静态调用链确认常规 StyledText→shape_text→line_layout 的折行路径，未修改上游。

2.1～2.3 未完成：尚未取得六组合真实测量，不用现有字体激活截图替代；尚无可支撑 vendor、预折行或接受 M1 中文排版限制的实测和具体裁决。不将建档、夹具或完整性自测视为 V-04 排版通过。

2026-11-09 本轮：六组合实际截图与逐像素行带测量完成，完整性 evidence_complete=true，能力 capability_passed=null；V-04/UI 已回填，最小片段真实复现行首闭标点和中文直立/English 斜体。证据 finish-01/w2/observations.json、pixel-measurements.json 及 gui/w2-*。2.2 仍未完成：未接受已知限制、未批准 vendor 或改变 renderer，实际 fallback run/glyph advance/平滑 A/B 未取证，不把记录完整性当能力通过。

2026-11-09 用户已选择 M1 内最小核心断点修复准备方向，并要求中文斜体先取实际 fallback run。独立 change `m1-cjk-linebreak-core-fix` 已形成 proposal/design/spec/tasks，具体补丁/来源准入和修复验证未执行；不把方向决定记为已批准 vendor 或关闭 2.2。

2026-11-09 独立复审核对后将 2.1 退回未完成：六组合截图、宽度、换行、行高与可见斜体差异已有真实证据，但实际回退/advance 和平滑对照仍未测。设计允许明确记录未取证，核对器的 `evidence_complete` 只证明记录完整；该布尔值不能代替整项测量完成。保留已证分项，不扩写原契约为必须解析具体 face 名称，也不把修复本身纳入测量任务。复核见 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/review/cjk-plan-final.md`。
