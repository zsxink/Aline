# R05b 独立复核任务书

你**没有**参与 R05 / R05b 的实现或验证。目标是独立判定 R05 复核退回的三项
（撞名 fail-open、成员继承无断言、深嵌套 traceback）是否真被修复，以及主 Agent
自行补强的断言是否**真的会红**。**不得**采信 `report.md` / `handoff.md` / `post-fix.txt`
的结论，它们只是被审材料。

## 被审对象

- 提交：`814148e`（R05b 修复）与 `e4a6af0`（R05 原始实现，已复核退回）
- 代码：`scripts/check_lints.py`
- 证据：`docs/validation/m0-engine-foundation/remediation/R05/run-1/`、
  `docs/validation/m0-engine-foundation/remediation/R05/run-2/`
  （`brief.md`、`pre-fix.txt`、`post-fix.txt`、`commands.txt`、`report.md`、`handoff.md`）
- 环境：`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`；Cargo 调用须 `--locked --offline`

## 必须独立回答的问题（逐条给出结论 + 你自己跑出来的证据）

1. **撞名 fail-open 是否真修复**：**自建**夹具（不复用 `/tmp/r05f4`、`/tmp/r05main`、
   `/tmp/weakrepo`）：在一个已有 `[lints.rust]` 表的 crate 里，用**引号点分键**
   （`"clippy.unwrap_used" = ...`）让守卫误认为已配置、而 Cargo 报
   `unused manifest key`。用 `git show e4a6af0:scripts/check_lints.py` 导出旧脚本，
   证明旧脚本 exit 0（fail-open）；新脚本 exit 非 0 且 detail 含行号与路径。

2. **三条拒绝路径是否真有断言**：读 `scripts/check_lints.py` 的 self-test，确认
   键集、类型、值（含点分键撞名）三类拒绝都有 case，且每个 case 断言**具体 detail**
   而不只是 exit code。自己挑 ≥2 个 case，删掉/展平 detail 里的关键片段
   （行号、路径、期望值），确认 self-test 变红。

3. **主 Agent 补的 `_assert_rejection_detail` 是否真的有效**：报告称九种单点变异
   全部变红。自己设计 **≥3 处**不同变异（不得照抄报告的九种），每处必须让
   `--self-test` exit 非 0 并命中你预期的 case。脚本按 `Path(__file__).resolve().parent.parent`
   推导 ROOT，变异副本必须放 `scripts/_revN.py`，跑完立即 `rm -f`，**不得留在工作树**。

4. **成员继承**：确认 workspace 级 `[workspace.lints]` 与成员级覆盖的继承语义
   有反例覆盖；自己构造一个「成员显式覆盖掉 workspace 的 deny」夹具，判断守卫是
   正确放行还是误报，并说明依据（`Cargo.toml` 语义，不是报告措辞）。

5. **深嵌套 traceback**：构造 ≥30 层嵌套的 `Cargo.toml` 表或等价输入，确认新脚本
   以命名规则 + 非零退出码结束，而不是抛出 `RecursionError` traceback。

6. **正常仓库无回归**：对真实仓库跑新旧脚本，比较退出码、`failures` 数量与规则集合；
   差集必须双向为空或新增的只会更严。`failures` 必须为 0。

7. **未覆盖范围是否诚实**：`report.md` / `handoff.md` 声明的残留
   （`RecursionError` 兜底无断言、引号点分键仅守卫放行而 Cargo 报错等边界）
   逐条判断是否属实，有无**未声明的**新盲区。

## 输出

只写 `docs/validation/m0-engine-foundation/remediation/R05/run-2/review.md`
（若已存在则写 `review-run2.md`）。≤120 行，含 A–F 分节：
A 结论（`APPROVE` / `APPROVE_WITH_CONCERNS` / `REJECT` + 一句话理由）、
B 逐问题独立结论与自建证据、C 变异记录（变异内容 → 变红 case）、
D 发现的缺陷（含严重度，无则写「无」）、E 残留与需主 Agent 裁决项、F 未覆盖范围复核。

**约束**：不 commit、不改任何被审代码或他人文件、不派子代理、
临时文件放 `/tmp` 并自行清理。返回正文 ≤800 中文字符，只报结论与关键证据。
