# 6.8 记录：文档回填、基础夹具与工作区门禁

日期：2026-10-31。状态：引擎 §6/§7/§11/§12 与解析配置记录已追加；V-06/V-07 基础夹具段落已追加；组 6 统一工作区门禁 19 项全部退出 0。本文只记录回填与门禁，不宣称 AC-09、V-06、V-07 或 M0 出口通过。

## 1. 回填结果

回填前 SHA-256（2026-10-31，回填开始前记录）：

| 文档 | 回填前 SHA-256 | 回填位置 | 追加内容 |
|---|---|---|---|
| `docs/engine-design.md` | `279780bb6f0ef5ff1ef2cd6fff1ea1775846096efc8fc1a4c72f4b59ff1c1c4d` | §6.2、§7.1、§11.2、§12.1、§12.3、§11.2 修复记录 | 解析配置依据、映射/保护基础、有限 HTML 实验与修复、复用实验、预算与敌意输入 |
| `docs/v1-validation-plan.md` | `239d6ca4b4f7677d1206a9a63ef06c1646c5697b5a4ee3442c2bc3a1f4e6370b` | V-06 M0 编码段后、V-07 组 5 段后 | 解析预算/敌意输入基础证据；解析/映射/保护基础夹具证据与未冻结项 |
| `docs/v1-prd.md` | `ef660179e1eb75d9dc7b6436bd26f26a2fff4a50383f41ae941598db61bffefa` | §9 M0 表后（组 5 记录后） | 组 6 对 AC-09 的基础贡献，明确未通过 |
| `docs/v1-architecture.md` | `16eae13eba22cb2ae90d703422f381470c03381a301203a9d8379423d4562f74` | §7.1 表后 | `parse`/`map_position` 两行的实际落地补充 |
| `docs/rust-style-guide.md` | `6d94f86c948b7b310519d7c30e650f4fa1a8095cae42dc19d6967707f0e46b56` | 未回填 | 本组无新增测试组织规范需要记录 |

回填后 SHA-256（工作区实际文件，证据记录见 `step-6.8/backfill-hashes.log`）：

| 文档 | 回填后 SHA-256 |
|---|---|
| `docs/engine-design.md` | `e60101de16e81ed0e2dd747faaaf8bc99634efa835e1eb89446d8b6f99057627` |
| `docs/v1-validation-plan.md` | `5ad602af9c70c5dcd0766dc79f3fc480e9fd34975cad426480a726cafaaaa09b` |
| `docs/v1-prd.md` | `47962a6fff1051c61fb00f0c6618e5c205c4ab6da397856406949154d5e08e34` |
| `docs/v1-architecture.md` | `0e94cca4cf19a19f4a3643ff88e820b42a7aeefa4940c659c3adafd503895637` |
| `docs/rust-style-guide.md` | `6d94f86c948b7b310519d7c30e650f4fa1a8095cae42dc19d6967707f0e46b56`（未改） |

`git diff --stat docs/` 为 4 个文件、22 行新增、0 行删除；`git diff --check` 退出 0。回填只追加带日期的状态与证据段，未改写既有规范句、AC、V、TA、ED、PA 状态或链接。

## 2. 回填边界

- 引擎 §6.2 记录 `ParseConfig`/`ParseFeatures` 与六个显式位（`0x00000c1e`、代次 1），并说明 Front matter 由前置改写识别、未启用原生 metadata、smart punctuation 与 heading attributes 关闭；不改上方“建议配置”措辞。
- 引擎 §7.1 记录片段关系、`source_at`/`drawn_at` 的 affinity 规则、手写金样位置，并写明 SoftBreak 口径待 V-07 冻结。
- 引擎 §11.2 记录已实现/已测试项与尚未冻结项；修复记录另列已落地项，仍不宣称“有限 HTML 已完成”。
- 引擎 §12.1 记录复用命中与拒绝条件，并写明未实现局部解析优化、未测性能收益。
- 引擎 §12.3 记录八维预算、协作式取消、实测扫描步数 80、`pulldown-cmark` 长调用盲区与 `RawFallback` 不可达；明确 `ParseBudget::m0()` 数值不是 V-06 发布限额。
- V-06/V-07 段落逐项列实验证据与未覆盖项，保留“V-06 仍未通过”“V-07 仍未通过”。
- PRD 只写组 6 对 AC-09 的“基础贡献”，明确 AC-09 未通过。
- 架构 §7.1 只更新两行的实际落地，其余接口仍为“仅契约未验证”。

## 3. 统一工作区门禁

2026-10-31T09:42:36.350886Z 至 09:43:04.038768Z，在仓库根以隔离环境运行：

