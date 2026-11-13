# Tasks

## 1. 原值与语义提取（UI §3.1～3.2、R-13、S-7）

- [x] 1.1 建立私有 palette/token 两层和逐值独立测试，值注明已确认/原型/实验来源；验收 `cargo test -p aline-ui --lib native_window::tokens`。2026-11-08：1 passed，逐值检查 19 个语义常量；证据 `logs/validation/m1-single-window-readonly/2026-11-08-w6/token-test/`。
- [x] 1.2 shell、reader、window 仅消费语义色，清理旧常量与散落字面量；验收守卫、`cargo fmt --all --check`、`cargo clippy -p aline-ui --all-targets -- -D warnings`，检查 diff 无视觉/布局/公开 API 变化。2026-11-08：接线已实施，守卫与 UI Clippy exit 0；首轮完整 fmt check 命中并行实施中的 W3 文件及 tokens 格式，tokens 已单独格式化，等待工作树稳定后主 Agent 完整复跑，尚未勾选。

## 2. 防回归守卫（CODE_STANDARDS §6、R-17、S-14）

- [x] 2.1 新增专属颜色字面量守卫与正例/最小违规反例，测试注释/字符串/test 模块不误报、生产组件不得漏报；验收 `python3 scripts/check_ui_tokens.py --self-test` 与正常源码检查，输出限制在 logs/validation 并归一化。2026-11-08：6 项自测通过，实际源码无违规；复用 `safety_lex` 的词法与精确测试作用域，不凭文件名豁免。证据 `logs/validation/m1-single-window-readonly/2026-11-08-w6/tokens-self-test/` 与 `tokens-guard/`。

## 3. 真实证据与交付（V-09 分项、R-17）

- [x] 3.1 取得相同 binary 构建条件、夹具、窗口内容尺寸/缩放、语言/模式的重构前后真实 macOS 截图并比较；记录截图/身份/观察于 M1 evidence 下 tokens 子目录，不用数值测试代替真实视觉证据。
- [x] 3.2 回填 `docs/ui-spec.md` 与验证计划的 token 化实现/已测边界，保留 M4 主题及 Windows/Linux 未测；运行 OpenSpec 严格校验与适用 workspace fmt/Clippy/test/工程守卫，实际结果留 logs，不提交/推送/归档。

2026-11-09 本轮：完整 workspace 29 项通过（workspace-final-02）。真实 Visual A/B 采用同一快照反向展开 token 的字面值控制版，非历史产物；初次正文/工具栏/状态栏差像素 0，原生标题与滚动条存在差异，追加 End/Home 抓图阅读位置不一致，不声称全窗零差或 Source A/B。源清单、19 色原值和真实截图/像素比较见 finish-01/w6-before/；该明确边界与 UI/V-09 已同步。
