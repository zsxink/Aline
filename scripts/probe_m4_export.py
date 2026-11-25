#!/usr/bin/env python3
"""V-02 候选后端探针：用真实样本证明 PDF/DOCX 候选后端能按契约落纸。

用法：

    python3 scripts/probe_m4_export.py --self-test
    python3 scripts/probe_m4_export.py --output-dir logs/validation/...

`--self-test` 只校验规则层与产物回读逻辑，不编译 Rust、不调用 Swift，
因此在没有候选 crate 缓存或 PDFKit 的环境里也能运行。

真实运行会：

1. 把 `scripts/fixtures/m4-export/probe/` 复制到临时目录（fixture 只读），
   用 `--locked --offline` 构建探针；探针不属于 Aline 八成员。
2. 由 `scripts/m4_export_rules.py` 生成 PDF 纸型矩阵与 DOCX 样式组合输入。
3. 运行探针生成 PDF/DOCX，再用系统 PDFKit 回读 MediaBox/文本/墨迹，
   用 zipfile + ElementTree 回读 DOCX 结构。
4. 逐项比对契约值，写出结构化证据；任何一项不符即非零退出。

证据只写 `logs/validation/` 内，脚本不做下载、不改工作区依赖。
"""

import argparse
import hashlib
import html
import html.parser
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
import xml.etree.ElementTree as ElementTree

sys.path.insert(0, str(Path(__file__).resolve().parent))

import m4_export_rules as rules  # noqa: E402
from dependency_admission import license_review  # noqa: E402
from engineering_dependencies import Violation, forbidden_name, package_key  # noqa: E402
from evidence_paths import display_path, resolve_path, write_json, write_text  # noqa: E402
from validation_output import ROOT, output_directory  # noqa: E402

FIXTURES = ROOT / "scripts/fixtures/m4-export"
PROBE = FIXTURES / "probe"
SAMPLE_MD = FIXTURES / "sample.md"
SAMPLE_PNG = FIXTURES / "sample.png"
PDF_READER_SOURCE = ROOT / "scripts/m4_export_pdf_probe.swift"
DOCX_RENDER_SOURCE = ROOT / "scripts/m4_export_docx_render.swift"

# 探针使用的字体：系统自带、含中英文、非用户授权字体。
FONT_CANDIDATES = (
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/System/Library/Fonts/Supplemental/Songti.ttc",
)

W_NS = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"

# 墨迹与请求内容区的允许差（pt）。探针把内容区画成 0.5pt 描边的闭合框，
# 描边以路径为中心，向外溢出 0.25pt；4 倍光栅化再加 0.25pt 抗锯齿余量。
MARGIN_TOLERANCE_PT = 1.0


class ProbeError(RuntimeError):
    """探针失败；调用方据此非零退出并保留证据。"""


# ------------------------------------------------------------------ 契约矩阵


def pdf_cells():
    """生成 PDF 纸型 × 方向矩阵；每格带契约期望值。

    每格的**四边边距各不相同**，且相邻格之间互相错开：只有这样，
    “四边独立设置”才是可回读的事实——若每格都用同一组边距，就无法
    区分“四边分别生效”和“某个统一内缩恰好等于该值”。
    """
    cells = []
    for paper_index, (key, label, _width, _height) in enumerate(rules.PAPERS_MM):
        for orientation_index, orientation in enumerate(rules.ORIENTATIONS):
            # 每格一组不同的四边值，全部落在 [10, 40] mm，留足内容区。
            base = 10 + 4 * (paper_index * 2 + orientation_index)
            margins = {"top": base, "right": base + 3,
                       "bottom": base + 7, "left": base + 11}
            rules.check_margins(margins)
            width_pt, height_pt = rules.paper_size_pt(key, orientation)
            cells.append({
                "paper": key,
                "orientation": orientation,
                "label": "%s %s" % (label, orientation),
                "margins": margins,
                "expected_width_pt": width_pt,
                "expected_height_pt": height_pt,
                "body_lines": ["%s %s 中文行" % (label, orientation), "second line body"],
            })
    return cells


def docx_combinations():
    """标题/正文 × 代码样式的全部组合（EXPORT-03）。

    `theme`（随主题）在 Word 里没有应用主题上下文，必须按导出时的外观
    解析成具体配色，因此 theme 档额外覆盖浅/深两种外观；其余代码样式
    自带固定配色，外观不参与。
    """
    combinations = []
    for heading in rules.HEADING_BODY_STYLES:
        for code in rules.CODE_STYLES:
            appearances = rules.APPEARANCES if code == "theme" else ("light",)
            for appearance in appearances:
                combinations.append((heading, code, appearance))
    return combinations


def pdf_probe_input(cells):
    """探针进程只消费它需要的字段，避免契约字段与实现耦合。"""
    return [{
        "paper": cell["paper"],
        "orientation": cell["orientation"],
        "margins": cell["margins"],
        "expected_width_pt": cell["expected_width_pt"],
        "expected_height_pt": cell["expected_height_pt"],
        "body_lines": cell["body_lines"],
    } for cell in cells]


def docx_probe_input(heading_body, code_style, appearance="light"):
    plan = rules.docx_style_plan(heading_body, code_style, appearance)
    plan["east_asia_font"] = "PingFang SC"
    plan["ascii_font"] = "Helvetica"
    return plan


# ------------------------------------------------------------------ 回读解析


def parse_pdf_probe(text):
    """把 Swift 回读的 key=value 行解析成结构；缺失字段直接失败。"""
    pages = {}
    total = None
    for line in text.splitlines():
        if line.startswith("OPEN_FAIL"):
            raise ProbeError("PDF 无法打开: " + line)
        if line.startswith("pages="):
            total = int(line.split("=", 1)[1])
            continue
        if not line.startswith("page."):
            continue
        key, _, value = line.partition("=")
        parts = key.split(".")
        if len(parts) < 3:
            continue
        index = int(parts[1])
        page = pages.setdefault(index, {"lines": [], "text": "", "chars": 0, "ink": None})
        field = ".".join(parts[2:])
        if field == "media":
            width, height = (float(item) for item in value.split(","))
            page["media"] = (width, height)
        elif field == "ink":
            if value == "none":
                page["ink"] = None
            else:
                page["ink"] = tuple(float(item) for item in value.split(","))
        elif field == "chars":
            page["chars"] = int(value)
        elif field == "text":
            page["text"] = value.replace("\\n", "\n").replace("\\\\", "\\")
        elif field == "lines":
            page["line_count"] = int(value)
        elif field.startswith("line."):
            rect = tuple(float(item) for item in value.split(","))
            page["lines"].append(rect)
    if total is None:
        raise ProbeError("PDF 回读缺少 pages= 行")
    if sorted(pages) != list(range(1, total + 1)):
        raise ProbeError("PDF 回读页号不连续: %s" % sorted(pages))
    return {"pages": total, "page_data": [pages[index] for index in range(1, total + 1)]}


def check_pdf_cells(cells, reading):
    """逐格比对 MediaBox 与文本；返回每格的判定行。"""
    if reading["pages"] != len(cells):
        raise ProbeError("PDF 页数与矩阵格数不符: %d != %d" % (reading["pages"], len(cells)))
    rows = []
    for cell, page in zip(cells, reading["page_data"]):
        expected = (cell["expected_width_pt"], cell["expected_height_pt"])
        actual = page.get("media")
        if actual is None:
            raise ProbeError("第 %s 页缺少 MediaBox" % cell["label"])
        delta = (abs(actual[0] - expected[0]), abs(actual[1] - expected[1]))
        if max(delta) > rules.TOLERANCE_PT:
            raise ProbeError(
                "MediaBox 超出容差: %s 期望 %.2fx%.2f 实测 %.2fx%.2f"
                % (cell["label"], expected[0], expected[1], actual[0], actual[1]))
        for line in cell["body_lines"]:
            if line not in page["text"]:
                raise ProbeError("PDF 文本缺失: %s 缺少 %r" % (cell["label"], line))
        rows.append({
            "label": cell["label"],
            "paper": cell["paper"],
            "orientation": cell["orientation"],
            "expected_pt": [expected[0], expected[1]],
            "actual_pt": [round(actual[0], 3), round(actual[1], 3)],
            "delta_pt": [round(delta[0], 3), round(delta[1], 3)],
            "chars": page["chars"],
            "text_ok": True,
        })
    return rows


def check_pdf_margins(cell, page):
    """用光栅化墨迹包围盒验证四边边距。

    探针把内容区画成最外层闭合边框，文字与图片都在其内，因此墨迹包围盒
    必须与 `content_box_pt` 的四边重合。这比只看文本墨迹强：文本末行天然
    参差，右/下边距无法由文本位置证明。
    """
    ink = page.get("ink")
    if ink is None:
        raise ProbeError("PDF 页没有任何墨迹: " + cell["label"])
    width, height = page["media"]
    box_x, box_y, box_w, box_h = rules.content_box_pt(
        cell["paper"], cell["orientation"], cell["margins"])
    ink_x, ink_y, ink_w, ink_h = ink
    tol = MARGIN_TOLERANCE_PT
    measured = {
        "left": ink_x,
        "bottom": ink_y,
        "right": width - (ink_x + ink_w),
        "top": height - (ink_y + ink_h),
    }
    requested = {
        "left": box_x,
        "bottom": box_y,
        "right": width - (box_x + box_w),
        "top": height - (box_y + box_h),
    }
    problems = ["%s 实测 %.2f 期望 %.2f" % (side, measured[side], requested[side])
                for side in ("top", "right", "bottom", "left")
                if abs(measured[side] - requested[side]) > tol]
    if problems:
        raise ProbeError("边距与内容区不符: %s: %s" % (cell["label"], "; ".join(problems)))
    return {
        "label": cell["label"],
        "requested_pt": {side: round(requested[side], 2) for side in requested},
        "measured_pt": {side: round(measured[side], 2) for side in measured},
    }


