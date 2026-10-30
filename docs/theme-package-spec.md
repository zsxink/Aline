# Aline V1 主题包规范

版本：0.2 · 主题决定回写版  
更新日期：2026-10-29  
状态：**TH-01～TH-10 已确认全部采用方案 A 并回写，当前无待确认主题包问题。schemaVersion=1、字段集合与合并/管理行为已确定；数值限额、默认视觉值和原生能力仍待验证，V-01～V-09 均未执行，不表示实现或验收完成。**  
产品基线：[PRD 0.9](./v1-prd.md)。

## 1. 用途、范围、权威与状态

本文定义主题作者、解析器开发者和评审者共同使用的包格式、字段、继承、资源、导入校验、错误和验收契约。不取代 PRD 的范围、架构的模块与数据流、UI 的选择/预览交互、插件的运行行为或 Rust 规范。仓库尚无正式应用代码。

权威按职责划分：[AGENTS.md](../AGENTS.md) 规定铁律与工作规则，[当前 PRD](./v1-prd.md) 规定产品范围与结果，已确认的[架构指引](./v1-architecture.md)、[架构规范](./architecture-guide.md)、[UI 规范](./ui-spec.md)及插件等专项规范约束实现，[验证计划](./v1-validation-plan.md)记录方法、证据状态与待验证边界；决定记录和确认归档用于追踪选择。阅读顺序不是同级规范的裁决优先级，同级资料无法按职责解释的冲突须列出，不能按日期、文件位置或实现便利自行选择。历史设计不自动取得规范效力，已删除目录不作开发必需输入。PL-01～PL-03 已确认方案 A，按当前 PRD/插件规范及[确认归档](./archive/2026-10-29-plugin-decisions.md)承接，不重开。旧稿自称 schema 3、原型使用某字段，都不是产品确认。

| 标记 | 效力 |
|---|---|
| **必须** / 已定契约 | PRD、铁律或必要的正确性/安全边界；不得降低 |
| **已确认格式** | 第 3～8 节字段名、编码、元数据语法、继承和资源规则按 TH 方案 A 采纳；具体实现和未冻结数值仍须验证 |
| **决定 TH-xx** | 第 14 节保留十项已确认摘要与原问题入口，原备选方案归档，不再是待决分支 |
| **待验证 V-xx** | 能力、数值或平台证据；实验不冒充产品问题，确认格式也不等于实验通过 |

第 3～8 节的格式、字段集合、类型/单位及合并细则已由用户按方案 A 采纳，“必须/拒绝”按现行契约执行。本文示例只交付 JSON 文本，不是实际创建的可安装主题包。V-04/V-06/V-09 尚未给出的数值上限、完整默认值和原生能力仍不得当作已验证；M4 前完成带预算的发布格式与夹具，不能将未填预算解释为无限制。

## 2. V1 能力与配置责任

### 2.1 已决定的产品结果

| 来源 | 必须满足的结果 | 本文边界 |
|---|---|---|
| THEME-01/02、PRD §5.13 | GitHub、Notion、Vue、飞书四套各有白天/夜间/护眼；默认 GitHub + 随系统；系统只映射白天/夜间；手选模式关闭跟随，换家族保留模式与跟随 | 模式 key 为 light/dark/sepia，不在包中保存用户跟随开关 |
| THEME-03、APP-05 | 即时影响界面、内容及打开块，保留输入/选区/历史；全局设置、窗口布局独立 | 不让主题声明命令、编辑或窗口状态 |
| THEME-04/05 | 静态四 JSON + 三模式预览；ZIP 导入、打开目录、刷新、大图预览；缺字段逐字段继承 GitHub，数组整体替换，非法字段/类型/范围/资源报错；冲突 ID 不覆盖 | 缺模式配置可以继承，缺预览不能作为合法包安装 |
| THEME-06/07 | 字体、字号/行高/间距、Markdown 块、界面语义色、背景透明度和受支持图标样式；授权静态 TTF/OTF | 字段集、图标枚举及格式细则已确认，见后文；字体/透明度仍待 V-04 |
| THEME-08/09 | 完整校验后安装；失败无半安装且当前主题不变；发现内置及用户主题；首次补缺内置，升级不覆盖用户修改；活动主题失效回可用默认并说明 | 版本升级/删除与内置根规则按 TH-06/07/09，见 §10 |
| PLUGIN-08、APP-05 | 代码主题和高亮模式两维分别跟随或固定；用户显式字号/行高优先，清除后回主题值，分区重置只清本区 | 包不写用户覆盖或插件开关 |
| APP-03、UI-02 | 名称按钮应用、缩略图独立预览；大图切模式不应用；介绍最多五行且可查看全文 | 描述数据需入清单，呈现与焦点归 UI §11 |

### 2.2 样式的唯一责任

| 样式来源 | 主题作者能控制 | 不能控制 |
|---|---|---|
| markdown.json | 正文/源码文字、标题/行内/块样式、普通代码表面和代码排版、内容选区/光标色 | Markdown 语义、源文本、IME/历史、表格对齐语法、行号开关、换行行为、文档列宽/窗口布局 |
| application.json | 应用表面/文字/交互状态语义色、UI 排版、背景透明度、受支持内置图标风格 | 编辑画布另造一套颜色真源、菜单/命令/快捷键、点击区域、面板尺寸、原生窗口控件布局、禁用判定 |
| highlight.json | 独立代码表面与语法类别颜色 | 代码字体/字号/行高/圆角/内边距、语言定义、插件开关、核心 Markdown 语义识别 |
| 插件有效设置 | 公式背景随内容/透明/自定义；Mermaid 自己的静态主题与明暗策略 | 主题包不能替用户修改插件设置；Mermaid 护眼映射亮色，不继承应用强调色 |

内容/UI 共用应用所选家族和模式；没有 PRD 依据开放二者分别挑选不同包。代码配色可以独立组合。导出页面样式与插件关闭后的输出由 V-02 和插件规范负责，不能因包里有颜色就规定导出表现。

不开放 CSS、HTML、脚本、远程 URL、远程字体、任意图标、可执行内容、通用组件宽高/定位/z-index 或主题间任意继承。颜色只改变已存在状态的呈现，不能隐藏危险/错误、禁用焦点、关闭删除线/斜体等语义标记。只读灰色区在三模式保持可辨中性表面（MD-11、UI §7.1）。

## 3. 包布局与解析外壳（TH-01/02/04 已确认）

```text
paper-note/
├── theme.json
├── markdown.json
├── application.json
├── highlight.json
├── previews/
│   ├── light.png
│   ├── dark.png
│   └── sepia.png
├── fonts/                         # 不携带字体则整个目录省略
│   └── ExampleSans-Regular.ttf
└── licenses/                      # 携带字体则需对应授权文本
    └── ExampleSans-OFL.txt
```

一个 ZIP 仅承载一个包：四文件直接置于根，或全部置于一个名为 id 的单层目录；不允许多清单/多主题、额外包装层或散落文件。配置文件名固定，files 显式重复声明以便检查职责，不能改路径重定向到另一文件。fonts/licenses 可选；previews 必需。三配置文件最小内容各为 `{}`，theme.json 不能为空。根 themes/index.json 是原型发现索引，不是包文件，不导入。允许的文件集合为四 JSON、清单登记的预览/字体/授权；不登记或不支持文件拒绝，不默默丢弃。空目录条目可存在但不得借其制造路径别名或文件/目录冲突。

### 3.1 通用 JSON 规则

- UTF-8 标准 JSON；必须拒绝 BOM、注释、尾逗号、重复 key、非对象顶层、非有限数、超预算长度/深度。重复 key 在解析时拒绝，不能先以最后值覆盖后再校验。
- 对象封闭：仅允许本稿逐项列出的 key；所有层级均拒绝未知 key、错误形状和错误类型，不因字段位于未激活模式或未知语言而跳过。
- `null` 不作重置/删除语法；按 TH-03 将其视为错误。空字符串不作“自动”；`{}` 是无覆盖；空数组依所属字段规则，不作继承指令（TH-03）。
- 数值不接受数字字符串或隐式强制转换。大小写敏感 key/枚举；颜色 hex 大小写等价。JSON 转义解码后再校验字符串和资源路径。
- 四文件作为一个版本单元；仅清单声明 schemaVersion，三个配置文件不再声明，避免版本冲突。缺任何必需文件返回 MissingFile，未知字段 UnsupportedField，未知 schema UnsupportedSchema，不能统一成“无法读取”。

### 3.2 公共类型、单位与预算

下表定义后续字段表的缩写，不表示已经新增 Rust 类型。全部数值边界在读入时和继承后检查。

