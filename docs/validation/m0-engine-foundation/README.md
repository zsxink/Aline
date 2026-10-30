# M0 引擎地基验证记录

日期：2026-10-30  
范围：本次 apply 第 1 组（兼容目标与工具链/依赖候选登记）  
状态：1.1～1.4 已完成；见[第 1 组完成与交接记录](./group-1-report.md)及[准入身份登记](./group-1-admission.json)。精确例外按用户继续指示已同步现行规范，主 Agent 与独立 reviewer 复核并修复问题后完成验收。第 2 组正式 workspace 尚未创建，M0 出口和 V 项未通过。

最新证据见第 1 组完成记录、[GPUI 隔离候选审计](./admission-2026-10-30/report.md)、[纯引擎依赖候选](./pure-candidates-2026-10-30/report.md)及[候选守卫](./admission-2026-10-30/guard-summary.md)。[最小例外草案](./admission-2026-10-30/proposed-policy.md)保留审阅时快照，已由现行架构 §3.2 承接。下方工具链/依赖及“本组状态与限制”保留开工初次检查的历史快照，其中“尚未下载/未解析/未完成”不是当前状态；最新结论以前述完成记录和原始日志为准。

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
| `uname -a` | 0 | Darwin XianMac.local 27.0.0，`arm64` |
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

- `find /Users/xian/.cargo/registry/src /Users/xian/.cargo/git/checkouts /Users/xian/.cargo/git/db -maxdepth 5 -iname '*gpui*' -print 2>/dev/null`：未发现 GPUI crate/cache。未从网络下载依赖。
- `/Users/xian/Project/book/zed` 当前 HEAD 为 `a5c4ca8bf6c4717a328a4a61b42d6d1d38949bcc`（2026-10-25，主线提交）；本地状态干净。它不是已证明的 `gpui-pre` 候选，不能用来替代锁定来源。
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
