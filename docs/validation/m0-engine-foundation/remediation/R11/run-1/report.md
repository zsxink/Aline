# R11 报告：F-13 候选/工具/设备采集补证

批次：R11（F-13，严重，维度 D2）
运行：`run-1`
基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`
执行时 HEAD：`e2b12bc7496365aba3e3950cea9b0a054ffa7451`
执行方式：主 Agent 直接执行（只读取证，不派 worker）
本批**不改任何代码**，只补证据并登记历史缺口。

## 1. 盘点方法

1. 用脚本递归审计 `docs/validation/m0-engine-foundation/admission-2026-10-30/**/*.command.json`，
   共 **105** 份，逐份检查字段集 `{argv, cwd, started_utc, finished_utc, exit_code, id}`。
   结果：**14 份缺字段**（4 份 `safety-*` 缺 `cwd` + 时点；10 份 `tree-*` 缺时点）。
   逐份原值落 `legacy-gaps.json`，人读镜像落 `legacy-gaps.md`。
2. 新采集用 `collect.py`（stdlib only，`python3 -I` 运行）：每条命令包一层，
   写 `<label>.command.json`（`id`/`argv`/`cwd`/`env_overrides`/`started_utc`/`finished_utc`/`exit_code`）
   与 `<label>.stdout.log`、`<label>.stderr.log` 原始输出，全部落 `commands/`。
   `env_overrides` 固定为 `CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。
3. 本轮**不**运行 build / clippy / test（属 R14 统一门禁），不联网、不 `cargo update`、
   不 `rustup update`、不 `rustup component add`。因此没有任何 Cargo 构建日志进入本批证据。

## 2. 新采集清单（17 条命令，全部 exit 0）

| label | 命令 | 用途 |
|---|---|---|
| `toolchain-rustc-Vv` | `rustc -Vv` | 编译器身份 |
| `toolchain-cargo-V` | `cargo -V` | cargo 身份 |
| `toolchain-cargo-fmt-version` | `cargo fmt --version` | rustfmt 身份 |
| `toolchain-cargo-clippy-version` | `cargo clippy --version` | clippy 身份 |
| `toolchain-rustup-show` | `rustup show` | 活动工具链 + target |
| `toolchain-rustup-toolchain-list` | `rustup toolchain list` | 已装工具链 |
| `toolchain-1.98.1-targets-installed` | `rustup target list --installed --toolchain 1.98.1-aarch64-apple-darwin` | 活动工具链 target |
| `toolchain-stable-targets-installed` | `rustup target list --installed --toolchain stable-aarch64-apple-darwin` | stable target（对照） |
| `toolchain-python3-version` | `python3 --version` | 脚本运行时 |
| `device-sw-vers` | `sw_vers` | OS 版本 |
| `device-uname-a` | `uname -a` | 内核/架构 |
| `device-system-profiler-hardware` | `system_profiler SPHardwareDataType` | 机型/芯片/内存（已打码） |
| `device-system-profiler-displays` | `system_profiler SPDisplaysDataType` | 显示（已打码主机名） |
| `disk-df-repo` | `df -h <repo>` | 磁盘余量 |
| `isolation-cargo-home-exists` | `test -d <cargo_home>` | 隔离目录是否仍在 |
| `isolation-cargo-home-ls` | `ls -la <candidate_root>` | 隔离目录内容 |
| `isolation-read-isolation-json` | `cat admission-2026-10-30/isolation.json` | 隔离记录原文 |

采集窗口：`2026-11-01T04:32:49Z` ～ `2026-11-01T04:36:48Z`（UTC）。
命令记录数为 17；`commands/` 共 51 个文件（17 × 3）。

## 3. 打码策略（实际执行）

`device-system-profiler-hardware.stdout.log` 与 `device-uname-a.stdout.log` 就地打码：

| 字段 | 处理 |
|---|---|
| `Serial Number (system)` | → `[REDACTED]` |
| `Hardware UUID` | → `[REDACTED]` |
| `Provisioning UDID` | → `[REDACTED]` |
| `uname -a` 的主机名（含账号名） | → `[REDACTED]` |
| `Model Name` / `Model Identifier` / `Chip` / 核数 / 内存 / 固件版本 / 分辨率 / 显示器类型 / Metal 版本 / Vendor | **保留** |

