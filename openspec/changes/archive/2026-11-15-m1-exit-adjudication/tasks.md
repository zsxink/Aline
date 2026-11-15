# Tasks

## 1. 证据复读与出口核对

关联 `docs/v1-architecture.md` §13 M1 出口行、`docs/development-process.md` §1、`docs/v1-validation-plan.md` 的 M1 分项状态段与结果表；平台 macOS arm64。

- [x] 1.1 复读 2026-11-11 的 13 行核对与机器可读产物（五样本七步、1 MiB 滚动、V-05 矩阵、V-09 十四项、窗口基线），确认 `passed`/`failures`/步骤/字节/草稿/历史/revision 字段与文档陈述一致。验收：逐项对照表写入本 change design；发现不一致则先记录，不以文档记忆代替原始 JSON。
- [x] 1.2 按架构 §13 M1 出口行逐条判定，区分「M1 出口满足」「M1 分项满足但整项未通过」「M0/跨平台/M2 保留」三类；不得把任一 V 项或 AC-15/16 写成整项通过。验收：design §2/§3 完成，保留项清单齐全。

## 2. 裁决记录

关联决定编号、阶段状态与验证计划的回写纪律。

- [x] 2.1 在 `docs/decisions.md` 新增裁决条目：M1 出口在 macOS arm64 范围收口；M2 入口解锁但未开始；V-01/V-04/V-07/V-09 整项状态不变；M0 豁免与未测平台保留；代码块横向滚动列为 M2 入口待办。验收：条目含日期、依据、范围、不声称项、回写位置与后续复审条件。
- [x] 2.2 回填 `docs/v1-architecture.md` §13、`docs/v1-validation-plan.md`、`docs/development-process.md` §1/§3.1、`AGENTS.md`、`README.md`、`docs/README.md`、`openspec/config.yaml` 的当前状态；历史日期段落不改写。验收：各处表述与决定一致，无「M1 出口未整项通过」与「已收口」并存的当前状态矛盾。

## 3. 校验与交付

- [x] 3.1 执行 `openspec validate m1-exit-adjudication --strict --no-interactive`、`openspec validate --all --strict --no-interactive`、`git diff --check`、改动文档相对链接/锚点检查与绝对路径扫描。验收：全部通过；原始输出写 `logs/validation/m1-single-window-readonly/2026-11-13-exit-adjudication/`，不提交 logs。
- [x] 3.2 逐项核对本 change 与 `docs/v1-architecture.md` §13、`docs/development-process.md` §1/§3.1、`docs/v1-validation-plan.md` 的措辞，确认未声称 M2 已开始、未声称 W4/W5 可接线、未关闭 M0 豁免或任一 V/AC 整项。验收：不提交、不推送、不归档，等待用户另行指令。
