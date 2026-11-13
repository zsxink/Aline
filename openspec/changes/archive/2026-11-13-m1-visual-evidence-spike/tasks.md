# Tasks

## 1. 锁定能力与准入预检

关联 QMD-01 W1、现行依赖准入与可信证据要求；CODE_STANDARDS §3～6、R-11/R-17、S-12/S-14。

- [x] 1.1 实现 `python3 scripts/probe_m1_visual_evidence.py --output-dir logs/validation/m1-single-window-readonly/<run-id>/w1`：验证锁定归档 checksum、定位离屏/真实纹理 API、采集候选 test-support metadata/tree，复用正式准入守卫；缺身份/缺能力/采集或准入失败必须非零，输出路径归一化。自测覆盖正确归档、被篡改归档和缺 API，不修改正式清单或锁文件。**2026-11-08：** 三项自测通过；真实候选采集按缺依赖失败关闭，输出目录拒绝也返回非零且未创建目录。**2026-11-09 继续：** 修正 `--cargo-home` 仅选择归档而未传至 Cargo 子进程的问题；正常/异常及原有/缺省环境均恢复原值。显式隔离审计和图差异自测扩为 14 项，覆盖副本不反写、精确 feature 激活/未知声明拒绝、节点增量、许可证拒绝与缺节点失败；AST、OpenSpec strict 和 `git diff --check` 通过。证据见 `2026-11-08-finish-01/w1/selftest-14/` 与同轮命令记录。
- [x] 1.2 执行真实预检并记录 go/no-go、候选 feature/节点变化及限制；默认预检无法取得候选图时，按 design 决定 5 用 `--audit-candidate` 形成仅 logs 内的候选清单/锁及差异，不构建、不中途批准候选；用现行工程守卫验证正式图不受影响，运行 `openspec validate m1-visual-evidence-spike --strict --no-interactive` 与 `git diff --check`。必要摘要回填 `docs/v1-validation-plan.md`（由主 Agent 汇总共享文件）。**2026-11-09：** 完整候选图/锁/差异及默认正式规则 no-go 记录齐备；独立复核核对全部身份和归档、边/features/选包及最小 API，并披露 option-ext 既有选包口径冲突。用户随后按决定 §27 明确批准这份精确隔离 W1 图并纠正口径，验证计划 V-04 已回填；只授权隔离 spike，不改生产默认 feature、正式锁或七场景基线。真实离屏仍归 2.1。

## 2. 真实离屏能力

- [x] 2.1 仅在具体候选图准入后，主线程真实 macOS spike 构造离屏窗口、调用纹理截图，连续两次验证尺寸、非空和稳定性并保存原始像素/日志。若准入或平台阻塞则保留本项未完成及所需证据，不以真实桌面截图、合成像素或静态源码代替；不修改生产入口、不将离屏通过写成 V-09 整项通过。**2026-11-09：** `scripts/run_w1_offscreen.py` 和稳定批准锁/描述/源码已落地，不依赖历史 logs。`runtime/real-03/summary.json` 为 exit 0、passed=true；完整 507 节点图、锁与 scoped 正式守卫匹配，fmt/显式 arm64 build/Cargo JSON 唯一工件与 debug profile/lipo/Clippy 全部通过。两个独立进程各两次真实 Metal 纹理读取均为逻辑 640×400、真实 scale=2、像素 1280×800、4,096,000 字节，四份 SHA-256 均为 `233c7ef4035f04f3ec495049a8dede6524713eccd2b278c764234df6a2bb2c00`，非空非单色且逐字节相同；二进制与源码/批准/锁身份回执、RGBA/PPM/原始命令全部保留。本轮仅 macOS arm64 debug 能力实验，不记录性能或屏幕呈现/真实输入/其它平台成绩。独立审查修复工件/目标绑定问题，runner 正反自测 20 项通过；真实失败 `real-01`（重包归档拒绝）和 `real-02`（最终 Clippy 新 lint 拒绝）原样保留，未用失败运行为完成依据。

当前阻塞（2026-11-08）：真实预检源码核对通过，候选 metadata/tree 均因离线缓存缺少 `futures-concurrency` 失败，具体候选闭包及准入尚未取得；正式工程守卫的 all-target metadata 也因缺少 `android_system_properties@0.1.6` 失败。因此 1.2/2.1 不勾选，未下载候选依赖或改锁。证据位于 `logs/validation/m1-single-window-readonly/2026-11-08-w1/`；`w1/summary.json` 明确 `decision: no-go`、`runtime_executed: false`、`lock_unchanged: true`。后续需要完整且已核对的离线缓存、候选精确依赖图准入及真实主线程两次像素实验。

同日补充：主 Agent 将锁文件已有依赖的缓存复制至独立目录后，统一 workspace 门禁 27 项通过；候选 `test-support` 图仍缺 `futures-concurrency`，不改变 1.2/2.1 未完成状态。新证据见 `logs/validation/m1-single-window-readonly/2026-11-08-current/workspace-final/` 与 `w1-preflight/`。

2026-11-09 继续：已完成仅 logs 内的完整隔离候选解析及锁定离线复采，记录位于 `logs/validation/m1-single-window-readonly/2026-11-08-finish-01/w1/isolated-audit-final/`。候选 metadata 507 包、macOS arm64 选包 357 包，较正式图新增 72 包（本机选中 23 包）、9 包 features/边变化，无移除或已有外部版本替换；新增包 SPDX 允许集合检查 72/72 可满足、五引擎无新增消费者。正式七场景工程守卫通过，正式十份清单/锁输入哈希不变。候选仍被 GPUI 子图指纹与五条未登记直接分支拒绝，故结论为 **no-go，尚未准入**，并非缺包阻塞已解除就可运行。`candidate-admission.json` 与 `admission-request.md` 给出每个新增节点的来源、checksum、features、全部入边、许可证及完整图绑定哈希。最小性结论与锁定 API 见 design 决定 6 和 `locked-api-paths.json`。`spike-src/` 已准备主线程真实离屏/两次纹理稳定性程序，rustfmt 解析及格式检查通过，明确未编译、未运行。1.2 的审计实现和验证已具证据，必要摘要仍由主 Agent 回填验证计划后再勾；2.1 等待具体准入和真实运行，继续未勾选。

2026-11-09 后续执行：上段为批准前记录，已由决定 §27 的精确批准及本轮真实 `runtime/real-03/` 成功运行推进至 3/3；此前 no-go 和失败证据保留，不改写为当时通过。固定入口与夹具见 `scripts/run_w1_offscreen.py`、`scripts/fixtures/w1-offscreen/`。独立复核原文见同轮 `review/w1-admission-review.md` 与 `review/w1-runtime-review.md`。本地旧 `sys-locale@0.3.2` 归档含 AppleDouble/Cargo 标记而非官方字节，已触发拒绝；从官方取得匹配锁的原始归档后，全部 499 个 registry 包共 26,450 个官方文件与已有缓存源码逐字节 hash 等价，无缺失/修改/额外源码。原缓存未覆盖；真实 W1 在新缓存中从全部正确归档重新解包，归档/源码核对及唯一补齐包详见 `runtime/source-audit/`、`runtime/real-03/archive-receipt.json`。这不把重包 hash 称为官方原始身份。
