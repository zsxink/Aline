# Aline 应用图标

2026-10-30 根据用户提供的「象牙底色上的炭灰字母与金色丝带图标.png」制作。图标主体包括象牙色圆角底板、炭灰 A 与金色丝带；底板之外为透明区域，不保留原图的白色画布。

## 实际开发使用哪个文件

用户已要求采用本组图标并删除旧图标。产品依据见 [PRD §6.1](../../v1-prd.md#61-主窗口与导航)、[UI §3.1](../../ui-spec.md#31-应用身份与默认色)，接入边界见 [架构 §4.5](../../v1-architecture.md#45-应用图标资源接入)，决定追踪见 [图标采用记录](../../decisions.md#12-2026-10-30-应用图标采用与旧图删除)。

| 开发用途 | 直接使用 | 说明 |
|---|---|---|
| macOS 应用包、Dock / Finder | [aline.icns](./aline.icns) | 已含多尺寸图像与透明安全区；不二次加背景、圆角或留白 |
| Windows EXE、安装器及快捷方式 | [aline.ico](./aline.ico) | 同一多尺寸 ICO，保持透明边缘 |
| 应用内品牌展示、Linux 应用图标 | [aline.png](./aline.png) | 透明 1024 × 1024 PNG；按实际显示或安装要求派生尺寸 |
| 平台接口明确要求 PNG | [aline-macos.png](./aline-macos.png) / [aline-windows.png](./aline-windows.png) | 选相应平台适配版，保留已有透明留白 |
| 重建 macOS ICNS | `aline.iconset/` | 构建输入，不把整个目录当运行时资源 |

`aline-original.png` 是带白色画布的输入备份，`aline-cutout.png` 是裁切母版，`preview.png` 是检查图，三者均不直接用作运行时或安装包图标。源资产暂放本目录；开发时将所需文件接入应用资源和各平台打包流程，不在用户设备运行时读取 `docs/`，不另行沿用旧图或从带白底原图重新制作。

## 文件清单

| 文件 | 用途 |
| --- | --- |
| `aline-original.png` | 用户提供的 PNG 原图，1254 × 1254，原字节备份，无透明通道 |
| `aline-cutout.png` | 1254 × 1254 透明裁切母版，保留底板 |
| `aline.png` | 1024 × 1024 RGBA 通用 PNG，裁去主体外多余透明留白 |
| `aline-macos.png` | 1024 × 1024 RGBA，主体放入约 824 × 824 区域，居中并留透明安全区 |
| `aline.icns` | macOS 传统应用图标，含 16、32、128、256、512 点的 1× / 2× 图像，最大 1024 像素 |
| `aline.iconset/` | `iconutil` 使用的完整 PNG 图标组 |
| `aline-windows.png` | 1024 × 1024 RGBA，主体放入约 960 × 960 区域，居中，外缘透明 |
| `aline.ico` | Windows 多尺寸图标，含 16、20、24、30、32、36、40、48、60、64、72、80、96、128、256 像素图像 |
| `preview.png` | 深浅背景及小尺寸对照图，仅用于检查，不作为图标输入 |
| `edit-prompt.txt` | 内置 imagegen 自动提取尝试的提示词；其输出因残边未采用 |

## 透明区与平台边界

传统 macOS `.icns` 可以在主体外保留透明安全区；透明不是白色边框。本目录的安全区尺寸是本次对现有设计的适配选择，不代表所有 macOS 图标格式的统一要求。Apple 当前另有 Icon Composer 分层工作流，其背景层和系统裁切要求不能直接套用这里的预裁切 `.icns`。见 [Apple 应用图标指南](https://developer.apple.com/design/human-interface-guidelines/app-icons)。

Windows `.ico` 使用透明画布，显示圆角底板及其内容，原图外部白底不参与显示。多尺寸与透明背景参考 [Microsoft 图标构建指南](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction)。本轮未生成 MSIX / Microsoft Store 的清单和全套商店资源。

## 来源与处理

原图 SHA-256：`1b9b602257cc033acd67d34ba6d9061d836c136d46854ea3757ff02d55dbf942`。两轮内置 imagegen 自动提取均留下外部残边，其输出未用于交付。最终直接沿原图底板轮廓裁切，向内收约 12 像素避开外侧白色倒角与阴影；裁切母版仅改变 alpha，RGB 与用户原图完全一致。随后裁去多余透明留白、缩放、导出 PNG / ICO，并通过系统 `iconutil` 封装 ICNS。缩放后移除低于约 3% 不透明度的滤波残点，避免游离碎边。

裁切轮廓采用圆角超椭圆矩形：边界 `(100, 90)–(1154, 1148)`，角半径 `290 × 291`，指数 `2.5`；4× 蒙版采样后降采样获得抗锯齿边缘。主体内容不重绘。

## 检查结果

- 原图备份哈希一致；裁切母版 RGB 与原图逐像素一致。
- 三个 1024 像素 PNG 均为 RGBA、四角完全透明，非透明区域为单一连通主体，无游离残片；已检查深浅底及 16～64 像素预览。
- ICO 回读覆盖全部 15 种尺寸，每帧均含透明区域与不透明主体。
- ICNS 经系统 `iconutil` 解包回读 10 帧，尺寸、alpha 与对应 iconset 一致。
- 尚未进行真实应用安装后 Dock / Finder / Windows 桌面显示验证。

旧的 `docs/assets/aline.png` 与 `docs/prototype/legacy/assets/aline.png` 已按用户要求删除；历史原型三处图标引用改用本目录的透明 `aline.png`。旧文件身份和本次有限变更保留于[原型清单](../../prototype/inventory.json)的原始 entries / authorized_updates 中，其余归档字节继续受检查保护。历史确认及审计快照保留当时事实，不作为继续使用旧图的依据。本组图标已被选定用于开发，打包尚未接入，也不表示 V-01 已执行。
