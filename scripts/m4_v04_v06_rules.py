#!/usr/bin/env python3
"""V-04/V-06 的纯规则层：主题包 schema、合并、ZIP 路径、预览/字体格式、
字体角色解析、PL-02 两维跟随，以及由实测冻结的资源预算。

本模块只含常量与纯函数，不读磁盘、不起进程、不编译 Rust。真实探针
`scripts/probe_m4_v04_v06.py` 用它把“契约值”和“实测结果”分开：契约在这里
冻结，实测只负责证明锁定 GPUI / `zip` / `serde_json` / `image` 能否按契约表现。

**预算数值来自实测，不来自旧稿。** 每个预算项在 `BUDGETS` 里带 `basis`
（测量点与观测值）和 `over`（超限样本）；探针按 `boundary` / `over` 两组样本
各跑一次，把“界限内通过、超一单位拒绝”变成可复跑事实。旧稿的
100/120 MB 包体、1 MB 预览、8 字体/20 MB 单个/80 MB 总量、128 字符、
10–72/11–20 px 一律不采用，只在文档里作为对照。

**本模块不是准入结论。** `zip` 8.6.0、`serde_json`、`image`、`gpui-pre`
的依赖准入由任务 1.5 裁决；这里只冻结“包长什么样、限额是多少、拒绝长什么样”。
"""

import json
import math
import unicodedata

# ---------------------------------------------------------------- 基本枚举


class ContractError(ValueError):
    """契约值非法；探针据此失败关闭，不生成证据。"""


MODES = ("light", "dark", "sepia")

# 保留通用角色：包内字体不得占用，样式引用它们由平台映射。
RESERVED_FAMILIES = ("system-ui", "sans-serif", "monospace")

# 保留内置身份：包 id 不得占用。
RESERVED_IDS = ("github", "notion", "vue", "feishu")

# Windows 保留设备名（大小写不敏感，带扩展名同样拒绝）。
DEVICE_NAMES = frozenset(
    ["con", "prn", "aux", "nul"]
    + ["com%d" % i for i in range(1, 10)]
    + ["lpt%d" % i for i in range(1, 10)]
)

# 字体容器格式：只接受静态单 face 的 TTF/OTF。
FONT_SIGNATURES = {
    b"\x00\x01\x00\x00": "ttf",
    b"true": "ttf",
    b"OTTO": "otf",
}
FONT_REJECTED_SIGNATURES = {
    b"ttcf": "ttc",
    b"wOFF": "woff",
    b"wOF2": "woff2",
    b"\x00\x02\x00\x00": "unknown",
}

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


# ---------------------------------------------------------------- 预算表
#
# 每项：value 是界限值；inclusive 表示“等于仍接受”；unit/owner 供实现方引用；
# basis 是冻结依据（实测点与观测值）；over 是超一单位的样本。
# 时间类预算是**守卫**而非性能目标：实测最坏值距界限 60 倍以上，用于兜住
# 病态输入与挂死，不用于宣称性能达标（性能门槛归 V-05）。

