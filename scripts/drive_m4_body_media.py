#!/usr/bin/env python3
"""M4 正文图片与链接协议真实窗口取证（任务 3.1）。

被测事实：
- 注入正文图片预算后，本地 PNG/GIF/SVG 在 Visual 视图原生解码并绘制；
  GIF 帧随时间推进（动画生效）；缺失目标保留替代文字。
- 远程图片目标不发起任何 HTTP 请求：夹具里的远程地址指向脚本自建的
  127.0.0.1 日志服务器，任何请求都会被记录并判本次取证失败。
- 链接只在平台修饰键（Cmd）点击时按 allowlist 路由：`#` 锚点不打开外部
  应用、不显示拒绝提示；`javascript:` 拒绝并在提示条显示拒绝文案；`http:`
  交给系统打开（本地日志服务器记录到请求即证明 open_url 已送达平台）。
- 图片展示不改文档：关闭后应用侧采样包的源码字节与磁盘逐字节一致。

图片由脚本确定性生成到临时目录（PIL），字节副本写入证据目录；文档由脚本
以 `ALINE_M4_MEDIA_BUDGET` 启动的真实 aline 进程打开，输入只经 macOS 无
障碍接口投递。应用侧由 `ALINE_M1_READONLY_EVIDENCE_DIR` 与
`ALINE_M2_EVIDENCE_DIR` 显式启用的采样器记录真实回调与关闭状态。

用法：drive_m4_body_media.py --evidence <证据目录>
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from PIL import Image, ImageChops

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    ROOT,
    WindowDriver,
)
from drive_m2_external_conflict import (  # noqa: E402  复用实例检查
    activate_cycle,
    check_single_instance,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_BINARY = ROOT / "target" / "debug" / "aline"
BUDGET_ENV = "ALINE_M4_MEDIA_BUDGET"
# 源字节,像素,解码字节,帧；仅 debug 构建读取，生产默认不注入。
BUDGET = "1048576,16777216,268435456,8"

# 正文区域在窗口内的纵向范围（与 drive_m2_editable_window 一致）。
CONTENT_TOP = 70.0
CONTENT_BOTTOM = 630.0

# 夹具图片尺寸（CSS px）：PNG/GIF 在首屏内完整可见，SVG 与两行替代文字
# 在向下滚动后可见；尺寸只影响布局，不改变支持矩阵判定。
RED_SIZE = (200, 100)
GIF_SIZE = (80, 80)
SVG_SIZE = (120, 45)
GIF_FRAME_MS = 300

COLORS = {
    "red": (220, 20, 20),
    "green": (20, 200, 40),
    "blue": (20, 60, 220),
    "magenta": (200, 30, 200),
    "yellow": (240, 200, 20),
}

# 文本列只占窗口左侧；图片在列内居中绘制，右半区不含任何文字。
# 扫描文本时只看左侧窄带，扫描图片颜色时只看右侧区域，两类像素互不干扰。
TEXT_SCAN_RIGHT = 340.0
IMAGE_SCAN_LEFT = 340.0

# 提示条底色（tokens::NOTICE_SURFACE = palette::GRAY_050）。
NOTICE_SURFACE = (246, 248, 250)
# 向下滚动的像素增量与滚动点（窗口 CSS 坐标）。
SCROLL_POINT = (480.0, 400.0)
SCROLL_DELTA = -420

REMOTE_ALT = "远程图不应取回"
MISSING_ALT = "缺失图替代文字"
REJECTED_TEXT = "被拒绝的协议"
ANCHOR_TEXT = "锚点链接"
EXTERNAL_TEXT = "外链"
REJECTED_FEEDBACK = "不支持的链接协议"


def fixture_markdown(remote_url, external_url):
    return (
        f"[{ANCHOR_TEXT}](#正文图片)\n"
        "\n"
        f"[{REJECTED_TEXT}](javascript:alert(1))\n"
        "\n"
        f"[{EXTERNAL_TEXT}]({external_url})\n"
        "\n"
        "![本地红块](big.png)\n"
        "\n"
        "![动画方块](anim.gif)\n"
        "\n"
        "![安全 SVG](safe.svg)\n"
        "\n"
        f"![{MISSING_ALT}](missing.png)\n"
        "\n"
        f"![{REMOTE_ALT}]({remote_url})\n"
    )


def build_fixture(directory, remote_url, external_url):
    """确定性生成图片与文档；返回文档路径与每个文件的字节数。"""
    directory.mkdir(parents=True, exist_ok=True)
    red = Image.new("RGB", RED_SIZE, COLORS["red"])
    red.save(directory / "big.png", format="PNG")

    frames = [
        Image.new("RGB", GIF_SIZE, COLORS["green"]),
        Image.new("RGB", GIF_SIZE, COLORS["blue"]),
    ]
    frames[0].save(directory / "anim.gif", format="GIF", save_all=True,
                   append_images=frames[1:], duration=GIF_FRAME_MS, loop=0)

    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d">'
        '<rect width="%d" height="%d" fill="#%02x%02x%02x"/></svg>\n'
        % (SVG_SIZE[0], SVG_SIZE[1], SVG_SIZE[0], SVG_SIZE[1],
           *COLORS["magenta"])
    )
    (directory / "safe.svg").write_text(svg, encoding="utf-8")

    document = directory / "body.md"
    document.write_text(fixture_markdown(remote_url, external_url),
                        encoding="utf-8")
    sizes = {}
    for path in sorted(directory.iterdir()):
        sizes[path.name] = {"bytes": path.stat().st_size,
                            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    return document, sizes


def png_bytes(size, color):
    import io
    buffer = io.BytesIO()
    Image.new("RGB", size, color).save(buffer, format="PNG")
    return buffer.getvalue()


class RequestLog:
    """127.0.0.1 请求日志；任何一条都写入证据并参与判定。"""

    def __init__(self):
        self.entries = []
        self.lock = threading.Lock()

    def record(self, path):
        with self.lock:
            self.entries.append({"path": path, "epoch_ns": time.time_ns()})

    def snapshot(self):
        with self.lock:
            return list(self.entries)


class RecordingHandler(BaseHTTPRequestHandler):
    log = None
    body = b""

    def do_GET(self):
        self.log.record(self.path)
        self.send_response(200)
        self.send_header("Content-Type", "image/png")
        self.send_header("Content-Length", str(len(self.body)))
        self.end_headers()
        self.wfile.write(self.body)

    def log_message(self, *args):
        pass


def start_server(log):
    handler = type("BoundHandler", (RecordingHandler,),
                   {"log": log, "body": png_bytes((40, 40), (240, 200, 20))})
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, server.server_address[1]


def launch(evidence):
    """启动真实 aline 进程；环境显式启用图片预算与两个应用侧采样器。"""
    log_dir = Path(tempfile.mkdtemp(prefix="aline-body-media-run-"))
    stdout = open(log_dir / "app.stdout", "wb")
    stderr = open(log_dir / "app.stderr", "wb")
    env = os.environ.copy()
    env["ALINE_M2_EVIDENCE_DIR"] = str(evidence)
    env["ALINE_M1_READONLY_EVIDENCE_DIR"] = str(evidence)
    env[BUDGET_ENV] = BUDGET
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(ROOT), env=env,
                            stdout=stdout, stderr=stderr, start_new_session=True)
    write_text(evidence / "app.pid", str(proc.pid) + "\n")
    write_json(evidence / "launch.json", {
        "pid": proc.pid,
        "started_epoch_ns": time.time_ns(),
        "binary": APP_BINARY.name,
        "cwd": str(ROOT),
        "env": {"ALINE_M2_EVIDENCE_DIR": "<evidence>",
                "ALINE_M1_READONLY_EVIDENCE_DIR": "<evidence>",
                BUDGET_ENV: BUDGET},
        "note": "脚本直接启动真实 aline 进程；图片预算只在 debug 构建读取",
    })
    return proc, stdout, stderr, log_dir


def settle_logs(evidence, stdout, stderr, log_dir):
    for stream in (stdout, stderr):
        stream.flush()
        stream.close()
    for name in ("app.stdout", "app.stderr"):
        source = log_dir / name
        if source.is_file() and source.stat().st_size:
            write_text(evidence / name, source.read_text(errors="replace"))
        else:
            write_text(evidence / name, "")
    shutil.rmtree(log_dir, ignore_errors=True)


def latest_shot(driver):
    shots = sorted(driver.evidence.glob("step-*.png"))
    if not shots:
        raise DriverError("证据目录没有截图")
    return shots[-1]


def capture(driver, note):
    driver.call(["get-app-state"], note)
    return latest_shot(driver)


def window_scale(driver, shot):
    snapshot = driver.snapshot()
    window = snapshot.get("window") or {}
    width = window.get("width")
    image = Image.open(shot)
    if not width:
        raise DriverError("窗口快照缺少宽度")
    return image.size[0] / float(width), image


def content_band(scale, height):
    top = int(CONTENT_TOP * scale)
    bottom = min(height, int(CONTENT_BOTTOM * scale))
    return top, bottom


def color_pixels(image, top, bottom, predicate, left=0.0, scale=1.0):
    """在给定纵向带内按谓词统计像素，返回计数与包围盒（截图坐标）。

    ``left`` 以 CSS px 给出，用于把扫描限制在图片区或文本区。
    """
    width, height = image.size
    pixels = image.load()
    start = int(left * scale)
    count = 0
    box = None
    for y in range(top, bottom, 2):
        for x in range(start, width, 2):
            if predicate(pixels[x, y]):
                count += 1
                box = [x, y, x, y] if box is None else [
                    min(box[0], x), min(box[1], y),
                    max(box[2], x), max(box[3], y)]
    return count, box


def near(pixel, color, tolerance=45):
    return all(abs(int(pixel[i]) - color[i]) <= tolerance for i in range(3))


def image_report(shot, scale):
    """图片区内的纯色像素计数与包围盒。

    只扫描图片区（列内居中的图片都在右半区），避免把蓝色链接文字计入
    GIF 蓝色帧。显示层做过色彩管理，参考色用较宽容差按最近色归类。
    """
    image = Image.open(shot).convert("RGB")
    top, bottom = content_band(scale, image.size[1])
    report = {}
    for name in ("red", "green", "blue", "magenta", "yellow"):
        count, box = color_pixels(
            image, top, bottom, lambda p, c=COLORS[name]: near(p, c, 85),
            left=IMAGE_SCAN_LEFT, scale=scale)
        report[name] = {"pixels": count, "bbox": box}
    boxes = [report[name]["bbox"] for name in ("green", "blue")
             if report[name]["bbox"]]
    gif_box = None
    if boxes:
        gif_box = [min(box[0] for box in boxes), min(box[1] for box in boxes),
                   max(box[2] for box in boxes), max(box[3] for box in boxes)]
    report["gif"] = {
        "green_pixels": report["green"]["pixels"],
        "blue_pixels": report["blue"]["pixels"],
        "bbox": gif_box,
    }
    return report


def frame_diff(first, second, box):
    """两帧在给定包围盒内的灰度差像素数；尺寸不同或无包围盒时返回 None。"""
    if box is None:
        return None
    left = Image.open(first).convert("L").crop(box)
    right = Image.open(second).convert("L").crop(box)
    difference = ImageChops.difference(left, right)
    return sum(1 for value in difference.getdata() if value > 24)


def is_text_pixel(pixel):
    """文本墨色：中性深色，或链接蓝（tokens::LINK = 0x0969DA）。

    链接蓝在显示层被色彩管理偏移到约 (46,103,211)；GIF 蓝色帧是
    (20,60,220)，其 g 分量明显更低，因此用 g > 80 把两者分开。
    """
    high = max(pixel)
    low = min(pixel)
    if high < 150 and high - low < 40:
        return True
    red, green, blue = pixel
    return blue > 140 and blue - red > 80 and blue - green > 50 and green > 80


def text_bands(shot, scale):
    """正文左侧文本区的中性深色或链接蓝文本行带。"""
    image = Image.open(shot).convert("RGB")
    width, height = image.size
    top, bottom = content_band(scale, height)
    right = min(width, int(TEXT_SCAN_RIGHT * scale))
    pixels = image.load()

    rows = []
    for y in range(top, bottom):
        xs = [x for x in range(0, right, 2) if is_text_pixel(pixels[x, y])]
        if xs:
            rows.append((y, min(xs), max(xs)))
    bands = []
    for y, left, right_edge in rows:
        if bands and y - bands[-1]["bottom"] <= 3:
            bands[-1]["bottom"] = y
            bands[-1]["left"] = min(bands[-1]["left"], left)
            bands[-1]["right"] = max(bands[-1]["right"], right_edge)
        else:
            bands.append({"top": y, "bottom": y,
                          "left": left, "right": right_edge})
    return bands


def notice_report(shot, scale):
    """正文区顶部提示条底色（NOTICE_SURFACE）的行数与范围。"""
    image = Image.open(shot).convert("RGB")
    width, height = image.size
    top, bottom = content_band(scale, height)
    limit = min(bottom, top + int(120 * scale))
    pixels = image.load()
    step = 4
    sampled = len(range(0, width, step))
    rows = []
    for y in range(top, limit):
        count = sum(1 for x in range(0, width, step)
                    if near(pixels[x, y], NOTICE_SURFACE, 6))
        if count > sampled * 0.5:
            rows.append(y)
    return {"rows": len(rows),
            "top": rows[0] if rows else None,
            "bottom": rows[-1] if rows else None}


def band_point(band, scale, dy=0.0):
    """文本带中心换算成窗口 CSS 坐标（点击与滚动都用这一坐标系）。"""
    return ((band["left"] + band["right"]) / 2.0 / scale,
            (band["top"] + band["bottom"]) / 2.0 / scale + dy)


def matching_band(bands, reference, tolerance=10):
    """按左右端点找回同一条文本带（提示条会让全部正文整体下移）。"""
    for band in bands:
        if (abs(band["left"] - reference["left"]) <= tolerance
                and abs(band["right"] - reference["right"]) <= tolerance):
            return band
    return None


def save_crop(shot, band, destination, scale, pad=8):
    image = Image.open(shot).convert("RGB")
    width, height = image.size
    box = (max(0, band["left"] - pad),
           max(0, band["top"] - pad),
           min(width, band["right"] + pad),
           min(height, band["bottom"] + pad))
    image.crop(box).save(destination)


def cmd_click(driver, point, note):
    # orca computer 的修饰键词表是 shift/control/alt/meta；macOS 上 Cmd = meta。
    driver.call(["click", "--x", str(round(point[0], 1)),
                 "--y", str(round(point[1], 1)), "--modifiers", "meta"], note)
    time.sleep(0.8)


def run(driver, document, log):
    check_single_instance(driver)
    driver.wait_until_ready()
    # 打开面板的「前往文件夹」走 System Events 键盘事件，必须先让本实例前台。
    activate_cycle(driver)
    time.sleep(0.8)
    driver.open_document(document)
    snapshot = driver.snapshot()
    scale, _ = window_scale(driver, capture(driver, "打开夹具后截图"))
    tree_before = driver.tree_text(snapshot)

    # 图片异步加载：轮询到红块出现或超时，记录等待时长。
    deadline = time.monotonic() + 8.0
    polls = 0
    report = {}
    while True:
        polls += 1
        shot = capture(driver, "轮询正文图片加载")
        report = image_report(shot, scale)
        if report["red"]["pixels"] > 0 or time.monotonic() >= deadline:
            break
        time.sleep(0.4)
    write_json(driver.evidence / "image-report.json",
               {"polls": polls, "report": report,
                "note": "图片区内的纯色像素计数与包围盒；GIF 以绿/蓝帧计数"})
    if report["red"]["pixels"] < 1000:
        raise DriverError("正文没有出现本地 PNG 红块：" + json.dumps(report))
    if report["gif"]["green_pixels"] + report["gif"]["blue_pixels"] < 800:
        raise DriverError("正文没有出现 GIF 帧：" + json.dumps(report))
    if report["yellow"]["pixels"]:
        raise DriverError("远程图片被绘制成黄色块：" + json.dumps(report))

    # 动画：GIF 两帧各 300ms，周期 600ms；单次截图约 240ms，固定间隔采样
    # 会与周期混叠（实测连续多次都落在同一帧）。用长短交替的间隔连拍，
    # 让采样相位相对 600ms 周期持续漂移，再两两比较。
    gif_box = report["gif"]["bbox"]
    shots = []
    samples = []
    for index, pause in enumerate((0.0, 0.05, 0.11, 0.05, 0.11, 0.05, 0.11, 0.05)):
        if pause:
            time.sleep(pause)
        shot = capture(driver, "动画帧采样 %d" % index)
        shots.append(shot)
        sample = image_report(shot, scale)
        samples.append({"green": sample["gif"]["green_pixels"],
                        "blue": sample["gif"]["blue_pixels"]})
    pairs = [{"a": a, "b": b, "diff": frame_diff(shots[a], shots[b], gif_box)}
             for a in range(len(shots)) for b in range(a + 1, len(shots))]
    write_json(driver.evidence / "animation-report.json",
               {"gif_bbox": gif_box, "samples": samples, "pairs": pairs,
                "note": "GIF 包围盒内两两截图灰度差像素数；任一大于 0 即帧推进"})
    if not any(pair["diff"] and pair["diff"] > 0 for pair in pairs):
        raise DriverError("多次采样里 GIF 没有推进：" + json.dumps(pairs))

    # 替代文字由正文行文本绘制；阅读器文本不在 AX 树里，如实记录并改用截图。
    alt_in_tree = {
        "missing": MISSING_ALT in tree_before,
        "remote": REMOTE_ALT in tree_before,
    }
    write_json(driver.evidence / "alt-report.json", {
        "missing_alt": MISSING_ALT, "remote_alt": REMOTE_ALT,
        "in_ax_tree": alt_in_tree,
        "note": "AX 树不含阅读器正文文本；替代文字以滚动后的截图裁片取证",
    })

    # 链接行在夹具最前：三个链接各占一个文本带，之后才是图片行。
    bands = text_bands(latest_shot(driver), scale)
    write_json(driver.evidence / "link-bands-before.json",
               {"bands": bands,
                "note": "左侧文本区文本带（含链接蓝）；前三条即三行链接"})
    if len(bands) < 3:
        raise DriverError("正文文本带不足，找不到三行链接：" + str(bands))
    links = bands[:3]

    # 锚点链接：allowlist 允许但没有可打开的外部目标，不得出现提示条。
    anchor_point = band_point(links[0], scale)
    cmd_click(driver, anchor_point, "Cmd+点击锚点链接")
    after_anchor = capture(driver, "锚点点击后截图")
    anchor_bands = text_bands(after_anchor, scale)
    anchor_matched = matching_band(anchor_bands, links[0])
    anchor_notice = notice_report(after_anchor, scale)
    anchor_report = {
        "point": [round(anchor_point[0], 1), round(anchor_point[1], 1)],
        "first_band_top_before": links[0]["top"],
        "first_band_top_after": anchor_matched["top"] if anchor_matched else None,
        "notice_surface_rows": anchor_notice["rows"],
        "bands_after": anchor_bands,
    }
    write_json(driver.evidence / "anchor-report.json", anchor_report)
    if anchor_matched is None:
        raise DriverError("锚点点击后找不到原链接文本带：" + json.dumps(anchor_report))
    if abs(anchor_matched["top"] - links[0]["top"]) > 4:
        raise DriverError("锚点链接点击后正文移动（出现提示条）")
    if anchor_notice["rows"] > 0:
        raise DriverError("锚点链接点击后出现提示条底色")

    # 拒绝链接：Cmd+点击后提示条出现，正文整体下移；提示条底色与裁片一并记录。
    rejected_point = band_point(links[1], scale)
    cmd_click(driver, rejected_point, "Cmd+点击 javascript: 链接")
    snapshot = driver.snapshot()
    after_rejected = latest_shot(driver)
    tree_after = driver.tree_text(snapshot)
    rejected_bands = text_bands(after_rejected, scale)
    rejected_matched = matching_band(rejected_bands, links[0])
    notice = notice_report(after_rejected, scale)
    shift = (rejected_matched["top"] - links[0]["top"]
             if rejected_matched else None)
    rejected_report = {
        "point": [round(rejected_point[0], 1), round(rejected_point[1], 1)],
        "first_band_top_before": links[0]["top"],
        "first_band_top_after": rejected_matched["top"] if rejected_matched else None,
        "notice_shift_px": shift,
        "notice_surface": notice,
        "bands_after": rejected_bands,
        "feedback_in_ax": REJECTED_FEEDBACK in tree_after,
        "tree_after_excerpt": tree_after[:2000],
    }
    write_json(driver.evidence / "rejected-link-report.json", rejected_report)
    if shift is None or shift <= 8:
        raise DriverError("拒绝链接后正文没有下移（未出现提示条）")
    if notice["top"] is not None:
        top = max(0, notice["top"] - 8)
        bottom = min(Image.open(after_rejected).size[1],
                     (rejected_matched["top"] if rejected_matched
                      else notice["bottom"]) - 2)
        save_crop(after_rejected, {"left": 0, "top": top, "right": 700,
                                   "bottom": bottom},
                  driver.evidence / "notice-crop.png", scale, pad=0)

    # 外链：提示条把正文整体下移，用同一平移量定位外链行；本地日志服务器
    # 收到 /open 即证明 cx.open_url 已把外链交给平台。
    external_shift = shift / scale
    external_point = band_point(links[2], scale, dy=external_shift)
    cmd_click(driver, external_point, "Cmd+点击 http 外链（交给系统打开）")
    opened = []
    deadline = time.monotonic() + 10.0
    while time.monotonic() < deadline:
        opened = [entry for entry in log.snapshot()
                  if entry["path"].startswith("/open")]
        if opened:
            break
        time.sleep(0.4)
    write_json(driver.evidence / "external-link-report.json", {
        "point": [round(external_point[0], 1), round(external_point[1], 1)],
        "requests": opened,
        "note": "本地日志服务器收到 /open 即证明 cx.open_url 已把外链交给平台",
    })
    if not opened:
        raise DriverError("外链点击后本地服务器没有收到 /open 请求")
    # 外链交给系统后会打开浏览器并夺走前台；切回被测实例再继续。
    activate_cycle(driver)
    time.sleep(0.6)

    # SVG 与两行替代文字在首屏之下：向下滚动后取证。
    driver.inject_scroll_wheel(SCROLL_POINT[0], SCROLL_POINT[1], 0,
                               SCROLL_DELTA, "向下滚动到正文底部")
    time.sleep(0.8)
    scrolled = capture(driver, "滚动后截图")
    scrolled_report = image_report(scrolled, scale)
    attempts = 1
    while scrolled_report["magenta"]["pixels"] < 300 and attempts < 4:
        driver.inject_scroll_wheel(SCROLL_POINT[0], SCROLL_POINT[1], 0,
                                   SCROLL_DELTA, "继续向下滚动")
        time.sleep(0.8)
        scrolled = capture(driver, "滚动后截图")
        scrolled_report = image_report(scrolled, scale)
        attempts += 1
    write_json(driver.evidence / "scrolled-image-report.json",
               {"attempts": attempts, "report": scrolled_report,
                "note": "滚动到正文底部后的图片区像素计数"})
    if scrolled_report["magenta"]["pixels"] < 300:
        raise DriverError("滚动后没有出现净化后的 SVG 品红块："
                          + json.dumps(scrolled_report))
    if scrolled_report["yellow"]["pixels"]:
        raise DriverError("滚动后出现远程图片黄色块：" + json.dumps(scrolled_report))
    svg_bottom = scrolled_report["magenta"]["bbox"][3] if \
        scrolled_report["magenta"]["bbox"] else 0
    below = 0
    image = Image.open(scrolled).convert("RGB")
    top, bottom = content_band(scale, image.size[1])
    for name in ("red", "green", "blue", "yellow"):
        count, _ = color_pixels(
            image, min(bottom, svg_bottom + int(40 * scale)), bottom,
            lambda p, c=COLORS[name]: near(p, c, 85),
            left=IMAGE_SCAN_LEFT, scale=scale)
        below += count
    write_json(driver.evidence / "below-svg-report.json", {
        "svg_bbox": scrolled_report["magenta"]["bbox"],
        "image_pixels_below": below,
        "note": "SVG 下方不应再有图片像素；缺失与远程图片行只保留替代文字",
    })
    if below:
        raise DriverError("SVG 下方仍有图片像素：" + str(below))

    scrolled_bands = text_bands(scrolled, scale)
    write_json(driver.evidence / "scrolled-bands.json",
               {"bands": scrolled_bands,
                "note": "滚动后的左侧文本带；最后两条应为缺失图与远程图替代文字"})
    if len(scrolled_bands) < 2:
        raise DriverError("滚动后文本带不足，找不到两行替代文字："
                          + str(scrolled_bands))
    missing_band = scrolled_bands[-2]
    remote_band = scrolled_bands[-1]
    save_crop(scrolled, missing_band,
              driver.evidence / "alt-missing-crop.png", scale)
    save_crop(scrolled, remote_band,
              driver.evidence / "alt-remote-crop.png", scale)

    # 远程图片：全程不得有任何 /remote.png 请求。
    requests = log.snapshot()
    remote = [entry for entry in requests if "remote.png" in entry["path"]]
    write_json(driver.evidence / "remote-requests.json", {
        "requests": requests, "remote_requests": remote,
        "note": "夹具只把远程地址指向本服务器；出现 /remote.png 即判失败",
    })
    if remote:
        raise DriverError("远程图片目标被请求：" + json.dumps(remote))

    # 关闭文档：应用侧采样包落盘后核对源码字节。
    driver.call(["hotkey", "--key", "cmd+w"], "关闭文档并写应用侧证据",
                allow_missing_window=True)
    deadline = time.monotonic() + 12.0
    expected = ("session.tsv", "final.tsv", "source-after.bin",
                "stored-after.bin", "disk-after.bin", "case-001/state.tsv")
    while True:
        present = {name: (driver.evidence / name).is_file()
                   for name in expected}
        if all(present.values()) or time.monotonic() >= deadline:
            break
        time.sleep(0.5)
    write_json(driver.evidence / "close-files.json", {
        "expected": list(expected), "present": present,
        "note": "关闭文档后应用侧采样器落盘的文件",
    })
    disk = Path(document).read_bytes()
    if not all(present.values()):
        raise DriverError("关闭后应用侧证据文件缺失：" + json.dumps(present))
    for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
        payload = (driver.evidence / name).read_bytes()
        if payload != disk:
            raise DriverError(name + " 与磁盘文档不一致（图片展示改动了文档）")
    case_state = (driver.evidence / "case-001" / "state.tsv").read_text()
    for flag in ("document_closed=true", "disk_bytes_equal=true",
                 "source_bytes_equal=true"):
        if flag not in case_state:
            raise DriverError("只读取证状态未确认：" + flag)
    write_json(driver.evidence / "document-unchanged.json", {
        "size": len(disk), "sha256": hashlib.sha256(disk).hexdigest(),
        "note": "关闭后 source/stored/disk 三个采样包与磁盘逐字节一致，只读取证确认文档已关闭且源码未变",
    })
    driver.close_window()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             text=True, capture_output=True)
    if running.stdout.strip():
        print("已有 aline 进程占用，拒绝干扰：" + running.stdout.strip(),
              file=sys.stderr)
        return 2
    if not APP_BINARY.is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2
    existing = [name for name in ("session.tsv", "final.tsv", "case-001/state.tsv")
                if (evidence / name).exists()]
    if existing:
        print("证据目录已有应用侧关闭文件，请换新目录：" + ",".join(existing),
              file=sys.stderr)
        return 2

    log = RequestLog()
    server, port = start_server(log)
    remote_url = "http://127.0.0.1:%d/remote.png" % port
    external_url = "http://127.0.0.1:%d/open" % port
    fixture_root = Path(tempfile.mkdtemp(prefix="aline-m4-body-media-"))
    document, sizes = build_fixture(fixture_root, remote_url, external_url)
    fixture_copy = evidence / "fixture"
    fixture_copy.mkdir(exist_ok=True)
    for path in fixture_root.iterdir():
        shutil.copyfile(str(path), str(fixture_copy / path.name))
    write_json(evidence / "fixture.json", {
        "document": document.name,
        "remote_url": remote_url,
        "external_url": external_url,
        "files": sizes,
        "note": "夹具由脚本确定性生成；字节副本在 fixture/ 下",
    })

    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        run(driver, document, log)
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:3000])
        print("驱动失败：" + str(error)[:1200], file=sys.stderr)
        return 1
    finally:
        if proc.poll() is None:
            write_json(evidence / "process-exit.json", {
                "pid": proc.pid, "note": "关闭窗口后进程未退出，记录后终止",
            })
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        else:
            write_json(evidence / "process-exit.json", {
                "pid": proc.pid, "exit_code": proc.returncode,
                "note": "正常关闭窗口后的退出状态",
            })
        settle_logs(evidence, stdout, stderr, log_dir)
        server.shutdown()
        server.server_close()
        shutil.rmtree(fixture_root, ignore_errors=True)
    write_json(evidence / "requests-final.json", {"requests": log.snapshot()})
    print("正文图片与链接协议取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
