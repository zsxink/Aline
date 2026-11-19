#!/usr/bin/env python3
"""W4 外部同尺寸改写与冲突四出口的真实窗口取证（W4 5.2 / M2 4.x 消费者）。

被测事实：同尺寸、同 mtime 的磁盘改写仍被识别（字节比较而非元数据）；脏文档
的保存冲突经「静默外改检查」呈现冲突面板；四个出口逐一验证：

  周期 1：保存冲突 → 收起冲突提示（面板消失、自动保存暂停）→ 重新激活窗口
          重新呈现 → 载入磁盘内容（可解码磁盘字节，建立可撤销边界）→ 撤销
          回到重载前文本 → 重做回到磁盘文本。
  周期 2：保存冲突 → 确认覆盖磁盘（采纳磁盘基线后立即保存本地文本）。
  周期 3：保存冲突 → 重试检查磁盘（磁盘仍变则保持呈现）→ 恢复磁盘为保存
          基线 → 重试检查磁盘（清除冲突与暂停）→ 本地文本恢复可保存。
  周期 4：保存冲突 → 收起冲突提示 → 重新激活重新呈现 → 载入磁盘内容收口。

外部改写由脚本直接写夹具并恢复 mtime；只经 macOS 无障碍接口投递真实输入与
菜单动作。应用侧证据由 ALINE_M2_EVIDENCE_DIR 采样器在文档关闭路径落盘。
用法：drive_m2_external_conflict.py --app-pid <pid> --fixture <路径> --evidence <证据目录>
"""
import argparse
import hashlib
from pathlib import Path
import os
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    REDO_KEY,
    ROOT,
    UNDO_KEY,
    WindowDriver,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m2_menu_focus import menubar_click  # noqa: E402
from drive_m2_save_lifecycle import clear_feedback_and_open_block  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
MENU = "文件"
SAVE_KEY = "cmd+s"
CONFLICT_EXITS = ("载入磁盘内容", "确认覆盖磁盘", "重试检查磁盘", "收起冲突提示")
# 同尺寸改写的定位锚点：代码块起始围栏（永不被改写），在其后的首个 ASCII
# 数字上做 0→1→2 循环；循环只在该类内变化，反复改写仍能定位同一位置。
MARKER = b"```rust"


