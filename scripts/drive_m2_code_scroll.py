#!/usr/bin/env python3
"""代码块真实水平滚动与滚后命中取证（需要已授权的前台窗口操作）。

用法：--fixture <合成 Markdown> --evidence logs/validation/<新目录>。
夹具只读；应含一个两行以上、宽于正文列的代码块。像素核查只看代码内容、
行号和块外区域，拒绝把整窗闪烁当作滚动；关闭后核对源码/磁盘/历史未改变。
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import time

from PIL import Image, ImageChops

from drive_m2_editable_window import DriverError, ROOT, WindowDriver, session_table
from drive_m2_external_missing import launch, settle_logs, store_has_generations
from drive_m2_interaction_matrix import close_document_and_wait
from drive_m2_menu_focus import activate_ready, verify_close_packet
from evidence_paths import write_json, write_text
from validation_output import require_validation_path


def screenshot(driver):
    """截图与窗口点单位来自同一次读取。"""
    state = driver.snapshot()
    path = driver.evidence / f"step-{driver.serial:03d}.png"
    image = Image.open(path).convert("RGB")
    width = float(state["window"]["width"])
    return path, image, image.width / width


def code_regions(image, scale):
    """仅在正文内定位长条代码底色，再用暗像素识别实际代码文字高度。"""
    background = (246, 248, 250)
    rows = []
    for y in range(round(100 * scale), image.height - round(60 * scale)):
        xs = [x for x in range(round(50 * scale), image.width - round(60 * scale))
              if image.getpixel((x, y)) == background]
        if len(xs) > 500 * scale:
            rows.append((y, min(xs), max(xs)))
    if not rows:
        raise DriverError("未找到正文内宽代码底色区域")
    top, bottom = rows[0][0], rows[-1][0]
    left, right = min(row[1] for row in rows), max(row[2] for row in rows)
    content_left = left + round(65 * scale)
    content_right = right - round(22 * scale)
    ink_rows = [y for y in range(top, bottom + 1)
                if any(max(image.getpixel((x, y))) < 170
                       for x in range(left + round(20 * scale), content_right))]
    if not ink_rows:
        raise DriverError("代码区域内没有可见文字")
    # 首个文字带是语言标签，不作为滚轮命中点；正文代码位于其下。
    bands = []
    for y in ink_rows:
        if not bands or y - bands[-1][-1] > round(5 * scale):
            bands.append([y])
        else:
            bands[-1].append(y)
    if len(bands) < 3:
        raise DriverError("夹具必须提供语言标签与至少两行代码")
    text_top, text_bottom = bands[1][0], bands[-1][-1] + 1
    return {
        "content": (content_left, text_top, content_right, text_bottom),
        "gutter": (left + round(20 * scale), text_top,
                   left + round(38 * scale), text_bottom),
        "outside": (left, round(85 * scale), right, top - round(5 * scale)),
        "point": ((content_left + content_right) / 2 / scale,
                  (bands[1][0] + bands[1][-1]) / 2 / scale),
    }


def changed_pixels(first, second, box):
    diff = ImageChops.difference(first.crop(box), second.crop(box)).convert("L")
    return sum(count for shade, count in enumerate(diff.histogram()) if shade > 3)


def verify_pixels(before, right, restored, regions):
    """内容必须移动、行号和块外不动、反向滚动必须逐像素返回。"""
    if before.size != right.size or before.size != restored.size:
        raise DriverError("滚轮前后窗口截图尺寸变化")
    moved = changed_pixels(before, right, regions["content"])
    gutter = changed_pixels(before, right, regions["gutter"])
    outside = changed_pixels(before, right, regions["outside"])
    returned = changed_pixels(before, restored, regions["content"])
    if moved < 100 or gutter or outside or returned:
        raise DriverError(f"水平滚动像素不满足约束：内容={moved} 行号={gutter} "
                          f"块外={outside} 返回差异={returned}")
    return {"content_changed": moved, "gutter_changed": gutter,
            "outside_changed": outside, "restored_changed": returned}


def run(driver, fixture):
    baseline = fixture.read_bytes()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    driver.open_document(fixture)
    before_path, before, scale = screenshot(driver)
    regions = code_regions(before, scale)
    x, y = regions["point"]
    driver.inject_scroll_wheel(x, y, -300, 0, "指针位于代码内容，向右水平滚动")
    time.sleep(0.8)
    right_path, right, _ = screenshot(driver)
    driver.inject_scroll_wheel(x, y, 300, 0, "向左水平滚动返回")
    time.sleep(0.8)
    restored_path, restored, _ = screenshot(driver)
    proof = verify_pixels(before, right, restored, regions)
    write_json(driver.evidence / "scroll-proof.json", {
        **proof, "regions": regions, "scale": scale,
        "before": before_path.name, "right": right_path.name,
        "restored": restored_path.name,
    })
    # 同一点两次打开块，中间恢复阅读态并水平滚动；源码选区须向右推进。
    driver.call(["click", "--x", str(x), "--y", str(y)], "未滚动代码文字命中")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Escape"], "关闭代码块保留选区")
    time.sleep(0.5)
    driver.inject_scroll_wheel(x, y, -300, 0, "再次水平滚动后验证命中映射")
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["click", "--x", str(x), "--y", str(y)], "滚后同一点代码文字命中")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Escape"], "关闭代码块，无输入")
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    rows = session_table(driver.evidence)
    selected = [int(row["selection_head"]) for row in rows
                if row["open_block"] == "true"]
    if len(selected) != 2 or selected[1] <= selected[0]:
        raise DriverError("滚后同一点源码命中未向右推进：" + str(selected))
    for row in rows:
        if (int(row["revision"]) or int(row["undo_depth"]) or int(row["redo_depth"])
                or int(row["composition_len"] or 0) or int(row["draft_len"] or 0)):
            raise DriverError("无输入滚动产生版本/历史/草稿变化：seq " + row["seq"])
    if fixture.read_bytes() != baseline:
        raise DriverError("只读滚动与命中改变磁盘字节")
    if (driver.evidence / "source-after.bin").read_bytes() != baseline:
        raise DriverError("只读滚动与命中改变内存源码字节")
    write_json(driver.evidence / "result.json", {
        "passed": True, "source_selections": selected,
        "source_sha256": hashlib.sha256(baseline).hexdigest(),
        "source_bytes": len(baseline),
        "scope": "真实像素滚轮、行号固定、反向恢复、滚后命中；不证明 IME/其他平台",
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=False)
    fixture = args.fixture.resolve()
    if not fixture.is_file() or not (ROOT / "target/debug/aline").is_file():
        raise DriverError("缺少夹具或当前 debug 二进制")
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             capture_output=True, text=True)
    if running.stdout.strip() or store_has_generations():
        raise DriverError("已有应用或 Active 恢复记录，拒绝干扰")
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate_ready(driver)
        run(driver, fixture)
        code = proc.wait(timeout=10)
        write_json(evidence / "process-exit.json", {"exit_code": code})
        if code:
            raise DriverError("正常关闭后进程未成功退出")
    except (DriverError, subprocess.TimeoutExpired) as error:
        write_text(evidence / "driver-error.txt", str(error))
        print(str(error), file=sys.stderr)
        return 1
    finally:
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=10)
        settle_logs(evidence, stdout, stderr, log_dir)
    print("真实水平滚动、固定行号、返回与源码命中核查通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
