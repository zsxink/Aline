#!/usr/bin/env python3
"""M4 3.4 链接/图片编辑弹窗与图片右键操作真实窗口取证（IMAGE-03/04/06、AC-05/10）。

被测事实（逐条写入 result.json 的 facts）：
- 入口统一：Cmd+K 与正文右键「新增链接…」打开同一链接弹窗（同标题、同字段）。
- 取消：输入文字与目标后取消，正文、磁盘与历史都不变。
- 确认与撤销：确认插入的链接文字与目标正确；撤销回到插入前，重做恢复，
  与普通编辑共用同一历史节点。
- 未许可协议：`javascript:` 目标被拒绝，弹窗保留并显示「不支持的链接协议」，
  文档不变。
- 文字右键菜单显示「新增链接…」「插入图片…」两项，均可打开对应弹窗。
- 图片右键四项按引用类型与资源可用性门控：
  * 本地可用：四项都可用；复制写入的剪贴板 PNG 与源文件逐字节一致；
    复制路径写入绝对路径；另存为在文档目录写出字节一致的文件；
    打开所在位置在 Finder 打开所在文件夹。
  * 本地文件在渲染后删除：复制/另存为/打开所在位置禁用并显示
    「找不到图片文件」，复制路径仍可用（回车执行的是复制路径）。
  * data: 引用：复制/另存为可用，复制路径/打开所在位置禁用并显示
    「内嵌图片没有文件路径」；回车执行的是复制，剪贴板字节等于内联 PNG。
- 执行失败：菜单打开后删除源文件再复制，显示「复制图片失败」，文档不变。

窗口内输入只经 macOS 无障碍接口（orca computer）投递；菜单原因行与状态
反馈用系统 Vision 识别读取（AX 树不含普通文本节点）。正文图片预算由
`ALINE_M4_MEDIA_BUDGET` 显式注入（V-06 数值未冻结，生产默认不注入）。

用法：drive_m4_insert_context.py --evidence <证据目录>
"""
import argparse
import base64
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from PIL import Image, ImageFilter

sys.path.insert(0, str(Path(__file__).resolve().parent))