BUDGETS = {
    "package_bytes": {
        "value": 64 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme::import（目录预检）",
        "basis": "受限解压阶梯（见「包体 → 受限解压耗时」表）：8 MiB 约 4.6 ms、"
                 "32 MiB 约 18 ms、64 MiB 约 36 ms，耗时随包体线性。64 MiB 是 "
                 "font_total（32 MiB）的两倍，再给四 JSON 与三预览留余量；"
                 "最坏合法包仍在 40 ms 量级完成预检加解压。",
        "over": "65 MiB 包",
    },
    "entry_count": {
        "value": 512,
        "unit": "entries",
        "inclusive": True,
        "owner": "aline-ui::theme::import（目录预检）",
        "basis": "目录枚举阶梯（8 MiB 载荷）：100 条 0.4 ms、1000 条 3 ms、"
                 "5000 条 17 ms、20000 条 100 ms、50000 条 430 ms——条目数 ×500、"
                 "耗时 ×1150，近线性。512 条按线性外推约 1.5 ms。合法包条目数由"
                 "四 JSON + 三预览 + 16 字体×两文件决定，上界 39 条，512 留 13 倍余量。",
        "over": "513 条目",
    },
    "path_bytes": {
        "value": 255,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme::zip（路径校验）",
        "basis": "包内路径是结构常量（previews/light.png、fonts/<file>.ttf），"
                 "校验是逐段字节比较，不随包体增长；255 同时是常见文件系统的"
                 "单段上限，取整段而非单段是更严的口径。",
        "over": "256 字节路径",
    },
    "path_segments": {
        "value": 16,
        "unit": "segments",
        "inclusive": True,
        "owner": "aline-ui::theme::zip（路径校验）",
        "basis": "规范包布局最深为 fonts/<file> 与 licenses/<file>，即 2 段；"
                 "16 段给作者自定义文件名留余量，同时挡住深层别名构造。",
        "over": "17 段路径",
    },
    "compression_ratio": {
        "value": 100,
        "unit": "declared/uncompressed",
        "inclusive": True,
        "owner": "aline-ui::theme::zip（目录预检）",
        "basis": "同尺寸 8 MiB 载荷对照：全零压缩到 8,157 B（1028:1，炸弹特征），"
                 "确定性随机压到 8,391,168 B（1.0:1，deflate 无收益）。合法包是"
                 "JSON 与 PNG，落在两者之间。100:1 给正常包留一个数量级余量，"
                 "同时把 64 MiB 包体的解压上界约束在 6.4 GiB 以内。",
        "over": "1028:1 全零包",
    },
    "extraction_total": {
        "value": 128 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme::import（受限解压）",
        "basis": "解压阶梯 64 MiB 约 36 ms；炸弹包在 8 MiB 界限处第 0 条目中止、"
                 "写满 8,388,608 B 用 1.2 ms，说明预算在解压过程中持续执行。"
                 "128 MiB 是包体上限的两倍，覆盖声明与实际不一致的包。",
        "over": "129 MiB 解压量",
    },
    "extraction_single_file": {
        "value": 64 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme::import（受限解压）",
        "basis": "合法单条目最大是 16 MiB 字体，其次 8 MiB 预览；64 MiB 是单条目"
                 "分配上界，与包体上限一致——单个条目声明超过整个包体即不可信。",
        "over": "65 MiB 单条目",
    },
    "json_bytes": {
        "value": 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "serde_json 解析 1 MiB 填充对象 0.41 ms（2534 成员）；"
                 "合法 theme.json 435 B 0.003 ms。1 MiB 相对合法包是 2400 倍余量，"
                 "解析耗时仍低于 1 ms。",
        "over": "1 MiB + 1 字节",
    },
    "json_depth": {
        "value": 32,
        "unit": "levels",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "serde_json 自身递归上限本轮实测：127、128 层通过，129 层报 "
                 "recursion limit exceeded；规范最深路径为 "
                 "modes.<m>.callout.<kind>.<field>，即 5 层。32 层是 6 倍余量，"
                 "且远低于解析器上限，使超限由本层给出类型化错误而非解析器的"
                 "通用 recursion limit。",
        "over": "33 层",
    },
    "json_members": {
        "value": 4096,
        "unit": "members/object",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "4096 成员对象（53 KiB）解析 0.44 ms；1 MiB 填充对象只有 2534 "
                 "成员，说明字节界限先于成员界限生效。合法对象最大成员数是 "
                 "headings 六级的字段数，远小于 4096。",
        "over": "4097 成员",
    },
    "json_array_items": {
        "value": 4096,
        "unit": "items/array",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "4096 项数组（20 KiB）解析 0.08 ms；合法数组是 Families"
                 "（建议 ≤8）与 fonts（≤16），上界差 256 倍。",
        "over": "4097 项",
    },
    "json_string_scalars": {
        "value": 4096,
        "unit": "scalars",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "4096 标量字符串（4 KiB）解析 0.001 ms；合法 Text 是名称/介绍/"
                 "授权标识，4096 标量已远超展示需要。旧稿 128 字符不采用。",
        "over": "4097 标量",
    },
    "json_string_bytes": {
        "value": 16 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "标量上限 4096 的 4 倍，覆盖 CJK（3 字节）与 emoji（4 字节）"
                 "最坏编码，同时限制单个字符串的分配量。",
        "over": "16 KiB + 1 字节",
    },
    "preview_bytes": {
        "value": 8 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（预览校验）",
        "basis": "本轮噪声 PNG 实测：2048² 16,784,432 B、4096² 67,133,428 B；"
                 "纯色 2048² 只有 87,480 B。8 MiB 允许 2048² 中等噪声截图，"
                 "完全不可压缩的 2048² 会先被字节界限拒绝——这正是字节与像素"
                 "双界限的用途。",
        "over": "8 MiB + 1 字节",
    },
    "preview_total_bytes": {
        "value": 24 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（预览校验）",
        "basis": "三模式预览各按单图上限计，总量取 3 倍。",
        "over": "24 MiB + 1 字节",
    },
    "preview_pixels": {
        "value": 2048 * 2048,
        "unit": "pixels",
        "inclusive": True,
        "owner": "aline-ui::theme（预览头预检）",
        "basis": "GPUI 文档路径解码实测（多轮波动数个百分点，取约值）：512² 约 "
                 "0.3 ms/1 MiB 位图、1024² 约 1 ms/4 MiB、2048² 约 4 ms/16 MiB、"
                 "4096² 约 16 ms/67 MiB、8192² 约 60 ms/268 MiB。2048² 是预览展示的"
                 "合理上限，解码约 4 ms、位图 16 MiB。",
        "over": "2049×2048",
    },
    "preview_decode_bytes": {
        "value": 48 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（预览解码）",
        "basis": "2048² 位图 16,777,216 B（宽×高×4×帧），三图共 48 MiB；"
                 "帧数固定为 1（APNG 拒绝），所以位图量由像素上限唯一决定。",
        "over": "三图超 48 MiB",
    },
    "font_count": {
        "value": 16,
        "unit": "files",
        "inclusive": True,
        "owner": "aline-ui::theme（字体清单）",
        "basis": "注册实测（多轮波动数个百分点，取约值）：1 KiB 约 0.09 ms、"
                 "1.7 MB 约 0.25 ms、15.3 MB 约 1.1 ms、23.3 MB 约 1.6 ms；"
                 "耗时随字节近线性。16 个字体在最坏总字节下约 26 ms。"
                 "合法主题通常 1～4 个 face。",
        "over": "17 个字体",
    },
    "font_single_bytes": {
        "value": 16 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（字体校验）",
        "basis": "Arial Unicode 23,278,008 B 注册约 1.6 ms；16 MiB 覆盖绝大多数"
                 "含 CJK 的静态字体（AppleGothic 15.3 MB 在其内），同时把单文件"
                 "分配限制在可控量级。",
        "over": "16 MiB + 1 字节",
    },
    "font_total_bytes": {
        "value": 32 * 1024 * 1024,
        "unit": "bytes",
        "inclusive": True,
        "owner": "aline-ui::theme（字体清单）",
        "basis": "16 个字体 × 16 MiB 的上界由包体 64 MiB 与解压 128 MiB 共同约束；"
                 "32 MiB 覆盖四字体×8 MiB 的常见组合。",
        "over": "32 MiB + 1 字节",
    },
    # 时间守卫：不是性能目标。
    "json_parse_time": {
        "value": 500,
        "unit": "ms",
        "inclusive": True,
        "owner": "aline-ui::theme（JSON 读取）",
        "basis": "1 MiB 解析实测 0.41 ms，界限是 1200 倍余量；用于兜住病态输入，"
                 "与取消标志共同终止，不只靠超时隐藏结果。",
        "over": "病态输入（由取消而非时钟触发）",
    },
    "preview_decode_time": {
        "value": 1000,
        "unit": "ms",
        "inclusive": True,
        "owner": "aline-ui::theme（预览解码）",
        "basis": "2048² 解码实测约 4 ms（release，本探针只跑 release；多轮波动"
                 "数个百分点）；界限是 240 倍余量，用于兜住病态输入。"
                 "超时即判失败，不返回部分位图。",
        "over": "病态输入（由取消而非时钟触发）",
    },
    "font_prepare_time": {
        "value": 2000,
        "unit": "ms",
        "inclusive": True,
        "owner": "aline-ui::theme（字体准备）",
        "basis": "16 字体 × 约 1.6 ms 实测约 26 ms，界限是 77 倍余量。",
        "over": "病态输入（由取消而非时钟触发）",
    },
    "verification_time": {
        "value": 5000,
        "unit": "ms",
        "inclusive": True,
        "owner": "aline-ui::theme::import（整体）",
        "basis": "最坏合法包各段实测之和（取约值）：解压约 36 + 目录预检约 1.5 + "
                 "四 JSON 约 0.02 + 三预览约 12 + 字体约 26 ≈ 76 ms；"
                 "5 s 是 65 倍余量，覆盖慢盘与冷缓存。",
        "over": "病态输入（由取消而非时钟触发）",
    },
}


