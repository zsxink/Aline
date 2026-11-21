#!/usr/bin/env python3
"""W4 异常恢复真实窗口取证：10 秒恢复发布 → kill -9 → 重启扫描 → 恢复/丢弃。

阶段 A（run-a）：打开夹具、进入块编辑、拼音组合保持未提交；等待独立 10 秒
恢复计时发布完整代（manifest 提交；`source.utf8` 含组合候选），期间源文件
字节不变；随后 SIGKILL 终止进程，模拟异常退出。

阶段 B（run-b）：重新启动；启动扫描呈现恢复面板（恢复/丢弃）。恢复 → 内存
文档接管窗口（源文件仍不变）→ 关闭文档后条目转为「已恢复」（只提供丢弃）→
丢弃先进入二次确认：取消保留记录，确认后退役清除。

恢复记录位于 $HOME/Library/Application Support/Aline/recovery/v1，证据只记
存储相对路径。应用侧由 ALINE_M2_EVIDENCE_DIR 采样器在文档关闭路径落盘。
用法：drive_m2_recovery_crash.py --fixture <路径> --evidence <证据目录>
"""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    ROOT,
    WindowDriver,
)
from drive_m2_menu_focus import menubar_click  # noqa: E402
from drive_m2_save_lifecycle import clear_feedback_and_open_block  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
APP_BINARY = ROOT / "target" / "debug" / "aline"
STORE_ROOT = (Path.home() / "Library" / "Application Support" / "Aline"
              / "recovery" / "v1")
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
COMPOSITION_KEYS = ["n", "i"]


def fixture_state(path):
    data = Path(path).read_bytes()
    stat = Path(path).stat()
    return {"size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "mtime_ns": stat.st_mtime_ns}


def store_snapshot():
    """恢复存储的只读快照；键均为相对 recovery/v1 的 POSIX 路径。"""
    entries = []
    if STORE_ROOT.is_dir():
        for base, _dirs, files in os.walk(STORE_ROOT):
            for name in files:
                path = Path(base) / name
                try:
                    stat = path.stat()
                    data = path.read_bytes()
                except OSError as error:
                    entries.append({"path": path.relative_to(STORE_ROOT).as_posix(),
                                    "error": type(error).__name__})
                    continue
                entries.append({
                    "path": path.relative_to(STORE_ROOT).as_posix(),
                    "size": stat.st_size,
                    "sha256": hashlib.sha256(data).hexdigest(),
                })
    entries.sort(key=lambda entry: entry["path"])
    generations = []
    active = STORE_ROOT / "active"
    if active.is_dir():
        for lineage in sorted(active.iterdir()):
            groups = lineage / "g"
            if not groups.is_dir():
                continue
            ordered = sorted(groups.iterdir(),
                             key=lambda path: (0, int(path.name))
                             if path.name.isdigit() else (1, path.name))
            for generation in ordered:
                if not generation.is_dir():
                    continue
                manifest = generation / "manifest"
                source = generation / "source.utf8"
                generations.append({
                    "document": lineage.name,
                    "generation": generation.name,
                    "manifest": manifest.is_file(),
                    "source_bytes": source.stat().st_size if source.is_file() else None,
                    "source_sha256": (hashlib.sha256(source.read_bytes()).hexdigest()
                                      if source.is_file() else None),
                })
    return {"files": entries, "generations": generations}


def osascript_step(driver, script, note):
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", "<System Events: " + note + ">"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise DriverError("激活失败：" + result.stderr[-600:])


def activate(driver):
    osascript_step(driver, f'tell application "System Events" to set frontmost '
                           f'of process "{APP_PROCESS}" to true',
                   "激活 aline 窗口")
    time.sleep(1.0)


def launch(evidence_dir):
    log_dir = Path(tempfile.mkdtemp(prefix="aline-recovery-run-"))
    stdout = open(log_dir / "app.stdout", "wb")
    stderr = open(log_dir / "app.stderr", "wb")
    env = os.environ.copy()
    env["ALINE_M2_EVIDENCE_DIR"] = str(evidence_dir)
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(ROOT), env=env,
                            stdout=stdout, stderr=stderr, start_new_session=True)
    write_text(evidence_dir / "app.pid", str(proc.pid) + "\n")
    write_json(evidence_dir / "launch.json", {
        "pid": proc.pid,
        "started_epoch_ns": time.time_ns(),
        "binary": APP_BINARY.name,
        "cwd": str(ROOT),
        "note": "脚本直接启动真实 aline 进程，环境只设 ALINE_M2_EVIDENCE_DIR",
    })
    return proc, stdout, stderr, log_dir


