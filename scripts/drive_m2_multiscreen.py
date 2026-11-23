#!/usr/bin/env python3
"""W5 4.2「跨屏候选窗定位」真实窗口取证（macOS arm64）。

被测事实：把被测窗口移到外接显示器后，中文输入法的候选窗仍随插入点上屏，
且几何落在外接显示器范围内；组合与取消不改动源码/版本/历史；关闭后源码、
存储与磁盘三方一致。窗口位置经辅助功能 API 设置（本项目自写工具），候选窗
几何只记录 owner/layer/bounds/onscreen，不读取窗口名称或候选正文。

不同 backing scale 若主机两块显示器均为 2×，只记录该事实并列为未测，不伪造
缩放差异。Windows/Linux/Intel 按用户范围保留未测。

用法：drive_m2_multiscreen.py --fixture <路径> --evidence <证据目录>
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from evidence_paths import write_json, write_text
from validation_output import require_validation_path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_candidate_follow import candidate_from_record, composition_at  # noqa: E402
from drive_m2_editable_window import DriverError, ROOT, WindowDriver  # noqa: E402
from drive_m2_external_missing import launch, settle_logs, store_has_generations  # noqa: E402
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m2_menu_focus import (  # noqa: E402
    activate_ready, fixture_state, verify_close_packet,
)

CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
# 窗口移动工具源码在取证时编译到临时目录，只操作被测 PID 的窗口。
MOVE_SOURCE = ROOT / "scripts" / "fixtures" / "move_window.swift"
DISPLAYS_SOURCE = ROOT / "scripts" / "fixtures" / "display_scales.swift"


def compile_swift(source, cache, name):
    binary = cache / name
    result = subprocess.run(["swiftc", "-O", str(source), "-o", str(binary)],
                            text=True, capture_output=True)
    if result.returncode != 0:
        raise DriverError("编译 " + name + " 失败：" + result.stderr[-800:])
    return binary


def read_displays(cache):
    binary = compile_swift(DISPLAYS_SOURCE, cache, "display_scales")
    result = subprocess.run([str(binary)], text=True, capture_output=True)
    if result.returncode != 0:
        raise DriverError("读取显示器失败：" + result.stderr[-600:])
    return json.loads(result.stdout)


def move_window(cache, pid, x, y):
    binary = compile_swift(MOVE_SOURCE, cache, "move_window")
    result = subprocess.run([str(binary), str(pid), str(x), str(y)],
                            text=True, capture_output=True)
    if result.returncode != 0:
        raise DriverError("移动窗口失败：" + result.stderr[-600:])
    return result.stdout.strip()


def external_display(displays):
    """取外接显示器（非内建）的 frame；无外接屏时拒绝，不伪造跨屏。"""
    external = [s for s in displays["screens"] if not s["is_builtin"]]
    if not external:
        raise DriverError("主机无外接显示器，无法取得跨屏证据")
    return external[0]


def run(driver, fixture, cache):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    before = fixture_state(fixture)
    baseline_bytes = Path(fixture).read_bytes()
    (driver.evidence / "baseline.bin").write_bytes(baseline_bytes)
    write_json(driver.evidence / "fixture-before.json", before)

    driver.open_document(fixture)
    driver.snapshot()

    displays = read_displays(cache)
    write_json(driver.evidence / "display-state.json", displays)
    screen = external_display(displays)
    target_x = screen["frame"]["x"] + 120
    target_y = screen["frame"]["y"] + 90
    move_status = move_window(cache, driver.pid, target_x, target_y)
    time.sleep(1.2)
    driver.snapshot()
    window = driver.snapshot().get("window") or {}
    write_json(driver.evidence / "window-relocation.json", {
        "move_status": move_status,
        "target_display": screen,
        "target_origin": [target_x, target_y],
        "window_after": window,
        "distinct_backing_scales": sorted(
            {s["backing_scale"] for s in displays["screens"]}),
    })
    wx, wy = float(window.get("x", -1)), float(window.get("y", -1))
    frame = screen["frame"]
    if not (frame["x"] <= wx < frame["x"] + frame["w"]
            and frame["y"] <= wy < frame["y"] + frame["h"]):
        raise DriverError("窗口原点未落在外接显示器范围内："
                          + json.dumps({"window": window, "screen": screen},
                                       ensure_ascii=False))

    # 在已移屏的窗口内打开块并启动真实拼音组合，记录候选窗几何。
    point = driver.heading_point()
    composition = composition_at(driver, point, "外接屏上的中文组合",
                                 "candidate-external.json")
    candidate = composition["candidate"]
    bounds = candidate["bounds"]
    within = (frame["x"] <= bounds["x"] <= frame["x"] + frame["w"]
              and frame["y"] <= bounds["y"] <= frame["y"] + frame["h"])
    write_json(driver.evidence / "multiscreen-check.json", {
        "passed": bool(candidate.get("onscreen") and within),
        "window_on_external": True,
        "candidate_onscreen": candidate.get("onscreen"),
        "candidate_within_external_display": within,
        "candidate_offset": composition["candidate_offset"],
        "candidate": candidate,
        "note": "窗口与候选窗均在外接显示器范围；组合期间源码/版本/历史不变，"
                "Escape 取消后关闭采样三方一致；候选窗几何不含名称或候选正文",
    })
    if not (candidate.get("onscreen") and within):
        raise DriverError("候选窗未在外接显示器上屏或超出其范围")

    after = fixture_state(fixture)
    write_json(driver.evidence / "fixture-after.json", after)
    if after["sha256"] != before["sha256"]:
        raise DriverError("取消组合后磁盘夹具发生变化")

    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    if store_has_generations():
        raise DriverError("干净关闭后恢复存储仍有 Active 完整代")
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    if subprocess.run(["pgrep", "-f", "target/debug/aline"],
                      text=True, capture_output=True).stdout.strip():
        print("已有 aline 进程占用，拒绝干扰", file=sys.stderr)
        return 2
    if not (ROOT / "target" / "debug" / "aline").is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2
    if not fixture.is_file():
        print("夹具不存在：" + str(fixture), file=sys.stderr)
        return 2
    existing = [n for n in CLOSE_FILES if (evidence / n).exists()]
    if existing:
        print("证据目录已有应用侧关闭文件，请换新目录：" + ",".join(existing),
              file=sys.stderr)
        return 2
    if store_has_generations():
        print("恢复存储存在 Active 完整代，拒绝混用场景", file=sys.stderr)
        return 2

    import tempfile
    cache = Path(tempfile.mkdtemp(prefix="aline-multiscreen-"))
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        activate_ready(driver)
        run(driver, fixture.resolve(), cache)
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:3000])
        print("驱动失败：" + str(error)[:1200], file=sys.stderr)
        return 1
    finally:
        if proc.poll() is None:
            write_json(evidence / "process-exit.json", {
                "pid": proc.pid, "note": "进程未自行退出，记录后终止"})
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        else:
            write_json(evidence / "process-exit.json", {
                "pid": proc.pid, "exit_code": proc.returncode,
                "note": "正常关闭窗口后的退出状态"})
        settle_logs(evidence, stdout, stderr, log_dir)
    print("跨屏候选窗取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
