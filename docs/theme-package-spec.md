# Aline 主题包规范

主题包是一个独立目录，可由 ZIP 分发并安装到 Aline 的主题目录。每个主题包必须包含清单和白天、夜间、护眼三张预览图；缺少任一模式预览时，主题包不通过导入校验。

## 目录结构

```text
my-theme/
├── theme.json
└── previews/
    ├── light.svg
    ├── dark.svg
    └── sepia.svg
```

ZIP 可直接包含 `theme.json` 和 `previews/`，也可将上述目录整体置于 ZIP 根目录下一层；若使用单层目录，目录名必须与 `id` 一致。清单里的预览图路径相对于 `theme.json` 所在目录。

## 清单字段

```json
{
  "schemaVersion": 1,
  "id": "my-theme",
  "name": "My Theme",
  "version": "1.0.0",
  "author": "Theme author",
  "colors": {
    "light": { "bg": "#ffffff", "surface": "#ffffff", "sidebar": "#f6f8fa", "fg": "#1f2328", "muted": "#59636e", "border": "#d1d9e0", "code": "#f6f8fa", "accent": "#0969da", "syntax": "#8250df" },
    "dark": { "bg": "#0d1117", "surface": "#161b22", "sidebar": "#0d1117", "fg": "#e6edf3", "muted": "#9198a1", "border": "#30363d", "code": "#161b22", "accent": "#2f81f7", "syntax": "#d2a8ff" },
    "sepia": { "bg": "#f3efe4", "surface": "#fbf8ef", "sidebar": "#eee8da", "fg": "#302f2b", "muted": "#6b665b", "border": "#ded7c8", "code": "#eee8da", "accent": "#956d32", "syntax": "#8250df" }
  },
  "previews": {
    "light": "previews/light.svg",
    "dark": "previews/dark.svg",
    "sepia": "previews/sepia.svg"
  }
}
```

- `schemaVersion` 当前为整数 `1`。
- `id` 由 1–48 个小写 ASCII 字母、数字或连字符组成，且不能以连字符开头；目录名与 `id` 相同。同一主题目录中不得出现重复 ID。
- `name` 为 1–48 个字符；`version` 为非空版本字符串；`author` 可选。
- `colors` 必须分别提供 `light`、`dark`、`sepia` 三种模式。每种模式包含 `bg`、`surface`、`sidebar`、`fg`、`muted`、`border`、`code`、`accent`、`syntax` 十六进制颜色值（`#RRGGBB`）。
- `previews` 必须提供三个不同文件路径，分别对应 `light`、`dark`、`sepia`。图片应展示主题应用在 Aline 编辑器界面后的样式，不使用通用色板代替。
- 预览图片支持 SVG、PNG、JPEG、WebP。SVG 应为自包含图像，不引用远程资源。

## ZIP 校验与导入

导入器必须在写入主题目录前完成全部检查，失败时不留下半安装的目录：

1. 文件扩展名为 `.zip`，压缩包可读取且不是 ZIP64；主题清单位于 ZIP 根目录或单层主题目录内。
2. 拒绝 POSIX 或 Windows 绝对路径、反斜杠路径、包含 `..` 的路径、重复文件条目及重复主题 ID。
3. 压缩包不超过 5 MB，全部文件解压后总量不超过 5 MB，单张预览图不超过 1 MB。
4. `theme.json` 可解析，版本为受支持的 schema，ID、名称、颜色字段和三张预览图字段均有效。
5. 三张预览图路径互不相同；文件存在、格式与扩展名匹配并且可被图像解码。
6. 校验通过后，将主题目录原子性地安装到主题文件夹；若 ID 已存在，提示冲突并保留原目录，由用户先移除或改名后重试。

## 主题文件夹与同步

所有内置主题和用户主题统一放在应用主题目录下，每个主题各占一个子目录。安装或首次启动时，将应用随包提供的内置主题复制到缺失的位置；升级时保留用户主题与已存在的主题目录，不静默覆盖用户修改。设置页的“打开主题文件夹”打开该目录；“刷新”重新扫描子目录及清单，增删主题后更新设置页和快速主题菜单。

平台默认位置：

- macOS：`~/Library/Application Support/Aline/themes/`
- Windows：`%APPDATA%/Aline/themes/`
- Linux：`$XDG_CONFIG_HOME/aline/themes/`；未设置时使用 `~/.config/aline/themes/`

## 原型实现边界

当前 UI 原型通过 `prototype/themes/` 提供四套内置主题与预览图，并动态读取该目录。设置页卡片只显示当前模式的预览图，点击后可在大图预览中切换另外两种模式；三张图片仍是主题包校验要求，但图库不重复显示该要求。浏览器原型通过新标签直接打开 `prototype/themes/` 目录；ZIP 导入以浏览器本地存储模拟安装。正式实现按本规范调用系统文件管理器直接打开应用数据目录，并执行校验、写入及刷新。