def settle_logs(evidence_dir, stdout, stderr, log_dir):
    for stream in (stdout, stderr):
        stream.flush()
        stream.close()
    for name in ("app.stdout", "app.stderr"):
        source = log_dir / name
        if source.is_file() and source.stat().st_size:
            write_text(evidence_dir / name, source.read_text(errors="replace"))
        else:
            write_text(evidence_dir / name, "")
    shutil.rmtree(log_dir, ignore_errors=True)


def wait_for_composition_generation(evidence_dir, expected_bytes, timeout):
    """等待最新完整代的 source.utf8 达到期望长度（含组合候选）。"""
    deadline = time.monotonic() + timeout
    observed = []
    while True:
        snapshot = store_snapshot()
        complete = [generation for generation in snapshot["generations"]
                    if generation["manifest"] and generation["source_bytes"]]
        elapsed = round(time.monotonic() - (deadline - timeout), 3)
        observed.append({"elapsed_seconds": elapsed,
                         "generations": snapshot["generations"]})
        if complete:
            latest = complete[-1]
            if latest["source_bytes"] == expected_bytes:
                result = {"generation": latest, "observed": observed,
                          "store": snapshot, "expected_bytes": expected_bytes}
                write_json(evidence_dir / "recovery-published.json", result)
                return latest, snapshot
        if time.monotonic() >= deadline:
            write_json(evidence_dir / "recovery-published-timeout.json", {
                "expected_bytes": expected_bytes,
                "observed": observed,
                "store": store_snapshot(),
                "note": "期限内没有出现含组合候选的完整恢复代（如实记录，不降级断言）",
            })
            raise DriverError("恢复发布未在期限内含组合候选；观察：" +
                              str(snapshot["generations"]))
        time.sleep(0.5)


def generation_dir(generation):
    return (STORE_ROOT / "active" / generation["document"] / "g"
            / generation["generation"])


def wait_files(directory, names, timeout):
    deadline = time.monotonic() + timeout
    present = {}
    while True:
        present = {name: (Path(directory) / name).is_file() for name in names}
        if all(present.values()) or time.monotonic() >= deadline:
            return present
        time.sleep(0.5)


def run_phase_a(fixture, evidence):
    """组合态等待恢复发布，然后 kill -9。"""
    before = store_snapshot()
    if before["generations"]:
        raise DriverError("恢复存储已有残留谱系，拒绝在同一场景混用：" +
                          str(before["generations"]))
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate(driver)
        write_json(evidence / "store-before.json", store_snapshot())
        fixture_before = fixture_state(fixture)
        write_json(evidence / "fixture-before.json", fixture_before)
        driver.open_document(fixture)
        driver.snapshot()
        body = driver.heading_point()
        body_point = [round(body[0], 1), round(body[1], 1)]
        clear_feedback_and_open_block(driver, body, "进入块编辑")
        driver.type_keys(COMPOSITION_KEYS, "拼音组合 ni（提交前保持）", pause=0.35)
        driver.snapshot()
        expected_bytes = fixture_before["size"] + len(
            "".join(COMPOSITION_KEYS).encode("utf-8"))
        generation, store_at_publish = wait_for_composition_generation(
            evidence, expected_bytes, 40.0)
        folder = generation_dir(generation)
        for name in ("source.utf8", "baseline.bytes", "manifest"):
            source = folder / name
            if source.is_file():
                shutil.copyfile(source, evidence / ("recovery-" + name))
        fixture_at_crash = fixture_state(fixture)
        if fixture_at_crash["sha256"] != fixture_before["sha256"]:
            raise DriverError("恢复发布期间源文件被改写")
        driver.snapshot()
        write_text(evidence / "input-source-before.txt", driver.input_source())
        os.kill(proc.pid, signal.SIGKILL)
        proc.wait(timeout=10)
        exit_code = proc.returncode
        write_json(evidence / "crash.json", {
            "pid": proc.pid,
            "signal": "SIGKILL",
            "exit_code": exit_code,
            "fixture": fixture_at_crash,
            "body_point": body_point,
            "recovery_generation": generation,
            "store": store_snapshot(),
            "note": "组合态恢复代完整发布后立即强制终止进程（未走关闭路径）",
        })
        if exit_code != -signal.SIGKILL:
            raise DriverError("进程未按 SIGKILL 退出：" + str(exit_code))
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        settle_logs(evidence, stdout, stderr, log_dir)


