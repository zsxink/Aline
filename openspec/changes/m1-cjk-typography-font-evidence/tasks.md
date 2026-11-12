# Tasks

## 1. 可复跑测量资产

- [x] 1.1 新增独立中文排版夹具，包含完整开闭标点、混排、emoji/组合字符和四种字形；核对文件与固定 SHA-256，生成六组合观测模板，缺记录保持失败。
- [x] 1.2 实现六组合完整性核对入口，验证正常完整记录、缺组合/重复/缺截图/身份漂移反例；执行 `python3 scripts/check_m1_cjk_typography.py --self-test`，输出规范化证据。
- [x] 1.3 核对当前 renderer 的实际换行调用链及锁定上游实现，记录证据到本轮 logs；不以历史 quiet-md 补丁当当前事实。

## 2. 真实测量与决策

- [ ] 2.1 在真实 macOS 原生窗口按正文宽 320/480/768 px × Visual/Source 六组合取得截图与换行/行高/回退/斜体/平滑测量，运行核对入口达到 `evidence_complete: true`；真实结果另报告，不据完整性写能力通过。
  - [x] 2.1a 六组合截图与换行/行高/可见斜体真实测量：2026-11-09 已完成，`evidence_complete=true`；可见斜体差异有真实证据。完整性布尔值不替代整项能力结论。
  - [x] 2.1b 实际回退 run / glyph advance：2026-11-11-cjk-06 已取得。六组合第 25 行均记录 3 个 run：CJK fallback run 的 `requested_family` 为 `.SystemUIFont`（visual）/ `Menlo`（source），`resolved_family=unknown`、`face_source=Unknown`；首字符 TextSystem advance 为 15.875（visual）/ 16.000（source），拉丁 run 为 4.188 / 9.633。按复审限定，`advance_px` 不写成 shaped advance，`resolved_family` 不写成完整 face；真实 `glyph.position.x` 的相邻差另记为位置观察（visual Han 15.860、全角冒号 16.078；source 16.000）。超宽行 6/9/12/19/22 的簇形状见 `5_m1_typography/report.md` 与 `analysis.json`。
  - [ ] 2.1c 字体平滑对照：未取得，**未完成**。
  - **2026-11-11-cjk-06 负结果（仍未勾选）**：同一 patched Release、372×640 visual，进程级 AppKit 参数 A/B：`-AppleFontSmoothing 0` 只改变标题带 1704 像素（bbox `[169, 21, 491, 47]`），正文 y≥140 变化 0；`-AppleAntiAliasingThreshold 100` 与 `-AppleFontSmoothing 3` 相对 default 均 0 变化。正文渲染未随进程级参数改变，属于负结果；产品正文平滑开/关对照需要渲染路径改动，属新 scope，需另行裁决。见 `5_m1_typography/report.md` §2 与 `smoothing/runs.json`。
- [ ] 2.2 对发现的缺陷整理最小失败输入与实际能力结果，依 QMD-01.6 分别处理换行、斜体、CJK 回退与字体平滑的决策；如需 vendor/预折行/接受已知限制，先取得具体裁决，不勾选等待中的决定。
  - [x] 2.2a 已实测缺陷的最小失败输入与实际结果：行首闭标点、中文直立 / English 斜体已有真实复现（2026-11-09 finish-01/w2）。
  - [ ] 2.2b CJK 回退与字体平滑的最小失败输入/实际结果：未取证，**未完成**。
  - **2026-11-11-cjk-06 部分数据（仍未勾选）**：CJK 回退的实际结果已取得（六组合 fallback run / 首字符 advance / 位置观察 / 超宽簇形状，见 2.1b）；四字形样本显示 CJK fallback 的 Han/冒号位置间隔在四种样式下相同，全角标点在粗体下变化。字体平滑只有进程级参数负结果（正文 0 变化），“最小失败输入”未建立；因此 2.2b 不勾选，也不把负结果写成平滑缺陷已定位。
  - [ ] 2.2c 依 QMD-01.6 对换行、斜体、CJK 回退、平滑分别形成决策；vendor/预折行/接受已知限制需具体裁决，**阻塞**。
  - **2026-11-11-cjk-06 决策输入（仍未决策）**：换行侧有 5.1a–d 的真实窗口证据但生产来源未准入；斜体侧有 CJK fallback 直立/位置间隔观察，未提出合成斜体；CJK 回退侧 face 身份未解析（`unknown`/`Unknown`），vendor / 预折行 / 接受已知限制均未选定；平滑侧只有进程级参数负结果，正文平滑开/关需渲染路径改动。上述四项均待具体裁决，本 change 不自行选择，不勾选 2.2c。
- [x] 2.3 仅将实际分项回填 V-04/UI §3.2，保留历史字体证据、跨平台/运行中字体与完整主题矩阵未测，执行 OpenSpec strict 与差异/路径检查。

当前状态（2026-11-08）：1.1～1.3 的证据在 `logs/validation/m1-single-window-readonly/2026-11-08-w2/`。检查器自测 2 项通过；无观测默认运行与仓库外输出目录均 exit 1，strict 校验与 diff 检查通过。静态调用链确认常规 StyledText→shape_text→line_layout 的折行路径，未修改上游。

2.1～2.3 未完成：尚未取得六组合真实测量，不用现有字体激活截图替代；尚无可支撑 vendor、预折行或接受 M1 中文排版限制的实测和具体裁决。不将建档、夹具或完整性自测视为 V-04 排版通过。

2026-11-09 本轮：六组合实际截图与逐像素行带测量完成，完整性 evidence_complete=true，能力 capability_passed=null；V-04/UI 已回填，最小片段真实复现行首闭标点和中文直立/English 斜体。证据 finish-01/w2/observations.json、pixel-measurements.json 及 gui/w2-*。2.2 仍未完成：未接受已知限制、未批准 vendor 或改变 renderer，实际 fallback run/glyph advance/平滑 A/B 未取证，不把记录完整性当能力通过。

2026-11-09 用户已选择 M1 内最小核心断点修复准备方向，并要求中文斜体先取实际 fallback run。独立 change `m1-cjk-linebreak-core-fix` 已形成 proposal/design/spec/tasks，具体补丁/来源准入和修复验证未执行；不把方向决定记为已批准 vendor 或关闭 2.2。

2026-11-09 独立复审核对后将 2.1 退回未完成：六组合截图、宽度、换行、行高与可见斜体差异已有真实证据，但实际回退/advance 和平滑对照仍未测。设计允许明确记录未取证，核对器的 `evidence_complete` 只证明记录完整；该布尔值不能代替整项测量完成。保留已证分项，不扩写原契约为必须解析具体 face 名称，也不把修复本身纳入测量任务。复核见 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/review/cjk-plan-final.md`。
