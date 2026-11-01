# R11 handoff

状态：**DONE（未提交）**。只补证据，未改任何代码。
基线 `2f1dcea`；执行时 HEAD `e2b12bc`；采集窗口 `2026-11-01T04:32:49Z`–`04:36:48Z`。

## 交付

- `commands/`：17 条命令 × 3 文件（`*.command.json` + `*.stdout.log` + `*.stderr.log`），全部 exit 0。
- `legacy-gaps.json` / `legacy-gaps.md`：105 份旧 `*.command.json` 审计，14 份缺字段。
- `candidate-identity-new.json`：`gpui-pre 0.3.9` 三方校验和一致（`34f61c1c…`）。
- `unverified.md`：U-1～U-9 不可复现项。
- `report.md`：盘点方法、打码策略、新旧对照、仍证据不足的原任务。
- `collect.py`：可复跑的采集器（stdlib only，支持 label 过滤以免覆写已打码日志）。

## 打码

`Serial Number (system)` / `Hardware UUID` / `Provisioning UDID` / `uname` 主机名 → `[REDACTED]`，
原地替换，无未打码副本留存。型号、芯片、核数、内存、固件、分辨率保留。

## 与旧记录的四项差异（重要）

1. OS：旧 macOS 27.0.1 / 26A434 → 现 **27.2 / 26B123**。
2. 活动工具链：旧记录称 stable → 现 **1.98.1**（`RUSTUP_TOOLCHAIN` 覆盖）。
3. target 集合：README 表四项属 **stable**；1.98.1 为 `aarch64-apple-darwin`、
   `x86_64-apple-darwin`、`x86_64-pc-windows-gnu`、`x86_64-unknown-linux-gnu`。旧表未标工具链。
4. 外接显示器：旧记录 4096×2304 → 现**无外接**。

## concerns

1. 上述差异我只在报告与 README 末尾追加勘误暴露，**未改**旧表正文；是否改正文留给 R14。
2. `cargo_home` 仍存在但缓存为空（仅 `CACHEDIR.TAG`），不能据此推断旧构建产物仍在。
3. 本轮未跑 build/clippy/test（属 R14），故 1.2/1.3/2.1 仍为「证据不足」。
4. 未推送：`origin/main..HEAD` 有 5 个提交，远端不含本轮任何内容，故 2.5 的 CI 执行仍不可核实。
5. 时间戳精度为秒，同秒短命令起止相同，属采集器粒度。
