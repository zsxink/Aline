# Tasks

## 1. 可复跑测量资产

- [x] 1.1 新增独立中文排版夹具，包含完整开闭标点、混排、emoji/组合字符和四种字形；核对文件与固定 SHA-256，生成六组合观测模板，缺记录保持失败。
- [x] 1.2 实现六组合完整性核对入口，验证正常完整记录、缺组合/重复/缺截图/身份漂移反例；执行 `python3 scripts/check_m1_cjk_typography.py --self-test`，输出规范化证据。
- [x] 1.3 核对当前 renderer 的实际换行调用链及锁定上游实现，记录证据到本轮 logs；不以历史 quiet-md 补丁当当前事实。

## 2. 真实测量与决策

- [x] 2.1 在真实 macOS 原生窗口按正文宽 320/480/768 px × Visual/Source 六组合取得截图与换行/行高/回退/斜体/平滑测量，运行核对入口达到 `evidence_complete: true`；真实结果另报告，不据完整性写能力通过。
  - [x] 2.1a 六组合截图与换行/行高/可见斜体真实测量：2026-11-09 已完成，`evidence_complete=true`；可见斜体差异有真实证据。完整性布尔值不替代整项能力结论。
  - [x] 2.1b 实际回退 run / glyph advance：2026-11-11-cjk-06 已取得。六组合第 25 行均记录 3 个 run：CJK fallback run 的 `requested_family` 为 `.SystemUIFont`（visual）/ `Menlo`（source），`resolved_family=unknown`、`face_source=Unknown`；首字符 TextSystem advance 为 15.875（visual）/ 16.000（source），拉丁 run 为 4.188 / 9.633。按复审限定，`advance_px` 不写成 shaped advance，`resolved_family` 不写成完整 face；真实 `glyph.position.x` 的相邻差另记为位置观察（visual Han 15.860、全角冒号 16.078；source 16.000）。超宽行 6/9/12/19/22 的簇形状见 `5_m1_typography/report.md` 与 `analysis.json`。
  - [x] 2.1c 字体平滑对照：未取得，**未完成**。
  - **2026-11-11-cjk-06 负结果（仍未勾选）**：同一 patched Release、372×640 visual，进程级 AppKit 参数 A/B：`-AppleFontSmoothing 0` 只改变标题带 1704 像素（bbox `[169, 21, 491, 47]`），正文 y≥140 变化 0；`-AppleAntiAliasingThreshold 100` 与 `-AppleFontSmoothing 3` 相对 default 均 0 变化。正文渲染未随进程级参数改变，属于负结果；产品正文平滑开/关对照需要渲染路径改动，属新 scope，需另行裁决。见 `5_m1_typography/report.md` §2 与 `smoothing/runs.json`。
  - **2026-11-12 用户裁决（决定 §35 其五，仍未勾选）**：**不做正文平滑开/关对照**。进程级 AppKit 参数负结果已证明正文 0 变化；开/关对照需改渲染路径，属 M4 主题/字体机制，不在 M1 实施。保留负结果与范围理由，2.1c 保持未勾，不以「已裁决不做」倒填为完成。
  - **2026-11-12 用户撤销上述裁决（其五），并更正旧负结果的根因（2.1c 仍未勾）**：上一条「不做正文开/关对照」已由用户撤销。复核发现旧负结果的**归因有误**：`-AppleFontSmoothing` 走 argv 参数域，只影响 GPUI 的 `glyph_dilation_for_color`（dilation 1→0）；而同构 Swift 复现显示，GPUI 的正文在 alpha-only `DeviceGray` 上下文栅格化，**dilation 0 与 1 位图逐字节相同**，因此旧探针本来就测不到正文平滑。真正改变正文像素的是 CGContext `allowsFontSmoothing` 默认值，来自应用/全局偏好域。已用应用偏好域 `defaults write aline AppleFontSmoothing 0` 做真实窗口 A/B（同 Release、同夹具、372×640 visual，跑完恢复为未设置）：正文 ROI 变化 17,875 像素、最大通道差 170、变化 256 行、墨迹 10,105,439 → 11,090,604（+9.75%）。证据 `logs/validation/m1-single-window-readonly/2026-11-12-smoothing-ab/`（`report.md`、`runs/run-2/ab.json`）。**2.1c 仍不勾**：本项要求六组合（320/480/768 × Visual/Source）内的平滑测量，本轮只覆盖单一宽度 372 的 Visual 正文，Source 视图、其余宽度与四字形均未测；且该开关是系统偏好，不是 Aline 产品设置，`crates/` 未调用 `set_text_rendering_mode`。
  - **2026-11-12 六组合平滑对照取得（勾选 2.1c，不改写上条历史状态）**：真实窗口、同一 Release（HEAD `159725ee`、`app_sha256=56323a4c…`、`cargo build -p aline --release --locked --offline`），对 320/480/768 × Visual/Source 六组合分别以应用偏好域 `AppleFontSmoothing=0`（关）与未设置（系统默认，本机实测为开）各启动一次并逐像素比较。六组合全部可观测：正文 ROI 变化 31,197–66,735 像素、最大通道差 112–165、变化 314–454 行，墨量比 1.1625–1.1922（+16.25%～+19.22%；Visual 约 +16.3～16.8%，Source 约 +18.3～19.2%）。偏好跑完已恢复为未设置；run-1 有两个变体在文件对话框遇到 `window_not_focused` 且无截图，run-2 全部成功且未触发重试，结论以 run-2 为准。证据 `logs/validation/m1-single-window-readonly/2026-11-12-smoothing-six-combos/`（`report.md`、`runs/run-2/ab.json`）。边界：开关是系统偏好，不是产品设置；产品无平滑 UI/环境入口，`crates/` 未调用 `set_text_rendering_mode`；只测两态、仅 macOS arm64。**2.1 伞项仍不勾**：2026-11-09 `w2/observations.json` 六条记录仍写「未执行字体平滑开关 A/B」，核对入口 `evidence_complete: true` 是对该记录集的完整性判定；以本轮测量重建完整观测集并重跑核对入口属跨轮组合，需另行裁决。**2.2b 仍不勾**：本对照证明平滑可观测、可量化，不构成「最小失败输入」——没有输入被证明会因平滑而错排。
  - **2026-11-12 2.1 伞项勾选（跨轮组合，不改写上条）**：用户接受「用既有 2.1a/2.1b 观测 + 2026-11-12 六组合平滑数据组成新的完整六组合观测集（明确标注跨轮）」的方案。新观测集 `logs/validation/m1-single-window-readonly/2026-11-12-2_1-cross-round/observations.json` 由只读组装脚本从三份既有证据生成：主截图与 `line_boundaries`/`line_height`/`italic` 取 2026-11-09 finish-01/w2（commit `b7fbafaa…`、app `e88bd6fe…`，夹具 `m1-cjk-typography.md`）；`fallback`/`punctuation_width` 取 2026-11-11-cjk-06（commit `6ed33f08…`、app `c87400a9…`，logs 隔离 patched Release，六组合 run 用 `m1-5_1a-breakpoints.md` 第 26 行、四字形用固定夹具 visual 960×640）；`smoothing` 取 2026-11-12 六组合 A/B（HEAD `159725ee`、app `56323a4c…`）。核对入口返回 `evidence_complete: true`、`capability_passed: null`、`failures: []`，exit 0。**勾选口径：** 2.1 是测量完整性，不是能力通过；`line_boundaries` 记录的是 3.2e 修复前换行，当前构建未在本字段重测；`fallback` 的 face 身份仍为 `unknown`/`Unknown`，advance 不是 shaped advance；`punctuation_width` 是 `glyph.position.x` 位置间隔；`smoothing` 是系统偏好 A/B、不是产品设置。2.2b/2.2c 仍未勾，V-04 / V-07 / V-09 与 M1 出口未整项通过。报告 `logs/validation/m1-single-window-readonly/2026-11-12-2_1-cross-round/report.md`。