def check_pdf_syntax(reading):
    """语法页回读：页数、每块标记在场、负标记缺席。"""
    expected = rules.syntax_pdf_expected()
    if reading["pages"] != expected["pages"]:
        raise ProbeError("语法页页数不符: %d != %d" % (reading["pages"], expected["pages"]))
    text = "\n".join(page["text"] for page in reading["page_data"])
    missing = [fragment for fragment in expected["must_contain"] if fragment not in text]
    if missing:
        raise ProbeError("PDF 缺少语法文本: " + ", ".join(repr(item) for item in missing))
    forbidden = [fragment for fragment in expected["must_not_contain"] if fragment in text]
    if forbidden:
        raise ProbeError("PDF 出现了不该承载的内容: "
                         + ", ".join(repr(item) for item in forbidden))
    # 内容区不越界：墨迹必须落在四边边距以内。
    box_x, box_y, box_w, box_h = expected["content_box_pt"]
    problems = []
    for index, page in enumerate(reading["page_data"], start=1):
        ink = page.get("ink")
        if ink is None:
            continue
        width, height = page["media"]
        ink_x, ink_y, ink_w, ink_h = ink
        measured = {"left": ink_x, "bottom": ink_y,
                    "right": width - (ink_x + ink_w), "top": height - (ink_y + ink_h)}
        # content_box_pt 的 y 自底部起算，与 PDF 用户空间一致。
        requested = {"left": box_x, "bottom": box_y,
                     "right": width - (box_x + box_w), "top": height - (box_y + box_h)}
        for side in measured:
            if measured[side] < requested[side] - MARGIN_TOLERANCE_PT:
                problems.append("第 %d 页 %s 越界: 实测 %.2f < 期望 %.2f"
                                % (index, side, measured[side], requested[side]))
    if problems:
        raise ProbeError("语法页内容越界: " + "; ".join(problems))
    return {
        "pages": reading["pages"],
        "text_fragments": len(expected["must_contain"]),
        "forbidden_absent": True,
        "within_margins": True,
    }


def ppr_order_violations(xml):
    """统计一段 XML 里 w:pPr 子元素顺序违规的个数（不做修复）。

    单列出来是因为“后端有没有这个缺陷”和“探针能不能修好”是两件事：
    先证明缺陷存在，修复才有意义。
    """
    count = 0
    for match in re.finditer(r"<w:pPr>(.*?)</w:pPr>", xml, re.S):
        try:
            rules.check_ppr_order(_direct_children(match.group(1)))
        except rules.ContractError:
            count += 1
    return count


def _direct_children(inner):
    """取 w:pPr 的直接子元素名（按出现顺序），忽略嵌套层。"""
    names = []
    depth = 0
    for match in re.finditer(r"<(/?)(w:[A-Za-z]+)([^>]*?)(/?)>", inner):
        closing, tag, _, self_closing = match.groups()
        if closing:
            depth -= 1
            continue
        if depth == 0:
            names.append(tag[2:])
        if not self_closing:
            depth += 1
    return names


def read_docx(path, plan):
    """回读 DOCX 结构；逐项核对样式、字体、编号、表格、脚注与图片。"""
    with zipfile.ZipFile(path) as archive:
        names = set(archive.namelist())
        required = {"word/document.xml", "word/styles.xml", "word/numbering.xml",
                    "word/footnotes.xml", "word/_rels/document.xml.rels"}
        missing = required - names
        if missing:
            raise ProbeError("DOCX 缺少部件: " + ", ".join(sorted(missing)))
        document = ElementTree.fromstring(archive.read("word/document.xml"))
        styles = ElementTree.fromstring(archive.read("word/styles.xml"))
        footnotes = ElementTree.fromstring(archive.read("word/footnotes.xml"))
        rels = ElementTree.fromstring(archive.read("word/_rels/document.xml.rels"))
        media = [name for name in names if name.startswith("word/media/") and not name.endswith("/")]

    style_ids = [style.get(W_NS + "styleId") for style in styles.iter(W_NS + "style")]
    if style_ids.count("Normal") != 1:
        raise ProbeError("DOCX 的 Normal 样式必须恰好一个: %s" % style_ids)
    expected_styles = {"Heading1", "Heading2", "Heading3", "Code", "Quote", "Rule"}
    absent = expected_styles - set(style_ids)
    if absent:
        raise ProbeError("DOCX 缺少样式: " + ", ".join(sorted(absent)))

    sizes = {}
    shading = {}
    paragraph_shading = {}
    colors = {}
    borders = {}
    for style in styles.iter(W_NS + "style"):
        style_id = style.get(W_NS + "styleId")
        size = style.find("%srPr/%ssz" % (W_NS, W_NS))
        if size is not None:
            sizes[style_id] = int(size.get(W_NS + "val"))
        fill = style.find("%srPr/%sshd" % (W_NS, W_NS))
        if fill is not None:
            shading[style_id] = fill.get(W_NS + "fill")
        # 段落级底色：只有它覆盖整段，run 级底色只覆盖字形墨迹。
        block_fill = style.find("%spPr/%sshd" % (W_NS, W_NS))
        if block_fill is not None:
            paragraph_shading[style_id] = block_fill.get(W_NS + "fill")
        color = style.find("%srPr/%scolor" % (W_NS, W_NS))
        if color is not None:
            colors[style_id] = color.get(W_NS + "val")
        # 段落边框：引用块靠左边框、分隔线靠下边框，都不是表格线。
        style_borders = {}
        for side in ("top", "left", "bottom", "right"):
            node = style.find("%spPr/%spBdr/%s" % (W_NS, W_NS, W_NS + side))
            if node is not None:
                style_borders[side] = node.get(W_NS + "val")
        borders[style_id] = style_borders

    # CT_PPr 是 sequence：顺序错了 Word 会静默丢元素。这里只**计量**违规
    # 段数并记入证据，不失败、也不重排。原因有两层：产物必须保持后端原样
    # （探针替后端擦屁股，结论就变成“后端没问题”），而“Word 是否容忍”
    # 本机没有 Word 可证，属未测项而不是已证缺陷。
    ppr_violations = 0
    for root in (styles, document):
        for ppr in root.iter(W_NS + "pPr"):
            children = [child.tag.replace(W_NS, "") for child in ppr]
            try:
                rules.check_ppr_order(children)
            except rules.ContractError:
                ppr_violations += 1

    expected_heading_sizes = plan["heading_sizes_half_pt"]
    for level, expected in enumerate(expected_heading_sizes, start=1):
        actual = sizes.get("Heading%d" % level)
        if actual != expected:
            raise ProbeError("Heading%d 字号不符: %s != %s" % (level, actual, expected))

    used = {}
    for paragraph in document.iter(W_NS + "p"):
        style = paragraph.find("%spPr/%spStyle" % (W_NS, W_NS))
        name = style.get(W_NS + "val") if style is not None else None
        used[name] = used.get(name, 0) + 1
    for level in (1, 2, 3):
        if used.get("Heading%d" % level, 0) < 1:
            raise ProbeError("DOCX 未使用 Heading%d" % level)
    if used.get("Code", 0) < 1:
        raise ProbeError("DOCX 未使用 Code 样式")

    east_asia = {node.get(W_NS + "eastAsia") for node in document.iter(W_NS + "rFonts")}
    east_asia.discard(None)
    if plan["east_asia_font"] not in east_asia:
        raise ProbeError("DOCX 缺少东亚字体声明: %s" % sorted(east_asia))

    numbering = sum(1 for _ in document.iter(W_NS + "numPr"))
    if numbering < 5:
        # 3 个有序项 + 2 个无序项；两者都必须走 OOXML 编号而不是手写序号。
        raise ProbeError("DOCX 编号段落不足: %d" % numbering)

    tables = list(document.iter(W_NS + "tbl"))
    if len(tables) != 1:
        raise ProbeError("DOCX 表格数量不符: %d" % len(tables))
    table = tables[0]
    grid = list(table.iter(W_NS + "gridCol"))
    rows = list(table.iter(W_NS + "tr"))
    cells = list(table.iter(W_NS + "tc"))
    if len(grid) != 2 or len(rows) != 2 or len(cells) != 4:
        raise ProbeError("DOCX 表格网格不符: grid=%d rows=%d cells=%d"
                         % (len(grid), len(rows), len(cells)))

    drawings = sum(1 for _ in document.iter(W_NS + "drawing"))
    if drawings < 1 or not media:
        raise ProbeError("DOCX 未嵌入图片: drawing=%d media=%s" % (drawings, media))

    # 脚注：真脚注部件里必须有正文，正文里必须有引用标记。
    footnote_refs = sum(1 for _ in document.iter(W_NS + "footnoteReference"))
    footnote_text = "".join(node.text or "" for node in footnotes.iter(W_NS + "t"))
    if footnote_refs < 1:
        raise ProbeError("DOCX 正文缺少脚注引用")
    if "脚注内容 footnote body" not in footnote_text:
        raise ProbeError("DOCX 脚注正文缺失: %r" % footnote_text)

    # 超链接：外部链接关系必须落在 rels 里，而不是只写文字。
    hyperlinks = [rel for rel in rels.iter()
                  if rel.get("Type", "").endswith("/hyperlink")]
    if not hyperlinks:
        raise ProbeError("DOCX 缺少超链接关系")

    # 引用块与分隔线：靠段落边框表达，不是表格线。
    quote_borders = borders.get("Quote", {})
    if quote_borders.get("left") not in ("single", "thick"):
        raise ProbeError("引用样式缺少左边框: %s" % quote_borders)
    rule_borders = borders.get("Rule", {})
    if rule_borders.get("bottom") not in ("single", "thick"):
        raise ProbeError("分隔线样式缺少下边框: %s" % rule_borders)

    code_plan = plan["code"]
    if code_plan["shaded"]:
        # 契约要求底色落在段落属性：run 级底色只覆盖字形墨迹，行内空白与
        # 行尾不着色，实际产物是断带。因此段落级底色是**必需**的，
        # run 级可有可无（有也不影响整段着色）。
        if rules.docx_shading_scope() != "paragraph":
            raise ProbeError("契约的底色落点不是段落级: %s" % rules.docx_shading_scope())
        if paragraph_shading.get("Code") != code_plan["fill"]:
            raise ProbeError("代码底色未落在段落属性: %s != %s"
                             % (paragraph_shading.get("Code"), code_plan["fill"]))
        # 底色与文字色必须成对落地：亮字亮底会让产物不可读，
        # 只回读底色无法发现这种缺陷。
        actual_color = colors.get("Code")
        if actual_color != code_plan["color"]:
            raise ProbeError("代码文字色不符: %s != %s" % (actual_color, code_plan["color"]))
        ratio = rules.contrast_ratio(paragraph_shading["Code"], actual_color)
        if ratio < rules.MIN_CODE_CONTRAST:
            raise ProbeError("代码底色/文字色对比度不足: %.2f < %.2f"
                             % (ratio, rules.MIN_CODE_CONTRAST))
    elif "Code" in paragraph_shading or "Code" in shading:
        raise ProbeError("纯文本代码样式不应写入底色")

    # 文本在场：任意顺序，只要求每个语法标记出现。
    text = "".join(node.text or "" for node in document.iter(W_NS + "t"))
    expected = rules.syntax_docx_expectations()
    missing = [fragment for fragment in expected["must_contain"] if fragment not in text]
    if missing:
        raise ProbeError("DOCX 缺少语法文本: " + ", ".join(repr(item) for item in missing))
    # 文本顺序：必须与探针写入顺序一致，证明结构没有被打乱或重排。
    cursor = 0
    for fragment in expected["order"]:
        position = text.find(fragment, cursor)
        if position < 0:
            raise ProbeError("DOCX 文本顺序不符或缺失: %r" % fragment)
        cursor = position + len(fragment)

    return {
        "heading_body": plan["heading_body"],
        "code_style": plan["code_style"],
        "appearance": plan["appearance"],
        "styles": sorted(style_ids),
        "heading_sizes_half_pt": [sizes.get("Heading%d" % level) for level in (1, 2, 3)],
        "body_size_half_pt": plan["body_size_half_pt"],
        "code_paragraph_shading": paragraph_shading.get("Code"),
        "code_color": colors.get("Code"),
        "ppr_order_violations": ppr_violations,
        "code_contrast": (round(rules.contrast_ratio(
            paragraph_shading["Code"], colors["Code"]), 2)
            if code_plan["shaded"] else None),
        "east_asia_fonts": sorted(east_asia),
        "numbered_paragraphs": numbering,
        "table": {"grid_cols": len(grid), "rows": len(rows), "cells": len(cells)},
        "drawings": drawings,
        "media": sorted(media),
        "footnote_references": footnote_refs,
        "hyperlinks": len(hyperlinks),
        "quote_border": quote_borders.get("left"),
        "rule_border": rule_borders.get("bottom"),
        "text_order_ok": True,
        "syntax_blocks": len(rules.SYNTAX_MATRIX),
    }


