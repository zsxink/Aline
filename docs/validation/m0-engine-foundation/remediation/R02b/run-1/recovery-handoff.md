# R02b 恢复执行交接

恢复日期：2026-11-01。实际基线 HEAD：`96024867f7ae949d7328232b05d93ed5bc6033d9`。

## 已有修复与恢复决策

接手时 `git status` 显示本批两份 Rust 文件及旧 run-1 证据均干净；HEAD 本身已是
`fix(aline-markdown): 空来源复合片段锚定零宽源码点`。因此本轮没有重复实现或改动生产代码，
也没有覆盖旧证据、修改任务总状态或回退其它写入者的文件。旧 `brief.md`、`report.md`、
`handoff.md` 所述作者/时间/身份属于原实现阶段；本文件记录新增恢复核验，不能混作原运行。

根因和修复沿用 `9602486`：`compose` 的第二层空来源片经 `middle_anchor` 调用
`source_point`；非 Exact 首层片原来返回整段来源，现在按起点/结束点约定返回零宽锚点。
本轮核对该私有函数只由 `middle_anchor` 消费，非空第二层路径及 Exact 平移均未改变。
保持现有具体类型/函数即可；没有新增状态、依赖、抽象、公开接口或 UI 行为。
适用 M0、引擎 §6.3/7.1/14 E-T04/E-T08、AC-02/09、V-07、S-7 与工程治理的独立预期要求。

## 新增覆盖

仅修改 `crates/aline-markdown/tests/rewrite.rs`，新增
`compose_synthetic_source_anchors_stay_empty_at_each_boundary`。
两次插入分别为原始 `ab` 点 1 插入 `xy`，再于副本点 1/2/3 插入 `!`。
手写全部片段预期，覆盖第一层 Synthetic 起点/内部，以及终点被相邻 Exact 接管；
新插入的来源全部为 `1..1 Synthetic`，关闭旧 `report.md` §6 登记的 Synthetic 分支缺口。

原 Aligned 起点/内部及 Hidden 回归与修复前失败证据继续保留：
`pre-fix.txt`、`pre-fix-probe.rs`、`post-fix.txt` §F/§H1；本轮没有伪造新修复前运行。
新增测试用于既有正常分支覆盖，未声称其在旧源码上也会失败。

## 本轮命令与结果

原始逐命令 argv/cwd/UTC 起止/退出码/stdout/stderr/受控环境和 SHA-256 位于：

- `recovery-checks.jsonl`：恢复时 HEAD 身份与工作树；原 HEAD 全 crate **126 passed**、
  Clippy `--all-targets -- -D warnings`、fmt `--check` 全部 exit 0。
- `recovery-final-checks.jsonl`：新测试单跑 **1 passed**；最终全 crate **127 passed / 0 failed**，
  rewrite 目标 **15 passed**；同样 Clippy/fmt 全 exit 0；`git diff --check` exit 0。
  各 Cargo 命令均 `--locked --offline` 且 `CARGO_NET_OFFLINE=true`，前后输入 SHA-256 一致。
  `codegraph sync` exit 0，首次 status 曾显示并发新增 1 文件，后续 JSON status 明确
  `added=0/modified=0/removed=0`、`state=complete`；完整原始输出已保留。

最终身份：

- 生产 `src/pre/rewrite.rs`：`aa730fb81caa167826de781cc896676e8bd489c225427a76e109a3dd57c2f522`（与原修复一致）。
- 测试 `tests/rewrite.rs`：`5fa6bd02d1998c2dd74b4f8842ba2c10b1adb1437c321a167b6eba0559341a59`。

## 交回与剩余事项

本 worker 交回两份 Rust 文件与 R02b/run-1 的写入所有权。
R02b 实现在 `9602486`，本轮补测尚未提交，整体仍**待未参与实现的 reviewer 独立审查**。
未运行 workspace 全量门禁（统一留 R14），未验证 Windows/Linux/真实 UI；R02 原有
compose 无取消/预算和空 map 边界仍不在本批范围。不得因此宣称 M0/V-01～V-09 通过。