import drive_m2_editable_window as native  # noqa: E402  复用同一真实输入驱动
from drive_m2_find_modes import activate, require_fresh  # noqa: E402
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m4_image_insert import (  # noqa: E402
    clipboard_png_readback,
    clipboard_text,
    distinct_texts,
    open_document_robust,
    restore_clipboard,
    session_rows,
)
from drive_m4_image_preview import (  # noqa: E402
    COLORS,
    MIN_SATURATION,
    TOLERANCE,
    color_mask,
    compile_swift,
    latest_shot,
    ocr_lines,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_BINARY = native.ROOT / "target" / "debug" / "aline"
READ_TEXT_SOURCE = native.ROOT / "scripts" / "fixtures" / "read_text.swift"
BUDGET_ENV = "ALINE_M4_MEDIA_BUDGET"
BUDGET = "1048576,16777216,268435456,8"

IMAGE_SIZE = (64, 48)
LOCAL_COLOR = COLORS["magenta"]
INLINE_COLOR = COLORS["cyan"]
MENU_PANEL_WIDTH = 220.0
MENU_ITEM_HEIGHT = 30.0
MENU_PANEL_PADDING = 4.0
CONTENT_TOP = 80.0
CONTENT_BOTTOM_MARGIN = 30.0

# 工具栏品牌常驻「Aline」，链接文字必须用窗口里只在插入后出现的标记。
LINK_TEXT = "ZuluLink"
LINK_TARGET = "https://example.test"
LINK_MARKUP = "[ZuluLink](https://example.test)"
REJECTED_TARGET = "javascript:alert(1)"

TEXT_MENU = ["新增链接", "插入图片"]
IMAGE_MENU = ["复制图片", "另存为", "复制路径", "打开所在位置"]
# OCR 只按前缀匹配：省略号与标点在识别结果里不稳定。
TEXT_MENU_OCR = ["新增链接", "插入图片"]
IMAGE_MENU_OCR = ["复制图片", "另存为", "复制路径", "打开所在"]
REASON_INLINE = "内嵌图片没有文件路径"
REASON_MISSING = "找不到图片文件"

DIALOG_FIELDS = {
    "link": ("显示文字", "网址或路径"),
    "image": ("替代文字（留空时使用目标）", "图片路径或网址"),
}
DIALOG_TITLES = {"link": "新增链接", "image": "插入图片"}
DIALOG_CONFIRM = "确认"
SAVE_PANEL_NAMES = ("Save", "存储", "保存", "另存为")

BASE_TEXT = "Alpha beta gamma.\n\n![dot](dot.png)\n\n![inline]({data})\n"

POLL = 0.25


def _assert(condition, note):
    if not condition:
        raise native.DriverError(note)


def flat(text):
    return re.sub(r"\s+", "", text)


def ocr_matches(text, phrase):
    """OCR 容错包含：忽略大小写与空白，并去掉结构助词「的」。

    run-insert-context-1 把「Alpha」读成「ALpha」（小写 l 认成大写 L），
    据此大小写不能作为断言依据；Vision 也偶发漏读「的」（run-preview-9）。
    """
    return (phrase.replace("的", "").lower()
            in text.replace("的", "").lower())


def build_fixture(root):
    """确定性生成文档、本地图片与 data: 引用的 PNG 字节。

    文档直接放在夹具根目录：文件面板只列到夹具根一层，列数与
    `Shared` 下同级的 preview 夹具一致（run-preview-5/6 的截断教训）。
    """
    local_png = root / "dot.png"
    Image.new("RGB", IMAGE_SIZE, LOCAL_COLOR).save(local_png, format="PNG")
    inline_png = root / "inline.png"
    Image.new("RGB", IMAGE_SIZE, INLINE_COLOR).save(inline_png, format="PNG")
    data_url = "data:image/png;base64," + base64.b64encode(
        inline_png.read_bytes()).decode("ascii")
    document = root / "notes.md"
    document.write_text(BASE_TEXT.format(data=data_url), encoding="utf-8")
    return document, local_png, inline_png, data_url


# ---------------------------------------------------------------- 观察工具

def ocr_window(driver, name):
    """识别最新整窗截图；返回去空白后的文本并落证据。"""
    driver.snapshot()
    shot = latest_shot(driver)
    lines = ocr_lines(driver.ocr, shot)
    write_text(driver.evidence / ("ocr-" + name + ".txt"), "\n".join(lines))
    return flat("\n".join(lines))


def wait_tree(driver, predicate, note, timeout=12.0):
    """轮询 AX 树直到谓词成立；超时把最后一棵树落盘后失败。"""
    deadline = time.monotonic() + timeout
    tree = ""
    while time.monotonic() < deadline:
        tree = driver.tree_text(driver.snapshot())
        if predicate(tree):
            return tree
        time.sleep(POLL)
    write_text(driver.evidence / "wait-failure.txt", tree)
    raise native.DriverError("等待超时：" + note)


def wait_ocr(driver, phrase, note, name, timeout=12.0):
    """轮询整窗 OCR 直到包含短语；返回最后的识别文本。"""
    deadline = time.monotonic() + timeout
    text = ""
    while time.monotonic() < deadline:
        text = ocr_window(driver, name)
        if ocr_matches(text, phrase):
            return text
        time.sleep(POLL)
    write_text(driver.evidence / "wait-failure.txt", text)
    raise native.DriverError("等待超时：" + note)


def wait_ocr_absent(driver, phrase, note, name, timeout=12.0):
    """轮询整窗 OCR 直到不再包含短语。"""
    deadline = time.monotonic() + timeout
    text = ""
    while time.monotonic() < deadline:
        text = ocr_window(driver, name)
        if not ocr_matches(text, phrase):
            return text
        time.sleep(POLL)
    write_text(driver.evidence / "wait-failure.txt", text)
    raise native.DriverError("等待超时：" + note)


def wait_clipboard_png(evidence, expected, timeout=10.0):
    """轮询剪贴板 PNG 直到与期望字节一致；返回 (字节, 最后错误)。"""
    deadline = time.monotonic() + timeout
    readback, error = None, "尚未读取"
    while time.monotonic() < deadline:
        readback, error = clipboard_png_readback(evidence)
        if readback == expected:
            return readback, None
        time.sleep(0.3)
    return readback, error


def wait_clipboard_text(expected, timeout=10.0):
    """轮询文本剪贴板直到等于期望值。"""
    deadline = time.monotonic() + timeout
    text = ""
    while time.monotonic() < deadline:
        text = clipboard_text().strip()
        if text == expected:
            return text
        time.sleep(0.3)
    return text


# ---------------------------------------------------------------- 输入投递

def press(driver, key, note):
    driver.call(["press-key", "--key", key], note)
    time.sleep(0.35)


def hotkey(driver, key, note):
    driver.call(["hotkey", "--key", key], note)
    time.sleep(0.6)


def type_text(driver, text, note):
    driver.call(["type-text", "--text", text], note)
    time.sleep(0.3)


def right_click(driver, x, y, note):
    """右键必须前台发送：Orca 认为右键会弹菜单接管键盘，后台一律拒绝。

    用户已于 2026-11-27 同意本任务取证使用 --restore-window；Aline 窗口会
    在每次右键时置前，运行结束后由脚本关闭。
    """
    driver.call(["click", "--x", str(round(x, 1)), "--y", str(round(y, 1)),
                 "--mouse-button", "right", "--restore-window"], note)
    time.sleep(0.7)


def click_element(driver, index, note):
    driver.call(["click", "--element-index", str(index), "--restore-window"],
                note)
    time.sleep(0.7)


def color_point(driver, color, name):
    """按截图颜色定位图片中心，返回窗口逻辑点。"""
    snapshot = driver.snapshot()
    window = snapshot.get("window") or {}
    _assert(window.get("width"), "缺少窗口尺寸，无法换算图片位置")
    shot = latest_shot(driver)
    image = Image.open(shot).convert("RGB")
    width, height = image.size
    scale = width / float(window["width"])
    top = int(CONTENT_TOP * scale)
    bottom = max(top, height - int(CONTENT_BOTTOM_MARGIN * scale))
    raw = color_mask(image, color, TOLERANCE, MIN_SATURATION).crop(
        (0, top, width, bottom))
    # 插入链接后，蓝色链接文字与文本光标也落在青色的容差内；先做 5×5
    # 腐蚀去掉细笔画，只留下 64×48pt 的实心图片块，再取包围盒。
    solid = raw.filter(ImageFilter.MinFilter(5))
    box = solid.getbbox()
    method = "eroded"
    if box is None:
        box = raw.getbbox()
        method = "raw"
    _assert(box is not None, "截图里找不到颜色夹具：" + name)
    absolute = (box[0], box[1] + top, box[2], box[3] + top)
    point = ((absolute[0] + absolute[2]) / 2 / scale,
             (absolute[1] + absolute[3]) / 2 / scale)
    raw_box = raw.getbbox()
    write_json(driver.evidence / f"image-point-{name}.json", {
        "color": list(color),
        "bbox_px": list(absolute),
        "raw_bbox_px": list(raw_box) if raw_box else None,
        "method": method,
        "scale": round(scale, 4),
        "point": [round(point[0], 2), round(point[1], 2)],
        "note": "按夹具色掩码定位正文图片中心（窗口逻辑点；腐蚀去掉细笔画干扰）",
    })
    return point


# ---------------------------------------------------------------- 右键菜单

def ax_menu_items(tree, expected=()):
    """读 AX 树里的菜单项；返回 {标签: 索引}。

    GPUI 菜单项用 Role::MenuItem；若宿主把它序列化成 button，只接受标签
    以预期动作开头的行，避免把工具栏按钮算成菜单项。
    """
    items = {}
    for line in tree.splitlines():
        match = re.match(r'^\s*(\d+) (menu item|button)(.*)$', line)
        if not match:
            continue
        role = match.group(2)
        name = match.group(3).strip()
        if name.startswith("(disabled)"):
            name = name[len("(disabled)"):].strip()
        if not name:
            continue
        if role == "button" and not any(name.startswith(label)
                                        for label in expected):
            continue
        items.setdefault(name, int(match.group(1)))
    return items


def pick_item(items, label):
    for name, index in items.items():
        if name.startswith(label):
            return index
    return None


def menu_snapshot(driver):
    snapshot = driver.snapshot()
    tree = driver.tree_text(snapshot)
    return snapshot, tree


def open_menu(driver, point, labels, ocr_labels, name):
    """右键打开菜单并等待全部标签出现（AX 优先，OCR 兜底）。"""
    press(driver, "Escape", "先关闭可能残留的菜单")
    right_click(driver, point[0], point[1], "在目标上右键：" + name)
    deadline = time.monotonic() + 3.0
    tree = ""
    while time.monotonic() < deadline:
        _, tree = menu_snapshot(driver)
        if all(label in tree for label in labels):
            write_text(driver.evidence / f"menu-{name}-tree.txt", tree)
            return {"method": "ax", "tree": tree, "text": ""}
        time.sleep(POLL)
    deadline = time.monotonic() + 6.0
    while time.monotonic() < deadline:
        text = ocr_window(driver, "menu-" + name)
        if all(ocr_matches(text, label) for label in ocr_labels):
            _, tree = menu_snapshot(driver)
            write_text(driver.evidence / f"menu-{name}-tree.txt", tree)
            return {"method": "ocr", "tree": tree, "text": text}
        time.sleep(0.6)
    write_text(driver.evidence / "wait-failure.txt", tree)
    raise native.DriverError("右键菜单没有出现：" + name)


def menu_item_point(snapshot, anchor, labels, label):
    """按菜单渲染几何换算菜单项中心（备用点击路径）。

    面板宽 220、内边距 4，每项约 30 高；打开位置取右键点并按窗口夹取，
    与 `render_context_menu` 的定位算式一致。
    """
    window = snapshot.get("window") or {}
    width = float(window.get("width") or 0.0)
    height = float(window.get("height") or 0.0)
    _assert(width > 0.0 and height > 0.0, "缺少窗口尺寸，无法换算菜单项位置")
    left = min(anchor[0], max(0.0, width - MENU_PANEL_WIDTH))
    top = min(anchor[1], max(0.0, height - (len(labels) * MENU_ITEM_HEIGHT
                                            + 2 * MENU_PANEL_PADDING)))
    index = labels.index(label)
    return (left + MENU_PANEL_WIDTH / 2,
            top + MENU_PANEL_PADDING + MENU_ITEM_HEIGHT * index
            + MENU_ITEM_HEIGHT / 2)


def menu_click(driver, label, labels, anchor, name):
    """点击菜单项；AX 标签可用时点元素，否则按几何坐标点击。"""
    snapshot, tree = menu_snapshot(driver)
    items = ax_menu_items(tree, labels)
    index = pick_item(items, label)
    if index is not None:
        click_element(driver, index, "点击菜单项：" + label)
        method = "ax"
    else:
        x, y = menu_item_point(snapshot, anchor, labels, label)
        driver.call(["click", "--x", str(round(x, 1)), "--y", str(round(y, 1)),
                     "--restore-window"],
                    "按几何位置点击菜单项：" + label)
        time.sleep(0.7)
        method = "coordinate"
    write_json(driver.evidence / f"menu-click-{name}.json", {
        "label": label,
        "method": method,
        "ax_items": items,
        "note": "点击右键菜单项；AX 标签缺失时按面板几何换算坐标",
    })
    return method


def menu_reasons(text, reasons):
    return {reason: ocr_matches(text, reason) for reason in reasons}


def menu_proof(driver, labels, ocr_labels, name):
    """菜单标签的两路证据：AX 菜单项优先，否则退回 OCR 文本。"""
    _, tree = menu_snapshot(driver)
    items = ax_menu_items(tree, labels)
    text = ocr_window(driver, name)
    proof = {
        "ax_items": items,
        "ocr_text": text,
        "method": "ax" if items else "ocr",
        "labels_found": {label: (pick_item(items, label) is not None
                                 if items else ocr_matches(text, label))
                         for label in labels},
        "extras": [entry for entry in items
                   if not any(entry.startswith(label) for label in labels)],
        "note": "菜单标签核对：AX 暴露菜单项时按标签与索引，否则按整窗 OCR 文本",
    }
    write_json(driver.evidence / f"menu-proof-{name}.json", proof)
    _assert(all(proof["labels_found"].values()),
            "菜单标签不全：" + json.dumps(proof, ensure_ascii=False))
    return proof


# ---------------------------------------------------------------- 弹窗

def dialog_fields_in_tree(tree, kind):
    first, second = DIALOG_FIELDS[kind]
    return first in tree and second in tree


def field_index(tree, label):
    """按占位提示定位弹窗文本字段。"""
    for line in tree.splitlines():
        match = re.match(r'^\s*(\d+) text field\b(.*)$', line)
        if match and label in match.group(2):
            return int(match.group(1))
    return None


def dialog_visible(driver, kind, name, timeout=12.0):
    """弹窗可见：AX 字段优先，否则整窗 OCR 的标题 + 确认按钮。"""
    title = DIALOG_TITLES[kind]
    deadline = time.monotonic() + timeout
    tree = ""
    text = ""
    while time.monotonic() < deadline:
        tree = driver.tree_text(driver.snapshot())
        if dialog_fields_in_tree(tree, kind):
            return {"method": "ax-fields", "tree": tree, "text": ""}
        text = ocr_window(driver, name)
        if (ocr_matches(text, title)
                and ocr_matches(text, DIALOG_CONFIRM)):
            return {"method": "ocr", "tree": tree, "text": text}
        time.sleep(POLL)
    write_text(driver.evidence / "wait-failure.txt", tree + "\n" + text)
    raise native.DriverError("弹窗没有出现：" + title)


def wait_dialog_closed(driver, kind, name, timeout=12.0):
    """弹窗关闭：AX 字段与 OCR 标题同时消失。"""
    title = DIALOG_TITLES[kind]
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        tree = driver.tree_text(driver.snapshot())
        text = ocr_window(driver, name)
        fields_gone = not dialog_fields_in_tree(tree, "link") \
            and not dialog_fields_in_tree(tree, "image")
        if fields_gone and not ocr_matches(text, title):
            return
        time.sleep(POLL)
    raise native.DriverError("弹窗没有关闭：" + title)


def fill_field(driver, label, text, note):
    tree = driver.tree_text(driver.snapshot())
    index = field_index(tree, label)
    _assert(index is not None, "弹窗里找不到字段：" + label)
    click_element(driver, index, "聚焦字段：" + label)
    hotkey(driver, "cmd+a", "全选字段内容：" + label)
    type_text(driver, text, note)
    wait_tree(driver, lambda t: text in t, "字段值已写入：" + label)


def fill_dialog(driver, kind, text, target, name):
    """填两个字段：AX 字段可定位时点选；否则用开场焦点 + Tab 键盘路径。"""
    first, second = DIALOG_FIELDS[kind]
    tree = driver.tree_text(driver.snapshot())
    if field_index(tree, first) is not None and field_index(tree, second) is not None:
        fill_field(driver, first, text, "输入文字字段")
        fill_field(driver, second, target, "输入目标字段")
        method = "ax-fields"
    else:
        # 打开时焦点已在文字字段；Tab 走 move_insert_dialog_focus 到目标字段。
        hotkey(driver, "cmd+a", "全选文字字段预填内容")
        type_text(driver, text, "输入文字字段")
        press(driver, "Tab", "Tab 移到目标字段")
        hotkey(driver, "cmd+a", "全选目标字段内容")
        type_text(driver, target, "输入目标字段")
        wait_tree(driver, lambda t: target in t, "目标字段值已写入")
        method = "keyboard-tab"
    write_json(driver.evidence / f"dialog-fill-{name}.json", {
        "kind": kind,
        "text": text,
        "target": target,
        "method": method,
        "note": "字段填写的实际路径（AX 点选或键盘 Tab）",
    })


def open_link_dialog_shortcut(driver):
    hotkey(driver, "cmd+k", "Cmd+K 打开链接弹窗")
    return dialog_visible(driver, "link", "dialog-link")


def open_link_dialog_menu(driver):
    open_menu(driver, driver.heading_point(), TEXT_MENU, TEXT_MENU_OCR, "text")
    menu_click(driver, "新增链接", TEXT_MENU, driver.heading_point(), "add-link")
    return dialog_visible(driver, "link", "dialog-link-via-menu")


# ---------------------------------------------------------------- 用例

def case_cancel(driver, evidence):
    """取消：输入文字与目标后取消，正文与历史都不变。"""
    opened = open_link_dialog_shortcut(driver)
    fill_dialog(driver, "link", LINK_TEXT, LINK_TARGET + "/cancel", "cancel")
    press(driver, "Escape", "Escape 取消弹窗")
    wait_dialog_closed(driver, "link", "dialog-after-cancel")
    body = ocr_window(driver, "body-after-cancel")
    _assert(not ocr_matches(body, LINK_TEXT),
            "取消后正文不应出现链接文字：" + body[:400])
    write_json(evidence / "case-cancel.json", {
        "dialog_open_method": opened["method"],
        "dialog_title": DIALOG_TITLES["link"],
        "fields": list(DIALOG_FIELDS["link"]),
        "body_after_cancel_has_link_text": ocr_matches(body, LINK_TEXT),
        "note": "输入文字与目标后取消；正文截图不含输入文字，历史不变由关闭后的会话采样复核",
    })


def case_confirm_undo_redo(driver, evidence):
    """确认插入链接，随后撤销/重做。"""
    open_link_dialog_shortcut(driver)
    fill_dialog(driver, "link", LINK_TEXT, LINK_TARGET, "confirm")
    press(driver, "Return", "回车确认插入")
    wait_dialog_closed(driver, "link", "dialog-after-confirm")
    text = wait_ocr(driver, LINK_TEXT, "插入后正文出现链接文字", "body-after-confirm")
    _assert(ocr_matches(text, "Alpha"), "插入后正文仍应保留原文：" + text[:400])
    hotkey(driver, "cmd+z", "撤销插入")
    after_undo = wait_ocr_absent(driver, LINK_TEXT, "撤销后链接文字消失",
                                 "body-after-undo")
    _assert(ocr_matches(after_undo, "Alpha"), "撤销后正文应保留原文："
            + after_undo[:400])
    hotkey(driver, "cmd+shift+z", "重做插入")
    wait_ocr(driver, LINK_TEXT, "重做后链接文字恢复", "body-after-redo")
    write_json(evidence / "case-confirm-undo.json", {
        "inserted_text": LINK_TEXT,
        "target": LINK_TARGET,
        "markup": LINK_MARKUP,
        "note": "确认插入→撤销→重做；截图逐段核对，精确文本与历史深度由会话采样复核",
    })


def case_rejected_scheme(driver, evidence):
    """未许可协议：确认被拒绝并显示原因，文档不变。"""
    open_link_dialog_shortcut(driver)
    fill_dialog(driver, "link", "Bad", REJECTED_TARGET, "rejected")
    press(driver, "Return", "回车触发拒绝")
    text = wait_ocr(driver, "不支持的链接协议", "弹窗显示拒绝原因", "dialog-rejected")
    tree = driver.tree_text(driver.snapshot())
    stayed = (dialog_fields_in_tree(tree, "link")
              or (ocr_matches(text, DIALOG_TITLES["link"])
                  and ocr_matches(text, DIALOG_CONFIRM)))
    _assert(stayed, "拒绝后弹窗应保持打开以便修改")
    press(driver, "Escape", "关闭拒绝后的弹窗")
    wait_dialog_closed(driver, "link", "dialog-rejected-closed")
    body = ocr_window(driver, "body-after-rejected")
    _assert(not ocr_matches(body, "Bad"), "拒绝后正文不应出现输入文字："
            + body[:400])
    write_json(evidence / "case-rejected.json", {
        "target": REJECTED_TARGET,
        "message": "不支持的链接协议",
        "dialog_stayed_open": stayed,
        "note": "确认后弹窗保留并显示原因；正文截图不含输入文字",
    })


def case_text_menu(driver, evidence):
    """文字右键两项；右键「新增链接…」与 Cmd+K 打开同一弹窗。"""
    point = driver.heading_point()
    open_menu(driver, point, TEXT_MENU, TEXT_MENU_OCR, "text")
    proof = menu_proof(driver, TEXT_MENU, TEXT_MENU_OCR, "text")

    menu_click(driver, "新增链接", TEXT_MENU, point, "add-link")
    opened = dialog_visible(driver, "link", "dialog-link-via-menu")
    press(driver, "Escape", "关闭右键入口的链接弹窗")
    wait_dialog_closed(driver, "link", "dialog-link-via-menu-closed")

    open_menu(driver, driver.heading_point(), TEXT_MENU, TEXT_MENU_OCR, "text")
    menu_click(driver, "插入图片", TEXT_MENU, driver.heading_point(), "insert-image")
    dialog_visible(driver, "image", "dialog-image")
    press(driver, "Escape", "关闭图片弹窗")
    wait_dialog_closed(driver, "image", "dialog-image-closed")
    write_json(evidence / "case-text-menu.json", {
        "items": TEXT_MENU,
        "proof_method": proof["method"],
        "extras": proof["extras"],
        "link_dialog_title": DIALOG_TITLES["link"],
        "link_dialog_fields": list(DIALOG_FIELDS["link"]),
        "right_click_entry": opened["method"],
        "note": "两个菜单项分别打开链接/图片弹窗；链接弹窗与 Cmd+K 的标题和字段一致",
    })
    return proof


def case_data_image(driver, evidence, inline_png):
    """data: 引用的图片菜单：复制/另存为可用，复制路径/打开所在位置禁用。"""
    point = color_point(driver, INLINE_COLOR, "inline")
    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "inline")
    proof = menu_proof(driver, IMAGE_MENU, IMAGE_MENU_OCR, "inline")
    text = proof["ocr_text"]
    reasons = menu_reasons(text, [REASON_INLINE, REASON_MISSING])
    write_json(evidence / "data-menu-reasons.json", {
        "reasons": reasons,
        "ocr_text": text,
        "note": "data: 引用只应显示「内嵌图片没有文件路径」",
    })
    _assert(reasons[REASON_INLINE], "data: 菜单缺少「内嵌图片没有文件路径」")
    _assert(not reasons[REASON_MISSING], "data: 菜单不应出现「找不到图片文件」")

    # 回车执行当前高亮项；四项里第一个可用项应是「复制图片」。
    press(driver, "Return", "回车执行菜单第一可用项")
    expected = inline_png.read_bytes()
    readback, error = wait_clipboard_png(evidence, expected)
    notice = wait_ocr(driver, "已复制图片", "状态栏显示复制成功", "feedback-inline-copy")
    equal = readback == expected
    write_json(evidence / "inline-copy.json", {
        "source_bytes": len(expected),
        "clipboard_bytes": len(readback) if readback is not None else None,
        "equal": equal,
        "error": error,
        "notice_ocr_has_copy_done": ocr_matches(notice, "已复制图片"),
        "note": "data: 图片复制的剪贴板 PNG 应与内联 PNG 逐字节一致",
    })
    _assert(readback is not None, "剪贴板没有 PNG 数据：" + str(error))
    _assert(equal, "剪贴板 PNG 与内联 PNG 不一致")
    write_json(evidence / "case-data-image.json", {
        "reason": REASON_INLINE,
        "enabled": ["复制图片", "另存为"],
        "disabled": ["复制路径", "打开所在位置"],
        "enter_ran": "复制图片",
        "proof_method": proof["method"],
        "note": "门控由原因行、菜单标签与回车执行结果共同核对",
    })
    return proof


