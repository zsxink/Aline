#!/usr/bin/env python3
"""W5 3.2「菜单借焦点」与 W5 3.4 ED-01 菜单入口的真实窗口取证。

背景：微信输入法组合期，Cmd+S/Cmd+. 等和弦被输入法消费，应用收不到命令
（见 logs/validation/m2-editable-core/2026-11-19-native-run/m2-12/run-notes.md
与 _ime-chord-probe/）。菜单动作不经键盘事件路由，是真实用户仍在使用的
保存入口；本脚本验证它与组合状态的交互。

场景：
  0) 对照：干净文档（无组合）点「文件 → 保存」，确认菜单动作确实送达应用；
  1) 组合进行中「文件 → 保存」：记录组合是否仍在、应用是否进入等待保存
     （应用侧采样 save=waiting-for-composition），随后空格确认候选，等待
     结算写盘；
  2) 组合进行中「文件 → 保存」后「文件 → 取消等待保存」：记录取消后的
     wait/save 状态与磁盘不变性，Escape 取消组合后再次核对磁盘。
只经 macOS 无障碍接口投递真实输入（orca computer）与 System Events 菜单
点击；应用侧证据由 ALINE_M2_EVIDENCE_DIR 采样器在文档关闭路径落盘。
用法：drive_m2_menu_focus.py --app-pid <pid> --fixture <路径> --evidence <证据目录>
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    ROOT,
    WindowDriver,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
MENU = "文件"


def fixture_state(path):
    data = Path(path).read_bytes()
    return {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def menubar_click(driver, item, note):
    """经 System Events 点击应用菜单项；记录命令、返回码与点击后快照。"""
    script = (f'tell application "System Events" to tell process "{APP_PROCESS}" '
              f'to click menu item "{item}" of menu "{MENU}" of menu bar item '
              f'"{MENU}" of menu bar 1')
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", f"<System Events: {MENU} > {item}>"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.stdout.strip():
        write_text(driver.evidence / f"step-{driver.serial:03d}.stdout",
                   result.stdout)
    if result.stderr.strip():
        write_text(driver.evidence / f"step-{driver.serial:03d}.stderr",
                   result.stderr)
    if result.returncode != 0:
        raise DriverError("菜单点击失败（" + item + "）：" + result.stderr[-600:])


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


def clear_feedback_and_open_block(driver, body_point, note):
    """保存提示条会把正文下移；先用无副作用的 Left 结算清掉提示，再点正文。"""
    driver.call(["press-key", "--key", "Left"], "方向键结算并清除提示条",
                allow_missing_window=True)
    time.sleep(0.3)
    driver.call(["click", "--x", str(round(body_point[0], 1)),
                 "--y", str(round(body_point[1], 1))], note)
    time.sleep(0.7)
    driver.snapshot()


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    body_point = driver.heading_point()
    notes = {"body_point": [round(body_point[0], 1), round(body_point[1], 1)]}

    # 场景 0：对照——无组合时菜单保存应送达应用（提示条「已保存」）。
    notes["control_menu_save"] = driver.serial + 1
    menubar_click(driver, "保存", "对照：无组合时菜单保存")
    time.sleep(0.8)
    driver.snapshot()
    control_disk = fixture_state(fixture)
    write_json(driver.evidence / "control-disk.json",
               {"after_menu_save": control_disk,
                "unchanged_bytes": control_disk["sha256"] == before["sha256"]})

    # 场景 1：组合进行中菜单保存。对照保存的提示条会把正文下移，先结算清除
    # 提示条再按固定命中点进入块编辑。
    notes["menu_wait_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑（组合保存）")
    driver.type_keys(["n", "i"], "拼音组合 ni", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存")
    time.sleep(0.8)
    driver.snapshot()  # 组合是否仍在、是否出现等待提示，以截图为准
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(0.5)
    driver.snapshot()
    saved1 = wait_disk_change(driver, fixture, control_disk["sha256"], 8.0,
                              "组合提交后等待保存应结算写盘")
    notes["menu_wait_end"] = driver.serial
    notes["saved1"] = saved1
    if not saved1["changed"]:
        raise DriverError("组合期菜单保存等待未在任何组合结束后写盘")

    # 场景 2：组合进行中菜单保存后立即菜单取消等待。
    notes["menu_cancel_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑")
    driver.type_keys(["s", "h", "i"], "拼音组合 shi", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存（等待）")
    time.sleep(0.8)
    driver.snapshot()
    menubar_click(driver, "取消等待保存", "组合进行中经菜单取消等待")
    time.sleep(0.8)
    driver.snapshot()
    after_cancel = fixture_state(fixture)
    driver.call(["press-key", "--key", "Escape"], "Escape 取消组合")
    time.sleep(0.8)
    driver.snapshot()
    after_escape = fixture_state(fixture)
    notes["menu_cancel_end"] = driver.serial
    write_json(driver.evidence / "menu-cancel-proof.json", {
        "before_cancel": saved1,
        "after_cancel": after_cancel,
        "after_escape": after_escape,
        "cancel_kept_disk": after_cancel["sha256"] == saved1["sha256"],
        "escape_kept_disk": after_escape["sha256"] == saved1["sha256"],
        "note": "取消等待与取消组合都不得写盘；以 session.tsv 的 wait/save 列核对状态机",
    })
    write_json(driver.evidence / "menu-focus-notes.json", notes)
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
    print("菜单借焦点取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
