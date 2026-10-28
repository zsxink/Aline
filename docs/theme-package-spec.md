# Aline 主题包规范

本文规定自定义主题包可声明的样式范围、字段继承、文件布局和导入校验。主题是静态数据，不包含脚本、任意 CSS 或可执行插件。

## 配置格式裁决

主题配置采用 **JSON，拆成四个职责文件**：

| 文件 | 负责内容 |
|---|---|
| `theme.json` | 主题身份、版本、默认继承主题、配置文件路径和三种模式的预览图路径 |
| `markdown.json` | Markdown 正文与块样式、字体排版、代码块、表格、引用等内容区规则 |
| `application.json` | 应用窗口与界面色彩、UI 字体、透明度、工具栏/菜单图标和文件树图标 |
| `highlight.json` | 代码块高亮颜色（背景、文字、行号及语法 token），可由代码高亮插件独立于应用界面/Markdown 模式选择 |

Rust + GPUI 本身不规定主题配置格式；GPUI 负责把解析完成的类型化 token 绘制到界面，配置解析留在不依赖 GPUI 的 app-core。Serde 同时支持 JSON 和 TOML，格式选择应由主题包分发、编辑和原型共用方式决定。这里选 JSON：主题 ZIP 会被 Rust 导入器和浏览器原型读取，JSON 可由两边原生解析，且 Zed 的官方自定义主题也采用 JSON，便于对照生态实践（[Serde 数据格式](https://serde.rs/#data-formats)、[TOML crate 文档](https://docs.rs/toml/latest/toml/)、[Zed 自定义主题文档](https://zed.dev/docs/themes)）。代价是标准 JSON 不支持注释和尾逗号，手工编辑不如 TOML 轻松；因此规范使用明确字段名、样例和校验错误，不用一份巨大的全量文件。

拆分配置按职责分区：主题元信息与资源、Markdown 内容呈现、应用外壳呈现、代码高亮配色。高亮配色独立出来，是因为代码高亮插件负责语法识别与语义区间，而插件设置允许代码主题与模式独立选择；用户可以组合 GitHub 界面、暗色高亮模式与 Vue 高亮主题。`highlight.json` 可覆盖代码块背景、文字、边框和行号颜色，以保证独立选择暗色高亮时对比度正确；代码块的字体、字号、行高、间距、圆角与尺寸仍属于 `markdown.json`。

## 主题包目录

```text
my-theme/
├── theme.json
├── markdown.json
├── application.json
├── highlight.json
├── fonts/                         # 可选；主题自带字体资源
│   └── ExampleSans-Regular.ttf
├── licenses/                      # 随字体附带的授权文本
│   └── ExampleSans-OFL.txt
└── previews/
    ├── light.svg
    ├── dark.svg
    └── sepia.svg
```

`theme.json`、`markdown.json`、`application.json`、`highlight.json` 和三张预览图都必须存在。三个配置文件可以只写 `{}`，也可以只写部分区块、字段或模式值；未定义值按各自默认规则继承。`theme.json.files` 必须分别给出三个配置文件的路径。`markdown.json` 与 `application.json` 的缺省值继承 GitHub；`highlight.json` 的缺省值继承 GitHub 高亮方案。代码高亮插件可以把代码主题设为“随应用切换”或固定某个主题包，也可以把高亮模式设为“随应用切换”或固定亮/暗/护眼；当任一项固定时，该项不受应用主题家族或模式变化影响。字体资源目录和清单中的字体列表可省略；自带字体时，清单需登记每个字体和对应授权文件。V1 的 `inherits` 只能为 `github`，省略时也默认为 `github`，不支持主题之间互相继承。配置文件、字体和预览图路径相对于 `theme.json` 所在目录。ZIP 可直接包含这些文件，也可将整个主题目录放在 ZIP 根目录下一层，目录名须与主题 `id` 一致。

## 可配置项目

### 1. Markdown 与编辑区样式（`markdown.json`）

- 字体族：正文、标题、等宽代码和应用 UI 字体分别设置有序回退列表，按列表顺序选择可用字体。可引用系统字体、通用族（如 `system-ui`、`sans-serif`、`monospace`）、应用内置字体或主题包登记的字体。
- 字号、字重、行高、文字颜色和对齐方式：正文及各级标题可分别配置；对齐支持 `left`、`center`、`right`、`justify`。
- 段落与块间距：段后间距、标题前后间距、列表项间距、引用/代码/表格块的外边距。
- Markdown 元素：正文、H1–H6、链接、粗体、斜体、删除线、行内代码、引用、列表、任务项、分隔线、图片说明和错误/提示块的语义颜色与排版。
- 代码块：背景、前景、边框、圆角、内边距、字体、字号、行高、行号槽颜色/背景。行号始终显示，不由主题关闭；围栏代码和缩进代码使用同一套代码块样式。
- 代码块的字体、字号、行高、内边距、圆角和尺寸由本文件控制；背景、前景、边框、行号颜色及语义类别颜色可由 `highlight.json` 覆盖，并由代码高亮插件设置中的“代码主题”和“代码高亮模式”解析。未覆盖的代码表面字段回退到当前 Markdown 主题，未识别的语法类别回退到解析后的代码块前景色。
- 表格：表头背景/前景/字重、单元格背景、可选斑马纹、边框、单元格内边距、行高。单元格左/中/右对齐仍以 Markdown 表格语法为准。
- 内容组件：引用线、提示块、公式与图表的背景/前景等颜色；不允许主题改变 Markdown 语义、编辑行为或插件开关。

### 2. 代码高亮语义配色（`highlight.json`）

- 按语言无关的语义类别配置颜色，例如注释、关键字、字符串、数字、函数、类型、操作符、标点、标签、属性、变量、插入和删除。
- 可选配置 `codeBlock` 的 `background`、`color`、`border`、`lineNumberColor` 和 `lineNumberBackground`，让独立高亮模式拥有匹配的代码表面与可读对比度；不配置时沿用 Markdown 主题中的对应颜色。
- 支持 `common` 和 `modes.light/dark/sepia` 覆盖。代码字体、行高、内边距、圆角和布局仍由 `markdown.json` 控制。
- 代码高亮插件可独立选择高亮主题及模式，例如 GitHub 界面 + 暗色高亮模式 + Vue 高亮主题。选“随应用”的主题解析为当前界面主题家族，选“随应用”的模式解析为当前应用模式。各字段缺省时以 GitHub 高亮方案在解析后模式下逐字段补齐。

### 3. 应用界面样式（`application.json`）

- 界面字体族和字号，以及窗口背景、侧栏、工具栏、标签栏、状态栏、弹出菜单、输入控件、选中态、悬停态、边框和强调色等语义颜色。
- 窗口背景透明度（`0.0`–`1.0`）。这是窗口背景表面的 alpha，不得降低窗口内文字、图标或控件整体不透明度；平台/窗口不支持透明背景时以相同颜色的不透明背景显示。
- 图标外观：使用应用已内置并明确支持的图标样式枚举、描边宽度和语义色。工具栏、菜单、状态栏按语义色着色。
- 文件树图标：文件夹关闭/展开、Markdown 文件、图片文件和其他文件可分别设置样式枚举与颜色。图标几何来自应用内置图标集，主题只选样式与色彩。

### 字段清单

配置文件均使用 `common` 和可选的 `modes.light`、`modes.dark`、`modes.sepia`。每个模式只写要覆盖的字段。下表定义主题允许写入的字段；未列出的键必须拒绝。

| 文件 / 路径 | 可设置字段 |
|---|---|
| `theme.json` | `schemaVersion`、`id`、`name`、`version`、`author`、`inherits`、必需的 `files.markdown`/`files.application`/`files.highlight`、必需的三模式 `previews`、可选的 `fonts[]` |
| `theme.json` → `fonts[]` | 每项包含 `path`、字体元数据中的 `family`、`license`、`licenseFile`；字体静态样式与字重从字体文件元数据读取，不重复写入清单 |
| `markdown.json` → `colors` | `body`、`muted`、`subtle`、`link`、`selection`、`caret`、`error`、`errorBackground` |
| `markdown.json` → `typography` | `bodyFamilies`、`headingFamilies`、`codeFamilies`、`bodySize`、`bodyWeight`、`lineHeight`、`bodyAlign` |
| `markdown.json` → `headings.h1`…`h6` | `families`、`size`、`weight`、`lineHeight`、`color`、`align`、`marginBefore`、`marginAfter` |
| `markdown.json` → `spacing` | `paragraphAfter`、`listItemGap`、`blockBefore`、`blockAfter`、`quotePadding` |
| `markdown.json` → `inline` | `strong.weight`/`strong.color`、`emphasis.italic`/`emphasis.color`、`strike.enabled`/`strike.color`、`link.color`/`link.underline`、`codeSpan.background`/`codeSpan.color`/`codeSpan.radius` |
| `markdown.json` → `list`、`blockquote`、`task`、`rule`、`image`、`callout`、`formula`、`diagram` | 各自 `color`、`background`、`border`；列表另有 `markerColor`，任务项另有 `accent`，引用另有 `lineColor`，图像说明另有 `captionColor`，提示块按 `note`、`tip`、`important`、`warning`、`caution` 分别设值 |
| `markdown.json` → `codeBlock` | `background`、`color`、`border`、`radius`、`paddingX`、`paddingY`、`fontSize`、`lineHeight`、`lineNumberColor`、`lineNumberBackground` |
| `markdown.json` → `table` | `headerBackground`、`headerColor`、`headerWeight`、`cellBackground`、`alternateBackground`、`zebra`、`border`、`cellPaddingX`、`cellPaddingY`、`rowLineHeight` |
| `highlight.json` → `syntax` | `comment`、`keyword`、`string`、`number`、`function`、`type`、`operator`、`punctuation`、`tag`、`attribute`、`variable`、`macro`、`inserted`、`deleted` |
| `highlight.json` → `codeBlock` | `background`、`color`、`border`、`lineNumberColor`、`lineNumberBackground` |
| `application.json` → `typography` | `uiFamilies`、`uiSize`、`uiWeight` |
| `application.json` → `colors` | `window`、`editor`、`surface`、`sidebar`、`toolbar`、`tabBar`、`statusBar`、`menu`、`input`、`text`、`muted`、`accent`、`border`、`selection`、`hover`、`focus`、`danger` |
| `application.json` → `window` | `backgroundOpacity` |
| `application.json` → `icons` | `style`、`strokeWidth`、语义色 `default`、`muted`、`accent`、`active`、`disabled`、`danger` |
| `application.json` → `fileTree` | `folderClosed`、`folderOpen`、`markdownFile`、`imageFile`、`otherFile`；每类仅有 `style` 和 `color` |

数值边界：字号 10–72 逻辑像素（UI 字号另限 11–20）、行高 1.0–3.0 倍、段落/块间距 0–128 逻辑像素、字重为 100–900 的百位值、图标描边宽度 0.75–3.0 逻辑像素、圆角 0–16 逻辑像素、透明度 0.0–1.0。字体族字段为有序字符串数组，单项长度不超过 128 字符；字体族名必须在系统字体、应用内置字体或当前主题已登记字体中。布尔项只接受 `true`/`false`；颜色统一使用 `#RRGGBB`。对齐枚举为 `left`、`center`、`right`、`justify`；图标样式枚举为 `outline`、`filled`，如果运行版本未提供某样式则校验失败。

自带字体范围：每个主题最多 8 个静态字体文件，单文件不超过 20 MB，字体总量不超过 80 MB。仅接受 `.ttf`、`.otf` 扩展名，并检查实际字体数据与文件元数据；`family` 必须与文件内的家族名一致，不能通过清单伪造别名。相同家族的常规/粗体/斜体字重可分别打包。可变字体、字体集合（TTC）、Web 字体（WOFF/WOFF2）和字体子集映射暂不支持。每个字体项必须有非空 `license` 和包内 `licenseFile`；导入器校验授权文件存在，主题作者负责确保许可证允许分发。

### 3. 不开放给主题包的项目

- 不接受任意 CSS、脚本、HTML、远程 URL 或任意 SVG 图标。主题字体仅允许本地登记的静态 TTF/OTF 文件，不能从远程下载、不能使用可变字体；字体注册需通过 GPUI 文本系统的受控加载入口。
- 不允许覆盖命令、快捷键、菜单项、文件操作、编辑器布局算法、窗口控件位置、面板尺寸下限、滚动/选择/焦点状态机、无障碍语义和插件配置。
- 不按围栏/缩进语法区分代码块；不允许主题改变 Markdown 解析结果或源文本。
- 不提供任意单个组件的宽高、定位、z-index 等布局覆盖。字号、行高、颜色和内容块间距属于主题；窗口布局尺寸仍由应用设置与布局规则控制。

图标使用枚举而非包内任意图形，避免每个主题都需重复携带整套图标，同时保证常用动作含义和视觉一致。字体二进制单独列入允许资源并校验格式、体积与授权；透明度只作用于背景，不作用于整个组件树，避免文字对比度被一并削弱。

## 字段继承与合并

GitHub 是系统默认主题，也是缺省字段的继承基准。主题加载按以下顺序逐字段合并：

1. Aline 内置 GitHub 默认主题的完整配置。
2. 主题自身配置的全局 `common` 值。
3. 当前模式（`light`、`dark` 或 `sepia`）的覆盖值。

对象递归合并到叶子字段；数组整体替换，不按下标拼接。缺失字段继承上一层；`null` 不表示“清空继承值”，而是非法配置。字段存在但类型、枚举、范围或颜色格式不合法时，整包校验失败并指出文件与字段路径，不静默回退。这样主题作者能只覆盖强调色、字体或表头，而不用复制全部默认配置。

合并结果须经过完整性检查：所有必需 token 有效、透明度在范围内、字号/行高为有限正数、间距为有限非负数、字重在支持范围内、对齐方式和图标样式属于受支持枚举。颜色格式使用 `#RRGGBB`；透明度使用独立数值字段，避免多种 alpha 编码混用。

## JSON 结构示例

`theme.json`（清单与资源索引）：

```json
{
  "schemaVersion": 3,
  "id": "my-theme",
  "name": "My Theme",
  "version": "1.0.0",
  "author": "Theme author",
  "inherits": "github",
  "files": {
    "markdown": "markdown.json",
    "application": "application.json",
    "highlight": "highlight.json"
  },
  "fonts": [
    {
      "path": "fonts/ExampleSans-Regular.ttf",
      "family": "Example Sans",
      "license": "OFL-1.1",
      "licenseFile": "licenses/ExampleSans-OFL.txt"
    }
  ],
  "previews": {
    "light": "previews/light.svg",
    "dark": "previews/dark.svg",
    "sepia": "previews/sepia.svg"
  }
}
```

`markdown.json` 展示模式级覆盖可以很精简；未列出的所有内容都继承 GitHub：

```json
{
  "common": {
    "typography": {
      "bodyFamilies": ["system-ui", "sans-serif"],
      "headingFamilies": ["system-ui", "sans-serif"],
      "codeFamilies": ["SFMono-Regular", "Consolas", "monospace"],
      "bodySize": 16,
      "lineHeight": 1.65,
      "bodyAlign": "left"
    },
    "spacing": { "paragraphAfter": 12 },
    "headings": {
      "h1": { "size": 32, "weight": 700, "marginBefore": 28, "marginAfter": 14 },
      "h2": { "size": 24, "weight": 600, "marginBefore": 24, "marginAfter": 12 }
    },
    "codeBlock": { "fontSize": 14, "lineHeight": 1.6, "lineNumberColor": "#6e7781", "lineNumberBackground": "#f6f8fa" },
    "table": { "cellPaddingX": 12, "cellPaddingY": 8, "zebra": false }
  },
  "modes": {
    "light": {
      "colors": { "body": "#24292f", "link": "#0969da" },
      "codeBlock": { "background": "#f6f8fa" },
      "table": { "headerBackground": "#f6f8fa", "border": "#d1d9e0" }
    },
    "dark": {
      "colors": { "body": "#e6edf3", "link": "#4493f8" },
      "codeBlock": { "background": "#161b22" }
    },
    "sepia": {}
  }
}
```

`highlight.json` 可由用户单独选择，不跟随界面主题家族切换：

```json
{
  "common": { "syntax": { "comment": "#6a737d", "keyword": "#8e44ad", "string": "#1f883d", "function": "#8250df", "type": "#0550ae" } },
  "modes": {
    "dark": { "syntax": { "comment": "#8b949e", "keyword": "#ff7b72", "string": "#a5d6ff" } },
    "sepia": {}
  }
}
```

`application.json` 可独立覆盖应用界面，并可分别定义三种模式：

```json
{
  "common": {
    "typography": { "uiFamilies": ["system-ui", "sans-serif"], "uiSize": 13 },
    "icons": { "style": "outline", "strokeWidth": 1.7 },
    "fileTree": {
      "folderClosed": { "style": "outline", "color": "#bf8700" },
      "folderOpen": { "style": "outline", "color": "#bf8700" },
      "markdownFile": { "style": "outline", "color": "#0969da" },
      "otherFile": { "style": "outline", "color": "#6e7781" }
    }
  },
  "modes": {
    "light": {
      "colors": { "window": "#ffffff", "sidebar": "#f6f8fa", "toolbar": "#ffffff", "text": "#1f2328", "accent": "#0969da", "border": "#d1d9e0" },
      "window": { "backgroundOpacity": 1.0 }
    },
    "dark": {
      "colors": { "window": "#0d1117", "sidebar": "#0d1117", "toolbar": "#161b22", "text": "#e6edf3", "accent": "#4493f8", "border": "#30363d" },
      "window": { "backgroundOpacity": 1.0 }
    },
    "sepia": {}
  }
}
```

示例中的数字字号/间距单位为逻辑像素；`lineHeight` 为行高倍数；`weight` 使用 100–900 的百位字重。正式 schema 会为每个对象定义严格字段 allowlist、类型、范围和枚举，未列出的字段必须拒绝，避免拼写错误被误认为成功覆盖。

## ZIP 校验与导入

导入器必须在写入主题目录前完成所有检查，失败时不留下半安装目录：

1. ZIP 可读取且不是 ZIP64；清单位于 ZIP 根目录或单层主题目录。
2. 拒绝 POSIX/Windows 绝对路径、反斜杠路径、包含 `..` 的路径、重复文件条目和重复主题 ID。
3. 压缩包不超过 100 MB，解压总量不超过 120 MB，单张预览图不超过 1 MB；字体文件数量、单文件体积与合计体积遵循本规范上限。
4. `theme.json` 为标准 JSON，schema 受支持，元信息、引用路径和三张预览图字段有效。
5. `theme.json.files` 必须分别引用 `markdown.json`、`application.json`、`highlight.json`；三个文件都必须存在、可解析为标准 JSON，并通过字段 allowlist、类型、范围、枚举和继承后完整性验证。配置路径须在主题目录内，不允许目录穿越。文件内容可以是空对象，字段缺省由对应继承规则填充；高亮方案单独继承 GitHub 高亮配置。
6. `fonts[]` 若存在，字体路径及 `licenseFile` 必须位于主题目录内；每个文件实际格式、扩展名、内嵌家族名、授权字段及体积都有效。拒绝损坏、加密、可变字体、TTC 和不支持格式。
7. 三张预览图路径互不相同；文件存在、格式与扩展名匹配且能解码。SVG 必须自包含，禁止脚本、外部引用和事件处理器。
8. 校验通过后原子性安装；若 ID 已存在，保留现有目录并提示冲突，用户移除或改名后再重试。

## 主题文件夹与同步

所有内置和用户主题统一存放在应用主题目录，每个主题各占一个子目录。安装或首次启动时，将随包提供的内置主题复制到缺失位置；升级保留用户主题及已有目录，不静默覆盖用户修改。设置页的“打开主题文件夹”打开目录；“刷新”重新扫描清单并更新设置页和快速主题菜单。

- macOS：`~/Library/Application Support/Aline/themes/`
- Windows：`%APPDATA%/Aline/themes/`
- Linux：`$XDG_CONFIG_HOME/aline/themes/`；未设置时为 `~/.config/aline/themes/`

## 版本与原型边界

主题包目标 schema 为 `3`。浏览器原型读取 Markdown、应用界面和独立高亮配色三个职责配置文件，并将配置应用到原型的 Markdown 区域、应用外壳、代码块表面和语法颜色；ZIP 导入校验字段、路径、体积、预览图与登记字体，并用 IndexedDB 保存二进制字体和预览资源。原型只模拟安装与应用效果，不代表 GPUI 字体注册入口已在锁定版本验证。旧版 schema 2 的本地原型主题会把 `markdown.json` 中已保存的语法色迁移到高亮配置；schema 2 ZIP 包需按本规范重新打包后导入。

GPUI 当前主线的文本系统提供按字节注册字体并失效字体/行布局缓存的能力（见 [GPUI `TextSystem::add_fonts`](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/text_system.rs)）。Aline 锁定的是较早的 `gpui-pre` 快照，具体注册入口必须在实现时针对锁定版本确认；主题包规范不把 GPUI 类型暴露到 app-core。字体先由 app 层读取并校验，UI 层注册到文本系统，再刷新窗口排版。若锁定版本不支持运行时添加字体，需在 M4 实现前补齐兼容能力或暂缓自带字体激活。