- [ ] 2.2 对发现的缺陷整理最小失败输入与实际能力结果，依 QMD-01.6 分别处理换行、斜体、CJK 回退与字体平滑的决策；如需 vendor/预折行/接受已知限制，先取得具体裁决，不勾选等待中的决定。
  - [x] 2.2a 已实测缺陷的最小失败输入与实际结果：行首闭标点、中文直立 / English 斜体已有真实复现（2026-11-09 finish-01/w2）。
  - [ ] 2.2b CJK 回退与字体平滑的最小失败输入/实际结果：未取证，**未完成**。
  - **2026-11-11-cjk-06 部分数据（仍未勾选）**：CJK 回退的实际结果已取得（六组合 fallback run / 首字符 advance / 位置观察 / 超宽簇形状，见 2.1b）；四字形样本显示 CJK fallback 的 Han/冒号位置间隔在四种样式下相同，全角标点在粗体下变化。字体平滑只有进程级参数负结果（正文 0 变化），“最小失败输入”未建立；因此 2.2b 不勾选，也不把负结果写成平滑缺陷已定位。
  - **2026-11-12 用户裁决（决定 §35 其五，仍未勾选）**：平滑侧不做正文开/关对照（属 M4，需改渲染路径），本项的最小失败输入不建立；CJK 回退侧 face 身份记为显式未知、不新增诊断钩子（4.1c）。2.2b 保持未勾。
  - **2026-11-12 用户撤销上述裁决（其五），平滑侧已取真实开/关对照（2.2b 仍未勾）**：应用偏好域 A/B 已在真实窗口完成（见 2.1c 注），正文平滑开/关可观测、可量化（+9.75% 墨迹）。**2.2b 仍不勾**：本项要的是「CJK 回退与字体平滑的**最小失败输入**」，平滑侧现有的是**偏好对照**而非缺陷最小输入——没有任何输入被证明会因平滑而错排；CJK 回退侧仍只有 fallback run 与 `resolved_family=unknown`/`face_source=Unknown` 的显式未知，未构成缺陷最小输入。
  - **2026-11-12 平滑侧行带不变性复分析（补充实际结果，2.2b 仍未勾）**：对六组合 A/B 既有截图按与 2026-11-09 相同的正文阈值重提行带：六组合开/关两侧行带数量全部相同、每带下缘逐值相同；仅上缘有 1 px 前移（768 source 一例 3 px），与平滑增加抗锯齿像素跨过暗阈值一致，未见行重排或行位置移动。该结果把平滑侧的实际结果补成「光栅差异、非布局差异」，但**仍不构成最小失败输入**——没有输入被证明会因平滑而错排，故 2.2b 与 2.2 伞项仍不勾。证据 `logs/validation/m1-single-window-readonly/2026-11-12-2_2b-layout/report.md`、`result.json`。
  - [x] 2.2c 依 QMD-01.6 对换行、斜体、CJK 回退、平滑分别形成决策；vendor/预折行/接受已知限制需具体裁决，**阻塞**。
  - **2026-11-11-cjk-06 决策输入（仍未决策）**：换行侧有 5.1a–d 的真实窗口证据但生产来源未准入；斜体侧有 CJK fallback 直立/位置间隔观察，未提出合成斜体；CJK 回退侧 face 身份未解析（`unknown`/`Unknown`），vendor / 预折行 / 接受已知限制均未选定；平滑侧只有进程级参数负结果，正文平滑开/关需渲染路径改动。上述四项均待具体裁决，本 change 不自行选择，不勾选 2.2c。
  - **2026-11-12 用户裁决（决定 §35，仍未勾选 2.2c）**：换行侧——**M1 接受已知限制**，不把锁定 GPUI 的 fork 带入生产依赖，3.2e 生产准入本轮不立项，vendor 决策推到 M2；斜体侧——不提出合成斜体，CJK fallback 直立/槽位观察按已证/未知边界记录；CJK 回退侧——**不新增平台诊断钩子**（4.1c），`resolved_family=unknown`/`face_source=Unknown` 记为显式未知，留 M4；平滑侧——**不做正文开/关对照**（2.1c/2.2b），保留进程级参数负结果与「需改渲染路径、属 M4」的范围理由。**其中平滑侧的「不做对照」已于 2026-11-12 被用户撤销**，真实开/关对照已取得（见 2.1c/2.2b 注），但 2.1c/2.2b 仍未勾。2.2c 仍不勾：裁决只落定方向与边界，vendor / 预折行 / 接受已知限制在 M2 重新评估前保持未决，本 change 不自行勾选等待中的决定。
  - **2026-11-12 追加裁决（决定 §36，仍未勾选 2.2c）**：用户解除生产准入限制，3.2e 已按完整生产准入链落地；上面「3.2e 生产准入本轮不立项、vendor 决策推到 M2」只描述 §35 裁决时的状态。来源准入不替 2.2c 完成 QMD-01.6 的四项决策：换行 / 斜体 / CJK 回退 / 平滑仍未按该条形成最终决策，4.1c / 5.2d 仍未完成，2.2c 保持未勾。
  - **2026-11-12 用户裁决（决定 §41，勾选 2.2c）**：用户指示「2.2c 三项按接受已知限制处理」。换行侧沿用决定 §35 其一的 M1 接受已知限制（不把锁定 GPUI 的 fork 带入生产依赖，vendor 决策在 M2 出现 caret/IME 后重新评估）；斜体侧接受 CJK fallback 直立、拉丁 run 正常倾斜为 M1 已知限制，不实现合成斜体、不走整元素 skew、不换全文字体；CJK 回退侧接受 `resolved_family=unknown` / `face_source=Unknown` 的 face 身份显式未知（首字符 TextSystem advance 不是 shaped advance），留待 M4 字体机制；平滑侧接受「跟随 macOS 系统偏好、产品不提供开关」为 M1 已知限制，六组合 A/B 只证明像素可观测差异，没有输入被证明因平滑而错排。QMD-01.6 四项决策至此均有具体裁决，2.2c 勾选。**勾选不扩张：** 本裁决不把「接受已知限制」写成缺陷已修复或能力通过；2.2b（最小失败输入/实际结果）未因本裁决完成，2.2 伞项仍不勾；4.1c / 4.2c / 4.2d 仍未勾；V-04 / V-07 / V-09 与 M1 出口仍未整项通过。决策记录见 `docs/decisions.md` §41。
