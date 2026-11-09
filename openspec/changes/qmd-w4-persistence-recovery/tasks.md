# Tasks

## 1. M2 前置和独立方案

关联全部 requirements、CODE_STANDARDS §2～6、R-15、K-04；当前仅建档，下面未完成项不代表已有实现。

- [ ] 1.1 按 `m1-single-window-readonly` tasks 7.1/7.3/8.2 与架构 §13 核对 M1 出口；记录 M2 可编辑会话/资源消费者和 W5 接线依赖。验收：V-08 中有明确身份/事务前置及 M1 成功证据；缺证据保持未完成，不提前启用写入口。
- [ ] 1.2 在本 design 冻结恢复记录版本/身份/路径代次/资源清单/完整性及逐步发布格式，制定不支持、损坏和预算拒绝独立输入。验收：独立设计审查通过，`crates/aline-ui/tests/fixtures/recovery/`（拟新增）具备正常/损坏/旧版本预期，明确不清理未知记录。

## 2. 受检文件替换

关联 source 的全部场景、S-5/7/10/12、R-15-07/13，先无头机制后 UI。

- [ ] 2.1 先建目标版本/同 size/mtime 外改、特殊目标、预算、取消及部分写/同步/权限/替换失败回归，再补 source 既有目标受检替换。验收拟新增 `cargo test -p aline-source --test checked_replace` 与 `cargo test -p aline-source --test atomic --test decode_golden`；独立磁盘 bytes 预期，已有 create-only 行为不变。
- [ ] 2.2 实测 macOS 目标身份、目录同步、权限/ACL/xattr、替换后同步失败的事实分类及外改竞争窗口；需要新依赖/unsafe 则先准入，不静默绕过。验收：真实文件阶段结果、失败后原文件/恢复材料和残余保证可核查，Windows/Linux/Intel 未测单列；回填 V-08 与 source 中文 rustdoc。

## 3. 串行保存和外改闭环

关联保存状态、外改/可撤销重载 requirements，R-3/5/6/15、S-7/12～14。

- [ ] 3.1 先写冻结快照/路径代次/旧回执/继续输入/取消/关闭的无头时序测试，再实现单文档唯一保存协调器与内容等价 dirty。验收拟新增 `cargo test -p aline-ui --test persistence_state`，比较文本、草稿、选区、revision、历史与实际基线，旧失败不覆盖新成功；同步架构 §7.3 接线边界。
- [ ] 3.2 接显式保存/另存为到同一队列与 ED-01 可取消等待；先用 W5 协议 fake 验证，自然平台结束的真实消费依赖 W5。验收拟新增 `cargo test -p aline-ui --test persistence_publication`：等待更新/取消/自然确认/自然取消、目标改变、成功后新输入仍脏；双语 catalog 同键同插值，中文 UI 反馈不抢焦点。
- [ ] 3.3 实现聚焦/写前内容版本检查、冲突四出口和删除/不可读保内存；自动/显式重载走可撤销历史边界。验收拟新增 `cargo test -p aline-ui --test persistence_reload`，同 size/mtime、覆盖确认后再变、Undo 保新磁盘基线/暂停自动保存、Redo 回基线及拒绝零副作用；回填引擎/架构重载实际边界。
- [ ] 3.4 实现独立 1 秒防抖自动保存和组合期延后、冲突/重载撤销暂停；未命名不选路径。验收拟新增 `cargo test -p aline-ui --test persistence_autosave` 使用虚拟时钟覆盖 999/1000ms、重复事件、组合延后、关闭取消和恢复策略；真实计时记录不替代虚拟边界。

## 4. 恢复、资源和关闭

关联恢复资源与关闭 requirements、SAFE-08/IMAGE-07/08、R-6-06/15-08～12。

- [ ] 4.1 实现独立 10 秒恢复目标、当前输入/版本和资源齐备后发布，注入损坏/部分写/资源不可读与重复重试。验收拟新增 `cargo test -p aline-ui --test persistence_recovery`，9999/10000ms、未命名/命名、组合当前输入、恢复写失败不假报保护；恢复先入内存、磁盘 bytes 不变。
- [ ] 4.2 接临时图片保全及首次保存/另存为最小逐步发布，当前/历史/恢复/在途引用均阻止早删；接单文档统一关闭 guard。验收拟新增 `cargo test -p aline-ui --test persistence_resources`，复制/写盘失败、取消、重试、继续输入、关闭/恢复后未保存保留和恢复图片可读；同步架构 §7.4 与 V-08 实际状态。

## 5. 集成、独立审查和真实证据

- [ ] 5.1 安排 source 平台安全与协调器一致性独立审查并修复 finding；删除被替代试验路径/无消费者导出/fake，执行 CodeGraph sync/status 和引用检查。验收：审查范围覆盖写入口、回执/资源释放、错误分类和 MD-11，主 Agent 复核通过；原始审查仅留 logs。
- [ ] 5.2 在真实 macOS 中文 UI 执行打开→输入→保存→关闭→重开、自动保存、同尺寸外改、冲突四出口、可撤销重载、异常恢复及图片可读，并与 W5 实测组合等待。验收：磁盘/内存独立 bytes、版本/历史/资源及操作序列齐全，V-08 更新实际结果；未测平台、断电/网络盘保证不扩大。
- [ ] 5.3 运行 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 及 engineering/lints/safety/public-api 守卫；新增守卫有正例/最小违规反例，八成员 lint 实际继承可查。验收：全部适用检查通过，原始 stdout/stderr 经 evidence_paths 归一化写 `<evidence>/qmd-w4/`，不以 all-targets 代表三平台。
- [ ] 5.4 逐项核对本 change 与 PRD AC-05/06/10、V-08、架构 §7/13，运行 `openspec validate qmd-w4-persistence-recovery --strict --no-interactive`、全量严格校验和 `git diff --check`。验收：成功/失败/未测与前置明确，全部任务仅在要求和真实证据齐全后勾选；不提交/推送/归档。