def budget(name):
    """返回冻结的预算值；名字不存在即契约错误，不静默返回 None。"""
    try:
        return BUDGETS[name]["value"]
    except KeyError as error:
        raise ContractError("未知预算项: %s" % name) from error


def over_limit(name, value):
    """按 inclusive 语义判断超限；等号是否接受由表决定。"""
    entry = BUDGETS[name]
    return value > entry["value"] if entry["inclusive"] else value >= entry["value"]


def boundary_sample(name):
    """界限样本：恰好等于上限。"""
    return BUDGETS[name]["value"]


def over_sample(name):
    """超一单位样本：整数上限 + 1。"""
    return BUDGETS[name]["value"] + 1


# ---------------------------------------------------------------- 字段 schema
#
# 叶子是类型名，对象是 dict。类型名对应下面的 `_type_ok`。
# 三个配置文件的公共外壳是 {common?, modes?}，由 `document_violations` 单独处理。

_COLORS = {
    "background": "Color", "body": "Color", "muted": "Color", "marker": "Color",
    "selection": "Color", "caret": "Color", "error": "Color",
    "errorBackground": "Color",
}

_HEADING = {
    "families": "Families", "size": "PxPositive", "weight": "Weight",
    "lineHeight": "Ratio", "color": "Color", "align": "Align",
    "marginBefore": "PxNonNegative", "marginAfter": "PxNonNegative",
}

_CALLOUT = {"color": "Color", "background": "Color", "border": "Color"}

MARKDOWN_SCHEMA = {
    "colors": dict(_COLORS),
    "typography": {
        "bodyFamilies": "Families", "codeFamilies": "Families",
        "bodySize": "PxPositive", "sourceSize": "PxPositive",
        "bodyWeight": "Weight", "lineHeight": "Ratio",
        "sourceLineHeight": "Ratio", "bodyAlign": "Align",
    },
    "headings": {name: dict(_HEADING) for name in
                 ("h1", "h2", "h3", "h4", "h5", "h6")},
    "spacing": {
        "paragraphAfter": "PxNonNegative", "listItemGap": "PxNonNegative",
        "blockBefore": "PxNonNegative", "blockAfter": "PxNonNegative",
        "quotePadding": "PxNonNegative",
    },
    "inline": {
        "strong": {"weight": "Weight", "color": "Color"},
        "emphasis": {"color": "Color"},
        "strike": {"color": "Color"},
        "link": {"color": "Color", "underline": "bool"},
        "codeSpan": {"background": "Color", "color": "Color",
                     "radius": "PxNonNegative"},
    },
    "list": {"markerColor": "Color"},
    "blockquote": {"color": "Color", "background": "Color", "border": "Color",
                   "lineColor": "Color"},
    "task": {"color": "Color", "background": "Color", "border": "Color",
             "accent": "Color"},
    "rule": {"color": "Color"},
    "image": {"border": "Color"},
    "callout": {name: dict(_CALLOUT) for name in
                ("note", "tip", "important", "warning", "caution")},
    "readOnlyBlock": {"color": "Color", "background": "Color", "border": "Color"},
    "formula": {"color": "Color", "background": "Color", "border": "Color"},
    "codeBlock": {
        "background": "Color", "color": "Color", "border": "Color",
        "lineNumberColor": "Color", "lineNumberBackground": "Color",
        "radius": "PxNonNegative", "paddingX": "PxNonNegative",
        "paddingY": "PxNonNegative", "fontSize": "PxPositive",
        "lineHeight": "Ratio",
    },
    "table": {
        "headerBackground": "Color", "headerColor": "Color",
        "cellBackground": "Color", "alternateBackground": "Color",
        "border": "Color", "headerWeight": "Weight", "zebra": "bool",
        "cellPaddingX": "PxNonNegative", "cellPaddingY": "PxNonNegative",
        "rowLineHeight": "Ratio",
    },
}

HIGHLIGHT_SCHEMA = {
    "codeBlock": {
        "background": "Color", "color": "Color", "border": "Color",
        "lineNumberColor": "Color", "lineNumberBackground": "Color",
    },
    "syntax": {name: "Color" for name in (
        "comment", "keyword", "string", "number", "function", "type",
        "operator", "punctuation", "tag", "attribute", "variable", "macro",
        "inserted", "deleted")},
}