def panel_buttons(driver, snapshot):
    labels = ("恢复", "丢弃", "确认丢弃", "取消")
    return {label: driver.find_button(snapshot, label) for label in labels}


def require_button(driver, snapshot, label, note):
    index = driver.find_button(snapshot, label)
    if index is None:
        raise DriverError(f"{note}：无障碍树找不到「{label}」按钮")
    return index


def frontmost_application():
    result = subprocess.run(
        ["osascript", "-e",
         'tell application "System Events" to get name of first application '
         'process whose frontmost is true'],
        text=True, capture_output=True)
    return result.stdout.strip() or None


def tallest_text_point(driver):
    """在最新截图里找最高的正文文本带并返回其中心（窗口坐标）。

    恢复后的提示条位于正文上方，`heading_point` 会先命中提示文字；这里改取
    像素带高度最大的区域（标题行），与是否有提示条无关。
    """
    shot = sorted(driver.evidence.glob("step-*.png"))[-1]
    image = Image.open(shot).convert("L")
    width, height = image.size
    window = driver.snapshot().get("window", {})
    scale = width / float(window["width"]) if window.get("width") else 1.0
    pixels = image.load()
    top = int(70 * scale)
    bottom = min(height, int(630 * scale))
    bands = []
    current = None
    for y in range(top, bottom):
        count = sum(1 for x in range(0, width, 2) if pixels[x, y] < 140)
        if count > 0:
            current = [y, y] if current is None else [current[0], y]
        elif current is not None and y - current[1] > 6:
            bands.append(current)
            current = None
    if current is not None:
        bands.append(current)
    if not bands:
        raise DriverError("正文区域没有找到可命中的文本")
    band = max(bands, key=lambda item: item[1] - item[0])
    xs = [x for y in range(band[0], band[1] + 1) for x in range(width)
          if pixels[x, y] < 140]
    point = ((min(xs) + max(xs)) / 2 / scale, (band[0] + band[1]) / 2 / scale)
    write_json(driver.evidence / "hit-point-tallest.json",
               {"step": driver.serial, "point": [round(point[0], 1),
                                                 round(point[1], 1)],
                "band_px": band, "bands": bands})
    return point


