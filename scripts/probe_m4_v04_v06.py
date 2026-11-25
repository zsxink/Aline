#!/usr/bin/env python3
"""V-04/V-06 实测探针：用合法与越界样本冻结主题包预算、默认配置与 GPUI 能力缺口。

用法：

    python3 scripts/probe_m4_v04_v06.py --self-test
    python3 scripts/probe_m4_v04_v06.py --output-dir logs/validation/...

`--self-test` 只校验规则层与比对逻辑，不编译 Rust、不起应用，因此在没有
GPUI 缓存、没有 GUI 会话的环境里也能运行。

真实运行会：

1. 把三个探针 crate 复制到临时目录，离线构建（不写工作区 target）。
2. 生成合法与越界主题包：schema 1 合法包、schema 3 原型包、路径穿越、
   符号链接、重复/规范化碰撞、APNG/SVG/JPEG/WebP/截断预览、超像素预览、
   超字体数/总字节、声明撒谎的解压炸弹、ZIP64、超条目数、超包体。
3. 用 `zip` 8.6.0 探针量目录预检、限流读取、受限解压的实际行为。
4. 用 GPUI 探针量图片解码/字体注册/字体激活/内容渲染的时间与位图占用。
5. 逐项与 `scripts/m4_v04_v06_rules.py` 的契约比对，输出预算冻结表与
   能力缺口清单。

证据只写 `logs/validation/` 内；脚本不改工作区依赖、不下载任何东西。
"""

import argparse
import io
import json
import os
from pathlib import Path
import platform
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import warnings
import zipfile
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parent))

import m4_v04_v06_rules as rules  # noqa: E402
from evidence_paths import display_path, resolve_path, write_json, write_text  # noqa: E402
from validation_output import ROOT, output_directory  # noqa: E402

FIXTURES = ROOT / "scripts/fixtures/m4-v04-v06"
PROBE_ZIP = FIXTURES / "probe-zip"
PROBE_LIMITS = FIXTURES / "probe-limits"
PROBE_CONTENT = FIXTURES / "probe-content"
FONT_DIR = FIXTURES / "fonts"

ZIP_BINARY = "m4-v04-zip"
LIMITS_BINARY = "m4-v04-limits"
CONTENT_BINARY = "m4-v04-content-limits"

# 探针构建时把夹具里指向 vendor 的相对 patch 改写成绝对路径；这条字符串
# 必须与三个 Cargo.toml 的写法逐字一致。
PATCH_LINE = 'path = "../../../../vendor/gpui-pre-0.3.9"'


class ProbeError(RuntimeError):
    """探针失败；调用方据此非零退出并保留证据。"""


# ------------------------------------------------------------------ PNG 生成
#
# 预览样本必须是**真实可解码**的 PNG，否则“格式合格”与“能解码”会混在一起。
# 只用标准库 zlib 手写最小 PNG，避免依赖 PIL（门禁环境不保证有）。


def png_bytes(width, height, pixel=(0, 0, 0, 255), level=6):
    """生成 RGBA 纯色 PNG；`level` 控制压缩率，用于构造不可压缩样本。"""
    raw = b"".join(
        b"\x00" + bytes(pixel) * width for _ in range(height)
    )
    return _png_container(width, height, zlib.compress(raw, level))


def _png_container(width, height, compressed):
    def chunk(tag, payload):
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", compressed) + chunk(b"IEND", b""))


def apng_bytes(width, height):
    """带 acTL 的最小 APNG：两帧声明、单帧数据（足以触发格式拒绝）。"""
    def chunk(tag, payload):
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))

    raw = b"".join(b"\x00" + bytes((0, 255, 0, 255)) * width for _ in range(height))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    actl = struct.pack(">II", 2, 0)
    seq = struct.pack(">IHHH", 0, width, height, 0) + b"\x00\x00"
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"acTL", actl)
            + chunk(b"fcTL", seq) + chunk(b"IDAT", zlib.compress(raw))
            + chunk(b"IEND", b""))


def jpeg_bytes(width, height):
    """最小基线 JPEG 头 + 伪造扫描段；只用于签名判定，不要求可解码。"""
    sof = (b"\xff\xc0" + struct.pack(">HBHHB", 11, 8, height, width, 1)
           + b"\x01\x11\x00")
    return (b"\xff\xd8" + b"\xff\xe0" + struct.pack(">H", 16) + b"JFIF\x00\x01\x01\x00"
            + b"\x00\x01\x00\x01\x00\x00" + sof + b"\xff\xd9")


SVG_PREVIEW = (b'<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">'
               b'<rect width="8" height="8" fill="#ff0000"/></svg>')

WEBP_PREVIEW = (b"RIFF" + struct.pack("<I", 12) + b"WEBPVP8 " + struct.pack("<I", 0))


# ------------------------------------------------------------------ 主题包


def schema1_manifest(package_id="probe-paper", version="1.0.0"):
    return {
        "schemaVersion": 1,
        "id": package_id,
        "name": "Probe Paper",
        "description": "验证用最小主题包。",
        "author": "Aline 验证",
        "version": version,
        "inherits": "github",
        "files": dict(rules.MANIFEST_FILE_KEYS),
        "previews": {"light": "previews/light.png",
                     "dark": "previews/dark.png",
                     "sepia": "previews/sepia.png"},
        "fonts": [],
    }


def schema1_documents():
    """三配置文件的合法内容：只写覆盖，不复制基线。

    markdown 写 common 与 dark 两个层，用来同时验证“缺模式仍合法”和
    “数组整体替换”；highlight/application 各写一个字段，保证三个文件都被解析。
    """
    return {
        "markdown.json": {
            "common": {
                "spacing": {"paragraphAfter": 18},
                "typography": {"codeFamilies": ["Probe Mono", "monospace"]},
            },
            "modes": {
                "dark": {"colors": {"body": "#DDDDDD"}},
            },
        },
        "highlight.json": {
            "modes": {"light": {"syntax": {"keyword": "#AA0000"}}},
        },
        "application.json": {
            "common": {"window": {"backgroundOpacity": 1.0}},
        },
    }


def build_package(entries):
    """把 `(name, bytes)` 序列写成 ZIP（deflate）；返回字节。

    `duplicate-name` 样本故意重名，zipfile 会就此发 UserWarning；这里局部静音，
    以免正常输出被预期内的警告污染。
    """
    buffer = io.BytesIO()
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", UserWarning)
        with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, data in entries:
                archive.writestr(name, data)
    return buffer.getvalue()


def legal_entries(root_prefix="", with_fonts=False):
    """合法包条目：四 JSON + 三预览（+ 可选字体与授权）。"""
    manifest = schema1_manifest()
    entries = [(root_prefix + "theme.json",
                json.dumps(manifest, ensure_ascii=False).encode())]
    for name, document in schema1_documents().items():
        entries.append((root_prefix + name, json.dumps(document).encode()))
    preview = png_bytes(64, 64, (255, 255, 255, 255))
    for mode in rules.MANIFEST_PREVIEW_MODES:
        entries.append((root_prefix + "previews/%s.png" % mode, preview))
    if with_fonts:
        entries.append((root_prefix + "fonts/ProbeSans-Regular.ttf",
                        (FONT_DIR / "ProbeSans-Regular.ttf").read_bytes()))
        entries.append((root_prefix + "licenses/ProbeSans-OFL.txt",
                        (FONT_DIR / "ProbeSans-OFL.txt").read_bytes()))
    return entries


def legal_font_manifest():
    manifest = schema1_manifest("probe-font")
    manifest["fonts"] = [{
        "path": "fonts/ProbeSans-Regular.ttf",
        "family": "Aline Probe Sans",
        "license": "OFL-1.1",
        "licenseFile": "licenses/ProbeSans-OFL.txt",
    }]
    return manifest