APPLICATION_SCHEMA = {
    "typography": {
        "uiFamilies": "Families", "uiSize": "PxPositive", "uiWeight": "Weight",
    },
    "colors": {name: "Color" for name in (
        "window", "surface", "sidebar", "toolbar", "tabBar", "statusBar",
        "menu", "input", "text", "muted", "accent", "border", "selection",
        "hover", "focus", "danger")},
    "window": {"backgroundOpacity": "Opacity"},
    "icons": {
        "style": "IconStyle", "strokeWidth": "PxPositive",
        "default": "Color", "muted": "Color", "accent": "Color",
        "active": "Color", "disabled": "Color", "danger": "Color",
    },
    "fileTree": {name: {"style": "IconStyle", "color": "Color"} for name in (
        "folderClosed", "folderOpen", "markdownFile", "imageFile", "otherFile")},
}

DOCUMENT_SCHEMAS = {
    "markdown": MARKDOWN_SCHEMA,
    "highlight": HIGHLIGHT_SCHEMA,
    "application": APPLICATION_SCHEMA,
}

# theme.json 的封闭字段与类型；`files` / `previews` / `fonts` 另有结构检查。
MANIFEST_FIELDS = {
    "schemaVersion": "SchemaVersion",
    "id": "Id",
    "name": "Text",
    "description": "Text",
    "author": "Text",
    "version": "Version",
    "inherits": "Inherits",
    "files": "Files",
    "previews": "Previews",
    "fonts": "FontList",
}
MANIFEST_REQUIRED = ("schemaVersion", "id", "name", "version", "files", "previews")
MANIFEST_FILE_KEYS = {"markdown": "markdown.json",
                      "application": "application.json",
                      "highlight": "highlight.json"}
MANIFEST_PREVIEW_MODES = ("light", "dark", "sepia")
FONT_ENTRY_FIELDS = ("path", "family", "license", "licenseFile")

# 颜色/尺寸/行高的可用范围。**这三组不是本轮实测冻结的预算**：它们不在
# `BUDGETS` 里，也没有对应的 `basis`/`over` 样本；规范 §3.2 把字号/行高的
# 可用上下限归 V-04/V-09 共同给出，而 V-09 的视觉金样与锁定 GPUI 的极端布局
# 实测都还没做。这里的数值只是**当前契约占位**（等号接受），M4 接线前必须
# 由 V-09 实测替换，不得当成已冻结的发布限额引用。
PX_POSITIVE_RANGE = (1.0, 512.0)
PX_NON_NEGATIVE_RANGE = (0.0, 512.0)
RATIO_RANGE = (0.5, 5.0)
OPACITY_RANGE = (0.0, 1.0)


# ---------------------------------------------------------------- 类型校验


def _is_finite_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) \
        and math.isfinite(value)


def _color_ok(value):
    if not isinstance(value, str) or len(value) != 7 or not value.startswith("#"):
        return False
    return all(char in "0123456789abcdefABCDEF" for char in value[1:])


def _text_ok(value, allow_control=False):
    if not isinstance(value, str):
        return False
    if "\x00" in value:
        return False
    if not allow_control:
        # 除制表/换行外不接受 C0/C1 控制字符。
        return not any(ord(char) < 0x20 and char not in "\t\n"
                       or 0x7f <= ord(char) <= 0x9f for char in value)
    return True


def _families_ok(value):
    return (isinstance(value, list) and len(value) > 0
            and all(_text_ok(item) and item.strip() != "" for item in value))


def _scalar_ok(value, type_name):
    """叶子类型校验；Path/Id/Version 等结构化类型由调用方另行检查。"""
    if type_name == "Color":
        return _color_ok(value)
    if type_name == "Text":
        return _text_ok(value)
    if type_name == "Families":
        return _families_ok(value)
    if type_name == "PxPositive":
        return (_is_finite_number(value)
                and PX_POSITIVE_RANGE[0] <= value <= PX_POSITIVE_RANGE[1])
    if type_name == "PxNonNegative":
        return (_is_finite_number(value)
                and PX_NON_NEGATIVE_RANGE[0] <= value <= PX_NON_NEGATIVE_RANGE[1])
    if type_name == "Ratio":
        return (_is_finite_number(value)
                and RATIO_RANGE[0] <= value <= RATIO_RANGE[1])
    if type_name == "Opacity":
        return (_is_finite_number(value)
                and OPACITY_RANGE[0] <= value <= OPACITY_RANGE[1])
    if type_name == "Weight":
        return isinstance(value, int) and not isinstance(value, bool) \
            and value in (100, 200, 300, 400, 500, 600, 700, 800, 900)
    if type_name == "Align":
        return value in ("left", "center", "right", "justify")
    if type_name == "IconStyle":
        return value in ("outline", "filled")
    if type_name == "bool":
        return isinstance(value, bool)
    raise ContractError("未知叶子类型: %s" % type_name)


def _walk(document, schema, path, violations):
    if not isinstance(document, dict):
        violations.append("%s: 必须是对象，实际 %s" % (path or "<root>",
                                                      type(document).__name__))
        return
    for key, value in document.items():
        child = "%s.%s" % (path, key) if path else key
        if key not in schema:
            violations.append("%s: 未知字段" % child)
            continue
        expected = schema[key]
        if isinstance(expected, dict):
            if not isinstance(value, dict):
                violations.append("%s: 必须是对象，实际 %s"
                                  % (child, type(value).__name__))
            else:
                _walk(value, expected, child, violations)
        elif value is None:
            violations.append("%s: null 不是重置语法，整包拒绝" % child)
        elif not _scalar_ok(value, expected):
            violations.append("%s: 期望 %s，实际 %r" % (child, expected, value))