def case_local_actions(driver, evidence, local_png):
    """本地可用图片：复制、复制路径、另存为、打开所在位置。"""
    point = color_point(driver, LOCAL_COLOR, "local")
    source_bytes = local_png.read_bytes()
    local_path = str(local_png)
    methods = {}

    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "local-copy")
    text = ocr_window(driver, "menu-local-copy")
    reasons = menu_reasons(text, [REASON_INLINE, REASON_MISSING])
    _assert(not any(reasons.values()), "本地可用图片不应出现禁用原因：" + text[:400])
    menu_click(driver, "复制图片", IMAGE_MENU, point, "copy")
    readback, error = wait_clipboard_png(evidence, source_bytes)
    wait_ocr(driver, "已复制图片", "状态栏显示复制成功", "feedback-copy")
    equal = readback == source_bytes
    write_json(evidence / "local-copy.json", {
        "source_bytes": len(source_bytes),
        "clipboard_bytes": len(readback) if readback is not None else None,
        "equal": equal,
        "error": error,
        "note": "本地图片复制的剪贴板 PNG 与源文件逐字节比较",
    })
    _assert(readback is not None, "剪贴板没有 PNG 数据：" + str(error))
    _assert(equal, "剪贴板 PNG 与源文件不一致")

    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "local-copy-path")
    methods["copy_path"] = menu_click(driver, "复制路径", IMAGE_MENU, point,
                                      "copy-path")
    copied = wait_clipboard_text(local_path)
    wait_ocr(driver, "已复制图片路径", "状态栏显示复制路径成功", "feedback-copy-path")
    write_json(evidence / "copy-path.json", {
        "clipboard": copied,
        "expected": local_path,
        "equal": copied == local_path,
        "note": "复制路径写入引用解析后的绝对路径",
    })
    _assert(copied == local_path, "剪贴板路径不符：" + copied)

    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "local-save-as")
    methods["save_as"] = menu_click(driver, "另存为", IMAGE_MENU, point, "save-as")
    panel_method = wait_save_panel(driver, local_png.name)
    panel_text = ocr_window(driver, "save-panel")
    _assert(ocr_matches(panel_text, local_png.name),
            "另存为面板没有预填原文件名：" + panel_text[:400])
    input_path = type_save_name(driver, local_png.name, "dot-copy.png")
    target = local_png.parent / "dot-copy.png"
    deadline = time.monotonic() + 15.0
    while time.monotonic() < deadline and not target.is_file():
        time.sleep(0.3)
    saved = target.read_bytes() if target.is_file() else b""
    write_json(evidence / "save-as.json", {
        "suggested_name": local_png.name,
        "typed_name": "dot-copy.png",
        "panel_method": panel_method,
        "input_path": input_path,
        "exists": target.is_file(),
        "bytes": len(saved),
        "equal": saved == source_bytes,
        "note": "另存为在文档目录写出与源文件字节一致的文件",
    })
    _assert(target.is_file(), "另存为没有写出文件")
    _assert(saved == source_bytes, "另存为文件字节与源文件不一致")
    wait_ocr(driver, "图片已另存为", "状态栏显示另存成功", "feedback-save-as")

    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "local-reveal")
    methods["reveal"] = menu_click(driver, "打开所在位置", IMAGE_MENU, point,
                                   "reveal")
    folder = local_png.parent.name
    found = poll_finder_window(folder)
    write_json(evidence / "reveal.json", {
        "folder": folder,
        "finder_window_found": found,
        "note": "reveal 无返回值；以 Finder 是否出现同名窗口作为可见结果",
    })
    if found:
        close_finder_window(folder)
    activate(driver)
    write_json(evidence / "case-local-image.json", {
        "copy": "剪贴板 PNG 与源文件逐字节一致",
        "copy_path": "剪贴板为绝对路径",
        "save_as": "文档目录写出 dot-copy.png 且字节一致",
        "reveal": "Finder 出现夹具目录窗口" if found else "未观察到 Finder 窗口（见 limits）",
        "click_methods": methods,
    })
    return {"reveal_found": found, "save_panel_method": panel_method}


