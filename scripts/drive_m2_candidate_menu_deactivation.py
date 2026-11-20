#!/usr/bin/env python3
"""W5 3.2/4.1「候选窗展开时失活」与「菜单借焦后失活」真实窗口取证。

两段式真实场景，共用一份文档：

  A) 候选窗展开时失活：HID 事件流投递 `n`/`i` 开始真实拼音组合，经
     CGWindowList 确认输入法候选窗已上屏；随即由 System Events 让 Finder
     成为前台（窗口真实失活），再次读取窗口列表确认候选窗已消失。平台应把
     marked `ni` 原样自然提交一次，应用不得自行结束或重放。重新激活后原块
     上下文仍可用，继续 `hao` 并由空格提交一个候选字。

  B) 菜单借焦后失活：第二段组合 `ni` 后，经 System Events 真实点开「文件」
     菜单（菜单借走焦点，组合保留），记录上屏窗口几何；在菜单仍展开时让
     Finder 成为前台（菜单借焦后的真实失活），再次确认候选窗消失。本机实测
     菜单展开后系统候选窗先退场、但组合仍保留，因此场景 B 只证明菜单借焦后的
     失活，不把候选窗退场归因于失活。平台自然结果随后由关闭采样核对。重新
     激活后继续 `hao` 并提交一个候选字。

两段最终应形成同一插入点的连续字节 `ni<候选字>ni<候选字>`，四个提交各推进
一个撤销步；采样无文本长度回落。继续组合实际提交的候选字由采样读取（run-1
把首候选写死为「好」，实际提交「号」`e58fb7`，按失败保留；本驱动只要求单个
非 ASCII 可打印候选字）。脚本不记录候选正文：候选窗只记录 owner/layer/bounds/
alpha/onscreen，Orca 快照不含系统候选窗；菜单展开时不做区域截屏，避免把
候选正文写入证据。

输入经 HID 注入器（scripts/inject_key.swift）投递，事件只到前台应用，故每次
注入前都核对被测实例是前台；失活/激活经 System Events 真实切换。应用由脚本
以 `ALINE_M2_EVIDENCE_DIR` 启动，采样器在文档关闭路径落盘。
用法：drive_m2_candidate_menu_deactivation.py --fixture <路径> --evidence <证据目录>
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

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
from drive_m2_deactivation import (  # noqa: E402  复用前台核对与单点插入核对
    frontmost_unix_id,
    session_timeline,
    single_insertion,
)
from drive_m2_external_conflict import our_process  # noqa: E402  按 pid 定位实例
from drive_m2_menu_focus import (  # noqa: E402  复用激活与关闭核对原语
    activate_ready,
    clear_feedback_and_open_block,
    fixture_state,
    verify_close_packet,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
MENU = "文件"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
KEY_A = 0
KEY_H = 4
KEY_O = 31
KEY_I = 34
KEY_N = 45
KEY_SPACE = 49


def write_command(driver, note, argv, exit_code, stdout="", stderr="",
                  started=None, finished=None, extra=None):
    """记录一条外部命令；argv 只写仓库相对脚本名或占位说明。"""
    record = {
        "argv": argv,
        "note": note,
        "exit_code": exit_code,
        "started_epoch_ns": started if started is not None else time.time_ns(),
        "finished_epoch_ns": finished if finished is not None else time.time_ns(),
    }
    if stdout:
        record["stdout"] = stdout.strip()
    if stderr:
        record["stderr"] = stderr.strip()
    if extra:
        record.update(extra)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", record)


def set_frontmost_timed(driver, name, target, note, pause, timeout=20.0):
    """经 System Events 真实切换前台；限时避免菜单跟踪期间挂死。"""
    driver.serial += 1
    script = f'tell application "System Events" to set frontmost of {target} to true'
    started = time.time_ns()
    try:
        result = subprocess.run(["osascript", "-e", script], text=True,
                                capture_output=True, timeout=timeout)
        stdout, stderr = result.stdout, result.stderr
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        write_command(driver, note,
                      ["osascript", "-e", f"<System Events: frontmost {name}>"],
                      None, started=started, extra={"timed_out": True})
        raise DriverError("前台切换超时（" + name + "）：菜单跟踪期间未返回")
    write_command(driver, note,
                  ["osascript", "-e", f"<System Events: frontmost {name}>"],
                  exit_code, stdout, stderr, started)
    if exit_code != 0:
        raise DriverError("前台切换失败（" + name + "）：" + stderr[-600:])
    time.sleep(pause)
    return {
        "name": name,
        "frontmost_unix_id": frontmost_unix_id(),
        "note": note,
    }


def menubar_open(driver, note, timeout=15.0):
    """经 System Events 真实点开「文件」菜单并保持展开；失败即中止。

    菜单是应用自己的系统窗口，由菜单跟踪循环借走焦点；点击本身不改文档、
    不启动保存等待，也不选择任何菜单项。
    """
    driver.serial += 1
    script = (f'tell application "System Events" to tell process "{APP_PROCESS}" '
              f'to click menu bar item "{MENU}" of menu bar 1')
    started = time.time_ns()
    try:
        result = subprocess.run(["osascript", "-e", script], text=True,
                                capture_output=True, timeout=timeout)
        stdout, stderr = result.stdout, result.stderr
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        write_command(driver, note,
                      ["osascript", "-e", f"<System Events: open {MENU} menu>"],
                      None, started=started, extra={"timed_out": True})
        raise DriverError("打开菜单超时：" + MENU)
    write_command(driver, note,
                  ["osascript", "-e", f"<System Events: open {MENU} menu>"],
                  exit_code, stdout, stderr, started)
    if exit_code != 0:
        raise DriverError("打开菜单失败（" + MENU + "）：" + stderr[-600:])


def ensure_frontmost(driver, note):
    """确认被测实例是前台；HID 按键只到前台应用。"""
    actual = frontmost_unix_id()
    if actual == str(driver.pid):
        return {"already_frontmost": True, "frontmost_unix_id": actual,
                "note": note}
    switched = set_frontmost_timed(
        driver, APP_PROCESS, f'(first process whose unix id is {driver.pid})',
        note, 0.8)
    if switched["frontmost_unix_id"] != str(driver.pid):
        raise DriverError("前台不是被测实例（期望 pid " + str(driver.pid)
                          + "，实际 " + (switched["frontmost_unix_id"] or "未知")
                          + "）")
    switched["already_frontmost"] = False
    return switched


def ime_windows(windows, our_pid):
    """筛出已上屏且几何有效的输入法候选窗；不读窗口名称/候选正文。"""
    matches = []
    for window in windows:
        if window.get("pid") == our_pid:
            continue
        owner = str(window.get("owner") or "")
        lowered = owner.lower()
        if not ("微信输入法" in owner or "wetype" in lowered
                or "inputmethod" in lowered):
            continue
        bounds = window.get("bounds") or {}
        if (not window.get("onscreen") or int(bounds.get("w", 0)) <= 0
                or int(bounds.get("h", 0)) <= 0):
            continue
        matches.append(window)
    matches.sort(key=lambda item: int(item.get("layer") or 0), reverse=True)
    return matches


def read_windows(evidence, name):
    path = Path(evidence) / name
    if not path.is_file():
        raise DriverError("缺少窗口几何记录：" + name)
    return json.loads(path.read_text(errors="replace")).get("windows") or []


def verify_candidate_present(evidence, name, our_pid, label):
    matches = ime_windows(read_windows(evidence, name), our_pid)
    if not matches:
        raise DriverError(label + "：组合中未列出已上屏的输入法候选窗")
    top = matches[0]
    return {
        "owner": top.get("owner"),
        "layer": top.get("layer"),
        "bounds": top.get("bounds"),
        "onscreen": top.get("onscreen"),
    }


def verify_candidate_absent(evidence, name, our_pid, label):
    matches = ime_windows(read_windows(evidence, name), our_pid)
    if matches:
        raise DriverError(label + "：候选窗仍上屏，失活未使输入法窗口退场")
    return {"onscreen_ime_windows": 0}


def menu_windows_observation(evidence, our_pid):
    """记录菜单展开时被测进程自己的非普通层窗口（仅几何，不含名称）。"""
    windows = read_windows(evidence, "phase-b-menu-windows.json")
    result = []
    for window in windows:
        if window.get("pid") != our_pid:
            continue
        layer = int(window.get("layer") or 0)
        bounds = window.get("bounds") or {}
        if layer == 0:
            continue
        result.append({
            "layer": layer,
            "bounds": bounds,
            "onscreen": window.get("onscreen"),
            "alpha": window.get("alpha"),
        })
    return result


def candidate_character(inserted, label):
    """要求提交字节是单个非 ASCII 可打印候选字，并返回该字。

    不同轮次/上下文下输入法首候选可能变化（run-1 键入 hao 实际提交「号」，
    而非常见「好」）；把首候选写死会把正常候选提交误判为失败。这里只限定
    形态，实际字符由采样记录。
    """
    text = inserted.decode("utf-8")
    if len(text) != 1 or text.isascii() or not text.isprintable():
        raise DriverError(label + "提交不是单个候选字：" + inserted.hex())
    return text


def single_delta(previous, current):
    """从相邻两行文本中取出唯一一段插入；返回位置与字节。"""
    if len(current) <= len(previous):
        return None
    prefix = 0
    while prefix < len(previous) and previous[prefix] == current[prefix]:
        prefix += 1
    suffix = 0
    while (suffix < len(previous) - prefix
           and previous[-1 - suffix] == current[-1 - suffix]):
        suffix += 1
    added = current[prefix:len(current) - suffix]
    if not added or current != previous[:prefix] + added + previous[prefix:]:
        return None
    return {"position": prefix, "bytes": added}


def growth_events(rows):
    """采样中每次文本增长的插入事件（提交或失活自然结果）。"""
    events = []
    for index in range(1, len(rows)):
        previous_hex = rows[index - 1]["text_hex"]
        current_hex = rows[index]["text_hex"]
        if not previous_hex or not current_hex:
            continue
        previous = bytes.fromhex(previous_hex)
        current = bytes.fromhex(current_hex)
        if len(current) <= len(previous):
            continue
        delta = single_delta(previous, current)
        row = rows[index]
        events.append({
            "seq": int(row["seq"]),
            "from_size": len(previous),
            "to_size": len(current),
            "inserted": delta,
            "undo_depth": int(row["undo_depth"]),
            "redo_depth": int(row["redo_depth"]),
        })
    return events


def composition_episodes(rows):
    """把连续 composition_len>0 的采样行归并为组合段。"""
    episodes = []
    current = None
    for row in rows:
        length = int(row["composition_len"] or 0)
        if length > 0:
            if current is None:
                current = {"start_seq": int(row["seq"]),
                           "end_seq": int(row["seq"]), "hexes": []}
            current["end_seq"] = int(row["seq"])
            if row["composition_hex"]:
                current["hexes"].append(row["composition_hex"])
        elif current is not None:
            episodes.append(current)
            current = None
    if current is not None:
        episodes.append(current)
    return episodes


def verify_session(evidence, baseline_bytes):
    """核对四段提交顺序、形态、连续位置、撤销步、组合段与终态。

    失活自然提交必须是 marked 原文 `ni`；继续组合的提交必须是单个非 ASCII
    可打印候选字（实际字符随输入法上下文可能变化，不写死）。两次失活提交与
    两次候选提交必须首尾相接，落在同一插入点。
    """
    rows = session_table(evidence)
    events = growth_events(rows)
    episodes = composition_episodes(rows)
    lengths = [int(row["text_len"]) for row in rows]
    decreases = [int(rows[index]["seq"])
                 for index in range(1, len(rows))
                 if lengths[index] < lengths[index - 1]]
    deltas = [event["inserted"]["bytes"] if event["inserted"] else None
              for event in events]
    undo_sequence = [event["undo_depth"] for event in events]
    last_text = (bytes.fromhex(rows[-1]["text_hex"])
                 if rows[-1]["text_hex"] else b"")
    source_bytes = (Path(evidence) / "source-after.bin").read_bytes()

    episode_checks = []
    for index, episode in enumerate(episodes):
        hexes = set(episode["hexes"])
        if index in (0, 2):
            expected = bool(hexes & {"6e", "6e69"})
        else:
            expected = bool(hexes & {"68", "6861", "68616f"})
        episode_checks.append({"index": index, "hexes": sorted(hexes),
                               "matches_expected": expected})

    result = {
        "rows": len(rows),
        "growth_events": [
            {
                "seq": event["seq"],
                "from_size": event["from_size"],
                "to_size": event["to_size"],
                "position": (event["inserted"]["position"]
                             if event["inserted"] else None),
                "hex": (event["inserted"]["bytes"].hex()
                        if event["inserted"] else None),
                "undo_depth": event["undo_depth"],
                "redo_depth": event["redo_depth"],
            }
            for event in events
        ],
        "episodes": episodes,
        "episode_checks": episode_checks,
        "length_decreases": decreases,
        "actual_deltas_hex": [item.hex() if item else None for item in deltas],
        "last_text_equals_source": last_text == source_bytes,
        "note": "两段失活应各产生一次原始 ni 自然提交，两次继续组合应各提交单个"
                "候选字；顺序为 ni、候选、ni、候选，撤销深度逐次加一，文本长度"
                "不得回落，四次插入必须首尾相接",
    }
    write_json(Path(evidence) / "session-check.json", result)

    if decreases:
        raise DriverError("采样出现文本长度回落（可能有输入或提交丢失）")
    if len(events) != 4:
        raise DriverError("采样文本增长事件数应为 4，实际 "
                          + str(len(events)))
    if deltas[0] != b"ni" or deltas[2] != b"ni":
        raise DriverError("两次失活自然提交应各为原始 ni，实际 "
                          + json.dumps([item.hex() if item else None
                                        for item in deltas]))
    first_candidate = candidate_character(deltas[1], "场景 A 继续组合")
    second_candidate = candidate_character(deltas[3], "场景 B 继续组合")
    if undo_sequence != [1, 2, 3, 4]:
        raise DriverError("四次提交的撤销深度应为 1,2,3,4，实际 "
                          + str(undo_sequence))
    if len(episodes) != 4:
        raise DriverError("组合段数应为 4，实际 " + str(len(episodes)))
    if not all(check["matches_expected"] for check in episode_checks):
        raise DriverError("组合段内容与真实拼音 ni/hao 不符")
    if not result["last_text_equals_source"]:
        raise DriverError("采样末尾文本与关闭源字节不一致")

    start = events[0]["inserted"]["position"]
    positions = []
    expected_position = start
    for delta in deltas:
        positions.append(expected_position)
        expected_position += len(delta)
    actual_positions = [event["inserted"]["position"] for event in events]
    if actual_positions != positions:
        raise DriverError("四次插入未首尾相接：期望 " + str(positions)
                          + "，实际 " + str(actual_positions))

    result["candidate_hex"] = [deltas[1].hex(), deltas[3].hex()]
    result["candidate_text"] = [first_candidate, second_candidate]
    result["start_position"] = start
    write_json(Path(evidence) / "session-check.json", result)
    return result


def verify_insertion(evidence, baseline_bytes, expected_insertion, position):
    """最终源字节必须是基线在采样插入点的连续重建字节。"""
    final = (Path(evidence) / "source-after.bin").read_bytes()
    inserted = single_insertion(baseline_bytes, final)
    expected_final = (baseline_bytes[:position] + expected_insertion
                      + baseline_bytes[position:])
    result = {
        "baseline": {"size": len(baseline_bytes),
                     "sha256": hashlib.sha256(baseline_bytes).hexdigest()},
        "final": {"size": len(final),
                  "sha256": hashlib.sha256(final).hexdigest()},
        "expected_final": {"size": len(expected_final),
                           "sha256": hashlib.sha256(expected_final).hexdigest()},
        "expected_insertion": {"position": position,
                               "size": len(expected_insertion),
                               "hex": expected_insertion.hex()},
        "inserted": (None if inserted is None else {
            "position": inserted["position"],
            "size": len(inserted["bytes"]),
            "hex": inserted["bytes"].hex(),
            "text": inserted["bytes"].decode("utf-8"),
        }),
        "note": "两段失活与两次继续输入应合并为同一插入点的连续字节",
    }
    write_json(Path(evidence) / "insertion-check.json", result)
    if inserted is None:
        raise DriverError("最终源字节不是基线的单点连续插入")
    if inserted["position"] != position:
        raise DriverError("插入点应为 " + str(position)
                          + "，实际 " + str(inserted["position"]))
    if inserted["bytes"] != expected_insertion:
        raise DriverError("插入字节应为 " + expected_insertion.hex()
                          + "，实际 " + inserted["bytes"].hex())
    if final != expected_final:
        raise DriverError("最终源字节与预期重建不一致")
    return result


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    baseline_bytes = Path(fixture).read_bytes()
    (driver.evidence / "baseline.bin").write_bytes(baseline_bytes)
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    body_point = driver.heading_point()
    notes = {"body_point": [round(body_point[0], 1), round(body_point[1], 1)]}

    # 场景 A：候选窗展开时失活。
    clear_feedback_and_open_block(driver, body_point,
                                  "点击标题行打开块编辑（候选窗失活）")
    notes["phase_a_frontmost"] = ensure_frontmost(
        driver, "确认被测实例在前台（场景 A 组合前）")
    driver.inject_keys([KEY_N, KEY_I], "场景 A：HID 拼音组合 ni",
                       names=["n", "i"])
    time.sleep(0.6)
    notes["phase_a_frontmost_during"] = frontmost_unix_id()
    if notes["phase_a_frontmost_during"] != str(driver.pid):
        raise DriverError("场景 A 组合期间被测实例不在前台")
    driver.snapshot()
    driver.capture_windows(
        "场景 A：候选窗已上屏（仅几何，不含候选正文）",
        record_name="phase-a-before-deactivation.json")
    notes["phase_a_candidate"] = verify_candidate_present(
        driver.evidence, "phase-a-before-deactivation.json", driver.pid,
        "场景 A 失活前")

    notes["phase_a_deactivate"] = set_frontmost_timed(
        driver, "Finder", 'process "Finder"',
        "场景 A：候选窗展开时 Finder 成为前台（窗口失活）", 0.6)
    time.sleep(1.2)
    during_a = frontmost_unix_id()
    if during_a == str(driver.pid):
        raise DriverError("场景 A 失活未生效：前台仍是本实例")
    driver.capture_windows(
        "场景 A：失活后候选窗应退场",
        record_name="phase-a-after-deactivation.json")
    notes["phase_a_candidate_after"] = verify_candidate_absent(
        driver.evidence, "phase-a-after-deactivation.json", driver.pid,
        "场景 A 失活后")
    write_json(driver.evidence / "phase-a-deactivation.json", {
        "frontmost_during": during_a,
        "expected_pid": driver.pid,
        "candidate_before": notes["phase_a_candidate"],
        "candidate_after": notes["phase_a_candidate_after"],
        "note": "候选窗上屏时真实失活；失活后输入法窗口应不再上屏",
    })

    notes["phase_a_reactivate"] = set_frontmost_timed(
        driver, APP_PROCESS,
        f'(first process whose unix id is {driver.pid})',
        "场景 A：重新激活被测实例", 1.0)
    if notes["phase_a_reactivate"]["frontmost_unix_id"] != str(driver.pid):
        raise DriverError("场景 A 重新激活后前台不是被测实例")
    driver.snapshot()

    ensure_frontmost(driver, "场景 A 继续输入前确认前台")
    driver.inject_keys([KEY_H, KEY_A, KEY_O],
                       "场景 A：失活提交后继续 HID 拼音组合 hao",
                       names=["h", "a", "o"])
    time.sleep(0.6)
    driver.capture_windows(
        "场景 A：继续组合时候选窗再次上屏",
        record_name="phase-a-resume-composition.json")
    verify_candidate_present(driver.evidence, "phase-a-resume-composition.json",
                             driver.pid, "场景 A 继续组合")
    driver.snapshot()
    ensure_frontmost(driver, "场景 A 空格提交前确认前台")
    driver.inject_keys([KEY_SPACE], "场景 A：空格提交继续组合的候选字",
                       names=["space"])
    time.sleep(1.2)
    driver.snapshot()

    # 场景 B：菜单借焦后失活。
    ensure_frontmost(driver, "场景 B 组合前确认前台")
    driver.inject_keys([KEY_N, KEY_I], "场景 B：HID 拼音组合 ni",
                       names=["n", "i"])
    time.sleep(0.6)
    if frontmost_unix_id() != str(driver.pid):
        raise DriverError("场景 B 组合期间被测实例不在前台")
    driver.snapshot()
    driver.capture_windows(
        "场景 B：菜单展开前候选窗已上屏",
        record_name="phase-b-before-menu.json")
    notes["phase_b_candidate"] = verify_candidate_present(
        driver.evidence, "phase-b-before-menu.json", driver.pid,
        "场景 B 菜单前")

    notes["phase_b_menu_open"] = driver.serial + 1
    menubar_open(driver, "场景 B：组合进行中真实点开「文件」菜单（借焦）")
    time.sleep(0.5)
    driver.capture_windows(
        "场景 B：菜单展开时的上屏窗口几何（仅几何，不含名称/正文）",
        record_name="phase-b-menu-windows.json")
    notes["menu_windows"] = menu_windows_observation(
        driver.evidence, driver.pid)
    # 实测菜单展开后候选窗已不在上屏列表，但组合仍保留（失活时平台仍提交
    # marked `ni`）。这里只记录，不把候选窗退场归因于菜单或失活。
    notes["phase_b_candidate_during_menu"] = [
        {"owner": window.get("owner"), "layer": window.get("layer"),
         "bounds": window.get("bounds"), "onscreen": window.get("onscreen")}
        for window in ime_windows(
            read_windows(driver.evidence, "phase-b-menu-windows.json"),
            driver.pid)
    ]

    notes["phase_b_deactivate"] = set_frontmost_timed(
        driver, "Finder", 'process "Finder"',
        "场景 B：菜单借焦后 Finder 成为前台（窗口失活）", 0.6)
    time.sleep(1.2)
    during_b = frontmost_unix_id()
    if during_b == str(driver.pid):
        raise DriverError("场景 B 失活未生效：前台仍是本实例")
    driver.capture_windows(
        "场景 B：菜单借焦失活后候选窗应退场",
        record_name="phase-b-after-deactivation.json")
    notes["phase_b_candidate_after"] = verify_candidate_absent(
        driver.evidence, "phase-b-after-deactivation.json", driver.pid,
        "场景 B 失活后")
    write_json(driver.evidence / "phase-b-deactivation.json", {
        "frontmost_during": during_b,
        "expected_pid": driver.pid,
        "candidate_before": notes["phase_b_candidate"],
        "candidate_during_menu": notes["phase_b_candidate_during_menu"],
        "menu_windows": notes["menu_windows"],
        "candidate_after": notes["phase_b_candidate_after"],
        "note": "菜单展开时真实失活；失活后输入法窗口应不再上屏。菜单展开后"
                "候选窗可能先退场，候选窗展开时失活由场景 A 证明",
    })

    notes["phase_b_reactivate"] = set_frontmost_timed(
        driver, APP_PROCESS,
        f'(first process whose unix id is {driver.pid})',
        "场景 B：重新激活被测实例", 1.0)
    if notes["phase_b_reactivate"]["frontmost_unix_id"] != str(driver.pid):
        raise DriverError("场景 B 重新激活后前台不是被测实例")
    driver.snapshot()

    ensure_frontmost(driver, "场景 B 继续输入前确认前台")
    driver.inject_keys([KEY_H, KEY_A, KEY_O],
                       "场景 B：失活提交后继续 HID 拼音组合 hao",
                       names=["h", "a", "o"])
    time.sleep(0.6)
    driver.capture_windows(
        "场景 B：继续组合时候选窗再次上屏",
        record_name="phase-b-resume-composition.json")
    verify_candidate_present(driver.evidence, "phase-b-resume-composition.json",
                             driver.pid, "场景 B 继续组合")
    driver.snapshot()
    ensure_frontmost(driver, "场景 B 空格提交前确认前台")
    driver.inject_keys([KEY_SPACE], "场景 B：空格提交继续组合的候选字",
                       names=["space"])
    time.sleep(1.2)
    driver.snapshot()

    # 自动保存可能已在多次失活/提交之间落盘；等待后显式保存补齐。
    time.sleep(2.2)
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "两段场景结束后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "autosave_changed_disk": auto_save["sha256"] != before["sha256"],
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "两段失活与提交的最终写盘；详细字节核对在关闭采样后进行",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("两段场景结束后磁盘仍是基线（提交未落盘）")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    session_result = verify_session(driver.evidence, baseline_bytes)
    session_timeline(driver.evidence)
    first_candidate = bytes.fromhex(session_result["candidate_hex"][0])
    second_candidate = bytes.fromhex(session_result["candidate_hex"][1])
    expected_insertion = (b"ni" + first_candidate + b"ni" + second_candidate)
    verify_insertion(driver.evidence, baseline_bytes, expected_insertion,
                     session_result["start_position"])
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
        run(driver, fixture.resolve())
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
    print("候选窗/菜单借焦失活取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
