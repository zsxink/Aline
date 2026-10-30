# 原型、资产与独立工具闭包复核

2026-10-30。本文是删除准备维护审计附件，不是产品规范。结论仅覆盖原型与资产专项，不代替其余旧文档及 reports 的整体结论。两个待删除目录均未移动、删除或改写；历史副本字节也未修改。

## 独立结果与范围

直接递归枚举 `docs_bak/prototype/` 及 `docs_bak/docs/icons/`，包含隐藏文件，未从旧报告枚举清单反推文件集合。共 48 文件、879281 字节：45 个原型有效文件、1 个旧 icons 品牌 PNG、2 个 `.DS_Store`。原型有效文件为 1 Markdown、1 HTML、5 CSS、6 JS、17 JSON、13 SVG、1 PNG、1 Python。原型下 11 个子目录（不含自身）；旧 icons 为另一素材目录。`.DS_Store` 只登记大小和哈希，不审阅内容；没有因 `.gitignore` 忽略它们而漏记。未发现字体二进制、独立截图、额外 HTML、其它隐藏运行文件或许可文件。

48 项独立处置和本轮重新计算的 SHA-256 见 [prototype-disposition.csv](./prototype-disposition.csv)。45 个有效原型文件逐个与 `docs/prototype/legacy/` 相同相对路径目标直接比较全部字节，45/45 相同；源/目标文件集合也一致（排除源 Finder 文件）。旧 icons PNG 与 `docs/assets/aline.png` 同字节，并与旧原型/legacy 原型 PNG 同字节。不是仅检查文件名、标题或旧 manifest 声明。

复核还读取了 `docs/prototype/` 全部说明、账本、JSON、TSV、核对脚本及 `docs/assets/` 全部文件；核对 PROTO 确认归档及相关 UI/主题现行契约。源/副本的全部字节均被读取核对，全部 JSON/XML 结构完整解析；手工检查 HTML 的加载顺序、各脚本的加载/路径/存储/导入/生成分支和说明中的运行命令。没有用静态解析替代视觉/完整交互验收。`backup-references.tsv` 是原整理时的行文本快照，不是当前依赖表，也不是运行输入。

## 逐主题处置与闭包

| 主题 | 独立归宿、决定及判断 | 读取依赖与删除影响 |
|---|---|---|
| 页面骨架/5 CSS/6 JS/内嵌SVG/MathML | 全字节保存在 legacy；PROTO-04 A 长期只读。主窗/设置/菜单/双模式的历史意图仍保全，不提升旧键位/旧默认/演示能力为现行 UI | HTML 按 `prototype.js → icons.js → menus.js → showcase.js → interactions.js → theme-library.js` 加载。CSS 按 measure/prototype/light/menus/theme-gallery 顺序加载。均在 legacy 内 |
| 源码↔DOM、树/标签/编辑/剪贴板演示 | interactions.js 完整保留，包含 `htmlToMarkdown` 重建、localStorage tabs、关闭无正式保护等历史行为；不转入正式补丁引擎 | 只读内置 ALINE_DEMOS/浏览器存储并写浏览器会话；不读两个待删除目录。没有文件系统持久化或字节保真证明 |
| 原始 README 与来源线索 | legacy/README 原字节保留；外层 README 给正确本机 HTTP 命令、file://限制、存储与当前规范警示；PROTO-03/04 | 旧 `/prototype/` URL、`docs/v1-features.md` 和 Primer URL 是历史说明/来源线索，非启动资源。旧命令不能当当前运行方法 |
| 品牌 PNG | 四处同字节；正式独立参考在 docs/assets；PROTO-01 A 保留并补来源，未制作发布图标 | 标题栏/设置/关于由 legacy/assets 相对加载；现行 PRD/UI 指向 docs/assets。PNG 只有 IHDR/IDAT/IEND，无额外文本/EXIF作者证据 |
| 山丘 SVG | 800×360，原字节随 legacy 保留；PROTO-02 A 限历史原型；无正式示例/夹具资格 | showcase 的 `<img>` 和 interactions 的插入图片示例均为 `./assets/showcase-landscape.svg`；自包含，无外部 href/script/foreignObject |
| 四主题全部配置/三模式预览 | 17 JSON+12 SVG 原字节全部保留；PROTO-05 A 仅视觉候选；正式 schema1/PNG 不由旧文件推导 | `themes/index.json` 四 ID；JS 按 ID 读取 theme.json，再展开 files 三配置和 previews 三模式；当前索引 manifest 值与 ID 路径一致，共29主题有效文件全部闭合 |
| 主题应用/继承/ZIP/字体/IndexedDB | theme-library.js 全保留；schema3/浏览器 FontFace/自定义ZIP/旧预算不能作为正式schema1或原生能力证明 | 内置启动读 legacy 内文件；导入用户选择的ZIP属于浏览器输入，数据留 IndexedDB/localStorage，不读 docs_bak/reports。内置包无字体二进制 |
| 预览生成器与输出 | 原脚本及所有输出均独立归档；未运行，不标为“可重建生成物” | 只依赖 Python 标准库和脚本内 THEMES；会写12 SVG、4 theme.json和index，共17文件。未读当前3配置JSON，会输出含 builtin/colors、缺正式files的另一旧schema1；不能重建现存schema3清单，也不能作为正式生成链 |
| 独立核对工具 | docs/prototype/tools/verify-reference.py 仅标准库，必须输入是 docs/prototype/inventory.json、legacy 文件及复用 PNG | 原件仅在 `source.is_file()` 为真时可选读取核对。没有从 reports 导入脚本/CSV/JSON，也没有必需旧来源；无需修改工具 |
| 当前资产图 | docs/assets 的两架构 SVG 是现行规划图，非旧目录迁移物，均 XML 可解析且自包含；架构图的 `url(#arrow)` 是内部 marker | 不读待删除目录；不是产品截图/原生运行证据 |

