#!/usr/bin/env python3
"""V-03 支持矩阵的纯规则层：图片格式、SVG 安全边界、剪贴板路由、协议、
公式语法、Mermaid 图类型与静态主题字段。

本模块只含常量与纯函数，不读磁盘、不起进程、不编译 Rust，因此可在没有
锁定 GPUI 缓存、没有系统的环境里独立自测。真实探针脚本
`scripts/probe_m4_v03.py` 用它把“契约值”和“实测结果”分开：契约在这里
冻结，实测只负责证明锁定 GPUI / 候选后端能否按契约表现。

三层分离的理由：V-03 明确要求「不用'可打开文件'掩盖未支持的显示能力」。
把“期望”写成常量后，实测值才可能与之不符；若期望也由实测反推，探针永远
只能自证。每个矩阵行的 `expectation` 都写明本轮应看到的事实，探针逐项比对。

**本模块不是准入结论。** Mermaid 与公式后端的依赖闭包（mermaid-rs-renderer
依赖仓库禁库 `regex` 并带入 resvg/usvg 0.47；latex-rust 带入 RUSTSEC-2026-0192
的 unmaintained `ttf-parser`）由任务 1.5 裁决；这里只冻结“支持到什么程度”，
不冻结“用哪个 crate”。支持清单以本模块 + 实测证据为准，失败样例必须保留源码。
"""

import re

# ---------------------------------------------------------------- 图片格式


class ContractError(ValueError):
    """契约值非法；探针据此失败关闭，不生成证据。"""


# 图片格式矩阵。`formats` 是 PRD IMAGE-01 列出的、V1 必须支持解码的格式；
# `extensions` 是大小写不敏感的全部可接受扩展名（`.jpg` 与 `.jpeg` 同格式）。
# `decoder` 记录锁定 GPUI 走哪条解码路径，`animated` 记录该格式在**文档路径**
# 上是否真的逐帧播放——两者分开，因为“能解码”不等于“会动”。
IMAGE_FORMATS = (
    {
        "format": "png",
        "extensions": ("png",),
        "decoder": "static",
        "animated": False,
        "note": "APNG 虽为 PNG 的超集，锁定 GPUI 只取首帧（实测 frames=1）。",
    },
    {
        "format": "jpeg",
        "extensions": ("jpg", "jpeg"),
        "decoder": "static",
        "animated": False,
        "note": "有损编码，样本容差 ±2/通道。",
    },
    {
        "format": "gif",
        "extensions": ("gif",),
        "decoder": "frames",
        "animated": True,
        "note": "逐帧解码并转 BGRA；坏帧跳过，全坏才报错。",
    },
    {
        "format": "webp",
        "extensions": ("webp",),
        "decoder": "frames-if-animated",
        "animated": True,
        "note": "文档路径按 has_animation() 逐帧；剪贴板路径恒为静态（见 CLIPBOARD）。",
    },
    {
        "format": "bmp",
        "extensions": ("bmp",),
        "decoder": "static",
        "animated": False,
        "note": "静态位图。",
    },
    {
        "format": "svg",
        "extensions": ("svg",),
        "decoder": "usvg",
        "animated": False,
        "note": "经 usvg 解析后光栅化，按 SMOOTH_SVG_SCALE_FACTOR=2 放大；安全边界见 SVG_POLICY。",
    },
)

# Markdown 文档扩展名；大小写不敏感，`.MD` 与 `.md` 等价。
MARKDOWN_EXTENSIONS = ("md",)

# SVG 在锁定 GPUI 里的固定放大倍数（vendor/gpui-pre-0.3.9/src/svg_renderer.rs）。
SMOOTH_SVG_SCALE_FACTOR = 2.0

# 文档路径的像素格式是 BGRA（gpui 在解码后对每个像素 swap(0, 2)）。
DOCUMENT_PIXEL_ORDER = "bgra"


def image_format_for_extension(extension):
    """按扩展名返回格式条目；大小写不敏感，未知扩展名返回 None。"""
    needle = extension.lower().lstrip(".")
    for entry in IMAGE_FORMATS:
        if needle in entry["extensions"]:
            return entry
    return None


def accepted_extensions():
    """全部可接受扩展名（已排序），用于冻结清单与界面提示。"""
    return sorted(ext for entry in IMAGE_FORMATS for ext in entry["extensions"])


# ---------------------------------------------------------------- SVG 安全


