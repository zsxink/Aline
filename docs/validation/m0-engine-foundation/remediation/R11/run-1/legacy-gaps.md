# R11 旧缺口登记（F-13）

本文件只登记**事实**。旧记录无法追认采集时点，本轮**不补造**、不回填任何旧
`*.command.json`，也不在 README 里把旧缺口写成「已修复」。

## 1. 命令记录字段缺口

审计范围：`docs/validation/m0-engine-foundation/admission-2026-10-30/**/*.command.json`
（递归，共 **105** 份）。

缺字段要求集：`argv`、`cwd`、`started_utc`、`finished_utc`、`exit_code`、`id`。

**14 份缺字段**（完整原值见 `legacy-gaps.json`）：

| 文件 | 缺失字段 |
|---|---|
| `safety-build.command.json` | `cwd`、`started_utc`、`finished_utc` |
| `safety-clippy.command.json` | `cwd`、`started_utc`、`finished_utc` |
| `safety-fmt.command.json` | `cwd`、`started_utc`、`finished_utc` |
| `safety-negative-edition.command.json` | `cwd`、`started_utc`、`finished_utc` |
| `tree-all-base-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-all-test-support-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-linux-arm-both-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-linux-x64-both-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-linux-x64-wayland-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-linux-x64-x11-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-macos-arm-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-macos-arm-test-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-macos-intel-packages.command.json` | `started_utc`、`finished_utc` |
| `tree-windows-x64-packages.command.json` | `started_utc`、`finished_utc` |

## 2. 环境命令采集缺口

`admission-2026-10-30/environment-commands.json`：12 条，含 `argv`/`exit_code`/`stdout`/`stderr`/`actual`，
**无 `cwd`、无 `started_utc`/`finished_utc`**。本轮新采集见 `commands/toolchain-*.command.json`。

## 3. 设备/显示观察缺口

`docs/validation/m0-engine-foundation/README.md` 的 OS/CPU/设备/显示器表**未保全**
对应的 `system_profiler` 原始输出，也**没有采集时间**。本轮新采集见
`commands/device-system-profiler-hardware.stdout.log` 与 `...-displays.stdout.log`。

## 4. 判定

这**不**证明旧构建没有发生，也**不**判定硬件表必错。仅说明：这三类记录的
「采集时点 / 原始输出」无法由本轮追认，故原任务中依赖它们的条目仍只能算
「证据不足」，不能因本文件而升级为「已核实」。
