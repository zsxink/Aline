#!/usr/bin/env python3
"""V-02 导出契约的纯规则层：纸型、方向、四边页边距与分页预算。

本模块只含常量与纯函数，不读磁盘、不起进程，因此可在没有 Swift、PDFKit、
Quick Look 或候选 crate 缓存的环境里独立自测。真实探针脚本
`scripts/probe_m4_export.py` 用它把“契约值”和“产物实测”分开：契约在这里
冻结，实测只负责证明候选后端能按契约落纸。

单位统一为 PostScript point（1pt = 1/72 in），与 PDF MediaBox 一致；
纸型表按 ISO 216 / ANSI 的名义尺寸换算，四舍五入到 0.01pt 后与 PDF 阅读器
报告的 MediaBox 比较（容差见 TOLERANCE_PT）。
"""

import math

# 允许的容差：阅读器回读的 MediaBox 与契约值之差上限（pt）。
# 0.5pt 约等于 0.18mm，低于任何可见纸型差异，又能吸收浮点换算噪声。
TOLERANCE_PT = 0.5

# 纸型名义尺寸（mm），纵向。顺序即设置页展示顺序。
PAPERS_MM = (
    ("a4", "A4", 210.0, 297.0),
    ("a3", "A3", 297.0, 420.0),
    ("letter", "Letter", 215.9, 279.4),
    ("legal", "Legal", 215.9, 355.6),
)

ORIENTATIONS = ("portrait", "landscape")

# 四边页边距的边界（mm）。上界避免内容区退化到 0；下界 0 表示可贴边，
# 但真实后端能否画出贴边内容由实测决定，规则层只校验取值范围。
MARGIN_MIN_MM = 0.0
MARGIN_MAX_MM = 100.0

# 默认值：A4 纵向。四边默认值在 V-02 实测前**不冻结**，旧 UI 的 20mm
# 不作为导出默认（ui-spec §9 已登记）。探针必须显式给出边距，禁止隐式默认。
DEFAULT_PAPER = "a4"
DEFAULT_ORIENTATION = "portrait"

MM_PER_INCH = 25.4
PT_PER_INCH = 72.0


class ContractError(ValueError):
    """契约值非法；探针据此失败关闭，不生成产物。"""


def mm_to_pt(mm):
    """毫米转 point。"""
    return float(mm) * PT_PER_INCH / MM_PER_INCH


def pt_to_mm(pt):
    """point 转毫米。"""
    return float(pt) * MM_PER_INCH / PT_PER_INCH


def paper_size_pt(code, orientation):
    """返回 (宽, 高) pt；横向交换长短边。"""
    for key, _label, width_mm, height_mm in PAPERS_MM:
        if key == code:
            break
    else:
        raise ContractError("未知纸型: " + str(code))
    if orientation not in ORIENTATIONS:
        raise ContractError("未知方向: " + str(orientation))
    width, height = mm_to_pt(width_mm), mm_to_pt(height_mm)
    if orientation == "landscape":
        width, height = height, width
    return round(width, 2), round(height, 2)


def check_margins(margins_mm):
    """校验四边边距并返回 (top, right, bottom, left) mm 浮点元组。"""
    if not isinstance(margins_mm, dict):
        raise ContractError("边距必须是 {top,right,bottom,left} 对象")
    expected = {"top", "right", "bottom", "left"}
    if set(margins_mm) != expected:
        raise ContractError("边距必须恰好包含 top/right/bottom/left")
    values = []
    for side in ("top", "right", "bottom", "left"):
        value = margins_mm[side]
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise ContractError("边距必须是数值: " + side)
        if not math.isfinite(value):
            raise ContractError("边距必须是有限数值: " + side)
        if value < MARGIN_MIN_MM or value > MARGIN_MAX_MM:
            raise ContractError(
                "边距超出 [%.0f, %.0f] mm: %s=%s" % (MARGIN_MIN_MM, MARGIN_MAX_MM, side, value))
        values.append(float(value))
    return tuple(values)