def wait_save_panel(driver, suggested, timeout=10.0):
    """等待另存为面板出现；返回观察路径。

    面板是系统 NSSavePanel：orca 的应用快照可能只覆盖主窗口，因此
    同时用 System Events 读应用的窗口名，再用整窗 OCR 兜底。
    """
    deadline = time.monotonic() + timeout
    tree = ""
    while time.monotonic() < deadline:
        tree = driver.tree_text(driver.snapshot())
        if suggested in tree:
            write_text(driver.evidence / "save-panel-tree.txt", tree)
            write_json(driver.evidence / "save-panel.json", {
                "found": True,
                "method": "ax-tree",
                "note": "orca 快照里已出现另存为面板与原文件名",
            })
            return "ax-tree"
        window = save_panel_window()
        if window is not None:
            write_json(driver.evidence / "save-panel.json", {
                "found": True,
                "method": "system-events",
                "window": window,
                "note": "System Events 读到另存为面板窗口",
            })
            return "system-events"
        time.sleep(POLL)
    text = ocr_window(driver, "save-panel")
    if ocr_matches(text, suggested):
        write_json(driver.evidence / "save-panel.json", {
            "found": True,
            "method": "ocr",
            "note": "整窗识别读到另存为面板与原文件名",
        })
        return "ocr"
    write_text(driver.evidence / "wait-failure.txt", tree + "\n" + text)
    raise native.DriverError("另存为面板没有出现")