- [x] 2.3 仅将实际分项回填 V-04/UI §3.2，保留历史字体证据、跨平台/运行中字体与完整主题矩阵未测，执行 OpenSpec strict 与差异/路径检查。

当前状态（2026-11-08）：1.1～1.3 的证据在 `logs/validation/m1-single-window-readonly/2026-11-08-w2/`。检查器自测 2 项通过；无观测默认运行与仓库外输出目录均 exit 1，strict 校验与 diff 检查通过。静态调用链确认常规 StyledText→shape_text→line_layout 的折行路径，未修改上游。

2.1～2.3 未完成：尚未取得六组合真实测量，不用现有字体激活截图替代；尚无可支撑 vendor、预折行或接受 M1 中文排版限制的实测和具体裁决。不将建档、夹具或完整性自测视为 V-04 排版通过。

2026-11-09 本轮：六组合实际截图与逐像素行带测量完成，完整性 evidence_complete=true，能力 capability_passed=null；V-04/UI 已回填，最小片段真实复现行首闭标点和中文直立/English 斜体。证据 finish-01/w2/observations.json、pixel-measurements.json 及 gui/w2-*。2.2 仍未完成：未接受已知限制、未批准 vendor 或改变 renderer，实际 fallback run/glyph advance/平滑 A/B 未取证，不把记录完整性当能力通过。

2026-11-09 用户已选择 M1 内最小核心断点修复准备方向，并要求中文斜体先取实际 fallback run。独立 change `m1-cjk-linebreak-core-fix` 已形成 proposal/design/spec/tasks，具体补丁/来源准入和修复验证未执行；不把方向决定记为已批准 vendor 或关闭 2.2。

2026-11-09 独立复审核对后将 2.1 退回未完成：六组合截图、宽度、换行、行高与可见斜体差异已有真实证据，但实际回退/advance 和平滑对照仍未测。设计允许明确记录未取证，核对器的 `evidence_complete` 只证明记录完整；该布尔值不能代替整项测量完成。保留已证分项，不扩写原契约为必须解析具体 face 名称，也不把修复本身纳入测量任务。复核见 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/review/cjk-plan-final.md`。
