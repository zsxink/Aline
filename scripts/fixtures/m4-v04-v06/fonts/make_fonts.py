#!/usr/bin/env python3
"""生成 V-04 字体夹具：自制的极小静态 TTF，家族名与系统字体不冲突。

为什么自制而不是下载第三方字体：验证需要“包内字体”的家族名、字重和缺字
行为完全已知，第三方字体不可控也不便入库。生成后只提交产物；本脚本记录
生成方式，运行夹具不需要 fontTools。

用法（仅重建夹具时）：

    python3 -I make_fonts.py

依赖 fontTools 4.60.2（开发机可用，不进入任何门禁）。
"""

import pathlib

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

UNITS_PER_EM = 1000


def outline(advance, contours):
    """按 (起点, 线段) 描述构造一个简单轮廓。"""
    pen = TTGlyphPen(None)
    for contour in contours:
        start, segments = contour
        pen.moveTo(start)
        for command in segments:
            if command[0] == "line":
                pen.lineTo(command[1])
            else:
                pen.qCurveTo(*command[1])
        pen.closePath()
    return pen.glyph(), advance


def build(path, family, style, weight, glyph_chars, advance=600):
    """构建一个只含少量字形的静态 TTF。

    `glyph_chars` 决定 cmap；不含 'm' 的变体用于复现锁定 GPUI `load_family`
    的静默跳过（缺 'm' 字形的字体注册成功但永不激活）。`advance` 可调，便于
    生成同家族但度量不同的版本，用来区分“解析到本字族”和“沿回退栈落到系统
    字体”——两者在家族名上可能相同，只有 advance 能区分。
    """
    glyph_order = [".notdef", "space"] + ["uni%04X" % ord(c) for c in glyph_chars]
    glyphs = {}
    metrics = {}

    glyphs[".notdef"], metrics[".notdef"] = outline(advance, [])
    glyphs["space"], metrics["space"] = outline(advance, [])
    for char in glyph_chars:
        name = "uni%04X" % ord(char)
        # 一个 500x700 的方框，足以产生非零 advance 与可见墨迹。
        contours = [((100, 0), [("line", (100, 700)), ("line", (500, 700)),
                                ("line", (500, 0))])]
        glyphs[name], metrics[name] = outline(advance, contours)

    builder = FontBuilder(UNITS_PER_EM, isTTF=True)
    builder.setupGlyphOrder(glyph_order)
    builder.setupCharacterMap({ord(c): "uni%04X" % ord(c) for c in glyph_chars})
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({name: (advance, 0) for name in glyph_order})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({
        "familyName": family,
        "styleName": style,
        "uniqueFontIdentifier": "%s %s" % (family, style),
        "fullName": "%s %s" % (family, style),
        "psName": "%s-%s" % (family.replace(" ", ""), style.replace(" ", "")),
        "version": "Version 1.000",
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800,
                     usWinDescent=200, usWeightClass=weight)
    builder.setupPost()
    builder.setupMaxp()
    builder.setupHead()
    builder.save(str(path))


def main():
    root = pathlib.Path(__file__).resolve().parent
    # 含 'm'：可被 load_family 激活。
    build(root / "ProbeSans-Regular.ttf", "Aline Probe Sans", "Regular", 400, "mxi")
    # 同家族不同字重：用于观测 GPUI 是否拒绝同族多 face。
    build(root / "ProbeSans-Bold.ttf", "Aline Probe Sans", "Bold", 700, "mxi")
    # 不含 'm'：注册成功但 load_family 静默跳过。
    build(root / "ProbeNoM-Regular.ttf", "Aline Probe NoM", "Regular", 400, "xi")
    # 同家族、不同 advance：验证“已解析过的字族再注册”是否真的改变度量。
    build(root / "ProbeSansAlt-Regular.ttf", "Aline Probe Sans", "Regular", 400,
          "mxi", advance=700)
    # 抢占保留通用角色名：规范禁止包内 family 占用 system-ui/sans-serif/
    # monospace，但锁定 GPUI 不知道这层契约，实测会被注册成功并参与解析。
    build(root / "HijackMonospace.ttf", "monospace", "Regular", 400, "mxi")
    # 抢占系统族名：锁定 GPUI 的内存源优先于系统源，同名系统族会被遮蔽。
    build(root / "HijackHelvetica.ttf", "Helvetica", "Regular", 400, "mxi")
    (root / "ProbeSans-OFL.txt").write_text(
        "Aline Probe Sans — 自制测试字体，无第三方版权。\n"
        "仅用于 Aline 验证夹具，不得作为产品字体分发。\n", encoding="utf-8")
    (root / "corrupt.ttf").write_bytes(
        (root / "ProbeSans-Regular.ttf").read_bytes()[:512])
    (root / "empty.ttf").write_bytes(b"")


if __name__ == "__main__":
    main()
