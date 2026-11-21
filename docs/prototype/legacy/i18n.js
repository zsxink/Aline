/* Aline 原型界面语言切换：中文 → 英文。文档内容、文件名与用户导入主题的自带文案不翻译。 */
(() => {
  'use strict';

  const STORAGE_KEY = 'aline.prototype.locale';
  const PREFERENCES = new Set(['system', 'zh-CN', 'en']);
  const ATTRS = ['title', 'aria-label', 'placeholder', 'alt', 'data-toast', 'data-full'];

  /* 静态界面文案：以现有中文为键，避免改写原型的可读模板。 */
  const EN = {
    // 窗口、工具栏与标题栏
    'Aline — UI 原型': 'Aline — UI Prototype',
    'macOS 系统菜单栏': 'macOS system menu bar',
    '最小化': 'Minimize',
    '最大化': 'Maximize',
    '关闭': 'Close',
    '切换侧边栏': 'Toggle sidebar',
    '最近打开的文件': 'Recent Files',
    '最近打开的文件夹': 'Recent Folders',
    '清除历史记录': 'Clear History',
    '打开文件夹': 'Open Folder',
    '打开文件': 'Open File',
    '新建文件': 'New File',
    '保存': 'Save',
    '导出': 'Export',
    '设置': 'Settings',
    '设置…': 'Settings…',
    '打开设置': 'Open settings',
    '切换主题': 'Switch theme',
    '粗体': 'Bold',
    '斜体': 'Italic',
    '删除线': 'Strikethrough',
    '行内代码': 'Inline Code',
    '标题 1': 'Heading 1',
    '标题 2': 'Heading 2',
    '标题 3': 'Heading 3',
    '标题 4': 'Heading 4',
    '标题 5': 'Heading 5',
    '标题 6': 'Heading 6',
    '引用': 'Quote',
    '链接': 'Link',
    '无序列表': 'Bulleted List',
    '有序列表': 'Numbered List',
    '分隔线': 'Horizontal Rule',
    '代码块': 'Code Block',
    '插入图片': 'Insert Image',
    '新增 Frontmatter': 'Add Frontmatter',
    '所见即所得': 'Rendered View',
    '源码模式': 'Source View',
    '专注模式': 'Focus Mode',
    '新建标签': 'New Tab',
    '新建标签页': 'New Tab',
    '打开的文件列表': 'Open file list',
    '文件': 'File',
    '大纲': 'Outline',
    '开始写作 — 输入即所得': 'Start writing — what you type is what you see',
    '快速切换主题': 'Quick theme switch',
    '默认': 'Default',
    'Markdown 示例': 'Markdown Demo',
    'Markdown 文档': 'Markdown document',
    'Markdown 源码': 'Markdown source',
    '未保存': 'Unsaved',
    '已复制': 'Copied',
    '已保存': 'Saved',
    '至少保留一个文档': 'Keep at least one document open',
    '当前文档无标题': 'No headings in this document',
    '没有打开的文档': 'No document open',
    '全部关闭': 'Close All',
    '没有打开的标签': 'No tabs open',
    '保存原型会话（⌘S）': 'Save prototype session (⌘S)',
    '已保存到此浏览器的原型会话': 'Saved to this browser prototype session',
    '本组设置已恢复默认': 'This section has been reset to defaults',
    '浏览器存储空间不足，请复制内容后保存。': 'Browser storage is full. Copy your content before saving.',
    '示例素材：': 'Example asset: ',
    '收起所有文件夹': 'Collapse all folders',
    '展开所有文件夹': 'Expand all folders',
    '收起所有子文件夹': 'Collapse all subfolders',
    '展开所有子文件夹': 'Expand all subfolders',
    '刷新': 'Refresh',
    '新建文件夹': 'New Folder',
    '更改代码语言': 'Change code language',
    '复制代码': 'Copy code',
    '搜索代码语言': 'Search code languages',
    '搜索或输入语言…': 'Search or type a language…',
    '代码语言': 'Code language',
    '未列出的语言将展示为纯文本': 'Unlisted languages are shown as plain text',
    '切换': 'Toggle',
    '主题模式': 'Theme mode',

    // 设置外壳与通用页
    '偏好设置': 'Preferences',
    '搜索设置': 'Search settings',
    '应用设置': 'Application Settings',
    '插件设置': 'Plugin Settings',
    '通用': 'General',
    '外观': 'Appearance',
    '编辑器': 'Editor',
    '图片': 'Images',
    '高级': 'Advanced',
    '插件': 'Plugins',
    '原型': 'Prototype',
    '更改会自动保存': 'Changes are saved automatically',
    '关闭设置': 'Close settings',
    '快捷键一览': 'Keyboard Shortcuts',
    '关闭快捷键': 'Close keyboard shortcuts',
    '完成': 'Done',
    '一个专注于写作的 Markdown 编辑器': 'A Markdown editor focused on writing',
    '版本 0.1.0 · UI 原型': 'Version 0.1.0 · UI prototype',
    '文件与应用': 'File & App',
    '格式与插入': 'Format & Insert',
    '视图与标签': 'View & Tabs',
    '一级标题': 'Heading 1',
    '二级标题': 'Heading 2',
    '插入链接': 'Insert Link',
    '切换编辑模式': 'Toggle Editing Mode',
    '下一个标签': 'Next Tab',
    '上一个标签': 'Previous Tab',
    '管理文件行为与启动方式。': 'Manage file behavior and startup.',
    '语言': 'Language',
    '界面语言': 'Interface Language',
    '默认跟随系统；文档内容与文件名不会自动翻译。': 'Follows the system by default. Document content and file names are not translated.',
    '跟随系统': 'Follow System',
    '自动保存': 'Auto Save',
    '编辑时自动保存到文件系统': 'Automatically save edits to the file system',
    '自动保存间隔': 'Auto Save Interval',
    '文件有修改时才会执行保存　默认 30 秒 · 范围 1–2147483': 'Saves only when the file changes. Default 30 s · Range 1–2147483',
    '秒': 's',
    '显示不支持格式的文件': 'Show Files with Unsupported Formats',
    '关闭后文件树仅显示 Markdown 与图片': 'When off, the file tree shows only Markdown and images',
    '启动': 'Startup',
    '恢复上次工作区': 'Restore Last Workspace',
    '启动时打开上次使用的文件夹': 'Open the last used folder on startup',
    '恢复默认': 'Reset to Defaults',

    // 外观
    '调整界面配色与阅读舒适度。': 'Adjust interface colors and reading comfort.',
    '模式': 'Mode',
    '白天': 'Light',
    '夜间': 'Dark',
    '护眼': 'Sepia',
    '随系统切换': 'Follow System Appearance',
    '根据系统外观自动切换白天或夜间；手动切换模式时自动关闭。': 'Switch between Light and Dark with the system. Choosing a mode manually turns this off.',
    '字体与排版': 'Typography',
    '正文字号': 'Body Font Size',
    '行高': 'Line Height',
    '1.5 · 紧凑': '1.5 · Compact',
    '1.6 · 标准': '1.6 · Standard',
    '1.7 · 舒展': '1.7 · Comfortable',
    '1.8 · 宽松': '1.8 · Loose',
    '2.0 · 双倍': '2.0 · Double',
    '界面': 'Interface',
    '默认展开侧边栏': 'Expand Sidebar by Default',
    '工具栏提示': 'Toolbar Tooltips',
    '主题配置': 'Theme Configuration',

    // 编辑器
    '调整写作与代码编辑体验。': 'Adjust writing and code editing.',
    '编辑行为': 'Editing',
    '拼写检查': 'Spell Check',
    '自动换行': 'Word Wrap',
    '长行在所见即所得与源码模式中自动折行': 'Wrap long lines in Rendered View and Source View',
    '代码块行号': 'Code Block Line Numbers',
    '在代码块左侧显示行号': 'Show line numbers to the left of code blocks',
    '代码块自动换行': 'Code Block Word Wrap',

    // 图片
    '统一图片保存位置、引用方式与命名。': 'Unify image storage, references, and naming.',
    '存储位置与应用范围': 'Storage and Scope',
    '插入图片时': 'When Inserting Images',
    '剪贴板图片始终保存到这里；本地和网络图片可分别选择是否保存': 'Clipboard images are always saved here; local and network images can be saved separately.',
    '复制到指定路径（默认）': 'Copy to the specified path (default)',
    '复制到当前文件夹 ./（和文档同级）': 'Copy to the current folder ./ (next to the document)',
    '复制到 ./${filename}-images': 'Copy to ./${filename}-images',
    '指定路径': 'Specified Path',
    '支持相对路径（相对于文档）、绝对路径、Windows 盘符路径和 UNC 路径': 'Supports relative paths (to the document), absolute paths, Windows drive paths, and UNC paths.',
    '选择文件夹…': 'Choose Folder…',
    '对本地图片应用': 'Apply to Local Images',
    '复制到上述位置并更新引用路径': 'Copy to the location above and update the reference path',
    '对网络图片应用': 'Apply to Network Images',
    '下载到上述位置并更新引用路径': 'Download to the location above and update the reference path',
    '引用路径': 'Reference Path',
    'Markdown 引用样式': 'Markdown Reference Style',
    '插入图片时使用的路径格式': 'Path format used when inserting images',
    '相对路径（推荐）': 'Relative Path (Recommended)',
    '绝对路径': 'Absolute Path',
    '剪贴板图片命名': 'Clipboard Image Naming',
    '文件名模板': 'File Name Template',
    '扩展名自动保留原始格式，重名时追加序号。': 'The original extension is preserved; duplicate names get a numeric suffix.',
    '变量：${filename} 当前文档名 · ${date:yyyyMMdd} 日期 · ${time:HHmmss} 时间': 'Variables: ${filename} document name · ${date:yyyyMMdd} date · ${time:HHmmss} time',

    // 高级
    '调整文件树、文档处理与图表服务。': 'Adjust the file tree, document handling, and diagram services.',
    '调整文件树与文档处理。': 'Adjust the file tree and document handling.',
    '文件树性能': 'File Tree Performance',
    '忽略目录': 'Ignored Folders',
    '逗号分隔，支持 * 和 ?': 'Comma-separated; supports * and ?',
    '单次加载条目': 'Items per Load',
    '默认 500 项 · 范围 50–5000': 'Default 500 items · Range 50–5000',
    '项': 'items',
    '自动恢复展开深度': 'Auto-Restore Expansion Depth',
    '默认 8 层 · 范围 1–32': 'Default 8 levels · Range 1–32',
    '层': 'levels',
    '文档处理': 'Document Handling',
    '通过文件大小与行数决定默认编辑模式，仍可手动切换到所见即所得。': 'File size and line count determine the default editing mode; you can still switch to Rendered View manually.',
    '所见即所得文档大小': 'Rendered View Document Size',
    '超过此大小的新文档使用源码模式': 'New documents larger than this open in Source View',
    '源码文档大小': 'Source View Document Size',
    '超过此大小时询问是否仍然打开': 'Ask before opening documents larger than this',
    '最大行数': 'Maximum Lines',
    '超过此行数时默认使用源码模式': 'Documents with more lines open in Source View by default',
    '行': 'lines',
    '服务器地址': 'Server Address',
    '使用外部服务器会将图表文本发送给第三方。敏感内容建议使用自建服务器。': 'Using an external server sends diagram text to a third party. Use a self-hosted server for sensitive content.',
    'PlantUML 公共服务器：': 'PlantUML public server: ',
    '输入 PlantUML 服务器地址': 'Enter the PlantUML server URL',

    // 插件
    '在这里启用或停用内置插件；插件的专属选项集中在左侧“插件设置”分组。': 'Enable or disable built-in plugins here; plugin-specific options are under “Plugin Settings” in the sidebar.',
    '内置插件': 'Built-in Plugins',
    '渲染 Markdown 中的 Mermaid 图表': 'Render Mermaid diagrams in Markdown',
    '表格增强': 'Table Enhancements',
    '提供表格插入、行列操作与对齐工具；当前无需单独配置': 'Provides table insertion, row/column operations, and alignment tools; no separate configuration yet',
    '导出为 PDF、Word 和 HTML': 'Export to PDF, Word, and HTML',
    '公式': 'Math',
    '渲染行内和块级数学公式': 'Render inline and block math',
    '代码语言高亮': 'Code Highlighting',
    '为代码块提供语法高亮；专属模式与主题在“插件设置”中配置': 'Provides syntax highlighting for code blocks; configure its modes and themes under “Plugin Settings”',
    '启用 Mermaid': 'Enable Mermaid',
    '启用表格增强': 'Enable table enhancements',
    '启用导出': 'Enable export',
    '启用公式': 'Enable math',
    '启用代码语言高亮': 'Enable code highlighting',

    // Mermaid
    '控制 Mermaid 图表的主题来源与自定义主题；这些设置不改变应用界面的主题。': 'Control Mermaid theme sourcing and custom themes; these settings do not change the application theme.',
    '图表主题': 'Diagram Theme',
    '主题来源': 'Theme Source',
    '跟随应用明暗时只读取白天或夜间状态，不继承应用强调色。': 'When following the app, only the Light or Dark state is used; the accent color is not inherited.',
    '跟随应用明暗模式': 'Follow App Light/Dark',
    '固定主题': 'Fixed Theme',
    '选择内置主题或已导入的 Mermaid 自定义主题。': 'Choose a built-in theme or an imported custom Mermaid theme.',
    'Mermaid 主题来源': 'Mermaid theme source',
    'Mermaid 图表主题': 'Mermaid diagram theme',
    'default · 默认': 'default · Default',
    'dark · 暗色': 'dark · Dark',
    'forest · 森林': 'forest · Forest',
    'neutral · 中性': 'neutral · Neutral',
    '导入主题…': 'Import Theme…',
    '管理自定义主题': 'Manage Custom Themes',

    // 导出
    '分别配置 PDF、Word 与 HTML 导出选项。导出入口由应用设置中的插件开关控制。': 'Configure PDF, Word, and HTML export options. Export entry points are controlled by the plugin switch in application settings.',
    '纸张尺寸': 'Paper Size',
    'PDF 纸张尺寸': 'PDF paper size',
    '页面方向': 'Orientation',
    'PDF 页面方向': 'PDF page orientation',
    '纵向': 'Portrait',
    '横向': 'Landscape',
    '页边距': 'Margins',
    '上、右、下、左': 'Top, right, bottom, left',
    '上': 'Top',
    '右': 'Right',
    '下': 'Bottom',
    '左': 'Left',
    '标题与正文样式': 'Heading and Body Styles',
    'Word 标题与正文样式': 'Word heading and body styles',
    '跟随文档样式': 'Follow Document Style',
    '紧凑': 'Compact',
    '标准': 'Standard',
    '代码块样式': 'Code Block Style',
    'Word 代码块样式': 'Word code block style',
    '跟随文档主题': 'Follow Document Theme',
    '浅色背景': 'Light Background',
    '深色背景': 'Dark Background',
    '纯文本': 'Plain Text',
    '包含样式': 'Include Styles',
    'HTML 包含样式': 'Include HTML styles',
    '导出独立 HTML 页面时嵌入呈现样式': 'Embed presentation styles in standalone HTML exports',
    '图片资源': 'Image Assets',
    'HTML 图片资源处理': 'HTML image asset handling',
    '选择导出 HTML 时的图片处理方式': 'Choose how images are handled when exporting HTML',
    '复制到导出资源目录': 'Copy to an export assets folder',
    '保留原引用路径': 'Keep original reference paths',
    '导出 PDF (.pdf)': 'Export PDF (.pdf)',
    '导出 Word (.docx)': 'Export Word (.docx)',
    '导出 HTML (.html)': 'Export HTML (.html)',
    '打印…': 'Print…',

    // 公式
    '设置公式渲染结果的背景；公式插件关闭时，源码仍可见且可编辑。': 'Set the background of rendered math; when the Math plugin is off, the source remains visible and editable.',
    '公式外观': 'Math Appearance',
    '公式背景': 'Math Background',
    '跟随内容主题': 'Follow Content Theme',
    '透明': 'Transparent',
    '自定义颜色': 'Custom Color',
    '自定义背景色': 'Custom Background Color',
    '用于行内与块级公式的渲染表面。': 'Rendering surface for inline and block math.',

    // 代码语言高亮
    '配置代码高亮插件的专属选项。启用状态在“应用设置 > 插件”中管理。': 'Configure options for the code highlighting plugin. Its enabled state is managed under “Application Settings > Plugins”.',
    '代码块配色': 'Code Block Colors',
    '代码高亮模式': 'Code Highlight Mode',
    '可独立于应用外观选择亮色、暗色或护眼代码配色。': 'Choose a Light, Dark, or Sepia code palette independently of the app appearance.',
    '随应用切换': 'Follow App',
    '亮': 'Light',
    '暗': 'Dark',
    '代码主题': 'Code Theme',
    '选择主题包中的代码语法颜色；也可以跟随当前应用主题。': 'Choose syntax colors from the theme package, or follow the current app theme.',
    '代码主题与高亮模式可分别设置。例如：浅色应用 + 暗色代码高亮。': 'Code theme and highlight mode are independent. For example: Light app + Dark code highlighting.',

    // 主题库
    '打开主题文件夹': 'Open Theme Folder',
    '导入 ZIP': 'Import ZIP',
    '正在读取主题目录…': 'Reading theme folder…',
    '内置': 'Built-in',
    '已导入': 'Imported',
    '预览': 'Preview',
    '关闭预览': 'Close preview',
    '切换到上一种模式': 'Previous mode',
    '切换到下一种模式': 'Next mode',
    '沿用 GitHub Markdown 的清晰标题层级与蓝色链接，表格和代码块用轻量灰底分组。素白画布搭配中性边框，适合长文与技术文档阅读。': 'Clean GitHub Markdown heading hierarchy and blue links, with light gray groupings for tables and code blocks. A plain white canvas with neutral borders suits long-form and technical reading.',
    '以柔和灰白和宽松留白突出正文，标题分层简洁，代码块和表格保持低对比表面。整体减少界面强调，适合笔记、知识整理和长时间阅读。': 'Soft gray-white surfaces and generous whitespace highlight the text; headings stay simple, and code blocks and tables use low-contrast surfaces. Reduced interface emphasis suits notes, knowledge organization, and long reading sessions.',
    '以 Vue 绿作为链接和交互强调色，白色画布搭配清晰标题层级与深色代码区。结构轻快、对比清楚，适合组件文档和开发指南。': 'Vue green accents links and interactions, with a white canvas, clear heading hierarchy, and a dark code area. Lightweight structure and clear contrast suit component docs and developer guides.',
    '用飞书蓝做链接与交互强调，浅灰侧栏和细边框分出内容区域。表格、代码采用克制底色，适合协作说明、流程文档和日常笔记。': 'Feishu blue accents links and interactions, with a light gray sidebar and fine borders separating content. Restrained table and code surfaces suit collaboration notes, process docs, and daily notes.',
    '飞书': 'Feishu',
    '无法读取内置主题索引': 'Could not read the built-in theme index',
    '内置主题索引版本无效': 'Invalid built-in theme index version',
    '预览图片数据与扩展名不匹配': 'Preview image data does not match its extension',
    'SVG 预览必须自包含且不含脚本、外部引用或事件': 'SVG previews must be self-contained and contain no scripts, external references, or events',
    '主题 ZIP 使用了当前原型不支持的压缩方式': 'The theme ZIP uses a compression method this prototype does not support',
    '文件不是有效 ZIP，或 ZIP 目录已损坏': 'The file is not a valid ZIP, or its ZIP directory is damaged',
    '暂不支持 ZIP64 主题包': 'ZIP64 theme packages are not supported yet',
    'ZIP 中央目录格式错误': 'Invalid ZIP central directory format',
    '主题包包含越界路径': 'The theme package contains a path outside the package',
    '主题包解压后超过 120 MB': 'Theme package exceeds 120 MB after extraction',
    '只支持静态 TTF/OTF 字体，不支持 TTC、可变或 Web 字体': 'Only static TTF/OTF fonts are supported; TTC, variable, and web fonts are not',
    '不支持可变字体': 'Variable fonts are not supported',
    '字体缺少有效名称表': 'Font is missing a valid name table',
    '请选择 .zip 格式的主题包': 'Choose a theme package in .zip format',
    '主题 ZIP 大小不能超过 100 MB': 'Theme ZIP size must not exceed 100 MB',
    '主题包缺少 theme.json': 'Theme package is missing theme.json',
    'theme.json 只能放在 ZIP 根目录或单层主题目录中': 'theme.json must be at the ZIP root or in a single top-level theme folder',
    'ZIP 顶层主题目录名必须与主题 id 一致': 'The top-level theme folder name must match the theme id',
    '白天、夜间、护眼必须分别提供三张预览图': 'Light, Dark, and Sepia modes must each provide a distinct preview image',
    '主题字体仅支持 .ttf 与 .otf': 'Theme fonts must be .ttf or .otf',
    '主题字体总量超过 80 MB': 'Theme fonts exceed 80 MB in total',

    // 菜单
    '关于 Aline': 'About Aline',
    '检查更新': 'Check for Updates',
    '服务': 'Services',
    '隐藏 Aline': 'Hide Aline',
    '隐藏其他': 'Hide Others',
    '显示全部': 'Show All',
    '退出 Aline': 'Quit Aline',
    '文件': 'File',
    '打开文件…': 'Open File…',
    '打开文件夹…': 'Open Folder…',
    '关闭文件夹': 'Close Folder',
    '快速切换…': 'Quick Switch…',
    '另存为…': 'Save As…',
    '最近打开的文件': 'Recent Files',
    '最近打开的文件夹': 'Recent Folders',
    '关闭标签': 'Close Tab',
    '关闭标签页': 'Close Tab',
    '关闭窗口': 'Close Window',
    '编辑': 'Edit',
    '撤销': 'Undo',
    '重做': 'Redo',
    '剪切': 'Cut',
    '复制': 'Copy',
    '粘贴': 'Paste',
    '全选': 'Select All',
    '查找…': 'Find…',
    '替换…': 'Replace…',
    '插入': 'Insert',
    'Markdown 链接': 'Markdown Link',
    '标注（Callout）': 'Callout',
    '数学块': 'Math Block',
    '表格': 'Table',
    '脚注': 'Footnote',
    '任务列表': 'Task List',
    '图片…（插入附件）': 'Image… (Insert Attachment)',
    '折叠（Fold）': 'Fold',
    '格式': 'Format',
    '正文': 'Paragraph',
    '高亮': 'Highlight',
    '数学公式（行内）': 'Math (Inline)',
    '注释（%%）': 'Comment (%%)',
    '视图': 'View',
    '切换所见即所得/源码': 'Toggle Rendered/Source View',
    '显示文件树': 'Show File Tree',
    '显示大纲': 'Show Outline',
    '放大': 'Zoom In',
    '缩小': 'Zoom Out',
    '重置缩放': 'Reset Zoom',
    '全屏': 'Enter Full Screen',
    '开发者工具': 'Developer Tools',
    '主题': 'Theme',
    '窗口': 'Window',
    '缩放': 'Zoom',
    '下一个标签页': 'Next Tab',
    '上一个标签页': 'Previous Tab',
    '恢复关闭的标签页': 'Reopen Closed Tab',
    '关闭所有标签页': 'Close All Tabs',
    '帮助': 'Help',
    '打开日志目录': 'Open Log Folder',
    '问题反馈（GitHub Issue）': 'Report an Issue (GitHub)',
    '选择模式': 'Select mode',
    '所有主题': 'All themes',
    '新增链接…': 'Add Link…',
    '文本格式': 'Text Format',
    '段落设置': 'Paragraph',
    '插入': 'Insert',
    '链接…': 'Link…',
    '分隔线': 'Horizontal Rule',
    '图片…': 'Image…',
    '以纯文本形式粘贴': 'Paste as Plain Text',
    '在上方插入行': 'Insert Row Above',
    '在下方插入行': 'Insert Row Below',
    '在左侧插入列': 'Insert Column Left',
    '在右侧插入列': 'Insert Column Right',
    '删除当前行': 'Delete Row',
    '删除当前列': 'Delete Column',
    '删除表格': 'Delete Table',
    '左对齐': 'Align Left',
    '居中对齐': 'Align Center',
    '右对齐': 'Align Right',
    '表头行': 'Header Row',
    '复制到剪切板': 'Copy to Clipboard',
    '另存为': 'Save As',
    '复制路径': 'Copy Path',
    '打开文件所在': 'Open Containing Folder',
    '图片另存为 SVG': 'Save Image as SVG',
    '图片另存为 PNG': 'Save Image as PNG',
    '复制 SVG': 'Copy SVG',
    '复制 PNG': 'Copy PNG',
    '重命名': 'Rename',
    '复制（副本）': 'Duplicate',
    '删除': 'Delete',
    '复制文件路径': 'Copy File Path',
    '复制绝对路径': 'Copy Absolute Path',
    '复制文件夹路径': 'Copy Folder Path',
    '在文件资源管理器中显示': 'Show in File Manager',
    '关闭其他标签页': 'Close Other Tabs'
  };

  /* 动态文案规则：先匹配整句，再翻译捕获到的界面词。 */
  const RULES = [
    [/^行 (\d+), 列 (\d+)$/, m => `Ln ${m[1]}, Col ${m[2]}`],
    [/^(\d+) 字$/, m => `${m[1]} ${m[1] === '1' ? 'word' : 'words'}`],
    [/^(\d+) 行$/, m => `${m[1]} ${m[1] === '1' ? 'line' : 'lines'}`],
    [/^未命名 (\d+) — Aline$/, m => `Untitled ${m[1]} — Aline`],
    [/^未命名 (\d+)$/, m => `Untitled ${m[1]}`],
    [/^关闭 (.+)$/, m => `Close ${translate(m[1])}`],
    [/^新建文件夹 (\d+)$/, m => `New folder ${m[1]}`],
    [/^示例素材：(.+)$/, m => `Example asset: ${m[1]}`],
    [/^(.+)（原型操作）$/, m => `${translate(m[1])} (prototype action)`],
    [/^选择 Mermaid 主题文件（原型操作）$/, () => 'Choose a Mermaid theme file (prototype action)'],
    [/^打开 Mermaid 自定义主题管理（原型操作）$/, () => 'Open Mermaid custom theme management (prototype action)'],
    [/^已加载 (\d+) 个主题$/, m => `${m[1]} ${m[1] === '1' ? 'theme' : 'themes'} loaded`],
    [/^正在检查 (.+)…$/, m => `Checking ${m[1]}…`],
    [/^刷新失败：(.+)$/, m => `Refresh failed: ${translate(m[1])}`],
    [/^导入失败：(.+)$/, m => `Import failed: ${translate(m[1])}`],
    [/^“(.+)”已通过检查并加入主题列表（原型存储）$/, m => `“${m[1]}” passed validation and was added to the theme list (prototype storage)`],
    [/^预览 (.+) 主题，当前为(.+)模式$/, m => `Preview ${translate(m[1])} theme, currently ${translate(m[2])} mode`],
    [/^(.+) (.+)模式界面预览$/, m => `${translate(m[1])} ${translate(m[2])} mode interface preview`],
    [/^(.+) (.+)模式主题预览$/, m => `${translate(m[1])} ${translate(m[2])} mode theme preview`],
    [/^(.+) · (白天|夜间|护眼)$/, m => `${translate(m[1])} · ${translate(m[2])}`],
    [/^(.+)主题为 Markdown 阅读与编辑提供一套协调的界面配色。$/, m => `${translate(m[1])} provides a coordinated interface color scheme for Markdown reading and editing.`],
    [/^当前代码高亮：(.+) · (.+)。可在白天应用界面中单独使用暗色代码块与语法配色。$/, m => `Current code highlighting: ${translate(m[1])} · ${translate(m[2])}. You can use dark code blocks and syntax colors while the app interface is in Light mode.`],
    [/^(.+) 必须是 JSON 对象$/, m => `${m[1]} must be a JSON object`],
    [/^(.+) 不是支持的主题字段$/, m => `${m[1]} is not a supported theme field`],
    [/^(.+) 类型无效$/, m => `${m[1]} has an invalid type`],
    [/^(.+) 字段无效$/, m => `${m[1]} is an invalid field`],
    [/^(.+) 不接受 null$/, m => `${m[1]} must not be null`],
    [/^(.+) 对齐值无效$/, m => `${m[1]} has an invalid alignment value`],
    [/^(.+) 图标样式只支持 outline 或 filled$/, m => `${m[1]} icon style must be outline or filled`],
    [/^(.+) 必须是布尔值$/, m => `${m[1]} must be a boolean`],
    [/^(.+) 颜色必须使用 #RRGGBB$/, m => `${m[1]} color must use #RRGGBB`],
    [/^(.+) 必须是有效的字体族列表$/, m => `${m[1]} must be a valid font family list`],
    [/^(.+) 必须是数值$/, m => `${m[1]} must be a number`],
    [/^(.+) 必须是有限数值$/, m => `${m[1]} must be a finite number`],
    [/^(.+) 范围必须为 ([0-9.]+)–([0-9.]+)$/, m => `${m[1]} must be in the range ${m[2]}–${m[3]}`],
    [/^(.+) 字重必须为 100–900 的百位值$/, m => `${m[1]} weight must be a multiple of 100 from 100 to 900`],
    [/^(.+) 行高必须为 1–3$/, m => `${m[1]} line height must be between 1 and 3`],
    [/^(.+) 透明度必须为 0–1$/, m => `${m[1]} opacity must be between 0 and 1`],
    [/^(.+) 描边宽度必须为 0.75–3$/, m => `${m[1]} stroke width must be between 0.75 and 3`],
    [/^(.+) 圆角必须为 0–16$/, m => `${m[1]} radius must be between 0 and 16`],
    [/^(.+) 标题间距必须为 0–128$/, m => `${m[1]} heading spacing must be between 0 and 128`],
    [/^(.+) 间距必须为 0–128$/, m => `${m[1]} spacing must be between 0 and 128`],
    [/^(.+\.json) 必须是对象$/, m => `${m[1]} must be an object`],
    [/^(.+\.json) 不支持 (.+)$/, m => `${m[1]} does not support ${m[2]}`],
    [/^(.+\.json) 模式无效：(.+)$/, m => `${m[1]} has an invalid mode: ${m[2]}`],
    [/^主题清单无效：(.+)$/, m => `Invalid theme manifest: ${m[1]}`],
    [/^(.+) 的 theme\.json 不支持字段 (.+)$/, m => `theme.json for ${m[1]} does not support field ${m[2]}`],
    [/^(.+) 当前只支持继承 GitHub 默认主题$/, m => `${m[1]} currently only supports inheriting the default GitHub theme`],
    [/^(.+) 必须声明安全的 markdown\.json、application\.json 与 highlight\.json 路径$/, m => `${m[1]} must declare safe paths for markdown.json, application.json, and highlight.json`],
    [/^(.+) 必须提供三种模式各不相同的安全预览路径$/, m => `${m[1]} must provide safe, distinct preview paths for all three modes`],
    [/^(.+) 字体列表最多包含 8 个字体$/, m => `${m[1]} can contain at most 8 fonts`],
    [/^(.+) 的 files 或 previews 包含未知字段$/, m => `${m[1]} files or previews contain unknown fields`],
    [/^(.+) 的字体必须声明安全路径、家族名、许可证和许可证文件$/, m => `Fonts for ${m[1]} must declare a safe path, family, license, and license file`],
    [/^主题 id 已存在：(.+)$/, m => `Theme id already exists: ${m[1]}`],
    [/^主题资源路径无效：(.+)$/, m => `Invalid theme resource path: ${m[1]}`],
    [/^主题包缺少文件：(.+)$/, m => `Theme package is missing file: ${m[1]}`],
    [/^缺少有效的 (.+) 预览图：(.+)$/, m => `Missing a valid ${m[1]} preview: ${m[2]}`],
    [/^预览图格式无效：(.+)$/, m => `Invalid preview image format: ${m[1]}`],
    [/^字体文件超过 20 MB：(.+)$/, m => `Font file exceeds 20 MB: ${m[1]}`],
    [/^字体家族名与文件元数据不符：(.+)$/, m => `Font family name does not match the file metadata: ${m[1]}`],
    [/^字体授权文件为空：(.+)$/, m => `Font license file is empty: ${m[1]}`],
    [/^主题已应用，字体加载失败：(.+)$/, m => `Theme applied, but fonts failed to load: ${translate(m[1])}`],
    [/^ZIP 数据损坏：(.+)$/, m => `Corrupt ZIP data: ${m[1]}`],
    [/^ZIP 数据截断：(.+)$/, m => `Truncated ZIP data: ${m[1]}`],
    [/^ZIP 解压大小不匹配：(.+)$/, m => `ZIP decompressed size mismatch: ${m[1]}`],
    [/^ZIP 文件重复：(.+)$/, m => `Duplicate ZIP file: ${m[1]}`],
    [/^(.+) 缺少 (.+) 预览图$/, m => `${translate(m[1])} is missing the ${translate(m[2])} preview`],
    [/^(.+) 缺少 (.+)$/, m => `${translate(m[1])} is missing ${m[2]}`],
    [/^(.+) 的预览图超过 1 MB$/, m => `The preview for ${m[1]} exceeds 1 MB`]
  ];

  function readPreference() {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (PREFERENCES.has(saved)) return saved;
      if (saved !== null) localStorage.removeItem(STORAGE_KEY);
    } catch (_) { /* 无存储时跟随系统。 */ }
    return 'system';
  }

  function systemLocale() {
    const languages = navigator.languages && navigator.languages.length
      ? navigator.languages
      : [navigator.language || ''];
    return /^zh(-|$)/i.test(languages[0] || '') ? 'zh-CN' : 'en';
  }

  function resolveLocale(value) {
    return value === 'system' ? systemLocale() : value;
  }

  let preference = readPreference();
  let locale = resolveLocale(preference);

  function applyRules(value) {
    for (const [pattern, replacement] of RULES) {
      const match = value.match(pattern);
      if (match) return replacement(match);
    }
    return value;
  }

  function translate(value) {
    if (locale === 'zh-CN') return value;
    const exact = EN[value];
    return exact !== undefined ? exact : applyRules(value);
  }

  function isNameContext(element) {
    return !!element.closest?.('.tab-title, .tab-list-name, .outline-row, .file-row span, .brand-popover > button');
  }

  function isEditorContent(element) {
    const editor = document.getElementById('editor');
    if (!editor || element === editor || !editor.contains(element)) return false;
    return !element.closest('.code-toolbar, .empty-document-state, .placeholder');
  }

  function sourceText(node) {
    const state = node.__alineI18n;
    return state && node.nodeValue === state.out ? state.src : node.nodeValue;
  }

  function translateText(node) {
    const parent = node.parentElement;
    if (!parent) return;
    if (isEditorContent(parent)) return;
    const raw = sourceText(node);
    const core = raw.trim();
    if (!core) return;
    if (parent.tagName === 'OPTION' && !parent.hasAttribute('value')) parent.setAttribute('value', core);
    let outCore;
    if (locale !== 'zh-CN' && core === '文件' && parent.closest('.side-switch')) outCore = 'Files';
    else if (isNameContext(parent)) outCore = locale === 'zh-CN' ? core : applyRules(core);
    else outCore = translate(core);
    if (outCore === core && !node.__alineI18n) return;
    const lead = raw.slice(0, raw.indexOf(core));
    const out = `${lead}${outCore}${raw.slice(lead.length + core.length)}`;
    node.__alineI18n = { src: raw, out };
    if (node.nodeValue !== out) node.nodeValue = out;
  }

  const attrStates = new WeakMap();

  function translateAttribute(element, name) {
    if (!element.hasAttribute?.(name) || isEditorContent(element)) return;
    const current = element.getAttribute(name);
    const states = attrStates.get(element) || {};
    const state = states[name];
    const src = state && current === state.out ? state.src : current;
    const out = translate(src);
    states[name] = { src, out };
    attrStates.set(element, states);
    if (out !== current) element.setAttribute(name, out);
  }

  function walk(node) {
    if (node.nodeType === Node.TEXT_NODE) {
      translateText(node);
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE && node.nodeType !== Node.DOCUMENT_NODE && node.nodeType !== Node.DOCUMENT_FRAGMENT_NODE) return;
    if (node.nodeType === Node.ELEMENT_NODE) {
      if (node.tagName === 'SCRIPT' || node.tagName === 'STYLE' || node.tagName === 'TEXTAREA') return;
      ATTRS.forEach(name => translateAttribute(node, name));
      if (node.id === 'editor') {
        node.querySelectorAll('.code-toolbar, .empty-document-state, .placeholder').forEach(walk);
        return;
      }
    }
    for (const child of node.childNodes) walk(child);
  }

  function syncLanguageSelect() {
    const select = document.querySelector('#language-select');
    if (select && select.value !== preference) select.value = preference;
  }

  function applyAll() {
    document.documentElement.lang = locale;
    walk(document.documentElement);
    syncLanguageSelect();
  }

  function setPreference(value) {
    if (!PREFERENCES.has(value)) return;
    preference = value;
    locale = resolveLocale(preference);
    try { localStorage.setItem(STORAGE_KEY, preference); } catch (_) { /* 原型仍可切换，只是不持久化。 */ }
    applyAll();
  }

  const observer = new MutationObserver(records => {
    for (const record of records) {
      if (record.type === 'characterData') translateText(record.target);
      else if (record.type === 'attributes') translateAttribute(record.target, record.attributeName);
      else record.addedNodes.forEach(node => walk(node));
    }
  });

  function boot() {
    applyAll();
    observer.observe(document.documentElement, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: ATTRS
    });
  }

  document.addEventListener('change', event => {
    const select = event.target.closest?.('#language-select');
    if (select) setPreference(select.value);
  });

  window.AlineI18n = {
    get locale() { return locale; },
    get preference() { return preference; },
    setPreference,
    translate
  };

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot, { once: true });
  else boot();
})();