def document_violations(document, kind):
    """校验 markdown/highlight/application 单个配置文件（不含继承）。

    顶层只允许 common/modes；modes 只允许三模式；每层先独立校验再合并。
    """
    if kind not in DOCUMENT_SCHEMAS:
        raise ContractError("未知配置文件: %s" % kind)
    schema = DOCUMENT_SCHEMAS[kind]
    violations = []
    if not isinstance(document, dict):
        return ["<root>: 必须是对象"]
    for key in document:
        if key not in ("common", "modes"):
            violations.append("%s: 未知顶层字段" % key)
    common = document.get("common", {})
    _walk(common, schema, "common", violations)
    modes = document.get("modes", {})
    if not isinstance(modes, dict):
        violations.append("modes: 必须是对象")
    else:
        for mode, value in modes.items():
            if mode not in MODES:
                violations.append("modes.%s: 未知模式" % mode)
                continue
            _walk(value, schema, "modes.%s" % mode, violations)
    return violations


def leaf_paths(schema, path=""):
    """展开 schema 的全部叶子路径，供“基线必须逐叶完整”的自检使用。"""
    leaves = []
    for key, value in schema.items():
        child = "%s.%s" % (path, key) if path else key
        if isinstance(value, dict):
            leaves.extend(leaf_paths(value, child))
        else:
            leaves.append(child)
    return leaves


def missing_leaves(document, schema, path=""):
    """返回有效文档相对 schema 缺失的叶子路径。"""
    missing = []
    for key, value in schema.items():
        child = "%s.%s" % (path, key) if path else key
        if isinstance(value, dict):
            nested = document.get(key, {}) if isinstance(document, dict) else {}
            if not isinstance(nested, dict):
                missing.append(child)
                continue
            missing.extend(missing_leaves(nested, value, child))
        else:
            if not isinstance(document, dict) or key not in document:
                missing.append(child)
    return missing


# ---------------------------------------------------------------- JSON 前置检查


def json_text_violations(data):
    """按规范 §9.2 对**原始字节**做解析前置检查，返回 `(value, violations)`。

    为什么不能只依赖反序列化：`serde_json` 接受重复 key（后者覆盖前者），
    Python 的 `json` 接受 NaN/Infinity 且会悄悄去掉 BOM；规范要求这些在解析
    时就拒绝。所以这一层自己检查 BOM、UTF-8、重复 key、非有限数、顶层类型，
    以及字节/深度/成员/数组/字符串预算。

    深度口径与 Rust 探针一致：顶层对象算 1 层，嵌套加 1；字符串算 1 层。
    """
    violations = []
    if not isinstance(data, (bytes, bytearray)):
        return None, ["JSON 必须是字节序列"]
    raw = bytes(data)
    if raw.startswith(b"\xef\xbb\xbf"):
        return None, ["拒绝 BOM"]
    if over_limit("json_bytes", len(raw)):
        violations.append("超过 %d 字节上限" % budget("json_bytes"))
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        return None, violations + ["不是合法 UTF-8: %s" % error]

    duplicates = []
    constants = []

    def pairs_hook(pairs):
        seen = set()
        for key, _value in pairs:
            if key in seen:
                duplicates.append(key)
            seen.add(key)
        return dict(pairs)

    def constant_hook(name):
        constants.append(name)
        return 0

    try:
        value = json.loads(text, object_pairs_hook=pairs_hook,
                           parse_constant=constant_hook)
    except ValueError as error:
        return None, violations + ["JSON 解析失败: %s" % error]
    if constants:
        violations.append("非有限数: %s" % ", ".join(sorted(set(constants))))
    if duplicates:
        violations.append("重复 key: %s" % ", ".join(sorted(set(duplicates))))
    if not isinstance(value, dict):
        violations.append("顶层必须是对象")
        return value, violations
    shape = _json_shape(value)
    for name, measured in (
            ("json_depth", shape["depth"]),
            ("json_members", shape["max_members"]),
            ("json_array_items", shape["max_array_items"]),
            ("json_string_scalars", shape["max_string_scalars"]),
            ("json_string_bytes", shape["max_string_bytes"])):
        if over_limit(name, measured):
            violations.append("%s 实测 %d 超过上限 %d"
                              % (name, measured, budget(name)))
    return value, violations


def _json_shape(value):
    """与 `probe-zip` 的 `shape_of` 同口径的形状统计。"""
    shape = {"depth": 1, "max_members": 0, "max_array_items": 0,
             "max_string_scalars": 0, "max_string_bytes": 0}
    if isinstance(value, dict):
        shape["max_members"] = len(value)
        children = value.values()
    elif isinstance(value, list):
        shape["max_array_items"] = len(value)
        children = value
    elif isinstance(value, str):
        shape["max_string_scalars"] = len(value)
        shape["max_string_bytes"] = len(value.encode("utf-8"))
        return shape
    else:
        return shape
    for child in children:
        inner = _json_shape(child)
        shape["depth"] = max(shape["depth"], inner["depth"] + 1)
        for key in ("max_members", "max_array_items",
                    "max_string_scalars", "max_string_bytes"):
            shape[key] = max(shape[key], inner[key])
    return shape


# ---------------------------------------------------------------- 深合并


def deep_merge(base, overlay, path=""):
    """递归合并：对象逐叶、数组整体替换、标量覆盖、null 拒绝。

    调用方必须先对每层单独跑 `document_violations`；本函数不再重复类型校验，
    只实现 §6.1 的组合语义。
    """
    if not isinstance(base, dict) or not isinstance(overlay, dict):
        raise ContractError("合并双方必须是对象: %s" % (path or "<root>"))
    result = dict(base)
    for key, value in overlay.items():
        child = "%s.%s" % (path, key) if path else key
        if value is None:
            raise ContractError("%s: null 不能作为覆盖值" % child)
        if isinstance(value, dict):
            existing = result.get(key, {})
            if not isinstance(existing, dict):
                raise ContractError("%s: 不能把标量替换成对象" % child)
            result[key] = deep_merge(existing, value, child)
        else:
            result[key] = value
    return result