def save_panel_window():
    result = subprocess.run(
        ["osascript", "-e",
         'tell application "System Events" to tell process "aline" '
         'to get name of every window'],
        text=True, capture_output=True)
    if result.returncode != 0:
        return None
    for name in result.stdout.replace("\n", ",").split(","):
        name = name.strip()
        if name and any(token in name for token in SAVE_PANEL_NAMES):
            return name
    return None


def wait_save_panel_closed(driver, timeout=5.0):
    """轮询快照窗口标题，等待另存为面板关闭。

    面板打开时快照的窗口标题是面板名（run-3 为 ``Save``）；面板关闭后
    快照回到文档主窗口。窗口标题每次都是新读的，不受树的增量缓存影响。
    """
    deadline = time.monotonic() + timeout
    title = ""
    while time.monotonic() < deadline:
        snapshot = driver.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        if title and not any(token in title for token in SAVE_PANEL_NAMES):
            write_json(driver.evidence / "save-panel-closed.json", {
                "window_title": title,
                "note": "另存为面板关闭后主窗口重新成为快照窗口",
            })
            return True
        time.sleep(POLL)
    write_json(driver.evidence / "save-panel-still-open.json", {
        "window_title": title,
        "note": "等待另存为面板关闭超时",
    })
    return False


def type_save_name(driver, suggested, name):
    """在另存为面板写入文件名并触发保存；返回实际走的输入路径。

    NSSavePanel 打开时文件名字段已聚焦；run-3 证明对字段做 AX 点击会清空
    焦点（其后的 cmd+a/type-text 全部丢失、回车无效）。改用 AX set-value
    写值，再按回车触发默认按钮；面板未关闭时退回点击 Save 按钮。
    """
    tree = driver.tree_text(driver.snapshot())
    match = re.search(r'^\s*(\d+) text field\b.*' + re.escape(suggested),
                      tree, re.M)
    if match:
        index = int(match.group(1))
        payload = driver.call(["set-value", "--element-index", str(index),
                               "--value", name], "写入另存文件名")
        write_json(driver.evidence / "save-panel-set-value.json", {
            "element_index": index,
            "value": name,
            "verification": payload.get("verification"),
            "note": "用 AX set-value 写入文件名，避免点击字段导致失焦",
        })
        time.sleep(0.4)
        press(driver, "Return", "回车确认另存为")
        if not wait_save_panel_closed(driver, timeout=5.0):
            button = re.search(r'^\s*(\d+) button (?:Save|存储|保存)\b',
                               tree, re.M)
            if button:
                click_element(driver, int(button.group(1)),
                              "面板未关闭，点击保存按钮")
                wait_save_panel_closed(driver, timeout=5.0)
        return "ax-set-value"
    script = (
        'tell application "System Events" to tell process "aline"\n'
        'keystroke "a" using command down\n'
        'keystroke "%s"\n'
        'key code 36\n'
        'end tell' % name
    )
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / "save-panel-keys.json", {
        "script": script,
        "exit_code": result.returncode,
        "stderr": result.stderr[-400:],
        "note": "面板不在 orca 快照内时用 System Events 改文件名并回车",
    })
    time.sleep(1.0)
    return "system-events"


