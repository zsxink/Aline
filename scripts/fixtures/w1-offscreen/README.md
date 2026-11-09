# W1 固定离屏能力夹具

本目录保存决定 §27 批准的隔离 macOS arm64 测试锁、批准图描述与独立能力场景源码。不是生产 workspace 成员，不改变应用的默认 feature。`approval.json` 绑定正式构造输入、候选锁/清单、全部 metadata 节点和本机选包；图、版本、来源、checksum、features、边或 target 漂移均须重新审计。

```sh
python3 scripts/run_w1_offscreen.py --self-test --output-dir logs/validation/m1-single-window-readonly/<run-id>/w1/runtime/selftest
python3 scripts/run_w1_offscreen.py --cargo-home <cargo-home> --output-dir logs/validation/m1-single-window-readonly/<run-id>/w1/runtime/real
```

运行前需要已安装仓库登记的 Rust/Xcode/Metal 工具链，以及包含固定锁全部归档和索引的 Cargo 缓存。runner 核对每份归档 checksum，在本轮独立缓存重新解包源码，只作锁定离线采集和构建，不下载或安装工具。运行产物在指定验证目录，独立编译缓存为 `target/w1-offscreen`。

默认缺少或错误归档即拒绝。需要补齐已批准锁中的原始归档时，可显式加 `--fetch-approved-archives`；只从 crates.io 获取该精确包，SHA-256 必须等于固定锁，再写入本轮新缓存，不改原缓存、不解析新版本。metadata、构建和 Clippy 仍锁定离线执行。不能把内容相同但包含额外元数据的本地重打包 `.crate` 当作官方原始归档。

runner 仅在验证目录构造八成员副本：复用正式清单、激活获批的精确测试 feature、安装本目录的候选锁与场景源码。完整图匹配后，在内存策略副本登记该隔离场景并复用正式守卫。生产清单、生产锁和正式七场景基线保持原样。

实际二进制必须来自本轮 Cargo JSON 报告的唯一 `aline` 工件，显式 target、debug profile 与 Mach-O arm64 均受检；回执保存源码、批准描述、锁和二进制 SHA-256。两个独立进程各读两次真实 Metal 纹理，检查逻辑尺寸、真实缩放、物理像素尺寸、非空非单色及逐字节稳定性，保留 RGBA、PPM 与原始命令。该结果只证明离屏能力，不证明屏幕呈现、真实点击/焦点/IME、显示缩放变化、锁屏、其它平台或性能门槛。

此场景的色块与中文字符串属于独立测试内容，不是产品 UI 文案，也不充当真实 Aline 阅读器的视觉金样。