def package_cases(work):
    """生成全部包样本，返回 `[(case, path, expectation)]`。

    `expectation` 是契约期望：`accept` 或拒绝原因类别。探针不按“库是否报错”
    记通过，而是按本层契约判定，库的行为差异作为能力缺口单列。
    """
    cases = []
    legal = work / "legal.zip"
    legal.write_bytes(build_package(legal_entries()))
    cases.append(("legal", legal, "accept"))

    nested = work / "legal-nested.zip"
    nested.write_bytes(build_package(legal_entries("probe-paper/")))
    cases.append(("legal-nested-root", nested, "accept"))

    with_font = work / "legal-font.zip"
    entries = legal_entries(with_fonts=True)
    entries[0] = ("theme.json", json.dumps(legal_font_manifest()).encode())
    with_font.write_bytes(build_package(entries))
    cases.append(("legal-font", with_font, "accept"))

    prototype = work / "schema3.zip"
    entries = legal_entries()
    manifest = schema1_manifest()
    manifest["schemaVersion"] = 3
    entries[0] = ("theme.json", json.dumps(manifest).encode())
    prototype.write_bytes(build_package(entries))
    cases.append(("schema3-reject", prototype, "reject:schemaVersion"))

    traversal = work / "traversal.zip"
    entries = legal_entries() + [("../evil.json", b"{}")]
    traversal.write_bytes(build_package(entries))
    cases.append(("path-traversal", traversal, "reject:path"))

    absolute = work / "absolute.zip"
    absolute.write_bytes(build_package(legal_entries() + [("/abs.json", b"{}")]))
    cases.append(("path-absolute", absolute, "reject:path"))

    backslash = work / "backslash.zip"
    backslash.write_bytes(build_package(legal_entries() + [("fonts\\a.ttf", b"x")]))
    cases.append(("path-backslash", backslash, "reject:path"))

    dot_segment = work / "dot-segment.zip"
    dot_segment.write_bytes(build_package(legal_entries() + [("a/./b.json", b"{}")]))
    cases.append(("path-dot-segment", dot_segment, "reject:path"))

    double_slash = work / "double-slash.zip"
    double_slash.write_bytes(build_package(legal_entries() + [("a//b.json", b"{}")]))
    cases.append(("path-empty-segment", double_slash, "reject:path"))

    trailing = work / "trailing-space.zip"
    trailing.write_bytes(build_package(legal_entries() + [("a /b.json", b"{}")]))
    cases.append(("path-trailing-space", trailing, "reject:path"))

    device = work / "device-name.zip"
    device.write_bytes(build_package(legal_entries() + [("fonts/CON.ttf", b"x")]))
    cases.append(("path-device-name", device, "reject:path"))

    ads = work / "ads.zip"
    ads.write_bytes(build_package(legal_entries() + [("fonts/a.ttf:stream", b"x")]))
    cases.append(("path-ads", ads, "reject:path"))

    duplicate = work / "duplicate.zip"
    duplicate.write_bytes(build_package(legal_entries() + [
        ("theme.json", json.dumps(schema1_manifest("other")).encode())]))
    cases.append(("duplicate-name", duplicate, "reject:duplicate"))

    nfd = unicodedata_nfd("café.png")
    nfc = unicodedata_nfc("café.png")
    normalization = work / "normalization.zip"
    normalization.write_bytes(build_package(legal_entries() + [
        ("previews/" + nfc, b"x"), ("previews/" + nfd, b"x")]))
    cases.append(("nfc-nfd-collision", normalization, "reject:collision"))

    symlink = work / "symlink.zip"
    symlink.write_bytes(_symlink_zip(legal_entries(), "fonts/link.ttf", "../theme.json"))
    cases.append(("symlink", symlink, "reject:symlink"))

    apng = work / "preview-apng.zip"
    apng.write_bytes(build_package(_replace_preview(legal_entries(),
                                                    "previews/light.png",
                                                    apng_bytes(64, 64))))
    cases.append(("preview-apng", apng, "reject:preview-format"))

    svg = work / "preview-svg.zip"
    svg.write_bytes(build_package(_replace_preview(legal_entries(),
                                                  "previews/light.png", SVG_PREVIEW)))
    cases.append(("preview-svg", svg, "reject:preview-format"))

    jpeg = work / "preview-jpeg.zip"
    jpeg.write_bytes(build_package(_replace_preview(legal_entries(),
                                                    "previews/light.png",
                                                    jpeg_bytes(64, 64))))
    cases.append(("preview-jpeg", jpeg, "reject:preview-format"))

    webp = work / "preview-webp.zip"
    webp.write_bytes(build_package(_replace_preview(legal_entries(),
                                                    "previews/light.png",
                                                    WEBP_PREVIEW)))
    cases.append(("preview-webp", webp, "reject:preview-format"))

    truncated = work / "preview-truncated.zip"
    whole = png_bytes(64, 64)
    truncated.write_bytes(build_package(_replace_preview(legal_entries(),
                                                         "previews/light.png",
                                                         whole[:len(whole) // 2])))
    cases.append(("preview-truncated", truncated, "reject:preview-decode"))

    big_pixels = work / "preview-pixels.zip"
    side = 2048
    big_pixels.write_bytes(build_package(_replace_preview(
        legal_entries(), "previews/light.png", png_bytes(side + 1, side))))
    cases.append(("preview-over-pixels", big_pixels, "reject:preview-pixels"))

    boundary_pixels = work / "preview-pixels-boundary.zip"
    boundary_pixels.write_bytes(build_package(_replace_preview(
        legal_entries(), "previews/light.png", png_bytes(2048, 2048))))
    cases.append(("preview-boundary-pixels", boundary_pixels, "accept"))

    many_fonts = work / "fonts-over-count.zip"
    manifest = schema1_manifest("probe-many")
    manifest["fonts"] = [{
        "path": "fonts/f%02d.ttf" % index, "family": "Probe %02d" % index,
        "license": "OFL-1.1", "licenseFile": "licenses/ProbeSans-OFL.txt",
    } for index in range(rules.budget("font_count") + 1)]
    entries = legal_entries()
    entries[0] = ("theme.json", json.dumps(manifest).encode())
    entries.append(("licenses/ProbeSans-OFL.txt",
                    (FONT_DIR / "ProbeSans-OFL.txt").read_bytes()))
    for index in range(rules.budget("font_count") + 1):
        entries.append(("fonts/f%02d.ttf" % index,
                        (FONT_DIR / "ProbeSans-Regular.ttf").read_bytes()))
    many_fonts.write_bytes(build_package(entries))
    cases.append(("fonts-over-count", many_fonts, "reject:font-count"))

    bomb = work / "bomb.zip"
    bomb.write_bytes(_bomb_zip())
    cases.append(("decompression-bomb", bomb, "reject:ratio"))

    lying = work / "lying-size.zip"
    lying.write_bytes(_lying_zip())
    cases.append(("lying-declared-size", lying, "reject:extraction-total"))

    zip64 = work / "zip64.zip"
    zip64.write_bytes(_zip64_package())
    cases.append(("zip64", zip64, "accept"))

    entries_over = work / "entries-over.zip"
    entries_over.write_bytes(_many_entries_zip(rules.budget("entry_count") + 1))
    cases.append(("entries-over-count", entries_over, "reject:entry-count"))

    not_zip = work / "not-zip.pdf"
    not_zip.write_bytes(b"%PDF-1.7\n% valid UTF-8 markdown that starts with PK\n")
    cases.append(("not-zip", not_zip, "reject:not-zip"))

    return cases


def unicodedata_nfc(text):
    import unicodedata
    return unicodedata.normalize("NFC", text)


def unicodedata_nfd(text):
    import unicodedata
    return unicodedata.normalize("NFD", text)


def _replace_preview(entries, name, data):
    return [(entry, data if entry == name else payload) for entry, payload in entries]


def _symlink_zip(entries, link_name, target):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in entries:
            archive.writestr(name, data)
        info = zipfile.ZipInfo(link_name)
        info.create_system = 3
        info.external_attr = (0o120777 << 16)
        archive.writestr(info, target)
    return buffer.getvalue()


def _bomb_zip():
    """64 MiB 全零载荷压到约 64 KiB：压缩比约 1000:1。"""
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        archive.writestr("fonts/zeros.ttf", b"\0" * (64 * 1024 * 1024))
    return buffer.getvalue()


def _lying_zip():
    """声明 8 KiB、实际解压 32 MiB：中央目录的 size 字段被改小。"""
    payload = bytes(range(256)) * (128 * 1024)
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("fonts/liar.ttf", payload)
    data = bytearray(buffer.getvalue())
    # 定位本地文件头与中央目录中的 uncompressed size 并改写为 8192。
    lie = struct.pack("<I", 8192)
    data[22:26] = lie          # 本地文件头：偏移 18 是压缩大小，22 是原始大小
    central = data.find(b"PK\x01\x02")
    if central >= 0:
        data[central + 24:central + 28] = lie
    return bytes(data)


def _zip64_package():
    """强制 ZIP64 扩展信息：合法包，用于确认库能读 ZIP64 而不误拒。"""
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in legal_entries():
            info = zipfile.ZipInfo(name)
            with archive.open(info, "w", force_zip64=True) as handle:
                handle.write(data)
    return buffer.getvalue()


def _many_entries_zip(count):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_STORED) as archive:
        for index in range(count):
            archive.writestr("f%05d.bin" % index, b"x")
    return buffer.getvalue()


# ------------------------------------------------------------------ 期望判定


def path_case_verdict(name):
    """把包样本名映射到本层契约判定；未登记的样本视为契约缺陷。"""
    table = {
        "legal": ("accept", []),
        "legal-nested-root": ("accept", []),
        "legal-font": ("accept", []),
        "preview-boundary-pixels": ("accept", []),
        "zip64": ("accept", []),
        "schema3-reject": ("reject", ["schemaVersion"]),
        "path-traversal": ("reject", ["相对段 .."]),
        "path-absolute": ("reject", ["POSIX 根路径"]),
        "path-backslash": ("reject", ["含反斜杠"]),
        "path-dot-segment": ("reject", ["相对段 ."]),
        "path-empty-segment": ("reject", ["空段"]),
        "path-trailing-space": ("reject", ["以空格或点结尾"]),
        "path-device-name": ("reject", ["平台保留设备名"]),
        "path-ads": ("reject", ["ADS 冒号"]),
        "duplicate-name": ("reject", ["重复"]),
        "nfc-nfd-collision": ("reject", ["碰撞"]),
        "symlink": ("reject", ["符号链接"]),
        "preview-apng": ("reject", ["APNG"]),
        "preview-svg": ("reject", ["PNG"]),
        "preview-jpeg": ("reject", ["PNG"]),
        "preview-webp": ("reject", ["PNG"]),
        "preview-truncated": ("reject", ["截断"]),
        "preview-over-pixels": ("reject", ["像素"]),
        "fonts-over-count": ("reject", ["字体"]),
        "decompression-bomb": ("reject", ["压缩比", "ratio"]),
        "lying-declared-size": ("reject", ["声明"]),
        "entries-over-count": ("reject", ["条目"]),
        "not-zip": ("reject", ["EOCD", "ZIP"]),
    }
    if name not in table:
        raise rules.ContractError("包样本 %s 未登记契约判定" % name)
    return table[name]


def _zip_magic(path):
    """ZIP 本地文件头或空归档结束块；不靠库报错来判“是不是 ZIP”。"""
    try:
        with open(path, "rb") as handle:
            head = handle.read(4)
    except OSError:
        return False
    return head in (b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08")


def audit_package(listing, stream=None):
    """对一个包跑纯规则层的全部检查，返回拒绝原因列表。

    这是探针的核心：不依赖 `zip` 库的判断，用条目名、声明大小与**实际读取
    字节**独立得出结论，再与库的实际行为对照。`stream` 是 `zip read` 的逐条
    结果；声明大小与实际读取不一致本身就是拒绝理由（中央目录可以撒谎）。
    """
    reasons = []
    if listing is None or not listing.get("ok"):
        return ["库无法读取该归档"]
    entries = listing.get("entries", [])
    names = []
    typed = []
    for entry in entries:
        name = entry.get("name", "")
        names.append(name)
        typed.append((name, bool(entry.get("is_dir"))))
        if entry.get("is_symlink"):
            reasons.append("%s: 符号链接条目" % name)
        for violation in rules.zip_entry_violations(name, bool(entry.get("is_dir"))):
            reasons.append("%s: %s" % (name, violation))
    declared = listing.get("declared_entry_count")
    actual = listing.get("entry_count")
    if declared is not None and actual is not None and declared != actual:
        reasons.append("重复条目：中央目录声明 %d 条，库暴露 %d 条"
                       % (declared, actual))
    unique = listing.get("unique_name_count")
    if unique is not None and actual is not None and unique < actual:
        reasons.append("重复条目：%d 条条目只有 %d 个唯一名" % (actual, unique))
    for group in rules.collision_pairs([name.rstrip("/") for name in names]):
        reasons.append("规范化/大小写碰撞: %s" % (group,))
    for first, second in rules.ancestor_conflicts(typed):
        reasons.append("祖先冲突: %s / %s" % (first, second))
    if listing.get("has_overlapping_files"):
        reasons.append("条目数据区重叠")
    if rules.over_limit("entry_count", actual or 0):
        reasons.append("超过条目上限 %d" % rules.budget("entry_count"))
    declared_total = listing.get("declared_total") or 0
    compressed_total = listing.get("compressed_total") or 0
    if compressed_total > 0 and declared_total / compressed_total > rules.budget(
            "compression_ratio"):
        reasons.append("压缩比 %.1f:1 超过上限"
                       % (declared_total / compressed_total))
    if rules.over_limit("extraction_total", declared_total):
        reasons.append("解压总量 %d 超过上限 %d"
                       % (declared_total, rules.budget("extraction_total")))
    for entry in entries:
        if rules.over_limit("extraction_single_file", entry.get("declared_size") or 0):
            reasons.append("%s: 单文件超过上限" % entry.get("name"))
    for row in stream or []:
        if row.get("error") is not None:
            reasons.append("%s: 读取失败 %s" % (row.get("name"), row["error"]))
            continue
        declared = row.get("declared_size")
        read = row.get("read_bytes")
        if declared is not None and read is not None and declared != read:
            reasons.append("%s: 实际读取 %d 字节与声明 %d 字节不符"
                           % (row.get("name"), read, declared))
    return reasons


def audit_manifest_bytes(data):
    """解析 theme.json 并跑 JSON 前置检查与清单契约；返回 `(manifest, reasons)`。"""
    manifest, reasons = rules.json_text_violations(data)
    if manifest is None:
        return None, reasons
    reasons.extend(rules.manifest_violations(manifest))
    return manifest, reasons


def audit_previews(manifest, entries_by_name):
    reasons = []
    previews = (manifest or {}).get("previews") or {}
    total = 0
    for mode in rules.MANIFEST_PREVIEW_MODES:
        name = previews.get(mode)
        if not isinstance(name, str) or name not in entries_by_name:
            continue
        data = entries_by_name[name]
        kind, width, height, animated, error = rules.preview_verdict(data)
        total += len(data)
        if error:
            reasons.append("%s: %s" % (name, error))
            continue
        if width is not None:
            reasons.extend("%s: %s" % (name, item)
                           for item in rules.preview_within_budget(
                               width, height, len(data)))
    if rules.over_limit("preview_total_bytes", total):
        reasons.append("预览总量 %d 超过上限" % total)
    return reasons


def audit_fonts(manifest, entries_by_name):
    reasons = []
    total = 0
    for entry in (manifest or {}).get("fonts") or []:
        if not isinstance(entry, dict):
            continue
        name = entry.get("path")
        if not isinstance(name, str):
            continue
        data = entries_by_name.get(name)
        if data is None:
            reasons.append("%s: 字体资源不存在" % name)
            continue
        total += len(data)
        if rules.over_limit("font_single_bytes", len(data)):
            reasons.append("%s: 超过单字体上限" % name)
        extension = name.rsplit(".", 1)[-1]
        kind, error = rules.font_verdict(data, extension)
        if error:
            reasons.append("%s: %s" % (name, error))
    if rules.over_limit("font_total_bytes", total):
        reasons.append("字体总量 %d 超过上限" % total)
    return reasons


# ------------------------------------------------------------------ 运行探针


def run(argv, cwd=None, env=None, timeout=1800):
    completed = subprocess.run(argv, cwd=cwd, env=env, text=True,
                               capture_output=True, timeout=timeout)
    if completed.returncode != 0:
        raise ProbeError("命令失败(%d): %s\nstdout:\n%s\nstderr:\n%s"
                         % (completed.returncode, " ".join(argv),
                            completed.stdout[-4000:], completed.stderr[-4000:]))
    return completed


def parse_json_lines(text):
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            rows.append(json.loads(line))
        except ValueError:
            continue
    return rows


def build_probe(source, name, work, output, log_name, release=True):
    """复制到临时目录并离线构建；不写工作区 target，不改工作区依赖。"""
    crate = work / name
    shutil.copytree(source, crate, ignore=shutil.ignore_patterns("target"))
    manifest = crate / "Cargo.toml"
    text = manifest.read_text()
    vendor = (ROOT / "vendor/gpui-pre-0.3.9").resolve()
    text = text.replace(PATCH_LINE, 'path = "%s"' % vendor)
    manifest.write_text(text)
    target = work / ("target-" + name)
    env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(target))
    argv = ["cargo", "build", "--offline", "--manifest-path", str(manifest)]
    if release:
        argv.append("--release")
    build = run(argv, env=env)
    write_text(output / (log_name + "-build.log"), build.stdout + build.stderr)
    profile = "release" if release else "debug"
    binary = target / profile / name
    if not binary.is_file():
        raise ProbeError("探针二进制未生成: " + str(binary))
    return binary, env


def environment_facts():
    isolation = json.loads((ROOT / "scripts/fixtures/isolation.json").read_text())
    registered = resolve_path(isolation["cargo_home"])
    actual = os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo")
    exists = registered is not None and Path(registered).is_dir()
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "rustc": run(["rustc", "--version"]).stdout.strip(),
        "cargo": run(["cargo", "--version"]).stdout.strip(),
        "profile": "release（三个探针 crate 均 --release）",
        "locked_gpui": "gpui-pre 0.3.9（vendor/gpui-pre-0.3.9）",
        "scale_note": "本探针不读显示器缩放；位图占用按 width×height×4×frames 计",
        # 与 probe_m4_v03 同一披露：登记缓存缺失时证据不能反推实际构建缓存。
        "cargo_home": display_path(actual),
        "registered_cargo_home": isolation["cargo_home"],
        "registered_cargo_home_exists": exists,
        "cargo_home_note": ("使用登记隔离缓存" if exists else
                            "登记的隔离缓存不存在；本次以宿主缓存 + CARGO_NET_OFFLINE=true "
                            "构建，证据不能反推实际缓存，属入口环境缺陷而非本变更引入"),
        "not_measured": [
            "真实窗口合成与 backgroundOpacity 三平台回退",
            "主题安装事务、注册表发布与崩溃恢复",
            "字体 face 的 PostScript 身份与 weight/style 匹配结果",
            "文档图片与预览在真实视口中的交互（缩放/平移/1:1）",
        ],
    }


def real_run(output):
    if sys.platform != "darwin":
        raise ProbeError("V-04/V-06 探针当前只批准 macOS（锁定 GPUI 字体与解码）")
    work = Path(tempfile.mkdtemp(prefix="aline-m4-v04-"))
    try:
        return _real_run(output, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def _real_run(output, work):
    zip_binary, _ = build_probe(PROBE_ZIP, ZIP_BINARY, work, output, "zip")
    limits_binary, _ = build_probe(PROBE_LIMITS, LIMITS_BINARY, work, output, "limits")
    content_binary, _ = build_probe(PROBE_CONTENT, CONTENT_BINARY, work, output, "content")

    write_json(output / "environment.json", environment_facts())

    # ---- 包样本
    packages_dir = work / "packages"
    packages_dir.mkdir()
    cases = package_cases(packages_dir)
    package_rows = []
    read_rows = []
    for case, path, _expectation in cases:
        listing_rows = parse_json_lines(run([str(zip_binary), "list", str(path)]).stdout)
        listing = listing_rows[0]["result"] if listing_rows else None
        # 真实预览/字体字节：用 Python 的 zipfile 按名读（样本是本机生成的，
        # 不是下载物；读出来只为跑格式契约，不参与解压预算判定）。超过单字体
        # 上限的条目跳过预读——它们由 Rust 探针限流读取，避免把炸弹展开进内存。
        raw = {}
        try:
            with zipfile.ZipFile(path) as archive:
                for info in archive.infolist():
                    if info.file_size <= rules.budget("font_single_bytes"):
                        raw[info.filename] = archive.read(info)
        except (zipfile.BadZipFile, OSError):
            raw = {}
        reasons = []
        stream = []
        if not _zip_magic(path):
            reasons.append("不是 ZIP 归档（缺少 PK 签名）")
        elif listing and listing.get("ok"):
            # 逐条限流读取：用来量“声明大小撒谎”这类只有读到才知道的差异。
            read_parsed = parse_json_lines(run([
                str(zip_binary), "read",
                str(rules.budget("extraction_single_file")),
                str(path)]).stdout)
            stream = (read_parsed[0]["result"] or {}).get("entries", []) if read_parsed else []
            read_rows.append({"file": case,
                              "limit": rules.budget("extraction_single_file"),
                              "entries": stream})
            reasons.extend(audit_package(listing, stream))
            theme = raw.get("theme.json") or raw.get("probe-paper/theme.json")
            if theme is not None:
                manifest, manifest_reasons = audit_manifest_bytes(theme)
                reasons.extend(manifest_reasons)
                reasons.extend(audit_previews(manifest, raw))
                reasons.extend(audit_fonts(manifest, raw))
            else:
                reasons.append("缺少 theme.json")
        else:
            reasons.append("库无法读取该归档: %s"
                           % ((listing or {}).get("error") or "未知错误"))
        expectation, needles = path_case_verdict(case)
        if expectation == "accept":
            passed = not reasons
        else:
            # 拒绝样本要求命中**登记的**拒绝理由；任意报错不算通过。
            # 大小写不敏感：库的错误文本大小写不受控，理由本身才是契约。
            passed = any(needle.lower() in reason.lower()
                         for needle in needles for reason in reasons)
        package_rows.append({
            "case": case,
            "expectation": expectation,
            "passed": passed,
            "reasons": reasons,
            "library_ok": bool(listing and listing.get("ok")),
            "library_error": (listing or {}).get("error"),
            "entry_count": (listing or {}).get("entry_count"),
            "declared_entry_count": (listing or {}).get("declared_entry_count"),
            "declared_total": (listing or {}).get("declared_total"),
        })
    write_json(output / "packages.json", package_rows)

    # ---- 受限解压：预算是否在过程中执行
    extract_rows = []
    bomb = packages_dir / "bomb.zip"
    for limit in (rules.budget("extraction_total"),
                  rules.budget("extraction_total") // 16):
        dest = work / ("extract-%d" % limit)
        dest.mkdir()
        rows = parse_json_lines(run([str(zip_binary), "extract", str(limit),
                                     str(dest), str(bomb)]).stdout)
        extract_rows.append({"file": "bomb.zip", "limit": limit,
                             "result": rows[0]["result"] if rows else None})
    legal = packages_dir / "legal.zip"
    dest = work / "extract-legal"
    dest.mkdir()
    rows = parse_json_lines(run([str(zip_binary), "extract",
                                 str(rules.budget("extraction_total")),
                                 str(dest), str(legal)]).stdout)
    extract_rows.append({"file": "legal.zip",
                         "limit": rules.budget("extraction_total"),
                         "result": rows[0]["result"] if rows else None})
    write_json(output / "extraction.json", extract_rows)

    # 每个可读样本的限流读取结果都已在包循环里收集，这里只落盘。
    write_json(output / "reads.json", read_rows)

    # ---- 目录枚举阶梯：条目数 → 预检耗时（`entry_count` 预算的依据）
    ladder_dir = work / "ladder"
    ladder_dir.mkdir()
    ladder_cases = _entry_ladder(ladder_dir)
    rows = parse_json_lines(run([str(zip_binary), "list"]
                                + [str(path) for _name, path in ladder_cases]).stdout)
    by_file = {row["file"]: row["result"] for row in rows}
    entry_rows = []
    for name, path in ladder_cases:
        result = by_file.get(str(path), {})
        entry_rows.append({"case": name, "source_bytes": path.stat().st_size,
                           "result": result})
    write_json(output / "entries.json", entry_rows)

    # ---- 包体阶梯：解压耗时随包体线性（`package_bytes`/`extraction_total` 的依据）
    body_cases = _body_ladder(ladder_dir)
    body_rows = []
    for name, path in body_cases:
        dest = work / ("extract-" + name)
        dest.mkdir()
        rows = parse_json_lines(run([str(zip_binary), "extract",
                                     str(rules.budget("extraction_total")),
                                     str(dest), str(path)]).stdout)
        body_rows.append({"case": name, "source_bytes": path.stat().st_size,
                          "result": rows[0]["result"] if rows else None})
    write_json(output / "bodies.json", body_rows)

    # ---- 压缩比对照：全零 vs 不可压缩（`compression_ratio` 的依据）
    ratio_rows = []
    for name, path in _ratio_pair(ladder_dir):
        rows = parse_json_lines(run([str(zip_binary), "list", str(path)]).stdout)
        result = rows[0]["result"] if rows else {}
        declared = result.get("declared_total") or 0
        compressed = result.get("compressed_total") or 0
        ratio_rows.append({
            "case": name,
            "source_bytes": path.stat().st_size,
            "declared_total": declared,
            "compressed_total": compressed,
            "ratio": (declared / compressed) if compressed else None,
        })
    write_json(output / "ratios.json", ratio_rows)

    # ---- JSON 形状与耗时
    json_dir = work / "json"
    json_dir.mkdir()
    json_cases = _json_cases(json_dir)
    rows = parse_json_lines(run([str(zip_binary), "json"]
                                + [str(path) for _name, path in json_cases]).stdout)
    by_file = {row["file"]: row["result"] for row in rows}
    json_rows = []
    for name, path in json_cases:
        result = by_file.get(str(path), {})
        data = path.read_bytes()
        parsed, contract_reasons = rules.json_text_violations(data)
        # 库与契约不一致的地方才是要记录的能力缺口；一致时只记一行。
        json_rows.append({
            "case": name,
            "source_bytes": len(data),
            "library": result,
            "contract_reasons": contract_reasons,
            "contract_ok": not contract_reasons,
            "library_ok": bool(result.get("ok")),
            "agrees": bool(result.get("ok")) == (not contract_reasons),
        })
    write_json(output / "json.json", json_rows)

    # ---- 图片：解码耗时与位图
    image_dir = work / "images"
    image_dir.mkdir()
    image_cases = _image_cases(image_dir)
    rows = parse_json_lines(run([str(limits_binary), "images"]
                                + [str(path) for _name, path in image_cases]).stdout)
    by_file = {row["file"]: row["result"] for row in rows}
    image_rows = [{"case": name, "result": by_file.get(str(path), {})}
                  for name, path in image_cases]
    write_json(output / "images.json", image_rows)

    # ---- 字体注册与激活
    font_rows = _font_runs(limits_binary, work)
    write_json(output / "fonts.json", font_rows)

    # ---- 内容渲染耗时
    content_rows = _content_runs(content_binary)
    write_json(output / "content.json", content_rows)

    summary = {
        "packages": package_rows,
        "extraction": extract_rows,
        "reads": read_rows,
        "entries": entry_rows,
        "bodies": body_rows,
        "ratios": ratio_rows,
        "json": json_rows,
        "images": image_rows,
        "fonts": font_rows,
        "content": content_rows,
        "budgets": {name: {key: value for key, value in entry.items()}
                    for name, entry in rules.BUDGETS.items()},
        "defaults": rules.default_config(),
        "gaps": capability_gaps(),
    }
    write_json(output / "summary.json", summary)
    write_text(output / "summary.md", render_summary(summary))
    return summary


def _entry_ladder(directory):
    """条目数阶梯：8 MiB 载荷按条目均分，只改条目数。

    预检只读中央目录，所以载荷取同一个总量可以让“条目数”成为唯一变量。
    """
    payload = 8 * 1024 * 1024
    cases = []
    for count in (100, 1000, 5000, 20000, 50000):
        path = directory / ("entries-%05d.zip" % count)
        per = payload // count
        block = bytes(random_bytes(64 * 1024))
        path.write_bytes(build_package([
            ("e%05d.bin" % index,
             (block * (per // len(block) + 1))[:per])
            for index in range(count)]))
        cases.append(("entries-%d" % count, path))
    return cases


def _body_ladder(directory):
    """包体阶梯：12 个等大条目，总量 8/32/64 MiB。"""
    cases = []
    for total_mib in (8, 32, 64):
        path = directory / ("body-%02dmb.zip" % total_mib)
        entries = total_mib // 4
        per = 4 * 1024 * 1024
        block = bytes(random_bytes(64 * 1024))
        path.write_bytes(build_package([
            ("b%02d.bin" % index, (block * (per // len(block) + 1))[:per])
            for index in range(entries)]))
        cases.append(("body-%dMiB" % total_mib, path))
    return cases


def _ratio_pair(directory):
    """同尺寸的两种载荷：全零（炸弹特征）与不可压缩随机。"""
    size = 8 * 1024 * 1024
    zeros = directory / "ratio-zeros.zip"
    zeros.write_bytes(build_package([("z.bin", b"\0" * size)]))
    random_path = directory / "ratio-random.zip"
    random_path.write_bytes(build_package([("r.bin", bytes(random_bytes(size)))]))
    return [("zeros", zeros), ("random", random_path)]


def random_bytes(count):
    """确定性伪随机字节；固定种子使样本可复跑、字节稳定。

    `Random.randbytes`（3.9+）是 C 实现；逐字节 `getrandbits` 在 8 MiB 上要
    跑几千万次 Python 调用，会让探针本身成为瓶颈。
    """
    return random.Random(20261126).randbytes(count)


def _json_cases(directory):
    cases = []
    boundary_depth = rules.budget("json_depth")
    members = rules.budget("json_members")
    items = rules.budget("json_array_items")
    scalars = rules.budget("json_string_scalars")
    cases.append(("depth-boundary", _write_json_file(
        directory / "depth-boundary.json", _nested(boundary_depth))))
    cases.append(("depth-over", _write_json_file(
        directory / "depth-over.json", _nested(boundary_depth + 1))))
    # 解析器自身的递归上限：证明本层 32 层远低于它，超限由本层给出类型化
    # 错误而不是解析器的通用 recursion limit。具体边界值由本轮实测确定，
    # 不照抄文档里的“128”。
    for depth in (127, 128, 129, 130, 200):
        cases.append(("depth-%d" % depth, _write_json_file(
            directory / ("depth-%d.json" % depth), _nested(depth))))
    cases.append(("members-boundary", _write_json_file(
        directory / "members-boundary.json",
        {"common": {("k%05d" % i): 1 for i in range(members)}})))
    cases.append(("members-over", _write_json_file(
        directory / "members-over.json",
        {"common": {("k%05d" % i): 1 for i in range(members + 1)}})))
    cases.append(("array-boundary", _write_json_file(
        directory / "array-boundary.json",
        {"common": {"families": ["F"] * items}})))
    cases.append(("array-over", _write_json_file(
        directory / "array-over.json",
        {"common": {"families": ["F"] * (items + 1)}})))
    cases.append(("string-boundary", _write_json_file(
        directory / "string-boundary.json",
        {"common": {"name": "a" * scalars}})))
    cases.append(("string-over", _write_json_file(
        directory / "string-over.json",
        {"common": {"name": "a" * (scalars + 1)}})))
    size = rules.budget("json_bytes")
    cases.append(("bytes-boundary", _write_raw(
        directory / "bytes-boundary.json", _filler_json(size))))
    cases.append(("bytes-over", _write_raw(
        directory / "bytes-over.json", _filler_json(size + 1))))
    cases.append(("legal-manifest", _write_json_file(
        directory / "legal-manifest.json", schema1_manifest())))
    cases.append(("duplicate-key", _write_raw(
        directory / "duplicate-key.json",
        b'{"common": {"spacing": {"paragraphAfter": 16}},'
        b' "common": {"spacing": {"paragraphAfter": 18}}}')))
    cases.append(("duplicate-nested-key", _write_raw(
        directory / "duplicate-nested-key.json",
        b'{"common": {"spacing": {"paragraphAfter": 16, "paragraphAfter": 18}}}')))
    cases.append(("bom", _write_raw(directory / "bom.json",
                                    b"\xef\xbb\xbf" + json.dumps({}).encode())))
    cases.append(("trailing-comma", _write_raw(directory / "trailing.json",
                                               b'{"common": {},}')))
    cases.append(("comment", _write_raw(directory / "comment.json",
                                        b'{"common": {} // x\n}')))
    cases.append(("nan", _write_raw(directory / "nan.json", b'{"common": {"x": NaN}}')))
    cases.append(("top-array", _write_raw(directory / "top-array.json", b"[]")))
    cases.append(("top-string", _write_raw(directory / "top-string.json", b'"x"')))
    cases.append(("invalid-utf8", _write_raw(directory / "invalid-utf8.json",
                                             b'{"common": {"x": "\xff"}}')))
    return cases


def _nested(depth):
    """构造深度恰为 `depth` 的对象（顶层对象算 1 层）。"""
    value = 1
    for _ in range(depth - 1):
        value = {"k": value}
    return value


def _filler_json(target_bytes):
    """生成字节数**恰好**为 `target_bytes` 的合法 JSON 对象。

    逐段拼接而不是反复 `json.dumps` 整个对象——后者是 O(n²)，在 1 MiB 上会
    卡住几十秒。成员值用长字符串填充，使 1 MiB 对应的成员数远低于成员预算，
    这样“字节边界/越界”样本只触发字节这一条判定，不会顺带触发成员上限。
    """
    head = '{"common": {'
    tail = '"_": 0}}'
    pieces = []
    size = len(head) + len(tail)
    index = 0
    while True:
        piece = '"k%05d": "%s", ' % (index, "a" * 400)
        if size + len(piece) > target_bytes:
            break
        pieces.append(piece)
        size += len(piece)
        index += 1
    remaining = target_bytes - size
    minimal = '"k%05d": "", ' % index
    if remaining < len(minimal):
        if not pieces:
            raise rules.ContractError("无法精确构造 %d 字节的 JSON" % target_bytes)
        size -= len(pieces.pop())
        index -= 1
        remaining = target_bytes - size
        minimal = '"k%05d": "", ' % index
    pieces.append('"k%05d": "%s", ' % (index, "a" * (remaining - len(minimal))))
    text = head + "".join(pieces) + tail
    if len(text) != target_bytes:
        raise rules.ContractError("填充 JSON 长度偏差: %d != %d"
                                  % (len(text), target_bytes))
    return text.encode("utf-8")


def _write_json_file(path, value):
    path.write_text(json.dumps(value), encoding="utf-8")
    return path


def _write_raw(path, data):
    path.write_bytes(data)
    return path


def _image_cases(directory):
    cases = []
    # 512² 与 8192² 是像素预算依据里引用的两端，必须在本轮重新量。
    for side in (256, 512, 1024, 2048, 4096, 8192):
        path = directory / ("square-%d.png" % side)
        path.write_bytes(png_bytes(side, side, (30, 30, 30, 255), level=1))
        cases.append(("png-%d" % side, path))
    # 不可压缩噪声：量“字节界限先于像素界限拒绝”的对照。
    for side in (2048, 4096):
        path = directory / ("noise-%d.png" % side)
        path.write_bytes(_noise_png(side, side))
        cases.append(("noise-%d" % side, path))
    cases.append(("apng", _write_raw(directory / "anim.png", apng_bytes(64, 64))))
    cases.append(("jpeg", _write_raw(directory / "plain.jpg", jpeg_bytes(64, 64))))
    cases.append(("webp", _write_raw(directory / "plain.webp", WEBP_PREVIEW)))
    cases.append(("svg", _write_raw(directory / "plain.svg", SVG_PREVIEW)))
    truncated = png_bytes(64, 64)
    cases.append(("truncated", _write_raw(directory / "truncated.png",
                                          truncated[:len(truncated) // 2])))
    return cases


def _noise_png(width, height):
    """逐像素随机 RGBA：压缩几乎无效，用来量最坏字节体积。"""
    rng = random.Random(20261126)
    raw = b"".join(b"\x00" + rng.randbytes(width * 4) for _ in range(height))
    return _png_container(width, height, zlib.compress(raw, 1))


def _font_runs(binary, work):
    rows = []
    single = [FONT_DIR / "ProbeSans-Regular.ttf", FONT_DIR / "ProbeSans-Bold.ttf",
              FONT_DIR / "ProbeNoM-Regular.ttf", FONT_DIR / "corrupt.ttf",
              FONT_DIR / "empty.ttf"]
    # 真实静态 TTF 的注册阶梯：4.5 KB 自制字体到 23 MB Arial Unicode。
    # 这些是系统字体，只读不复制；缺席时跳过而不是让探针失败。
    system = [Path("/System/Library/Fonts/SFNSRounded.ttf"),
              Path("/System/Library/Fonts/Supplemental/AppleGothic.ttf"),
              Path("/System/Library/Fonts/Supplemental/Arial Unicode.ttf")]
    single.extend(path for path in system if path.is_file())
    result = parse_json_lines(run([str(binary), "fonts"]
                                  + [str(path) for path in single]).stdout)
    by_file = {row["file"]: row["result"] for row in result}
    for path in single:
        rows.append({"case": "register:" + path.name,
                     "source_bytes": path.stat().st_size,
                     "result": by_file.get(str(path), {})})

    activations = [
        ("activation:probe-sans", "Aline Probe Sans", ["ProbeSans-Regular.ttf"]),
        ("activation:no-m-glyph", "Aline Probe NoM", ["ProbeNoM-Regular.ttf"]),
        ("hijack:reserved-role", "monospace", ["HijackMonospace.ttf"]),
        ("hijack:system-family", "Helvetica", ["HijackHelvetica.ttf"]),
        ("conflict:same-family", "Aline Probe Sans",
         ["ProbeSansAlt-Regular.ttf", "ProbeSans-Regular.ttf"]),
    ]
    for case, family, files in activations:
        argv = [str(binary), "shape", family] + [str(FONT_DIR / name) for name in files]
        parsed = parse_json_lines(run(argv).stdout)
        rows.append({"case": case, "result": parsed[0] if parsed else {}})
    return rows


def _content_runs(binary):
    rows = []
    math_sizes = ["100", "400", "800", "1600", "3200", "6400"]
    parsed = parse_json_lines(run([str(binary), "math"] + math_sizes).stdout)
    for row in parsed:
        rows.append({"kind": "math", "result": row})
    mermaid_sizes = ["10", "20", "100", "200", "300", "400"]
    parsed = parse_json_lines(run([str(binary), "mermaid"] + mermaid_sizes).stdout)
    for row in parsed:
        rows.append({"kind": "mermaid", "result": row})
    return rows


# ------------------------------------------------------------------ 能力缺口


def capability_gaps():
    """锁定 GPUI / 依赖库的能力缺口：实测得到，不写成“工作正常”。"""
    return [
        {
            "id": "no-face-release",
            "area": "字体",
            "fact": "zed-font-kit 的 MemSource 无移除 API，add_fonts 只能追加；"
                    "主题删除/切换后包内 face 无法释放，font_generation 只推进代次。",
            "impact": "主题反复切换会累积内存中的 face；规范要求的“删除后释放 face”"
                      "在锁定实现下不可达，须记为已知限制或另立方案。",
        },
        {
            "id": "m-glyph-silent-skip",
            "area": "字体",
            "fact": "gpui-pre-apple 的 load_family 跳过缺少 'm' 字形的字体（只 log::warn），"
                    "但 all_font_names 仍能看到该家族；实测 ProbeNoM 注册成功、"
                    "resolved_family 落到 Helvetica、advance 未变。",
            "impact": "“注册成功/家族可见”不等于“激活生效”。生产必须用 advance 或"
                      "字形证据判定激活，不能只看 all_font_names 或 add_fonts 的 Ok。",
        },
        {
            "id": "reserved-family-hijack",
            "area": "字体",
            "fact": "包内字体可声明 family=monospace 并被注册、解析到（advance 变为"
                    "包内度量 9.6）。锁定 GPUI 不认识“保留通用角色”契约。",
            "impact": "保留角色名的拒绝必须由主题清单校验承担；不能依赖注册入口。",
        },
        {
            "id": "system-family-shadowing",
            "area": "字体",
            "fact": "内存源优先于系统源：声明 family=Helvetica 的包内字体会遮蔽系统"
                    "Helvetica（advance 9.6 而非系统度量），且 names_added 为空。",
            "impact": "“按包选择字体”无法用全局注册实现；同名系统族会被静默替换。"
                      "规范要求的隔离与命中须另找方案（每窗口独立 TextSystem 或别名）。",
        },
        {
            "id": "no-duplicate-key-rejection",
            "area": "JSON",
            "fact": "serde_json 接受重复 key 并以最后值覆盖；规范要求解析时拒绝。",
            "impact": "主题解析必须在 serde_json 之前或之外做重复 key 检查，"
                      "不能只依赖反序列化。",
        },
        {
            "id": "no-per-image-pixel-cap",
            "area": "图片",
            "fact": "image 0.25 的 Limits::default() 不限宽高，只有 512 MiB 分配上限；"
                    "GPUI 的 ImageAssetLoader 不加每图像素上限。",
            "impact": "预览与文档图片的像素上限必须由主题/资产入口在解码前读 IHDR 判定，"
                      "解码器不会替产品拒绝。",
        },
        {
            "id": "gpui-accepts-all-preview-formats",
            "area": "图片",
            "fact": "GPUI 解码器接受 JPEG/WebP/GIF/SVG，APNG 解为 1 帧；"
                    "“只接受静态 PNG”无法靠解码失败实现。",
            "impact": "预览格式拒绝必须由签名/块检查（PNG 签名 + 无 acTL）承担。",
        },
        {
            "id": "zip-normalizes-instead-of-rejecting",
            "area": "ZIP",
            "fact": "zip 8.6.0 的 enclosed_name 会剥离前导 /、折叠 a//b、./x、dir\\\\file，"
                    "只在穿越到根之上时返回 None；重复原始名被 IndexMap 静默合并；"
                    "EOCD 声明条目数不公开。",
            "impact": "路径拒绝、重复检测、声明/实际条目数比对都必须由本层实现；"
                      "不能把 enclosed_name 当作路径安全闭环。",
        },
        {
            "id": "no-font-face-identity",
            "area": "字体",
            "fact": "TextSystem 只暴露 FontId 槽位与 resolved_family，没有 PostScript face、"
                    "weight/style 匹配结果或 face_source。",
            "impact": "“相同 family/weight/style 重复 face 拒绝歧义”无法在注册后判定，"
                      "必须在字体文件元数据阶段（font-kit/ttf-parser）完成。",
        },
        {
            "id": "transparency-api-present-unmeasured",
            "area": "窗口",
            "fact": "WindowBackgroundAppearance 与 set_background_appearance 存在，"
                    "macOS 实现调用 renderer.update_transparency/setOpaque_；"
                    "本轮未做真实窗口合成测量。",
            "impact": "backgroundOpacity 的三平台合成与回退仍未取证，V-04 透明度分项不通过。",
        },
    ]


# ------------------------------------------------------------------ 汇总渲染


def _ms(value):
    if value is None:
        return "—"
    return "%.2f" % value


def render_summary(summary):
    lines = ["# V-04/V-06 实测汇总", ""]
    lines.append("## 预算冻结")
    lines.append("")
    lines.append("| 预算 | 值 | 单位 | 含端点 | 依据 |")
    lines.append("|---|---|---|---|---|")
    for name, entry in rules.BUDGETS.items():
        lines.append("| %s | %s | %s | %s | %s |" % (
            name, entry["value"], entry["unit"],
            "是" if entry["inclusive"] else "否",
            entry["basis"].replace("\n", " ")))
    lines.append("")
    lines.append("## 包样本")
    lines.append("")
    lines.append("| 样本 | 期望 | 通过 | 库可读 | 原因 |")
    lines.append("|---|---|---|---|---|")
    for row in summary["packages"]:
        lines.append("| %s | %s | %s | %s | %s |" % (
            row["case"], row["expectation"], "是" if row["passed"] else "否",
            "是" if row["library_ok"] else "否",
            "；".join(row["reasons"])[:200] or "—"))
    lines.append("")
    lines.append("## 测量阶梯")
    lines.append("")
    lines.append("### 条目数 → 目录预检耗时")
    lines.append("")
    lines.append("| 样本 | 包体 bytes | 条目数 | 耗时 ms |")
    lines.append("|---|---|---|---|")
    for row in summary["entries"]:
        result = row["result"] or {}
        lines.append("| %s | %d | %s | %s |" % (
            row["case"], row["source_bytes"], result.get("entry_count"),
            _ms(result.get("elapsed_ms"))))
    lines.append("")
    lines.append("### 包体 → 受限解压耗时")
    lines.append("")
    lines.append("| 样本 | 包体 bytes | 写入 bytes | 耗时 ms | 中止于 |")
    lines.append("|---|---|---|---|---|")
    for row in summary["bodies"]:
        result = row["result"] or {}
        lines.append("| %s | %d | %s | %s | %s |" % (
            row["case"], row["source_bytes"], result.get("written_bytes"),
            _ms(result.get("elapsed_ms")), result.get("aborted_at")))
    lines.append("")
    lines.append("### 压缩比对照")
    lines.append("")
    lines.append("| 载荷 | 声明 bytes | 压缩 bytes | 比值 |")
    lines.append("|---|---|---|---|")
    for row in summary["ratios"]:
        lines.append("| %s | %d | %d | %s |" % (
            row["case"], row["declared_total"], row["compressed_total"],
            ("%.1f:1" % row["ratio"]) if row["ratio"] else "—"))
    lines.append("")
    lines.append("### 图片解码")
    lines.append("")
    lines.append("| 样本 | 源 bytes | 宽×高 | 帧 | 位图 bytes | 耗时 ms | 结果 |")
    lines.append("|---|---|---|---|---|---|---|")
    for row in summary["images"]:
        result = row["result"] or {}
        lines.append("| %s | %s | %s×%s | %s | %s | %s | %s |" % (
            row["case"], result.get("source_bytes"), result.get("width"),
            result.get("height"), result.get("frames"), result.get("bitmap_bytes"),
            _ms(result.get("elapsed_ms")),
            "成功" if result.get("ok") else (result.get("error") or "失败")))
    lines.append("")
    lines.append("### 字体注册")
    lines.append("")
    lines.append("| 样本 | 源 bytes | 耗时 ms | 新增字族 | 结果 |")
    lines.append("|---|---|---|---|---|")
    for row in summary["fonts"]:
        result = row["result"] or {}
        if "after" in result:
            continue
        lines.append("| %s | %s | %s | %s | %s |" % (
            row["case"], row.get("source_bytes") or result.get("source_bytes"),
            _ms(result.get("elapsed_ms")),
            ",".join(result.get("names_added") or []) or "—",
            "成功" if result.get("ok") else (result.get("error") or "失败")))
    lines.append("")
    lines.append("### 字体激活（advance 是生效证据，字族名不是）")
    lines.append("")
    lines.append("| 样本 | 解析到 | 'm' advance | em advance |")
    lines.append("|---|---|---|---|")
    for row in summary["fonts"]:
        result = row["result"] or {}
        after = result.get("after")
        if not after:
            continue
        lines.append("| %s | %s | %s | %s |" % (
            row["case"], after.get("resolved_family"),
            _ms(after.get("advance_m")), _ms(after.get("em_advance"))))
    lines.append("")
    lines.append("### 内容渲染")
    lines.append("")
    lines.append("| 类型 | 规模 | 源码 bytes | 产物 bytes | 冷 ms | 热 ms |")
    lines.append("|---|---|---|---|---|---|")
    for row in summary["content"]:
        result = row["result"] or {}
        lines.append("| %s | %s | %s | %s | %s | %s |" % (
            row["kind"], result.get("terms") or result.get("nodes"),
            result.get("source_bytes"), result.get("output_bytes"),
            _ms(result.get("cold_ms")), _ms(result.get("warm_ms"))))
    lines.append("")
    lines.append("## JSON 前置检查与解析库的差异")
    lines.append("")
    lines.append("| 样本 | 解析库接受 | 契约接受 | 契约理由 |")
    lines.append("|---|---|---|---|")
    for row in summary["json"]:
        lines.append("| %s | %s | %s | %s |" % (
            row["case"], "是" if row["library_ok"] else "否",
            "是" if row["contract_ok"] else "否",
            "；".join(row["contract_reasons"]) or "—"))
    lines.append("")
    lines.append("## 能力缺口")
    lines.append("")
    for gap in summary["gaps"]:
        lines.append("- **%s**（%s）：%s" % (gap["id"], gap["area"], gap["fact"]))
        lines.append("  - 影响：%s" % gap["impact"])
    lines.append("")
    return "\n".join(lines)


# ------------------------------------------------------------------ 自测


def self_test():
    cases = []

    def case(name, function):
        try:
            function()
            cases.append({"case": name, "passed": True})
        except AssertionError as error:
            cases.append({"case": name, "passed": False, "detail": str(error)})
        except Exception as error:  # noqa: BLE001 - 自测把异常记为失败，不中断整轮
            cases.append({"case": name, "passed": False,
                          "detail": "%s: %s" % (type(error).__name__, error)})

    def expect(actual, expected):
        if actual != expected:
            raise AssertionError("期望 %r，实际 %r" % (expected, actual))

    # 包样本生成较慢（含 2048² 预览与 64 MiB 炸弹），整轮共用一份。
    package_work = Path(tempfile.mkdtemp(prefix="aline-m4-v04-selftest-"))
    package_paths = {case: path for case, path, _e in package_cases(package_work)}

    # ---- 预算表
    case("budgets-nonempty", lambda: expect(len(rules.BUDGETS) >= 20, True))
    case("budget-unknown-raises", lambda: _raises(
        rules.ContractError, lambda: rules.budget("no-such-budget")))
    case("budget-boundary-not-over", lambda: expect(
        rules.over_limit("json_depth", rules.boundary_sample("json_depth")), False))
    case("budget-over-is-over", lambda: expect(
        rules.over_limit("json_depth", rules.over_sample("json_depth")), True))
    case("budget-all-have-basis", lambda: expect(
        [name for name, entry in rules.BUDGETS.items()
         if not entry.get("basis") or not entry.get("owner")], []))
    # 已作废的旧草稿数字不得在同一维度复活；同值出现在别的维度不算复用
    # （1 MiB 曾是“预览上限”，现在是 JSON 上限，来源不同）。
    case("budget-no-legacy-values", lambda: expect(
        [(name, rules.budget(name)) for name, retired in {
            "package_bytes": {100 * 1024 * 1024, 120 * 1024 * 1024},
            "preview_bytes": {1024 * 1024},
            "font_count": {8},
            "font_single_bytes": {20 * 1024 * 1024},
            "font_total_bytes": {80 * 1024 * 1024},
            "json_string_scalars": {128},
            "preview_pixels": {8192 * 8192},
            "preview_decode_bytes": {256 * 1024 * 1024},
        }.items() if rules.budget(name) in retired], []))

    # ---- schema
    def valid_documents_pass():
        for kind, document in (("markdown", schema1_documents()["markdown.json"]),
                               ("highlight", schema1_documents()["highlight.json"]),
                               ("application", schema1_documents()["application.json"])):
            expect(rules.document_violations(document, kind), [])

    case("schema1-documents-pass", valid_documents_pass)

    def unknown_field_rejected():
        document = {"common": {"colors": {"body": "#111111", "link": "#222222"}}}
        expect(any("未知字段" in item
                   for item in rules.document_violations(document, "markdown")), True)

    case("unknown-field-rejected", unknown_field_rejected)

    def null_rejected():
        document = {"common": {"spacing": {"paragraphAfter": None}}}
        expect(any("null" in item
                   for item in rules.document_violations(document, "markdown")), True)

    case("null-rejected", null_rejected)

    def bad_color_rejected():
        for bad in ("#FFF", "red", "#GGGGGG", "#11223344"):
            document = {"modes": {"light": {"colors": {"body": bad}}}}
            expect(bool(rules.document_violations(document, "markdown")), True)

    case("bad-color-rejected", bad_color_rejected)

    def weight_enum():
        for weight in (100, 400, 900):
            document = {"common": {"typography": {"bodyWeight": weight}}}
            expect(rules.document_violations(document, "markdown"), [])
        for weight in (50, 450, 1000, "400", True):
            document = {"common": {"typography": {"bodyWeight": weight}}}
            expect(bool(rules.document_violations(document, "markdown")), True)

    case("weight-enum", weight_enum)

    def empty_families_rejected():
        document = {"common": {"typography": {"bodyFamilies": []}}}
        expect(bool(rules.document_violations(document, "markdown")), True)

    case("empty-families-rejected", empty_families_rejected)

    def zero_is_real_value():
        document = {"common": {"spacing": {"paragraphAfter": 0},
                               "table": {"zebra": False}}}
        expect(rules.document_violations(document, "markdown"), [])

    case("zero-and-false-are-values", zero_is_real_value)

    def unknown_mode_rejected():
        document = {"modes": {"dusk": {}}}
        expect(any("未知模式" in item
                   for item in rules.document_violations(document, "markdown")), True)

    case("unknown-mode-rejected", unknown_mode_rejected)

    # ---- JSON 前置检查
    def json_precheck_table():
        work = Path(tempfile.mkdtemp(prefix="aline-m4-v04-selftest-"))
        try:
            cases = dict(_json_cases(work))
            # depth-127…200 故意**不在**接受集：契约的 32 层上限先拒绝它们；
            # 它们存在是为了让库自身的递归上限可被观测，而不是为了通过。
            accept = {"depth-boundary", "members-boundary", "array-boundary",
                      "string-boundary", "bytes-boundary", "legal-manifest"}
            for name, path in cases.items():
                _value, reasons = rules.json_text_violations(path.read_bytes())
                if name in accept:
                    expect(reasons, [])
                else:
                    expect((name, bool(reasons)), (name, True))
        finally:
            shutil.rmtree(work, ignore_errors=True)

    case("json-precheck-table", json_precheck_table)

    def json_precheck_reasons():
        work = Path(tempfile.mkdtemp(prefix="aline-m4-v04-selftest-"))
        try:
            cases = dict(_json_cases(work))

            def reasons(name):
                return rules.json_text_violations(cases[name].read_bytes())[1]

            expect(any("重复 key" in item for item in reasons("duplicate-key")), True)
            expect(any("重复 key" in item
                       for item in reasons("duplicate-nested-key")), True)
            expect(any("BOM" in item for item in reasons("bom")), True)
            expect(any("非有限数" in item for item in reasons("nan")), True)
            expect(any("顶层必须是对象" in item for item in reasons("top-array")), True)
            expect(any("顶层必须是对象" in item for item in reasons("top-string")), True)
            expect(any("UTF-8" in item for item in reasons("invalid-utf8")), True)
            expect(any("json_depth" in item for item in reasons("depth-over")), True)
            expect(any("json_members" in item for item in reasons("members-over")), True)
            expect(any("json_array_items" in item
                       for item in reasons("array-over")), True)
            expect(any("json_string_scalars" in item
                       for item in reasons("string-over")), True)
            expect(any("字节上限" in item for item in reasons("bytes-over")), True)
        finally:
            shutil.rmtree(work, ignore_errors=True)

    case("json-precheck-reasons", json_precheck_reasons)

    # ---- 合并
    def arrays_replaced():
        base = {"typography": {"codeFamilies": ["Consolas", "monospace"]}}
        over = {"typography": {"codeFamilies": ["Example Mono", "monospace"]}}
        merged = rules.deep_merge(base, over)
        expect(merged["typography"]["codeFamilies"], ["Example Mono", "monospace"])

    case("arrays-replace-wholesale", arrays_replaced)

    def objects_merge():
        base = {"spacing": {"paragraphAfter": 12, "blockBefore": 20}}
        over = {"spacing": {"paragraphAfter": 18}}
        merged = rules.deep_merge(base, over)
        expect(merged["spacing"], {"paragraphAfter": 18, "blockBefore": 20})

    case("objects-merge-leafwise", objects_merge)

    def null_merge_rejected():
        _raises(rules.ContractError, lambda: rules.deep_merge(
            {"a": 1}, {"a": None}))

    case("merge-null-rejected", null_merge_rejected)

    def mode_resolution_matches_example_a():
        # 规范 §6.3 例 A：dark 的 body 取 dark 覆盖，段距/字体取 common。
        baseline_common = {"typography": {"codeFamilies": ["Consolas", "monospace"]},
                           "spacing": {"paragraphAfter": 12}}
        baseline_modes = {
            "light": {"colors": {"body": "#111111"}},
            "dark": {"colors": {"body": "#E6EDF3"}},
            "sepia": {"colors": {"body": "#222222"}},
        }
        document = {
            "common": {"spacing": {"paragraphAfter": 18},
                       "typography": {"codeFamilies": ["Example Mono", "monospace"]}},
            "modes": {"dark": {"colors": {"body": "#DDDDDD"}}},
        }
        dark = rules.effective_mode(document, "dark", baseline_common, baseline_modes)
        expect(dark["colors"]["body"], "#DDDDDD")
        expect(dark["spacing"]["paragraphAfter"], 18)
        expect(dark["typography"]["codeFamilies"], ["Example Mono", "monospace"])
        light = rules.effective_mode(document, "light", baseline_common, baseline_modes)
        expect(light["colors"]["body"], "#111111")
        expect(light["spacing"]["paragraphAfter"], 18)

    case("mode-resolution-example-a", mode_resolution_matches_example_a)

    def missing_mode_uses_baseline():
        baseline_common = {"colors": {"body": "#000000"}}
        baseline_modes = {"light": {"colors": {"body": "#111111"}},
                          "dark": {"colors": {"body": "#222222"}},
                          "sepia": {"colors": {"body": "#333333"}}}
        document = {"modes": {"dark": {"colors": {"body": "#999999"}}}}
        sepia = rules.effective_mode(document, "sepia", baseline_common, baseline_modes)
        expect(sepia["colors"]["body"], "#333333")

    case("missing-mode-falls-back", missing_mode_uses_baseline)

    # ---- 清单
    def manifest_accepts_schema1():
        expect(rules.manifest_violations(schema1_manifest()), [])

    case("manifest-schema1-accepted", manifest_accepts_schema1)

    def manifest_rejects_schema3():
        manifest = schema1_manifest()
        manifest["schemaVersion"] = 3
        expect(any("schemaVersion" in item
                   for item in rules.manifest_violations(manifest)), True)

    case("manifest-schema3-rejected", manifest_rejects_schema3)

    def manifest_rejects_reserved_id():
        for reserved in rules.RESERVED_IDS:
            expect(any("保留" in item for item in rules.manifest_violations(
                schema1_manifest(reserved))), True)

    case("manifest-reserved-id-rejected", manifest_rejects_reserved_id)

    def manifest_rejects_bad_versions():
        for bad in ("1.0", "01.0.0", "1.0.0-beta", "1.0.0.0", "v1.0.0"):
            manifest = schema1_manifest(version=bad)
            expect(any("version" in item
                       for item in rules.manifest_violations(manifest)), True)
        for good in ("0.0.0", "1.0.0", "10.20.30"):
            manifest = schema1_manifest(version=good)
            expect(rules.manifest_violations(manifest), [])

    case("manifest-version-format", manifest_rejects_bad_versions)

    def manifest_rejects_reserved_family():
        manifest = schema1_manifest("probe-role")
        manifest["fonts"] = [{"path": "fonts/a.ttf", "family": "monospace",
                              "license": "OFL-1.1",
                              "licenseFile": "licenses/a.txt"}]
        expect(any("保留角色" in item
                   for item in rules.manifest_violations(manifest)), True)

    case("manifest-reserved-family-rejected", manifest_rejects_reserved_family)

    def manifest_rejects_duplicate_previews():
        manifest = schema1_manifest()
        manifest["previews"]["dark"] = manifest["previews"]["light"]
        expect(any("互异" in item for item in rules.manifest_violations(manifest)), True)

    case("manifest-duplicate-preview-rejected", manifest_rejects_duplicate_previews)

    # ---- ZIP 路径
    def path_rules():
        accept = ["theme.json", "previews/light.png", "fonts/a.ttf",
                  "licenses/ProbeSans-OFL.txt", "probe-paper/theme.json"]
        reject = ["../evil.json", "/abs.json", "fonts\\a.ttf", "a/./b.json",
                  "a//b.json", "a /b.json", "fonts/CON.ttf", "fonts/a.ttf:stream",
                  "a/../b.json", "fonts/aux", "trail./x"]
        for name in accept:
            expect(rules.zip_entry_violations(name), [])
        for name in reject:
            expect(bool(rules.zip_entry_violations(name)), True)

    case("zip-path-rules", path_rules)

    def directory_trailing_slash():
        expect(rules.zip_entry_violations("fonts/", is_dir=True), [])
        expect(bool(rules.zip_entry_violations("fonts/", is_dir=False)), True)
        expect(bool(rules.zip_entry_violations("fonts//", is_dir=True)), True)

    case("zip-directory-slash", directory_trailing_slash)

    def path_limits():
        boundary = "a" * (rules.budget("path_bytes") - 4) + ".ttf"
        expect(len(boundary.encode()), rules.budget("path_bytes"))
        expect(rules.zip_entry_violations(boundary), [])
        over = "a" * (rules.budget("path_bytes") - 3) + ".ttf"
        expect(bool(rules.zip_entry_violations(over)), True)
        deep = "/".join(["d"] * rules.budget("path_segments"))
        expect(rules.zip_entry_violations(deep), [])
        deeper = "/".join(["d"] * (rules.budget("path_segments") + 1))
        expect(bool(rules.zip_entry_violations(deeper)), True)

    case("zip-path-limits", path_limits)

    def collisions():
        expect(rules.collision_pairs(["a.png", "A.PNG"]), [("a.png", "A.PNG")])
        expect(rules.collision_pairs(["café.png", unicodedata_nfd("café.png")]),
               [("café.png", unicodedata_nfd("café.png"))])
        expect(rules.collision_pairs(["a.png", "b.png"]), [])

    case("zip-collisions", collisions)

    def ancestors():
        expect(rules.ancestor_conflicts([("fonts", False), ("fonts/a.ttf", False)]),
               [("fonts", "fonts/a.ttf")])
        expect(rules.ancestor_conflicts([("fonts/", True), ("fonts/a.ttf", False)]), [])
        expect(rules.ancestor_conflicts([("previews", True), ("previews/a.png", False)]),
               [])

    case("zip-ancestor-conflicts", ancestors)

    # ---- 预览
    def preview_png():
        kind, width, height, animated, error = rules.preview_verdict(png_bytes(8, 8))
        expect((kind, width, height, animated, error), ("png", 8, 8, False, None))

    case("preview-png-accepted", preview_png)

    def preview_apng():
        kind, _w, _h, animated, error = rules.preview_verdict(apng_bytes(8, 8))
        expect((kind, animated), ("apng", True))
        expect(bool(error), True)

    case("preview-apng-rejected", preview_apng)

    def preview_other_formats():
        for data, kind in ((jpeg_bytes(8, 8), "jpeg"), (SVG_PREVIEW, "svg"),
                           (WEBP_PREVIEW, "webp"), (b"GIF89a" + b"\0" * 8, "gif")):
            found, _w, _h, _a, error = rules.preview_verdict(data)
            expect((found, bool(error)), (kind, True))

    case("preview-other-formats-rejected", preview_other_formats)

    def preview_truncated():
        data = png_bytes(64, 64)
        # 砍掉 IEND：块链不完整。
        _kind, _w, _h, _a, error = rules.preview_verdict(data[:len(data) // 2])
        expect(bool(error), True)
        # 只砍 IEND 的 12 字节：所有块都完整，但缺结束块。
        _kind, _w, _h, _a, error = rules.preview_verdict(data[:-12])
        expect("IEND" in (error or ""), True)
        # 完整 PNG 仍然通过。
        expect(rules.preview_verdict(data)[4], None)

    case("preview-truncated-rejected", preview_truncated)

    def preview_budget():
        side = 2048
        expect(rules.preview_within_budget(side, side, 1024), [])
        expect(bool(rules.preview_within_budget(side + 1, side, 1024)), True)
        expect(bool(rules.preview_within_budget(64, 64,
                                                rules.budget("preview_bytes") + 1)), True)

    case("preview-budget", preview_budget)

    # ---- 字体
    def font_formats():
        ttf = (FONT_DIR / "ProbeSans-Regular.ttf").read_bytes()
        expect(rules.font_verdict(ttf, "ttf"), ("ttf", None))
        expect(rules.font_verdict(ttf, "otf")[1] is not None, True)
        for magic, kind in ((b"ttcf", "ttc"), (b"wOFF", "woff"), (b"wOF2", "woff2")):
            expect(rules.font_verdict(magic + b"\0" * 16, "ttf")[0], kind)
        expect(rules.font_verdict(b"", "ttf")[0], None)

    case("font-formats", font_formats)

    def family_resolution():
        package = {"Aline Probe Sans"}
        builtin = {"GitHub Sans"}
        system = {"Helvetica"}
        expect(rules.resolve_family("Aline Probe Sans", package, builtin, system),
               ("package", "Aline Probe Sans"))
        expect(rules.resolve_family("GitHub Sans", package, builtin, system),
               ("builtin", "GitHub Sans"))
        expect(rules.resolve_family("Helvetica", package, builtin, system),
               ("system", "Helvetica"))
        expect(rules.resolve_family("Missing", package, builtin, system), (None, None))

    case("font-family-resolution", family_resolution)

    def family_chain_order():
        chain = ["Missing", "monospace"]
        expect(rules.family_chain(chain, set(), set(), set()),
               ("platform-role", "monospace"))
        expect(rules.family_chain(["Missing", "Helvetica"], set(), set(), {"Helvetica"}),
               ("system", "Helvetica"))

    case("font-family-chain", family_chain_order)

    def reserved_roles():
        for role in ("system-ui", "sans-serif", "monospace", "Monospace"):
            expect(rules.reserved_family(role), True)
        expect(rules.reserved_family("Aline Probe Sans"), False)

    case("font-reserved-roles", reserved_roles)

    # ---- PL-02
    def pl02_disabled_uses_content():
        result = rules.pl02_surfaces(
            False, True, True, "github", "light", "content", "highlight",
            None, None)
        expect(result["source"], "content-theme")
        expect(result["colors"], "content")
        expect(result["paused"]["familyFollow"], True)

    case("pl02-disabled-content-surface", pl02_disabled_uses_content)

    def pl02_fixed_survives_app_change():
        result = rules.pl02_surfaces(
            True, False, False, "notion", "sepia", "content", "highlight",
            "vue", "dark")
        expect((result["family"], result["mode"]), ("vue", "dark"))
        result = rules.pl02_surfaces(
            True, True, True, "notion", "sepia", "content", "highlight",
            "vue", "dark")
        expect((result["family"], result["mode"]), ("notion", "sepia"))

    case("pl02-fixed-dimensions", pl02_fixed_survives_app_change)

    # ---- 默认配置
    def defaults():
        config = rules.default_config()
        expect(config["family"], "github")
        expect(config["mode"], "follow-system")
        expect(config["installApplies"], False)
        expect(config["previewApplies"], False)
        expect(config["backgroundOpacity"], 1.0)
        expect(sorted(config["reservedFamilies"]), sorted(rules.RESERVED_FAMILIES))

    case("default-config", defaults)

    def baseline_leaves_complete():
        leaves = rules.baseline_requirement()
        expect(sorted(leaves), sorted(rules.DOCUMENT_SCHEMAS))
        for kind, schema in rules.DOCUMENT_SCHEMAS.items():
            # 基线要求必须覆盖该文件的每一个叶：少一个叶就意味着某个属性
            # 可以没有基线值，受保护基线就出现了洞。
            expect(sorted(leaves[kind]), sorted(rules.leaf_paths(schema)))
            expect(sorted(rules.missing_leaves({}, schema)), sorted(leaves[kind]))

    case("baseline-leaves", baseline_leaves_complete)

    # ---- 包样本契约
    def package_cases_registered():
        names = list(package_paths)
        for name in names:
            path_case_verdict(name)
        expect(len(names), len(set(names)))
        expect(all(path.is_file() for path in package_paths.values()), True)
        with zipfile.ZipFile(package_paths["legal"]) as archive:
            expect(sorted(archive.namelist()),
                   sorted(["theme.json", "markdown.json", "application.json",
                           "highlight.json", "previews/light.png",
                           "previews/dark.png", "previews/sepia.png"]))
        # 拒绝样本的名字必须原样落进归档：Python 的 writestr 不折叠路径，
        # 若它偷偷改写了名字，路径类样本就测不到契约。
        with zipfile.ZipFile(package_paths["path-traversal"]) as archive:
            expect("../evil.json" in archive.namelist(), True)

    case("package-cases-registered", package_cases_registered)

    def legal_package_audits_clean():
        listing = _fake_listing(package_paths["legal"])
        expect(audit_package(listing), [])
        with zipfile.ZipFile(package_paths["legal"]) as archive:
            manifest, reasons = audit_manifest_bytes(archive.read("theme.json"))
        expect(reasons, [])
        expect(rules.manifest_violations(manifest), [])

    case("legal-package-audits-clean", legal_package_audits_clean)

    def bomb_audit_rejects():
        listing = _fake_listing(package_paths["decompression-bomb"])
        reasons = audit_package(listing)
        expect(any("压缩比" in reason for reason in reasons), True)

    case("bomb-audit-rejects", bomb_audit_rejects)

    def duplicate_audit_rejects():
        # Python 的 zipfile 保留重名条目，`_fake_listing` 因此能看到“声明 9 条、
        # 唯一名 8 个”的差异；这正是 Rust 侧 `zip` 静默合并时丢掉的信息。
        listing = _fake_listing(package_paths["duplicate-name"])
        expect(listing["entry_count"], listing["unique_name_count"] + 1)
        reasons = audit_package(listing)
        expect(any("重复" in reason for reason in reasons), True)

    case("duplicate-audit-rejects", duplicate_audit_rejects)

    def symlink_audit_rejects():
        listing = _fake_listing(package_paths["symlink"])
        reasons = audit_package(listing)
        expect(any("符号链接" in reason for reason in reasons), True)

    case("symlink-audit-rejects", symlink_audit_rejects)

    def preview_audit_rejects():
        for case_name in ("preview-apng", "preview-svg", "preview-over-pixels"):
            with zipfile.ZipFile(package_paths[case_name]) as archive:
                raw = {info.filename: archive.read(info)
                       for info in archive.infolist()}
            manifest = json.loads(raw["theme.json"])
            expect(bool(audit_previews(manifest, raw)), True)

    case("preview-audit-rejects", preview_audit_rejects)

    def lying_size_audit_rejects():
        # 声明 8192、实际 32 MiB：目录预检看不出来，只有读到才知道。
        listing = _fake_listing(package_paths["lying-declared-size"])
        expect(audit_package(listing), [])  # 声明本身在预算内
        stream = [{"name": "fonts/liar.ttf", "declared_size": 8192,
                   "read_bytes": 32 * 1024 * 1024, "error": None}]
        reasons = audit_package(listing, stream)
        expect(any("声明" in reason for reason in reasons), True)

    case("lying-size-audit-rejects", lying_size_audit_rejects)

    def zip_magic_detects_non_zip():
        expect(_zip_magic(package_paths["legal"]), True)
        expect(_zip_magic(package_paths["not-zip"]), False)

    case("zip-magic-detects-non-zip", zip_magic_detects_non_zip)

    def not_zip_reason_is_ours():
        # 非 ZIP 样本必须由本层签名判定拒绝，而不是转述库的报错文本。
        expect(not _zip_magic(package_paths["not-zip"]), True)
        listing = {"ok": False, "error": "invalid Zip archive: Could not find EOCD"}
        expect(audit_package(listing), ["库无法读取该归档"])

    case("not-zip-reason-is-ours", not_zip_reason_is_ours)

    def unregistered_case_raises():
        _raises(rules.ContractError, lambda: path_case_verdict("no-such-case"))

    case("unregistered-case-raises", unregistered_case_raises)

    def json_cases_boundary():
        work = Path(tempfile.mkdtemp(prefix="aline-m4-v04-selftest-"))
        try:
            cases = dict(_json_cases(work))
            depth = rules.budget("json_depth")
            expect(_depth_of(json.loads(cases["depth-boundary"].read_text())), depth)
            expect(_depth_of(json.loads(cases["depth-over"].read_text())), depth + 1)
            expect(len(json.loads(cases["members-over"].read_text())["common"]),
                   rules.budget("json_members") + 1)
            expect(len(json.loads(cases["array-over"].read_text())["common"]["families"]),
                   rules.budget("json_array_items") + 1)
            expect(cases["bytes-boundary"].stat().st_size,
                   rules.budget("json_bytes"))
            expect(cases["bytes-over"].stat().st_size,
                   rules.budget("json_bytes") + 1)
            # 字节样本只应触发字节这一条预算：成员数远低于上限。
            members = len(json.loads(cases["bytes-boundary"].read_text())["common"])
            expect(members < rules.budget("json_members"), True)
        finally:
            shutil.rmtree(work, ignore_errors=True)

    case("json-cases-boundary", json_cases_boundary)

    def gaps_cover_measured():
        ids = {gap["id"] for gap in capability_gaps()}
        for expected in ("no-face-release", "m-glyph-silent-skip",
                         "reserved-family-hijack", "system-family-shadowing",
                         "no-duplicate-key-rejection", "no-per-image-pixel-cap",
                         "gpui-accepts-all-preview-formats",
                         "zip-normalizes-instead-of-rejecting"):
            expect(expected in ids, True)

    case("gaps-cover-measured", gaps_cover_measured)

    passed = all(case["passed"] for case in cases)
    shutil.rmtree(package_work, ignore_errors=True)
    return {"passed": passed, "cases": cases}


def _depth_of(value):
    if isinstance(value, dict) and value:
        return 1 + max(_depth_of(item) for item in value.values())
    if isinstance(value, list) and value:
        return 1 + max(_depth_of(item) for item in value)
    return 1


def _raises(error_type, function):
    try:
        function()
    except error_type:
        return
    raise AssertionError("期望抛出 %s" % error_type.__name__)


def _fake_listing(path):
    """用 Python 的 zipfile 造出与 `zip list` 同形的目录，仅供自测。"""
    entries = []
    declared_total = 0
    compressed_total = 0
    names = []
    with zipfile.ZipFile(path) as archive:
        for info in archive.infolist():
            name = info.filename
            names.append(name)
            declared_total += info.file_size
            compressed_total += info.compress_size
            entries.append({
                "name": name,
                "is_dir": info.is_dir(),
                "is_symlink": (info.external_attr >> 16) & 0o170000 == 0o120000,
                "declared_size": info.file_size,
                "compressed_size": info.compress_size,
            })
    return {
        "ok": True,
        "entries": entries,
        "entry_count": len(entries),
        "declared_entry_count": len(entries),
        "unique_name_count": len(set(names)),
        "has_overlapping_files": False,
        "declared_total": declared_total,
        "compressed_total": compressed_total,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true",
                        help="只运行规则与比对逻辑自测，不编译 Rust")
    parser.add_argument("--output-dir", help="证据落点，必须在 logs/validation/ 内")
    args = parser.parse_args(argv)

    if args.self_test:
        result = self_test()
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if result["passed"] else 1

    output = output_directory("probe-m4-v04-v06", args.output_dir)
    try:
        summary = real_run(output)
    except (ProbeError, rules.ContractError) as error:
        write_text(output / "failure.log", str(error))
        print("PROBE_FAILED " + str(error), file=sys.stderr)
        return 1
    failed = [row["case"] for row in summary["packages"] if not row["passed"]]
    print("PROBE_OK packages=%d failed=%d json=%d images=%d fonts=%d content=%d output=%s"
          % (len(summary["packages"]), len(failed), len(summary["json"]),
             len(summary["images"]), len(summary["fonts"]), len(summary["content"]),
             output.relative_to(ROOT)))
    if failed:
        print("FAILED_CASES " + " ".join(failed), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
