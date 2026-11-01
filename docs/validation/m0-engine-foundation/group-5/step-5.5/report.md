# 5.5 记录：文档回填与组 5 门禁

日期：2026-10-31。状态：引擎 §9、架构 §5.4、Rust §12、PRD AC-01～04 基础贡献与 V-07 分项已追加记录；组 5 统一工作区门禁 19 项全部退出 0。本文不修改代码、金样、Cargo 或 lint/guard，不宣称 AC-01～04、V-07、M0 出口或现有文件保存通过。独立复核见组级 `review.md`；本报告不代写复核结论。

## 1. 回填结果

| 文档 | 回填位置 | 回填前 SHA-256 | 回填后 SHA-256 | 追加内容 |
|---|---|---|---|---|
| `docs/engine-design.md` | §9.4 后（第 324 行前） | `675670da265f3bf5adc7d897b875fca665a1a97ee69790ede54ae0b990f37349` | `279780bb6f0ef5ff1ef2cd6fff1ea1775846096efc8fc1a4c72f4b59ff1c1c4d` | 组 5 原子批次、正逆补丁、锚点、历史与未实现边界 |
| `docs/v1-architecture.md` | §5.4 末（任务 3.3 记录后） | `9bb8a98e44828e80929a7e7ea59386fa795868a1e28a7fb76b59c358f5c49ef9` | `16eae13eba22cb2ae90d703422f381470c03381a301203a9d8379423d4562f74` | `StringBuffer`/历史原语、保存点身份、重载与 M2 边界 |
| `docs/rust-style-guide.md` | §12.2 后（§13 前） | `49210cc34ca57b186382b8d343f39578b872d331b2ce338cda4358f008dac021` | `6d94f86c948b7b310519d7c30e650f4fa1a8095cae42dc19d6967707f0e46b56` | 测试组织/命名/注入时间/金样/无新依赖的证据 |
| `docs/v1-prd.md` | §9 M0 表后 | `9809b622042928fdc457e4bc865c1d92bb31d5e938ec440a9ebfbeba467848c1` | `ef660179e1eb75d9dc7b6436bd26f26a2fff4a50383f41ae941598db61bffefa` | AC-01～04 的“基础贡献”，四项均未通过 |
| `docs/v1-validation-plan.md` | V-07 M0 编码段后 | `e60a36a7b9fa130fd6e5664d69a3b4d8d33a3adc68449f2a0ef235049d64f0a5` | `239d6ca4b4f7677d1206a9a63ef06c1646c5697b5a4ee3442c2bc3a1f4e6370b` | 组 5 补丁/锚点/历史原语证据；V-07 仍未通过 |

`git diff --stat` 为 5 个文件、10 行新增、0 行删除；`git diff --check` 退出 0。回填只追加带日期的状态与证据段，没有改写既有规范句、AC、V、TA、ED、PA 状态或链接。

## 2. 回填边界

- 引擎 §9 只记录已落地的低层语义与证据位置，明确未实现命令规划、IME、自动保存/外改提示、全文指纹和资源阈值。
- 架构 §5.4 明确保存点身份、保守 dirty、重载只改内存；DocCore/OpenBlock、IME、保存编排尚未实现。
- Rust §12 记录先金样后实现、集成/单元测试位置、注入时间、无真实 sleep、无新依赖；同时记录 `cargo test -p aline-buffer patch` 会命中名称含 `patch` 的其他用例。
- PRD 只写 AC-01～04 的“基础贡献”，四项均未通过；不把无头原语当端到端验收。
- V-07 仍写为未通过；TA-02 的真实自动保存暂停/外改提示尚属 M2，历史原语测试不能当闭环完成。

## 3. 统一工作区门禁

2026-10-31T06:29:09.117462Z 至 06:29:31.775798Z，在仓库根以隔离环境运行：

```sh
CARGO_HOME=<cargo-home> \
RUSTUP_TOOLCHAIN=1.98.1 CARGO_NET_OFFLINE=true \
python3 scripts/check_workspace.py \
  --cargo-home <cargo-home> \
  --output-dir docs/validation/m0-engine-foundation/group-5/step-5.5/workspace-gates
```

19 项全部 exit 0：工具链/工具存在性 4 项、版本 4 项、`fmt-effective-config`、`fmt`（`cargo fmt --all --check`）、`clippy`（`cargo clippy --workspace --all-targets -- -D warnings`）、`test`（`cargo test --workspace`）、`dependency`、`dependency-self-test`、`lint`、`lint-self-test`、`safety`、`safety-self-test`、`public-api`。原始 stdout/stderr、命令 JSON 与 `summary.json` 保存在 `workspace-gates/`，未改写。`cargo test --workspace` 共 76 个测试通过，0 失败。

复核后只改动了审计脚本（`run-audit.sh` 改为 `set -eu`）和文档，未改 Rust 代码、测试或金样。2026-10-31T07:09:40.063117Z 至 07:10:07.141288Z 以同一隔离环境把 19 项门禁重跑到 `workspace-gates-after-review/`，仍全部 exit 0，`cargo test --workspace` 仍为 76 个通过、0 失败。复核遗留项及处置见组级 `follow-ups.md`；该文件不替代独立复核结论。

提交前对暂存内容执行 `git diff --cached --check`：源码、文档与 OpenSpec 路径退出 0；生成日志和依赖夹具因文件末尾空行产生 `new blank line at EOF` 提示，日志按要求保持原样、未做改写。该检查未发现源码或文档的空白错误；提示范围仅限生成证据。

范围：当前 macOS arm64 主机与当前工作树；`--all-targets` 不等于其他 OS/target，不替代 9.1 最终门禁，也不证明 CI 已执行。

## 4. 未执行/未覆盖

- 未运行真实 Windows/Linux、IME、UI、自动保存/外改、磁盘满/断电/权限实验。
- 未实现 AC-01～04 的端到端 UI/保存闭环；PRD 的“基础贡献”不是通过。
- 未做 V-07 的块打开/提交分组、IME、跨块命令、格式化差异确认、只读保护与可视化矩阵。
- 未做组 6～9；未开始 M1+。
- 未修改代码，因此组 5 的代码测试结果不因本次文档回填重新判通过。

## 5. 证据边界

本文与回填段只说明组 5 低层 buffer 原语及当前 macOS 门禁。它们不证明：AC-01～04、V-01～V-09、M0 出口、现有文件保存、自动保存/冲突恢复、IME、UI 集成或跨平台通过。`tasks.md` 勾选不等于出口通过。