def effective_style(baseline, *layers):
    """G[D,m] / T[D,m] 的求值：基线 → 各层依次深合并。"""
    result = baseline
    for layer in layers:
        result = deep_merge(result, layer)
    return result


def effective_mode(document, mode, baseline_common, baseline_modes):
    """按 §6.1 求 T[D,m]：G[D,m] 递归合并 common，再合并 modes[m]。

    `baseline_common` / `baseline_modes` 是受保护 GitHub 基线的 common 与
    三模式对象。缺模式等同 `{}`。
    """
    if mode not in MODES:
        raise ContractError("未知模式: %s" % mode)
    baseline = deep_merge(baseline_common, baseline_modes.get(mode, {}))
    return effective_style(baseline, document.get("common", {}),
                           document.get("modes", {}).get(mode, {}))


# ---------------------------------------------------------------- 清单


def _id_ok(value):
    if not isinstance(value, str) or not value:
        return False
    if not value[0].isascii() or not value[0].islower():
        return False
    if value.endswith("-"):
        return False
    return all(char.isascii() and (char.islower() or char.isdigit() or char == "-")
               for char in value)


def _version_ok(value):
    if not isinstance(value, str):
        return False
    parts = value.split(".")
    if len(parts) != 3:
        return False
    for part in parts:
        if not part or not part.isascii() or not part.isdigit():
            return False
        if len(part) > 1 and part[0] == "0":
            return False
        if len(part) > 8:
            return False
    return True


def manifest_violations(manifest):
    """theme.json 的封闭字段、身份、files/previews/fonts 结构检查。"""
    violations = []
    if not isinstance(manifest, dict):
        return ["<root>: 必须是对象"]
    for key in manifest:
        if key not in MANIFEST_FIELDS:
            violations.append("%s: 未知字段" % key)
    for key in MANIFEST_REQUIRED:
        if key not in manifest:
            violations.append("%s: 缺少必需字段" % key)
    if "schemaVersion" in manifest:
        value = manifest["schemaVersion"]
        if value != 1 or isinstance(value, bool):
            violations.append("schemaVersion: 只接受整数 1，实际 %r" % (value,))
    if "id" in manifest and not _id_ok(manifest["id"]):
        violations.append("id: 不符合 ASCII 小写标识规则")
    elif isinstance(manifest.get("id"), str) and manifest["id"].lower() in RESERVED_IDS:
        violations.append("id: 保留给内置身份")
    if "version" in manifest and not _version_ok(manifest["version"]):
        violations.append("version: 必须为 major.minor.patch 且无前导零")
    for key in ("name", "description", "author"):
        if key in manifest and not _text_ok(manifest[key], allow_control=(key != "name")):
            violations.append("%s: 非法文本" % key)
    if "inherits" in manifest and manifest["inherits"] != "github":
        violations.append("inherits: 只接受 github")
    files = manifest.get("files")
    if files is not None:
        if not isinstance(files, dict):
            violations.append("files: 必须是对象")
        else:
            for key in files:
                if key not in MANIFEST_FILE_KEYS:
                    violations.append("files.%s: 未知职责" % key)
            for key, expected in MANIFEST_FILE_KEYS.items():
                if files.get(key) != expected:
                    violations.append("files.%s: 必须是 %s" % (key, expected))
    previews = manifest.get("previews")
    if previews is not None:
        if not isinstance(previews, dict):
            violations.append("previews: 必须是对象")
        else:
            for key in previews:
                if key not in MANIFEST_PREVIEW_MODES:
                    violations.append("previews.%s: 未知模式" % key)
            seen = []
            for mode in MANIFEST_PREVIEW_MODES:
                path = previews.get(mode)
                if not isinstance(path, str) or not path:
                    violations.append("previews.%s: 缺少路径" % mode)
                    continue
                seen.append(path)
                if not path.startswith("previews/") or not path.endswith(".png"):
                    violations.append("previews.%s: 必须位于 previews/ 且为 .png" % mode)
            if len(set(seen)) != len(seen):
                violations.append("previews: 三模式路径必须互异")
    fonts = manifest.get("fonts")
    if fonts is not None:
        if not isinstance(fonts, list):
            violations.append("fonts: 必须是数组")
        else:
            if len(fonts) > budget("font_count"):
                violations.append("fonts: 字体数超过上限 %d 个" % budget("font_count"))
            paths = []
            for index, entry in enumerate(fonts):
                label = "fonts[%d]" % index
                if not isinstance(entry, dict):
                    violations.append("%s: 必须是对象" % label)
                    continue
                for key in entry:
                    if key not in FONT_ENTRY_FIELDS:
                        violations.append("%s.%s: 未知字段" % (label, key))
                for key in FONT_ENTRY_FIELDS:
                    if key not in entry:
                        violations.append("%s.%s: 缺少必需字段" % (label, key))
                path = entry.get("path")
                if isinstance(path, str):
                    paths.append(path)
                    if not path.startswith("fonts/"):
                        violations.append("%s.path: 必须位于 fonts/" % label)
                    if not path.lower().endswith((".ttf", ".otf")):
                        violations.append("%s.path: 只接受 .ttf/.otf" % label)
                    violations.extend("%s.path: %s" % (label, item)
                                      for item in zip_entry_violations(path))
                family = entry.get("family")
                if isinstance(family, str) and family.lower() in RESERVED_FAMILIES:
                    violations.append("%s.family: 不得占用保留角色名" % label)
                license_file = entry.get("licenseFile")
                if isinstance(license_file, str):
                    if not license_file.startswith("licenses/"):
                        violations.append("%s.licenseFile: 必须位于 licenses/" % label)
                    if not license_file.lower().endswith(".txt"):
                        violations.append("%s.licenseFile: 只接受 .txt" % label)
                    violations.extend("%s.licenseFile: %s" % (label, item)
                                      for item in zip_entry_violations(license_file))
            if len(set(paths)) != len(paths):
                violations.append("fonts: 同一路径不能重复登记")
    return violations


