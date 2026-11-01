# R11 同批整改补记（run-3）

本记录修正 run-1 报告与采集工具的已确认问题，归属 M0 / R11 / F-13。
它只处理验证附件、日志隐私与事实表述，遵守工程基线 §5～6 和工程治理的日志隐私、拒绝场景、可信交付要求，不改变产品、crate、依赖或验收契约。

## 旧报告勘误

1. run-1 的 17 份 `command.json` 所记录的窗口是 **2026-11-01T04:32:49Z～2026-11-01T04:39:25Z**，原报告终点 `04:36:48Z` 漏计两条 target 查询。以旧命令文件的时点为准，不重写旧表格和命令日志，不追认旧执行来源。
2. run-1 留存的 `collect.py` 实际只有 **15 个默认 case**，不含两条 `rustup target list`，未知 label 也会空集成功退出。它还直接写入未脱敏的 stdout/stderr，并能覆盖已有日志。原报告 §3 关于“落盘前脱敏”和 label 过滤能防覆写的工具行为声明不成立。旧工具仅供审计，**不得复跑**；现存 17×3 文件完整且已脱敏的复核结论不等于旧工具安全。本轮不替换旧脚本。
3. `legacy-gaps.json` 对旧 105 份命令、14 份缺项的清单正确，但摘录的四个 `safety-*` 没有包含源文件已存在的 `env_overrides`。原值和源文件 SHA-256 补在 [JSON 补记](./legacy-env-overrides-addendum.json)。既有 `CARGO_HOME`、`CARGO_TARGET_DIR`、`RUSTUP_TOOLCHAIN` 值是旧源记录已有事实；缺失的 cwd 和起止时点仍缺失，不能借本轮时点补造。
4. run-1 §5 将 GPUI 0.3.9 的 regex 路径仍称为当前全闭包禁令问题，与正式规范冲突。现行依据是[架构 §3.2](../../../../../v1-architecture.md)与[引擎 §18.2](../../../../../engine-design.md)：仅已批准的锁定 GPUI regex 精确身份/路径及非桌面 WASI p2 条件边例外适用于 app/ui/plugin；Aline 自有直接依赖、五引擎闭包及其它路径继续禁止。身份、来源、校验和、所有入边、target 与 features 的限制继续有效。本轮只纠正文字，不新增例外、不重新准入或构建。
5. README 历史 `uname -a` 行按本轮授权仅将 hostname 替换为 `[REDACTED]`；其它字节保持不变。[替换证据](./readme-redaction.json)只存文件前后 SHA-256、行号和替换数，不保留原字段值。原报告中“仍未改、留 R14 裁决”的对应状态已被本次授权替换。

## 新采集及工具边界

[修正版采集器](./collect.py)与[安全自测](./test_collect.py)独立放在 run-3。采用具体函数：固定 case 管理只读子进程，脱敏函数同时处理 stdout/stderr，独占新目录函数统一控制写入口；没有新增依赖或框架。原始输出仅替换设备标识后保存，内存中的未脱敏值不写临时文件、不回显、不写异常正文。敏感字段覆盖 uname hostname、system_profiler serial、Hardware UUID、Provisioning UDID，字段值在两流的重复出现也替换；设备输出另替换本机 hostname。

默认运行生成唯一 `commands-<UTC>-<UUID>` 新目录；显式 `--output-dir` 必须不存在且父目录已存在，并拒绝旧 run-1 路径。未知 label 在创建目录/启动命令前非零拒绝。文件使用独占创建；命令失败仍记录脱敏输出并非零结束，不清理已有证据。若磁盘写入中断，新的运行目录可能仅有部分文件，必须保留作为失败证据，不能把它当完整运行。

本次执行 `python3 -I -B .../collect.py --output-dir .../run-3/commands`，完整 argv/cwd/UTC/exit 与原始已脱敏 stdout/stderr 见 [执行包装记录](./checks/collect-current.command.json)及 [17×3 新日志](./commands)。固定覆盖 `CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`，其余环境继承当前进程，不导出环境秘密。

本时点窗口为 **2026-11-01T08:32:46.881873Z～2026-11-01T08:32:47.589533Z**；17 命令全部 exit 0，包含两条分别归属 1.98.1 与 stable 的 target 查询。这是**新的本机观察**，不证明旧命令当时由哪个版本采集器执行，不能替代旧记录缺失的时点或 cwd。

## 验证与未验证项

- 7 项安全自测全部通过：默认 17 case、双流脱敏且模拟敏感值不落盘、未知 label 拒绝、旧目录/日志拒绝覆盖、run-1 路径拒绝、默认目录唯一、失败命令与 OSError 保全。全部使用 fake runner，不启动真实设备命令；完整 stdout/stderr 见 `checks/collector-selftest-1.*`，首次运行即通过。
- Python AST/JSON 解析、17×3 结构、argv/environment/时点、旧附件 hash、四个环境摘录原值与本地链接检查见 `checks/structure-2.*`、`validation.json`；首轮 `checks/structure.*` 因交接链接指向尚待该验证器生成的 `validation.json` 退出 1，已调整为生成后再检查该链接，首轮失败完整保留。命令差异、hash、CodeGraph 同步/状态和空白检查分别保全于本目录。
- 本轮未执行 Cargo fmt/clippy/test、metadata/tree、候选重建或重新准入；没有 Rust 源码变化。候选三方 checksum/source 的既有一致性结论不扩写为重新验证。旧历史缺项、Intel Mac/Windows/Linux 实机、远端 CI 与 V-01～V-09/M0 出口缺口继续保留。
- 本实现者只做上述验证，不给出整改复审结论；交回所有权后由新 reviewer 独立复审。