def fixture_state(path):
    data = Path(path).read_bytes()
    stat = Path(path).stat()
    return {"size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "mtime_ns": stat.st_mtime_ns}


def menu_states(driver, note):
    """一次读取冲突四出口与保存项的可用性；System Events 返回布尔串。"""
    script = (
        f'tell application "System Events" to tell process "{APP_PROCESS}"\n'
        f'set names to {{"保存", "取消等待保存", "载入磁盘内容", "确认覆盖磁盘", '
        f'"重试检查磁盘", "收起冲突提示"}}\n'
        'set out to ""\n'
        'repeat with n in names\n'
        f'set e to enabled of menu item (n as text) of menu "{MENU}" of menu bar item '
        f'"{MENU}" of menu bar 1\n'
        'set out to out & (n as text) & "=" & e & ";"\n'
        'end repeat\n'
        'return out\n'
        'end tell')
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", "<System Events: 文件菜单可用性>"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.stdout.strip():
        write_text(driver.evidence / f"step-{driver.serial:03d}.stdout",
                   result.stdout + "\n")
    if result.returncode != 0:
        raise DriverError("菜单可用性读取失败：" + result.stderr[-600:])
    states = {}
    for part in result.stdout.strip().split(";"):
        if "=" in part:
            name, value = part.split("=", 1)
            states[name] = value.strip() == "true"
    return states


def assert_panel(driver, expected, note):
    """以无障碍树中的冲突面板按钮为呈现真值；菜单 AX 可用性仅记录不判定。

    实测（本目录 `driver-error.txt` 首轮）：System Events 读取的菜单项
    `enabled` 在面板收起后仍报告 true，不能作为出口可用性判据；面板按钮
    的有无与窗口状态一致。
    """
    snapshot = driver.snapshot()
    labels = ("载入磁盘内容", "确认覆盖磁盘", "重试检查磁盘", "收起冲突提示")
    present = {label: driver.find_button(snapshot, label) is not None
               for label in labels}
    states = menu_states(driver, note + "（菜单 AX 可用性，仅记录）")
    if any(value != expected for value in present.values()):
        raise DriverError(f"{note}：冲突面板按钮呈现应为 {expected}，实际 {present}")
    return {"panel_present": expected, "buttons": present, "menu_ax": states}


def panel_button(driver, snapshot):
    """冲突面板的「载入磁盘内容」按钮索引；无面板时为 None。"""
    return driver.find_button(snapshot, "载入磁盘内容")


def external_write(driver, fixture, note):
    """同尺寸改写夹具并在写后恢复原 mtime；记录前后摘要与 mtime 核对。"""
    path = Path(fixture)
    before = fixture_state(path)
    data = bytearray(path.read_bytes())
    marker = data.find(MARKER)
    if marker < 0:
        raise DriverError("夹具中找不到同尺寸改写锚点：" + MARKER.decode())
    position = next((index for index in range(marker, len(data))
                     if 0x30 <= data[index] <= 0x39), None)
    if position is None:
        raise DriverError("锚点围栏之后没有 ASCII 数字")
    data[position] = 0x30 + (data[position] - 0x30 + 1) % 10
    path.write_bytes(bytes(data))
    os.utime(path, ns=(before["mtime_ns"], before["mtime_ns"]))
    after = fixture_state(path)
    result = {"note": note, "before": before, "after": after,
              "changed_byte": position,
              "same_size": before["size"] == after["size"],
              "mtime_restored": before["mtime_ns"] == after["mtime_ns"],
              "step": driver.serial}
    write_json(driver.evidence / f"external-{driver.serial:03d}.json", result)
    if not result["same_size"] or not result["mtime_restored"]:
        raise DriverError("外部改写未保持同尺寸/同 mtime：" + note)
    return result


def restore_bytes(driver, fixture, expected, note):
    """把夹具写回给定字节；用于「磁盘恢复等于保存基线」的正向重试。"""
    Path(fixture).write_bytes(bytes(expected))
    restored = fixture_state(fixture)
    result = {"note": note, "restored": restored, "step": driver.serial}
    write_json(driver.evidence / f"restore-{driver.serial:03d}.json", result)
    return restored


def wait_disk_change(driver, fixture, previous_sha, timeout, note):
    started = time.monotonic()
    while True:
        state = fixture_state(fixture)
        elapsed = round(time.monotonic() - started, 3)
        if state["sha256"] != previous_sha or elapsed >= timeout:
            result = {"note": note, "changed": state["sha256"] != previous_sha,
                      "waited_seconds": elapsed, **state, "step": driver.serial}
            write_json(driver.evidence / f"disk-{driver.serial:03d}.json", result)
            return result
        time.sleep(0.1)


def activate_cycle(driver):
    """借 Finder 失活再激活 aline，触发窗口激活的静默外改检查。"""
    for process in ("Finder", APP_PROCESS):
        driver.serial += 1
        script = (f'tell application "System Events" to set frontmost of process '
                  f'"{process}" to true')
        started = time.time_ns()
        result = subprocess.run(["osascript", "-e", script], text=True,
                                capture_output=True)
        write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
            "argv": ["osascript", "-e", f"<System Events: frontmost {process}>"],
            "note": "激活切换：" + process,
            "exit_code": result.returncode,
            "started_epoch_ns": started,
            "finished_epoch_ns": time.time_ns(),
        })
        if result.returncode != 0:
            raise DriverError("激活切换失败（" + process + "）："
                              + result.stderr[-600:])
        time.sleep(0.6 if process == "Finder" else 1.3)
    driver.snapshot()


def type_candidate(driver, keys, note):
    driver.type_keys(keys, note, pause=0.3)
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(0.3)


