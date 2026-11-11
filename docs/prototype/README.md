# Aline 历史界面原型与素材

整理日期：2026-10-30。PROTO-01～06已确认A，长期保留运行副本；除记录在案的授权变更外不改历史字节。策略/来源事实分别见[确认结果](./open-questions.md)和[补证账本](../assets/provenance.md)。**这里是非权威设计参考，不是应用代码、已确认 UI 金样或可安装主题包。** 当前要求按 [AGENTS.md](../../AGENTS.md) → [PRD](../v1-prd.md) → 已确认 [UI 规范](../ui-spec.md)/[主题包规范](../theme-package-spec.md) → [验证计划](../v1-validation-plan.md) → 历史原型理解。初次整理不新增产品要求；2026-10-30 用户要求删除旧图标，仅删除旧 PNG 并更新三处图标引用，见[采用与删除记录](../decisions.md#12-2026-10-30-应用图标采用与旧图删除)。2026-11-03 用户要求设置增加界面语言并真实切换界面文案，经裁决直接修改 legacy：新增 `i18n.js`，修改四个历史文件，见[语言切换记录](../decisions.md#23-2026-11-03-原型界面语言切换)。两次都是原型内的有限授权变更，不改变原生 M1 范围；V-01～V-09 仍未整项通过。

**“非权威”不等于“不参照”。** 2026-11-03 用户裁决：原生 UI 实现时按 `legacy/` 的 HTML/CSS 对照**布局结构、区域顺序、控件邻接、图标含义与交互意图**，逐区域映射见 [UI 规范 §4.1](../ui-spec.md#41-原型布局对照实现参照)。原型决定“怎么摆”，PRD 与已确认规范决定“有什么”；两者冲突时以 PRD/已确认规范为准。原型**不**决定产品范围、键位、默认值、运行时主题格式与冻结像素，也不作为原生能力的证明。

- [整理结果、差异与删除前清单](./review.md)
- [逐文件用途、引用和处置账本](./inventory.md)；[大小、尺寸与 SHA-256 清单](./inventory.json)
- [PROTO-01～PROTO-06的确认结果](./open-questions.md)
- [历史页面](./legacy/index.html)；[原始 README](./legacy/README.md)（其中启动 URL、功能范围及默认值已过时，以本页说明为准）

## 运行历史参考

在仓库根目录执行，只绑定本机：

```sh
python3 -m http.server 4173 --bind 127.0.0.1 --directory docs
```

打开 `http://127.0.0.1:4173/prototype/legacy/`。可附 `?platform=macos` 或 `?platform=windows` 查看浏览器模拟菜单；没有独立 Linux 外观模拟，这不证明任何原生平台能力。停止服务用 Ctrl+C。

不要以 `file://` 直接打开作为完整运行方法：主题发现会用相对路径 `fetch()` 读取 JSON 和预览，浏览器可能阻止本地文件读取。启动资源位于 `legacy/`，仅品牌图标共享 `docs/assets/app-icon/aline.png`，因此 HTTP 根目录须使用 `docs/`；无 CDN、在线字体或启动网络资源依赖；本机 HTTP 服务可在断网时使用。示例 GitHub 链接点击后会离开本地，Primer 链接仅为原 README 的来源线索，SVG 的 W3C URL 是命名空间。

建议使用独立浏览器配置或专用本地端口：原型会把示例文档、活动标签、部分设置及导入主题存入该 origin 的 localStorage/IndexedDB；历史存储可能改变首次画面。需要重置时仅清除此原型 origin 的站点数据，原型内的写作内容会丢失，先自行复制保留。它不保存到 Markdown 文件，不提供正式未保存保护，不用于编辑重要文档或验证不可信主题包。

## 目录与维护边界

```text
docs/prototype/
├── README.md / review.md / open-questions.md  # 当前整理说明，不是新产品规范
├── inventory.md / inventory.json            # 48 项逐文件处置及来源哈希
├── reference-check.json                     # 初次相对资源及本机 HTTP 检查结果
├── reference-check-2026-11-03.json          # 语言切换后的检查快照
├── backup-references.tsv                    # 整理后剩余旧目录引用快照
├── tools/verify-reference.py                # 只读核对副本、引用和 HTTP 可达性
└── legacy/                                 # 45 文件；40 个原字节，4 个授权修改，新增 i18n.js
    ├── index.html / README.md
    ├── *.css / *.js                         # 5 个样式、7 个脚本，保持加载顺序
    ├── assets/                             # 山丘示意图；旧品牌 PNG 已删除
    ├── themes/                             # 17 JSON、12 SVG；旧格式非正式包
    └── tools/generate-theme-previews.py      # 历史生成器，禁止在归档内重跑
```

`legacy/` 初次保存45个文件的原始字节；2026-10-30 按用户要求删除旧品牌 PNG，`index.html` 三处图标 src 改为共享新 PNG；2026-11-03 按用户要求新增界面语言切换演示，新增 `i18n.js`，修改 `index.html`、`prototype.js`、`interactions.js`、`menus.js`。现存45文件，其中40个仍与初次清单逐字节一致。两次变更的授权链、旧/新哈希与新增文件见 `inventory.json` 的 `authorized_updates` / `added_files`；除此之外不整合页面、不修复历史交互。涉及旧原型内容的仓库提交依次为 `28254bb214da4e9a77967a0a4604a9fcbb9a8935`（2026-10-28 迁移）与 `ad7e19d2866d5655be3cf05f3e36dda63ca10fc5`（2026-11-03 语言切换）；逐文件哈希记录当前实际文件，不能用提交号替代内容核对。原 `docs_bak/`、`reports/` 已经用户授权删除，见[删除执行记录（历史）](https://github.com/zsxink/Aline/blob/59d601242775db5e08f53d03a5d2b5c46154eaf0/docs/maintenance/deletion-readiness/deletion-execution.md)；本目录的历史副本及独立证据继续保留，运行不需要原目录。

维护只处理来源说明、入口、迁移完整性和记录在案的授权变更。除 2026-11-03 用户明确要求的原型语言切换外，历史浏览器逻辑不追随 PRD 改造；发现差异记入账本，未来若制作新参考应建独立版本并注明与原件关系。语言切换只是原型内的 DOM 翻译演示，不构成正式 i18n 架构，也不改变原生 M1 的语言范围。不要把 `legacy/` 拷贝到应用 `src/` 或 `resources/themes/`，不要从它反推新功能、键位、默认值或运行时主题格式。

主题清单是旧 schema 3、预览为 SVG；正式 schema 1 要求静态 PNG 和封闭字段。历史生成器甚至会重写为另一套 schema 1 旧清单（含旧 `builtin/colors`、缺正式 `files`），数字相同也不兼容现行 schema 1。它的预览是硬编码色表绘图，不是 GPUI 渲染截图，不能用于正式主题生成或验收。

标题栏、设置和关于页面共享[当前透明图标](../assets/app-icon/aline.png)，旧图标不再保留。应用开发的实际平台选图以[图标说明](../assets/app-icon/README.md)为准；这次引用更新不改变历史界面的非权威地位，也不补证其余历史材料的作者或许可。

## 核对副本

在仓库根目录执行：

```sh
python3 docs/prototype/tools/verify-reference.py
```

脚本只读：检查原始哈希、按路径串联的授权变更链（含两次图标删除/引用替换与语言切换修改）、45文件闭包、新增 `i18n.js` 的大小/哈希、唯一共享图标哈希、JSON/SVG 结构及清单路径，并临时在本机随机端口提供 HTTP，逐个请求历史文件与共享图标后关闭服务。它不执行 JS、不运行历史生成器，不校验正式主题包，也不等于原生 UI、浏览器完整交互或 V 实验通过。保存初次结果见 `reference-check.json`；2026-11-03 语言切换后的检查结果见 `reference-check-2026-11-03.json`。