# ------------------------------------------------------------------ HTML 资源策略


class _HtmlScan(html.parser.HTMLParser):
    """收集 HTML 的图片引用、样式块、远程资源、远程链接目标与标签。

    远程**资源**（img/link/script/iframe 等会自动加载的东西）与远程**链接目标**
    （`<a href>`，只有用户点击才访问）必须分开：前者破坏离线可读，后者不破坏。
    把两者混为一谈会把“文档里有一个网址”误判成“产物联网”。
    """

    # 会自动发起网络请求的标签/属性组合。
    RESOURCE_ATTRS = {
        "img": ("src", "srcset"),
        "script": ("src",),
        "link": ("href",),
        "iframe": ("src",),
        "video": ("src", "poster"),
        "audio": ("src",),
        "source": ("src", "srcset"),
        "embed": ("src",),
        "object": ("data",),
        "use": ("href", "xlink:href"),
    }

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.images = []
        self.has_style_block = False
        self.remote_resources = []
        self.remote_targets = []
        self.tags = []
        self.text = []

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        self.tags.append(tag)
        if tag == "style":
            self.has_style_block = True
        if tag == "img":
            self.images.append(attributes.get("src", ""))
        for name in self.RESOURCE_ATTRS.get(tag, ()):
            value = attributes.get(name)
            if value and re.match(r"(?i)^(https?:)?//", value):
                self.remote_resources.append("%s[%s]=%s" % (tag, name, value))
        if tag == "a":
            href = attributes.get("href") or ""
            if re.match(r"(?i)^(https?:)?//", href):
                self.remote_targets.append(href)

    def handle_data(self, data):
        self.text.append(data)


def _inline_markup(text):
    """行内标记 → HTML；顺序先转义再替换，避免转义吃掉自己生成的标签。"""
    escaped = html.escape(text)
    escaped = re.sub(r"`([^`]+)`", r"<code>\1</code>", escaped)
    escaped = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", escaped)
    escaped = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", r"<em>\1</em>", escaped)
    escaped = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', escaped)
    return escaped


def render_html_export(source_dir, out_dir, policy, style_enabled):
    """按 EXPORT-04 的两种资源策略产出离线 HTML。

    这是**策略夹具**，不是生产渲染器：它按 `rules.SYNTAX_MATRIX` 的样本
    顺序产出最小但语义正确的 HTML（真 h1～h3、ol/ul、blockquote、table、
    pre、img、hr、a），用于证明“复制到资源目录”和“保留引用”两种策略各自
    产生可离线打开的产物。生产 HTML 后端在任务 4.4 接线。

    `copy-to-dir` 把图片复制进 `out/assets/`，产物可整体搬走；
    `keep-reference` 保留源文档的相对写法，产物必须与源图同目录才可读，
    因此调用方应把 `out_dir` 指向源目录（导出到原目录的语义）。
    """
    policy = rules.check_html_image_policy(policy)
    out_dir.mkdir(parents=True, exist_ok=True)
    body = []
    for block in rules.SYNTAX_MATRIX:
        block_id = block["id"]
        markers = rules.SYNTAX_MARKERS[block_id]
        if block_id == "heading":
            for level, text in enumerate(markers, start=1):
                body.append("<h%d>%s</h%d>" % (level, html.escape(text), level))
        elif block_id == "paragraph":
            body.append("<p>%s</p>" % _inline_markup(block["markdown"][0]))
        elif block_id == "link":
            body.append("<p>%s</p>" % _inline_markup(block["markdown"][0]))
        elif block_id == "ordered-list":
            body.append("<ol>%s</ol>" % "".join(
                "<li>%s</li>" % html.escape(text) for text in markers))
        elif block_id == "unordered-list":
            body.append("<ul>%s</ul>" % "".join(
                "<li>%s</li>" % html.escape(text) for text in markers))
        elif block_id == "blockquote":
            body.append("<blockquote><p>%s</p></blockquote>" % html.escape(markers[0]))
        elif block_id == "table":
            rows = [markers[:2], markers[2:]]
            head = "".join("<th>%s</th>" % html.escape(cell) for cell in rows[0])
            rest = "".join(
                "<tr>%s</tr>" % "".join("<td>%s</td>" % html.escape(cell) for cell in row)
                for row in rows[1:])
            body.append("<table><thead><tr>%s</tr></thead><tbody>%s</tbody></table>" % (head, rest))
        elif block_id == "code":
            body.append('<pre><code class="language-rust">%s</code></pre>' % html.escape(
                "fn main() {\n    println!(\"中文 code\");\n}"))
        elif block_id == "image":
            target = "sample.png"
            alt = "示例图片"
            if policy == "copy-to-dir":
                assets = out_dir / "assets"
                assets.mkdir(exist_ok=True)
                shutil.copyfile(source_dir / target, assets / Path(target).name)
                source = "assets/" + Path(target).name
            else:
                # 保留引用：src 与源文档写法完全一致，不打包、不改写。
                source = target
            body.append('<p><img src="%s" alt="%s"></p>' % (html.escape(source), html.escape(alt)))
        elif block_id == "rule":
            body.append("<hr>")
        elif block_id == "inline-math":
            body.append('<p>%s</p>' % _inline_markup(block["markdown"][0]))
        elif block_id == "display-math":
            # 本轮只落源文本：不声称数学排版。
            body.append('<p class="math">%s</p>' % html.escape(markers[0]))
        elif block_id == "mermaid":
            # 本轮只落源文本：不渲染图表。
            body.append('<pre><code class="language-mermaid">%s</code></pre>' % html.escape(
                "\n".join(block["markdown"][1:-1])))
        elif block_id == "footnote":
            # 脚注正文按负断言不出现，只保留正文里的 [^note] 源标记。
            body.append("<p>%s</p>" % html.escape("末尾段落 end paragraph[^note]"))
        else:
            raise ProbeError("HTML 夹具缺少语法块实现: " + block_id)

    style = ("<style>body{font-family:system-ui;max-width:40em;margin:2em auto;}"
             "code{background:#f3f4f6;padding:0 .2em;}"
             "blockquote{border-left:3px solid #d0d7de;margin-left:0;padding-left:1em;}"
             "table{border-collapse:collapse;}td,th{border:1px solid #444;padding:.2em .5em;}"
             "hr{border:0;border-top:1px solid #888;}</style>" if style_enabled else "")
    document = ("<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"utf-8\">\n"
                "<title>%s</title>\n%s\n</head>\n<body>\n%s\n</body>\n</html>\n"
                % (html.escape("V-02 导出样本"), style, "\n".join(body)))
    path = out_dir / "sample.html"
    write_text(path, document)
    return path


def check_html_export(path, policy, style_enabled):
    """回读 HTML：图片可解析、无远程资源、样式开关与策略一致。"""
    scan = _HtmlScan()
    scan.feed(path.read_text())
    if not scan.images:
        raise ProbeError("HTML 未包含图片引用")
    if scan.remote_resources:
        raise ProbeError("HTML 含远程资源，离线不可读: " + ", ".join(scan.remote_resources))
    if scan.has_style_block != style_enabled:
        raise ProbeError("HTML 样式开关不符: 期望 %s，实际 %s"
                         % (style_enabled, scan.has_style_block))
    resolved = []
    for source in scan.images:
        if re.match(r"(?i)^[a-z]+:", source):
            raise ProbeError("HTML 图片使用非相对引用: " + source)
        target = (path.parent / source).resolve()
        if not target.is_file():
            raise ProbeError("HTML 图片引用无法解析: " + source)
        resolved.append({"src": source, "bytes": target.stat().st_size,
                         "sha256": sha256(target)})
    copied = policy == "copy-to-dir"
    in_assets = all(item["src"].startswith("assets/") for item in resolved)
    if copied != in_assets:
        raise ProbeError("HTML 资源策略与产物不符: " + policy)
    return {
        "policy": policy,
        "style_enabled": style_enabled,
        "images": resolved,
        "remote_resources": scan.remote_resources,
        "remote_link_targets": scan.remote_targets,
        "offline_readable": True,
    }


