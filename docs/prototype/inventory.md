# 原型与素材逐文件账本

后续状态（2026-10-30）：用户要求采用新应用图标并删除旧图，两份旧 PNG 已删除，legacy/index.html 仅更新三处引用，当时现存44个legacy文件；43文件保持原字节。下表保留初次48项盘点及当时目标，不表示旧图仍存在或可用于开发。精确授权与现状哈希见 [inventory.json](./inventory.json) 的 authorized_updates，开发选图见[图标说明](../assets/app-icon/README.md)。

后续状态（2026-11-03）：用户要求原型设置增加界面语言（简体中文/英文）并真实切换界面文案；经裁决直接修改 legacy，新增 i18n.js，修改 index.html、prototype.js、interactions.js、menus.js，现存45个legacy文件，其中40个仍与初次清单逐字节一致。授权链、新增文件大小/哈希见 [inventory.json](./inventory.json) 的 authorized_updates 与 added_files；这是原型演示，不改变原生 M1 范围。变更用途见下节。

初次盘点日期：2026-10-30。范围含旧原型46文件、旧icons目录2文件，共48项；其中2个系统文件不迁移。`docs_bak/docs/assets/` 不存在，不虚构已迁移内容。逐字节副本45项；旧icons PNG复用现有参考1项；没有删除原件。

路径前缀：下表“旧相对路径”除最后两项外均相对 `docs_bak/prototype/`；目标相对仓库根。`legacy` 全部是非权威历史材料；“迁移归档”不等于确认继续维护或允许对外发布。保留完整依赖是为了安全追溯，不把所有旧成果认定为正式资产。

每项大小、SHA-256、图像格式和尺寸见 [inventory.json](./inventory.json)。字段/功能差异详见 [复核结果](./review.md)，来源/后续形态待 [PROTO 清单](./open-questions.md)。

