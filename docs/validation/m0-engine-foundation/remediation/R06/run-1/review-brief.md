# R06 独立复核任务书

你**没有**参与 R06 的实现。目标是独立判定 F-09 / F-15 是否真被修复、修复是否引入
回归或放宽。**不得**采信 `report.md` / `handoff.md` 的结论，它们只是被审材料。

## 被审对象

- 提交：`60ace78`（父 `340152c`）
- 代码：`scripts/check_safety.py`（+321/−122）；`scripts/safety_lex.py` **未改**
- 证据目录：`docs/validation/m0-engine-foundation/remediation/R06/run-1/`
  （`brief.md`、`pre-fix.txt`、`post-fix.txt`、`commands.txt`、`handoff.md`、`report.md`）
- 环境：`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`；Cargo 调用须 `--locked --offline`

## 必须独立回答的问题（逐条给出结论 + 你自己跑出来的证据）

1. **F-09 是否真修复**：**自建**一个八成员形状的夹具（不要复用 `<tmp>/aline-m0-independent-review.*`、
   不要复用 `<tmp>/r06*`），在 `crates/aline-buffer/Cargo.toml` 写
   `[lib] path = "../../outside.rs"`，`outside.rs` 内放 `v.unwrap()`。
   用 `git show 60ace78^:scripts/check_safety.py` 导出旧脚本（**同时导出 `safety_lex.py` 到同一目录**），
   证明旧脚本 exit 0 且 `failures=[]`；新脚本 exit 1 且规则为 `external-target-path`。
   再自建一个目录外 `build = "../../outside-build.rs"` 的夹具重复一遍。

2. **F-15 是否真修复**：`--root` 指向不存在路径、指向无 `Cargo.toml` 的目录、
   指向成员清单 —— 三种都必须 exit 非 0。`members = []` 的空 workspace 必须报 `empty-scan`。

3. **是否放宽既有断言**：`wrong-package-in-known-ui-path` 与
   `quoted-package-and-description-decoy` 的期望从 `['member-package-identity']`
   改为 `['member-package-identity','missing-member']`。请**独立复算**该夹具下
   `missing-member` 是否必然出现（一个 workspace 内包名 `aline-source` 只能有一份），
   判定这是「加强」还是「放宽」。同时核对 case 总数：`60ace78^` 的脚本自测应得 53 项，
   `60ace78` 应得 60 项，**且既有 53 项的 name/expectation 一条未删**（用
   `git diff 60ace78^ 60ace78 -- scripts/check_safety.py` 逐条核对，不要只看总数）。

4. **扫描范围无缩小**：对真实仓库分别用旧脚本与新脚本跑 `--root .`，
   比较 `scanned` 的 `(file, kind)` 集合，差集必须双向为空；`failures` 必须都为 0。

5. **断言有效性（变异）**：自己设计 **≥3 处**变异（不得照抄 `report.md` §3 的四处），
   每处都必须让 `--self-test` exit 非 0 并命中你预期的 case。注意：脚本按
   `Path(__file__).resolve().parent.parent` 推导 ROOT，变异副本必须放进仓库
   `scripts/` 下（如 `scripts/_revN.py`），跑完立即删除；**不得留在工作树**。

6. **fail-closed 是否真的闭环**：读代码确认 `workspace_scan` 的所有失败路径都
   返回非零退出码，没有被 `except (OSError, ValueError, KeyError)` 吞掉后仍然 exit 0 的分支。
   特别核对 `cargo metadata` 失败、JSON 非法、`targets` 为空这三种。

7. **未覆盖范围是否诚实**：`report.md` §5 列了 6 条限制（只拒绝不受审、依赖树/过程宏/
   `include!` 不在范围、不向上找根、`build.rs` 纳入生产扫描、`--no-deps` 依赖、
   零 Rust 改动未跑 cargo test）。逐条判断是否属实，有无**未声明的**新盲区。

8. **自测不再污染仓库**：确认 `--self-test` 在无 `--output-dir` 时也不写
   `docs/validation/m0-engine-foundation/group-2/safety-fixtures/`（夹具改走 `tempfile.mkdtemp`）。
   若仍会写受跟踪路径，这是回归。

## 输出

只写 `docs/validation/m0-engine-foundation/remediation/R06/run-1/review.md`
（若已存在则写 `review-run2.md`）。≤120 行，含 A–F 分节：
A 结论（`APPROVE` / `APPROVE_WITH_CONCERNS` / `REJECT` + 一句话理由）、
B 逐问题独立结论与自建证据、C 变异记录（变异内容 → 变红 case）、
D 发现的缺陷（含严重度，无则写「无」）、E 残留与需主 Agent 裁决项、F 未覆盖范围复核。

**约束**：不 commit、不改任何被审代码或他人文件、不派子代理、
临时文件放 `/tmp` 并自行清理。返回正文 ≤800 中文字符，只报结论与关键证据。
