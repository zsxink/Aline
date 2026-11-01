# M0 引擎地基验证记录

日期：2026-10-31

范围：`m0-engine-foundation` 第 1～9 组的证据入口；具体结论以各原始日志和独立复核为准。

路径约定：证据中的本机路径统一写作 `<repo>`、`<tmp>`、`<cargo-home>` 等占位符（生成与还原规则见 `scripts/evidence_paths.py`），不记录具体机器位置；历史脚本复跑前须按当前机器还原这些占位符。

原始证据分发说明（2026-11-01）：本目录只保留 `.md` 报告、复核结论与交接记录；各批次的 `command.json`、stdout/stderr 日志及原始 `.rs`/`.json`/`.tsv` 副本不随仓库分发。报告中的相对链接因此可能指向已移除的本地文件，这是预期状态，不代表结论失效。

可复现输入已迁出到仓库资产：Rust 金样与夹具在 `crates/*/tests/fixtures/`，门禁夹具、隔离配置与工程基线在 `scripts/fixtures/`，门禁实现在 `scripts/check_*.py`。复跑时按报告记录的 argv/cwd 执行，输出写到 `logs/validation/`（已被 `.gitignore` 排除）；未入库的历史原始证据另存本地备份，不随仓库分发，已提交部分仍可从提交历史追溯。

当前入口：第 1～8 组已有分项证据，第 9 组证据包与回填已完成独立核对。任务勾选不代替出口验收，完整 M0 出口和 V-01～V-09 均不据此宣称通过。下方早期环境、依赖及“本组状态与限制”是第 1～2 组开工时的历史快照，其“尚未”表述不是当前状态。

| 组 | 证据入口与范围 |
|---|---|
| 1–2 | [候选准入与交接](./group-1-report.md)、准入身份（原 `group-1-admission.json`，未随仓库分发）、[工程门禁与 catalog](./group-2/report.md) |
| 3 | [公共接口与身份基础](./group-3/step-3.3/report.md) |
| 4 | [source 编码与回填](./group-4/step-4.5/report.md) |
| 5 | [buffer 与历史回填](./group-5/step-5.5/report.md) |
| 6 | [解析回填](./group-6/step-6.8/report.md)、[独立复判](./group-6/review-round-4.md) |
| 7 | [编辑协议回填](./group-7/step-7.4/report.md)、[独立复判](./group-7/review-round-4.md) |
| 8 | [首次复核](./group-8/review-round-1.md)、[补充实现](./group-8/followup-2026-10-31/report.md)、[复判](./group-8/review-round-3.md)、[8.5 回填](./group-8/step-8.5/report.md) |
| 9 | [workspace 门禁](./group-9/step-9.1/report.md)、[E1](./group-9/step-9.2/report.md)、[E2/E3/E5](./group-9/step-9.3/report.md)、[E4/E7](./group-9/step-9.4/report.md)、[E6 与 M1 交接](./group-9/step-9.5/report.md)、[独立审查及回填](./group-9/step-9.6/report.md) |


第 2 组最新证据见[工程门禁及 catalog](./group-2/report.md)，源码行尾定义已获用户确认，见[同步记录](./source-representation-decision.md)；第 1 组证据见第 1 组完成记录、[GPUI 隔离候选审计](./admission-2026-10-30/report.md)、[纯引擎依赖候选](./pure-candidates-2026-10-30/report.md)及[候选守卫](./admission-2026-10-30/guard-summary.md)。[最小例外草案](./admission-2026-10-30/proposed-policy.md)保留审阅时快照，已由现行架构 §3.2 承接。下方工具链/依赖及“本组状态与限制”保留开工初次检查的历史快照，其中“尚未下载/未解析/未完成”不是当前状态；最新结论以前述完成记录和原始日志为准。

## 环境与兼容目标（1.1）

**实际观察**

- OS/CPU：macOS 27.0.1 (26A434)，Darwin 27.0.0，`aarch64-apple-darwin`。
- 设备：MacBook Pro，Apple M5，10 核 CPU，32 GB 内存。
- 显示：内建 Liquid Retina XDR，3024×1964；一台外接显示器，4096×2304（UI 缩放 2048×1152，75 Hz）。
- 设备序列号、UDID 等标识未记录。
- 本机已安装 Rust target：`aarch64-apple-darwin`、`aarch64-unknown-linux-gnu`、`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`。安装 target 只说明工具组件存在，不代表交叉构建或平台验证通过。

**人工目标登记**

- 先 macOS，再 Windows/Linux；正式 V1 三平台同发。保留 Intel Mac 与 Windows 10。
- macOS：Apple Silicon `aarch64`、Intel `x86_64`；最低系统版本未定。仅当前 Apple Silicon 主机可用，Intel Mac 缺设备。
- Windows 10：`x86_64` 为候选，最低版本/build 与 ARM64 范围未定；无 Windows 取证设备。
- Linux：`x86_64`、`aarch64` 为候选，发行版/最低版本未定；X11、Wayland 为显示候选；无 Linux 取证设备或 VM。
- 上述候选没有“已支持”含义。安装、签名、系统菜单、文件关联、重复进程转交、中文输入等 V-01 项均未执行。