# SVG 的 href 解析契约。usvg 0.46 的默认 string resolver **会读取本地文件**
# （`opts.get_abs_path` → `path.exists()` → `std::fs::read`），相对路径以进程
# cwd 为基准；这意味着恶意 SVG 能读到进程可读的任意文件。生产必须替换成
# 只放行 data: 的 resolver。`data:` 仍放行：它是内联字节，不触达文件系统。
SVG_POLICY = {
    # 字符串 href（相对路径、绝对路径、`../` 穿越、`file:` URI）一律拒绝。
    "string_href": "deny",
    # data: URI 放行（内联字节，默认 data resolver 支持 png/jpeg/gif/webp/svg）。
    "data_href": "allow",
    # 远程 http(s) href 不抓取（usvg 本身不发网络请求；显式拒绝以免依赖该实现细节）。
    "remote_href": "deny",
    # 脚本、foreignObject、外部实体：不执行、不展开，按“无效果”处理。
    "script": "not-executed",
    "foreign_object": "ignored",
    "entity_expansion": "not-expanded",
}

# 期望“无效果”的 SVG 输入类别。探针必须证明它们既没渲染出额外内容，也没报错崩溃。
SVG_NEUTRAL_CASES = (
    ("script", "内联 <script>alert(1)</script>"),
    ("foreign-object", "内联 <foreignObject> 含 HTML"),
    ("http-href", "<image href=\"https://…\"> 远程图片"),
    ("entity", "自定义实体引用"),
)


def svg_href_verdict(href):
    """按 SVG_POLICY 判定一个 `<image href>` 是否放行。"""
    if href.startswith("data:"):
        return SVG_POLICY["data_href"]
    lowered = href.lower()
    if lowered.startswith("http://") or lowered.startswith("https://"):
        return SVG_POLICY["remote_href"]
    return SVG_POLICY["string_href"]


# ---------------------------------------------------------------- 剪贴板


# 剪贴板读取优先级（gpui-pre-macos pasteboard.rs 的实现顺序，冻结为契约）。
# 关键事实：**没有 public.html / public.rtf 分支**，所以“剪贴板里有 HTML”
# 不会引入富文本导入——与验证计划「不因为剪贴板有 HTML 就引入新的富文本导入
# 范围」一致，但原因不是我们主动过滤，而是锁定实现根本没有读它。
CLIPBOARD_READ_ORDER = (
    ("files", "NSFilenamesPboardType 文件列表，附带纯文本路径"),
    ("text", "public.utf8-plain-text 纯文本"),
    ("image", "按 ImageFormat 顺序：png→jpeg→webp→gif→svg→bmp→tiff→ico→pnm"),
)

# 写入契约：单项纯文本/图片各自直写；多项混合只拼接字符串项，图片被丢弃。
# 每项都带 `expect`：写完后读回应看到什么。`single-image` 的格式必须是写出的
# 那个格式（GPUI 不嗅探魔数，只按声明的 UTI 解码）。
CLIPBOARD_WRITE_CASES = (
    {"case": "single-text", "expect_entries": ["string"], "expect_text": "写入的纯文本",
     "note": "单条纯文本直写"},
    {"case": "single-image", "expect_entries": ["image"], "expect_format": "Png",
     "note": "单张图片直写并原格式读回"},
    {"case": "multi-entry", "expect_entries": ["string"], "expect_text": "文本先",
     "note": "文本+图片混合：只写文本，图片被丢弃"},
)

CLIPBOARD_WRITE_RULES = {
    "single-text": "写纯文本",
    "single-image": "写图片",
    "multi-entry": "只拼接 String 项，Image/ExternalPaths 被丢弃",
}

# 剪贴板图片解码路径与文档路径不同：`Image::to_image_data` 对 WebP 恒走静态
# 解码，不做 has_animation 判断。动画 WebP 粘进来自此只有一帧。
CLIPBOARD_WEBP_ANIMATED_FRAMES = 1


def clipboard_route(flavors):
    """给定剪贴板声明的 flavor 集合，按读取优先级返回命中的路由。"""
    if "files" in flavors:
        return "files"
    if "text" in flavors:
        return "text"
    if "image" in flavors:
        return "image"
    return "none"


def clipboard_route_from_entries(entries):
    """由读回的条目反推路由：探针只报告条目，路由名由本层统一推导。

    这样契约里写的是语义路由（files/text/image/none），而实测侧不需要重复
    实现一遍优先级判断——否则两边各写一套，永远无法互相证伪。
    """
    if not entries:
        return "none"
    kind = entries[0].get("kind")
    return {"paths": "files", "string": "text", "image": "image"}.get(kind, "none")