| 类型 | 已确认表达与固有边界 | 未冻结内容 |
|---|---|---|
| Color | 字符串，恰为 `#RRGGBB`，六位 ASCII hex；不接受 #RGB、alpha、颜色名、函数、渐变/URL | alpha 只在 backgroundOpacity 中表达，TH-02 |
| Text | Unicode 字符串，不解释 Markdown/HTML；禁止 NUL，除 description/license 文本外不含控制字符 | 长度按解码后 Unicode 标量数，另有 UTF-8 字节预算；最大值 V-04 |
| Families | 非空 Text 数组，有序回退；元素非空，无 CSS 表达式/URL | 单项/数量/总字节上限 V-04；系统族是否存在由平台解析，非纯 schema 校验 |
| PxPositive | 有限 JSON number，>0，逻辑像素，非物理像素/pt/rem | 最大/最小可用值 V-04/V-09；旧 10–72 等不是现行边界 |
| PxNonNegative | 有限 number，≥0，逻辑像素 | 最大值 V-04/V-09；旧 128/16 等不冻结 |
| Ratio | 有限 number，>0；行高相对该文字自己的字号的倍数 | 可用上下限 V-04/V-09，不抄旧 1–3 |
| Weight | 整数枚举 100/200/300/400/500/600/700/800/900 | 真实字体字重匹配/缺字重效果 V-04；不伪造已注册字重 |
| Align | `left` / `center` / `right` / `justify` | 中文两端对齐/原生布局支持 V-04/V-09；不影响表格语法对齐 |
| Opacity | 有限 number，闭区间 [0,1] | 三平台实际合成 V-04，不支持按同色不透明回退 |
| Path | 非空包内相对路径字符串，规则见 §9 | 字节/段数上限 V-04；不作为宿主绝对路径直接使用 |
| bool | 仅 JSON true/false | 不接受 0/1 或字符串 |

**预算不是无限：**解析/验证必须接受 V-04/V-06 冻结的预算，缺预算不能对外开放“无上限导入”。固有正负约束可先测试；最大字符串/数组/嵌套、字号和布局边界尚无最终数字，M4 前须补齐上下限、含端点/超一单位样本及机器证据。未冻结前不能声称全量字段值验收完成。示例数值只是说明样本，不是默认值或限额。

## 4. schema 与清单元数据（TH-01/02 已确认）

正式 V1 使用 schemaVersion `1`，不自动接受或迁移历史原型 `2/3`；原型包须按本规范重新打包（TH-01）。包 version 是作者资源版本，不能代替 schemaVersion，也不使同 ID 可以覆盖。

| theme.json 字段 | 必需 / 缺省 | 类型、范围与语义 |
|---|---|---|
| schemaVersion | 必需，无默认 | JSON 整数；只接受 1，任何其它值 UnsupportedSchema，不能尝试宽松解析 |
| id | 必需，无默认 | ASCII 小写字母开头，余下仅小写字母/数字/连字符，末位非连字符；单段稳定机器身份；长度预算 V-04。禁止 .、..、平台保留设备名及大小写/规范化碰撞；github/notion/vue/feishu 保留给内置身份 |
| name | 必需，无默认 | 非空 Text；展示名，可重复但 UI 必须借 id/来源区分；不作路径或翻译 key |
| description | 可选，默认空字符串 | 纯文本 Text，允许换行；介绍来源。缺失显示无介绍的本地化状态，不由脚本按 id 编造文案；五行限制是 UI 展示截断，不是数据截断 |
| author | 可选，默认空字符串 | Text；作者自述，不代表签名/可信来源；不作系统权限依据 |
| version | 必需，无默认 | 字符串，格式为 `major.minor.patch` 三个非负十进制整数，不带前导零（0 例外）、前缀/预发布/build；段长预算 V-04；仅报告版本，不自动升级排序 |
| inherits | 可选，默认 github | 仅字符串 `github`；不接受自定义路径/包 id，禁止继承链/环。内置 GitHub 的根配置由应用提供，不能递归继承自己 |
| files | 必需，无默认 | 封闭对象，三个 key 全必需：markdown=`markdown.json`、application=`application.json`、highlight=`highlight.json`；每值是固定 Path |
| previews | 必需，无默认 | 封闭对象 light/dark/sepia，全必需、三路径互异、资源存在；格式见 §8 |
| fonts | 可选，默认 [] | 字体项数组；不从别的主题继承清单或二进制，元素见下表；文件数/总字节 V-04 |

每个 fonts 元素为封闭对象，下列四项均必需且非空：

| 字段 | 类型 / 检查 | 缺失或不符 |
|---|---|---|
| path | Path；位于 fonts/，静态 .ttf/.otf 且实际格式相符；同一路径不能重复登记 | MissingResource / InvalidFont / DuplicateResource |
| family | Text；匹配字体元数据的规范家族名，不作为伪造别名；多语言名称匹配细则经 V-04 固化 | FontMetadataMismatch |
| license | Text；授权标识或许可证名称，不宣称已完成法律审查 | MissingLicense |
| licenseFile | Path；位于 licenses/，非空 UTF-8 .txt 授权全文；可以由同许可的多个字体共享 | MissingLicense / InvalidResource |

字体引用的已确认语义：通用 system-ui/sans-serif/monospace 是保留角色，不能被包内 family 抢占；其它族名按“当前包登记 family → 应用内置 family → 本机系统 family”的来源优先级解析，同来源同 face 歧义拒绝，不访问其它用户包。由宿主将逻辑引用绑定到隔离 face，不能靠全局注册顺序决定命中。该来源顺序已按 TH-02 确认；隔离和命中字形能力属于 V-04，二者不能混为一项技术实验。

同一家族多个静态 face 可分别登记，字重/斜体从二进制读取；相同 family/weight/style 的重复 face 拒绝歧义。不得伪造来源标识；应用保留通用角色名不能登记为包内 family。不同包同家族名的隔离/引用能力必须通过 V-04 证明；不能用后注册覆盖前注册的效果冒充按包选择。所有声明字体均检查，包括当前模式未用的字体。

## 5. 三配置文件的字段契约（TH-02/03/05 已确认）

### 5.1 外壳、缺省与封闭对象

markdown.json、application.json、highlight.json 顶层仅有 `common` 和 `modes`，均可省略，缺省 `{}`。common 是本文件的样式对象；modes 是封闭对象，仅 light/dark/sepia 三个可选 key，各值是同类型样式对象。整个配置 `{}` 合法；模式缺省不表示“不支持此模式”，按 §6 补齐。不能填数组、null 或额外 mode。

以下路径均指 common 或 modes.<mode> 下的路径；父对象和每个叶子均可省略。除明确的“跟随”外，缺省来自**同一职责、同一模式的 GitHub 完整基线，经 common/模式覆盖后的值**，不另设零值默认。表中的 `{a,b}` 是精确 key 展开，`h1…h6` 恰为六个 key，不是任意字符串字典。对象只含表中列出的子字段；未列属性不允许。GitHub 基线必须逐叶完整并自检；其数值/三模式视觉金样须由 V-04/V-09 形成，本轮不抄原型充当默认表。

### 5.2 markdown.json

| 路径 / 精确叶子 | 类型 | 作用与不能改变的行为 |
|---|---|---|
| colors.{background,body,muted,marker,selection,caret,error,errorBackground} | Color | 编辑画布、正文、辅助文字、打开块标记、内容选区/光标、错误前景/底色；仅内容区，不能隐藏标记/错误 |
| typography.{bodyFamilies,codeFamilies} | Families | 正文/代码有序回退；源码正文使用 codeFamilies；标题字体由各级 headings.families 唯一定义，不接受浏览器 CSS 字体栈语法 |
| typography.{bodySize,sourceSize} | PxPositive | 正文/源码字号分别指定；不按标题比例推断正文字号 |
| typography.bodyWeight | Weight | 正文默认字重，不取消 strong 语义 |
| typography.{lineHeight,sourceLineHeight} | Ratio | 正文/源码行高；不能更改输入分组/软换行设置 |
| typography.bodyAlign | Align | 段落文字对齐；不覆盖 Markdown 表格列对齐 |
| headings.h1…h6.{families,size,weight,lineHeight,color,align,marginBefore,marginAfter} | 依次 Families、PxPositive、Weight、Ratio、Color、Align、PxNonNegative、PxNonNegative | 各级标题样式；每级每叶独立继承 GitHub，不隐含“缺省使用本主题 bodySize/bodyFamilies”二次计算；标题层级/大纲结构不变 |
| spacing.{paragraphAfter,listItemGap,blockBefore,blockAfter,quotePadding} | PxNonNegative | 段后、列表项、块默认前后、引用内边距。块默认外距用于未定义专属外距的块；标题 marginBefore/After 替代同侧 blockBefore/After，不叠加。段后仅段落，listItemGap 仅相邻列表项 |
| inline.strong.{weight,color} | Weight、Color | 粗体文字；不得借极低字重抹除语义可辨性，效果验 V-09 |
| inline.emphasis.color / inline.strike.color | Color | 斜体/删除线前景；不开放 italic=false、enabled=false，渲染仍保留相应语义标记 |
| inline.link.{color,underline} | Color、bool | 唯一内容链接色入口，取消旧 colors.link 别名；underline=false 也须保持链接可辨与键盘可达 |
| inline.codeSpan.{background,color,radius} | Color、Color、PxNonNegative | 行内代码表面，不改变文字或选择范围 |
| list.markerColor | Color | 列表标记；不能重写序号或隐藏任务框 |
| blockquote.{color,background,border,lineColor} | Color | 引用正文、背景、外边界、引用竖线；不改变嵌套结构 |
| task.{color,background,border,accent} | Color | 任务文字、框底/边、完成态强调；完成状态取源码 |
| rule.color | Color | 分隔线；删除无意义的 background/border 多重同义字段 |
| image.border | Color | 内嵌图片边界；不缩放像素、不引入未立项的图片说明编辑能力 |
| callout.{note,tip,important,warning,caution}.{color,background,border} | Color | 五类提示块文字、底色、语义边界；类别名称/图形仍由核心提供 |
| readOnlyBlock.{color,background,border} | Color | 脚注/目录/Front matter 中性灰色展示区；不能改为可编辑，不能借语言控件伪装成代码块 |
| formula.{color,background,border} | Color | 公式内容主题默认值；显式插件背景高于 background；不支配 Mermaid 内部颜色 |
| codeBlock.{background,color,border,lineNumberColor,lineNumberBackground} | Color | 当前内容家族/模式的**普通代码**表面；按已确认 PL-02，高亮关闭时使用它，独立代码主题/模式保留暂停，再启用恢复 |
| codeBlock.{radius,paddingX,paddingY} | PxNonNegative | 围栏/缩进统一布局，不新增标题栏/行号开关 |
| codeBlock.fontSize / codeBlock.lineHeight | PxPositive、Ratio | 使用 codeFamilies；独立代码配色不得覆盖 |
| table.{headerBackground,headerColor,cellBackground,alternateBackground,border} | Color | 表头、普通/斑马单元格、网格；zebra=false 时 alternateBackground 合法但暂不用于绘制 |
| table.headerWeight / table.zebra | Weight、bool | 表头字重/交替底色；不增删实际表头或表行 |
| table.{cellPaddingX,cellPaddingY} / table.rowLineHeight | PxNonNegative、Ratio | 单元格水平/垂直内距、文字行高；无固定行高，不裁切内容 |

