# 文档素材说明

当前应用身份使用 [app-icon/](./app-icon/README.md) 中用户提供并完成透明裁切的象牙色圆角底板、炭灰 A 与金色丝带图标。实际开发按以下文件接入：

| 用途 | 文件 |
|---|---|
| macOS 应用包 | [aline.icns](./app-icon/aline.icns) |
| Windows EXE / 安装器 / 快捷方式 | [aline.ico](./app-icon/aline.ico) |
| 应用内品牌展示、Linux 应用图标 | [透明 aline.png](./app-icon/aline.png) |

底板之外透明，不带原图白色画布或白边。原图、裁切母版、平台 PNG 适配版及深浅底预览的用途见[完整图标使用说明](./app-icon/README.md)。PRD §6.1、UI §3.1 与架构 §4.5 已指向本组资源；资源选定不代表打包接入或 V-01 已完成。

旧的 `docs/assets/aline.png`（512 × 512）及历史原型内同字节副本已按用户要求删除，页面引用已切换到新 PNG。旧 SHA-256 `252fe12bbaa6af7c6738cbc089ad9c8c27e6e4beb97ed760aea9732a6f838317` 仅作身份追溯；历史“保留品牌参考”的决定在此范围内由[本次采用与删除记录](../decisions.md#12-2026-10-30-应用图标采用与旧图删除)替代，不再把旧图作为开发参考或回退资源。来源记录见[素材账本](./provenance.md)。

`v1-architecture.svg` 与 `v1-modules.svg` 是现行架构文档图。历史山丘示意图和十二张主题 SVG 仅在[非权威原型归档](../prototype/README.md)保留，没有提升为产品截图、正式主题预览或运行时资源。