# 剪贴板 flavor 组合矩阵的**契约**：每个组合应命中哪条路由、命中项是什么。
# `kind` 是第一个条目的类型（string/paths/image）；`format` 仅图片项使用。
# 这张表是 V-03「协议与剪贴板」一节的判据——探针写 flavor、读回、逐行比对。
# 关键断言有两条：`html-only`/`rtf-only` 必须**无路由**（锁定实现不读这两种
# flavor，所以“剪贴板有 HTML”不会变成富文本导入）；`gif-webp` 必须读回 Webp
# （即使 GIF 先声明），证明选择依据是 ImageFormat 枚举顺序而非声明顺序。
CLIPBOARD_FLAVOR_CASES = (
    {"flavor": "text", "route": "text", "kind": "string",
     "note": "仅纯文本"},
    {"flavor": "html-text", "route": "text", "kind": "string",
     "note": "HTML + 纯文本：取纯文本，HTML 分支不存在"},
    {"flavor": "html-only", "route": "none", "kind": None,
     "note": "仅 HTML：无任何路由（不引入富文本导入）"},
    {"flavor": "rtf-text", "route": "text", "kind": "string",
     "note": "RTF + 纯文本：取纯文本"},
    {"flavor": "file-text", "route": "files", "kind": "paths",
     "note": "public.file-url 被系统合成为 NSFilenamesPboardType，命中文件分支，"
             "并附带纯文本路径"},
    {"flavor": "files", "route": "files", "kind": "paths",
     "note": "NSFilenamesPboardType 命中文件分支"},
    {"flavor": "png", "route": "image", "kind": "image", "format": "Png",
     "note": "仅 PNG：图片分支"},
    {"flavor": "text-png", "route": "text", "kind": "string",
     "note": "文本先于图片"},
    {"flavor": "gif-webp", "route": "image", "kind": "image", "format": "Webp",
     "note": "按 ImageFormat 顺序取 Webp，而非声明顺序的 Gif"},
)


# ---------------------------------------------------------------- 链接协议


# 与 crates/aline-markdown/src/tree/html.rs 的 is_allowed_link 逐条对齐。
# 这份常量是**镜像**：真正的生产判定在 Rust 侧，探针通过真实解析验证两者一致。
ALLOWED_LINK_SCHEMES = ("http", "https", "mailto")
ALLOWED_LINK_PREFIXES = ("#", "/")

# 明确拒绝的协议（点击后应提示“不支持的协议”，不改文档）。
REJECTED_LINK_SCHEMES = (
    "javascript", "data", "vbscript", "file", "ftp", "tel", "ssh", "smb",
)


def link_allowed(href):
    """镜像 is_allowed_link：空、拒绝协议、无 scheme 的相对路径都按规则判定。"""
    trimmed = href.strip()
    if not trimmed:
        return False
    if trimmed.startswith(ALLOWED_LINK_PREFIXES):
        return True
    lowered = trimmed.lower()
    return any(lowered.startswith(scheme + ":") for scheme in ALLOWED_LINK_SCHEMES)


def link_verdict(href):
    """返回 (allowed, reason)；reason 用于界面提示的冻结文案键。"""
    trimmed = href.strip()
    if not trimmed:
        return False, "link.rejected.empty"
    if link_allowed(trimmed):
        return True, "link.allowed"
    lowered = trimmed.lower()
    for scheme in REJECTED_LINK_SCHEMES:
        if lowered.startswith(scheme + ":"):
            return False, "link.rejected.scheme"
    if ":" in lowered.split("/")[0]:
        return False, "link.rejected.scheme"
    # 无 scheme 的裸路径（如 `notes/a.md`）不满足 allowlist，按拒绝处理。
    return False, "link.rejected.unsupported"


# ---------------------------------------------------------------- 公式语法