def poll_finder_window(name, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = subprocess.run(
            ["osascript", "-e",
             'tell application "Finder" to get name of every window'],
            text=True, capture_output=True)
        if result.returncode == 0 and name in result.stdout:
            return True
        time.sleep(0.5)
    return False


def close_finder_window(name):
    """只关闭与夹具目录同名的 Finder 窗口，不动用户已有窗口。"""
    subprocess.run(
        ["osascript", "-e",
         'tell application "Finder" to close (every window whose name is "%s")' % name],
        text=True, capture_output=True)


def case_copy_failure_and_missing(driver, evidence, local_png):
    """执行失败反馈；删除源文件后的菜单门控。"""
    point = color_point(driver, LOCAL_COLOR, "before-delete")
    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "before-delete")
    before = clipboard_text()
    local_png.unlink()
    menu_click(driver, "复制图片", IMAGE_MENU, point, "copy-failure")
    wait_ocr(driver, "复制图片失败", "状态栏显示复制失败", "feedback-copy-failed")
    after = clipboard_text()
    write_json(evidence / "copy-failure.json", {
        "clipboard_unchanged": before == after,
        "note": "菜单打开后删除源文件再复制：读取失败给出反馈，不写剪贴板",
    })
    _assert(before == after, "复制失败不应改写剪贴板")

    open_menu(driver, point, IMAGE_MENU, IMAGE_MENU_OCR, "missing")
    proof = menu_proof(driver, IMAGE_MENU, IMAGE_MENU_OCR, "missing")
    text = proof["ocr_text"]
    reasons = menu_reasons(text, [REASON_INLINE, REASON_MISSING])
    write_json(evidence / "missing-menu.json", {
        "proof": proof,
        "reasons": reasons,
        "note": "本地文件删除后的图片菜单（图片仍在渲染缓存中）",
    })
    _assert(reasons[REASON_MISSING], "缺失资源菜单缺少「找不到图片文件」")
    _assert(not reasons[REASON_INLINE], "缺失资源菜单不应出现内嵌原因")

    # 缺失状态下第一个可用项应是「复制路径」：回车应复制路径而不是图片。
    press(driver, "Return", "回车执行菜单第一可用项")
    copied = wait_clipboard_text(str(local_png))
    wait_ocr(driver, "已复制图片路径", "缺失资源下回车执行复制路径",
             "feedback-missing-enter")
    write_json(evidence / "missing-enter.json", {
        "clipboard": copied,
        "expected": str(local_png),
        "equal": copied == str(local_png),
        "note": "复制/另存为/打开所在位置禁用后，菜单高亮的第一可用项是复制路径",
    })
    _assert(copied == str(local_png), "缺失资源下回车没有复制路径：" + copied)
    write_json(evidence / "case-copy-failure.json", {
        "feedback": "复制图片失败",
        "missing_reason": REASON_MISSING,
        "enabled": ["复制路径"],
        "disabled": ["复制图片", "另存为", "打开所在位置"],
        "proof_method": proof["method"],
        "note": "执行失败有反馈且不改文档；缺失时只有复制路径可用",
    })
    return proof