保留字段的理由：它们是 1.1 兼容目标表与 V-01 目标栏需要的事实，且不构成设备唯一标识。
打码是**原地替换**（同一 label 下不保留未打码副本），故本目录不存在可复原敏感标识的文件。
打码在采集后、落盘前完成，`collect.py` 复跑时用 `label` 过滤，不会覆写已打码日志。

**已发现但本批未改的残留**：`docs/validation/m0-engine-foundation/README.md` 第 46 行
「命令与结果」表的 `uname -a` 行含当时的主机名（带账号名）。按「证据只追加」规则本批不改写
历史行，只在 README 末尾追加勘误说明，是否就地打码留 R14 裁决。

## 4. 新采集与旧记录的对照

| 项目 | 旧记录（`admission-2026-10-30` / README 表） | 本轮新采集 | 结论 |
|---|---|---|---|
| `rustc -Vv` | 1.98.1 `48a229cea`，host aarch64-apple-darwin，LLVM 22.1.8 | 完全一致 | 一致 |
| `cargo -V` | 1.98.1 `797e8a9bc` | 一致 | 一致 |
| `cargo fmt --version` | rustfmt 1.9.0-stable `48a229ceae` | 一致 | 一致 |
| `cargo clippy --version` | clippy 0.1.98 `48a229ceae` | 一致 | 一致 |
| `python3 --version` | 3.9.6 | 3.9.6 | 一致 |
| `sw_vers` | macOS 27.0.1，build 26A434 | **macOS 27.2，build 26B123** | **已变化**（系统升级） |
| `uname -a` | Darwin 27.0.0 | **Darwin 27.2.0**（内核 `xnu-13432.41.4~25`） | **已变化** |
| 活动工具链 | 旧记录称 `stable-aarch64-apple-darwin` 为活动 | **活动为 `1.98.1-aarch64-apple-darwin`**（`RUSTUP_TOOLCHAIN` 覆盖） | **口径不同**：旧记录未带该覆盖 |
| 已装 target（README 表） | `aarch64-apple-darwin`、`aarch64-unknown-linux-gnu`、`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu` | 该四项属于 **stable** 工具链；**1.98.1 的 target 是** `aarch64-apple-darwin`、`x86_64-apple-darwin`、`x86_64-pc-windows-gnu`、`x86_64-unknown-linux-gnu` | **旧表未标明工具链**，与当前活动工具链不符 |
| 设备 | MacBook Pro，Apple M5，10 核，32 GB | Mac17,2，Apple M5，10 核（4 Super + 6 Efficiency），32 GB，固件 20457.41.3 | 一致（本轮多出型号标识与固件） |
| 显示 | 内建 Liquid Retina XDR 3024×1964；**一台外接显示器 4096×2304（2048×1152@75Hz）** | 内建 3024×1964 Retina，Metal 4；**当前无外接显示器** | **已变化**（外接未接入；旧值不能作为当前事实） |
| 隔离候选根 | `isolation.json` 记录的 `cargo_home` | 目录**仍存在**，`cargo-home/` 仅含 `registry/CACHEDIR.TAG` 等空缓存 | 只记事实：未重建、未重新准入 |
| `gpui-pre` 0.3.9 校验和 | `archive_sha256` = `lock_checksum` = `34f61c1c…`，`matches_lock: true` | 仓库 `Cargo.lock` 与 `candidate/Cargo.lock` 均为 `34f61c1c…` | **三方一致**（见 `candidate-identity-new.json`） |

**对照结论**：工具链版本一致；OS 版本、活动工具链口径、target 集合归属、外接显示器四项
与旧表不同。旧表保留为当时快照，**不修改**；差异以本节与 README 末尾追加的勘误为准。

## 5. 候选身份（只读摘录，非重新准入）