def content_box_pt(paper, orientation, margins_mm):
    """返回内容区 (x, y, width, height) pt；y 自页面底部起算（PDF 坐标）。

    竖直方向：页面高 - 上边距 - 下边距；水平方向：页面宽 - 左 - 右。
    """
    width, height = paper_size_pt(paper, orientation)
    top, right, bottom, left = check_margins(margins_mm)
    box_width = width - mm_to_pt(left) - mm_to_pt(right)
    box_height = height - mm_to_pt(top) - mm_to_pt(bottom)
    if box_width <= 0 or box_height <= 0:
        raise ContractError("边距挤占后内容区非正: %.2f x %.2f pt" % (box_width, box_height))
    return (mm_to_pt(left), mm_to_pt(bottom), box_width, box_height)


def paginate(block_heights_pt, page_height_pt, first_top_pt, gap_pt=0.0):
    """按块高贪心分页，返回每页承载的块索引列表。

    `first_top_pt` 是首块顶部到页面顶部的距离（即上边距），用于校验剩余空间；
    调用方负责把块高换算成“占用的垂直空间”。一个块高于整页可用高度时拒绝，
    不静默截断——这与 EXPORT-01“不静默删内容”一致。
    """
    if page_height_pt <= 0:
        raise ContractError("页高必须为正")
    available = page_height_pt - first_top_pt
    if available <= 0:
        raise ContractError("上边距已占满整页")
    pages = []
    current = []
    used = 0.0
    for index, block in enumerate(block_heights_pt):
        if block <= 0:
            raise ContractError("块高必须为正: 索引 %d" % index)
        need = block if not current else block + gap_pt
        if need > available:
            raise ContractError("单块高于整页可用高度: 索引 %d" % index)
        if current and used + need > available:
            pages.append(current)
            current = [index]
            used = block
        else:
            current.append(index)
            used += need
    if current:
        pages.append(current)
    if not pages:
        pages = [[]]
    return pages


def expected_page_count(block_heights_pt, page_height_pt, first_top_pt, gap_pt=0.0):
    """分页结果页数；探针用它和真实产物的页数比较。"""
    return len(paginate(block_heights_pt, page_height_pt, first_top_pt, gap_pt))


# DOCX 样式组合：标题/正文三档，代码四档（EXPORT-03）。
HEADING_BODY_STYLES = ("document", "compact", "standard")
CODE_STYLES = ("theme", "light", "dark", "plain")

# HTML 资源策略（EXPORT-04）。
HTML_IMAGE_POLICIES = ("copy-to-dir", "keep-reference")

# 代码样式解析所依赖的应用外观；`theme`（随主题）没有固定颜色，
# 必须按导出时的外观解析成具体色值，否则“随主题”无从回读。
APPEARANCES = ("light", "dark")

# 代码面配色：浅色面与深色面成对定义，避免“亮字 + 亮底”这类不可读组合。
CODE_PALETTE = {
    "light": {"fill": "F3F4F6", "color": "1F2328"},
    "dark": {"fill": "1F2328", "color": "E6EDF3"},
}

# 代码块底色与文字色的最低对比度（WCAG 2.1 AA 正文阈值）。
# 低于该值的组合在阅读器里几乎不可读，按保真缺陷处理而不是样式偏好。
MIN_CODE_CONTRAST = 4.5

# 代码底色的落点。`run` 级（w:rPr/w:shd）只覆盖字形墨迹范围，行内空白与
# 行尾不着色，Word 里渲染成一条条断开的灰带；`paragraph` 级（w:pPr/w:shd）
# 才覆盖整个段落框。任务 1.2 的实际产物暴露了这个差别，因此冻结为契约。
DOCX_SHADING_SCOPE = "paragraph"

# ECMA-376 的 CT_PPr 是 xsd:sequence，子元素顺序有语义：顺序错了，Word 会
# 静默丢弃后面的元素。这里只列探针实际会写到的子元素，顺序取自
# CT_PPrBase 后接 CT_PPr 的扩展段（rPr/sectPr/pPrChange 在最后）。
DOCX_PPR_ORDER = (
    "pStyle", "keepNext", "keepLines", "pageBreakBefore", "framePr", "widowControl",
    "numPr", "suppressLineNumbers", "pBdr", "shd", "tabs", "suppressAutoHyphens",
    "kinsoku", "wordWrap", "overflowPunct", "topLinePunct", "autoSpaceDE",
    "autoSpaceDN", "bidi", "adjustRightInd", "snapToGrid", "spacing", "ind",
    "contextualSpacing", "mirrorIndents", "suppressOverlap", "jc", "textDirection",
    "textAlignment", "textboxTightWrap", "outlineLvl", "divId", "cnfStyle",
    "rPr", "sectPr", "pPrChange",
)