JSON 全量结构复查：每家族 application 配置112个叶值、highlight57个、markdown254个，均有 common/modes；四清单均 schema3，ID 与索引一致，`files` 三键和三模式 `previews` 路径完整。所有 13 个历史 SVG 可解析，无脚本、foreignObject、href/事件属性或外部 URL；12 张预览均960×540。外部 GitHub 示例链接是点击导航，Primer 为 README 来源线索，W3C URL 为 XML 命名空间；没有 CDN、在线字体或外部 src 启动资源。模板内的旧 PlantUML 输入被 icons.js 移除，不是必需运行服务。6 JS 均执行 `node --check`，退出0，仅说明语法可解析。

## 本轮实际执行与不可见模拟

执行环境：Python 3.9.6、Node v24.21.0；仅使用系统/现有运行时，无新增依赖。本轮重新执行当前工具，而非引用旧检查结果：

```sh
python3 docs/prototype/tools/verify-reference.py
```

完整输出见 [prototype-check.json](./prototype-check.json)：48清单项，46目标哈希（45 legacy+1复用PNG），45个文件本机HTTP响应200且内容逐字节相同，4主题，15条静态资源引用，errors为空，退出0。

对 `docs_bak/` 与 `reports/` 同时不可见的只读模拟见 [prototype-without-sources.json](./prototype-without-sources.json)。在当前进程临时替换 pathlib 的 `is_file/exists/is_dir`，对这两个路径及其子项返回False；若 `Path.open` 试图读取它们立即失败，然后 runpy 执行原核对脚本。48次源 is_file 查询被屏蔽、missing_originals=48、46目标哈希与45次HTTP仍通过、errors为空、退出0；两个真实目录未操作。由于工具本来不读取 reports，原型工具在 A、B、C 三种场景的必须输入都独立保留。

可在仓库根目录按下列方法重现不可见模拟（仅打印结果，不回写原件）：

```sh
python3 - <<'PY'
from pathlib import Path
from unittest.mock import patch
import runpy
root = Path.cwd()
blocked = [root / 'docs_bak', root / 'reports']
def hidden(path):
    value = path.absolute()
    return any(value == item or item in value.parents for item in blocked)
def absent(original):
    def check(self, *args, **kwargs):
        return False if hidden(self) else original(self, *args, **kwargs)
    return check
original_open = Path.open
def checked_open(self, *args, **kwargs):
    if hidden(self):
        raise AssertionError('尝试读取模拟不可见目录：' + str(self))
    return original_open(self, *args, **kwargs)
with patch.object(Path, 'is_file', absent(Path.is_file)), \
     patch.object(Path, 'exists', absent(Path.exists)), \
     patch.object(Path, 'is_dir', absent(Path.is_dir)), \
     patch.object(Path, 'open', checked_open):
    runpy.run_path(str(root / 'docs/prototype/tools/verify-reference.py'), run_name='__main__')
PY
```

模拟仅覆盖当前已读工具所使用的 pathlib 接口；不是容器级文件隐藏或任意未知程序的隔离测试。这里有原脚本逐段检查相配合，没有以 mock 通过推定全仓所有工具安全。

## 剩余事实、文档引用和限制

PROTO-01/03 的策略已经确认，作者、创作工程/生成记录、适用许可与可公开范围仍缺事实证据。旧目录没有发现独有作者/许可文件，全部已有线索和原字节均在独立位置，不能把保留策略或 author=Aline 字段当作权利证明。来源缺口限制公开再分发/产品复用，不要求重新选择已经确认的内部保留策略，也不因删去重复旧路径而丢失已知证据。

原型说明里的原收尾报告链接属于实际阅读链接，不应只改成无链接文字掩盖问题；由主审统一安置必要 reports 证据并更新消费者。旧历史快照中“待裁决”“不能删除”等原话保留其时间语境；外层现状说明和本次独立报告负责当前结论。原型工具与素材运行闭包本身没有 reports 必需依赖。

本轮没有执行浏览器完整交互、实际断网演练、ZIP导入、用户原型数据编辑、字体激活、原生 GPUI、三平台 IME、资源安全压力或 V-01～V-09。没有运行历史生成器，没有输出新主题包或许可结论。旧静态资源/HTTP/哈希/JS语法检查不等于上述实验通过。

本专项的46个有意义源文件均已完整独立保全，2系统文件可按用户规则忽略内容，无原型/素材迁移阻塞。就本专项而言可以进入删除评估；docs_bak 整体、reports 整体及两者同时删除仍以主报告逐项结论为准，本附件不授予实际删除授权。