`candidate-identity-new.json` 从 `gpui-package-identity.json` 摘录 `gpui-pre 0.3.9` 的
`archive_sha256`/`lock_checksum`/`source`/`license`/`zed-rev 93d7456…`/补丁说明，并与
仓库 `Cargo.lock`、`candidate/Cargo.lock` 的同名条目逐项比对：

```
repo  checksum: 34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0
cand  checksum: 34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0
evidence lock_checksum: 34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0
```

`checksum_equal: true`、`source_equal: true`、`evidence_lock_checksum_matches_repo: true`。
本记录**不**运行 `cargo update`，**不**重建候选，**不**改变 1.4 已记录的准入结论
（0.3.6/0.3.9 的 `regex` 全闭包禁令问题仍然成立）。

## 6. 哪些原任务仍「证据不足」

| 原任务 | 状态 | 原因 |
|---|---|---|
| 1.1 设备表 | 已登记，**仍缺 Intel Mac / Windows 10 / Linux 实机** | 见 `unverified.md` U-1～U-3 |
| 1.2 隔离候选组合验证 | **仍证据不足** | 旧 `safety-*` 缺 cwd 与时点（U-5/U-7）；本轮未重建候选（U-8） |
| 1.3 依赖账本 metadata/tree | **仍证据不足** | 旧 `tree-*` 缺时点（U-5）；本轮未执行 `cargo metadata`/`cargo tree` |
| 2.5 CI 最小接线 | 配置存在，**执行未核实** | `.github/workflows/m0.yml` 存在，但远端从未运行本轮内容（U-4） |
| 2.1 五引擎跨平台无头构建 | **仍证据不足** | 本轮不构建（U-9） |

本批**不**勾销任何原任务，**不**宣称 V-01～V-09 通过，**不**宣称 M0 闭合。
原 46 条任务在 R14 统一重新裁决。

## 7. 遗留与风险

1. `commands/*.command.json` 的 `started_utc`/`finished_utc` 精度为秒；同一秒内完成的
   短命令起止时间相同，这是采集器粒度所致，非缺字段。
2. 旧 README 设备表与本轮观察存在 4 项差异（§4）。按「证据只追加」规则，旧表**未改**，
   差异只通过末尾勘误小节暴露；是否需要改正文由 R14 裁决。
3. `cargo_home` 仍存在但缓存为空，说明隔离目录只是**未被清理**，不能据此推断旧构建产物仍在。
4. 本批未产出任何跨平台证据；U-1～U-4 需要真实设备或远端 CI 权限，超出本轮可及范围。

## 8. R11 同批整改勘误（run-3 追加，原文保留）

本节明确更正前文，不修改原历史表格、命令日志和旧采集器。详见 [run-3 补记](../run-3/report-addendum.md)与[交接](../run-3/handoff.md)。

- §2 采集窗口终点应为 `2026-11-01T04:39:25Z`，原 `04:36:48Z` 漏计两条 target 查询。
- §3 所称落盘前脱敏及 label 过滤防覆写不符合留存 `collect.py` 的真实行为：它默认仅 15 case、缺两条 target 查询、未知 label 空集成功、直接写原始输出并覆盖已有日志。旧脚本保留审计，禁止复跑；已脱敏的现存 17×3 日志不因此变成安全工具的证据。
- `legacy-gaps.json` 的 105/14 清单保留；四个 safety-* 源文件已有的 `env_overrides` 原值补在 [JSON 补记](../run-3/legacy-env-overrides-addendum.json)，旧 cwd/时点缺口仍不追认。
- §5 关于 GPUI 0.3.9 regex 当前禁令的表述不成立。现行架构 §3.2/引擎 §18.2 已批准锁定 GPUI 的精确 regex 路径及非桌面 WASI p2 条件边，五引擎及其它路径继续禁止；本轮仅纠正文义，不重新准入或构建。
- README 旧 `uname -a` 行的 hostname 已按本轮授权最小替换为 `[REDACTED]`；此前“残留待 R14”的状态不再适用，原值没有进入新证据。

run-3 修正版与本时点新采集另存，不能追认 run-1 的执行工具来源或补造历史缺失字段。原跨平台、旧证据缺项、CI 和 M0/V 缺口继续保留，整改复审由新的 reviewer 独立完成。