def _srgb_channel(value):
    """8 位 sRGB 通道归一化并做 gamma 展开。"""
    channel = value / 255.0
    return channel / 12.92 if channel <= 0.04045 else ((channel + 0.055) / 1.055) ** 2.4


def relative_luminance(hex_color):
    """十六进制颜色（RRGGBB）的相对亮度，用于对比度计算。"""
    text = str(hex_color).lstrip("#")
    if len(text) != 6:
        raise ContractError("颜色必须是 RRGGBB: " + str(hex_color))
    try:
        red, green, blue = (int(text[index:index + 2], 16) for index in (0, 2, 4))
    except ValueError as error:
        raise ContractError("颜色必须是十六进制: " + str(hex_color)) from error
    return (0.2126 * _srgb_channel(red) + 0.7152 * _srgb_channel(green)
            + 0.0722 * _srgb_channel(blue))


def contrast_ratio(background, foreground):
    """两色的 WCAG 对比度；结果在 1.0（同色）到 21.0（黑白）之间。"""
    first = relative_luminance(background)
    second = relative_luminance(foreground)
    lighter, darker = max(first, second), min(first, second)
    return (lighter + 0.05) / (darker + 0.05)


def check_heading_body_style(value):
    if value not in HEADING_BODY_STYLES:
        raise ContractError("未知标题/正文样式: " + str(value))
    return value


def check_code_style(value):
    if value not in CODE_STYLES:
        raise ContractError("未知代码样式: " + str(value))
    return value


def check_html_image_policy(value):
    if value not in HTML_IMAGE_POLICIES:
        raise ContractError("未知 HTML 图片策略: " + str(value))
    return value


def check_appearance(value):
    if value not in APPEARANCES:
        raise ContractError("未知外观: " + str(value))
    return value


def docx_shading_scope():
    """代码底色的落点；契约要求段落级（见 DOCX_SHADING_SCOPE 的说明）。"""
    return DOCX_SHADING_SCOPE


def docx_ppr_order():
    """CT_PPr 允许的子元素顺序（ECMA-376 wml.xsd 的 sequence）。"""
    return list(DOCX_PPR_ORDER)


def check_ppr_order(children):
    """核对一个 w:pPr 的子元素顺序；未知子元素直接拒绝。

    回读 DOCX 时逐个段落调用：顺序错在 Word 里表现为“元素消失”，
    只看 XML 有没有元素无法发现，因此顺序本身必须成为断言。
    """
    known = {name: index for index, name in enumerate(DOCX_PPR_ORDER)}
    indexes = []
    for child in children:
        if child not in known:
            raise ContractError("w:pPr 含未知子元素: " + str(child))
        indexes.append(known[child])
    if indexes != sorted(indexes):
        expected = sorted(children, key=lambda name: known[name])
        raise ContractError("w:pPr 子元素顺序不符: %s，应为 %s"
                            % (list(children), expected))
    return True