# ---------------------------------------------------------------- ZIP 路径


def zip_entry_violations(name, is_dir=False):
    """§9.1 规则 2 的纯实现；`name` 是 ZIP 解码后的原始条目名。

    只做结构判断，不访问文件系统。目录条目允许一个终止 `/`，剥离后再分段。
    """
    violations = []
    if not isinstance(name, str) or name == "":
        return ["路径为空"]
    if "\x00" in name:
        violations.append("含 NUL")
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in name):
        violations.append("含控制字符")
    if "\\" in name:
        violations.append("含反斜杠")
    body = name
    if body.endswith("/"):
        if not is_dir:
            violations.append("普通文件不允许终止 /")
        body = body[:-1]
    if body.endswith("/"):
        violations.append("重复终止 /")
    if body.startswith("/"):
        violations.append("POSIX 根路径")
    if len(body) >= 2 and body[1] == ":":
        violations.append("Windows 盘符")
    if body.startswith("//"):
        violations.append("UNC 路径")
    if len(body.encode("utf-8")) > budget("path_bytes"):
        violations.append("超过 %d 字节" % budget("path_bytes"))
    segments = body.split("/")
    if len(segments) > budget("path_segments"):
        violations.append("超过 %d 段" % budget("path_segments"))
    for segment in segments:
        if segment == "":
            violations.append("空段")
            continue
        if segment in (".", ".."):
            violations.append("相对段 %s" % segment)
        if segment.endswith(" ") or segment.endswith("."):
            violations.append("段 %r 以空格或点结尾" % segment)
        if ":" in segment:
            violations.append("段 %r 含 ADS 冒号" % segment)
        stem = segment.split(".", 1)[0].lower()
        if stem in DEVICE_NAMES:
            violations.append("段 %r 是平台保留设备名" % segment)
    return violations


def collision_key(path):
    """跨平台等价键：NFC 规范化 + 不依赖语言的大小写折叠。

    规范化只用于**比较**，不改写原路径继续使用——把恶意路径改成合法后继续
    是规范明确禁止的。
    """
    return unicodedata.normalize("NFC", path).casefold()


def collision_pairs(names):
    """返回规范化/大小写折叠后等价的原始名对；空列表表示无碰撞。"""
    buckets = {}
    for name in names:
        buckets.setdefault(collision_key(name), []).append(name)
    pairs = []
    for key, group in buckets.items():
        if len(group) > 1:
            pairs.append(tuple(group))
    return sorted(pairs)


def ancestor_conflicts(entries):
    """文件/目录同名或祖先文件冲突：`fonts` 是**文件**又有 `fonts/a.ttf`。

    `entries` 是 `(name, is_dir)` 序列。真实目录条目与其子条目共存是合法的，
    所以只有“非目录条目”成为祖先时才记冲突；同名但一个文件一个目录也拒绝。
    """
    by_key = {}
    conflicts = []
    for name, is_dir in entries:
        body = name.rstrip("/")
        key = collision_key(body)
        if key in by_key:
            other_name, other_is_dir = by_key[key]
            conflicts.append((other_name, name))
        else:
            by_key[key] = (name, is_dir)
    for name, is_dir in entries:
        segments = name.rstrip("/").split("/")
        for index in range(1, len(segments)):
            prefix = collision_key("/".join(segments[:index]))
            if prefix in by_key and not by_key[prefix][1]:
                conflicts.append((by_key[prefix][0], name))
                break
    return sorted(set(conflicts))


# ---------------------------------------------------------------- 预览格式


def preview_verdict(data):
    """判定预览资源：只接受单帧 PNG。

    返回 `(kind, width, height, animated, error)`。`kind` 是识别到的格式
    （png/jpeg/gif/webp/svg/apng/unknown），`error` 为 None 表示格式合格。
    APNG 用 `acTL` 块识别——PNG 签名与 IHDR 与普通 PNG 相同，只有动画块能区分。
    """
    if not isinstance(data, (bytes, bytearray)) or len(data) < 8:
        return ("unknown", None, None, False, "资源过短")
    head = bytes(data[:8])
    if head.startswith(b"\xff\xd8\xff"):
        return ("jpeg", None, None, False, "预览只接受 PNG")
    if head.startswith(b"GIF87a") or head.startswith(b"GIF89a"):
        return ("gif", None, None, False, "预览只接受 PNG")
    if head.startswith(b"RIFF") and bytes(data[8:12]) == b"WEBP":
        return ("webp", None, None, False, "预览只接受 PNG")
    if head.lstrip().startswith(b"<"):
        return ("svg", None, None, False, "预览只接受 PNG")
    if head != PNG_SIGNATURE:
        return ("unknown", None, None, False, "不是 PNG 签名")
    width = height = None
    animated = False
    saw_iend = False
    offset = 8
    while offset + 12 <= len(data):
        length = int.from_bytes(data[offset:offset + 4], "big")
        chunk = bytes(data[offset + 4:offset + 8])
        # 块头声明了长度但数据不够，说明文件被截断——不能只看签名和 IHDR
        # 就放行，否则半个 PNG 也能“通过格式检查”再在解码时才失败。
        if offset + 12 + length > len(data):
            return ("png", width, height, animated, "PNG 数据被截断（块不完整）")
        if chunk == b"IHDR" and length >= 8:
            width = int.from_bytes(data[offset + 8:offset + 12], "big")
            height = int.from_bytes(data[offset + 12:offset + 16], "big")
        if chunk == b"acTL":
            animated = True
        if chunk == b"IEND":
            saw_iend = True
            break
        offset += 12 + length
    if width is None or height is None:
        return ("png", None, None, animated, "缺少 IHDR")
    if not saw_iend:
        return ("png", width, height, animated, "PNG 数据被截断（缺少 IEND）")
    if animated:
        return ("apng", width, height, True, "拒绝 APNG")
    return ("png", width, height, False, None)


