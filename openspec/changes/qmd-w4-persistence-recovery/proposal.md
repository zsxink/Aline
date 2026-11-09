# Proposal

## Why

QMD-01 W4 要把已确认的保存/恢复方法落实为 M2 单文档安全闭环。当前 `aline-source` 只具备 create-only 发布，DocCore 只有当前快照/组合等待基础；它们不能证明既有文件安全替换、保存状态、外改重载或恢复已实现（PRD §5.2 SAFE-04/05/07/08、AC-05/06/10；验证计划 V-08）。

用户 2026-11-08 已要求 W1～W7 各自立项和实施；本 change 独立承载 W4，沿用 M2 阶段，不以立项取消 M1 出口或 V-08 身份/事务前置（开发流程 §1～3、架构 §13、决定 §25/26）。

## What Changes

- 在 M1 出口后实现单文档手动保存/另存为、1 秒防抖自动保存、外改冲突和可撤销重载、独立 10 秒恢复及临时图片最小保全；包含取消、关闭、失败与晚到写盘回执。
- 在接线前完成 V-08 身份/事务方案及无头故障测试；原字节保真与 BOM/行尾策略继续消费 source，不从解析树重建文本。
- 真实保存/重开、故障与恢复证据按平台分别记录；macOS 首轮不能代替 Windows/Linux/Intel Mac，不把已有 create-only 测试升级为平台通过。

### Non-Goals

不在 M1 只读窗口提前开放半套保存；不实现 M4 完整资源迁移和导出，亦不实现 M5 多标签、多窗、全应用 IPC、工作区路径移动。M2 所需未命名临时图片和恢复图片可读不被后置。无新的依赖、unsafe、数据库、通知运行时或静默非原子回退授权。

## Capabilities

### New Capabilities

- `ui/document-persistence`：单文档保存状态、自动保存、外改/可撤销重载、独立恢复及关闭保护。

### Modified Capabilities

- `engine/source`：补充既有目标受检替换的成功事实、前置冲突及失败保全要求；保留 create-only 行为和既有编码契约。

## Impact

影响 `aline-source` 文件原语、`aline-ui/document` 当前内容消费者及拟新增的 `aline-ui/persistence`、资源最小协调路径、双语 catalog；不新增 crate，不让 buffer 依赖 source。适用 CODE_STANDARDS §2～6、R-3/5/6/15/16/17、S-5/7/10～14，重点为实际写入事实、唯一串行所有者、旧回执和保真。公开 API 仅为真实跨 crate 消费者导出。

### Acceptance Evidence

无头故障矩阵和独立审查先于 UI 接线；真实磁盘逐字节比较、保存期间继续输入、同 size/mtime 外改、再次外改、重载 Undo/Redo 基线不退、恢复先入内存且图片可读。命令、环境、原始结果写 `logs/validation/m1-single-window-readonly/<run-id>/qmd-w4/` 并经 `scripts/evidence_paths.py` 归一化，摘要回填 V-08/架构 §7/13。只有全部适用要求及证据齐全才勾选实现任务。

### Risks

现有目标替换、目录同步、权限/ACL/xattr 和外部竞争窗口仍需平台证明；若安全访问保证需要新依赖/unsafe 例外，先独立准入和裁决，未获准时失败关闭。当前 M1 出口和 M2 可编辑/资源消费者未完成，是明确前置，不是已取得平台失败结果；本次工件不声称 W4 实现或 V-08 通过。