def docx_style_plan(heading_body, code_style, appearance="light"):
    """把选项映射成 DOCX 需要落地的样式事实，供探针逐项回读。

    - 标题层级：一级至三级标题 使用标题样式，正文使用 Normal；
    - 标题/正文档位影响字号与段距（document 随文档、compact 紧凑、standard 标准）；
    - 代码样式影响 Code 段的字体、底色与文字色（plain 为纯文本，无底色）。

    代码底色与文字色必须成对给出：深色样式若只把文字调亮而底色仍是浅色，
    产物在阅读器里几乎不可读——这不是样式偏好，而是保真缺陷，因此这里
    直接按 `MIN_CODE_CONTRAST` 拒绝不可读组合。`theme` 表示随主题，Word 里
    没有应用主题上下文，按导出时的 `appearance` 解析成具体浅/深配色。
    返回值只描述**应出现的事实**，不描述实现细节。
    """
    heading_body = check_heading_body_style(heading_body)
    code_style = check_code_style(code_style)
    appearance = check_appearance(appearance)
    body_size_half_pt = {"document": 22, "compact": 20, "standard": 24}[heading_body]
    heading_sizes = {"document": (32, 28, 24), "compact": (28, 24, 20),
                     "standard": (36, 32, 28)}[heading_body]
    if code_style == "plain":
        code = {"font": "Courier New", "shaded": False, "fill": None, "color": "auto"}
    else:
        palette = CODE_PALETTE[appearance if code_style == "theme" else code_style]
        ratio = contrast_ratio(palette["fill"], palette["color"])
        if ratio < MIN_CODE_CONTRAST:
            raise ContractError("代码配色对比度不足: %.2f < %.2f" % (ratio, MIN_CODE_CONTRAST))
        code = {"font": "Courier New", "shaded": True, "fill": palette["fill"],
                "color": palette["color"], "contrast": round(ratio, 2)}
    return {
        "heading_body": heading_body,
        "code_style": code_style,
        "appearance": appearance,
        "body_size_half_pt": body_size_half_pt,
        "heading_sizes_half_pt": heading_sizes,
        "code": code,
    }


# ------------------------------------------------------------ 语法 × 格式矩阵
#
# 任务 1.2 要求“语法 × 格式”矩阵有据。这里冻结的是**每种语法块在每种格式
# 里落到什么层级**，不是“都支持”这种无信息结论。层级词汇只有四个：
#
# - `native`    目标格式有对应结构（Word 的真标题样式、真表格、真脚注……）；
# - `flattened` 变成排版结果（PDF 的字形与线条，HTML 的标签），结构语义丢失；
# - `source-text` 只落源文本，不渲染（公式、Mermaid 本轮就是这条边界）；
# - `absent`    该格式本轮不承载，产物里**不得**出现该内容。
#
# `absent` 是可回读的负断言：探针要求对应文本在产物里找不到。写成“未测”
# 会让缺口随时间漂移成“大概支持”，负断言不会。

SYNTAX_FORMATS = ("pdf", "docx", "html")
SYNTAX_LEVELS = ("native", "flattened", "source-text", "absent")

