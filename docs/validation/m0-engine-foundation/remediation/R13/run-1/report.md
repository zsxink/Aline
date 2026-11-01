# R13 修复与取证报告

日期：2026-11-01。基线 HEAD 为 `96024867f7ae949d7328232b05d93ed5bc6033d9`；本轮工作区包含未提交的先前整改输入。按主 Agent 指示不提交，且不伪造 R01–R12 后续提交点。基线状态、dirty input identity 与 diff 摘要见 `baseline-*` 记录。CI 只核实配置，远端运行未核实。

| 项 | 原文/问题 | 证伪事实 | 处置 | 理由 |
|---|---|---|---|---|
| F-12 AGENTS 构建与检查 | “当前仓库仅有文档”“当前尚无配置或 CI” | 8 成员 Cargo workspace、lint inheritance、guards/self-tests、m0 workflow 配置存在 | 改写第 48、55 行现行事实 | 现行状态已被直接核验；明确远端 CI 未核实，避免宣布验收完成 |
| F-12 docs/README | “当前无 Cargo workspace”；“尚无 Cargo workspace 或 CI” | workspace 和 workflow 已配置 | 改写第 3、25 行相关现行陈述 | 原段落的日期背景保留；M0/V 状态仍标为未完成 |
| F-12 rust-style-guide | “仓库尚无 Cargo workspace、Rust 配置或 CI” | workspace、lint 配置、guards、workflow 配置已存在 | 改写状态段第 7 行 | 不把配置存在写成远端 CI 或阶段验收通过 |
| F-12 CODE_STANDARDS | 两行“当前未启用” | engineering/lint scripts 与 self-test 入口已存在 | 改写 §6.2 两行 | 记录已接入及证据入口，不宣称完整出口通过 |
| F-17 4.1 报告 | 第 33 行用例总数 31，与代码/数据/输出不一致 | test constant=32，金样 32 行，独立输出 32/21/11 | 只追加第 5 节勘误 | 历史原文不改写；明确仅报告 prose 有误，代码金样一致 |
| F-18 提交 scope | `16f8e30` 以 `docs(workspace)` 修改任务勾选 | git show stat 只有 tasks.md，+2/-2；RS-05 纯文档 scope 为 docs | 在 `scope-deviation.md` 登记 | 不改 Git 历史，也不更改 RS-05 |
| 保留未改：CODE_STANDARDS 头部 | 第 3 行仍写“当前尚无 Cargo workspace，自动化工程门禁须在 M0 建立并验证” | 与本轮核实的 8 成员 workspace、已接入守卫不符；文件上方“生效日期 2026-10-30”本身不足以把该现行措辞改成历史描述 | 本轮不改 | R13 派发契约把该文件写入权限严格限定为只改 §6.2 第 153/154 行；越界修改未获授权。明确登记为 F-12 残留，交 R14 跟踪；本报告不宣称四处陈述完全清零 |

## 未改项与边界

- 保留 `docs/README.md` 中 2026-10-30 初始化规范的日期化历史语境；只更新同句中的过时现状，不移除历史事实。
- 保留 4.1 报告的原始“31”句子，并追加纠正；不改旧日志、金样、测试代码或提交。
- 保留 `openspec/CODE_STANDARDS.md:3` 的“当前尚无 Cargo workspace”头部陈述：它不是历史快照措辞，事实已过期；依派发契约的精确文件所有权本轮不改，作为 F-12 残留交 R14，不能据此宣称文档已全量一致。
- 保留 AGENTS 的 fmt/clippy/test 门禁要求；更新后不意味着这些门禁本轮执行或持续通过。
- 未更改 `docs/decisions.md`：F-18 是既有提交例外的登记，局限于本轮修复证据，RS-05 的常设决定不变。
- 不宣布 M0 闭合、V-01～V-09 通过或守卫充分。

## 验证限制

本轮使用 Git、awk、sed、rg 与 Python 文档检查；没有运行 Cargo 测试、lint、类型检查或 GitHub Actions。多数成功命令有外部捕获的 stdout/stderr；`pre-fix.command.json` 是 `capture-pre-fix.py` 自写的摘要，原运行未保留外部 stdout/stderr，且其中 exit=0 是脚本内部完成标记，不是独立观察到的进程退出码。七条 `attempt-1-*.command.json` 也没有同名前缀 stdout/stderr；可见的非前缀日志来自后续成功重跑，不能回填为第一次输出。`attempt-1-guard-self-test-entries.command.json` 记录 exit=2（错误的 rg 参数形态），仅作为部分命令元数据保留。未补造旧日志；另用 [HEAD 摘录一致性检查](pre-fix-head-integrity.json) 比对 5 个当时工作树干净的来源文件及 10 行摘录，均与记录的 HEAD 相符。命令入口见 `commands.txt`。工作树存在其他代理的未提交修改；本轮只负责派发契约列出的文件。

## R14 追加勘误（2026-11-01）

R13 独立 reviewer 当时指出 `openspec/CODE_STANDARDS.md:3` 仍称“当前尚无 Cargo workspace”；该结论对 R13 结束时的文件状态成立。R14 开始后用户明确授权最小更正，主 Agent 仅更新该行状态描述：M0 workspace/本地门禁已建立、R14 登记离线检查仍有未核实、M0 出口未闭合、V-01～V-09 状态不变。R14 governance reviewer 独立核对认为更正准确。历史 R13 报告及其审查结论仍按各自记录时点保留；本勘误不宣称远端 CI、跨平台或 M0 出口通过。