def close_recovered_document(driver, fixture):
    """依次尝试多条关闭投递路径，逐条取证是否真正关闭恢复文档。

    恢复后的内存文档没有路径关联，关闭命中窗口级 CloseDocument。此前 run-b2
    在「激活 + cmd+w」与「文件菜单 AXPress」下都未关闭，本函数把两条路径拆开
    并加入诊断探针：cmd+f 打开查找面板说明按键确实进入应用；点正文再关闭
    用来区分应用内焦点状态。每一步都记录窗口标题、前台应用与探针结果，
    不在此处断言；调用方要求首条（激活后 cmd+w）即关闭，其余路径只是诊断兜底。
    """
    attempts = []
    name = Path(fixture).name

    def observe(label, path, extra=None):
        snapshot = driver.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        record = {"attempt": len(attempts) + 1, "label": label, "path": path,
                  "title": title, "closed": name not in title,
                  "frontmost": frontmost_application()}
        if extra:
            record.update(extra)
        attempts.append(record)
        return record

    activate(driver)
    driver.call(["hotkey", "--key", "cmd+w"], "关闭恢复文档：激活后 cmd+w",
                allow_missing_window=True)
    time.sleep(1.5)
    if observe("激活后 cmd+w", "hotkey")["closed"]:
        return attempts

    driver.call(["hotkey", "--key", "cmd+f"],
                "按键投递探针：cmd+f 应打开查找面板", allow_missing_window=True)
    time.sleep(0.8)
    snapshot = driver.snapshot()
    find_open = driver.find_row(snapshot, "查找") is not None
    if find_open:
        driver.call(["press-key", "--key", "escape"], "Escape 关闭查找面板",
                    allow_missing_window=True)
        time.sleep(0.5)
        driver.snapshot()
    if observe("cmd+f 探针", "hotkey",
               {"find_panel_opened": find_open})["closed"]:
        return attempts

    # 提示条会把正文整体下移；改按最高文本带（标题行）取点，避开提示文字。
    point = tallest_text_point(driver)
    driver.call(["click", "--x", str(round(point[0], 1)),
                 "--y", str(round(point[1], 1))],
                "点击标题行（避开恢复提示条），建立文档焦点")
    time.sleep(0.8)
    driver.snapshot()
    driver.call(["hotkey", "--key", "cmd+w"], "关闭恢复文档：正文聚焦后 cmd+w",
                allow_missing_window=True)
    time.sleep(1.5)
    if observe("正文聚焦后 cmd+w", "hotkey")["closed"]:
        return attempts

    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(
        ["osascript", "-e",
         f'tell application "System Events" to tell process "{APP_PROCESS}" '
         'to keystroke "w" using command down'],
        text=True, capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", "<System Events: keystroke cmd+w>"],
        "note": "关闭恢复文档：System Events 按键（另一条投递路径）",
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise DriverError("System Events 按键失败：" + result.stderr[-600:])
    time.sleep(1.5)
    if observe("System Events 按键 cmd+w", "osascript")["closed"]:
        return attempts

    menubar_click(driver, "关闭文档", "关闭恢复文档（文件菜单）")
    time.sleep(1.5)
    observe("文件菜单 关闭文档", "menu")
    return attempts


def run_phase_b(fixture, evidence, expect_source_sha256=None, restore_click=None):
    """启动扫描 → 恢复 → 关闭恢复文档 → 丢弃二次确认与退役。"""
    fixture_at_start = fixture_state(fixture)
    write_json(evidence / "fixture-before.json", fixture_at_start)
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate(driver)
        deadline = time.monotonic() + 8.0
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        while (buttons["恢复"] is None or buttons["丢弃"] is None) \
                and time.monotonic() < deadline:
            time.sleep(0.5)
            snapshot = driver.snapshot()
            buttons = panel_buttons(driver, snapshot)
        startup_store = store_snapshot()
        write_json(evidence / "startup-panel.json", {
            "buttons": buttons,
            "store": startup_store,
            "title": (snapshot.get("window") or {}).get("title"),
            "expected_source_sha256": expect_source_sha256,
            "note": "重启后的启动扫描；恢复面板应在无文档时呈现逐文档恢复/丢弃",
        })
        if buttons["恢复"] is None or buttons["丢弃"] is None:
            raise DriverError("启动扫描没有呈现恢复面板：" + str(buttons))
        if len(startup_store["generations"]) != 1:
            raise DriverError("启动扫描谱系数应为 1，实际：" +
                              str(startup_store["generations"]))
        if expect_source_sha256 is not None:
            complete = [generation for generation in startup_store["generations"]
                        if generation["manifest"] and generation["source_bytes"]]
            if len(complete) != 1 \
                    or complete[0]["source_sha256"] != expect_source_sha256:
                raise DriverError(
                    "启动扫描的完整代与期望谱系不符：期望 source_sha256="
                    + expect_source_sha256 + "，实际：" + str(complete))

        restore = require_button(driver, snapshot, "恢复", "启动面板")
        if restore_click is not None:
            click_x, click_y = restore_click
            driver.call(["click", "--x", str(click_x), "--y", str(click_y)],
                        "恢复记录到内存文档（窗口坐标点击，真实鼠标路径）")
        else:
            driver.call(["click", "--element-index", str(restore)],
                        "恢复记录到内存文档")
        deadline = time.monotonic() + 8.0
        while time.monotonic() < deadline:
            title = (driver.snapshot().get("window") or {}).get("title") or ""
            if Path(fixture).name in title:
                break
            time.sleep(0.5)
        snapshot = driver.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        fixture_after_restore = fixture_state(fixture)
        write_json(evidence / "restored.json", {
            "title": title,
            "buttons": panel_buttons(driver, snapshot),
            "fixture": fixture_after_restore,
            "store": store_snapshot(),
            "restore_click": list(restore_click) if restore_click else None,
            "note": "恢复只入内存；源文件保持不写，记录不被消费",
        })
        if Path(fixture).name not in title:
            raise DriverError("恢复后窗口标题不含夹具名：" + title)
        if fixture_after_restore["sha256"] != fixture_at_start["sha256"]:
            raise DriverError("恢复内存文档时源文件被改写")

        # 关闭投递路径逐条取证：激活后 cmd+w / cmd+f 探针 / 正文聚焦后 cmd+w /
        # System Events 按键 / 文件菜单。任何一条真正关闭即可继续收口。
        attempts = close_recovered_document(driver, fixture)
        write_json(evidence / "close-attempts.json", {
            "attempts": attempts,
            "closed": any(attempt["closed"] for attempt in attempts),
            "note": "恢复文档关闭的多路径投递记录；closed 表示窗口标题已离开夹具名",
        })
        # 回归门（M2-16）：恢复采纳后窗口根应立即持有焦点，激活后的首次
        # cmd+w 必须关闭文档。首条失败说明「面板按钮移除后焦点悬空」缺陷回归。
        if not attempts[0]["closed"]:
            raise DriverError(
                "恢复后首次 cmd+w 未关闭文档（窗口根焦点回归）：" +
                str([attempt["label"] for attempt in attempts]))
        snapshot = driver.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        if Path(fixture).name in title:
            raise DriverError("所有关闭投递路径都未关闭恢复文档：" +
                              str([attempt["label"] for attempt in attempts]))
        present = wait_files(evidence, CLOSE_FILES, 12.0)
        if not all(present.values()):
            raise DriverError("恢复文档关闭后采样证据缺失：" + str(present))
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        write_json(evidence / "restored-entry.json", {
            "buttons": buttons,
            "store": store_snapshot(),
            "close_files": present,
            "note": "已恢复条目只提供丢弃；记录保留至显式丢弃",
        })
        if buttons["丢弃"] is None:
            raise DriverError("关闭恢复文档后没有出现丢弃按钮")
        if buttons["恢复"] is not None:
            raise DriverError("已恢复条目仍提供恢复按钮")

        discard = require_button(driver, snapshot, "丢弃", "已恢复条目")
        driver.call(["click", "--element-index", str(discard)], "丢弃（进入二次确认）")
        time.sleep(0.5)
        snapshot = driver.snapshot()
        prompt = panel_buttons(driver, snapshot)
        write_json(evidence / "discard-prompt.json", {
            "buttons": prompt,
            "store": store_snapshot(),
            "note": "丢弃先进入二次确认；确认前存储不得变化",
        })
        if prompt["确认丢弃"] is None or prompt["取消"] is None:
            raise DriverError("丢弃没有进入二次确认：" + str(prompt))

        cancel = require_button(driver, snapshot, "取消", "丢弃确认")
        driver.call(["click", "--element-index", str(cancel)], "取消丢弃")
        time.sleep(0.5)
        snapshot = driver.snapshot()
        after_cancel = panel_buttons(driver, snapshot)
        write_json(evidence / "discard-cancelled.json", {
            "buttons": after_cancel,
            "store": store_snapshot(),
            "note": "取消后保持条目与存储不变",
        })
        if after_cancel["确认丢弃"] is not None or after_cancel["取消"] is not None:
            raise DriverError("取消后确认按钮仍存在")
        if after_cancel["丢弃"] is None:
            raise DriverError("取消后条目消失")

        discard = require_button(driver, snapshot, "丢弃", "取消后条目")
        driver.call(["click", "--element-index", str(discard)], "再次丢弃")
        time.sleep(0.5)
        snapshot = driver.snapshot()
        confirm = require_button(driver, snapshot, "确认丢弃", "二次确认")
        driver.call(["click", "--element-index", str(confirm)], "确认丢弃")
        deadline = time.monotonic() + 8.0
        cleaned = False
        while time.monotonic() < deadline:
            store = store_snapshot()
            if not store["generations"]:
                cleaned = True
                break
            time.sleep(0.5)
        time.sleep(0.5)
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        store = store_snapshot()
        write_json(evidence / "discard-confirmed.json", {
            "buttons": buttons,
            "store": store,
            "generations_cleared": cleaned,
            "note": "确认丢弃后退役并清理；条目与恢复代全部消失",
        })
        if buttons["丢弃"] is not None:
            raise DriverError("确认丢弃后条目仍在")
        if not cleaned:
            raise DriverError("确认丢弃后恢复代未清理：" + str(store["generations"]))

        fixture_final = fixture_state(fixture)
        write_json(evidence / "fixture-after.json", fixture_final)
        if fixture_final["sha256"] != fixture_at_start["sha256"]:
            raise DriverError("整个异常恢复场景改写了源文件")
        driver.close_window()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            write_json(evidence / "window-close-timeout.json",
                       {"pid": proc.pid, "note": "关闭窗口后进程未退出，记录后终止"})
            proc.terminate()
            proc.wait(timeout=10)
        write_json(evidence / "process-exit.json", {
            "pid": proc.pid,
            "exit_code": proc.returncode,
            "note": "正常关闭窗口后的退出状态",
        })
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        settle_logs(evidence, stdout, stderr, log_dir)


def existing_apps():
    result = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                            text=True, capture_output=True)
    return [line for line in result.stdout.splitlines() if line.strip().isdigit()]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--run-a", default="run-a",
                        help="强制终止阶段的证据子目录名（默认 run-a）")
    parser.add_argument("--run-b", default="run-b",
                        help="重启恢复阶段的证据子目录名（默认 run-b）")
    parser.add_argument("--phase-b-only", action="store_true",
                        help="跳过阶段 A，直接用存储中既有谱系执行阶段 B")
    parser.add_argument("--expect-source-sha256", default=None,
                        help="阶段 B 要求存储中唯一完整代的 source.utf8 摘要；"
                             "用于把阶段 B 绑定到既有阶段 A 的取证")
    parser.add_argument("--restore-click", default=None,
                        help="按窗口坐标点击恢复按钮的 x,y（真实鼠标路径）；"
                             "缺省用无障碍元素按下")
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    running = existing_apps()
    if running:
        print("已有 aline 进程占用，拒绝干扰：" + ",".join(running), file=sys.stderr)
        return 2
    if not APP_BINARY.is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2
    if args.phase_b_only and not args.expect_source_sha256:
        print("--phase-b-only 必须配合 --expect-source-sha256 绑定既有谱系",
              file=sys.stderr)
        return 2
    phase_a = evidence / args.run_a
    phase_b = evidence / args.run_b
    phase_b.mkdir(parents=True, exist_ok=True)
    restore_click = None
    if args.restore_click:
        try:
            restore_click = tuple(float(part) for part in args.restore_click.split(","))
        except ValueError:
            print("--restore-click 需要 x,y 两个数字", file=sys.stderr)
            return 2
        if len(restore_click) != 2:
            print("--restore-click 需要 x,y 两个数字", file=sys.stderr)
            return 2
    try:
        if not args.phase_b_only:
            phase_a.mkdir(parents=True, exist_ok=True)
            run_phase_a(fixture.resolve(), phase_a)
        run_phase_b(fixture.resolve(), phase_b, args.expect_source_sha256,
                    restore_click)
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:3000])
        print("驱动失败：" + str(error)[:1200], file=sys.stderr)
        return 1
    print("异常恢复取证完成：恢复发布→强制终止→重启扫描→恢复→丢弃", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