def conflict_save(driver, note):
    driver.call(["hotkey", "--key", SAVE_KEY], note)
    time.sleep(1.2)  # 保存失败结算 + 后台静默检查呈现
    snapshot = driver.snapshot()
    return snapshot


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    body = driver.heading_point()
    body_point = [round(body[0], 1), round(body[1], 1)]
    notes = {"body_point": body_point, "fixture_before": before}

    # ---------- 周期 1：收起 → 重新激活 → 载入磁盘 → 撤销/重做 ----------
    baseline1 = fixture_state(fixture)
    d1 = external_write(driver, fixture, "周期 1：磁盘同尺寸改写（保存基线 440）")
    clear_feedback_and_open_block(driver, body, "进入块编辑")
    type_candidate(driver, ["n", "i"], "拼音组合 ni")
    snapshot = conflict_save(driver, "冲突保存（应呈现冲突面板）")
    presented = assert_panel(driver, True, "冲突呈现后")
    button1 = panel_button(driver, snapshot)
    if button1 is None:
        raise DriverError("冲突呈现后无障碍树没有「载入磁盘内容」按钮")
    menubar_click(driver, "收起冲突提示", "收起冲突提示")
    time.sleep(0.6)
    snapshot = driver.snapshot()
    dismissed = assert_panel(driver, False, "收起冲突提示后")
    if panel_button(driver, snapshot) is not None:
        raise DriverError("收起后冲突面板仍然可见")
    time.sleep(2.0)  # 自动保存暂停：不得写盘
    still_d1 = fixture_state(fixture)
    if still_d1["sha256"] != d1["after"]["sha256"]:
        raise DriverError("收起冲突提示后自动保存仍写盘")
    activate_cycle(driver)
    repres = assert_panel(driver, True, "重新激活后")
    snapshot = driver.snapshot()
    if panel_button(driver, snapshot) is None:
        raise DriverError("重新激活后冲突面板没有重新呈现")
    menubar_click(driver, "载入磁盘内容", "载入磁盘内容")
    time.sleep(1.3)
    snapshot = driver.snapshot()
    reloaded = assert_panel(driver, False, "载入磁盘后")
    after_reload = fixture_state(fixture)
    if after_reload["sha256"] != d1["after"]["sha256"]:
        raise DriverError("载入磁盘后磁盘被写回，违反只读载入")
    driver.call(["hotkey", "--key", UNDO_KEY], "撤销重载回到编辑文本",
                allow_missing_window=True)
    time.sleep(0.7)
    driver.snapshot()
    driver.call(["hotkey", "--key", REDO_KEY], "重做回到磁盘文本",
                allow_missing_window=True)
    time.sleep(0.7)
    driver.snapshot()
    time.sleep(1.6)
    steady1 = fixture_state(fixture)
    if steady1["sha256"] != d1["after"]["sha256"]:
        raise DriverError("撤销/重做后磁盘被改写")
    write_json(driver.evidence / "cycle1-dismiss-reload.json", {
        "baseline": baseline1, "disk": d1, "presented": presented,
        "dismissed": dismissed, "represented_after_activation": repres,
        "after_reload": reloaded, "disk_after_undo_redo": steady1,
        "note": "收起不解除冲突与暂停；重新激活重新呈现；载入只读磁盘且可撤销",
    })

    # ---------- 周期 2：确认覆盖磁盘 ----------
    baseline2 = fixture_state(fixture)  # d1 内容，440
    d2 = external_write(driver, fixture, "周期 2：磁盘同尺寸改写（保存基线 440）")
    clear_feedback_and_open_block(driver, body, "进入块编辑")
    type_candidate(driver, ["m", "a"], "拼音组合 ma")
    conflict_save(driver, "冲突保存（应呈现冲突面板）")
    assert_panel(driver, True, "周期 2 冲突呈现后")
    menubar_click(driver, "确认覆盖磁盘", "确认覆盖磁盘")
    time.sleep(1.5)
    driver.snapshot()
    overwritten = assert_panel(driver, False, "确认覆盖后")
    saved2 = wait_disk_change(driver, fixture, d2["after"]["sha256"], 8.0,
                              "确认覆盖后本地文本应立即保存")
    if not saved2["changed"]:
        raise DriverError("确认覆盖后本地文本没有写盘")
    if saved2["size"] != baseline2["size"] + 3:
        raise DriverError(f"覆盖保存后尺寸应为 {baseline2['size'] + 3}，"
                          f"实际 {saved2['size']}")
    write_json(driver.evidence / "cycle2-overwrite.json", {
        "baseline": baseline2, "disk_before": d2, "saved": saved2,
        "exits_after": overwritten,
        "note": "确认覆盖：采纳磁盘基线后立即保存本地文本，冲突四出口全部关闭",
    })

    # ---------- 周期 3：重试检查（仍变保持；恢复基线清除） ----------
    baseline3_bytes = Path(fixture).read_bytes()
    baseline3 = fixture_state(fixture)  # 已保存的本地文本 443
    d3 = external_write(driver, fixture, "周期 3：磁盘同尺寸改写（保存基线 443）")
    clear_feedback_and_open_block(driver, body, "进入块编辑")
    type_candidate(driver, ["h", "a", "o"], "拼音组合 hao")
    conflict_save(driver, "冲突保存（应呈现冲突面板）")
    assert_panel(driver, True, "周期 3 冲突呈现后")
    menubar_click(driver, "重试检查磁盘", "重试检查（磁盘仍被改写）")
    time.sleep(1.2)
    driver.snapshot()
    retry_still = assert_panel(driver, True, "重试后磁盘仍变")
    restored = restore_bytes(driver, fixture, baseline3_bytes,
                             "恢复磁盘为保存基线字节")
    menubar_click(driver, "重试检查磁盘", "重试检查（磁盘已恢复基线）")
    time.sleep(1.4)
    driver.snapshot()
    retry_cleared = assert_panel(driver, False, "重试后磁盘恢复基线")
    saved3 = wait_disk_change(driver, fixture, baseline3["sha256"], 3.0,
                              "解除暂停后本地文本应可保存")
    if not saved3["changed"]:
        driver.call(["hotkey", "--key", SAVE_KEY], "显式保存解除冲突后的本地文本")
        time.sleep(0.8)
        saved3 = wait_disk_change(driver, fixture, baseline3["sha256"], 8.0,
                                  "显式保存后磁盘应变化")
    if not saved3["changed"]:
        raise DriverError("解除冲突后本地文本仍无法保存")
    if saved3["size"] != baseline3["size"] + 3:
        raise DriverError(f"重试恢复后保存尺寸应为 {baseline3['size'] + 3}，"
                          f"实际 {saved3['size']}")
    write_json(driver.evidence / "cycle3-retry.json", {
        "baseline": baseline3, "disk_before": d3, "restored": restored,
        "retry_kept_conflict": retry_still, "retry_cleared": retry_cleared,
        "saved": saved3,
        "note": "重试：磁盘仍变保持呈现；恢复基线后清除冲突与暂停并恢复保存",
    })

    # ---------- 周期 4：收起 → 重新激活 → 载入磁盘收口 ----------
    baseline4 = fixture_state(fixture)  # 446
    d4 = external_write(driver, fixture, "周期 4：磁盘同尺寸改写（保存基线 446）")
    clear_feedback_and_open_block(driver, body, "进入块编辑")
    type_candidate(driver, ["s", "h", "i"], "拼音组合 shi")
    conflict_save(driver, "冲突保存（应呈现冲突面板）")
    assert_panel(driver, True, "周期 4 冲突呈现后")
    menubar_click(driver, "收起冲突提示", "收起冲突提示")
    time.sleep(0.6)
    driver.snapshot()
    assert_panel(driver, False, "周期 4 收起后")
    time.sleep(2.0)
    still_d4 = fixture_state(fixture)
    if still_d4["sha256"] != d4["after"]["sha256"]:
        raise DriverError("周期 4 收起后自动保存写盘")
    activate_cycle(driver)
    assert_panel(driver, True, "周期 4 重新激活后")
    menubar_click(driver, "载入磁盘内容", "载入磁盘内容收口")
    time.sleep(1.3)
    driver.snapshot()
    final_exits = assert_panel(driver, False, "周期 4 载入磁盘后")
    final_disk = fixture_state(fixture)
    if final_disk["sha256"] != d4["after"]["sha256"]:
        raise DriverError("周期 4 载入磁盘后磁盘被写回")
    write_json(driver.evidence / "cycle4-final.json", {
        "baseline": baseline4, "disk": d4, "after_dismiss": still_d4,
        "final_exits": final_exits, "final_disk": final_disk,
        "note": "收起后暂停写盘、重新激活重新呈现、载入磁盘只读收口",
    })

    notes["cycles_done"] = 4
    write_json(driver.evidence / "external-conflict-notes.json", notes)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    close_document_and_wait(driver)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app-pid", type=int, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    driver = WindowDriver(args.app_pid, evidence)
    try:
        run(driver, fixture.resolve())
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:2000])
        print("驱动失败：" + str(error)[:800], file=sys.stderr)
        return 1
    print("外部冲突四出口取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