```sh
CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home \
RUSTUP_TOOLCHAIN=1.98.1 CARGO_NET_OFFLINE=true \
python3 scripts/check_workspace.py \
  --cargo-home /Users/xian/Library/Caches/aline-m0/cargo-home \
  --output-dir docs/validation/m0-engine-foundation/group-6/step-6.8/workspace-gates-post-fix
```

19 项全部 exit 0：工具链/工具存在性 4 项、版本 4 项、`fmt-effective-config`、`fmt`、`clippy`、`test`、`dependency`、`dependency-self-test`、`lint`、`lint-self-test`、`safety`、`safety-self-test`、`public-api`。原始 stdout/stderr、命令 JSON 与 `summary.json` 保存在 `workspace-gates-post-fix/`，未改写。`cargo test --workspace` 共 187 个测试通过、0 失败。

修复前另有一次门禁运行（2026-10-31T08:43:35.845279Z 起，19 项 exit 0、`cargo test --workspace` 134 个测试）保存在 `workspace-gates/`。两次都保留，不用后一次覆盖前一次。

## 4. 定向与全 crate 证据

修复后（2026-10-31T09:42:10Z–09:42:11Z，`commands-post-fix.log`，全部 exit 0）：

| 目标 | 用例数 | 日志 |
|---|---|---|
| `spec` | 10 | `targeted-tests-post-fix/spec.stdout.log` |
| `rewrite` | 9 | `targeted-tests-post-fix/rewrite.stdout.log` |
| `mapping` | 16 | `targeted-tests-post-fix/mapping.stdout.log` |
| `html` | 20 | `targeted-tests-post-fix/html.stdout.log` |
| `protection` | 6 | `targeted-tests-post-fix/protection.stdout.log` |
| `hostile` | 15 | `targeted-tests-post-fix/hostile.stdout.log` |
| `reuse` | 13 | `targeted-tests-post-fix/reuse.stdout.log` |
| `diagnostics` | 5 | `targeted-tests-post-fix/diagnostics.stdout.log` |
| 全 crate | 98 | `targeted-tests-post-fix/full-crate.stdout.log` |

修复前（2026-10-31T08:48:11Z–08:48:13Z，`commands.log`，全部 exit 0）的对应数字为 spec 8、rewrite 6、mapping 10、html 11、protection 6、hostile 10、reuse 7、全 crate 62。两组证据都保留，用于说明修复前后的差异。

上表是 09:42 时点的快照。round-2 复核处置后（10:19Z–10:34Z）的最终计数为 html 21（N-F3-2 现状用例）、hostile 19（N-F2-2/N-F2-3）、reuse 18（N-F2-1）、全 crate 108，工作区 `cargo test --workspace` 为 204 passed；证据在 `step-6.5`、`step-6.6/fix-n-f2-2-n-f2-3/`、`step-6.7/fix-n-f2-1/` 与 `group-6/leader-verify-round-2/`，本表数字不改写。

金样独立校验：`python3 -I verify_goldens.py` 输出“6 个夹具、38 个区间、0 个问题；所有区间升序、无缝覆盖、端点合法、块尾无分隔换行、间隙只含换行”（`goldens-independent.stdout.log`）。

## 5. 环境与平台

逐变量 `printenv` 证据（`step-6.8/environment-per-variable.stdout.log`，2026-10-31T09:23:35Z）显示 `CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`；`uname -a` 为 Darwin 27.2.0 arm64，`rustc` 为 1.98.1 aarch64-apple-darwin。

说明：首次尝试在同一脚本中用 `printenv CARGO_HOME RUSTUP_TOOLCHAIN CARGO_NET_OFFLINE` 时三变量都显示“未设置”，因为该命令在新 shell 中先于 `export` 执行；随后在同一日志中以显式 `export` 再跑一遍，得到上述真实值。这次修正保留在同一条证据日志中，未删除首次输出。

**未覆盖**：Windows/Linux、其他 target、CI；门禁只在当前 macOS arm64 上运行。

## 6. 未完成与已知缺口

- 非过滤集 void 标签（`input/link/meta/base`）展示口径待 V-07 冻结。
- SoftBreak 投影口径待 V-07 冻结。
- `RawFallback` 在公开 `parse` 入口无触达路径；预算注入的实际诊断是 `BudgetExceeded`。
- `pulldown-cmark` 内部长调用不可中断。
- 未实现局部解析优化、未测性能、未验证真实 UI 命中/IME/灰色只读呈现。
- 本文不宣称 AC-09、V-06、V-07 或 M0 出口通过；独立复核见组级 `review.md` 与 `review-round-2.md`，本报告不代写复核结论。