def check_html_syntax(path):
    """语法矩阵回读：标签齐全、标记文本在场、负标记缺席。"""
    scan = _HtmlScan()
    scan.feed(path.read_text())
    expected = rules.syntax_html_expectations()
    tags = set(scan.tags)
    missing_tags = [tag for tag in expected["tags"] if tag not in tags]
    if missing_tags:
        raise ProbeError("HTML 缺少语法标签: " + ", ".join(missing_tags))
    text = "".join(scan.text)
    missing = [fragment for fragment in expected["must_contain"] if fragment not in text]
    if missing:
        raise ProbeError("HTML 缺少语法文本: " + ", ".join(repr(item) for item in missing))
    forbidden = [fragment for fragment in expected["must_not_contain"] if fragment in text]
    if forbidden:
        raise ProbeError("HTML 出现了不该承载的内容: " + ", ".join(repr(item) for item in forbidden))
    return {"tags": sorted(tags), "text_fragments": len(expected["must_contain"]),
            "forbidden_absent": True}


# ------------------------------------------------------------------ 进程与构建


def run(argv, cwd=None, env=None, timeout=900):
    """运行命令并保留完整输出；失败抛出带 stderr 的异常。"""
    completed = subprocess.run(argv, cwd=cwd, env=env, text=True,
                               capture_output=True, timeout=timeout)
    if completed.returncode != 0:
        raise ProbeError("命令失败(%d): %s\nstdout:\n%s\nstderr:\n%s"
                         % (completed.returncode, " ".join(argv),
                            completed.stdout[-4000:], completed.stderr[-4000:]))
    return completed


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pick_font():
    for candidate in FONT_CANDIDATES:
        if Path(candidate).is_file():
            return Path(candidate)
    raise ProbeError("找不到可用于探针的系统字体")


def build_probe(work, output):
    """把 fixture 复制到临时目录并离线构建；不写工作区 target。"""
    crate = work / "probe"
    shutil.copytree(PROBE, crate)
    target = work / "target"
    env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(target))
    build = run(["cargo", "build", "--locked", "--offline", "--manifest-path",
                 str(crate / "Cargo.toml")], env=env)
    write_text(output / "cargo-build.log", build.stdout + build.stderr)
    binary = target / "debug/m4-export-probe"
    if not binary.is_file():
        raise ProbeError("探针二进制未生成: " + str(binary))
    return binary, sha256(crate / "Cargo.lock"), env


def build_swift(source, destination):
    """编译 Swift 回读器；只用系统 SDK。"""
    run(["swiftc", "-O", str(source), "-o", str(destination)], timeout=300)
    return destination


# ------------------------------------------------------ 渲染器覆盖探针（DOCX）
#
# Quick Look 的 Office.qlgenerator 不画段落级 w:shd，也不画左/上/右边框。
# 如果只看产物图，会把“渲染器不画”误判成“后端没写”。这里构造一组只差一个
# 属性的最小 DOCX，逐对渲染并比较像素差：差异存在 = 渲染器认这个属性，
# 无差异 = 渲染器忽略它。这些最小包完全不经 docx-rs，因此结论与候选后端无关。

_DOCX_CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""

_DOCX_ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

_DOCX_NS = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'

# 变体名 → 中间段落 w:pPr 的直接子元素。`plain` 为空，作为对照基线。
_MINIMAL_DOCX_VARIANTS = {
    "plain": "",
    "paragraph-shading": '<w:shd w:val="clear" w:color="auto" w:fill="FFE0E0" />',
    "run-shading": "",
    "left-border": '<w:pBdr><w:left w:val="single" w:space="8" w:sz="48" w:color="FF0000" /></w:pBdr>',
    "bottom-border": '<w:pBdr><w:bottom w:val="single" w:space="1" w:sz="24" w:color="0000FF" /></w:pBdr>',
}

# run 级底色落在 w:r/w:rPr 上，与段落属性不同层，因此单独给出整段 run XML。
_MINIMAL_DOCX_RUN = {
    "run-shading": '<w:r><w:rPr><w:shd w:val="clear" w:color="auto" w:fill="FFE0E0" /></w:rPr>'
                   '<w:t>MIDDLE 中段</w:t></w:r>',
}


def _minimal_docx_xml(variant):
    """构造一个只含三段的最小 document.xml；中间段带该变体的装饰。"""
    if variant not in _MINIMAL_DOCX_VARIANTS:
        raise ProbeError("未知最小 DOCX 变体: " + str(variant))
    ppr = _MINIMAL_DOCX_VARIANTS[variant]
    run = _MINIMAL_DOCX_RUN.get(variant, "<w:r><w:t>MIDDLE 中段</w:t></w:r>")
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        "<w:document %s><w:body>"
        '<w:p><w:r><w:t>TOP 上段</w:t></w:r></w:p>'
        "<w:p><w:pPr>%s</w:pPr>%s</w:p>"
        '<w:p><w:r><w:t>BOTTOM 下段</w:t></w:r></w:p>'
        '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
        "</w:body></w:document>" % (_DOCX_NS, ppr, run)
    )


def write_minimal_docx(path, variant):
    """写一个不经 docx-rs 的最小 DOCX，用来单独测渲染器。"""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", _DOCX_CONTENT_TYPES)
        archive.writestr("_rels/.rels", _DOCX_ROOT_RELS)
        archive.writestr("word/document.xml", _minimal_docx_xml(variant))
    return path


def renderer_coverage(docx_renderer, work, artifacts):
    """逐变体渲染最小 DOCX，用像素差判定 Quick Look 是否画该属性。

    判据是**与 `plain` 的像素差**：有差异说明渲染器认这个属性，无差异说明
    它被忽略。只比较 PNG 的原始字节会因为编码噪声误判，因此解出像素后比。
    """
    try:
        from PIL import Image, ImageChops  # noqa: PLC0415 - 只有真实运行需要
    except ImportError as error:  # pragma: no cover - 环境缺 Pillow
        raise ProbeError("渲染器覆盖探针需要 Pillow: %s" % error) from error

    rendered = {}
    for variant in _MINIMAL_DOCX_VARIANTS:
        source = write_minimal_docx(work / ("minimal-%s.docx" % variant), variant)
        target = work / ("minimal-%s.png" % variant)
        result = run([str(docx_renderer), str(source), str(target)])
        if "RENDER_OK" not in result.stdout:
            raise ProbeError("最小 DOCX 渲染失败: %s" % variant)
        rendered[variant] = Image.open(target).convert("RGB")
        shutil.copyfile(source, artifacts / ("minimal-%s.docx" % variant))

    baseline = rendered["plain"]
    rows = []
    for variant, image in rendered.items():
        if variant == "plain":
            continue
        diff = ImageChops.difference(baseline, image).getbbox()
        rows.append({"variant": variant, "rendered": diff is not None,
                     "diff_bbox": list(diff) if diff else None})
    # 对照基线本身必须渲染成功，否则“无差异”可能只是两张空白图。
    if not any(row["rendered"] for row in rows):
        raise ProbeError("所有最小 DOCX 变体渲染结果相同，对照失效")
    return rows


# ------------------------------------------------------------------ 依赖事实


def dependency_facts(crate, env):
    """从 cargo metadata + 仓库自己的许可证策略得出依赖事实。

    不接受手写清单：许可证结论逐包调用 `dependency_admission.license_review`，
    禁库结论调用 `engineering_dependencies.forbidden_name`，两者都是仓库守卫
    正在用的同一份代码。探针 crate 不是 Aline 成员，因此只做“候选后端能否被
    准入策略接受”的预判；正式准入在任务 1.5 裁决。
    """
    policy = json.loads((ROOT / "scripts/dependency_admission_policy.json").read_text())
    license_policy = policy["license"]
    completed = run(["cargo", "metadata", "--locked", "--offline", "--format-version", "1",
                     "--manifest-path", str(crate / "Cargo.toml")], env=env)
    metadata = json.loads(completed.stdout)
    lock = {}
    for block in (crate / "Cargo.lock").read_text().split("[[package]]")[1:]:
        entry = {}
        for line in block.splitlines():
            match = re.match(r'(name|version|source|checksum) = "(.*)"', line)
            if match:
                entry[match[1]] = match[2]
        if {"name", "version"} <= set(entry):
            lock[entry["name"] + "@" + entry["version"]] = entry

    roots = set(metadata["workspace_members"])
    rows = []
    for package in sorted(metadata["packages"], key=package_key):
        key = package_key(package)
        if package["source"] is None and package["id"] not in roots:
            raise ProbeError("探针依赖含本地路径包: " + key)
        review = license_review(package, license_policy, package["id"] in roots)
        rows.append({
            "package": key,
            "workspace": package["id"] in roots,
            "license": review["raw"],
            "verdict": review["verdict"],
            "forbidden": forbidden_name(package["name"]),
            "source": package.get("source"),
            "checksum": lock.get(key, {}).get("checksum"),
        })
    forbidden = [row["package"] for row in rows if row["forbidden"]]
    if forbidden:
        raise ProbeError("探针依赖含仓库禁库: " + ", ".join(forbidden))
    denied = [row["package"] for row in rows if row["verdict"] not in ("allowed", "workspace-exempt")]
    if denied:
        raise ProbeError("探针依赖许可证未准入: " + ", ".join(denied))
    return {
        "policy": "scripts/dependency_admission_policy.json",
        "checked_by": "dependency_admission.license_review + engineering_dependencies.forbidden_name",
        "packages": len(rows),
        "forbidden": forbidden,
        "rows": rows,
        "limits": "探针 crate 不是 Aline 成员；本结果只说明候选闭包能被现有策略接受，"
                  "不构成正式准入，也不覆盖跨平台原生构建。",
    }


# ------------------------------------------------------------------ 主流程