新增 background/marker/sourceSize/sourceLineHeight/readOnlyBlock 是为唯一画布真源、打开块/源码及只读区补齐责任；它们已按 TH-02 确认；不得额外增设未列字段。删除旧 typography.headingFamilies 冗余角色；各级 headings.families 从同级 GitHub 缺省补齐，打开编辑/提交展示共用同一有效字体，不再出现角色二次覆盖。

### 5.3 highlight.json

| 路径 / 精确叶子 | 类型 | 语义 |
|---|---|---|
| codeBlock.{background,color,border,lineNumberColor,lineNumberBackground} | Color | 独立代码模式的一整套表面；每个缺叶先继承 GitHub **高亮**同模式，不临时回取当前 Markdown 颜色 |
| syntax.{comment,keyword,string,number,function,type,operator,punctuation,tag,attribute,variable,macro,inserted,deleted} | Color | 注释、关键字、字符串、数字、函数、类型、运算符、标点、标签、属性、变量、宏、插入、删除的语义着色；不是任意语言 scope 字典 |

提供者把语言语义映射到有限类别；无匹配/未知类别使用已解析的 codeBlock.color，不能索引未知 key 后 panic。语法识别/类别映射与核心 Markdown 标记着色不由包声明。开启时用独立代码主题和独立高亮模式解析整套高亮颜色，再与内容主题的代码几何/字体组合；不采用旧稿“有时回 Markdown、有时回 GitHub”的双重缺省。此消歧方案已按 TH-02 确认，关闭色源只引用 PL-02，不新增 TH 重问。

### 5.4 application.json

| 路径 / 精确叶子 | 类型 | 作用与边界 |
|---|---|---|
| typography.uiFamilies / typography.uiSize / typography.uiWeight | Families、PxPositive、Weight | 应用自身 UI 文字；不承诺原生系统菜单/标题栏可被任意换字体，能力验 V-01/V-09 |
| colors.{window,surface,sidebar,toolbar,tabBar,statusBar,menu,input} | Color | 窗口背景、通用浮层、侧栏、工具栏、标签/状态栏、应用菜单、输入框表面；编辑画布只读 markdown.colors.background |
| colors.{text,muted,accent,border,selection,hover,focus,danger} | Color | 主要/辅助文字、强调、边界、控件选中/悬停、焦点环、危险状态；状态机和辅助文字/图形仍由应用负责 |
| window.backgroundOpacity | Opacity | 仅窗口背景合成，不乘文字/控件树透明度，不使内容/焦点不可辨；不能控制点击穿透、模糊材质或系统装饰 |
| icons.style / icons.strokeWidth | IconStyle、PxPositive | IconStyle 为 outline/filled，仅应用内置几何；strokeWidth 只用于 outline，filled 下保留合法值但无描边作用；枚举已按 TH-05 确认，描边数值与完整动作/状态覆盖待 V-09 |
| icons.{default,muted,accent,active,disabled,danger} | Color | 操作图标六种语义前景，不改变图标名称/命令含义 |
| fileTree.{folderClosed,folderOpen,markdownFile,imageFile,otherFile}.{style,color} | IconStyle、Color | 核心五类文件树图标的风格/色彩覆盖；缺 style/color 继承 GitHub 该类值，不再隐含继承本包 icons；无任意扩展名→图标映射 |

删除旧 application.colors.editor，避免与内容画布重复；不保留旧 diagram.*，防止应用主题越权覆盖 Mermaid 的独立设置。图标枚举已确认两种风格，仍须完整动作/状态覆盖验收，不代表锁定运行环境已验证可用。

## 6. 继承、合并与优先级（TH-03/07 已确认）

### 6.1 同一职责的解析

既定要求：缺字段逐字段继承 GitHub、数组整体替换、非法值明确报错。按已确认 TH-03，解析顺序唯一确定如下，`D` 是 markdown/application/highlight 之一，`m` 是对应有效模式：

```text
G[D,m] = 受应用保护的 GitHub 完整 common 递归合并其 modes[m]
T[D,m] = G[D,m] 递归合并所选包 common，再递归合并所选包 modes[m]
```

每文件独立解析；不跨职责继承颜色、不从另一个模式取字段、不把当前系统 dark 自动当包缺 sepia 的替代。随应用受保护的 GitHub 自身作为根，不递归读 inherits；可修改磁盘 GitHub 副本不参与其它主题缺省或最终安全回退（TH-07）。显式选择磁盘副本时，将其视为本包覆盖受保护根，保留用户修改。缺模式等同该模式 `{}`，所以只写 dark 的合法包在 light/sepia 下仍可用，但其缺省来自 GitHub 对应模式；三张真实预览仍全需。

| 输入 | 已确认合并结果 |
|---|---|
| 对象 | 递归逐叶；空对象不清除上层，不能把标量替代对象 |
| 数组 | 整体替换，顺序保留，不拼接/按下标补齐；Families=[] 非法；fonts=[] 是合法的无自带字体且不是样式继承 |
| 字段缺失 | 保留上层；不等于空字符串、0、false 或 null |
| bool=false / 数字0 | 只在该字段类型允许时是真实值（例如 zebra=false、paddingX=0）；不用 truthy 判断继承 |
| null / 非法类型/值/枚举 | 整包拒绝，不忽略该叶，不因最后被合法层覆盖而容忍它 |
| 未识别语法类别 | 运行时回高亮 codeBlock.color；不意味着配置允许未知 syntax key |

先验证每层声明，再合并，最后检查三模式有效结果完整性/范围/资源可解析性。JSON 合法不等于包有效。用户主题只提供覆盖，不需要复制 GitHub；**应用提供的根基线**必须完整，缺叶属于应用内置资源缺陷，不能用零值补成成功。

### 6.2 有效样式各来源的组合

