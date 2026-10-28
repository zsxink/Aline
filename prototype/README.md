# Aline 界面原型

这是用于对照 MarkFlow 现有界面、评审 Aline 桌面 UI 的可运行交互原型，不是正式应用实现。Aline 正式产品仍按仓库文档规划为 Rust + GPUI 原生应用；本原型仅覆盖界面结构和演示交互，不负责文件系统、编辑器内核、导出等真实能力。

## 运行

直接打开 `index.html`，或在仓库根目录执行：

```sh
python3 -m http.server 4173
```

再访问 `http://localhost:4173/prototype/`。

## 原型范围

- 主窗口：按 MarkFlow 0.1.5 的标题栏、工具栏、标签、250px 文件/大纲侧栏、编辑区和状态栏布局复刻，并提供主题与专注模式演示。
- 设置：采用 MarkFlow 0.1.5 的全窗口偏好设置布局，包含通用、外观、编辑器、图片、高级、快捷键、关于页面。
- 其他窗口：关于对话框；近期文件菜单、通知等常见弹层。
- 应用名称与图标使用 Aline；布局和配色依据本机 MarkFlow 源码与运行截图复刻。

## 素白主题与一期语法展示（2026-10-28）

浅色主题以 GitHub Primer 的白色表面、灰色文字与边框、蓝色交互强调为依据；主窗口仍沿用 MarkFlow 的布局。对应样式集中在 `light.css`，图标集中在 `icons.js`，仅使用系统字体，不下载或分发字体文件。

左侧提供五份可切换文档：基础语法、列表与引用、代码与表格、公式与图表、图片与 HTML。覆盖 `docs/v1-features.md` 中的一期元素，包含五种 GitHub Alerts、嵌套列表、任务框、代码高亮、表格对齐、目录、脚注、HTML 文本与样式，以及公式和 Mermaid 预览。

标签独立保存内容和滚动位置，支持关闭、新建、源码切换、大纲跳转、侧栏拖宽、任务框勾选及常用格式按钮。保存写入当前浏览器的 localStorage，不写入磁盘。

示例的公式使用 MathML，Mermaid 使用预制 SVG；原始示例可在源码和可视化模式间切换。修改源码后的预览仅支持常用语法，复杂语法的动态解析、字节保真、完整撤销及真实文件操作留给正式引擎。浏览器原型不是引擎验收结果。Frontmatter 编辑、PlantUML、分屏和导出不作为一期展示入口。

参考：https://primer.style/product/css-utilities/colors/

中文排版已引入 Measure 基础样式。自动校验未运行成功：npm registry 返回 `No matching version found for measure-cjk@0.1.0`；不能报告 error/warn 数量为零。