# ---------------------------------------------------------------- 关闭后核对

def verify(evidence, document, base_text, local_png):
    rows = session_rows(evidence)
    sequence = distinct_texts(rows)
    inserted = None
    for text in sequence:
        if LINK_MARKUP in text:
            inserted = text
    _assert(inserted is not None, "会话采样里没有插入后的文本")
    stripped = inserted.replace(LINK_MARKUP, "", 1)
    _assert(stripped == base_text, "插入文本不是基线加链接：\n" + inserted)
    write_json(evidence / "text-sequence.json", {
        "distinct_count": len(sequence),
        "sequence": [
            "基线" if text == base_text else
            "插入后" if text == inserted else "其它：" + text[:80]
            for text in sequence
        ],
        "inserted_text": inserted,
        "note": "取消与拒绝不产生新文本；撤销/重做形成基线↔插入后的往返",
    })
    _assert(len(sequence) == 4,
            "文本变更次数不符（取消/拒绝/图片操作不应改文档）："
            + json.dumps(sequence, ensure_ascii=False))
    _assert(sequence[0] == base_text and sequence[2] == base_text,
            "撤销没有回到基线：" + json.dumps(sequence, ensure_ascii=False))
    _assert(sequence[1] == inserted and sequence[3] == inserted,
            "重做没有恢复插入文本：" + json.dumps(sequence, ensure_ascii=False))

    first_insert = next(row for row in rows if row["text"] == inserted)
    after_undo = [row for row in rows
                  if row["text"] == base_text and row["undo_depth"] == 0
                  and row["redo_depth"] > 0]
    after_redo = [row for row in rows
                  if row["text"] == inserted and row["redo_depth"] == 0
                  and row["undo_depth"] > 0]
    write_json(evidence / "history.json", {
        "insert_undo_depth": first_insert["undo_depth"],
        "undo_rows": len(after_undo),
        "redo_rows": len(after_redo),
        "note": "插入产生一步历史；撤销后 redo 可用；重做后 undo 可用",
    })
    _assert(first_insert["undo_depth"] == 1, "插入应恰好产生一步历史")
    _assert(after_undo, "撤销状态没有被采样到")
    _assert(after_redo, "重做状态没有被采样到")

    disk = document.read_bytes()
    expected = inserted.encode("utf-8")
    write_json(evidence / "disk-after-save.json", {
        "size": len(disk),
        "expected_bytes": len(expected),
        "equal": disk == expected,
        "note": "保存后磁盘文档等于插入后的文本（取消/拒绝/图片操作都不改文档）",
    })
    _assert(disk == expected, "保存后磁盘文档与插入文本不一致")
    _assert(not local_png.is_file(), "缺失资源用例应已删除本地图片")

    final = (evidence / "final.tsv").read_text(encoding="utf-8")
    write_text(evidence / "final-state.txt", final)
    _assert("disk_equals_source=true" in final, "关闭时源码与磁盘不一致")
    _assert("disk_equals_stored=true" in final, "关闭时基线存储与磁盘不一致")
    case_state = (evidence / "case-001" / "state.tsv").read_text(encoding="utf-8")
    write_text(evidence / "case-state.txt", case_state)
    _assert("document_closed=true" in case_state, "只读取证未确认文档关闭")
    for name in ("source-after.bin", "stored.bin", "disk-after.bin"):
        data = (evidence / "case-001" / name).read_bytes()
        _assert(data == expected, "关闭后取证字节不等于插入文本：" + name)


# ---------------------------------------------------------------- 运行

def run(driver, document, local_png, inline_png):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    open_document_robust(driver, document)
    wait_tree(driver, lambda t: "可视化" in t and "源码" in t, "文档视图接管窗口")
    time.sleep(1.0)
    write_text(driver.evidence / "window-title-after-open.txt", driver.title())
    activate(driver)
    driver.click_body("点击正文把焦点交给编辑器")
    time.sleep(0.6)

    wait_ocr(driver, "Alpha", "正文文字出现", "body-baseline")
    write_json(driver.evidence / "rendered-images.json", {
        "local": list(color_point(driver, LOCAL_COLOR, "local")),
        "inline": list(color_point(driver, INLINE_COLOR, "inline")),
        "note": "两张正文图片按夹具色在截图中的位置（窗口逻辑点）",
    })

    observations = {}
    case_cancel(driver, driver.evidence)
    case_confirm_undo_redo(driver, driver.evidence)
    case_rejected_scheme(driver, driver.evidence)
    observations["text_menu"] = case_text_menu(driver, driver.evidence)
    observations["data_menu"] = case_data_image(driver, driver.evidence, inline_png)
    observations.update(case_local_actions(driver, driver.evidence, local_png))
    observations["missing_menu"] = case_copy_failure_and_missing(
        driver, driver.evidence, local_png)

    activate(driver)
    hotkey(driver, "cmd+s", "保存文档")
    deadline = time.monotonic() + 12.0
    saved = False
    while time.monotonic() < deadline:
        if LINK_MARKUP.encode("utf-8") in document.read_bytes():
            saved = True
            break
        time.sleep(0.3)
    write_json(driver.evidence / "save-wait.json", {
        "saved": saved,
        "note": "Cmd+S 后轮询磁盘文档包含链接文本",
    })
    _assert(saved, "保存后磁盘文档没有链接文本")
    close_document_and_wait(driver)
    return observations