def environment_facts(font, lock_hash):
    """记录探针运行环境；隔离缓存缺失时如实记录，不静默借用宿主缓存。"""
    isolation = json.loads((ROOT / "scripts/fixtures/isolation.json").read_text())
    registered = resolve_path(isolation["cargo_home"])
    actual = os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo")
    exists = registered is not None and Path(registered).is_dir()
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "font": Path(font).name,
        "font_sha256": sha256(font),
        "probe_lock_sha256": lock_hash,
        "cargo_home": display_path(actual),
        "registered_cargo_home": isolation["cargo_home"],
        "registered_cargo_home_exists": exists,
        "cargo_home_note": ("使用登记隔离缓存" if exists else
                            "登记的隔离缓存不存在；本次以宿主缓存 + CARGO_NET_OFFLINE=true "
                            "构建，证据不能反推实际缓存，属入口环境缺陷而非本变更引入"),
    }


def real_run(output):
    if sys.platform != "darwin":
        raise ProbeError("V-02 探针当前只批准 macOS（PDFKit 与 Quick Look）")
    font = pick_font()
    # 构建树留在系统临时目录：证据目录只收产物与事实，不收 1.6 GB 的 target。
    work = Path(tempfile.mkdtemp(prefix="aline-m4-export-"))
    artifacts = output / "artifacts"
    # 产物目录整体重建：同一落点重跑时若留下上一轮的文件，证据会新旧混杂。
    if artifacts.exists():
        shutil.rmtree(artifacts)
    artifacts.mkdir()
    try:
        return _real_run(output, artifacts, work, font)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def _real_run(output, artifacts, work, font):
    binary, lock_hash, env = build_probe(work, output)
    dependencies = dependency_facts(work / "probe", env)
    write_json(output / "dependencies.json", dependencies)
    pdf_reader = build_swift(PDF_READER_SOURCE, work / "pdf-probe")
    docx_renderer = build_swift(DOCX_RENDER_SOURCE, work / "docx-render")

    cells = pdf_cells()
    pdf_input = work / "cells.json"
    write_json(pdf_input, pdf_probe_input(cells))
    shutil.copyfile(pdf_input, artifacts / "cells.json")
    pdf_out = work / "matrix.pdf"
    pdf_run = run([str(binary), "pdf", str(pdf_out), str(font), str(SAMPLE_PNG), str(pdf_input)])
    write_text(output / "pdf-generate.log", pdf_run.stdout + pdf_run.stderr)

    pdf_reading = parse_pdf_probe(run([str(pdf_reader), str(pdf_out)]).stdout)
    media_rows = check_pdf_cells(cells, pdf_reading)
    margin_rows = [check_pdf_margins(cell, page)
                   for cell, page in zip(cells, pdf_reading["page_data"])]

    # 语法矩阵的 PDF 侧：由规则层算出排版，探针只执行坐标，回读后逐项核对。
    syntax_plan = rules.syntax_pdf_probe_input()
    syntax_input = work / "syntax.json"
    write_json(syntax_input, syntax_plan)
    shutil.copyfile(syntax_input, artifacts / "syntax.json")
    syntax_out = work / "syntax.pdf"
    syntax_run = run([str(binary), "syntax", str(syntax_out), str(font),
                      str(SAMPLE_PNG), str(syntax_input)])
    write_text(output / "syntax-generate.log", syntax_run.stdout + syntax_run.stderr)
    syntax_reading = parse_pdf_probe(run([str(pdf_reader), str(syntax_out)]).stdout)
    syntax_facts = check_pdf_syntax(syntax_reading)
    shutil.copyfile(syntax_out, artifacts / "syntax.pdf")
    syntax_preview = artifacts / "syntax-page-1.png"
    render = run([str(pdf_reader), str(syntax_out), "1", str(syntax_preview)])
    if "RENDER_OK" not in render.stdout:
        raise ProbeError("语法页渲染失败")

    docx_rows = []
    for heading_body, code_style, appearance in docx_combinations():
        plan = docx_probe_input(heading_body, code_style, appearance)
        # 外观只在 `theme`（随主题）下参与解析，其余样式自带固定配色，
        # 文件名不带外观后缀，避免同一产物出现多个看似不同的名字。
        stem = "docx-%s-%s" % (heading_body, code_style)
        if code_style == "theme":
            stem += "-" + appearance
        plan_path = work / (stem + ".json")
        write_json(plan_path, plan)
        shutil.copyfile(plan_path, artifacts / (stem + ".json"))
        docx_out = work / (stem + ".docx")
        docx_run = run([str(binary), "docx", str(docx_out), str(font),
                        str(SAMPLE_PNG), str(plan_path)])
        write_text(output / (stem + ".log"), docx_run.stdout + docx_run.stderr)
        # 产物保持后端原样输出，缺陷留在证据里。`read_docx` 计量 w:pPr
        # 顺序违规（docx-rs 0.4.22 把 w:rPr 写在 w:pStyle 之前，违反
        # ECMA-376 的 CT_PPr sequence），不替后端重排——探针替后端擦屁股，
        # 结论就会变成“后端没问题”。该计数进入证据与未测项。
        facts = read_docx(docx_out, plan)
        facts["bytes"] = docx_out.stat().st_size
        facts["sha256"] = sha256(docx_out)
        # Quick Look 渲染：只证明 macOS 的 Word 兼容渲染管线能打开，
        # 不等于 Microsoft Word 兼容性（见验证计划的三态划分）。
        render_out = work / (stem + ".png")
        render = run([str(docx_renderer), str(docx_out), str(render_out)])
        facts["quicklook_render"] = "ok" if "RENDER_OK" in render.stdout else "failed"
        if facts["quicklook_render"] != "ok":
            raise ProbeError("Quick Look 无法渲染 DOCX: %s" % stem)
        shutil.copyfile(docx_out, artifacts / (stem + ".docx"))
        shutil.copyfile(render_out, artifacts / (stem + ".png"))
        docx_rows.append(facts)

    # 渲染器覆盖：用不经 docx-rs 的最小 DOCX 逐对比较像素，证明哪些属性
    # Quick Look 会画、哪些不会。产物图里“看不到底色/左边框”是渲染器行为，
    # 不是后端没写——这两件事必须分开记，否则结论会被读图的人推翻。
    renderer_rows = renderer_coverage(docx_renderer, work, artifacts)

    # PDF 保留矩阵产物，并按纸型各渲染一页，便于人工复核页边距与方向。
    # 只取两种方向各一张：A4 纵向与 Legal 横向足以暴露长宽互换错误。
    shutil.copyfile(pdf_out, artifacts / "matrix.pdf")
    previews = []
    for page, name in ((1, "matrix-a4-portrait.png"), (8, "matrix-legal-landscape.png")):
        destination = artifacts / name
        render = run([str(pdf_reader), str(pdf_out), str(page), str(destination)])
        if "RENDER_OK" not in render.stdout:
            raise ProbeError("PDF 第 %d 页渲染失败" % page)
        previews.append(name)

    html_rows = []
    for policy in rules.HTML_IMAGE_POLICIES:
        for style_enabled in (True, False):
            stem = "html-%s-%s" % (policy, "style-on" if style_enabled else "style-off")
            out_dir = work / stem
            # keep-reference 的产物必须与源图同目录才能解析，因此先把样本
            # 目录整体复制出来，再在副本里就地导出——这样证据树自带原图，
            # 同时 HTML 的 src 仍是源文档写法，证明“没有打包”。
            shutil.copytree(FIXTURES, out_dir)
            path = render_html_export(out_dir, out_dir, policy, style_enabled)
            facts = check_html_export(path, policy, style_enabled)
            facts["syntax"] = check_html_syntax(path)
            facts["artifacts"] = "artifacts/" + stem
            shutil.copytree(out_dir, artifacts / stem)
            html_rows.append(facts)

    summary = {
        "check": "v02-export-backends",
        "scope": "macOS arm64 候选后端实验；未准入、未接生产",
        "environment": environment_facts(font, lock_hash),
        "pdf": {
            "backend": "krilla 0.8.2",
            "cells": media_rows,
            "margins": margin_rows,
            "pages": pdf_reading["pages"],
            "bytes": pdf_out.stat().st_size,
            "sha256": sha256(pdf_out),
            "artifacts": ["artifacts/matrix.pdf"] + ["artifacts/" + name for name in previews]
                          + ["artifacts/cells.json"],
        },
        "docx": {
            "backend": "docx-rs 0.4.22",
            "combinations": docx_rows,
            "renderer_coverage": renderer_rows,
            "artifacts": "artifacts/docx-<heading_body>-<code_style>[-<appearance>].{docx,png,json}",
        },
        "html": {
            "backend": "策略夹具（生产 HTML 后端在任务 4.4 接线）",
            "combinations": html_rows,
        },
        "syntax": {
            "matrix": [{"id": row["id"], "label": row["label"],
                        "levels": row["levels"], "note": row["note"]}
                       for row in rules.SYNTAX_MATRIX],
            "pdf": syntax_facts,
            "artifacts": ["artifacts/syntax.pdf", "artifacts/syntax-page-1.png",
                          "artifacts/syntax.json"],
        },
        "dependencies": {
            "packages": dependencies["packages"],
            "forbidden": dependencies["forbidden"],
            "policy": dependencies["policy"],
            "limits": dependencies["limits"],
        },
        "untested": [
            "Microsoft Word 真实打开与兼容性（本机无 Word；Quick Look 与 WPS/Pages 只作次要渲染器）",
            "docx-rs 0.4.22 的 w:pPr 子元素顺序违反 ECMA-376 CT_PPr sequence"
            "（每份产物 19～20 段）；Word 是否容忍未证，属后端待修项",
            "Quick Look 不渲染段落级 w:shd 与左/上/右边框（见 docx.renderer_coverage），"
            "因此产物图不能用来判断这两项是否落地",
            "PDF 长表格/代码跨页、页眉页脚、目录与脚注",
            "krilla 字距调整、复杂脚本整形与两端对齐",
            "公式与 Mermaid 的静态图像边界（三格式本轮都只落源文本）",
            "HTML 生产渲染器（本轮只验证资源策略与语法夹具，未接生产后端）",
            "插件开关矩阵（PL-01/PL-02 依赖生产实现，见任务 4.x）",
        ],
    }
    write_json(output / "summary.json", summary)
    write_text(output / "matrix.md", render_matrix(summary))
    return summary


