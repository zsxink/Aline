#!/usr/bin/env python3
"""W5 3.3「候选窗跟随插入点」真实窗口取证。

被测事实：同一长文档在两个不同可见位置分别开始真实拼音组合 `ni`，由
CGWindowList 只记录输入法候选窗几何（owner/layer/bounds），不读取窗口名称
或候选正文。两次候选窗都必须上屏，且第二次在滚动后的可见行开始组合时，
候选窗的屏幕位置随插入点下移；两次组合均可由 Escape 取消且源码/版本/历史
不变。滚动前另用 debug 布局报告证明文档处于虚拟列表（top_row>0、
rendered<rows），从而把第二处取证落在滚动后的虚拟列表可见行上。

用法：drive_m2_candidate_follow.py --fixture <路径> --evidence <证据目录>
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import time

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    ROOT,
    WindowDriver,
    session_table,
)
from drive_m2_external_missing import (  # noqa: E402  复用启动与存储快照原语
    launch,
    settle_logs,
    store_has_generations,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m2_menu_focus import (  # noqa: E402  复用块打开、激活与关闭核对原语
    activate_ready,
    clear_feedback_and_open_block,
    fixture_state,
    verify_close_packet,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
LAYOUT_REPORT = re.compile(
    r"m1-layout mode=(\S+) rows=(\d+) rendered=(\d+) height_cache=\S+ "
    r"top_row=(\d+)")
SCROLL_EVENTS = 3
SCROLL_DELTA = -900
SECOND_TARGET_Y = 500.0


def parse_layout_reports(text):
    """解析应用 stderr 的 debug 布局报告。"""
    reports = []
    for line in text.splitlines():
        hit = LAYOUT_REPORT.search(line)
        if hit:
            reports.append({
                "mode": hit.group(1),
                "rows": int(hit.group(2)),
                "rendered": int(hit.group(3)),
                "top_row": int(hit.group(4)),
            })
    return reports


def runtime_layout_reports(log_dir):
    path = Path(log_dir) / "app.stderr"
    if not path.is_file():
        return []
    return parse_layout_reports(path.read_text(errors="replace"))


def report_layout(driver, note):
    """触发 debug 布局报告；报告写入应用 stderr。"""
    driver.call(["hotkey", "--key", "cmd+alt+l"], note)
    time.sleep(0.8)


def text_band_point(driver, target_y):
    """在最新截图正文区找中心最接近 target_y 的文本带并返回帧坐标。"""
    driver.snapshot()
    shot = sorted(driver.evidence.glob("step-*.png"))[-1]
    image = Image.open(shot).convert("L")
    width, height = image.size
    window = driver.snapshot().get("window", {})
    scale = width / float(window["width"]) if window.get("width") else 1.0
    frame_height = float(window.get("height") or height / scale)
    pixels = image.load()
    top = int(90 * scale)
    bottom = min(height, int((frame_height - 60) * scale))
    bands = []
    band = None
    for y in range(top, bottom):
        count = sum(1 for x in range(0, width, 2) if pixels[x, y] < 140)
        if count > 0:
            band = [y, y] if band is None else [band[0], y]
        elif band is not None:
            if y - band[1] > 4:
                bands.append(band)
                band = None
    if band is not None:
        bands.append(band)
    if not bands:
        raise DriverError("滚动后正文区没有找到可命中的文本带")

    def center_y(item):
        return (item[0] + item[1]) / 2 / scale

    chosen = min(bands, key=lambda item: abs(center_y(item) - target_y))
    xs = []
    for y in range(chosen[0], chosen[1] + 1):
        for x in range(0, width):
            if pixels[x, y] < 140:
                xs.append(x)
    if not xs:
        raise DriverError("目标文本带内没有暗像素")
    point = ((min(xs) + max(xs)) / 2 / scale, center_y(chosen))
    write_json(driver.evidence / "second-hit-point.json",
               {"point": [round(point[0], 1), round(point[1], 1)],
                "target_y": target_y, "bands": len(bands),
                "band_px": chosen})
    return point


def candidate_from_record(evidence, record_name, our_pid):
    """从窗口几何记录中取已上屏的输入法候选窗，不读取名称/内容。"""
    path = Path(evidence) / record_name
    if not path.is_file():
        raise DriverError("缺少窗口几何记录：" + record_name)
    windows = json.loads(path.read_text(errors="replace")).get("windows") or []
    matches = []
    for window in windows:
        if window.get("pid") == our_pid:
            continue
        owner = str(window.get("owner") or "")
        lowered = owner.lower()
        if ("微信输入法" in owner or "wetype" in lowered
                or "inputmethod" in lowered):
            matches.append(window)
    if not matches:
        raise DriverError("组合中未列出输入法候选窗：" + record_name)
    matches.sort(key=lambda item: int(item.get("layer") or 0), reverse=True)
    top = matches[0]
    bounds = top.get("bounds") or {}
    if (not top.get("onscreen") or int(bounds.get("w", 0)) <= 0
            or int(bounds.get("h", 0)) <= 0):
        raise DriverError("候选窗几何无效：" + json.dumps(top, ensure_ascii=False))
    return {
        "owner": top.get("owner"),
        "layer": top.get("layer"),
        "bounds": {
            "x": float(bounds.get("x", 0)),
            "y": float(bounds.get("y", 0)),
            "w": float(bounds.get("w", 0)),
            "h": float(bounds.get("h", 0)),
        },
        "onscreen": top.get("onscreen"),
    }


def composition_at(driver, point, label, record_name):
    """在给定帧坐标打开块、组合 ni、记录候选窗后 Escape 取消。"""
    clear_feedback_and_open_block(driver, point, label + "：点击可见行打开块编辑")
    driver.type_keys(["n", "i"], label + "：拼音组合 ni", pause=0.35)
    time.sleep(0.7)
    driver.snapshot()
    driver.capture_windows(label + "：组合中的窗口几何（不含名称/候选正文）",
                           record_name=record_name)
    window = driver.snapshot().get("window") or {}
    candidate = candidate_from_record(driver.evidence, record_name, driver.pid)
    caret = [round(float(window.get("x", 0)) + point[0], 1),
             round(float(window.get("y", 0)) + point[1], 1)]
    driver.call(["press-key", "--key", "Escape"], label + "：Escape 取消组合")
    time.sleep(0.8)
    driver.snapshot()
    return {
        "point": [round(point[0], 1), round(point[1], 1)],
        "caret_screen_estimate": caret,
        "candidate": candidate,
        "candidate_offset": [
            round(candidate["bounds"]["x"] - caret[0], 1),
            round(candidate["bounds"]["y"] - caret[1], 1),
        ],
    }


def verify_follow(evidence, baseline_bytes, follow):
    """核对两处组合均零写入、候选窗随插入点下移且不落到远距离。"""
    rows = session_table(evidence)
    composition_rows = [(index, row) for index, row in enumerate(rows)
                        if int(row["composition_len"] or 0) > 0]
    starts = sorted({int(row["composition_start"] or -1)
                     for _, row in composition_rows})
    if len(starts) < 2:
        raise DriverError("采样未记录两处不同组合起点：" + str(starts))
    for _, row in composition_rows:
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("组合期间源码发生变化（seq " + row["seq"] + "）")
        if int(row["undo_depth"]) != 0 or row["redo_depth"] != "0":
            raise DriverError("组合期间历史深度变化（seq " + row["seq"] + "）")
        if row["open_block"] != "true":
            raise DriverError("组合期间块被关闭（seq " + row["seq"] + "）")

    first, second = follow
    caret_delta = second["caret_screen_estimate"][1] - first["caret_screen_estimate"][1]
    candidate_delta = second["candidate"]["bounds"]["y"] - first["candidate"]["bounds"]["y"]
    if caret_delta < 150:
        raise DriverError("两次插入点的屏幕纵向距离过小（" + str(caret_delta)
                          + "），不能证明跟随")
    if candidate_delta < 60:
        raise DriverError("候选窗未随插入点下移（纵向位移 " + str(candidate_delta) + "）")
    first_offset = abs(first["candidate_offset"][1])
    second_offset = abs(second["candidate_offset"][1])
    if first_offset > 160 or second_offset > 160:
        raise DriverError("候选窗与插入点纵向距离过大（" + str(first_offset)
                          + " / " + str(second_offset) + "）")

    result = {
        "composition_starts": starts,
        "gated_rows": [int(row["seq"]) for _, row in composition_rows],
        "caret_delta_y": round(caret_delta, 1),
        "candidate_delta_y": round(candidate_delta, 1),
        "first": first,
        "second": second,
        "note": "两次组合分别位于长文档两处可见行；候选窗均上屏并随插入点下移，"
                "组合期间源码/版本/历史不变，Escape 取消后关闭采样三方一致；"
                "候选窗几何来自已上屏窗口列表，不含窗口名称或候选正文",
    }
    write_json(Path(evidence) / "candidate-follow-check.json", result)
    return result


def run(driver, fixture, log_dir):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    baseline_bytes = Path(fixture).read_bytes()
    (driver.evidence / "baseline.bin").write_bytes(baseline_bytes)
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()

    first_point = driver.heading_point()
    report_layout(driver, "首屏布局报告（无组合）")
    top_report = runtime_layout_reports(log_dir)[-1:]
    if not top_report or top_report[0]["top_row"] != 0 \
            or top_report[0]["rendered"] >= top_report[0]["rows"]:
        raise DriverError("首屏布局报告未显示虚拟化")
    first = composition_at(driver, first_point, "第一处组合（首屏）",
                           "candidate-first.json")

    scroll_x = round(first_point[0], 1)
    scroll_y = 420.0
    for index in range(SCROLL_EVENTS):
        driver.inject_scroll_wheel(scroll_x, scroll_y, 0, SCROLL_DELTA,
                                   "向下滚动到虚拟列表后段（"
                                   + str(index + 1) + "/" + str(SCROLL_EVENTS) + "）")
        time.sleep(0.7)
    report_layout(driver, "滚动后布局报告（无组合）")
    down_report = runtime_layout_reports(log_dir)[-1:]
    if not down_report or down_report[0]["top_row"] < 1 \
            or down_report[0]["rendered"] >= down_report[0]["rows"]:
        raise DriverError("滚动后布局报告未显示原行移出虚拟渲染范围")
    second_point = text_band_point(driver, SECOND_TARGET_Y)
    second = composition_at(driver, second_point, "第二处组合（滚动后可见行）",
                            "candidate-second.json")

    after = fixture_state(fixture)
    write_json(driver.evidence / "fixture-after.json", after)
    if after["sha256"] != before["sha256"]:
        raise DriverError("两次取消组合后磁盘夹具发生变化")
    write_json(driver.evidence / "scenario-notes.json", {
        "first_point": first["point"],
        "second_point": second["point"],
        "top_report": top_report[0],
        "down_report": down_report[0],
        "fixture_before": before,
        "fixture_after": after,
    })
    write_json(driver.evidence / "layout-reports.json", {
        "reports": runtime_layout_reports(log_dir),
        "note": "cmd-alt-l 触发的 debug m1-layout 报告；top_row/rendered 证明虚拟化",
    })

    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    verify_follow(driver.evidence, baseline_bytes, (first, second))
    write_json(driver.evidence / "store-after.json", {
        "active_generations": store_has_generations(),
        "note": "干净关闭后的恢复存储 Active 完整代；应清空",
    })
    if store_has_generations():
        raise DriverError("干净关闭后恢复存储仍有 Active 完整代")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             text=True, capture_output=True)
    if running.stdout.strip():
        print("已有 aline 进程占用，拒绝干扰：" + running.stdout.strip(),
              file=sys.stderr)
        return 2
    if not (ROOT / "target" / "debug" / "aline").is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2
    if not fixture.is_file():
        print("夹具不存在：" + str(fixture), file=sys.stderr)
        return 2
    existing = [name for name in CLOSE_FILES if (evidence / name).exists()]
    if existing:
        print("证据目录已有应用侧关闭文件，请换新目录：" + ",".join(existing),
              file=sys.stderr)
        return 2
    generations = store_has_generations()
    if generations:
        print("恢复存储存在 Active 完整代，拒绝混用场景：" + ",".join(generations),
              file=sys.stderr)
        return 2
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate_ready(driver)
        run(driver, fixture.resolve(), log_dir)
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
    print("候选窗跟随插入点取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