| 旧相对路径 | 类别/用途 | 被谁引用/依赖关系 | 当前承接依据 | 价值、旧内容与处置边界 | 本轮处置/目标 |
|---|---|---|---|---|---|
| `README.md` | 说明：历史目的、启动与演示边界；保留 MarkFlow/Primer/Measure 来源线索 | 人工入口；旧说明的 /prototype/ URL 与 v1-features 口径过时 | PRD §1；UI §2/19.2 | 有追溯价值；默认值与范围不得继承 | 迁移归档；`docs/prototype/legacy/README.md` |
| `assets/aline.png` | 品牌参考 PNG：Aline 字母图标，512×512 RGBA；与现有 docs/assets 图标同哈希 | index.html 标题栏/设置/关于 | PRD §6.1；UI §3.1 | 身份参考已承接；来源/授权待 PROTO-01，不增发正式资产 | 迁移归档；`docs/prototype/legacy/assets/aline.png` |
| `assets/showcase-landscape.svg` | 演示 SVG：山丘/日光插画，800×360；非产品截图 | showcase.js 正文图片；interactions.js 图片插入示例 | IMAGE-01；UI §7.3 | 只随运行副本保留，非品牌；额外用途/来源待 PROTO-02 | 迁移归档；`docs/prototype/legacy/assets/showcase-landscape.svg` |
| `icons.js` | JavaScript/图标：统一内联 SVG 图标及名称，删 PlantUML/Frontmatter 入口，将导出按钮改为示例 | index.html 第二个脚本；依赖 prototype.js，供后续脚本取图标 | UI §3.2/10；PRD §8 | 图标形态/覆盖变更需追溯；仅 outline 原型，不证明 filled 覆盖；来源待 PROTO-03 | 迁移归档；`docs/prototype/legacy/icons.js` |
| `index.html` | HTML：唯一页面；主窗、设置/关于/快捷键弹层静态骨架 | 入口；按顺序加载 5 CSS、6 JS，引用 assets/aline.png | PRD §6；UI §4/10/11 | 布局可参考；Frontmatter 按钮运行时删除，设置仍是旧模态承载 | 迁移归档；`docs/prototype/legacy/index.html` |
| `interactions.js` | JavaScript：接管内存示例树/标签、源码与DOM简化转换、复制/语言菜单/侧栏/会话 | index.html；依赖 showcase/prototype/icons；引用山丘 SVG | EDIT/FILE/SAFE；UI §5～8/12 | 交互意图可参考；localStorage tabs恢复/直接关闭无保护、execCommand 非正式事务 | 迁移归档；`docs/prototype/legacy/interactions.js` |
| `light.css` | CSS：GitHub 素白表面、内容、代码控件与布局修订 | index.html；叠加 prototype.css | PRD §6.2；UI §3/7 | 六项基准色以 PRD 为准；其余样式/尺寸非冻结 | 迁移归档；`docs/prototype/legacy/light.css` |
| `measure.css` | CSS：中文行禁则、行高、合成斜体与 text-autospace 样式 | index.html；@layer measure | UI §14；V-09 | 排版研究线索；来源许可待 PROTO-03，旧 Measure 检查失败 | 迁移归档；`docs/prototype/legacy/measure.css` |
| `menus.css` | CSS：浏览器模拟系统/右键菜单、子菜单与焦点样式 | index.html；menus.js 创建的 DOM | UI §10/13；V-01/V-09 | 菜单表面可参考；非 OS 菜单/系统能力证据 | 迁移归档；`docs/prototype/legacy/menus.css` |
| `menus.js` | JavaScript：macOS/Windows 模拟菜单、八类对象右键、键盘子菜单 | index.html；调用全局打开设置/关于/新建等；多数动作仅 toast | UI §10.3/10.5；FILE/EDIT/EXPORT | 菜单分区有价值；包含未立项/禁用项及旧键位，不沿用为 V1 | 迁移归档；`docs/prototype/legacy/menus.js` |
| `prototype.css` | CSS：主窗口网格、主题模式、标签/设置/代码等基础样式 | index.html；被后续样式层覆盖 | UI §3～7/11；V-09 | 布局参考；像素、全局 CSS 与浏览器滚动不作原生规范 | 迁移归档；`docs/prototype/legacy/prototype.css` |
| `prototype.js` | JavaScript：早期壳与设置模板、主题/通知、部分浏览器存储 | index.html 第一个脚本；向后续脚本提供 $/settings/主题函数 | APP-01～05；UI §11；SAFE-04 | 仍显示拼写/30秒/网络下载等旧设置；PlantUML 模板被 icons.js 删除 | 迁移归档；`docs/prototype/legacy/prototype.js` |
| `showcase.js` | JavaScript/内嵌图：五组语法示例+README，共6文档；MathML 公式、内嵌预制 SVG 图表 | index.html；interactions.js 消费 ALINE_DEMOS；引用山丘 SVG | MD-01～11；UI §7/8；V-03/V-07 | 只作夹具线索；非 pulldown-cmark/只读区/保真/安全测试 | 迁移归档；`docs/prototype/legacy/showcase.js` |
| `theme-gallery.css` | CSS：主题卡、说明、预览大图及相关设置样式 | index.html；theme-library.js/prototype.js 生成 DOM | APP-03；UI §11.2；V-09 | 双动作布局有价值；CSS 不属于主题包字段 | 迁移归档；`docs/prototype/legacy/theme-gallery.css` |
| `theme-library.js` | JavaScript：索引/四 JSON/预览读取、浏览器 ZIP/字体与 IndexedDB、主题卡/代码配色 | index.html 最后脚本；fetch themes/index.json；逐主题 files/previews | THEME/APP-03；UI §11；主题规范 §3～11 | 旧 schema/字段/FontFace/存储/安装，仅历史研究；非正式校验器 | 迁移归档；`docs/prototype/legacy/theme-library.js` |
| `themes/.DS_Store` | 系统文件：macOS Finder 元数据 | 无运行资源引用 | 不适用 | 废弃建议明确，无长期参考价值；原件未删 | 无需迁移；保留原件待整体删除 |
| `themes/feishu/application.json` | 主题配置 JSON：feishu 应用UI颜色/图标/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/feishu/application.json` |
| `themes/feishu/highlight.json` | 主题配置 JSON：feishu 代码表面/语法色/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/feishu/highlight.json` |
| `themes/feishu/markdown.json` | 主题配置 JSON：feishu 正文/块/字体/间距/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/feishu/markdown.json` |
| `themes/feishu/previews/dark.svg` | 主题预览 SVG：feishu dark 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/feishu/previews/dark.svg` |
| `themes/feishu/previews/light.svg` | 主题预览 SVG：feishu light 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/feishu/previews/light.svg` |
| `themes/feishu/previews/sepia.svg` | 主题预览 SVG：feishu sepia 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/feishu/previews/sepia.svg` |
| `themes/feishu/theme.json` | 主题清单 JSON：feishu 身份/版本/files/三模式路径；schema 3 | themes/index.json → theme-library.js；旧生成器会覆盖 | THEME-01/04；主题规范 §4/15.1 | 身份可参考，缺 description/PNG、格式过时；只归档不转正式包 | 迁移归档；`docs/prototype/legacy/themes/feishu/theme.json` |
| `themes/github/application.json` | 主题配置 JSON：github 应用UI颜色/图标/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/github/application.json` |
| `themes/github/highlight.json` | 主题配置 JSON：github 代码表面/语法色/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/github/highlight.json` |
| `themes/github/markdown.json` | 主题配置 JSON：github 正文/块/字体/间距/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/github/markdown.json` |
| `themes/github/previews/dark.svg` | 主题预览 SVG：github dark 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/github/previews/dark.svg` |
| `themes/github/previews/light.svg` | 主题预览 SVG：github light 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/github/previews/light.svg` |
| `themes/github/previews/sepia.svg` | 主题预览 SVG：github sepia 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/github/previews/sepia.svg` |
| `themes/github/theme.json` | 主题清单 JSON：github 身份/版本/files/三模式路径；schema 3 | themes/index.json → theme-library.js；旧生成器会覆盖 | THEME-01/04；主题规范 §4/15.1 | 身份可参考，缺 description/PNG、格式过时；只归档不转正式包 | 迁移归档；`docs/prototype/legacy/themes/github/theme.json` |
| `themes/index.json` | 主题索引：四个主题 ID 与清单路径；schemaVersion=3 | theme-library.js fetch；生成器也会重写此文件 | 主题规范 §3/15.1 | 仅原型发现索引，正式单包不允许附带注册表索引 | 迁移归档；`docs/prototype/legacy/themes/index.json` |
| `themes/notion/application.json` | 主题配置 JSON：notion 应用UI颜色/图标/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/notion/application.json` |
| `themes/notion/highlight.json` | 主题配置 JSON：notion 代码表面/语法色/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/notion/highlight.json` |
| `themes/notion/markdown.json` | 主题配置 JSON：notion 正文/块/字体/间距/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/notion/markdown.json` |
| `themes/notion/previews/dark.svg` | 主题预览 SVG：notion dark 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/notion/previews/dark.svg` |
| `themes/notion/previews/light.svg` | 主题预览 SVG：notion light 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/notion/previews/light.svg` |
| `themes/notion/previews/sepia.svg` | 主题预览 SVG：notion sepia 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/notion/previews/sepia.svg` |
| `themes/notion/theme.json` | 主题清单 JSON：notion 身份/版本/files/三模式路径；schema 3 | themes/index.json → theme-library.js；旧生成器会覆盖 | THEME-01/04；主题规范 §4/15.1 | 身份可参考，缺 description/PNG、格式过时；只归档不转正式包 | 迁移归档；`docs/prototype/legacy/themes/notion/theme.json` |
| `themes/vue/application.json` | 主题配置 JSON：vue 应用UI颜色/图标/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/vue/application.json` |
| `themes/vue/highlight.json` | 主题配置 JSON：vue 代码表面/语法色/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/vue/highlight.json` |
| `themes/vue/markdown.json` | 主题配置 JSON：vue 正文/块/字体/间距/三模式 | 同目录 theme.json.files → theme-library.js | 主题规范 §5～6/15.1；V-04 | 仅视觉/字段研究；见差异表，不作为正式样例/完整默认值 | 迁移归档；`docs/prototype/legacy/themes/vue/markdown.json` |
| `themes/vue/previews/dark.svg` | 主题预览 SVG：vue dark 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/vue/previews/dark.svg` |
| `themes/vue/previews/light.svg` | 主题预览 SVG：vue light 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/vue/previews/light.svg` |
| `themes/vue/previews/sepia.svg` | 主题预览 SVG：vue sepia 预制界面示意，960×540 | 同目录 theme.json.previews → theme-library.js 卡片/大图；生成器输出 | APP-03；主题规范 §8.1/15.1；V-09 | 三模式示意可参考；硬编码绘图非真实截图，不转PNG冒充正式预览 | 迁移归档；`docs/prototype/legacy/themes/vue/previews/sepia.svg` |
| `themes/vue/theme.json` | 主题清单 JSON：vue 身份/版本/files/三模式路径；schema 3 | themes/index.json → theme-library.js；旧生成器会覆盖 | THEME-01/04；主题规范 §4/15.1 | 身份可参考，缺 description/PNG、格式过时；只归档不转正式包 | 迁移归档；`docs/prototype/legacy/themes/vue/theme.json` |
| `tools/generate-theme-previews.py` | 生成工具：Python 标准库；硬编码色表绘制12 SVG，并覆盖4清单和索引 | 人工运行；无页面加载；输出 themes/*/theme.json、previews/*.svg、index.json | 主题规范 §8/15.1；V-04/V-09 | 仅归档制作线索；会回写另一旧 schema 1，不能运行作迁移/正式生成器 | 迁移归档；`docs/prototype/legacy/tools/generate-theme-previews.py` |
| `docs_bak/docs/icons/.DS_Store` | 系统文件：macOS Finder 元数据 | 无运行资源引用 | 不适用 | 废弃建议明确，无长期参考价值；原件未删 | 无需迁移；保留原件待整体删除 |
| `docs_bak/docs/icons/Aline.png` | 品牌重复 PNG：512×512 RGBA；与两个现存PNG字节相同 | 旧 UI 稿图标链接；现行 PRD/UI 已指向 docs/assets/aline.png | PRD §6.1；UI §3.1 | 不再新增重复正式资产；授权待 PROTO-01 | 复用现有参考；`docs/assets/aline.png` |

## 2026-11-03 语言切换新增与修改

用户要求“真的切换，界面文案跟着变”，并裁决直接修改 `legacy/`。新增 `i18n.js` 在 DOM 层按现有中文源文案切换界面语言；修改的四个历史文件只做必要的内部键与加载接入，不改变原有历史交互范围。精确字节与授权链以 [inventory.json](./inventory.json) 为准。

| 路径 | 类别/用途 | 依赖/影响 | 处置边界 |
|---|---|---|---|
| `i18n.js` | JavaScript：中→英词表、动态文案规则、DOM/属性翻译、MutationObserver、localStorage 偏好 | index.html 首个脚本；读取 `#editor`、系统菜单、设置/主题/对话框等现有 DOM | 新增；原型演示，不是正式 i18n 架构或原生能力证据 |
| `index.html` | 页面入口：增加 i18n.js 加载与脚本缓存版本 | 静态加载顺序 | 授权修改；三处图标引用仍指向共享新 PNG |
| `prototype.js` | 设置模板：通用页新增“语言”分组与内部页面键 `dataset.page` | 设置导航、插件可见性 | 授权修改；不改变其他设置模板 |
| `interactions.js` | 设置记忆改用内部页面键，避免英文显示值破坏恢复 | 设置控件状态 | 授权修改；不改变保存/交互逻辑 |
| `menus.js` | 菜单项保留内部 `data-label` 供动作判断 | 系统菜单动作 | 授权修改；修复英文下动作退化为提示的问题 |

## 视觉素材与来源

| 素材 | 尺寸/格式/数量 | 已知来源线索 | 授权与使用限制 |
|---|---|---|---|
| 三处 Aline PNG（旧2处、现行1处） | 512×512 RGBA PNG，相同235494字节 | PRD/UI 已采用身份参考；图像为深色 A 与浅色飘带 | 没有可核对授权记录；PROTO-01 |
| 山丘示意图 | 800×360 SVG，1张 | 自包含 rect/circle/path；两个JS引用，未见作者/许可说明 | 只供历史演示；PROTO-02 |
| GitHub/Notion/Vue/飞书预览 | 每张960×540 SVG，4×3=12张 | 工具给出生成构图线索及硬编码色表；清单 author=Aline 是自述，不能当权利证明 | 非产品截图/正式PNG预览；来源及继续使用见PROTO-03/05 |
| 功能图标/公式/流程时序图 | HTML/JS中内嵌SVG/MathML，无独立图片文件 | icons.js/menus.js/interactions.js/showcase.js及HTML内联几何 | 不额外抽取为正式图标库/发布图片；来源PROTO-03；V-03/V-09未执行 |

未发现字体二进制、独立产品截图、其它HTML页面、主题PNG预览或构建依赖清单；不为缺失资源补造出处。`themes/`共29个有效文件（17 JSON + 12 SVG），另有1个系统文件；单主题7文件，共28，外加索引1。旧生成器的结果与现存清单schema不同，保留工具不证明当前样本可重现。

## 当前完整依赖链

```text
index.html
  ├─ measure.css → prototype.css → light.css → menus.css → theme-gallery.css（加载顺序）
  ├─ ../../assets/app-icon/aline.png（当前共享图标；旧品牌 PNG 已删除）
  └─ i18n.js → prototype.js → icons.js → menus.js → showcase.js → interactions.js → theme-library.js
       ├─ showcase/interactions → assets/showcase-landscape.svg
       └─ theme-library → themes/index.json → 4×theme.json
                           └─ 各自3配置JSON + 3模式SVG
tools/generate-theme-previews.py（只归档，不是运行依赖）
  └─ 会覆盖上述索引、4清单及12预览，不能在归档目录重跑
```

没有到旧目录外的启动资源引用。历史README中的 /prototype/、示例正文中的虚拟 images/、v1-features.md 名称是过时说明，不是遗漏的运行资源；正确访问方式见当前README。静态引用/主题路径/HTTP证据见 [reference-check.json](./reference-check.json)。