def render_matrix(summary):
    """把结果渲染成可读矩阵；数字与 summary.json 同源。"""
    lines = ["# V-02 候选后端矩阵（自动生成）", "",
             "本文件由 `scripts/probe_m4_export.py` 生成，数值与 `summary.json` 一致。", "",
             "## PDF 纸型 × 方向", "",
             "| 格 | 纸型 | 方向 | 期望 pt | 实测 pt | 偏差 pt | 字符数 |",
             "|---|---|---|---|---|---|---|"]
    for row in summary["pdf"]["cells"]:
        lines.append("| %s | %s | %s | %.2f×%.2f | %.2f×%.2f | %.3f×%.3f | %d |" % (
            row["label"], row["paper"], row["orientation"],
            row["expected_pt"][0], row["expected_pt"][1],
            row["actual_pt"][0], row["actual_pt"][1],
            row["delta_pt"][0], row["delta_pt"][1], row["chars"]))
    lines += ["", "## PDF 四边边距（墨迹不越界）", "",
              "| 格 | 请求 top/right/bottom/left pt | 实测 top/right/bottom/left pt |",
              "|---|---|---|"]
    for row in summary["pdf"]["margins"]:
        requested = row["requested_pt"]
        measured = row["measured_pt"]
        lines.append("| %s | %.2f/%.2f/%.2f/%.2f | %.2f/%.2f/%.2f/%.2f |" % (
            row["label"],
            requested["top"], requested["right"], requested["bottom"], requested["left"],
            measured["top"], measured["right"], measured["bottom"], measured["left"]))
    lines += ["", "## DOCX 样式组合", "",
              "| 标题/正文 | 代码 | 外观 | 标题字号(半磅) | 正文字号 | 代码底色(段落) | 代码文字色 | 对比度 | 编号项 | 表格 | pPr 顺序违规 | Quick Look |",
              "|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for row in summary["docx"]["combinations"]:
        table = row["table"]
        lines.append("| %s | %s | %s | %s | %s | %s | %s | %s | %d | %d×%d | %d | %s |" % (
            row["heading_body"], row["code_style"], row["appearance"],
            "/".join(str(value) for value in row["heading_sizes_half_pt"]),
            row["body_size_half_pt"], row["code_paragraph_shading"] or "无",
            row["code_color"] or "继承",
            "%.2f" % row["code_contrast"] if row["code_contrast"] else "—",
            row["numbered_paragraphs"], table["rows"], table["grid_cols"],
            row["ppr_order_violations"], row["quicklook_render"]))
    lines += ["", "> 代码底色落在段落属性（`w:pPr/w:shd`）：run 级底色只覆盖字形墨迹，"
              "行内空白与行尾不着色，Word 里是断带而不是代码块。", "",
              "> `pPr 顺序违规` 是 ECMA-376 `CT_PPr` sequence 的违规段数："
              "docx-rs 0.4.22 把 `w:rPr` 写在 `w:pStyle` 之前。探针如实计量、不重排；"
              "Word 是否容忍本机未证。", "",
              "### 渲染器覆盖（Quick Look，不经 docx-rs 的最小 DOCX）", "",
              "| 变体 | 是否渲染 | 与基线的像素差包围盒 |",
              "|---|---|---|"]
    for row in summary["docx"]["renderer_coverage"]:
        lines.append("| %s | %s | %s |" % (
            row["variant"], "是" if row["rendered"] else "否",
            "—" if not row["diff_bbox"] else ",".join(str(v) for v in row["diff_bbox"])))
    lines += ["", "> 结论只对 Quick Look 成立：它不画段落级底色与左/上/右边框，"
              "所以产物图里“看不到引用条、代码块无灰底”是渲染器覆盖问题，不是后端没写。"]
    dependencies = summary["dependencies"]
    lines += ["", "## HTML 资源策略 × 样式开关", "",
              "| 图片策略 | 样式 | 图片引用 | 远程资源 | 远程链接目标 | 离线可读 |",
              "|---|---|---|---|---|---|"]
    for row in summary["html"]["combinations"]:
        lines.append("| %s | %s | %s | %s | %s | %s |" % (
            row["policy"], "开" if row["style_enabled"] else "关",
            ", ".join(item["src"] for item in row["images"]),
            "无" if not row["remote_resources"] else ", ".join(row["remote_resources"]),
            "无" if not row["remote_link_targets"] else ", ".join(row["remote_link_targets"]),
            "是" if row["offline_readable"] else "否"))
    lines += ["", "> " + summary["html"]["backend"]]
    syntax = summary["syntax"]
    lines += ["", "## 语法 × 格式矩阵", "",
              "层级：`native` 目标格式有对应结构；`flattened` 变成排版结果；"
              "`source-text` 只落源文本；`absent` 该格式不承载。", "",
              "| 语法块 | PDF | DOCX | HTML | 边界说明 |",
              "|---|---|---|---|---|"]
    for row in syntax["matrix"]:
        lines.append("| %s | %s | %s | %s | %s |" % (
            row["label"], row["levels"]["pdf"], row["levels"]["docx"],
            row["levels"]["html"], row["note"]))
    lines += ["", "PDF 语法页：%d 页，%d 条文本标记全部在场，负标记缺席，内容未越界；产物 `artifacts/syntax.pdf`。"
              % (syntax["pdf"]["pages"], syntax["pdf"]["text_fragments"])]
    lines += ["", "## 候选依赖闭包", "",
              "按仓库自己的 `dependency_admission.license_review` 与"
              " `engineering_dependencies.forbidden_name` 逐包核对：",
              "",
              "- 包数：%d" % dependencies["packages"],
              "- 禁库命中：%s" % ("无" if not dependencies["forbidden"]
                                  else ", ".join(dependencies["forbidden"])),
              "- 策略：`%s`" % dependencies["policy"],
              "", dependencies["limits"]]
    lines += ["", "## 本轮未测", ""]
    lines += ["- " + item for item in summary["untested"]]
    return "\n".join(lines) + "\n"


# ------------------------------------------------------------------ 自测


def self_test():
    """不依赖 Rust/Swift 的规则与回读自测；任何断言失败即非零退出。"""
    cases = []

    def case(name, function):
        try:
            function()
        except Exception as error:  # noqa: BLE001 - 自测要报告任何失败
            cases.append({"case": name, "passed": False, "detail": str(error)})
        else:
            cases.append({"case": name, "passed": True, "detail": ""})

    def reject(call, needle):
        try:
            call()
        except (rules.ContractError, ProbeError) as error:
            if needle not in str(error):
                raise AssertionError("拒绝原因不符: %s" % error)
        else:
            raise AssertionError("应当拒绝但通过了")

    case("a4-portrait-size", lambda: _expect(
        rules.paper_size_pt("a4", "portrait"), (595.28, 841.89)))
    case("a3-landscape-swaps", lambda: _expect(
        rules.paper_size_pt("a3", "landscape"), (1190.55, 841.89)))
    case("letter-size", lambda: _expect(
        rules.paper_size_pt("letter", "portrait"), (612.0, 792.0)))
    case("legal-size", lambda: _expect(
        rules.paper_size_pt("legal", "portrait"), (612.0, 1008.0)))
    case("unknown-paper-rejected", lambda: reject(
        lambda: rules.paper_size_pt("b5", "portrait"), "未知纸型"))
    case("unknown-orientation-rejected", lambda: reject(
        lambda: rules.paper_size_pt("a4", "diagonal"), "未知方向"))
    case("margin-out-of-range-rejected", lambda: reject(
        lambda: rules.check_margins({"top": 20, "right": 15, "bottom": 25, "left": 101}),
        "边距超出"))
    case("margin-missing-side-rejected", lambda: reject(
        lambda: rules.check_margins({"top": 20, "right": 15, "bottom": 25}),
        "top/right/bottom/left"))
    case("margin-non-numeric-rejected", lambda: reject(
        lambda: rules.check_margins({"top": "20", "right": 15, "bottom": 25, "left": 30}),
        "必须是数值"))
    case("content-box-a4", lambda: _expect_close(
        rules.content_box_pt("a4", "portrait", {"top": 20, "right": 15, "bottom": 25, "left": 30}),
        (85.04, 70.87, 467.72, 714.33), 0.02))
    # 上界 100mm 与最小页面短边 210mm 之间仍留 10mm 内容区，因此
    # `content_box_pt` 的“内容区非正”分支在当前常量下不可达，是防御性守卫。
    # 这里验证边界仍为正且等于 10mm，而不是假装测到了那个分支。
    case("content-box-max-margins-still-positive", lambda: _expect_close(
        rules.content_box_pt("a4", "landscape",
                             {"top": 100, "right": 100, "bottom": 100, "left": 100}),
        (rules.mm_to_pt(100), rules.mm_to_pt(100),
         rules.mm_to_pt(297 - 200), rules.mm_to_pt(210 - 200)), 0.01))
    case("pagination-two-pages", lambda: _expect(
        rules.expected_page_count([200.0, 200.0, 200.0], 600.0, 60.0, 0.0), 2))
    case("pagination-single-block-too-tall", lambda: reject(
        lambda: rules.paginate([600.0], 600.0, 60.0), "高于整页"))
    case("pagination-zero-block-rejected", lambda: reject(
        lambda: rules.paginate([0.0], 600.0, 60.0), "块高必须为正"))
    case("heading-style-rejected", lambda: reject(
        lambda: rules.docx_style_plan("fancy", "theme"), "未知标题/正文样式"))
    case("code-style-rejected", lambda: reject(
        lambda: rules.docx_style_plan("document", "rainbow"), "未知代码样式"))
    case("html-policy-rejected", lambda: reject(
        lambda: rules.check_html_image_policy("inline-base64"), "未知 HTML 图片策略"))
    case("docx-plan-document-theme", lambda: _expect(
        {key: value for key, value in rules.docx_style_plan("document", "theme").items()
         if key != "code"},
        {"heading_body": "document", "code_style": "theme", "appearance": "light",
         "body_size_half_pt": 22, "heading_sizes_half_pt": (32, 28, 24)}))
    case("docx-plan-plain-unshaded", lambda: _expect(
        rules.docx_style_plan("standard", "plain")["code"],
        {"font": "Courier New", "shaded": False, "fill": None, "color": "auto"}))
    # 这条是“亮字亮底”缺陷的回归用例：dark 必须配深色底，且对比度达标。
    case("docx-dark-pairs-dark-fill", lambda: _expect(
        rules.docx_style_plan("document", "dark")["code"]["fill"], "1F2328"))
    case("docx-dark-contrast-readable", lambda: _expect(
        rules.docx_style_plan("document", "dark")["code"]["contrast"] >= rules.MIN_CODE_CONTRAST,
        True))
    case("docx-theme-follows-appearance", lambda: _expect(
        [rules.docx_style_plan("document", "theme", appearance)["code"]["fill"]
         for appearance in rules.APPEARANCES],
        [rules.CODE_PALETTE["light"]["fill"], rules.CODE_PALETTE["dark"]["fill"]]))
    case("docx-appearance-rejected", lambda: reject(
        lambda: rules.docx_style_plan("document", "theme", "sepia"), "未知外观"))
    case("contrast-white-on-white-rejected", lambda: _expect(
        rules.contrast_ratio("FFFFFF", "FFFFFF") < rules.MIN_CODE_CONTRAST, True))
    case("contrast-black-on-white-max", lambda: _expect(
        round(rules.contrast_ratio("000000", "FFFFFF"), 2), 21.0))
    case("contrast-bad-color-rejected", lambda: reject(
        lambda: rules.contrast_ratio("FFF", "000000"), "颜色必须是"))
    case("pdf-probe-parse", lambda: _expect(
        parse_pdf_probe("pages=1\npage.1.media=595.280,841.890\n"
                        "page.1.ink=85.040,70.870,467.720,714.330\n"
                        "page.1.chars=3\npage.1.text=a\\nb\npage.1.lines=1\n"
                        "page.1.line.0=1.0,2.0,3.0,4.0\n"),
        {"pages": 1, "page_data": [{"media": (595.28, 841.89), "chars": 3,
                                    "ink": (85.04, 70.87, 467.72, 714.33),
                                    "text": "a\nb", "line_count": 1,
                                    "lines": [(1.0, 2.0, 3.0, 4.0)]}]}))
    case("pdf-probe-ink-none", lambda: _expect(
        parse_pdf_probe("pages=1\npage.1.media=595.280,841.890\npage.1.ink=none\n"
                        "page.1.chars=0\npage.1.text=\npage.1.lines=0\n")["page_data"][0]["ink"],
        None))
    case("pdf-probe-open-fail", lambda: reject(
        lambda: parse_pdf_probe("OPEN_FAIL cannot open"), "PDF 无法打开"))
    case("pdf-probe-missing-pages", lambda: reject(
        lambda: parse_pdf_probe("page.1.media=1,1\n"), "缺少 pages="))
    case("pdf-cell-mismatch-rejected", lambda: reject(
        lambda: check_pdf_cells(pdf_cells()[:1], {
            "pages": 1, "page_data": [{"media": (595.28, 841.89), "chars": 0,
                                       "text": "", "lines": [], "line_count": 0}]}),
        "PDF 文本缺失"))
    case("pdf-margin-violation-rejected", lambda: reject(
        lambda: check_pdf_margins(
            {"label": "x", "paper": "a4", "orientation": "portrait",
             "margins": {"top": 20, "right": 15, "bottom": 25, "left": 30}},
            {"media": (595.28, 841.89), "ink": (0.0, 0.0, 595.28, 841.89), "text": "t"}),
        "边距与内容区不符"))
    case("pdf-margin-exact-accepted", lambda: _expect(
        check_pdf_margins(
            {"label": "x", "paper": "a4", "orientation": "portrait",
             "margins": {"top": 20, "right": 15, "bottom": 25, "left": 30}},
            {"media": (595.28, 841.89), "ink": (85.04, 70.87, 467.72, 714.33), "text": "t"},
        )["measured_pt"],
        {"left": 85.04, "bottom": 70.87, "right": 42.52, "top": 56.69}))
    case("pdf-no-ink-rejected", lambda: reject(
        lambda: check_pdf_margins(
            {"label": "x", "paper": "a4", "orientation": "portrait",
             "margins": {"top": 20, "right": 15, "bottom": 25, "left": 30}},
            {"media": (595.28, 841.89), "ink": None, "text": ""}),
        "没有任何墨迹"))
    case("docx-combinations-cover-matrix", lambda: _expect(
        len(docx_combinations()),
        len(rules.HEADING_BODY_STYLES)
        * (len(rules.CODE_STYLES) - 1 + len(rules.APPEARANCES))))

    # 语法矩阵：每个语法块在每种格式里都必须有明确落地层级，不允许留空。
    case("syntax-matrix-covers-formats", lambda: _expect(
        sorted(rules.check_syntax_matrix()), sorted(rules.SYNTAX_FORMATS)))
    case("syntax-matrix-unique-ids", lambda: _expect(
        len({row["id"] for row in rules.SYNTAX_MATRIX}), len(rules.SYNTAX_MATRIX)))
    # 两条典型落地：PDF 没有文档结构，标题只能靠字号区分；Word 有真标题样式。
    case("syntax-heading-pdf-flattened", lambda: _expect(
        rules.syntax_level("heading", "pdf"), "flattened"))
    case("syntax-heading-docx-native", lambda: _expect(
        rules.syntax_level("heading", "docx"), "native"))
    # 脚注是本轮最明确的缺口：krilla 无脚注概念，PDF 只落 [^note] 源标记，
    # 脚注正文不得出现（负断言见下方 syntax-*-forbidden-*）。
    case("syntax-footnote-pdf-source-text", lambda: _expect(
        rules.syntax_level("footnote", "pdf"), "source-text"))
    case("syntax-footnote-docx-native", lambda: _expect(
        rules.syntax_level("footnote", "docx"), "native"))
    case("syntax-unknown-kind-rejected", lambda: reject(
        lambda: rules.syntax_level("diagram", "pdf"), "未知语法块"))
    case("syntax-unknown-format-rejected", lambda: reject(
        lambda: rules.syntax_level("heading", "epub"), "未知格式"))
    # 样本由规则生成：磁盘上的 sample.md 与规则不一致即说明两份样本漂移了。
    case("syntax-sample-matches-rules", lambda: _expect(
        SAMPLE_MD.read_text(), rules.render_markdown()))
    # 矩阵标记必须真的出现在样本里，否则矩阵在描述不存在的内容。
    case("syntax-markers-present-in-sample", lambda: _expect(
        [text for texts in rules.SYNTAX_MARKERS.values() for text in texts
         if text not in rules.render_markdown()], []))
    # 负标记必须真的在样本里——它描述的是“源里有、产物里不该有”，
    # 源里都没有的话，负断言就是在验证一个不存在的东西。
    case("syntax-forbidden-present-in-sample", lambda: _expect(
        [text for formats in rules.SYNTAX_FORBIDDEN.values()
         for texts in formats.values() for text in texts
         if text not in rules.render_markdown()], []))
    case("syntax-pdf-has-pages", lambda: _expect(
        len(rules.syntax_pdf_document()["pages"]) >= 2, True))
    # 图片独占一页：这样回读时墨迹包围盒就等于图片矩形，可当几何事实比对。
    case("syntax-pdf-image-page-alone", lambda: _expect(
        [1 for page in rules.syntax_pdf_document()["pages"]
         if [op["op"] for op in page["ops"]] == ["image"]], [1]))
    case("syntax-pdf-heading-sizes-descend", lambda: _expect(
        rules.SYNTAX_HEADING_SIZES_PT[1] > rules.SYNTAX_HEADING_SIZES_PT[2]
        > rules.SYNTAX_HEADING_SIZES_PT[3] > rules.SYNTAX_BODY_SIZE_PT, True))
    # 链接必须带真实矩形，否则 PDF 里的注释覆盖不到文字。
    case("syntax-pdf-link-has-rect", lambda: _expect(
        [op for page in rules.syntax_pdf_document()["pages"] for op in page["ops"]
         if op["op"] == "link" and not (op["w"] > 0 and op["h"] > 0)],
        []))
    # 每个非 absent 的语法块都要在 PDF 版式里真的排进去，否则矩阵是空话。
    case("syntax-pdf-covers-blocks", lambda: _expect(
        sorted({op["block"] for page in rules.syntax_pdf_document()["pages"]
                for op in page["ops"]}),
        sorted(row["id"] for row in rules.SYNTAX_MATRIX
               if row["levels"]["pdf"] != "absent")))
    case("syntax-pdf-absent-footnote-not-laid", lambda: _expect(
        [op for page in rules.syntax_pdf_document()["pages"] for op in page["ops"]
         if "脚注内容" in op.get("text", "")], []))

    # 代码块底色必须落在**段落**属性上。run 级 w:rPr/w:shd 只覆盖字形墨迹，
    # 行内空白与行尾不着色，Word 里渲染成一条条断开的灰带——这是实际产物
    # 里看到的缺陷，不是样式偏好，因此契约要求段落级底色。
    case("docx-code-shading-is-paragraph-level", lambda: _expect(
        rules.docx_shading_scope(), "paragraph"))
    # ECMA-376 的 CT_PPr 是 sequence，子元素顺序错了 Word 会丢元素。
    # docx-rs 0.4.22 把 w:rPr 排在最前（默认段落属性就写 `<w:pPr><w:rPr /></w:pPr>`），
    # 这里冻结正确顺序，让回读能逐项核对产物。
    case("docx-ppr-order-puts-pstyle-first", lambda: _expect(
        rules.docx_ppr_order()[:2], ["pStyle", "keepNext"]))
    case("docx-ppr-order-puts-rpr-late", lambda: _expect(
        rules.docx_ppr_order()[-3:], ["rPr", "sectPr", "pPrChange"]))
    case("docx-ppr-order-check-accepts-sorted", lambda: _expect(
        rules.check_ppr_order(["pStyle", "pBdr", "ind", "rPr"]), True))
    case("docx-ppr-order-check-rejects-rpr-first", lambda: reject(
        lambda: rules.check_ppr_order(["rPr", "pStyle"]), "顺序不符"))
    # docx-rs 0.4.22 把 w:rPr 写在 w:pStyle 之前，违反 CT_PPr 的 sequence。
    # 探针只计量、不重排：重排会让“后端有这个缺陷”这一事实从证据里消失。
    case("docx-ppr-violations-counted", lambda: _expect(
        ppr_order_violations('<w:pPr><w:rPr /><w:pStyle w:val="Quote" /></w:pPr>'), 1))
    case("docx-ppr-violations-ignores-conforming", lambda: _expect(
        ppr_order_violations('<w:pPr><w:pStyle w:val="A" /><w:rPr /></w:pPr>'), 0))
    # 嵌套子元素内部也有 w:* 标签，只数直接子元素才不会被它们误判。
    case("docx-ppr-violations-sees-nested", lambda: _expect(
        ppr_order_violations(
            '<w:pPr><w:rPr /><w:numPr><w:ilvl w:val="0" /><w:numId w:val="2" /></w:numPr></w:pPr>'),
        1))
    case("docx-ppr-violations-unknown-child", lambda: _expect(
        ppr_order_violations('<w:pPr><w:bogus /></w:pPr>'), 1))

    # 渲染器覆盖：Quick Look 只画 run 级底色与下边框，不画段落级底色和
    # 左/上/右边框。这不是后端缺陷（手写最小 DOCX 同样如此），但必须成为
    # 可复跑的事实，否则看产物图的人会把“渲染器不画”误判成“后端没写”。
    case("renderer-docx-variants-differ", lambda: _expect(
        sorted(_MINIMAL_DOCX_VARIANTS), ["bottom-border", "left-border",
                                         "paragraph-shading", "plain", "run-shading"]))
    case("renderer-docx-plain-has-no-decoration", lambda: _expect(
        [token for token in ("w:shd", "w:pBdr") if token in _minimal_docx_xml("plain")], []))
    case("renderer-docx-paragraph-shading-is-paragraph-level", lambda: _expect(
        _minimal_docx_xml("paragraph-shading").count("<w:pPr><w:shd"), 1))
    case("renderer-docx-run-shading-is-run-level", lambda: _expect(
        "<w:rPr><w:shd" in _minimal_docx_xml("run-shading"), True))

    # HTML 策略：用临时目录里的最小样本，验证两条策略各自离线可读。
    def html_case(policy, style_enabled):
        def run_case():
            with tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                source = base / "src"
                source.mkdir()
                (source / "sample.md").write_text("# 标题\n\n正文\n\n![图](sample.png)\n")
                (source / "sample.png").write_bytes(b"\x89PNG\r\n\x1a\n")
                # 与真实运行同构：先在样本副本里就地导出，再回读。
                out_dir = base / "out"
                shutil.copytree(source, out_dir)
                path = render_html_export(out_dir, out_dir, policy, style_enabled)
                facts = check_html_export(path, policy, style_enabled)
                if facts["policy"] != policy or facts["style_enabled"] != style_enabled:
                    raise AssertionError("回读事实与策略不符: %r" % facts)
                packaged = (out_dir / "assets" / "sample.png").is_file()
                if packaged != (policy == "copy-to-dir"):
                    raise AssertionError("资源打包事实与策略不符: %s" % policy)
        return run_case

    case("html-copy-to-dir", html_case("copy-to-dir", True))
    case("html-keep-reference", html_case("keep-reference", True))
    case("html-style-off", html_case("keep-reference", False))
    case("html-remote-resource-rejected", lambda: _html_reject(
        '<img src="https://example.com/a.png">', "远程资源"))
    # 远程链接目标不破坏离线可读：`<a href>` 只有用户点击才联网。
    # 这条与上一条成对存在，防止把“文档里有网址”误判成“产物联网”。
    case("html-remote-link-target-allowed", lambda: _expect(
        _html_scan_flags('<p><a href="https://example.com">链接</a></p>'),
        {"remote_resources": [], "remote_link_targets": ["https://example.com"]}))
    case("html-remote-script-rejected", lambda: _html_reject(
        '<img src="sample.png"><script src="//cdn.example.com/a.js"></script>', "远程资源"))
    case("html-missing-image-rejected", lambda: _html_reject(
        '<img src="missing.png">', "无法解析"))
    case("html-no-image-rejected", lambda: _html_reject("<p>无图</p>", "未包含图片"))
    case("html-absolute-image-rejected", lambda: _html_reject(
        '<img src="file:///etc/passwd">', "非相对引用"))

    # 语法矩阵回读的失败路径：缺标签、缺文本、出现负标记都必须被拒绝。
    def html_syntax_case(markup, needle):
        def run_case():
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "sample.html"
                path.write_text("<!DOCTYPE html><html><body>%s</body></html>" % markup)
                try:
                    check_html_syntax(path)
                except ProbeError as error:
                    if needle not in str(error):
                        raise AssertionError("拒绝原因不符: %s" % error)
                else:
                    raise AssertionError("应当拒绝但通过了")
        return run_case

    full_html = "".join(
        "<%s></%s>" % (tag, tag) if tag not in ("img", "hr") else "<%s>" % tag
        for tag in rules.syntax_html_expectations()["tags"])
    full_html += "".join(rules.syntax_html_expectations()["must_contain"])
    case("html-syntax-accepts-complete", lambda: _expect(
        _html_syntax_ok(full_html)["forbidden_absent"], True))
    case("html-syntax-missing-tag-rejected", html_syntax_case(
        full_html.replace("<table></table>", ""), "缺少语法标签"))
    case("html-syntax-missing-text-rejected", html_syntax_case(
        full_html.replace("第一项 first item", ""), "缺少语法文本"))
    case("html-syntax-forbidden-present-rejected", html_syntax_case(
        full_html + "脚注内容 footnote body", "不该承载"))

    # 语法页回读：页数与负标记。
    def pdf_syntax_case(text, needle):
        def run_case():
            reading = parse_pdf_probe(text)
            try:
                check_pdf_syntax(reading)
            except ProbeError as error:
                if needle not in str(error):
                    raise AssertionError("拒绝原因不符: %s" % error)
            else:
                raise AssertionError("应当拒绝但通过了")
        return run_case

    expected_pages = rules.syntax_pdf_expected()["pages"]
    good_lines = ["pages=%d" % expected_pages]
    for index in range(1, expected_pages + 1):
        good_lines += ["page.%d.media=595.280,841.890" % index,
                       "page.%d.ink=none" % index, "page.%d.chars=1" % index,
                       "page.%d.lines=0" % index]
    # 把所有标记塞进首页文本，模拟“全部在场”。
    good_lines.append("page.1.text=" + "\\n".join(
        rules.syntax_pdf_expected()["must_contain"]).replace("\\", "\\\\"))
    case("pdf-syntax-accepts-complete", lambda: _expect(
        check_pdf_syntax(parse_pdf_probe("\n".join(good_lines) + "\n"))["forbidden_absent"],
        True))
    case("pdf-syntax-page-count-rejected", pdf_syntax_case(
        "pages=1\npage.1.media=595.280,841.890\npage.1.ink=none\npage.1.chars=0\n"
        "page.1.text=\npage.1.lines=0\n", "页数不符"))
    case("pdf-syntax-missing-text-rejected", pdf_syntax_case(
        "\n".join(line for line in good_lines if not line.startswith("page.1.text"))
        + "\npage.1.text=无关内容\n", "缺少语法文本"))
    # 负标记：把所有必须出现的文本都放进去，再额外塞入脚注正文，必须被拒绝。
    case("pdf-syntax-forbidden-present-rejected", pdf_syntax_case(
        "\n".join(line for line in good_lines if not line.startswith("page.1.text"))
        + "\npage.1.text=" + "\\n".join(
            rules.syntax_pdf_expected()["must_contain"]
            + rules.syntax_pdf_expected()["must_not_contain"]).replace("\\", "\\\\") + "\n",
        "不该承载"))

    passed = all(row["passed"] for row in cases)
    return {"passed": passed, "cases": cases}


def _html_syntax_ok(markup):
    """语法回读的通过路径：把完整标记写进临时 HTML 后必须通过。"""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "sample.html"
        path.write_text("<!DOCTYPE html><html><body>%s</body></html>" % markup)
        return check_html_syntax(path)


def _html_scan_flags(markup):
    """只看扫描器的分类结果，不触发策略检查。"""
    scan = _HtmlScan()
    scan.feed(markup)
    return {"remote_resources": scan.remote_resources,
            "remote_link_targets": scan.remote_targets}


def _html_reject(markup, needle):
    """HTML 回读的失败路径：把畸形产物写进临时目录后必须被拒绝。"""
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        path = base / "sample.html"
        path.write_text("<!DOCTYPE html><html><body>%s</body></html>" % markup)
        try:
            check_html_export(path, "keep-reference", False)
        except ProbeError as error:
            if needle not in str(error):
                raise AssertionError("拒绝原因不符: %s" % error)
        else:
            raise AssertionError("应当拒绝但通过了")


def _expect(actual, expected):
    if actual != expected:
        raise AssertionError("期望 %r，实际 %r" % (expected, actual))


def _expect_close(actual, expected, tolerance):
    if len(actual) != len(expected):
        raise AssertionError("长度不符: %r != %r" % (actual, expected))
    for left, right in zip(actual, expected):
        if abs(left - right) > tolerance:
            raise AssertionError("期望 %r±%s，实际 %r" % (expected, tolerance, actual))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true",
                        help="只运行规则与回读自测，不编译 Rust 或调用 Swift")
    parser.add_argument("--output-dir", help="证据落点，必须在 logs/validation/ 内")
    args = parser.parse_args(argv)

    if args.self_test:
        result = self_test()
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if result["passed"] else 1

    output = output_directory("probe-m4-export", args.output_dir)
    try:
        summary = real_run(output)
    except (ProbeError, rules.ContractError, Violation) as error:
        write_text(output / "failure.log", str(error))
        print("PROBE_FAILED " + str(error), file=sys.stderr)
        return 1
    print("PROBE_OK pages=%d docx=%d output=%s"
          % (summary["pdf"]["pages"], len(summary["docx"]["combinations"]),
             output.relative_to(ROOT)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
