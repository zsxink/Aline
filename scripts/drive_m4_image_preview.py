#!/usr/bin/env python3
"""M4 3.3 只读图片入口与视口真实窗口取证（PL-03 / AC-19）。

被测事实（每条都在 result.json 的 facts 里列明观察方式）：
- 适应：默认按视口缩小且不放大；工具栏百分比与截图中实测图像矩形一致。
- 1:1：原始尺寸 100%，图像大于视口时被视口裁剪。
- 缩放：1.25 步进（100%→125%→100%→80%）。
- 平移：左键拖拽与滚轮改变图像偏移；滚轮不缩放。
- 路径复用：同路径再次经文件面板打开不重载，缩放与平移保留。
- 关闭：关闭预览恢复空状态与窗口标题。
- 错误与重试：损坏文件显示可重试说明并保留路径；修复后重试成功且小图不放大。
- 插件关闭：预览转说明状态并保留路径；新预览请求被拦截并说明；正文图片
  仍正常绘制（打开带图文档核对）；重新启用后适应按当前视口重算。
- 旧结果：被替换的加载、关闭后的迟到结果、禁用期间的迟到结果都不上屏。

窗口内输入只经 macOS 无障碍接口（orca computer）投递；颜色测量来自窗口
截图，显示层色彩管理偏移按容差归类。旧结果用例依赖 debug 构建的
`ALINE_M4_PREVIEW_DELAY_MS` 放慢后台读取（生产构建恒不读取）；图片预算由
`ALINE_M4_MEDIA_BUDGET` 显式注入（V-06 数值未冻结，生产默认不注入）。

用法：drive_m4_image_preview.py --evidence <证据目录>
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from PIL import Image, ImageChops

sys.path.insert(0, str(Path(__file__).resolve().parent))

import drive_m2_editable_window as native  # noqa: E402  复用同一真实输入驱动
from drive_m2_find_modes import activate, require_fresh, system_events  # noqa: E402
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m4_image_insert import (  # noqa: E402
    find_panel_path_field, open_document_robust, raise_panel, shrink_panel_listing,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_BINARY = native.ROOT / "target" / "debug" / "aline"
RESIZE_SOURCE = native.ROOT / "scripts" / "fixtures" / "resize_window.swift"
READ_TEXT_SOURCE = native.ROOT / "scripts" / "fixtures" / "read_text.swift"
BUDGET_ENV = "ALINE_M4_MEDIA_BUDGET"
BUDGET = "1048576,16777216,268435456,8"
DELAY_ENV = "ALINE_M4_PREVIEW_DELAY_MS"
# 陈旧结果用例要求「第二次打开」在第一次加载结果到达前完成；文件面板是系统
# 面板服务的远程视图，深目录下每次快照可达 3～4 秒、一次打开可达 30 秒
# （run-preview-6 实测），12 秒延迟不够，放宽到 20 秒并配合浅夹具目录。
DELAY_MS = 20000

QUADRANT_SIZE = (1600, 1200)
SMALL_SIZE = (64, 32)
BLOCK_SIZE = (120, 80)

# 夹具色刻意避开界面语义色（含强调蓝与五类 Alert），容差含显示层色彩管理
# 偏移（饱和青/黄实测偏移 50～80），按最坏通道差 100 归类。
COLORS = {
    "magenta": (200, 30, 200),
    "cyan": (0, 180, 220),
    "indigo": (30, 70, 150),
    "yellow": (240, 200, 20),
    "pink": (255, 105, 180),
}
QUADRANTS = (
    ("magenta", 0, 0),
    ("cyan", 1, 0),
    ("indigo", 0, 1),
    ("yellow", 1, 1),
)
TOLERANCE = 100
# 饱和度门限（0～255，约 0.4）：界面灰文本在 100 通道容差内会落进品红/粉/
# 靛蓝掩码（run-preview-6 空状态实测 626/1181/2067 像素，曾让「关闭后品红
# 为 0」的等待超时）；夹具色饱和度均 ≥0.59，离线比对确认加门限后图像计数
# 与包围盒逐像素不变、空状态杂散归零。
MIN_SATURATION = 102
# 跳过标题栏（交通灯）与工具栏（品牌图标）：内容区约从窗口上沿 76 点开始，
# 从 80 点起扫描；下沿同样跳过状态栏文字，避免中性灰被归入靛蓝类。
SCAN_TOP = 80.0
SCAN_BOTTOM_MARGIN = 30.0

LOADING_PHRASE = "正在加载图片"
DISABLED_PHRASE = "预览已暂停"
BLOCKED_PHRASE = "无法打开新的预览"
UNSUPPORTED_PHRASE = "不是受支持的图片格式"


def matches_phrase(text, phrase):
    """OCR 容错包含：Vision 偶尔漏读结构助词「的」（run-preview-9 把状态栏
    「无法打开新的预览」读成「无法打开新预览」），比对前两侧都去掉「的」。"""
    return phrase.replace("的", "") in text.replace("的", "")


def _assert(condition, note):
    if not condition:
        raise native.DriverError(note)


def build_fixture(directory):
    """确定性生成四象限大图、小图、损坏文件与正文图片文档。"""
    directory.mkdir(parents=True, exist_ok=True)
    width, height = QUADRANT_SIZE
    quadrants = Image.new("RGB", QUADRANT_SIZE)
    for name, column, row in QUADRANTS:
        box = (column * width // 2, row * height // 2,
               (column + 1) * width // 2, (row + 1) * height // 2)
        quadrants.paste(
            Image.new("RGB", (box[2] - box[0], box[3] - box[1]), COLORS[name]), box)
    quadrants.save(directory / "quadrants.png", format="PNG")
    Image.new("RGB", SMALL_SIZE, COLORS["pink"]).save(directory / "small.png", format="PNG")
    Image.new("RGB", BLOCK_SIZE, COLORS["pink"]).save(directory / "block.png", format="PNG")
    (directory / "broken.png").write_bytes(b"not an image\n")
    (directory / "body.md").write_text(
        "# 正文图片\n\n![块](block.png)\n", encoding="utf-8")


def latest_shot(driver):
    shots = sorted(driver.evidence.glob("step-*.png"))
    if not shots:
        raise native.DriverError("证据目录没有截图")
    return shots[-1]


def color_mask(image, color, tolerance=TOLERANCE, min_saturation=MIN_SATURATION):
    """单色掩码：三通道差值都 ≤ 容差且饱和度达门限的像素为 255。"""
    difference = ImageChops.difference(image, Image.new("RGB", image.size, color))
    red, green, blue = difference.split()
    worst = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    mask = worst.point(lambda value: 255 if value <= tolerance else 0)
    saturation = image.convert("HSV").split()[1]
    gate = saturation.point(lambda value: 255 if value >= min_saturation else 0)
    return ImageChops.multiply(mask, gate)


def measure(shot, window):
    """按颜色统计内容区内的像素数与包围盒（换算为窗口逻辑点）。"""
    image = Image.open(shot).convert("RGB")
    width, height = image.size
    scale = width / float(window["width"])
    top = min(height, int(SCAN_TOP * scale))
    bottom = max(top, height - int(SCAN_BOTTOM_MARGIN * scale))
    report = {"scale": round(scale, 4), "shot": Path(shot).name}
    for name, color in COLORS.items():
        region = color_mask(image, color).crop((0, top, width, bottom))
        pixels = region.histogram()[255]
        box = region.getbbox()
        report[name] = {
            "pixels": pixels,
            "bbox": None if box is None else [
                round(box[0] / scale, 2), round((box[1] + top) / scale, 2),
                round(box[2] / scale, 2), round((box[3] + top) / scale, 2)],
        }
    return report


def ocr_lines(binary, shot):
    """系统 Vision 文本识别整窗截图；返回识别到的行。"""
    result = subprocess.run([str(binary), str(shot), "zh-Hans", "en-US"],
                            text=True, capture_output=True)
    if result.returncode != 0:
        raise native.DriverError("文字识别失败：" + result.stderr[-400:])
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        raise native.DriverError("文字识别输出无法解析：" + result.stdout[:300])
    return payload.get("lines") or []


def percent_in(text):
    matches = re.findall(r"(\d{1,3})\s*%", text)
    if not matches:
        return None
    values = {int(match) for match in matches}
    _assert(len(values) == 1,
            "同屏出现多个百分比：" + ",".join(str(value) for value in sorted(values)))
    return values.pop()


def capture(driver, note):
    snapshot = driver.snapshot()
    tree = driver.tree_text(snapshot)
    window = snapshot.get("window") or {}
    shot = latest_shot(driver)
    lines = ocr_lines(driver.ocr, shot)
    text = "\n".join(lines)
    return {
        "note": note,
        "tree": tree,
        "text": text,
        "flat": re.sub(r"\s+", "", text),
        "window": window,
        "percent": percent_in(text),
        "colors": measure(shot, window) if window.get("width") else {},
        "shot": shot.name,
    }


def summary(state):
    return {
        "note": state.get("note"),
        "percent": state.get("percent"),
        "text": state.get("text"),
        "shot": state.get("shot"),
        "window": state.get("window"),
        "pixels": {name: value["pixels"] for name, value in state.get("colors", {}).items()
                   if name in COLORS},
    }


def observed(state):
    """把当次可见文字与百分比并入用例记录，便于脱离截图核对。"""
    return {"percent": state["percent"], "text": state["text"], "shot": state["shot"]}


def wait_for(driver, predicate, note, timeout=30.0, interval=0.5):
    deadline = time.monotonic() + timeout
    state = None
    while time.monotonic() < deadline:
        state = capture(driver, note)
        if predicate(state):
            return state
        time.sleep(interval)
    write_json(driver.evidence / "wait-failure.json",
               {"note": note, "last": summary(state) if state else None})
    raise native.DriverError("等待超时：" + note)


def image_box(state):
    boxes = [state["colors"][name]["bbox"] for name, _, _ in QUADRANTS]
    if any(box is None for box in boxes):
        return None
    return [min(box[0] for box in boxes), min(box[1] for box in boxes),
            max(box[2] for box in boxes), max(box[3] for box in boxes)]


def scan_area(state):
    """内容扫描带的设备像素数，用来判定图像是否仍铺满视口。"""
    width = state["window"]["width"]
    height = state["window"]["height"]
    scale = state["colors"]["scale"]
    top = min(int(height * scale), int(SCAN_TOP * scale))
    bottom = max(top, int(height * scale) - int(SCAN_BOTTOM_MARGIN * scale))
    return int(width * scale) * (bottom - top)


def quadrant_coverage(state):
    """四象限颜色覆盖的像素总数（即图像铺满扫描带时为扫描带面积）。"""
    return sum(state["colors"][name]["pixels"] for name, _, _ in QUADRANTS)


def vertical_boundary_x(state):
    """左/右象限分界线的横坐标；优先用上排，被裁掉时退回下排。"""
    left = state["colors"]["magenta"]["bbox"] or state["colors"]["indigo"]["bbox"]
    right = state["colors"]["cyan"]["bbox"] or state["colors"]["yellow"]["bbox"]
    if left is None or right is None:
        return None
    return round((left[2] + right[0]) / 2.0, 2)


def horizontal_boundary_y(state):
    """上/下象限分界线的纵坐标；上排移出扫描带时返回 None。

    下排优先用黄色而非靛蓝：上下两排的接缝混色像素会被容差判成靛蓝，
    靛蓝包围盒的上沿因此不可信，右列黄色的上沿才是真实分界线。
    """
    top = state["colors"]["magenta"]["bbox"] or state["colors"]["cyan"]["bbox"]
    bottom = state["colors"]["yellow"]["bbox"] or state["colors"]["indigo"]["bbox"]
    if top is None or bottom is None:
        return None
    return round((top[3] + bottom[1]) / 2.0, 2)


def click_button(driver, label, note):
    snapshot = driver.snapshot()
    index = driver.find_button(snapshot, label)
    if index is None:
        raise native.DriverError("找不到按钮：" + label)
    # 点前复核目标索引：orca 的 click 在内部重新枚举元素，快照与点击之间索引
    # 若漂移，点击会落到别的元素（run-preview-4 曾因此在「原始尺寸」点击时
    # 击中关闭按钮，应用以 0 正常退出、无 panic 输出）。复核子树会存成
    # step-*.tree.txt，作为「点击目标仍是该按钮」的证据。
    subtree = driver.call(["get-app-state", "--element-index", str(index)],
                          "复核点击目标：" + label)
    text = (subtree.get("snapshot") or {}).get("treeText", "")
    if ("button " + label) not in text:
        snapshot = driver.snapshot()
        index = driver.find_button(snapshot, label)
        if index is None:
            raise native.DriverError("点击目标索引漂移且找不到按钮：" + label)
        subtree = driver.call(["get-app-state", "--element-index", str(index)],
                              "复核点击目标（重取）：" + label)
        text = (subtree.get("snapshot") or {}).get("treeText", "")
        if ("button " + label) not in text:
            raise native.DriverError("点击目标复核失败：" + label)
    driver.call(["click", "--element-index", str(index)], note)
    time.sleep(0.5)


def open_file(driver, path, evidence, label):
    """经文件面板打开目标并留存本次面板记录。"""
    open_document_robust(driver, path)
    record = evidence / "open-panel.json"
    if record.is_file():
        shutil.copyfile(str(record), str(evidence / f"open-panel-{label}.json"))
    return record


def open_expect_blocked(driver, path, evidence):
    """经文件面板选中图片但预期被拦截：窗口标题保持原值，不核对新标题。

    与 ``open_document_robust`` 同一面板流程，只把「标题应变为新文件名」换成
    「面板关闭且标题不变」；拦截说明由调用方在状态栏核对。
    """
    target = str(Path(path).resolve())
    name = Path(target).name
    if driver.title() == "Open":
        system_events(driver, "key code 53", "关闭残留文件面板")
        time.sleep(1.0)
    title_before = driver.title()
    activate(driver)
    driver.call(["hotkey", "--key", native.OPEN_PANEL_KEY], "打开文件面板")
    deadline = time.monotonic() + 6.0
    while time.monotonic() < deadline and driver.title() != "Open":
        time.sleep(0.2)
    _assert(driver.title() == "Open", "文件面板未打开（拦截用例）")
    time.sleep(0.4)
    raise_panel(driver)
    system_events(driver, 'keystroke "g" using {command down, shift down}',
                  "打开前往文件夹")
    field, truncated = find_panel_path_field(driver)
    if field is None and truncated:
        if shrink_panel_listing(driver):
            raise_panel(driver)
            system_events(driver, 'keystroke "g" using {command down, shift down}',
                          "重新打开前往文件夹（拦截用例）")
            field, _ = find_panel_path_field(driver)
    _assert(field is not None, "前往文件夹未出现路径字段（拦截用例）")
    driver.call(["set-value", "--element-index", str(field), "--value", target],
                "写入夹具绝对路径")
    time.sleep(0.4)
    system_events(driver, "key code 36", "回车确认路径")
    container = None
    deadline = time.monotonic() + 6.0
    while time.monotonic() < deadline:
        snapshot = driver.snapshot()
        container = driver.find_row_container(snapshot, name)
        if container is not None:
            break
        time.sleep(0.2)
    _assert(container is not None, "文件面板没有目标行（拦截用例）")
    # AXOpen 辅助动作在本机会偶发 accessibility_error（run-preview-7 第 109 步
    # 即以该错误终止）；与 open_document_robust 一致，记录错误后继续用点击、
    # 回车兜底，而不是让拦截用例因一次辅助动作失败而整轮作废。
    secondary_error = None
    try:
        driver.call(["perform-secondary-action", "--element-index", str(container),
                     "--action", "open"], "行容器 open 辅助动作（拦截用例）")
    except native.DriverError as error:
        secondary_error = str(error)[:300]
    time.sleep(1.5)
    if driver.title() == "Open":
        driver.call(["click", "--element-index", str(container)], "点击行容器（拦截用例）")
        time.sleep(1.5)
    if driver.title() == "Open":
        system_events(driver, "key code 36", "回车打开所选夹具（拦截用例）")
        time.sleep(1.5)
    write_json(evidence / "open-blocked.json", {
        "target": name,
        "title_before": title_before,
        "title_after": driver.title(),
        "secondary_action_error": secondary_error,
        "panel_open": driver.title() == "Open",
        "note": "插件关闭时选中图片：面板关闭，窗口标题保持原预览文件名",
    })
    _assert(driver.title() != "Open", "文件面板没有关闭（拦截用例）")
    _assert(driver.title() == title_before, "被拦截的打开不应改变窗口标题")


def ready_predicate(state):
    return (state["percent"] is not None
            and state["colors"]["magenta"]["pixels"] > 100
            and state["colors"]["cyan"]["pixels"] > 100)


def case_fit_actual_zoom_pan(driver, evidence, fixture):
    """适应 / 1:1 / 缩放步进 / 拖拽与滚轮平移 / 路径复用 / 关闭。"""
    open_file(driver, fixture / "quadrants.png", evidence, "fit")
    state = wait_for(driver, ready_predicate, "等待适应视图就绪")
    fit_percent = state["percent"]
    box = image_box(state)
    window = state["window"]
    width = box[2] - box[0]
    height = box[3] - box[1]
    write_json(evidence / "case-fit.json", {
        **observed(state),
        "percent": fit_percent,
        "image_box": box,
        "window": window,
        "implied_scale": [round(width / QUADRANT_SIZE[0], 4),
                          round(height / QUADRANT_SIZE[1], 4)],
        "note": "适应：百分比应与实测矩形/原图尺寸一致，且横向留出边距（不放大、不溢出）",
    })
    _assert(fit_percent is not None and 10 <= fit_percent < 100,
            "适应模式百分比应低于 100：" + str(fit_percent))
    _assert(abs(width / QUADRANT_SIZE[0] - fit_percent / 100.0) <= 0.02,
            "适应宽度与百分比不一致")
    _assert(abs(height / QUADRANT_SIZE[1] - fit_percent / 100.0) <= 0.02,
            "适应高度与百分比不一致")
    _assert(abs(width / height - QUADRANT_SIZE[0] / QUADRANT_SIZE[1]) <= 0.03,
            "适应后宽高比应与原图一致")
    _assert(width < window["width"] - 20, "适应模式不应横向填满或溢出视口")
    content = (box[1], box[3])
    write_json(evidence / "content-area.json", {
        "top": content[0],
        "bottom": content[1],
        "height": round(content[1] - content[0], 2),
        "window": window,
        "note": "适应为高度受限：实测图像下沿即内容区下沿，上沿按扫描起点 80 点截去数点",
    })

    click_button(driver, "原始尺寸", "切到 1:1")
    state = wait_for(driver, lambda s: s["percent"] == 100 and image_box(s) is not None,
                     "等待 1:1")
    box = image_box(state)
    write_json(evidence / "case-actual-size.json", {
        **observed(state),
        "percent": state["percent"],
        "image_box": box,
        "window": window,
        "note": "1:1 时图像 1600×1200 大于视口，包围盒被视口裁剪到内容区",
    })
    _assert(box[2] - box[0] >= window["width"] - 8, "1:1 应横向覆盖视口")
    _assert(box[3] - box[1] >= (content[1] - content[0]) - 8, "1:1 应纵向覆盖视口")

    percents = [state["percent"]]
    click_button(driver, "放大", "放大一档")
    state = wait_for(driver, lambda s: s["percent"] == 125, "等待 125%")
    percents.append(state["percent"])
    click_button(driver, "缩小", "缩小一档")
    state = wait_for(driver, lambda s: s["percent"] == 100, "等待回到 100%")
    percents.append(state["percent"])
    click_button(driver, "缩小", "再缩小一档")
    state = wait_for(driver, lambda s: s["percent"] == 80, "等待 80%")
    percents.append(state["percent"])
    write_json(evidence / "case-zoom-steps.json", {
        **observed(state),
        "percents": percents,
        "note": "1.25 步进：100%→125%→100%→80%",
    })
    _assert(percents == [100, 125, 100, 80], "缩放步进序列不符：" + str(percents))

    click_button(driver, "原始尺寸", "回到 1:1 准备平移")
    state = wait_for(driver, lambda s: s["percent"] == 100 and image_box(s) is not None,
                     "等待 1:1（平移前）")
    before = state["colors"]["magenta"]["bbox"]
    center_x = window["width"] / 2.0
    center_y = (content[0] + content[1]) / 2.0
    driver.call(["drag",
                 "--from-x", str(round(center_x, 1)), "--from-y", str(round(center_y, 1)),
                 "--to-x", str(round(center_x - 120.0, 1)),
                 "--to-y", str(round(center_y - 80.0, 1))],
                "左键从视口中心拖拽 (-120,-80)")
    time.sleep(0.6)
    state = capture(driver, "拖拽后采样")
    after = state["colors"]["magenta"]["bbox"]
    shift = [round(after[2] - before[2], 2), round(after[3] - before[3], 2)]
    write_json(evidence / "case-drag-pan.json", {
        **observed(state),
        "before_right_bottom": [before[2], before[3]],
        "after_right_bottom": [after[2], after[3]],
        "shift": shift,
        "expected": [-120, -80],
        "percent": state["percent"],
        "note": "品红象限的右/下边界即图像内部分界线，随拖拽位移；容差 8 点",
    })
    _assert(abs(shift[0] + 120) <= 8 and abs(shift[1] + 80) <= 8,
            "拖拽位移与指针不一致：" + str(shift))
    _assert(state["percent"] == 100, "拖拽不应改变缩放")

    before = state
    before_x = vertical_boundary_x(before)
    before_y = horizontal_boundary_y(before)
    # 合成 HID 滚轮偶发未生效（run-preview-10 与 run-8/9 注入坐标、退出码完全
    # 相同，却零位移）；允许最多 3 次重注入，断言仍只核对方向、无缩放与平移约束。
    wheel_attempts = 0
    moved_up = False
    while wheel_attempts < 3 and not moved_up:
        wheel_attempts += 1
        driver.inject_scroll_wheel(center_x, center_y, 0, -120,
                                   f"滚轮向下平移（第 {wheel_attempts} 次）")
        time.sleep(0.8)
        state = capture(driver, f"滚轮后采样（第 {wheel_attempts} 次）")
        after_x = vertical_boundary_x(state)
        after_y = horizontal_boundary_y(state)
        moved_up = after_y is None or (before_y is not None and after_y < before_y - 8)
    coverage = quadrant_coverage(state)
    area = scan_area(state)
    write_json(evidence / "case-wheel-pan.json", {
        **observed(state),
        "boundary_x_before": before_x,
        "boundary_x_after": after_x,
        "boundary_y_before": before_y,
        "boundary_y_after": after_y,
        "moved_up": moved_up,
        "wheel_attempts": wheel_attempts,
        "coverage": coverage,
        "scan_area": area,
        "percent": state["percent"],
        "note": "滚轮 deltaY=-120 只平移、不改缩放；平台层会把系统滚轮增量换算成"
                "自身卷动距离，故不核对注入量与位移一一对应，而核对方向（图像上移）、"
                "左右分界线不动、以及图像仍铺满扫描带（平移被约束在图像内）",
    })
    _assert(state["percent"] == 100, "滚轮不应改变缩放")
    _assert(moved_up, "滚轮应把图像上移：" + str((before_y, after_y)))
    _assert(before_x is not None and after_x is not None
            and abs(after_x - before_x) <= 8,
            "纯纵向滚轮不应改变左右分界线：" + str((before_x, after_x)))
    _assert(coverage >= area * 0.99,
            "滚轮平移露出背景，说明平移未被约束在图像内：" + str((coverage, area)))

    before = state
    open_file(driver, fixture / "quadrants.png", evidence, "reuse")
    time.sleep(0.8)
    state = capture(driver, "同路径再次打开后采样")
    before_x = vertical_boundary_x(before)
    after_x = vertical_boundary_x(state)
    panel = evidence / "open-panel-reuse.json"
    panel_record = json.loads(panel.read_text(encoding="utf-8")) if panel.is_file() else {}
    attempts = panel_record.get("attempts") or []
    write_json(evidence / "case-path-reuse.json", {
        **observed(state),
        "percent_before": before["percent"],
        "percent_after": state["percent"],
        "boundary_x_before": before_x,
        "boundary_x_after": after_x,
        "boundary_y_before": horizontal_boundary_y(before),
        "boundary_y_after": horizontal_boundary_y(state),
        "coverage_after": quadrant_coverage(state),
        "scan_area": scan_area(state),
        "panel": panel_record,
        "note": "同路径复用：百分比、平移与铺满状态保留（重载会回到适应并居中 47%）；"
                "面板记录证明选中并确认了同一文件",
    })
    _assert(state["percent"] == 100, "同路径复用不应重置为适应")
    _assert(before_x is not None and after_x is not None
            and abs(after_x - before_x) <= 4,
            "同路径复用不应重置平移：" + str((before_x, after_x)))
    _assert(quadrant_coverage(state) >= scan_area(state) * 0.99,
            "同路径复用后图像应仍铺满视口")
    _assert(any(attempt.get("opened_by") for attempt in attempts),
            "缺少面板选中与打开方式记录")

    click_button(driver, "关闭预览", "关闭预览")
    state = wait_for(driver, lambda s: "适应窗口" not in s["flat"]
                     and s["colors"]["magenta"]["pixels"] == 0, "等待预览关闭")
    title = driver.title()
    write_json(evidence / "case-close.json", {
        **observed(state),
        "tree_has_preview": "适应窗口" in state["flat"],
        "title": title,
        "note": "关闭预览后工具栏恢复空状态，窗口标题回到应用名",
    })
    _assert(title == "Aline", "关闭预览后窗口标题应恢复应用名：" + title)


def case_error_retry(driver, evidence, fixture):
    """损坏图片显示可重试说明并保留路径；修复后重试成功且不放大。"""
    open_file(driver, fixture / "broken.png", evidence, "broken")
    state = wait_for(driver, lambda s: UNSUPPORTED_PHRASE in s["flat"], "等待不支持格式说明")
    write_json(evidence / "case-error.json", {
        **observed(state),
        "has_message": UNSUPPORTED_PHRASE in state["flat"],
        "has_path": "broken.png" in state["flat"],
        "has_retry": "重试" in state["flat"],
        "percent": state["percent"],
        "shot": state["shot"],
        "note": "损坏文件显示不支持说明、保留路径并提供重试",
    })
    _assert("broken.png" in state["flat"], "失败状态应保留图片路径")
    _assert("重试" in state["flat"], "失败状态应提供重试按钮")
    _assert(state["colors"]["pink"]["pixels"] == 0, "失败状态不应绘制图片")

    Image.new("RGB", SMALL_SIZE, COLORS["pink"]).save(fixture / "broken.png", format="PNG")
    click_button(driver, "重试", "修复后重试")
    state = wait_for(driver, lambda s: s["percent"] == 100
                     and s["colors"]["pink"]["pixels"] > 100, "等待重试成功")
    box = state["colors"]["pink"]["bbox"]
    width = box[2] - box[0]
    height = box[3] - box[1]
    write_json(evidence / "case-retry.json", {
        **observed(state),
        "percent": state["percent"],
        "image_box": box,
        "source_size": list(SMALL_SIZE),
        "note": "小图适应不放大：64×32 在 100% 下原尺寸显示",
    })
    _assert(abs(width - SMALL_SIZE[0]) <= 4 and abs(height - SMALL_SIZE[1]) <= 4,
            "小图应原尺寸显示（不放大）：" + str(box))
    click_button(driver, "关闭预览", "关闭重试预览")
    wait_for(driver, lambda s: "适应窗口" not in s["flat"], "等待预览关闭")


def case_plugin_off_resize(driver, evidence, fixture, cache):
    """插件关闭保留路径、拦截新请求；变更尺寸后再启用按新视口重算。"""
    open_file(driver, fixture / "quadrants.png", evidence, "resize-fit")
    state = wait_for(driver, ready_predicate, "等待适应视图就绪（禁用前）")
    fit_before = state["percent"]
    box = image_box(state)
    height_before = box[3] - box[1]
    window_height_before = state["window"].get("height")

    activate(driver)
    driver.call(["hotkey", "--key", "cmd+alt+p"], "关闭图片预览插件")
    state = wait_for(driver, lambda s: DISABLED_PHRASE in s["flat"], "等待插件关闭说明")
    write_json(evidence / "case-plugin-off.json", {
        **observed(state),
        "has_message": DISABLED_PHRASE in state["flat"],
        "has_path": "quadrants.png" in state["flat"],
        "image_pixels": state["colors"]["magenta"]["pixels"],
        "note": "插件关闭后预览转说明状态并保留图片路径，视口不再绘制图片",
    })
    _assert("quadrants.png" in state["flat"], "插件关闭后应保留图片路径")
    _assert(state["colors"]["magenta"]["pixels"] == 0, "插件关闭后不应绘制图片")

    open_expect_blocked(driver, fixture / "small.png", evidence)
    state = wait_for(driver, lambda s: matches_phrase(s["flat"], BLOCKED_PHRASE),
                     "等待新预览被拦截")
    write_json(evidence / "case-preview-blocked.json", {
        **observed(state),
        "has_feedback": matches_phrase(state["flat"], BLOCKED_PHRASE),
        "still_disabled": DISABLED_PHRASE in state["flat"],
        "still_quadrants": "quadrants.png" in state["flat"],
        "note": "插件关闭时新的图片打开请求被拦截并说明；原预览路径保持不变",
    })
    _assert(DISABLED_PHRASE in state["flat"], "拦截后原预览说明应保留")
    _assert("quadrants.png" in state["flat"], "拦截不应替换原预览路径")
    _assert(state["colors"]["pink"]["pixels"] == 0, "被拦截的图片不应上屏")

    resize = resize_window(cache, driver.pid, 1200, 800)
    time.sleep(1.0)
    state = capture(driver, "窗口改尺寸后采样")
    write_json(evidence / "case-resize.json", {
        **observed(state),
        "resize": resize,
        "window": state["window"],
        "note": "经辅助功能 API 把窗口改为 1200×800",
    })
    _assert(abs(state["window"].get("width", 0) - 1200) <= 2, "窗口宽度未改为 1200")

    activate(driver)
    driver.call(["hotkey", "--key", "cmd+alt+p"], "重新启用图片预览插件")
    state = wait_for(driver, ready_predicate, "等待重新启用后加载完成", timeout=40.0)
    fit_after = state["percent"]
    box = image_box(state)
    height_after = box[3] - box[1]
    window_height_after = state["window"].get("height")
    write_json(evidence / "case-reenable-after-resize.json", {
        **observed(state),
        "percent_before": fit_before,
        "percent_after": fit_after,
        "content_height_before": round(height_before, 2),
        "content_height_after": round(height_after, 2),
        "window_height_before": window_height_before,
        "window_height_after": window_height_after,
        "image_box": box,
        "note": "重新启用后适应按当前视口重算：内容区变高，百分比随之变大",
    })
    # 屏幕可用高度会把 1200×800 裁短（run-preview-8 实测窗口高 764），所以不核对
    # 名义高度，而核对内容区增幅与窗口增幅一致：工具栏/状态栏固定，内容区应随窗口
    # 等高变化；同时百分比按新视口重算且与实测矩形吻合。
    _assert(height_after > height_before, "内容区高度没有随窗口变大")
    _assert(
        abs((height_after - height_before)
            - (window_height_after - window_height_before)) <= 2.0,
        "内容区高度增幅应与窗口高度增幅一致",
    )
    _assert(fit_after is not None and fit_after > (fit_before or 0),
            "重新启用后适应百分比应随视口变大")
    _assert(abs(height_after / QUADRANT_SIZE[1] - fit_after / 100.0) <= 0.02,
            "重新启用后的百分比与实测矩形不一致")

    # 插件关闭时正文图片仍工作：先关掉已暂停的预览视口（M4 无标签栏，预览视口
    # 占据主内容区，不关闭会挡住文档），再打开带图文档核对正文图片。
    activate(driver)
    driver.call(["hotkey", "--key", "cmd+alt+p"], "再次关闭图片预览插件")
    wait_for(driver, lambda s: DISABLED_PHRASE in s["flat"], "等待插件再次关闭")
    click_button(driver, "关闭预览", "插件关闭时关闭预览视口")
    state = wait_for(driver, lambda s: (s["window"].get("title") == "Aline"
                                        and DISABLED_PHRASE not in s["flat"]),
                     "等待预览视口关闭")
    write_json(evidence / "case-disabled-close.json", {
        **observed(state),
        "title": state["window"].get("title"),
        "note": "插件关闭时关闭预览视口：工具栏退出预览状态，标题回到应用名",
    })
    open_file(driver, fixture / "body.md", evidence, "body-image")
    state = wait_for(driver, lambda s: s["colors"]["pink"]["pixels"] > 100,
                     "等待正文图片绘制", timeout=40.0)
    write_json(evidence / "case-body-image-plugin-off.json", {
        **observed(state),
        "pink_pixels": state["colors"]["pink"]["pixels"],
        "percent": state["percent"],
        "title": state["window"].get("title"),
        "note": "插件关闭时打开带图文档：正文图片仍绘制，且没有预览百分比",
    })
    _assert(state["percent"] is None, "正文接管后不应仍有预览百分比")
    _assert(state["window"].get("title") == "body.md",
            "文档标题应为 body.md：" + str(state["window"].get("title")))
    activate(driver)
    driver.call(["hotkey", "--key", "cmd+alt+p"], "重新启用插件（文档保持）")
    time.sleep(1.0)
    state = capture(driver, "重新启用后文档采样")
    write_json(evidence / "case-body-image-reenable.json", {
        **observed(state),
        "pink_pixels": state["colors"]["pink"]["pixels"],
        "title": state["window"].get("title"),
        "note": "预览已关闭时重新启用插件：不复活预览，已打开的文档与正文图片不受影响",
    })
    _assert(state["colors"]["pink"]["pixels"] > 100, "重新启用插件不应影响正文图片")
    _assert("适应窗口" not in state["flat"], "预览已关闭时重新启用不应复活预览")
    _assert(state["window"].get("title") == "body.md", "重新启用不应改变文档标题")


def resize_window(cache, pid, width, height):
    binary = compile_swift(RESIZE_SOURCE, cache, "resize_window")
    result = subprocess.run([str(binary), str(pid), str(width), str(height)],
                            text=True, capture_output=True)
    record = {
        "argv": ["scripts/fixtures/resize_window.swift", str(pid), str(width), str(height)],
        "exit_code": result.returncode,
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip()[-600:],
        "note": "经辅助功能 API 设置窗口尺寸",
    }
    if result.returncode != 0:
        raise native.DriverError("窗口改尺寸失败：" + record["stderr"])
    return record


def compile_swift(source, cache, name):
    binary = cache / name
    if binary.is_file():
        return binary
    build = subprocess.run(["swiftc", "-O", str(source), "-o", str(binary)],
                           text=True, capture_output=True)
    if build.returncode != 0:
        raise native.DriverError("Swift 工具编译失败：" + build.stderr[-800:])
    return binary


def case_stale_results(driver, evidence, fixture):
    """旧结果不上屏：被替换的加载、关闭后的迟到结果、禁用期间的迟到结果。"""
    delay_seconds = DELAY_MS / 1000.0

    # 1) 被替换的加载结果。
    open_file(driver, fixture / "quadrants.png", evidence, "stale-first")
    first_open = time.monotonic()
    wait_for(driver, lambda s: LOADING_PHRASE in s["flat"], "等待第一次加载开始", timeout=10.0)
    open_file(driver, fixture / "small.png", evidence, "stale-second")
    second_open = time.monotonic()
    replaced_after = second_open - first_open
    _assert(replaced_after < delay_seconds - 1.0,
            "第二次打开晚于第一次结果到达，无法区分旧结果拒绝：" + str(replaced_after))
    sample_at = max(second_open + 1.0, first_open + delay_seconds + 0.5)
    _assert(sample_at < second_open + delay_seconds - 0.5,
            "采样点落在第二次加载完成之后，无法观察旧结果拒绝")
    while time.monotonic() < sample_at:
        time.sleep(0.2)
    state = capture(driver, "第一次结果到达后的中途采样")
    write_json(evidence / "case-stale-superseded.json", {
        **observed(state),
        "first_open_to_second_open_s": round(replaced_after, 2),
        "sample_after_first_open_s": round(time.monotonic() - first_open, 2),
        "delay_ms": DELAY_MS,
        "loading_text": LOADING_PHRASE in state["flat"],
        "magenta_pixels": state["colors"]["magenta"]["pixels"],
        "pink_pixels": state["colors"]["pink"]["pixels"],
        "shot": state["shot"],
        "note": "第一次加载的结果已产生，视口仍显示第二次加载且没有第一张图",
    })
    _assert(LOADING_PHRASE in state["flat"], "第二次加载应仍在进行")
    _assert(state["colors"]["magenta"]["pixels"] == 0, "被替换的加载结果不应上屏")
    state = wait_for(driver, lambda s: s["percent"] == 100
                     and s["colors"]["pink"]["pixels"] > 100,
                     "等待第二次加载完成", timeout=delay_seconds + 25.0)
    box = state["colors"]["pink"]["bbox"]
    write_json(evidence / "case-stale-final.json", {
        **observed(state),
        "percent": state["percent"],
        "image_box": box,
        "yellow_pixels": state["colors"]["yellow"]["pixels"],
        "note": "最终只显示第二次打开的图片",
    })
    _assert(abs((box[2] - box[0]) - SMALL_SIZE[0]) <= 4, "最终图像不是第二次打开的小图")
    # 第二张图是粉色，而粉色与品红在容差内互相命中，不能用品红判定第一张图缺席；
    # 黄色只属于第一张四象限图，用它判定旧结果未上屏。
    _assert(state["colors"]["yellow"]["pixels"] == 0, "最终不应出现第一张图")
    click_button(driver, "关闭预览", "关闭预览（旧结果用例后）")
    wait_for(driver, lambda s: "适应窗口" not in s["flat"], "等待预览关闭")

    # 2) 关闭后的迟到结果。
    open_file(driver, fixture / "quadrants.png", evidence, "stale-closed")
    closed_open = time.monotonic()
    wait_for(driver, lambda s: LOADING_PHRASE in s["flat"], "等待加载开始（关闭用例）",
             timeout=10.0)
    click_button(driver, "关闭预览", "加载中关闭预览")
    state = wait_for(driver, lambda s: "适应窗口" not in s["flat"], "等待预览关闭")
    while time.monotonic() < closed_open + delay_seconds + 1.5:
        time.sleep(0.3)
    state = capture(driver, "迟到结果到达后采样")
    write_json(evidence / "case-stale-closed.json", {
        **observed(state),
        "elapsed_since_open_s": round(time.monotonic() - closed_open, 2),
        "delay_ms": DELAY_MS,
        "has_preview": "适应窗口" in state["flat"],
        "magenta_pixels": state["colors"]["magenta"]["pixels"],
        "title": driver.title(),
        "shot": state["shot"],
        "note": "加载中关闭后，迟到结果不得重新打开预览",
    })
    _assert("适应窗口" not in state["tree"], "迟到结果不应重新打开预览")
    _assert(state["colors"]["magenta"]["pixels"] == 0, "迟到结果不应上屏")
    _assert(driver.title() == "Aline", "关闭后窗口标题应保持应用名")

    # 3) 禁用期间的迟到结果。
    open_file(driver, fixture / "quadrants.png", evidence, "stale-disabled")
    disabled_open = time.monotonic()
    wait_for(driver, lambda s: LOADING_PHRASE in s["flat"], "等待加载开始（禁用用例）",
             timeout=10.0)
    activate(driver)
    driver.call(["hotkey", "--key", "cmd+alt+p"], "加载中关闭图片预览插件")
    state = wait_for(driver, lambda s: DISABLED_PHRASE in s["flat"], "等待插件关闭说明")
    while time.monotonic() < disabled_open + delay_seconds + 1.5:
        time.sleep(0.3)
    state = capture(driver, "禁用期间迟到结果采样")
    write_json(evidence / "case-stale-disabled.json", {
        **observed(state),
        "elapsed_since_open_s": round(time.monotonic() - disabled_open, 2),
        "delay_ms": DELAY_MS,
        "has_message": DISABLED_PHRASE in state["flat"],
        "has_path": "quadrants.png" in state["flat"],
        "magenta_pixels": state["colors"]["magenta"]["pixels"],
        "shot": state["shot"],
        "note": "禁用期间迟到的加载结果不得上屏；说明状态与路径保留",
    })
    _assert(DISABLED_PHRASE in state["flat"], "禁用说明应保留")
    _assert("quadrants.png" in state["flat"], "禁用期间应保留图片路径")
    _assert(state["colors"]["magenta"]["pixels"] == 0, "禁用期间的迟到结果不应上屏")

    activate(driver)
    driver.call(["hotkey", "--key", "cmd+alt+p"], "重新启用插件")
    state = wait_for(driver, ready_predicate, "等待重新启用后加载完成",
                     timeout=delay_seconds + 25.0)
    write_json(evidence / "case-stale-reenabled.json", {
        **observed(state),
        "percent": state["percent"],
        "note": "重新启用后重新加载当前路径并回到适应视图",
    })
    _assert(state["percent"] is not None and state["percent"] < 100,
            "重新启用后应回到适应视图")
    click_button(driver, "关闭预览", "关闭预览（禁用用例后）")
    wait_for(driver, lambda s: "适应窗口" not in s["flat"], "等待预览关闭")


def watch_app_exit(proc, evidence):
    """后台记录应用退出码与时刻：静默退出（无 panic 输出、无崩溃报告）也要留痕。

    returncode 0 表示进程自行正常退出（如最后一个窗口关闭）；
    负值表示信号（如 SIGKILL/SIGSEGV），用于区分正常退出与崩溃。
    """
    def loop():
        code = proc.wait()
        write_json(evidence / "app-exit.json", {
            "pid": proc.pid,
            "returncode": code,
            "exited_epoch_ns": time.time_ns(),
            "note": "应用进程退出记录；0 为正常退出，负值为信号",
        })
    threading.Thread(target=loop, daemon=True, name="app-exit-watch").start()


def launch(evidence, label, env_extra):
    """启动真实 aline 进程；日志先落临时目录，收尾时规范化写入证据目录。"""
    log_dir = Path(tempfile.mkdtemp(prefix=f"aline-m4-preview-{label}-"))
    env = os.environ.copy()
    env.update(env_extra)
    stdout = open(log_dir / "app.stdout", "wb")
    stderr = open(log_dir / "app.stderr", "wb")
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(native.ROOT), env=env,
                            stdout=stdout, stderr=stderr, start_new_session=True)
    write_json(evidence / f"launch-{label}.json", {
        "pid": proc.pid,
        "binary": "target/debug/aline",
        "env": {key: ("<evidence>" if key.endswith("EVIDENCE_DIR") else value)
                for key, value in env_extra.items()},
        "note": "脚本直接启动真实 aline 进程；证据目录由环境变量显式启用",
    })
    watch_app_exit(proc, evidence)
    return proc, stdout, stderr, log_dir


def stop(evidence, proc, stdout, stderr, log_dir):
    was_running = proc.poll() is None
    if was_running:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    write_json(evidence / "app-shutdown.json", {
        "pid": proc.pid,
        "alive_at_stop": was_running,
        "returncode": proc.returncode,
        "note": "脚本收尾对应用进程的处理记录；alive_at_stop 为 false 表示应用已自行退出",
    })
    for stream in (stdout, stderr):
        stream.flush()
        stream.close()
    for name in ("app.stdout", "app.stderr"):
        source = log_dir / name
        write_text(evidence / name,
                   source.read_text(errors="replace") if source.is_file() else "")
    shutil.rmtree(log_dir, ignore_errors=True)


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
        print("已有 aline 进程占用，拒绝干扰：" + running.stdout.strip(), file=sys.stderr)
        return 2
    if not APP_BINARY.is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2

    # 夹具放在 /Users/Shared 下（列视图只有 根→Users→Shared→夹具 四列，本机约 30 项）；
    # $TMPDIR 的 /var/folders 祖先链与 /tmp 的 /private/tmp 列各有数百项，
    # 面板快照上千节点、弹层会被 1200 节点上限截掉且每次调用慢到 3～4 秒
    # （run-preview-5/6）。路径在证据里按既有规则记为 <home>/Shared/...。
    fixture_root = Path(tempfile.mkdtemp(prefix="aline-m4-image-preview-",
                                         dir="/Users/Shared"))
    build_fixture(fixture_root)
    fixture_copy = evidence / "fixture"
    fixture_copy.mkdir(exist_ok=True)
    for name in ("quadrants.png", "small.png", "block.png", "broken.png", "body.md"):
        shutil.copyfile(str(fixture_root / name), str(fixture_copy / name))
    write_json(evidence / "fixture.json", {
        "quadrant_size": list(QUADRANT_SIZE),
        "small_size": list(SMALL_SIZE),
        "block_size": list(BLOCK_SIZE),
        "colors": {name: list(color) for name, color in COLORS.items()},
        "budget": BUDGET,
        "delay_ms": DELAY_MS,
        "note": "夹具在系统临时目录确定性生成；证据目录保留同一份副本",
    })
    cache = Path(tempfile.mkdtemp(prefix="aline-m4-preview-tools-"))
    ocr = compile_swift(READ_TEXT_SOURCE, cache, "read_text")
    phase1 = evidence / "phase1"
    phase2 = evidence / "phase2"
    phase1.mkdir()
    phase2.mkdir()
    require_fresh(phase1)
    require_fresh(phase2)
    try:
        # 第一阶段：正常延迟下的入口、视口与插件开关。
        proc, stdout, stderr, log_dir = launch(phase1, "phase1", {
            "ALINE_M2_EVIDENCE_DIR": str(phase1),
            "ALINE_M1_READONLY_EVIDENCE_DIR": str(phase1),
            BUDGET_ENV: BUDGET,
        })
        try:
            driver = native.WindowDriver(proc.pid, phase1)
            driver.ocr = ocr
            driver.wait_until_ready()
            write_text(phase1 / "input-source-before.txt", driver.input_source())
            case_fit_actual_zoom_pan(driver, phase1, fixture_root)
            case_error_retry(driver, phase1, fixture_root)
            case_plugin_off_resize(driver, phase1, fixture_root, cache)
            close_document_and_wait(driver)
        finally:
            stop(phase1, proc, stdout, stderr, log_dir)

        # 第二阶段：注入加载延迟，核对旧结果拒绝。
        proc, stdout, stderr, log_dir = launch(phase2, "phase2", {
            BUDGET_ENV: BUDGET,
            DELAY_ENV: str(DELAY_MS),
        })
        try:
            driver = native.WindowDriver(proc.pid, phase2)
            driver.ocr = ocr
            driver.wait_until_ready()
            case_stale_results(driver, phase2, fixture_root)
        finally:
            stop(phase2, proc, stdout, stderr, log_dir)

        facts = [
            "适应：百分比与截图中实测图像矩形一致，横向留边距（不放大）",
            "1:1：100% 时图像覆盖并超出视口，包围盒被裁剪",
            "缩放：100%→125%→100%→80% 按 1.25 步进",
            "平移：拖拽 (-120,-80) 与滚轮 -120 只改变偏移，百分比不变",
            "路径复用：同路径再次打开保留缩放与平移",
            "关闭：关闭预览恢复空状态与应用名标题",
            "错误与重试：损坏文件说明保留路径，修复后重试成功且小图不放大",
            "插件关闭：保留路径、拦截新请求；正文图片仍绘制；改尺寸后重新启用按新视口重算适应",
            "旧结果：被替换、关闭后、禁用期间的迟到结果都不上屏",
        ]
        limits = [
            "生产窗口不注入图片预算（V-06 未冻结），生产预览会显示未启用预算的说明；本取证以 ALINE_M4_MEDIA_BUDGET 注入",
            "旧结果用例依赖 debug 构建的 ALINE_M4_PREVIEW_DELAY_MS；生产构建不读取该变量",
            "预览与 Markdown 共用标签栏的集成归 M5，本取证只核对独立视口",
            "正文图片在插件关闭时仍绘制由本脚本核对，图片插入/资产事务见 3.2 取证",
        ]
        write_json(evidence / "result.json", {
            "status": "通过",
            "facts": facts,
            "limits": limits,
        })
    finally:
        shutil.rmtree(fixture_root, ignore_errors=True)
        shutil.rmtree(cache, ignore_errors=True)
    print("通过：证据目录 " + str(evidence))
    return 0


if __name__ == "__main__":
    sys.exit(main())