# 公式语法矩阵：每条是“应能渲染”的 TeX 片段。探针逐个交给后端，成功即算
# 支持；失败必须给出可读诊断且源码保留。`text` 一条单列，因为它暴露了
# 字体覆盖的边界——STIX Two Math 无 CJK 字形。
MATH_SYNTAX_CASES = (
    ("fraction", r"\frac{1}{2}", True),
    ("greek", r"\alpha + \beta = \gamma", True),
    ("radical-integral", r"\int_0^1 \sqrt{x^2+1}\,dx", True),
    ("matrix", r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", True),
    ("cases", r"f(x)=\begin{cases} 1 & x>0 \\ 0 & x\le 0 \end{cases}", True),
    ("summation", r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}", True),
    ("color", r"\textcolor{red}{x} + \color{blue}{y}", True),
    # CJK 在数学文本里：后端若字体无中文字形会失败；这是**已知边界**，不是缺陷。
    ("cjk-text", r"\text{速度} = \frac{d}{t}", False),
)

# 错误输入：必须失败，且失败是“带诊断的错误”而非“渲染出假内容”。
MATH_ERROR_CASES = (
    ("unbalanced-brace", r"\frac{1}{"),
    ("unknown-command", r"\notacommand{x}"),
)

# 无 CJK 字形时的诊断关键字（后端各自的措辞，探针只要求提到缺字形）。
MATH_CJK_DIAGNOSTIC_HINT = "glyph"


# ---------------------------------------------------------------- Mermaid


# 首批图类型：锁定候选后端声明支持，探针逐个用最小合法样本渲染。
# 顺序按 PRD/原型出现频率排，便于阅读矩阵。
MERMAID_DIAGRAM_TYPES = (
    "flowchart",
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "pie",
    "gantt",
    "journey",
    "timeline",
    "mindmap",
    "gitGraph",
    "xychart-beta",
    "quadrantChart",
    "sankey-beta",
    "kanban",
    "C4Context",
    "block-beta",
    "architecture-beta",
    "requirementDiagram",
    "zenuml",
    "packet-beta",
    "radar-beta",
    "treemap",
)

# 主题跟随应用明暗时的映射契约。护眼（eye-care）映射到亮色：护眼底色本身
# 是浅色，若映射到暗色图表会出现“亮底暗图”的割裂。
MERMAID_APPEARANCE_MAP = {
    "light": "light",
    "dark": "dark",
    "eye-care": "light",
}

# 静态图表主题的冻结格式：字段集合封闭、值类型受限、不允许脚本或外部引用。
# 这是**导入前的校验契约**；锁定候选的 Theme 反序列化**不**封闭（接受未知
# 字段、不做类型外的语义校验），因此生产必须在反序列化前/后自行校验。
MERMAID_THEME_FIELDS = (
    "font_family", "font_size", "primary_color", "primary_text_color",
    "primary_border_color", "line_color", "secondary_color", "tertiary_color",
    "edge_label_background", "cluster_background", "cluster_border", "background",
    "sequence_actor_fill", "sequence_actor_border", "sequence_actor_line",
    "sequence_note_fill", "sequence_note_border", "sequence_activation_fill",
    "sequence_activation_border", "text_color", "git_colors", "git_inv_colors",
    "git_branch_label_colors", "git_commit_label_color", "git_commit_label_background",
    "git_tag_label_color", "git_tag_label_background", "git_tag_label_border",
    "pie_colors", "pie_title_text_size", "pie_title_text_color",
    "pie_section_text_size", "pie_section_text_color", "pie_legend_text_size",
    "pie_legend_text_color", "pie_stroke_color", "pie_stroke_width",
    "pie_outer_stroke_width", "pie_outer_stroke_color", "pie_opacity",
)

# 导入主题里**禁止出现**的内容：脚本、外部资源、危险结构。命中即拒绝导入。
MERMAID_THEME_FORBIDDEN = (
    ("<script", "脚本标签"),
    ("javascript:", "javascript 协议"),
    ("</", "闭合标签（颜色字段应为纯色值）"),
    ("url(", "外部资源引用"),
    ("@import", "CSS 导入"),
    ("expression(", "CSS 表达式"),
)

# 颜色字段允许的形态：十六进制、命名色、rgb/rgba/hsl/hsla 函数。不含 url()。
_COLOR_RE = re.compile(
    r"^(#[0-9a-fA-F]{3,8}|[a-zA-Z]+|rgba?\([^)]*\)|hsla?\([^)]*\))$"
)
_NUMBER_FIELDS = {
    "font_size", "pie_title_text_size", "pie_section_text_size",
    "pie_legend_text_size", "pie_stroke_width", "pie_outer_stroke_width",
    "pie_opacity",
}
_STRING_FIELDS = {"font_family"}
_ARRAY_FIELDS = {
    "git_colors": 8, "git_inv_colors": 8, "git_branch_label_colors": 8,
    "pie_colors": 12,
}


def mermaid_theme_violations(theme):
    """校验一个候选静态主题（已解析成 dict）；返回违规列表，空表示可导入。

    只做本层能做的静态检查：字段封闭、值类型、颜色形态、禁止内容。
    语义层面的“这个颜色在深色背景下是否可读”不在本层，属 1.4 的对比度预算。
    """
    violations = []
    if not isinstance(theme, dict):
        return ["主题必须是 JSON 对象"]
    unknown = sorted(set(theme) - set(MERMAID_THEME_FIELDS))
    if unknown:
        violations.append("未知字段: " + ", ".join(unknown))
    missing = sorted(set(MERMAID_THEME_FIELDS) - set(theme))
    if missing:
        violations.append("缺少字段: " + ", ".join(missing))
    for field in MERMAID_THEME_FIELDS:
        if field not in theme:
            continue
        value = theme[field]
        if field in _NUMBER_FIELDS:
            if not isinstance(value, (int, float)) or isinstance(value, bool):
                violations.append("%s 必须是数值" % field)
            continue
        if field in _STRING_FIELDS:
            if not isinstance(value, str):
                violations.append("%s 必须是字符串" % field)
            else:
                violations.extend(_forbidden_hits(field, value))
            continue
        if field in _ARRAY_FIELDS:
            expected = _ARRAY_FIELDS[field]
            if not isinstance(value, list) or len(value) != expected:
                violations.append("%s 必须是长度 %d 的数组" % (field, expected))
                continue
            for index, item in enumerate(value):
                if not isinstance(item, str):
                    violations.append("%s[%d] 必须是字符串" % (field, index))
                elif not _COLOR_RE.match(item.strip()):
                    violations.append("%s[%d] 不是合法颜色: %r" % (field, index, item))
                else:
                    violations.extend(_forbidden_hits("%s[%d]" % (field, index), item))
            continue
        # 其余均为颜色字段。
        if not isinstance(value, str):
            violations.append("%s 必须是字符串" % field)
            continue
        if not _COLOR_RE.match(value.strip()):
            violations.append("%s 不是合法颜色: %r" % (field, value))
            continue
        violations.extend(_forbidden_hits(field, value))
    return violations


def _forbidden_hits(field, value):
    lowered = value.lower()
    return ["%s 含%s" % (field, label)
            for needle, label in MERMAID_THEME_FORBIDDEN if needle in lowered]


def mermaid_theme_template():
    """返回一份合法的默认静态主题，供探针与导入样例共用。"""
    return {
        "font_family": "sans-serif",
        "font_size": 16.0,
        "primary_color": "#ECECFF",
        "primary_text_color": "#333333",
        "primary_border_color": "#7B88A8",
        "line_color": "#2F3B4D",
        "secondary_color": "#FFFFDE",
        "tertiary_color": "#F0F0FF",
        "edge_label_background": "#F8FAFC",
        "cluster_background": "#FFFFDE",
        "cluster_border": "#AAAA33",
        "background": "#FFFFFF",
        "sequence_actor_fill": "#EAEAEA",
        "sequence_actor_border": "#666666",
        "sequence_actor_line": "#999999",
        "sequence_note_fill": "#FFF5AD",
        "sequence_note_border": "#AAAA33",
        "sequence_activation_fill": "#F4F4F4",
        "sequence_activation_border": "#666666",
        "text_color": "#333333",
        "git_colors": ["#ff0000"] * 8,
        "git_inv_colors": ["#000000"] * 8,
        "git_branch_label_colors": ["#ffffff"] * 8,
        "git_commit_label_color": "#000021",
        "git_commit_label_background": "#FFFFDE",
        "git_tag_label_color": "#131300",
        "git_tag_label_background": "#ECECFF",
        "git_tag_label_border": "#AAAAFF",
        "pie_colors": ["#ECECFF"] * 12,
        "pie_title_text_size": 25.0,
        "pie_title_text_color": "#333333",
        "pie_section_text_size": 17.0,
        "pie_section_text_color": "#333333",
        "pie_legend_text_size": 17.0,
        "pie_legend_text_color": "#333333",
        "pie_stroke_color": "#333333",
        "pie_stroke_width": 1.0,
        "pie_outer_stroke_width": 2.0,
        "pie_outer_stroke_color": "#333333",
        "pie_opacity": 0.7,
    }
