# Design

## Context

动机见 proposal。锁定 `gpui-pre@0.3.9` 有 macOS + `test-support` 门控的 `VisualTestAppContext`，其 `capture_screenshot` 走 `Window::render_to_image`；锁定 `gpui-pre-macos@0.3.9` 的 `PlatformWindow::render_to_image` 也受 `test-support` 门控。生产后端目前只启用 `font-kit`，不能把源码接口存在当作可用截图。

## Goals / Non-Goals

目标是给出可复查的预检结果和真实能力实验的明确入口。非目标是改动 Aline 生产窗口构造、增加第二套准入守卫、截图回归框架或变更 V-09 验收方式。

## Decisions

1. 使用一个普通 Python 脚本编排现有 `check_engineering.record_command`、`check_inputs` 和策略。输入为现行清单、锁文件和 Cargo 缓存；输出目录与路径规范化复用 `validation_output`/`evidence_paths`。默认预检临时解析依赖的 `test-support`，不写回正式清单或锁，实际执行必须 `--locked --offline`。脚本没有生产状态、线程或 UI 所有权。
2. 从缓存 `.crate` 归档读取源码，先验证 SHA-256 等于 Cargo.lock 的 checksum，避免把可变解包缓存误认成锁定事实。记录具体源码片段、归档身份、候选 feature 节点与准入错误；缺档、缺接口、元数据失败、准入失败全部非零。接口静态结果与 runtime 结果分开。
3. 若预检拒绝，停止依赖该图的构建，不自动更新指纹或批准新边。真实 spike 需要已准入图，在主线程构造 headless MacPlatform/VisualTestAppContext，连续两次运行相同场景，核对逻辑尺寸、scale、像素尺寸、非空及稳定性。返回值的原始 RGBA 可保存无损 PPM，避免为了 PNG 加 `image` 直连；仍不能绕过 test-support 的准入。
4. 不拆 crate、不增加 trait 或窗口测试公共 API：此时真实阻碍是候选图而不是抽象不足。源码边界、API 兼容性和生产行为均保持。
5. 2026-11-08 继续实施时，默认预检在离线缓存缺 `futures-concurrency` 处拒绝，且正式锁根本不含 `futures-concurrency`、`postage`、`proptest`；仅补缓存也不能得到可核对的完整候选锁。因此增加显式 `--audit-candidate` 的**隔离审计**路径：仅在本轮验证目录复制八成员清单与正式锁，源入口用不构建的最小桩满足 Cargo metadata 的目标发现；在隔离清单的精确 `gpui_macos` 声明追加 `test-support`，保留已有 `font-kit` 并记录该单行差异。允许联网 `cargo update --workspace` 仅补齐候选锁所需的新闭包，保留既有外部锁版本，然后以 `--locked --offline` 复采全图、macOS arm64 选包树和 target-filter metadata，与正式图按包身份/校验和/features/完整入边作差。此路径不修改正式清单、锁文件或策略，不构建、不激活生产 feature；所有候选新路径与指纹仍交现行守卫失败关闭，解析成功不是准入批准。输出具体待审差异和许可证策略检查，准入前不运行离屏。缓存只用于 metadata/归档获取，不安装工具或执行候选构建脚本。
6. 最小特性路径按锁定归档核对：core 的 `Window::render_to_image` 与 macOS 的 `PlatformWindow::render_to_image` 实现都受 `cfg(test 或 feature=test-support)` 控制；只开 core feature 不能启用 macOS 的具体纹理读取。依赖 crate 不会因调用方测试而自动取得 `cfg(test)`，故在不改上游的约束下需要 `gpui_macos/test-support` 传递 core 与 apple 的对应特性。core 的该特性硬编码启用 wayland/x11，Cargo 的特性不能减法关闭；不以额外 patch 或绕过后端冒充最小图。隔离审计中出现的 `zed-scap` 为非 macOS target 分支；保留在完整 metadata 中审计，本机 tree 不选中不等于可删除该身份约束。
7. 2026-11-09 用户按[决定 §27](../../../docs/decisions.md#27-2026-11-09-w1-精确离屏测试图与-option-ext-口径)批准精确隔离图后，提供 `scripts/run_w1_offscreen.py` 与 `scripts/fixtures/w1-offscreen/` 的稳定锁、批准描述及源码，运行不依赖历史 logs。runner 只在新验证目录创建八成员 metadata/运行副本，不改生产 workspace；正式输入清单哈希、候选锁和根清单哈希先检查，完整 507 节点（含 sidecar）的身份/来源/checksum/许可证/features/边/本机选包共同绑定完整图指纹。仅对这个已绑定图在内存策略副本中登记获批 macOS arm64 指纹与五条分支用途，复用现行依赖/许可证守卫，不放宽生产策略。归档逐一核对后复制到本轮独立 Cargo 缓存，registry/src 必须由归档重新解包，禁止复用可变旧源码；只用锁定离线采集/构建。运行使用独立 `target/w1-offscreen`，二进制初始主线程进入离屏调用链；两个独立进程各取两份纹理，核对尺寸、scale、非空非单色与逐字节稳定性。debug 运行仅作能力证据，不记性能成绩。失败保留日志和像素，任务 2.1 继续未完成。
8. 离屏 runner 的工件与目标独立复核后增加显式 `--target aarch64-apple-darwin`、Cargo JSON 唯一工件/package/profile/target 路径验证、Mach-O arm64 检查及 binary/source/approval/lock 哈希回执，不用固定路径猜本轮产物。当前缓存的 `sys-locale` 原始归档曾被本地重打包，触发 checksum 拒绝；正确官方归档及全部 499 个 registry 包的解包源码逐文件核对后，已有缓存源码与官方文件内容等价，但坏重包不能冒充原始归档。因此显式 `--fetch-approved-archives` 只允许补齐固定批准锁中的精确原始包，核对 checksum 后写新验证缓存；默认仍拒绝，原缓存保留，Cargo 图采集/构建仍离线，不安装工具或升级依赖。

## Risks / Trade-offs

- [候选图与已批准 M1 指纹不同] → 预检调用现有策略失败关闭，保留明确拒绝；只在具体依赖准入完成后进入 runtime。
- [缓存或平台缺失] → 默认预检记录未测、保持离线；显式隔离审计只获取候选 metadata/归档用于形成准入材料，不升级 GPUI、不构建、不把静态核对或解析成功当作平台成功。
- [源码注释提及 ScreenCaptureKit] → 以实际 `capture_screenshot` 调用链为准，其实现为直接纹理读取；不新增 screen-capture feature。
- [离屏不能覆盖真实输入和系统窗口] → 所有文件/摘要标记 offscreen，不替代 M1 7.1/7.3、V-09 真实证据。