def preview_within_budget(width, height, source_bytes):
    """按字节与像素双界限判定；两者都必须通过。"""
    violations = []
    if over_limit("preview_bytes", source_bytes):
        violations.append("超过单图 %d 字节" % budget("preview_bytes"))
    pixels = width * height
    if over_limit("preview_pixels", pixels):
        violations.append("超过 %d 像素" % budget("preview_pixels"))
    return violations


# ---------------------------------------------------------------- 字体格式


def font_verdict(data, extension):
    """判定字体容器：只接受与扩展名一致的静态 TTF/OTF。"""
    if not isinstance(data, (bytes, bytearray)) or len(data) < 4:
        return (None, "字体数据过短")
    head = bytes(data[:4])
    if head in FONT_REJECTED_SIGNATURES:
        return (FONT_REJECTED_SIGNATURES[head], "不接受 %s 容器"
                % FONT_REJECTED_SIGNATURES[head])
    kind = FONT_SIGNATURES.get(head)
    if kind is None:
        return (None, "无法识别的字体签名")
    if extension.lower().lstrip(".") != kind:
        return (kind, "扩展名与实际格式不一致")
    return (kind, None)


def reserved_family(name):
    """通用角色名是否被占用（大小写不敏感）。"""
    return isinstance(name, str) and name.lower() in RESERVED_FAMILIES


def resolve_family(requested, package_families, builtin_families, system_families):
    """按“包内登记 → 应用内置 → 本机系统”解析一个非通用族名。

    返回 `(source, family)`；三处都没有返回 `(None, None)`。通用角色不走此函数：
    它们由平台映射，包内同名 family 已在清单层被拒绝。
    """
    if requested in package_families:
        return ("package", requested)
    if requested in builtin_families:
        return ("builtin", requested)
    if requested in system_families:
        return ("system", requested)
    return (None, None)


def family_chain(families, package_families, builtin_families, system_families):
    """有序回退：逐项解析，通用角色原样保留为平台映射，返回首个命中。"""
    for family in families:
        if reserved_family(family):
            return ("platform-role", family)
        source, resolved = resolve_family(family, package_families,
                                          builtin_families, system_families)
        if source is not None:
            return (source, resolved)
    return (None, None)


# ---------------------------------------------------------------- PL-02


def pl02_surfaces(highlight_enabled, code_family_follow, code_mode_follow,
                  app_family, app_mode, content_theme, highlight_theme,
                  fixed_family, fixed_mode, paused=None):
    """按已确认 PL-02 求代码块的配色来源与两维解析结果。

    参数是“当前应用家族/模式”与“保留的固定配置”。关闭高亮时配色回当前
    内容主题的普通代码表面，独立代码主题/模式**保留但暂停**，再启用按当前
    有效配置恢复；固定维度不被应用变化覆盖，跟随维度取当前应用设置。
    返回 `(colors, family, mode, paused_config)`。
    """
    if not highlight_enabled:
        colors = content_theme
        family = app_family if code_family_follow else fixed_family
        mode = app_mode if code_mode_follow else fixed_mode
        return {
            "source": "content-theme",
            "colors": colors,
            "family": family,
            "mode": mode,
            # 暂停配置原样保留，关闭期间不被改写。
            "paused": paused if paused is not None else {
                "family": fixed_family, "mode": fixed_mode,
                "familyFollow": code_family_follow, "modeFollow": code_mode_follow,
            },
        }
    family = app_family if code_family_follow else fixed_family
    mode = app_mode if code_mode_follow else fixed_mode
    return {
        "source": "highlight-theme",
        "colors": highlight_theme,
        "family": family,
        "mode": mode,
        "paused": None,
    }


# ---------------------------------------------------------------- 默认配置


def default_config():
    """冻结的主题默认配置。

    只冻结**非视觉**默认与产品已确认的六个基准色；其余基线色值仍由
    V-04/V-09 视觉金样形成，本轮不抄原型充当默认表（规范 §5.1 明令）。
    """
    return {
        "family": "github",
        "mode": "follow-system",
        "systemMapping": {"light": "light", "dark": "dark"},
        "manualModeClearsFollow": True,
        "familySwitchKeepsMode": True,
        "codeHighlight": {
            "enabled": True,
            "familyFollow": True,
            "modeFollow": True,
            "fixedFamily": None,
            "fixedMode": None,
        },
        "backgroundOpacity": 1.0,
        "opacityFallback": "same-rgb-opaque",
        "userOverrides": {},
        "previewApplies": False,
        "installApplies": False,
        "reservedFamilies": list(RESERVED_FAMILIES),
        "reservedIds": list(RESERVED_IDS),
        # PRD §307 已确认的默认 GitHub 浅色基准色，仅此六项是已确认数值。
        "confirmedBaselineLight": {
            "canvas": "#FFFFFF",
            "body": "#1F2328",
            "muted": "#59636E",
            "border": "#D1D9E0",
            "accent": "#0969DA",
            "codeBackground": "#F6F8FA",
        },
        "unfrozen": "GitHub 基线的其余叶值与 dark/sepia 数值由 V-04/V-09 视觉金样形成",
    }


def baseline_requirement():
    """受保护 GitHub 基线必须逐叶完整；返回每个配置文件的必需叶子。"""
    return {kind: leaf_paths(schema) for kind, schema in DOCUMENT_SCHEMAS.items()}