SYNTAX_MATRIX = (
    {"id": "heading", "label": "标题 1～3 级",
     "markdown": ["# 中文标题 Heading 1", "", "## 二级标题 Section", "",
                  "### 三级标题 Sub", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "PDF 无文档结构，标题靠字号与间距区分；Word 与 HTML 是真标题层级。"},
    {"id": "paragraph", "label": "正文与行内标记",
     "markdown": ["正文段落 body text，含行内 `code`、**加粗**、*斜体*。", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "PDF 只能逐字形定位，加粗/斜体需另一套字形；本轮 PDF 只落常规字重。"},
    {"id": "link", "label": "链接",
     "markdown": ["含[链接](https://example.com)的段落。", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "PDF 用链接注释覆盖文字矩形，可点击但不是文档结构。"},
    {"id": "ordered-list", "label": "有序列表",
     "markdown": ["1. 第一项 first item", "2. 第二项 second item",
                  "3. 第三项 third item", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "Word 编号由 OOXML 编号定义生成，正文里不得再写序号。"},
    {"id": "unordered-list", "label": "无序列表",
     "markdown": ["- 无序项 bullet one", "- 无序项 bullet two", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "Word 用 bullet 编号定义，不用手写圆点字符。"},
    {"id": "blockquote", "label": "引用块",
     "markdown": ["> 引用块 quote text", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "Word 用段落左边框 + 缩进表达，HTML 用 blockquote。"},
    {"id": "table", "label": "表格",
     "markdown": ["| 列 A | 列 B |", "| --- | --- |", "| 甲 | 乙 |", "| 丙 | 丁 |", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "PDF 画网格线；Word 是真表格且单元格可编辑。"},
    {"id": "code", "label": "围栏代码块",
     "markdown": ["```rust", "fn main() {", "    println!(\"中文 code\");", "}", "```", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "PDF 用底色矩形 + 等宽文本；Word 用 Code 段落样式。"},
    {"id": "image", "label": "图片",
     "markdown": ["![示例图片](sample.png)", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "三格式都内嵌位图；PDF 的替代文字不进入文档结构。"},
    {"id": "rule", "label": "分隔线",
     "markdown": ["---", ""],
     "levels": {"pdf": "flattened", "docx": "native", "html": "native"},
     "note": "Word 用空段落的下边框表达，不是表格线。"},
    {"id": "inline-math", "label": "行内公式",
     "markdown": ["行内公式 $E = mc^2$ 与后文。", ""],
     "levels": {"pdf": "source-text", "docx": "source-text", "html": "source-text"},
     "note": "本轮三格式都只落 LaTeX 源文本，未做数学排版——这是明确的可编辑性边界。"},
    {"id": "display-math", "label": "块级公式",
     "markdown": ["$$", "a^2 + b^2 = c^2", "$$", ""],
     "levels": {"pdf": "source-text", "docx": "source-text", "html": "source-text"},
     "note": "同上；块级公式在 PDF/HTML 里保留独占段落与源文本。"},
    {"id": "mermaid", "label": "Mermaid 图表",
     "markdown": ["```mermaid", "graph TD", "  A[开始] --> B[结束]", "```", ""],
     "levels": {"pdf": "source-text", "docx": "source-text", "html": "source-text"},
     "note": "本轮不渲染图表，只落源代码；静态图像边界见 V-03。"},
    {"id": "footnote", "label": "脚注",
     "markdown": ["末尾段落 end paragraph[^note]", "", "[^note]: 脚注内容 footnote body", ""],
     "levels": {"pdf": "source-text", "docx": "native", "html": "source-text"},
     "note": "Word 是真脚注；PDF 与 HTML 只保留正文里的 [^note] 源标记，脚注正文不出现——这是本轮明确的边界。"},
)

# 回读用的标记文本：必须出现在产物里的字符串（按块）。
SYNTAX_MARKERS = {
    "heading": ["中文标题 Heading 1", "二级标题 Section", "三级标题 Sub"],
    "paragraph": ["正文段落 body text"],
    "link": ["链接"],
    "ordered-list": ["第一项 first item", "第二项 second item", "第三项 third item"],
    "unordered-list": ["无序项 bullet one", "无序项 bullet two"],
    "blockquote": ["引用块 quote text"],
    "table": ["列 A", "列 B", "甲", "乙"],
    "code": ["fn main() {", "中文 code"],
    "image": [],
    "rule": [],
    "inline-math": ["E = mc^2"],
    "display-math": ["a^2 + b^2 = c^2"],
    "mermaid": ["graph TD", "A[开始] --> B[结束]"],
    "footnote": ["末尾段落 end paragraph"],
}

# 回读用的负标记：这些文本**不得**出现在对应格式的产物里。
#
# 负断言和层级是两件事：脚注块整体是 `source-text`（正文里的 [^note] 会落纸），
# 但脚注正文本身不该出现。写成“未测”会让缺口随时间漂移成“大概支持”，
# 负断言不会——产物里出现即失败。
SYNTAX_FORBIDDEN = {
    "footnote": {"pdf": ["脚注内容 footnote body"],
                 "html": ["脚注内容 footnote body"]},
}

# PDF 版式常量：A4 纵向、四边 20mm。语法页只用于证明各原语可绘制，
# 不承担“生产默认边距”的含义（默认值仍待 4.4 接线时按实测确定）。
SYNTAX_PDF_PAPER = "a4"
SYNTAX_PDF_ORIENTATION = "portrait"
SYNTAX_PDF_MARGIN_MM = 20.0
SYNTAX_HEADING_SIZES_PT = {1: 22.0, 2: 17.0, 3: 14.0}
SYNTAX_BODY_SIZE_PT = 10.0
SYNTAX_CODE_SIZE_PT = 9.0
SYNTAX_LINE_GAP_PT = 4.0
SYNTAX_LEADING = 1.45  # 行距 = 字号 × 该系数


def check_syntax_matrix():
    """校验矩阵完整性：块 ID 唯一、格式齐全、层级合法、说明非空。"""
    seen = set()
    for row in SYNTAX_MATRIX:
        block_id = row["id"]
        if block_id in seen:
            raise ContractError("语法块 ID 重复: " + block_id)
        seen.add(block_id)
        if set(row["levels"]) != set(SYNTAX_FORMATS):
            raise ContractError("语法块缺少格式层级: " + block_id)
        for fmt in SYNTAX_FORMATS:
            if row["levels"][fmt] not in SYNTAX_LEVELS:
                raise ContractError("非法层级 %s: %s/%s" % (row["levels"][fmt], block_id, fmt))
        if not str(row.get("note", "")).strip():
            raise ContractError("语法块缺少可编辑性说明: " + block_id)
        if block_id not in SYNTAX_MARKERS:
            raise ContractError("语法块缺少回读标记: " + block_id)
        # 负标记必须指向真实存在的语法块，且只对“不承载该内容”的格式生效。
        for fmt, texts in SYNTAX_FORBIDDEN.get(block_id, {}).items():
            if fmt not in SYNTAX_FORMATS:
                raise ContractError("负标记格式非法: %s/%s" % (block_id, fmt))
            if not texts:
                raise ContractError("负标记为空: %s/%s" % (block_id, fmt))
    return SYNTAX_FORMATS


def syntax_level(block_id, fmt):
    """查矩阵：某语法块在某格式里的落地层级。"""
    if fmt not in SYNTAX_FORMATS:
        raise ContractError("未知格式: " + str(fmt))
    for row in SYNTAX_MATRIX:
        if row["id"] == block_id:
            return row["levels"][fmt]
    raise ContractError("未知语法块: " + str(block_id))


def render_markdown():
    """由矩阵生成 `scripts/fixtures/m4-export/sample.md`。

    样本只有这一个来源：磁盘上的 sample.md 必须与它逐字节相同（自测断言），
    否则“矩阵描述的语法”和“探针实际消费的样本”会悄悄漂移。
    """
    lines = ["<!-- 由 scripts/m4_export_rules.py 的 SYNTAX_MATRIX 生成，勿手改。 -->", ""]
    for row in SYNTAX_MATRIX:
        lines.append("<!-- %s: %s -->" % (row["id"], row["label"]))
        lines.extend(row["markdown"])
    return "\n".join(lines).rstrip("\n") + "\n"


# ------------------------------------------------- PDF 语法页版式（纯计算）


def _content_box():
    """语法页内容区 (x, y_top, width, height)，y 自页面顶部起算（krilla 坐标）。"""
    width, height = paper_size_pt(SYNTAX_PDF_PAPER, SYNTAX_PDF_ORIENTATION)
    margin = mm_to_pt(SYNTAX_PDF_MARGIN_MM)
    return margin, margin, width - 2 * margin, height - 2 * margin


def syntax_pdf_document():
    """把语法块排成 PDF 绘制指令；坐标已是 krilla 的左上原点、y 向下。

    布局在这里算完，Rust 探针只执行坐标——这样“排到哪”是可断言的契约事实，
    而不是靠肉眼从产物里猜。分页由内容高度决定，图片块独占一页，使其墨迹
    包围盒恰好等于图片矩形，回读时可直接比对几何。
    """
    x0, top, width, height = _content_box()
    bottom = top + height
    pages = []
    ops = []
    cursor = top

    def new_page():
        nonlocal ops, cursor
        if ops:
            pages.append({"ops": ops})
        ops = []
        cursor = top

    def need(amount):
        nonlocal cursor
        if cursor + amount > bottom:
            new_page()
        y = cursor
        cursor += amount
        return y

    def text_op(block, text, size, indent=0.0, x=None):
        leading = size * SYNTAX_LEADING
        y = need(leading)
        ops.append({"op": "text", "block": block, "text": text, "size": size,
                    "x": (x0 + indent) if x is None else x,
                    "y": y + size, "leading": leading})
        return y

    for row in SYNTAX_MATRIX:
        block = row["id"]
        if block == "heading":
            for level in (1, 2, 3):
                text_op(block, SYNTAX_MARKERS[block][level - 1],
                        SYNTAX_HEADING_SIZES_PT[level])
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "paragraph":
            text_op(block, SYNTAX_MARKERS[block][0], SYNTAX_BODY_SIZE_PT)
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "link":
            label = "含链接的段落。"
            y = text_op(block, label, SYNTAX_BODY_SIZE_PT)
            size = SYNTAX_BODY_SIZE_PT
            link_width = size * len(label) * 0.6
            ops.append({"op": "link", "block": block, "uri": "https://example.com",
                        "x": x0, "y": y, "w": link_width, "h": size * SYNTAX_LEADING})
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "ordered-list":
            for index, text in enumerate(SYNTAX_MARKERS[block]):
                text_op(block, "%d. %s" % (index + 1, text), SYNTAX_BODY_SIZE_PT, indent=8.0)
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "unordered-list":
            for text in SYNTAX_MARKERS[block]:
                text_op(block, "• " + text, SYNTAX_BODY_SIZE_PT, indent=8.0)
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "blockquote":
            y = text_op(block, SYNTAX_MARKERS[block][0], SYNTAX_BODY_SIZE_PT, indent=10.0)
            ops.append({"op": "line", "block": block, "x1": x0 + 4.0, "y1": y,
                        "x2": x0 + 4.0, "y2": y + SYNTAX_BODY_SIZE_PT * SYNTAX_LEADING,
                        "width": 1.5, "color": "8A8A8A"})
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "table":
            rows = [SYNTAX_MARKERS[block][:2], SYNTAX_MARKERS[block][2:]]
            row_height = 18.0
            table_height = row_height * len(rows)
            table_top = need(table_height)
            column_width = width / 2.0
            for row_index, cells in enumerate(rows):
                for column_index, text in enumerate(cells):
                    ops.append({"op": "text", "block": block, "text": text,
                                "size": SYNTAX_BODY_SIZE_PT,
                                "x": x0 + column_width * column_index + 3.0,
                                "y": table_top + row_height * row_index + 13.0,
                                "leading": row_height})
            for line_index in range(len(rows) + 1):
                y = table_top + row_height * line_index
                ops.append({"op": "line", "block": block, "x1": x0, "y1": y,
                            "x2": x0 + width, "y2": y, "width": 0.5, "color": "444444"})
            for line_index in range(3):
                x = x0 + column_width * line_index
                ops.append({"op": "line", "block": block, "x1": x, "y1": table_top,
                            "x2": x, "y2": table_top + table_height,
                            "width": 0.5, "color": "444444"})
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "code":
            lines = ["fn main() {", "    println!(\"中文 code\");", "}"]
            leading = SYNTAX_CODE_SIZE_PT * SYNTAX_LEADING
            block_height = leading * len(lines) + 8.0
            y = need(block_height)
            ops.append({"op": "rect", "block": block, "x": x0, "y": y,
                        "w": width, "h": block_height, "fill": "F3F4F6"})
            for index, text in enumerate(lines):
                ops.append({"op": "text", "block": block, "text": text,
                            "size": SYNTAX_CODE_SIZE_PT, "x": x0 + 4.0,
                            "y": y + 4.0 + leading * index + SYNTAX_CODE_SIZE_PT,
                            "leading": leading})
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "image":
            # 独占一页：墨迹包围盒即图片矩形，回读可直接比对。
            new_page()
            image_height = min(height - 20.0, 120.0)
            ops.append({"op": "image", "block": block, "x": x0, "y": top,
                        "w": image_height * 2.0, "h": image_height})
            new_page()
        elif block == "rule":
            y = need(12.0)
            ops.append({"op": "line", "block": block, "x1": x0, "y1": y + 6.0,
                        "x2": x0 + width, "y2": y + 6.0, "width": 0.8, "color": "888888"})
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "inline-math":
            text_op(block, "行内公式 E = mc^2 与后文。", SYNTAX_BODY_SIZE_PT)
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "display-math":
            text_op(block, "$$", SYNTAX_BODY_SIZE_PT, indent=width / 2.0 - 20.0)
            text_op(block, SYNTAX_MARKERS[block][0], SYNTAX_BODY_SIZE_PT, indent=width / 2.0 - 60.0)
            text_op(block, "$$", SYNTAX_BODY_SIZE_PT, indent=width / 2.0 - 20.0)
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "mermaid":
            lines = ["```mermaid"] + SYNTAX_MARKERS[block] + ["```"]
            leading = SYNTAX_CODE_SIZE_PT * SYNTAX_LEADING
            block_height = leading * len(lines) + 8.0
            y = need(block_height)
            ops.append({"op": "rect", "block": block, "x": x0, "y": y,
                        "w": width, "h": block_height, "fill": "F3F4F6"})
            for index, text in enumerate(lines):
                ops.append({"op": "text", "block": block, "text": text,
                            "size": SYNTAX_CODE_SIZE_PT, "x": x0 + 4.0,
                            "y": y + 4.0 + leading * index + SYNTAX_CODE_SIZE_PT,
                            "leading": leading})
            cursor += SYNTAX_LINE_GAP_PT
        elif block == "footnote":
            # 正文段照常落；脚注正文按 absent 层级**不落**。
            text_op(block, SYNTAX_MARKERS[block][0], SYNTAX_BODY_SIZE_PT)
            cursor += SYNTAX_LINE_GAP_PT
        else:
            raise ContractError("语法块缺少 PDF 版式: " + block)
    if ops:
        pages.append({"ops": ops})
    return {"paper": SYNTAX_PDF_PAPER, "orientation": SYNTAX_PDF_ORIENTATION,
            "margin_mm": SYNTAX_PDF_MARGIN_MM, "pages": pages}


def syntax_pdf_probe_input():
    """语法页布局的探针输入：坐标已换算成 krilla 的左上原点、y 向下。"""
    document = syntax_pdf_document()
    width, height = paper_size_pt(SYNTAX_PDF_PAPER, SYNTAX_PDF_ORIENTATION)
    return {"page_width_pt": width, "page_height_pt": height,
            "pages": document["pages"]}


def syntax_pdf_expected():
    """语法页的期望事实：页数、每块标记与负标记。"""
    document = syntax_pdf_document()
    return {
        "pages": len(document["pages"]),
        "must_contain": [text for row in SYNTAX_MATRIX
                         if row["levels"]["pdf"] != "absent"
                         for text in SYNTAX_MARKERS[row["id"]]],
        "must_not_contain": SYNTAX_FORBIDDEN.get("footnote", {}).get("pdf", []),
        "content_box_pt": _content_box(),
    }


def syntax_docx_expectations():
    """DOCX 语法块的期望事实：原生结构计数、文本在场与文档顺序。"""
    return {
        "styles": ["Normal", "Heading1", "Heading2", "Heading3", "Code", "Quote", "Rule"],
        "numbered_paragraphs": 5,
        "bulleted_paragraphs": 2,
        "tables": 1,
        "table_rows": 2,
        "table_grid_cols": 2,
        "drawings": 1,
        "footnotes": 1,
        "hyperlink_relationships": 1,
        "quote_borders": 1,
        "rule_borders": 1,
        # 在场检查：任意顺序都算，只要求出现。
        "must_contain": [text for row in SYNTAX_MATRIX
                         if row["levels"]["docx"] != "absent"
                         for text in SYNTAX_MARKERS[row["id"]]],
        # 顺序检查：真实文档里标题与正文交错，顺序必须与探针实际写入一致，
        # 不能按矩阵的块顺序排——否则检查的是“矩阵分组”而非“文档顺序”。
        "order": ["中文标题 Heading 1", "正文段落 body text", "链接", "二级标题 Section",
                  "三级标题 Sub", "第一项 first item", "第二项 second item", "第三项 third item",
                  "无序项 bullet one", "无序项 bullet two", "引用块 quote text",
                  "列 A", "列 B", "甲", "乙", "fn main() {", "中文 code",
                  "行内公式 E = mc^2", "a^2 + b^2 = c^2", "graph TD", "A[开始] --> B[结束]",
                  "末尾段落 end paragraph"],
        "must_contain_in_footnotes": ["脚注内容 footnote body"],
    }


def syntax_html_expectations():
    """HTML 语法块的期望事实：标签、文本与负标记。"""
    return {
        "tags": ["h1", "h2", "h3", "p", "ol", "ul", "blockquote", "table",
                 "pre", "img", "hr", "a"],
        "must_contain": [text for row in SYNTAX_MATRIX
                         if row["levels"]["html"] != "absent"
                         for text in SYNTAX_MARKERS[row["id"]]],
        "must_not_contain": SYNTAX_FORBIDDEN.get("footnote", {}).get("html", []),
    }
