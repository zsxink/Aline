# M1 原生后端准入候选方案

状态：**用户已批准候选方案、三个精确 MPL 包例外（`cbindgen`、`option-ext`、`dwrote`）、`libloading@0.8.9` 的精确 ISC metadata-only 路径例外、原列的 `wasi`/`wasip2` 路径和随后发现的9条精确 `wasip2` metadata 边；也已批准表中7个 M1 GPUI 组合图指纹，M0 值保留为历史。真实 Cargo 准入守卫已在 all、all-features 与五个桌面目标场景通过；完整守卫自测117/117、许可证规则13/13及路径并集14/14通过。**这些是 2026-11-02 的依赖准入裁决，本身不等于原生 build/窗口证据；后者在 2026-11-03 Apple Metal 工具可用后已由 tasks 4.2/4.3/4.4 分别取得锁定离线 build、macOS 真实空窗显示/退出、中文界面启动与文件打开/取消/关闭的真实操作证据，design §1 亦已记录。Windows 10/Intel Mac/Linux 的跨平台构建与窗口仍属未测项，不因本裁决或 macOS 首轮结果推断为已验证。**不批准 GPUI 升级、自有 unsafe、未登记的其它包例外或其它指纹变化。**

## 推荐范围

M1 先准入 macOS 原生后端，使用 crates.io `gpui-pre-macos =0.3.9`，关闭默认 feature，只启用 `font-kit`；系统默认语言使用 `sys-locale =0.3.2`。不升级 GPUI，不添加 Aline 自有 `unsafe`。

候选 manifest 差异（仅供裁决）：

```toml
# workspace.dependencies
gpui_macos = { package = "gpui-pre-macos", version = "=0.3.9", default-features = false, features = ["font-kit"] }
sys-locale = { version = "=0.3.2", default-features = false }

# aline-ui 的 macOS target dependency
gpui_macos.workspace = true

# app 的系统 locale 检测依赖
sys-locale.workspace = true
```

`font-kit` 必须开启：同源 `MacPlatform::new` 在未启用时选择 `NoopTextSystem`，不能用于实际正文渲染。应用装配的目标类型为 `Rc<dyn gpui::Platform>`，通过 `gpui::Application::with_platform` 传入 `MacPlatform`。

## 来源、版本与类型身份

- 锁定核心：crates.io `gpui-pre =0.3.9`，仓库声明来源 revision `93d745668805029476e6d7fbb19652116fed51de`；当前项目 lock 的 checksum 是 `34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0`。
- 直接后端：`gpui-pre-macos =0.3.9`，Apache-2.0，checksum `c7c988b7e49775cc8542dbe48d5a10e073d8e898a023ef3ff3d24f33363e9863`。
- macOS 后端依赖 `gpui-pre-apple =0.3.9`，Apache-2.0，checksum `b52f58893fe1a0836464428dfad2b5eaf42c9c303a9638f4c2aced5a69072a15`。两个发布包的 metadata 都声明同一 Zed revision，并以 `package="gpui-pre", version="=0.3.9"` 依赖核心；Cargo metadata 将核心解析为单一 package ID，因此不会引入第二份 GPUI trait 类型。
- Apple shader build script 读取的 `scene.rs`、`geometry.rs`、`color.rs`、`window.rs`、`platform.rs` 五个文件已与锁定核心逐字节比较一致。该包的 build script 会调用 `xcrun metal`；隔离 `cargo check` 在此步骤失败，因为当前环境没有 `metal` 工具，未到 Rust 类型检查阶段。因此源码和 Cargo package identity 支持兼容判断，但仍缺成功编译证据。
- `sys-locale =0.3.2`，MIT OR Apache-2.0，checksum `8eab9a99a024a169fe8a903cf9d4a3b3601109bcc13bd9e3c6fff259138626c4`，桌面目标无第三方依赖。GPUI core 没有系统首选语言 API。该库在 macOS 调用 `CFLocaleCopyPreferredLanguages`；app 通过安全 API 读取系统首选语言，再映射至现有 `Locale`，读取失败按契约回退英文。显式注入 `Locale` 的单元测试不能代替实际系统检测观测。

