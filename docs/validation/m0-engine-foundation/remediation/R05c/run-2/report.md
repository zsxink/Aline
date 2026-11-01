# R05c：孤立引号策略键与含空格输出目录

本轮仅修改 `scripts/check_lints.py`，新增本目录证据；未修改任务表、progress、R05b review 或 R05c/run-1/pre-fix.txt。对应 M0 工程守卫、tasks.md §4 R05/F-08、R05b 复核 D1/D2、Rust S-2-05/C-01/C-05、CODE_STANDARDS §6.2 与工程治理“工程门禁必须证明实际有效”。不改变 PRD/AC/V 验收口径或 crate/API。

## 根因与最小实现

D1：`_flatten_tables` 仅在两个不同元组路径展平为同一字符串时拒绝。真实 rust 表存在、clippy 只有 `[workspace]` 下 `"lints.clippy"` 时没有第二个 clippy 路径可撞名，守卫误取该字典为策略，而 Cargo 忽略它。rust/clippy 对称形态同样有问题。

沿用现有解析、展平和 `CheckError`，不引入新解析器或抽象。`tables()` 持有本次调用的 `seen`，通用撞名检查完成后，校验两个策略字符串背后的真实路径必须分别为 `('workspace', 'lints', 'rust')` / `('workspace', 'lints', 'clippy')`。否则 `policy-table-path` 拒绝，提供文件、精确行号、策略键、真实与实际完整路径。先执行通用撞名检查，保留两条历史碰撞反例的后出现行号与路径诊断。代价是仅对这两个已登记策略路径增加常数次判断；普通 TOML 引号键和真实点分嵌套键继续接受。

D2：按空格截断 `file:line:` 会截断文件路径。改为用已知源文件的转义字符串匹配 `:<数字>:` 前缀，同时验证文件身份；路径内空格或冒号不会改变分段。路径断言从任意子串提升为完整元组文本匹配，防止把期望路径改写为展平字符串后仍绿。

输入、输出及副作用边界不变：输入清单路径；解析状态仅属于调用栈；返回表或受检拒绝；只有显式 self-test 写隔离夹具。现有解析/继承/深度检查未删减。

## 修前与修后

原始身份见 [isolation.json](isolation.json)，各命令保存同名 `.json` / `.stdout.log` / `.stderr.log`，包括 argv、cwd、UTC 起止、exit 与脚本 SHA256。输入原文见 [fixture-inputs](fixture-inputs)，其余隔离输入指纹见 [input-identities.json](input-identities.json)。旧 `run-1/pre-fix.txt` SHA256 仍为 `38ab98c3dab00b33879e70e348518ce3108469bde6e15f6c68a3109bb11709dc`。

| 场景 | 修前 | 修后 | 证据前缀 |
|---|---:|---:|---|
| 正常八成员仓库 | 0 | 0 | `pre-normal` / `post-normal` |
| 真实 rust + 孤立引号 clippy | 错误 0 | 1，`policy-table-path`，文件第 5 行 | `pre-mixed-clippy` / `post-mixed-clippy` |
| 真实 clippy + 孤立引号 rust | 错误 0 | 1，`policy-table-path`，文件第 12 行 | `pre-mixed-rust` / `post-mixed-rust` |
| 两张策略都只有引号键 | 错误 0 | 1，`policy-table-path` | `pre-quoted-only` / `post-quoted-only` |
| 完整 `--self-test`，目录含空格 | 1，错误地称缺少文件:行号 | 0 | `pre-self-test-space` / `post-self-test-space` |
| 表路径自测，目录含空格和 `:7:` | 未执行 | 0 | `table-checks-space-colon` |