1. 应用家族 + 有效模式分别生成内容 T[markdown,m] 和 UI T[application,m]，二者没有独立用户家族选择。
2. 代码家族和模式各自解析“跟随/固定”。用两者生成 T[highlight,codeMode]，只替换代码颜色；代码几何/字体仍取内容主题。按已确认 [PL-02](./plugin-system.md#pl-02--关闭代码语言高亮后的普通代码配色来源)，关闭插件时取当前内容主题普通代码表面，独立代码主题/模式保留暂停；关闭期间切内容主题也更新普通代码，再启用按保留配置与当前跟随维度恢复。
3. 用户已开放且显式设置的排版覆盖最后应用；不创造任意 token 覆盖编辑器。字号/行高已有明确优先级，其他设置仅在其 UI/PRD 已开放时使用。
4. 公式背景：主题默认 → 用户插件选择透明/自定义；随主题时无额外覆盖。Mermaid 独立，只按自身跟随明暗/固定规则计算，sepia 跟随亮色。
5. 系统模式仅在随系统开启时决定 m=light/dark；固定 sepia 不被系统变化覆盖。主题包没有模式状态字段。

### 6.3 两个可唯一求值的例子

**例 A：缺叶、模式覆盖与数组替换。** 为说明算法，假设测试基线 G[markdown,dark].colors.body=#E6EDF3、spacing.paragraphAfter=12、typography.codeFamilies=["Consolas","monospace"]。主题 common 仅设 paragraphAfter=18、codeFamilies=["Example Mono","monospace"]，dark 仅设 colors.body=#DDDDDD。没有用户覆盖时，dark 的三个结果依次为 #DDDDDD、18、整数组 ["Example Mono","monospace"]；没有 Consolas。改看 light 时 body 取 G[markdown,light].colors.body，而段距/字体仍来自 common。例中基线值是测试假设，非新冻结的 GitHub 默认。

**例 B：用户字号/行高覆盖、切换与清除。** 主题 A 的正文 bodySize=16、lineHeight=1.6，B 为 18/1.8，用户明确设置正文 20/1.9。选 A 最终 20/1.9；换 B 仍为 20/1.9，正文源字节、草稿、选区、历史不变；仅清除字号覆盖后为 18/1.9；再清除行高为 18/1.8。若 B 两叶都没写，清除后回 B 按模式从 GitHub 解析的值。若既有设置允许显式字体族，其已设置值也在主题层之后应用，清除则重新解析主题字体列表，不能写回或改包内数组。

补充代码例：GitHub 白天内容 + 固定 Vue 家族 + 固定 dark 高亮；换 Notion/sepia 后代码颜色仍为 Vue/dark，代码字号/行高/间距改随 Notion/sepia，再叠加相关显式排版覆盖；行号始终显示。预览图展示包默认三模式，不声称涵盖用户所有混搭/覆盖组合。

## 7. 用户覆盖、清除与配置发布

**必须：**用户字号/行高覆盖存于应用设置，包文件不是设置文件。清除某项仅移除该显式值，立即按当前主题/模式重算；分区重置仅清该区覆盖，不能删包/清其它插件配置或恢复所有窗口布局。主题切换不清覆盖；安装/刷新不得向包内写入用户值。

包只交付静态数据；解析与继承在 aline-app-core/theme，磁盘/ZIP/字体注册/有效快照发布在 aline-ui 设置服务，app 注入目录/平台服务；plugin 仅消费所需 token。遵循架构 §4.4/8.2/9 与架构规范 R-13-01～05：候选完整可用后一次发布带代次快照，窗口/插件不各读文件。字体、主题内容版本、布局宽度/换行等真实依赖纳入失效键，不能仅用 theme_id；旧任务拒收和资源引用释放归宿主。

切换包含正在编辑块、中文组合输入、其它窗口及后台渲染时，不改源文、草稿、选区、输入分组、撤销/重做、dirty 或保存点，不强制 IME 确认/取消。重排按源码锚点保持可定位；不得让旧任务盖回新颜色。具体 GPUI 注册/缓存操作与调度交由架构及 V-04/V-05/V-07/V-09 验证，不在包格式里设计 Rust API。

## 8. 预览、字体与静态资源

### 8.1 三模式预览（TH-04 已确认）

**已定：**三模式预览必须存在且与实际样式相符；预览只为展示资产，不参与 token/字体解析，不从图片取色或生成运行时配置。名称应用/缩略图预览及大图键盘行为见 UI §11.2，预览模式不写应用当前模式。

V1 包预览仅允许自包含静态 PNG；清单分别引用 previews/ 下三个不同 .png 文件，文件签名/解码类型/扩展名一致，拒绝 APNG、混入其它格式及截断资源。透明像素可存在，但按该模式画布合成展示；不能借透明预览声明窗口透明能力。该格式已按 TH-04 确认；SVG/JPEG/WebP 等不是包预览允许格式。

缺少任一预览/引用不存在/解码失败/超限：导入失败。已安装包外部损坏后刷新不得冒充完整合法包；按 §10/11 处理活动主题失效，列表反馈错误，不用同图换三个标签。未加载期间的占位是 UI 状态，不作为可安装包的缺省资产。

V-04/V-09 用同一真实 Markdown 夹具、同一版本、对应模式及无用户覆盖/默认插件状态生成和对照预览；记录字体/平台/显示缩放/窗口尺寸、预览生成器版本与实际截图，比较画布、标题/正文、代码、表格、只读灰区和状态色。不得仅看文件存在、不能将本轮四套 SVG 当新产品预览已通过。

### 8.2 字体与授权

**已定：**只接受随包登记且附授权文本的静态 TTF/OTF，不接受远程、可变字体、字体集合 TTC、WOFF/WOFF2 或执行内容。实际格式与扩展名一致；读取元数据、静态 face、完整性及预算校验后才交受控注册入口，不调用外部程序安装到系统字体目录。主题作者负责取得可再分发授权；导入器检查声明和文本存在/非空，不把自动检查说成授权合法性的证明。

字体回退区分三件事：

- 系统字体列表中某族本机没有：沿有序列表选下一项；通用 system-ui/sans-serif/monospace 由平台映射。-apple-system/BlinkMacSystemFont 是浏览器历史标识，在本规范原生格式中没有特殊通用族映射；本机无真实同名字体时沿列表回退，建议作者使用原生系统族或真正家族名。
- 字体有效但缺某字形：逐字形使用后续字体/平台中文与符号后备，仍保持可读；匹配、字重、emoji/中文覆盖以及最终后备路径由 V-04 实测，不承诺每个系统族在三平台相同。全链缺字必须可诊断，不能把缺字方框当完成；文档源文本保持原样。
- 包内声明字体损坏/元数据不符/授权缺失/注册失败：资源错误或 FontActivationFailed；不得悄悄跳过并显示导入/应用成功。保留原主题，能力须修复或取得新产品决定，不能自行退化成“仅系统字体”并通过 THEME-07。

没有携带字体的主题不需 fonts/licenses。携带但未使用的字体也须验证；清单和样式引用只认本包、应用内置或系统字体，不得跨用户主题目录偷取二进制。样式中的普通系统族引用不能被当文件路径。

动态注册、失败后注册副作用隔离、同名族隔离、中文回退、主题删除后仍被布局/在途任务引用的 face 释放、主题切换后字体/行布局缓存失效全属 **V-04 未执行**。在证明可安全激活与失败保全前不能发布自带字体导入能力。是否需要最小平台例外依 Rust 规范，本文不授权 unsafe。

### 8.3 透明度、SVG 与外部资源

backgroundOpacity 只改变背景合成，文字、焦点、危险提示不统一变淡。**已决定**不支持透明的平台按原 RGB 同色不透明（opacity=1）回退，显示实际能力/回退说明；**未验证**各平台具体透明能力、合成与刷新，不承诺毛玻璃/点击穿透。

主题包预览仅允许静态 PNG，所有 SVG（含自包含无脚本 SVG）均返回 UnsupportedResource，不进入 SVG 解析器。主题包不提供任意 SVG 图标等其它入口。Markdown/图片 tab/图表导出的 SVG 安全子集、外部引用/脚本/事件/DTD/实体/复杂度限制仍归 V-03/V-06，不因主题包拒绝 SVG 而取消那些安全验证。未来若要扩展包资源格式，须另行取得决定并修订格式、校验和预算，不能将未采纳 TH-04 B 当实现后备。

任何资源引用禁止协议 URL、file:/data:、绝对路径和包外引用，图片 tab/Markdown 支持这些引用不意味着主题包也支持。没有背景图、任意 SVG 图标、网络请求或嵌套压缩包的通用扩展口；未来格式要显式修订 schema/校验和验证样例。

## 9. ZIP 校验、路径安全与安装事务

### 9.1 路径与归档规则

以下是 THEME-08 的必须安全边界；具体 ZIP 实现不得削弱这些检查：

1. 先读受限归档目录，确认一个清单/一个包根和条目类型。拒绝加密、不能完整校验的压缩方法/分卷/损坏结构；ZIP64 是否支持由 V-04 的解包库及计数溢出实验决定，不继承旧“非 ZIP64”作为产品限制。所有累加/乘法/偏移受检，不能只信归档声明大小。
2. 归档名和 Path 均使用 `/`；仅真实目录条目允许一个终止 `/`，先移除该终止符再检查分段，普通文件/清单 Path 不允许终止 `/`；其余空段（含 `a//`）、重复终止符仍拒绝。拒绝 POSIX 根路径、Windows 盘符/UNC/设备前缀、反斜杠、空段、`.`/`..` 段、NUL/控制字符、尾空格/尾点、ADS 冒号和平台保留设备名。路径不做百分号解码，不展开 ~、变量或快捷方式。解码 JSON 转义后同样验证。
3. 拒绝符号链接、硬链接及其它特殊条目，包括目标看似包内的链接；普通文件/目录之外不解压。对已有主题目录扫描也拒绝链接/重解析点越界，不能只检查 ZIP。
4. 拒绝重复原始路径、以 Unicode NFC 规范化并做不依赖语言的大小写折叠后等价的碰撞、文件/目录同名及祖先文件冲突（如 fonts 是文件又有 fonts/a.ttf）。不能以“当前文件系统区分大小写”放过跨平台歧义；规范化比较不能把恶意路径改成合法后继续。跨平台 Unicode/保留名检查样本由 V-01/V-04 验证。
5. 校验每个实际写目标仍处于本次隔离根内；不跟随宿主已有符号链接/重解析点，不信任字符串前缀判断。隔离目录由应用私有创建，不将 ZIP 权限/可执行位/所有者写入宿主；读取、验证到安装期间防路径替换竞争，不把先 canonicalize 再随意打开称为安全闭环。
6. 只允许 §3 文件集合与结构；配置/预览/字体/授权引用全量交叉检查，不同职责配置不得同路径，字体不可伪装为图像。未登记文件包括 .DS_Store/__MACOSX 不静默跳过，明确提示重新打包（封闭包结构按 TH-02 已确认）。

### 9.2 校验与发布顺序

```text
选择 ZIP → 目录/路径/压缩与条目预算预检 → 私有隔离区受限读取/解压
→ theme.json 解析与 schema/身份/资源声明 → 三 JSON 全字段/三模式继承校验
→ 授权/资源存在、类型/元数据/大小 → 预览受限解码、字体受限完整性校验
→ 字体激活能力准备/可用性确认 → 重验 ID/目录/资源与取消状态
→ 不覆盖地发布完整目录 → 发布注册表 → 报告安装成功
```

预算要在分配/解压/解析/解码的过程中持续执行，不能全部解压后才验“总大小”。字体注册前不能跳过授权或恶意预览检查。可独立验证部分的具体并行实现不在本稿冻结；对用户的成功点必须在完整验证/安装/有效注册之后。按已确认 TH-08，安装成功只更新可用列表，不自动应用，当前家族/模式/跟随及用户覆盖保持，须由名称按钮显式应用；预览本身绝不应用。

- 导入任务只写私有 staging 和本次新目录；冲突 ID 无论 version 高低、内容是否相同都拒绝，不改原目录，不把“重复”报安装成功。不把改 ZIP 文件名当更换 id。
- 校验/字体准备/取消失败，回收本次临时材料，活动主题、已存在包及注册表保持原样。失败的字体准备不能污染当前字体选择或缓存；具体可回滚/隔离能力由 V-04 证明，不宣称一次 rename 能撤销字体注册。
- 发布前再次确认 ID 空闲，采用不覆盖的安装语义；并发导入同 ID 只能一个成功。不能先删目标再 rename，也不允许局部拷贝目录被扫描成可用包。
- 安装目录发布成功但注册表发布失败，要撤销/隔离该新目录，不能留下可被后续扫描误认为已成功的半安装。清理失败独立返回诊断并标记不可发现，重启先清理/隔离再扫描；原主题仍可用。
- 崩溃/权限/磁盘满/取消在每一步故障注入。启动扫描只接纳完整验证的包，隔离区永不算主题。磁盘已完成的操作须结算真实结果；不得只丢回调假装回滚。目录原子发布/崩溃恢复与字体激活不是一项跨资源原子事务，V-04 必须交付实际保证及恢复记录方案。

### 9.3 资源限额账本（全部待验证）

| 预算 | 单位/执行点 | 证据与超限行为 |
|---|---|---|
| ZIP 包体、目录大小、条目数、路径长度、压缩比 | bytes/entries/segments/ratio；读取/目录预检与逐条解压 | V-04；拒绝解压炸弹、溢出和不可信声明，原包与主题不变 |
| 解压总量、单文件量、JSON/授权文本量 | 实际 bytes，持续计数；解压/读取时 | V-04/V-06；到界限终止，无完整解压后追认 |
| 字符串标量/字节数、数组项数、JSON 嵌套与对象成员 | scalars/bytes/items/depth/members；JSON 流程 | V-04/V-06；类型化 ResourceLimit，指出字段/预算 |
| 预览单图/总量、宽高/像素乘积、解码内存 | bytes/pixels/bytes；图像头预检及解码中 | V-04/V-06；拒绝而非有损缩图后当原包有效 |
| 字体数、单字体/总量、face/表复杂度 | files/bytes/faces/steps；元数据/完整性/激活前 | V-04/V-06；不加载超限字体，无部分字体“成功” |
| 验证/解析/解码/字体准备时间与工作量、取消、任务并发 | duration/steps/jobs；全过程，真实终止/结算 | V-04/V-05/V-06；不只是超时后隐藏结果，不能卡死 UI |
| 字号/行高/间距/圆角/描边/界面字体范围 | 逻辑 px/ratio；字段及合并结果 | V-04/V-09；建立极端布局/中英文夹具后冻结允许范围 |

每项冻结产物含数值、边界是否包含、负责模块、测量环境/版本、内存/时间、拒绝错误与恢复路径。旧 100/120 MB、1 MB 预览、8 字体/20 MB 单个/80 MB 总量、128 字符、10–72/11–20 px 等均不作为当前限额；可供实验比较，不能默认复制。尚无数值意味着发布阻塞仍在，不意味着导入无限制。

## 10. 发现、冲突、升级、刷新与删除

| 场景 | 已确认契约 | 待验证 |
|---|---|---|
| 目录发现 | app 目录服务注入路径，UI 扫描/校验用户可打开的主题目录；编译期内置根与磁盘副本分开管理，不写回源码仓库 | V-01/V-04 实际三平台路径与来源身份 |
| 继承与安全回退根（TH-07） | 随应用受保护 GitHub 是所有缺省和最终回退的唯一根；磁盘 github 副本的修改只在显式选择该副本时作为本包覆盖，不影响其他主题缺省。磁盘副本全部损坏仍有可用根 | 根默认三模式完整性、版本变化回归与说明 V-04/V-09 |
| 首次启动/内置缺失 | 补齐缺失内置包；已有修改、同名损坏目录不静默覆盖/删除，损坏不当成目录缺失。受保护根升级独立于磁盘副本，保留用户修改 | V-04 补缺/损坏/升级故障 |
| 同 ID/重复导入（TH-06） | 无论 version、相同内容或保留内置 ID，普通导入均冲突拒绝，不覆盖。V1 无单独覆盖升级命令；作者更换 ID，或用户备份后目录手工替换完整包再刷新 | V-04 同 ID 并发、手工替换/刷新失败恢复 |
| 同名不同 ID | 展示名可重复，以稳定 ID/来源区分，不按名称覆盖 | V-09 列表/菜单同名区分 |
| 安装成功（TH-08） | 只更新可用列表，不自动应用；保留当前家族/模式/跟随及覆盖。名称按钮显式应用，缩略图预览不应用 | V-04/V-09 安装与应用的分别反馈 |
| 手工修改与刷新 | 校验完整包后一次发布注册表；仍有效且正在使用的同一包刷新可更新其样式。不得按文件依次发布，资源变化推进代次；不新增自动监听/notify | V-04 字体准备/缓存/旧任务/刷新失败恢复 |
| 非活动包损坏 | 保留磁盘内容，以错误条目隔离，不加入可选择注册表；其他主题继续可用 | V-04 局部错误不丢整表 |
| 当前家族/固定代码包删除或失效（TH-10） | 保留文档与原选择身份，有效呈现回受保护默认并说明；界面明确“原选择失效，当前使用默认”，不能冒充原包仍生效。固定代码模式仍保留，缺失家族暂用默认对应代码模式，不擅自改为跟随 | V-04/V-09 全窗状态/固定维度一致 |
| 修复/重装/重新出现（TH-10） | 刷新仅将包恢复为可选择，不自动重新应用；重启也不绕过显式选择。用户显式选择后才解除失效回退状态；失效后改选其它主题时旧身份不再是当前意图，修复不得盖回新选择 | V-04/V-09 修复、重启及新选择交错 |
| 删除（TH-09） | V1 无应用内删除按钮；通过打开目录手工移除，再刷新。应用不提供删除事务或擅自删磁盘包；外部删除后的字体/在途引用仍由宿主结算 | V-04 删除后刷新/仍有引用的资源生命周期 |
| 安装/刷新 I/O 或清理失败 | 区分真实阶段结果，不报伪成功；安装失败保持原包/当前主题，无可发现半安装；刷新读取失败不将原包误报删除，确认内容失效才安全回退 | §9 的 V-04 故障/崩溃恢复证据 |

打开目录、刷新/安装/应用反馈、来源标识与键盘路径按 UI §11.2；包不能用 builtIn/author/id 自证官方身份。磁盘内置副本由应用可信来源记录识别，不能仅凭保留 ID 导入冒名。失效选择身份是应用设置，根/字体/主题版本是派生依赖，不能混存进包清单。以上决定不等于平台安装、注册与恢复实现已经完成。

## 11. 错误类别、用户反馈与降级

下面是诊断语义，不冻结 Rust 枚举/API 名；app-core 返回类型化错误，UI 用 t("key")/zh-CN/en 映射。诊断至少含类别、文件、JSON Pointer（字段错误）、资源相对路径（资源错误）、预期/实际类型或范围、可采取的修复动作；JSON 语法错误给行列。多错误可受限汇总，不能为收集所有错误耗尽资源；不泄露资源内容/宿主敏感路径。

| 类别 | 例子 | 用户可观察结果 / 修复方向 |
|---|---|---|
| ArchiveInvalid / UnsafePath | ZIP 损坏、链接、穿越、路径碰撞 | 拒绝安装，指向条目及重新打包方向；不解压到包外 |
| MissingFile / MissingField | 缺 highlight.json / previews.sepia | 说明缺文件或必需 key，补齐后重新导入；不回退成空文件安装 |
| JsonSyntax / DuplicateKey | 尾逗号/同对象重复 id | 指文件和位置；修复 JSON，不称版本不兼容 |
| UnsupportedSchema | 旧/未来/非整数 schemaVersion | 给出声明值与当前支持版本；按受支持规范重新打包，不自动迁移 |
| UnsupportedField / UnsupportedValue | 拼写错误/未知模式/不支持图标枚举 | 定位字段并列允许项；不是忽略后成功 |
| InvalidType / OutOfRange | bodySize 字符串、负 padding/alpha>1、预算外数值 | 指预期类型/单位/边界；保留原主题 |
| MissingResource / InvalidResource / UnsupportedResource | 不存在图片、损坏 PNG、未支持 SVG | 指资源/格式，补齐或转换为允许格式；不能导入时用占位冒充合法 |
| MissingLicense / InvalidFont / FontMetadataMismatch | 无授权、损坏/可变字体、伪造 family | 补授权或正确静态 face；不静默回系统字体 |
| FontActivationFailed / FontGlyphMissing | 注册失败/回退链仍无字形 | 激活失败不发布候选；缺字说明且保源，修复字体/回退链；动态/逐字形诊断能力待 V-04 |
| ResourceLimit / ValidationTimeout | ZIP/像素/字节/步数/时间超限 | 指预算及实际触发位置，终止真实工作并回收隔离材料；无截断安装 |
| IdConflict / DuplicateResource | 同 ID/重复 face 或资源路径 | 显示稳定 ID/来源，原包不变；按 TH-06 不提供覆盖升级命令，用户备份后目录手工替换/刷新或作者更换 ID，改显示名不能解决 |
| InstallIo / CleanupFailed / RefreshFailed / DeleteFailed | 无权限/磁盘满/外改/占用 | 成功、失败、取消分开；可重试，残留不可发现且说明；不得先破坏原主题 |
| ActiveThemeInvalid / TransparencyFallback | 运行期包失效/平台无透明 | 前者回可用默认并说明，后者按同 RGB 不透明及实际能力反馈；均保文档/输入/历史 |

字体缺字的诊断应按字体/回退链汇总，避免逐字形刷屏或在每键路径重复做全量字体检查；呈现及测量由 V-04/V-09 收口。

只提供一种模式配置不是错误：三个模式都按 GitHub 补齐并有各自预览。但只提供一张预览是缺资源错误。导入失败保持当前选择；运行期原活动包失效才走默认回退，二者不能混同。主题切换途中候选失败保留上一有效主题，旧渲染任务不得复活候选；当前已失效主题也不能永久冒充正常可用。

## 12. 完整示例与最小包（格式已确认，未创建真实资源）

下面依次是同一个 paper-note 包的四个完整 JSON 文件，路径对应 §3，省略叶子按 §6 继承；不是所有叶子显式填写的 GitHub 根基线。格式/字段已按 TH-01～05 确认，三张 PNG 与可选字体/授权须由真实包作者提供。本轮只维护示例文本，不创建资源或可安装包；数值样本尚不能代替 V-04/V-09 的范围与真实渲染验收。

**theme.json：**

```json
{
  "schemaVersion": 1,
  "id": "paper-note",
  "name": "Paper Note",
  "description": "适合长文阅读的主题。\n提供白天、夜间和护眼预览。",
  "author": "Theme author",
  "version": "1.0.0",
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
    "light": "previews/light.png",
    "dark": "previews/dark.png",
    "sepia": "previews/sepia.png"
  }
}
```

**markdown.json：**

```json
{
  "common": {
    "typography": {
      "bodyFamilies": ["Example Sans", "system-ui", "sans-serif"],
      "codeFamilies": ["Consolas", "monospace"],
      "bodySize": 16,
      "sourceSize": 14,
      "bodyWeight": 400,
      "lineHeight": 1.65,
      "sourceLineHeight": 1.6,
      "bodyAlign": "left"
    },
    "headings": {
      "h1": {"size": 32, "weight": 700, "marginBefore": 24, "marginAfter": 12}
    },
    "spacing": {"paragraphAfter": 12, "listItemGap": 4},
    "inline": {
      "strong": {"weight": 700},
      "emphasis": {"color": "#303030"},
      "strike": {"color": "#666666"},
      "link": {"underline": true},
      "codeSpan": {"radius": 4}
    },
    "list": {"markerColor": "#666666"},
    "blockquote": {"lineColor": "#777777"},
    "task": {"accent": "#0969DA"},
    "rule": {"color": "#D1D9E0"},
    "image": {"border": "#D1D9E0"},
    "callout": {"note": {"border": "#0969DA"}},
    "readOnlyBlock": {"border": "#999999"},
    "formula": {"border": "#D1D9E0"},
    "codeBlock": {"fontSize": 14, "lineHeight": 1.6, "paddingX": 16, "paddingY": 12, "radius": 4},
    "table": {"zebra": false, "cellPaddingX": 12, "cellPaddingY": 8, "rowLineHeight": 1.6}
  },
  "modes": {
    "light": {"colors": {"background": "#FFFFFF", "body": "#303030"}},
    "dark": {"colors": {"background": "#181818", "body": "#EEEEEE"}, "inline": {"emphasis": {"color": "#EEEEEE"}}},
    "sepia": {"colors": {"background": "#F5F0E5", "body": "#38342C"}}
  }
}
```

**application.json：**

```json
{
  "common": {
    "typography": {"uiFamilies": ["system-ui", "sans-serif"], "uiSize": 13, "uiWeight": 400},
    "icons": {"style": "outline", "strokeWidth": 1.7},
    "fileTree": {"markdownFile": {"style": "outline", "color": "#0969DA"}},
    "window": {"backgroundOpacity": 1}
  },
  "modes": {
    "light": {"colors": {"window": "#FFFFFF", "accent": "#0969DA"}},
    "dark": {"colors": {"window": "#181818", "accent": "#80BFFF"}},
    "sepia": {"colors": {"window": "#F5F0E5", "accent": "#805C28"}}
  }
}
```

**highlight.json：**

```json
{
  "common": {},
  "modes": {
    "light": {"codeBlock": {"background": "#F6F8FA", "color": "#303030"}, "syntax": {"comment": "#59636E", "keyword": "#8250DF"}},
    "dark": {"codeBlock": {"background": "#202020", "color": "#EEEEEE"}, "syntax": {"comment": "#AAAAAA", "keyword": "#D2A8FF"}},
    "sepia": {"codeBlock": {"background": "#EEE8DA", "color": "#38342C"}, "syntax": {"comment": "#6B665B", "keyword": "#805C28"}}
  }
}
```

**最小合法包的清单**如下；markdown.json、application.json、highlight.json 的完整内容各自都是 `{}`，另有三张合法且反映 GitHub 缺省三模式的 PNG；没有 fonts/licenses。不是省略那三个文件，也不从 GitHub 复制预览路径引用到包外。

```json
{
  "schemaVersion": 1,
  "id": "minimal-note",
  "name": "Minimal Note",
  "version": "1.0.0",
  "files": {
    "markdown": "markdown.json",
    "application": "application.json",
    "highlight": "highlight.json"
  },
  "previews": {
    "light": "previews/light.png",
    "dark": "previews/dark.png",
    "sepia": "previews/sepia.png"
  }
}
```

## 13. 测试矩阵与需求追踪

本表是**实现计划，未执行产品测试**。解析/继承/诊断无头检查归 aline-app-core/theme，ZIP/目录/字体/发布/多窗归 aline-ui，平台服务归 app，代码着色组合由 plugin/highlight 消费。模块名沿架构草图，不冻结 API。每行实际夹具需记录文件/操作/预期/实际/平台；TH 已按 A 确认，使用本稿确定期望，不从实现输出反录期望。

| 编号 | 字段/规则与输入场景 | 必须检查的结果 / 待验证 | PRD / AC | V / 主责 |
|---|---|---|---|---|
| TH-T01 | §3/4/12 完整包、最小包、四内置各三模式 | 四文件/三预览、静态资源、完整解析可用；内置不能只交一套；按 TH-01/02/04/05 已确认规则制作真实夹具 | THEME-01/04/05，AC-12/13 | V-04/09；core + UI |
| TH-T02 | 缺必需文件/key、未知字段/模式、错误形状/类型、重复 JSON key | 定位字段/文件，整包失败，无宽松强转；不是缺省继承 | THEME-04/05/08，AC-13 | V-04；core |
| TH-T03 | 未知/负/小数 schema、旧 2/3，包版本合法/非法 | UnsupportedSchema 与元数据错误区分，不自动迁移；TH-01 | THEME-05/08，AC-13 | V-04；core |
| TH-T04 | 所有字段数值边界、枚举、颜色格式、空/超长字符串、极深/多成员 JSON | 逐类型正反例，类型/值/预算分开；上下限由实验填入，不能用仅 >0 的测试宣称完成 | THEME-05/06/08，AC-13/16 | V-04/06/09；core + UI |
| TH-T05 | §6 对象/数组、{}、缺模式、false/0、null、空 Families | 例 A 精确求值，数组不拼接，非法被后层覆盖仍拒绝；TH-03/07 | THEME-05，AC-12/13 | V-04；core |
| TH-T06 | 用户字号/行高覆盖→换主题→逐项清除/分区重置，多窗口 | 例 B 精确求值，只清本区、包不变，设置共享/布局独立 | APP-05、THEME-03，AC-12/20 | V-04/09；core + UI |
| TH-T07 | 系统明暗/固定三模式/换家族/随系统状态 | 系统不选 sepia，手选关跟随，家族不改模式/跟随；状态入口一致 | THEME-01/02、APP-03，AC-12/15 | V-04/09；UI |
| TH-T08 | 代码家族×模式分别跟随/固定，公式背景/独立 Mermaid；高亮开关 | 高亮表面同切，内容排版不被覆盖；缺色只用对应高亮基线；关闭回内容主题表面，再启用恢复独立配置（PL-02 已确认）；TH-02 | PLUGIN-05/07/08、THEME-03/05，AC-11/12 | V-02/03/04/09；core + plugin + UI |
| TH-T09 | 重复/保留 ID、相同内容再导入、同名异 ID、并发同 ID | 冲突零覆盖，名称不当身份；原包可用；按 TH-06 无覆盖升级命令 | THEME-05/08/09，AC-13 | V-04；core + UI |
| TH-T10 | 内置缺失/被修改/损坏、应用升级、默认副本失效 | 缺失补齐、修改不静默覆盖、可用根回退；TH-07；保留原目录证据 | THEME-09，AC-12/13 | V-01/04；app + UI |
| TH-T11 | 外改后刷新、资源只改字节同 ID/version、删除/刷新失败 | 完整候选一次发布，新资源代次失效旧缓存；活动失效回默认有提示；按 TH-10 保留失效身份、修复不自动应用 | THEME-03/09、APP-05，AC-12/13/20 | V-04/09；UI |
| TH-T12 | ZIP 绝对/.. /反斜杠/驱动器/UNC/ADS/设备名、链接/重解析点、目录替换竞争 | 包外零写入、链接零跟随，隔离目录边界受检 | THEME-07/08，AC-13/16 | V-01/04/06；UI |
| TH-T13 | 重复路径/Unicode或大小写碰撞/祖先文件冲突/额外文件/多根 | 一律拒绝对应歧义，原包及注册表不变 | THEME-05/08，AC-13 | V-04；core + UI |
| TH-T14 | 远程 URL、file:/data:、CSS/脚本/可执行/任意图标、未登记资源 | 无网络/外部进程/包外读取，给路径/格式错误 | THEME-06～08，AC-13 | V-04/06；core + UI |
| TH-T15 | 炸弹/伪造大小/超大单文件/过多文件/超字体总量、超时取消 | 解压/解析/解码前中均有预算，实际停止，非仅隐藏结果；无部分安装 | THEME-08，AC-13/16 | V-04/05/06；UI |
| TH-T16 | 静态 TTF/OTF、损坏/可变/TTC/WOFF、伪造 family、缺/空授权、重复 face | 合法真实激活；非法拒绝并分类；失败不假装生效 | THEME-07/08，AC-13 | V-04/06；core + UI/app |
| TH-T17 | 中英/emoji 缺字、系统族缺失、同名系统族/包内族/跨包同族、换字体/删除仍有引用 | 按已确认包内→内置→系统顺序绑定，列表/逐字形回退可读、缺字可诊断、无跨包串字体/悬空引用；不以浏览器 FontFace 为证据 | THEME-03/07，AC-03/12/13 | V-01/04/09；UI/app |
| TH-T18 | 缺预览、损坏/伪扩展、超宽高/像素/内存、APNG/非法 SVG | 缺资源/格式/超限分开；按 TH-04 仅静态 PNG，SVG/APNG/JPEG/WebP 全拒绝，不运行 SVG 解析器 | THEME-04/07/08，AC-13 | V-03/04/06；UI |
| TH-T19 | 三预览与真实同模式渲染、介绍缺省/长文本、名称/缩略图键盘 | 真实内容/字体/表面一致；最多五行可看全文；预览切模式不应用；非同图换标签 | THEME-01/04、APP-03，AC-12/15 | V-04/09；UI |
| TH-T20 | opacity 0/1/越界、平台支持/不支持、亮暗/护眼 | 只合成背景，文字/焦点不消失；不支持同 RGB 不透明且说明，三平台记录 | THEME-06/07，AC-12/13 | V-01/04/09；UI/app |
| TH-T21 | 校验/解码/授权/字体准备/安装/注册表/清理每阶段失败与崩溃 | 原目录、当前主题/文档不变，无可发现半安装；残留隔离、重启可恢复，成功点真实 | THEME-08、SAFE-02，AC-01/13 | V-04；UI/app |
| TH-T22 | 切换/刷新时 IME、打开块、undo/redo、未保存输入、旧任务晚到 | bytes/草稿/选区/历史/dirty 前后不变，候选不被提交、旧结果不回挂，锚点保持可定位 | THEME-03、EDIT-05/06、SAFE-02，AC-01/03/04/12 | V-04/05/07/09；UI + plugin |
| TH-T23 | 导入成功后选择/不选择、当前/非当前/固定代码包删除，失效修复/重启、取消/失败 | 按 TH-08/09/10：安装不应用、无应用内删除、失效身份保留且修复/重启不自动应用；既定导入失败不改当前、失效回默认始终保留 | THEME-03/08/09，AC-12/13 | V-04/09；UI |

字段覆盖要求：§4/5 每个叶子至少一个合法和一个非法值；每种对象形状、必需 key、数组整体替换均有断言。当前仅完成文档 JSON 语法/字段一致性与引用静态检查，不存在 Rust 解析器、真实字体/PNG、ZIP 恶意包或三平台测试成绩。M1 做 V-04 字体实验，M4 实现前须按已确认格式补齐限额/支持矩阵，M6 完成平台发布门禁。

## 14. 待确认问题清单

2026-10-29 用户答复“待确认的问题采纳你的建议”，TH-01～TH-10 全部采用建议方案 A，**当前无待确认主题包问题，十项决定阻塞均已解除**。保留本节标题及各问题标题供既有链接使用；确认依据、回写矩阵与首轮完整问题/备选方案见[主题包确认归档](./archive/2026-10-29-theme-decisions.md)。

### TH-01 · 正式 schema 版本与原型兼容身份

**已确认 A：**正式 schemaVersion=1；不自动接受或迁移原型 2/3，作者按现行格式重新打包。

**回写与验收：**主题 §3/4/12、PRD THEME-04/08；TH-T01/03，V-04。决定已采纳，相关实现/实验尚未完成。

### TH-02 · 是否采用本稿封闭字段集与唯一色源

**已确认 A：**采用封闭字段表与唯一色源；补介绍/画布/源码/灰区，删除别名/冗余角色/语义开关/diagram；字体通用角色保留，其它族按当前包→应用内置→系统解析。

**回写与验收：**主题 §3～5/8、UI §3、架构 §8.2；TH-T01～09/17，V-04/09。决定已采纳，相关实现/实验尚未完成。

### TH-03 · common/模式的对象合并与空值语义

**已确认 A：**GitHub 同职责同模式→包 common→包本模式；对象逐叶递归、数组整体替换、null 拒绝、空对象不清除，不做未定义二次角色联动。

**回写与验收：**主题 §5/6、架构规范 §13；TH-T05/06，V-04。决定已采纳，相关实现/实验尚未完成。

### TH-04 · 包预览的允许格式

**已确认 A：**包预览仅自包含静态 PNG，三模式各一张；拒绝 APNG/SVG/JPEG/WebP 等，Markdown 与图片 tab 的格式范围不变。

**回写与验收：**主题 §3/8/9、PRD THEME-04/08、UI §11；TH-T18/19，V-04/06/09。决定已采纳，相关实现/实验尚未完成。

### TH-05 · 受支持内置图标样式集合

**已确认 A：**内置图标风格枚举 outline/filled；只选内置几何，不允许任意图标资源；完整动作/状态覆盖和描边数值仍待验证。

**回写与验收：**主题 §5.4、UI §3.2；TH-T04/19，V-04/09。决定已采纳，相关实现/实验尚未完成。

### TH-06 · 用户主题的同 ID 升级流程

**已确认 A：**V1 无单独同 ID 覆盖升级命令；普通同 ID 导入均拒绝；作者用新 ID 分发，或用户备份后目录手工替换并刷新校验。

**回写与验收：**主题 §9/10、PRD THEME-05/08、UI §11；TH-T09/21，V-04。决定已采纳，相关实现/实验尚未完成。

### TH-07 · GitHub 继承根与可修改内置副本

**已确认 A：**受保护随应用 GitHub 为继承和最终安全回退根；可编辑磁盘内置副本保留用户修改，不静默覆盖；根升级须回归/版本说明。

**回写与验收：**主题 §6/10、PRD THEME-09、架构 §8.2；TH-T05/10，V-04。决定已采纳，相关实现/实验尚未完成。

### TH-08 · 导入成功后的主题选择

**已确认 A：**成功安装不自动应用；保留当前家族/模式/跟随及用户覆盖，名称按钮显式应用，预览不应用。

**回写与验收：**主题 §9/10、PRD THEME-04/APP-03、UI §11.2；TH-T23，V-04/09。决定已采纳，相关实现/实验尚未完成。

### TH-09 · 是否提供应用内主题删除

**已确认 A：**V1 无应用内删除按钮；通过打开目录手工移除后刷新；外部失效仍按安全回退与说明处理。

**回写与验收：**主题 §10、PRD THEME-09、UI §11.2；TH-T11/23，V-04/09。决定已采纳，相关实现/实验尚未完成。

### TH-10 · 失效后的主题选择记忆

**已确认 A：**活动家族或固定代码包失效保留原选择身份，有效呈现回默认并说明；修复/重装/刷新/重启不自动重新应用，由用户显式选择；失效后新选择不得被旧身份覆盖。

**回写与验收：**主题 §10/11、PRD THEME-09、UI §11.2、架构 §8.2；TH-T11/23，V-04/09。决定已采纳，相关实现/实验尚未完成。

### 14.11 既有决定与验证边界

PL-01～PL-03、UI-01～UI-06、TA-01～TA-03、RS-01～RS-05、ED-01、PA-01 与 Q-18 的确认保持有效。PL-02 已确定高亮关闭回内容主题普通代码表面，独立配置保留暂停、再次启用恢复；本轮只补字段与色源契约，不重开其行为。

TH-01～TH-10 不再阻塞格式/管理行为选择。字段表和固有类型/单位/合并规则按本稿实施；最大/最小可用数值、完整 GitHub 默认值、字体/图标/透明度原生能力和包预算仍未验证，不能宣称完整发布 schema、AC 或 V 已通过。后续能力/范围变化须取得新的明确决定，不以技术降级自动替代本次确认。

## 15. 旧稿与原型逐项处置账本

2026-10-30收尾补记：PROTO-06已确认A，必要原始例证/完整源码索引已保全于[历史证据索引](./archive/migration-evidence/README.md)，Zed已有[现行复核](./zed-notes.md)。下表“移出/压缩/旧稿保留”等为首轮处置语境，不表示仍依赖docs_bak加载证据；具体本轮去向见[331主题收尾账本](maintenance/deletion-readiness/prior-evidence/closure-topic-mapping.md)。

保留/修订/移出/删除/待验证均指现行规则效力，**不删除或改写历史文件**。来源为[旧主题包规范](./archive/migration-evidence/theme-package-spec.md)及[原型主题目录](./prototype/legacy/themes/)。

| 旧章/主题 | 处置 | 现稿去向/修复 |
|---|---|---|
| 开篇、配置格式裁决：JSON/四职责、原型与第三方生态理由 | 保留、修订、删除 | §1～5；四文件取 PRD，不用浏览器共用或外部主线证明原生实现，删除这些理由的裁决效力 |
| 主题包目录：四文件/三预览、fonts/licenses、根/单层根 | 保留、修订 | §3/4/8/9；必需资产取 PRD，目录/路径规则已明确；不引用包外默认预览 |
| 可配置项目 §1：内容/代码/表格/字体/间距 | 修订、待验证 | §5.2/6；单位/叶子/生效范围明示，缺省不多源；删除 colors.link 重复和行为开关 |
| §1 formula/diagram、重复代码表面回退 | 修订、移出、删除 | §2/5；公式默认与插件覆盖区分，Mermaid 独立主题去插件稿；删除 diagram.* 越权、双重高亮缺省 |
| §2 语义高亮配色 | 保留、修订、移出 | §5.3；十四语义类，独立表面；语言识别/开启关闭归插件稿，PL-02 已确认关闭用当前内容主题 |
| 应用外观/图标/文件树、字段表 | 修订、待验证 | §5.4/TH-05；去 application.editor 画布别名；图标有限内置枚举，字段责任不任意组件布局 |
| 字段清单的字体族存在检查、静态 face 与授权 | 修订、待验证 | §4/8；系统族缺失正常回退，包内激活失败不可静默；许可证存在不等于法律证明；跨包同族 V-04 |
| 旧数值范围/字体数量/包体限额 | 删除、待验证 | §3.2/9.3；旧数值仅实验候选，发布前需实测冻结上下界，不能“待定=不限” |
| 不开放项目：静态、不改行为、不执行/联网 | 保留、修订 | §2/8/9；补只读区、符号链接/路径别名、包资源 allowlist，不把其它图片能力带入包 |
| 字段继承与合并 | 保留、修订（已确认） | §6；已定逐字段/数组替换，其余 TH-03/07；先验每层，不被后层覆盖掩盖非法 |
| JSON 示例 | 删除、修订 | §12 重写与表一致的四文件/最小包，不暗示旧 schema3 已冻结 |
| ZIP 校验与导入 | 保留、修订、待验证 | §9/11；补持续预算、路径碰撞/链接/竞争、失败恢复/并发安装；旧 ZIP64 禁止不自动沿用 |
| 主题文件夹与同步 | 修订、移出（已确认） | §10 只保目录/资源契约；UI 操作/传播链接上游，目录服务归架构；升级/根副本 TH-06/07 |
| 版本与原型边界、schema2迁移、主线 add_fonts 说明 | 删除、修订、待验证 | §1/4/8；保留历史事实，不承诺原型自动迁移，不以主线 GPUI API 当锁定版本证据，V-04 |

### 15.1 样本与原型脚本盘点

2026-10-30 历史样本与工具已完整保留于 [非权威原型目录](./prototype/README.md)，逐项处置见[整理账本](./prototype/review.md)。保留是证据归档，不是制作正式主题包；schema 1、封闭字段与静态 PNG 要求保持不变，旧 JSON/SVG 未转换，V 实验未执行。

| 样本材料 | 静态事实 | 处置与差异 |
|---|---|---|
| themes/{github,notion,vue,feishu}/theme.json | 四份均 schemaVersion=3、version=1.0.0、inherits=github；均有 id/name/author/files/previews，无 description/fonts；家族名分别 GitHub/Notion/Vue/飞书 | 保留身份/职责样本，修订版本和清单；不能证明字体加载；description 新字段已按 TH-02 确认 |
| 四套 markdown/application/highlight JSON（共十二份） | 顶层 common/modes、三模式齐全，字段树一致，值以色彩/排版/间距区分；有浏览器族 -apple-system/BlinkMacSystemFont、italic/strike.enabled；两文件重复代码表面，application 含 editor | 修订：原生字体族/语义/画布/代码色源消歧；缺字段合法不等于现有样本就是完整根基线；不直接迁文件 |
| 每套 previews/{light,dark,sepia}.svg（共十二张） | 三路径存在，均为历史 SVG 展示资产 | 按 TH-04 改用静态 PNG，真实性仍待验证；不是正式原生三模式渲染证据，不拿现存图替代 V-09 |
| themes/index.json | 原型内置 ID/manifest 路径发现索引 | 移出单包格式，拒绝导入包附带该索引改变注册表 |
| [theme-library.js](./prototype/legacy/theme-library.js) | 外置 themeDescriptions 为介绍兜底；清单 allowlist 无 description；配置校验有泛化叶子/形状检查，未形成严格逐字段类型闭环；预览可加载 SVG/PNG/JPEG/WebP，字体用 FontFace/IndexedDB | 删除作为正式解析器/安装证明的效力；描述入清单、严格类型/资源校验、字体/路径/原生激活重新验；颜色六位 hex 方向可参考 |
| [theme-gallery.css](./prototype/legacy/theme-gallery.css) | 卡片/介绍/大图的浏览器布局与样式 | 移出 UI 参考，保留已定双动作/介绍等结果；CSS 不进入包格式，像素/透明不当平台证据 |
| [generate-theme-previews.py](./prototype/legacy/tools/generate-theme-previews.py) | 从脚本内硬编码色表生成三模式 SVG，并覆盖索引和四份主题清单的历史工具（不读取现存三配置 JSON） | 仅归档制作线索；写回的是含旧 builtin/colors、缺正式 files 的另一历史 schema 1，不能重跑覆盖归档或用作正式生成器；不是 GPUI 实际渲染截图 |

## 16. 待验证、专项关系与确认回写

| 编号 | 必须交付的技术证据（当前均未执行） | 推进/冻结边界 |
|---|---|---|
| V-04 | 锁定 GPUI 静态字体激活/中文与字重/同族隔离/失败无污染、缓存刷新、透明回退；主题 schema/默认三模式、授权/资源/ZIP/冲突/升级/崩溃恢复样本 | M1 字体实验；M4 实现前格式/限额/支持清单；M6 三平台。TH-01～10 已确认，按采纳规则实验，不能把确认代替验证 |
| V-06 | 字节/文件数/嵌套/图片像素/内存/字体与生成资产/工作量/时限、真实终止、敌意输入 | M1～M4 实测预算，M6 回归；无数字不得开放无限制资源 |
| V-01/V-09 | 三平台路径/重解析点/图标/原生表面边界、中英文/键盘/尺寸/三模式真实预览/焦点与提示 | 先 macOS 后 Windows/Linux，正式三平台同发；旧 CSS/SVG 不替代实际证据 |
| V-03/V-02 | Markdown/图片 tab/图表输出的 SVG 安全解析子集（包预览拒绝 SVG）；语言类别映射/公式/Mermaid独立主题；导出字体与主题/插件关闭组合 | 图像/图表支持与格式输出按自身矩阵，本稿不增远程资源或脚本范围 |
| V-05/V-07 | 字体/主题失效的布局时延/内存、后台晚到与 IME/输入组/源码锚点稳定 | 主题切换零编辑、保存字节保真与配置生效分别检查 |

| 规范 | 本文依赖及本轮回写 |
|---|---|
| [PRD §5.10/5.13、AC-12/13](./v1-prd.md) | 唯一范围依据；已回写 TH-01～10 的版本/资源/管理行为，AC-12/13 及设置默认保持对应 |
| [架构 §4.4/8.2/9/12](./v1-architecture.md) | 模块/编排/缓存；§1/8.2/12/14 已回写现行入口、根默认/磁盘副本、失效意图与一次发布；app-core 不放文件/GPUI |
| [架构规范 §13/18](./architecture-guide.md) | R-13 状态/设置传播/样式边界；已同步根默认、继承/色源和失效意图，主题/插件入口指向现行规范 |
| [UI §3.2/11.2/16/18](./ui-spec.md) | 排版责任、卡片双动作与真实预览；已同步静态 PNG、两图标枚举、安装不应用/目录删除/无升级按钮及失效记忆；UI-02 不重开 |
| [插件 §5.3/6.5/12/14](./plugin-system.md) | 独立代码/公式/Mermaid；PL-02 沿用，现行主题入口与高亮色源已同步，不改变 PL 结论 |
| [验证计划 V-04/V-06/V-09](./v1-validation-plan.md) | 已挂入 TH-T01～23 对应格式/继承/管理场景，补同名字体/安装故障/路径别名/真实预览；V 项均未执行 |
| [Rust 规范](./rust-style-guide.md) / [引擎指引](./engine-design.md) | 类型化可恢复错误/中文本地化/无 unsafe 授权、源码真源/输入/历史/坐标；本稿不新增 Rust 签名或代码 |
| [产品批阅归档 Q-18](./archive/2026-10-28-v1-open-questions.md) / [决定原文与逐条复核](./decisions.md#4-历史时间线与逐条复核) | 确认先验证字体及透明回退的依据/历史追溯，旧字段或数值不因归档自动批准 |

首轮仅交付本文与 README，随后用户确认十项建议。本轮创建确认归档并回写主题、PRD、UI、架构、插件入口、验证计划与 README；docs_bak 和既有归档不改，无应用代码、主题包或实验结果。TH-01～TH-10 的决定阻塞均解除；字段/行为按已确认规则推进，未完成的默认值、数值预算与原生能力仍按 V-04/V-06/V-09 等收口。不能把文档回写当作可发布实现或三平台验收通过。