来源：[gpui-pre-macos 0.3.9](https://docs.rs/crate/gpui-pre-macos/0.3.9)、[gpui-pre-apple 0.3.9](https://docs.rs/crate/gpui-pre-apple/0.3.9)、[sys-locale 0.3.2](https://docs.rs/crate/sys-locale/0.3.2)、[锁定 Zed revision 的 GPUI platform manifest](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui_platform/Cargo.toml)。

## 依赖闭包与现行准入策略冲突

隔离 macOS target probe 使用锁定的 `gpui-pre =0.3.9`、上述精确后端和系统 locale 包；保存了 `cargo metadata --filter-platform aarch64-apple-darwin` 与 `cargo tree --target aarch64-apple-darwin -e normal,build` 的完整输出。过滤后的候选可达闭包为 349 个 package ID（包含现有 GPUI 核心）；闭包完整版本、来源、许可证、边与 feature 记录在本地 `logs/validation/m1-single-window-readonly/2026-11-02-apply/native-admission/`，没有写入仓库公共报告。

现行 dependency admission 许可证允许集不含 MPL-2.0，候选闭包至少有以下 metadata 身份需要单独处理：

| 包 | 版本 | 声明许可证 | 闭包位置 |
|---|---:|---|---|
| `cbindgen` | 0.28.0 | MPL-2.0 | `gpui-pre-apple` 的 build dependency |
| `option-ext` | 0.2.0 | MPL-2.0 | 字体/目录依赖路径 |
| `dwrote` | 0.11.5 | MPL-2.0 | 完整 all-target metadata 的条件矛盾路径；不被任何 desktop target tree 选中 |

用户已批准仅将 `cbindgen@0.28.0`、`option-ext@0.2.0` 与 `dwrote@0.11.5` 三个精确 package/version、且仅归属 `gpui-pre-macos` 闭包的 MPL-2.0 用途登记为例外。`dwrote` 的精确路径为：`gpui-pre-macos@0.3.9 --[cfg(target_os=macos)]→ gpui-pre-apple@0.3.9 --[cfg(any(target_os=macos,target_os=ios))]→ zed-font-kit@0.14.1-zed --[cfg(target_family=windows)]→ dwrote@0.11.5`。checksum 为 `9e1b35532432acc8b19ceed096e35dfa088d3ea037fe4f3c085f1f97f33b4d02`。该完整 metadata 路径同时要求 macOS 与 Windows 条件，任何真实 target 均不可能满足全路径；macOS 与 Windows target tree 均未选中此包。此例外不扩展全局 MPL allowlist，也不构成法律意见。

真实准入复跑曾发现 `libloading@0.8.9`，声明许可证 ISC、registry checksum `d7c4b02199fee7c5d21a5ae7d8cfa79a6ef5bb2fc834d6e9058e89c825efdc55`。用户已批准仅对该精确 package/checksum/root/路径登记例外。完整路径为 `gpui-pre-macos@0.3.9 --[cfg(target_os=macos)]→ gpui-pre-apple@0.3.9 --[cfg(any(target_os=macos,target_os=ios))]→ zed-font-kit@0.14.1-zed --[cfg(not(any(target_family=windows,target_os=macos,target_os=ios,target_arch=wasm32,target_env=ohos)))]→ yeslogic-fontconfig-sys@6.0.1 → dlib@0.5.3 → libloading@0.8.9`；alias、每个路径节点 checksum 与所有连续边由准入规则精确校验。后半段平台约束与 macOS root 条件矛盾；五个登记桌面 target tree 均未选择此包。例外不加入全局 ISC 允许集。

## unsafe 归属与目标影响

AppKit、Metal、CoreText、Windows 与 Linux 平台后端及底层 FFI 中的 `unsafe` 属于锁定版本的第三方 `gpui-pre-*` 包及其平台依赖；`sys-locale` 的原生 FFI 也封装在第三方 crate 内。Aline 调用安全 API，不改 `unsafe_code = "forbid"`。这只确认 unsafe 的代码所有者，不是第三方代码的全面安全审查。

该候选只包含 macOS 后端：Apple Silicon 与 Intel Mac 使用同一代码路径，但本轮实际实验仅能覆盖当前 Mac；Windows 10、Linux、Intel Mac 继续未测，不能声明支持或验证通过。后续若准入 Windows，候选为同源 `gpui-pre-windows =0.3.9`；Linux 候选为 `gpui-pre-linux =0.3.9` 并单独裁决 Wayland/X11 与 `gpui-pre-wgpu` 闭包。两者会新增 target-specific 闭包，需重新审计而不自动纳入本次许可。

## 进入原生接线前的验证门

以下 4 条是 2026-11-02 设定的接线前置门；截至 2026-11-03 已全部关闭（见文末「后续状态」），此处保留原门的内容以便追溯，不再表示当前阻塞。

1. 用户已完成 macOS 候选、精确包例外和七场景 hash 裁决。
2. 策略实现通过精确许可证13/13与完整守卫合成自测117/117；真实依赖守卫 all 与七场景均通过，桌面目标的 `dwrote`/`libloading` target tree 明确检查为不选中。
3. 在具有 Apple Metal 开发工具的真实 macOS 环境成功编译最小 `Rc<MacPlatform> -> Rc<dyn gpui::Platform>` 消费方；设门时当前环境缺少 `xcrun metal`、该 build 证据待补，2026-11-03 Xcode 27.2/Metal Toolchain 可用后已由 tasks 4.2 取得 `cargo build -p aline --locked --offline` 通过的证据，此门关闭。
4. 之后才接 `Application::with_platform`、主线程窗口与系统 locale 实验；已在 tasks 4.2/4.3/4.4 接通并取得真实空窗、中文界面与文件选择证据。Windows 10、Intel Mac、Linux 和完整跨平台构建继续保留为未测项。

## 原始审计位置

隔离探针源码、目标 metadata/tree、package/license 检查及失败构建记录留在 ignored logs；原始日志经 `scripts/evidence_paths.py` 规范化。它们不是通过该依赖准入策略或原生编译的证据。

## 用户批准例外后的守卫复核

2026-11-02 用户批准了上表 `cbindgen@0.28.0`、`option-ext@0.2.0` 与 `dwrote@0.11.5` 的精确 MPL-2.0 闭包例外。实现了按 package/version/source/checksum、后端 root、cfg 目标及所有即时入边限定的准入规则；专项和完整工程合成自测通过。正式 Cargo 依赖与 lock 暂已生成以核验真实闭包。

正式 `scripts/check_engineering.py` 仍失败关闭，发现该后端闭包另触及禁用包路径：

1. macOS target 的完整 metadata（但不在 macOS target tree 选中）包含 `wasi@0.11.1+wasi-snapshot-preview1`，Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT，checksum `ccf3ec651a847eb01de73ccad15eb7d99f80485de043efb2f370cd654f4ea44b`。精确 metadata 边为 `zed-font-kit@0.14.1-zed --dirs--> dirs@5.0.1 --dirs_sys--> dirs-sys@0.4.1 --redox_users[target_os=redox]--> redox_users@0.4.6 --getrandom--> getrandom@0.2.17 --wasi[target_os=wasi]--> wasi@0.11.1+wasi-snapshot-preview1`；该路径从获批的 `gpui-pre-macos@0.3.9`/`font-kit` 闭包到达。候选没有在 macOS/Windows/Linux target tree 中启用这条 WASI 目标分支，但现行守卫也检查 metadata-only 禁库。
2. 新后端使已有精确准入的 `wasip2@1.0.4+wasi-0.2.12`（checksum `b67efb37e106e55ce722a510d6b5f9c17f083e5fc79afc2badeb12cc313d9487`）经原路径可达：`gpui-pre-macos@0.3.9 → gpui-pre-scheduler@0.3.9 → rand@0.9.5 → rand_core@0.9.5 → getrandom@0.3.4 → wasip2@1.0.4+wasi-0.2.12`。用户已批准此精确路径，包括 desktop target-filtered metadata-only 出现；desktop target trees 必须不选中。

随后完整 metadata 审计发现 `wasip2` 有 14 条实际入边，而原登记路径含 5 条；用户随后明确批准剩余9条 metadata 边，仍限定同一包 checksum、获批 macOS root 和目标树不选择约束。实现和复验须仍逐边精确匹配：

```text
gpui-pre-macos@0.3.9 → gpui-pre-apple@0.3.9 → image@0.25.10 → ravif@0.13.0 → rav1e@0.8.1
gpui-pre-macos@0.3.9 → image@0.25.10
rav1e@0.8.1 --[cfg(fuzzing)]→ rand@0.9.5 → rand_chacha@0.9.0 → rand_core@0.9.5 → getrandom@0.3.4 --[target_env=p2]→ wasip2@1.0.4+wasi-0.2.12
rav1e@0.8.1 --[cfg(fuzzing)]→ rand_chacha@0.9.0
ravif@0.13.0 --[target=wasm32-unknown-unknown]→ rav1e@0.8.1
``` 

这些额外边仅见于完整 metadata 路径，其中 `cfg(fuzzing)`/WASI target 分支在桌面 target tree 不选中。该9边例外不允许其它来源、版本、alias、target、root、桌面 target-tree 选择或额外入边。完整结构化差异见 `logs/validation/m1-single-window-readonly/2026-11-02-apply/native-admission/native-path-real-route-diff.json`。

此外，`scripts/engineering_policy.json` 中受审 GPUI 核心指纹七场景均发生变化：

| 场景 | M0 基线 | 当前候选 |
|---|---|---|
| all / all-features | `cfb02f486601ea7deabbb3f00e252a09a584ae4ffd1edc27f1d826bd50da6793` | `e8b4330d9a42fd80a3587d939106807bbf3ded16708919b514f95e2466b132bf` |
| macos-arm | `566a8f38426457379e7f65e1fd302b5852b062544222a5f43cf331da88816f0b` | `c7a62d9185d8555b550e34367066bf65ebd75a3d34f1a6645dfa5a09b59a7fcf` |
| macos-intel | `720d711d934a5556d7c353f01745ed1dddd5614fd8b9790f9d0e0cd4eea6c22a` | `3ba0baf76f67797824be1ad3d156df8a0d4369082cc464759c52469a7036604e` |
| windows-x64 | `7be384797015bcf31fb7907681c8e1489aa296e3eef09d62034f8cd0e7bc682e` | `5e248cb4f27cfd83b90b2e3ede593294e51b1a702e41a41110881527efbdc66c` |
| linux-x64 | `9a9e650697a552c5984fdaf0e93f75f87bf38655915769a91a1250039462f3b3` | `ae06690df66eb9809ec614153a32c86d70b3ab4b4c5e65adf15ab2267148ee8e` |
| linux-arm | `02fee5ca4c0b024365fce876ff3a4b7d6e325efe6d563b9322b4729fa689537a` | `c4512e5e89b8102f50376bee404dcbef48087eb46e84e450eeda42d3c909c831` |

独立审查确认完整图从 347 到 435 包，GPUI 闭包从 317 到 325 包；core 包身份、checksum 和自身 features `[]` 未变，新增共享 feature/edge 由后端带入。完整新增 feature/edge 差分与 M0 七场景值保留在本地归一化审计证据中。用户批准保持现行 hash 语义，以表中七场景值作为 M1 组合图新指纹并保留 M0 历史值；不通过过滤共享 features/edges 保住旧 hash。准入策略中的 hash 登记和全场景复跑仍在执行，不能在完成复验前宣称守卫已通过。

2026-11-02 用户已明确批准前述精确 wasi/wasip2 路径及新增9条 `wasip2` metadata 边，并批准表中七场景 M1 指纹；`wasip2` 在任何 desktop target tree 仍必须不选中。用户另批准 `dwrote@0.11.5` 精确 MPL-2.0 与 `libloading@0.8.9` 精确 ISC metadata-only 例外。实现精确完整路径约束后，许可证回归13/13、branched path guard14/14、完整工程合成自测117/117通过；真实 Cargo 准入 all 与七场景通过，五个 desktop target 各自对 dwrote/libloading 检查均不选中。以上为 2026-11-02 当日结论，当时「原生窗口接线的准入门已通过；Metal 缺失仍阻断本地 GPUI 原生编译」。后续状态以 tasks 4.2/4.3/4.4 与 design §1 为准：Metal 工具就绪后锁定离线 build、真实空窗、中文界面启动与文件选择/取消/关闭均已实测；「Metal 缺失阻断」已不再是当前阻塞，Windows 10/Intel Mac/Linux 跨平台构建与窗口仍属未测。

## 后续状态（2026-11-03 起）

本文「来源、版本与类型身份」「依赖闭包」「进入原生接线前的验证门」「用户批准例外后的守卫复核」各节是 2026-11-02 依赖准入裁决当时的审计快照，其中「缺少 `xcrun metal`」「build 证据待补」「Metal 缺失仍阻断」等均为设门时的事实，不代表现状。此后的进展只回填到 tasks 第 4 组、design §1 与 `docs/v1-validation-plan.md` 的 V-01/V-09 分项，不倒改这些历史审计记录；判断当前状态时应一并阅读上述现行工件。