def launch(evidence):
    env = dict(os.environ)
    env["ALINE_M2_EVIDENCE_DIR"] = str(evidence)
    env["ALINE_M1_READONLY_EVIDENCE_DIR"] = str(evidence)
    env[BUDGET_ENV] = BUDGET
    # 应用输出落文件而不是管道：无人读取的管道写满会阻塞应用主线程。
    stdout = open(evidence / "app-stdout.txt", "wb")
    stderr = open(evidence / "app-stderr.txt", "wb")
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(native.ROOT), env=env,
                            stdout=stdout, stderr=stderr)
    write_json(evidence / "launch.json", {
        "pid": proc.pid,
        "binary": "target/debug/aline",
        "env": {"ALINE_M2_EVIDENCE_DIR": "<evidence>",
                "ALINE_M1_READONLY_EVIDENCE_DIR": "<evidence>",
                BUDGET_ENV: BUDGET},
        "note": ("不带文档参数启动；图片预算按 V-06 未冻结的临时注入启用"
                 "正文图片渲染"),
    })
    return proc, stdout, stderr


def stop(proc, stdout, stderr):
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    stdout.close()
    stderr.close()


def result_facts(observations):
    text_menu = observations.get("text_menu") or {}
    if text_menu.get("method") == "ax" and not text_menu.get("extras"):
        menu_fact = "文字右键菜单恰为「新增链接…」「插入图片…」，图片弹窗可打开"
    else:
        menu_fact = ("文字右键菜单显示「新增链接…」「插入图片…」两项"
                     "（AX 未暴露菜单项，按整窗 OCR 文本核对），图片弹窗可打开")
    reveal = observations.get("reveal_found")
    reveal_fact = ("打开所在位置在 Finder 打开夹具目录窗口" if reveal
                   else "打开所在位置未观察到 Finder 窗口（见 limits）")
    limits = [
        "未渲染的引用（远程、打开时缺失、aline-temp）没有图片目标可右键，其门控只由纯模型单元测试覆盖；本次覆盖已渲染图片与渲染后删除的本地文件",
        "另存为只核对成功写出；磁盘/权限失败反馈未在真实窗口触发",
        "打开所在位置无法报告系统失败（reveal_path 无返回值）；本用例只核对 Finder 窗口出现，失败情形未触发",
        "生产窗口不注入图片预算（V-06 未冻结），本次以 ALINE_M4_MEDIA_BUDGET 注入正文图片渲染",
        "文本字段输入使用 ASCII；中文输入法与预填单行选区不涉及",
        "行内混排图片的逐张右键定位未覆盖（已知缺口）",
    ]
    if text_menu.get("method") != "ax":
        limits.append("menuitem 无障碍角色在本次运行未被 orca 快照暴露，菜单项标签与门控原因改用整窗 OCR 核对")
    return [
        "Cmd+K 与正文右键「新增链接…」打开同一链接弹窗（标题与字段一致）",
        "取消：输入文字与目标后取消，正文、磁盘与历史都不变",
        "确认：插入的链接文字与目标正确；撤销回到插入前、重做恢复同一历史",
        "未许可协议：javascript: 目标被拒绝并显示「不支持的链接协议」，文档不变",
        menu_fact,
        "data: 图片菜单：复制可用（回车执行复制），复制路径/打开所在位置禁用并显示「内嵌图片没有文件路径」",
        "本地图片菜单：四项可用；复制写入的剪贴板 PNG 与源文件逐字节一致",
        "复制路径写入绝对路径；另存为在文档目录写出字节一致的文件",
        reveal_fact,
        "本地文件在渲染后删除：复制/另存为/打开所在位置禁用并显示「找不到图片文件」，复制路径仍可用（回车执行复制路径）",
        "执行失败：菜单打开后删除源文件再复制，显示「复制图片失败」且文档不变",
        "保存后磁盘等于插入后的文本；关闭后源码/存储/磁盘三份采样一致",
    ], limits


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    if any(evidence.iterdir()):
        print("证据目录必须为空：" + str(evidence), file=sys.stderr)
        return 2
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             text=True, capture_output=True)
    if running.stdout.strip():
        print("已有 aline 进程占用，拒绝干扰：" + running.stdout.strip(),
              file=sys.stderr)
        return 2
    if not APP_BINARY.is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2
    require_fresh(evidence)

    # 夹具放在 /Users/Shared 下：文件面板列视图只有 根→Users→Shared→夹具，
    # 避免快照超过 orca 的 1200 节点上限（run-preview-5/6 的教训）。
    fixture_root = Path(tempfile.mkdtemp(prefix="aline-m4-insert-context-",
                                         dir="/Users/Shared"))
    document, local_png, inline_png, data_url = build_fixture(fixture_root)
    base_text = document.read_text(encoding="utf-8")
    fixture_copy = evidence / "fixture"
    fixture_copy.mkdir(exist_ok=True)
    shutil.copyfile(str(document), str(fixture_copy / document.name))
    shutil.copyfile(str(local_png), str(fixture_copy / local_png.name))
    shutil.copyfile(str(inline_png), str(fixture_copy / inline_png.name))
    write_json(evidence / "fixture.json", {
        "document": document.name,
        "base_text": base_text,
        "image_size": list(IMAGE_SIZE),
        "local_color": list(LOCAL_COLOR),
        "inline_color": list(INLINE_COLOR),
        "budget": BUDGET,
        "note": ("夹具在系统临时目录确定性生成；证据目录保留同一份副本"
                 "（data: 引用为内联字节）"),
    })

    cache = Path(tempfile.mkdtemp(prefix="aline-m4-insert-tools-"))
    ocr = compile_swift(READ_TEXT_SOURCE, cache, "read_text")
    original_clipboard = clipboard_text()
    proc = None
    stdout = stderr = None
    try:
        proc, stdout, stderr = launch(evidence)
        driver = native.WindowDriver(proc.pid, evidence)
        driver.ocr = ocr
        observations = run(driver, document, local_png, inline_png)
        verify(evidence, document, base_text, local_png)
        facts, limits = result_facts(observations)
        write_json(evidence / "result.json", {
            "status": "通过",
            "facts": facts,
            "limits": limits,
        })
    except native.DriverError as error:
        write_text(evidence / "failure.txt", str(error))
        print("失败：" + str(error), file=sys.stderr)
        return 1
    finally:
        if proc is not None:
            stop(proc, stdout, stderr)
        restore_clipboard(original_clipboard)
        shutil.rmtree(fixture_root, ignore_errors=True)
        shutil.rmtree(cache, ignore_errors=True)
    print("通过：证据目录 " + str(evidence))
    return 0


if __name__ == "__main__":
    sys.exit(main())