修后完整 self-test：10 类 Rust 样例 × debug/release；八成员 unsafe 继承；inheritance 4、parser 25、table_path 12、member 5。原有正反例全部保留。表路径新增：真实表附无关 `[package] "a.b"`、真实点分嵌套值两个正例；四种 rust/clippy 对称与前后顺序混合反例、纯引号策略、根引号策略六个反例。每个拒绝均断言准确文件、行号、完整路径和键。内部原始结果与夹具已复制到 `pre-self-test/`、`post-self-test/`，未复制 Cargo 构建产物。

## Cargo 真实语义对照

macOS ARM64；Python 3.9.6；Cargo/Rust 1.98.1；`CARGO_HOME=<cargo-home>`，`CARGO_NET_OFFLINE=true`，`RUSTUP_TOOLCHAIN=1.98.1`。新夹具先独立执行 `cargo generate-lockfile --offline`（`lock-*.json`），随后本轮直接 Cargo check/clippy 全部显式 `--locked --offline`。最终采信带 `fresh-` 前缀的八条 `-vv` 记录，每条单独使用空 target 目录。

| 根策略 / 源码 | check | clippy `-D warnings` | `-vv` 关键证据 |
|---|---:|---:|---|
| 真实 rust + 引号 clippy / unwrap | 0 | 0 | `unused manifest key: workspace.lints.clippy`，driver 无 clippy deny |
| 真实 clippy + 引号 rust / unsafe | 0 | 0 | `unused manifest key: workspace.lints.rust`，driver 无 unsafe forbid |
| 两张真实策略 / 同一 unwrap 源码 | 0 | 101 | driver 有 `--deny=clippy::unwrap_used`，明确 unwrap 诊断 |
| 两张真实策略 / 同一 unsafe 源码 | 101 | 101 | driver 有 `--forbid=unsafe_code`，明确 unsafe block 诊断 |

纯引号策略 Cargo exit 101，明确 `workspace.lints` 未定义（`cargo-quoted-only`）。混合形态的静默成功不能用纯引号形态的 Cargo 失败兜底。

初次若干直接 Cargo 控制共用 `cargo-target`。随后从 mixed-rust 复制出真实 rust 控制时，`cargo-check-real-rust-control` 实得 0，与手写预期 101 不符；stdout/stderr/metadata 原样保留，未把它算通过。独立 target 重跑 `cargo-check-real-rust-control-fresh` 得到 101；再以每条独立 target + `-vv` 重跑全部八条得到上表。初次共用缓存结果不作为策略生效依据；本轮未调查 Cargo 的缓存失效机制。早期 target 环境补记见 [evidence-environment.json](evidence-environment.json)。

## 断言有效性与适用检查

[mutation-summary.json](mutation-summary.json) 的 9 个单点变异全部 exit 1，命中指定 case/具体原因，均无 traceback：删除孤立策略拒绝；重新截断空格路径；错误文件；错误行号；错误实际路径；错误展平键；期望路径改为展平字符串；成员 True 判定放宽；删除通用碰撞拒绝。变异副本和输出仅写新建临时目录，未覆盖生产脚本。

Python AST 与 `python -m py_compile`、正常仓库检查、完整 self-test、`cargo fmt --all --check`、`git diff --check` 通过。`codegraph sync` / `status` exit 0，记录时索引 up to date。生产 diff 保存于 `production-diff.stdout.log`。本轮未改 Rust，未重复运行整个 workspace clippy/test；完整门禁由 R14 在最终合并状态统一执行。

## 边界与交接

D1/D2 已由上述证据关闭，尚需主 Agent 核对与独立审查；本报告不代替审查结论。生产脚本所有权交回主 Agent。

未扩大范围：仍是自带 TOML 子集解析器；深度上限仍 100，30–100 层不因本轮变成拒绝；未新增 RecursionError 兜底 self-test。既有 self-test 内置 Cargo argv 仍仅显式 `--offline`，会在全新夹具生成 lock；本轮新增直接语义对照均锁定离线，该区别如实保留。未进行 Windows/Linux、完整语料或原生 UI 验证，不代表 M0/V 出口通过。