**命令与结果（实际）**

| 命令 | 退出码 | 关键输出 |
|---|---:|---|
| `sw_vers` | 0 | macOS 27.0.1，build 26A434 |
| `uname -a` | 0 | Darwin [REDACTED] 27.0.0，`arm64` |
| `system_profiler SPHardwareDataType` | 0 | MacBook Pro / Apple M5 / 10 cores / 32 GB；敏感设备标识未抄录 |
| `system_profiler SPDisplaysDataType` | 0 | 内建 Liquid Retina XDR 3024×1964；外接显示器 4096×2304，UI Looks like 2048×1152 @ 75 Hz |
| `rustup show` | 0 | 默认 stable；活动工具链 `stable-aarch64-apple-darwin`；另装 `1.98.1-aarch64-apple-darwin`；列出的四个 target 如上 |

目标表已回填至 [V-01](../../v1-validation-plan.md#v-01-三平台兼容与发布矩阵)。此表是 M0 兼容目标登记，不是 V-01 完成报告。

## 工具链与候选依赖（1.2–1.3）

**本机工具版本（实际观察）**

| 命令 | 退出码 | 关键输出 |
|---|---:|---|
| `rustc -Vv` | 0 | rustc 1.98.1 (`48a229cea`, 2026-09-01)，host `aarch64-apple-darwin`，LLVM 22.1.8 |
| `cargo -V` | 0 | cargo 1.98.1 (`797e8a9bc`, 2026-08-05) |
| `cargo fmt --version` | 0 | rustfmt 1.9.0-stable (`48a229cea`) |
| `cargo clippy --version` | 0 | clippy 0.1.98 (`48a229cea`) |
| `rustup component list --installed` | 0 | 当前 host 的 cargo、clippy、rust-docs、rust-std、rustc、rustfmt 组件已安装 |

仓库当前没有 `Cargo.toml`、Cargo workspace 或 `Cargo.lock`，因此尚不能从实际工程运行 `cargo metadata`、`cargo tree` 或候选 crate 构建。edition 2024 是 design 中的候选，未通过隔离候选工程验证；未声明 MSRV，也未做最低版本测试。

GPUI 候选检查（只读本地检查）：

- `find <cargo-home>/registry/src <cargo-home>/git/checkouts <cargo-home>/git/db -maxdepth 5 -iname '*gpui*' -print 2>/dev/null`：未发现 GPUI crate/cache。未从网络下载依赖。
- `<external>/zed` 指仓库外的参考克隆（不在本仓库内，也不随本仓库分发）当前 HEAD 为 `a5c4ca8bf6c4717a328a4a61b42d6d1d38949bcc`（2026-10-25，主线提交）；本地状态干净。它不是已证明的 `gpui-pre` 候选，不能用来替代锁定来源。
- 本地 registry/git cache 未发现 GPUI。Zed checkout `a5c4ca8bf6c4717a328a4a61b42d6d1d38949bcc` 是主线，不是可直接认作 `gpui-pre` 的锁定来源。
- 只读 GitHub 检索发现 `gpui-pre` 0.3.9 的镜像 manifest 声明 Zed 源快照 `93d745668805029476e6d7fbb19652116fed51de`、Apache-2.0、edition 2024。更关键的是，直接读取该快照对应的 [Zed 官方 `crates/gpui/Cargo.toml`](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui/Cargo.toml)，其中列有非 optional `regex = { workspace = true, default-features = true }`。按当前 `engineering-gates` change 的全闭包禁用要求，GPUI 依赖方向 `app/aline-ui/aline-plugin → gpui-pre 0.3.9 (gpui) → regex` 会违反准入条件。镜像的[规范化 manifest](https://github.com/wormhole-terminal/gpui-pre/blob/main/Cargo.toml)也显示此依赖。该源码证据是对官方 Zed 快照的只读审查，不等于 Cargo 发布包校验/构建证据。
- 另检查 `gpui-pre` 0.3.6 所声明的官方 Zed 快照 `bcf6582ce3500df93a8a39366640173e6786cea6`：[官方 GPUI manifest](https://github.com/zed-industries/zed/blob/bcf6582ce3500df93a8a39366640173e6786cea6/crates/gpui/Cargo.toml)同样列出 `regex.workspace = true`。因此已检查的两个历史候选版本均无法满足当前全闭包禁令；这不证明所有版本都不合规，但没有找到任何合规候选。
- 尚未取得 crates.io 发布包/校验和并与源快照核对，也未检查完整依赖闭包、补丁身份或构建日志；五引擎闭包未解析。不能将 0.3.9 判为准入通过，也不能仅凭这一个候选宣告不存在其它合规候选。若选择保留禁令，候选必须更换为无此路径的合规版本；不能以历史版本号推断合规。
- `web` 搜索服务查询连接失败；GitHub 只读接口随后成功返回仓库/manifest 内容。未下载任何 crate 或工具链。

**当前依赖账本状态**

| 依赖/范围 | 现有规范中的状态 | 实际准入证据 |
|---|---|---|
| GPUI | 单一精确 `gpui-pre` 来源为硬约束；历史 `=0.3.9` 仅为核对线索 | 候选源码/补丁与完整闭包未取得；阻塞 |
| pulldown-cmark | 唯一允许的 Markdown 解析器；精确版本及 features 待候选取证 | Cargo workspace 尚不存在，未解析/未测试 |
| TOML、rust-embed | catalog 候选，版本/许可/features/闭包须逐项登记 | 未选精确版本，未解析/未测试 |
| blake3、directories、rfd、similar、proptest、Unicode/公式/图表等候选 | 仅在实际消费者与准入核实后考虑；无消费者不得预装 | 未准入；不代表列入 M0 依赖 |
| notify、regex、tokio、Wasmtime/WASI、第二 Markdown 解析器 | 禁止项 | 尚无 workspace 图，别名/传递/目标/features 负例守卫未实现，禁止规则尚未被自动验证 |

`cargo metadata --format-version 1`、`cargo tree --target all -e all`、目标/features 分组合解析、别名/目标负例均**未执行**。解析失败或缺少 manifest 不能记为合规。没有实际包版本、来源、许可证或依赖路径证据，不补写推测值。

## 本组状态与限制

- 1.1：实际主机信息采集及人工目标/设备缺口登记完成；验证顺序与平台支持声明边界已核对。
- 1.2：仅记录已安装工具版本及公开 manifest 候选线索。edition/MSRV/GPUI 组合隔离构建、发布包/源码/补丁身份与最小不兼容配置均未验证。
- 1.3：依赖准入账本已记录当前知识与缺证状态；实际 metadata/tree、闭包和正反例均未执行。
- 1.4：依赖路径人工审查发现 `gpui-pre 0.3.6` 和 `0.3.9` 所引用的官方 Zed 源快照都含非 optional `regex` 依赖，故这两个候选不能满足当前 change 的全闭包禁令。尚未证明其它替代版本合规，未作依赖接线。
- 第 2 组被阻塞。推荐先经用户确认修改本 change 的候选准入范围：只允许经逐版本审查的 GPUI 自身传递 `regex` 出现在 `app/aline-ui/aline-plugin` 闭包；Aline 自有直接依赖仍禁止 `regex`，五引擎闭包仍必须无 GPUI 且无禁用依赖。此改动会放宽当前 delta 的全闭包禁令，必须先更新 proposal/specs/design/tasks 并取得确认。若维持严格全闭包禁令，则继续寻找无该路径的精确 GPUI 候选；找不到合规候选时停止第 2 组接线。

V-01～V-09 均未标记通过。本记录所列实测只覆盖当前 macOS 主机环境；Windows/Linux、Intel Mac、GPUI 候选构建和完整工程门禁均未验证。

## 勘误与新证据（2026-11-01 追加，不改上文原文）

M0 整改 R11 补采了当前可保全的证据，见
[remediation/R11/run-1/report.md](./remediation/R11/run-1/report.md)、
[legacy-gaps.md](./remediation/R11/run-1/legacy-gaps.md)、
[unverified.md](./remediation/R11/run-1/unverified.md)。

上文「实际观察」与「命令与结果」两表保留为**当时快照**，与当前观察存在四项差异：

1. OS：当时 macOS 27.0.1 (26A434)、Darwin 27.0.0；现为 **macOS 27.2 (26B123)、Darwin 27.2.0**。
2. 活动工具链：当时记录 `stable-aarch64-apple-darwin` 为活动；现为 **`1.98.1-aarch64-apple-darwin`**
   （`RUSTUP_TOOLCHAIN=1.98.1` 覆盖所致）。
3. target 集合：上文列出的四项属于 **stable** 工具链；当前活动工具链 1.98.1 的已装 target 为
   `aarch64-apple-darwin`、`x86_64-apple-darwin`、`x86_64-pc-windows-gnu`、`x86_64-unknown-linux-gnu`。
   上文未标明 target 所属工具链，属表述缺口。
4. 显示：当时记录一台外接显示器 4096×2304（2048×1152@75Hz）；本轮采集时**外接未接入**，
   仅有内建 Liquid Retina XDR 3024×1964。

旧 `*.command.json` 的缺字段情况（14 份缺 cwd/时点、环境命令无 cwd 与时点、设备观察无原始输出
与时点）已登记为历史事实，**未回填、未补造**，也不因本轮新证据视为已修复。

另：上文「命令与结果」表的 `uname -a` 行含当时的主机名（其中带账号名）。按 R11 的打码策略，
本轮新采集已将该主机名打码为 `[REDACTED]`；该历史行按「证据只追加」规则**未改写**，
但它是已知的敏感标识残留，须在 R14 统一裁决是否就地打码。

