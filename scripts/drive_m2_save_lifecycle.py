#!/usr/bin/env python3
"""W4 5.2 真实保存生命周期取证：手工保存、组合期保存等待（完成/取消）、
自动保存、关闭与重开。

第一段（--phase save）：
  1) 打开基线夹具，点标题行进入块编辑；中文 IME 提交一个候选后立即 Cmd+S，
     磁盘 bytes 必须由基线变为提交后的源码——手工保存真实写盘。
  2) 组合进行中按 Cmd+S：应用进入「等待保存」（应用侧采样表 save 列应为
     waiting-for-composition，画面上是「组合输入进行中，保存将在输入结束后继续」）；
     空格确认候选后等待自动结算并写盘——ED-01 正向路径。
  3) 组合进行中按 Cmd+S 后再按 Cmd+. 取消等待：采样表 wait=cancelled、save 回到
     idle，取消前后磁盘 bytes 不变；Escape 取消组合后源码回到上一保存点。
  4) 不再按任何保存键，提交一个中文候选，等 1 秒去抖：磁盘只由自动保存更新。
  5) 文档已与保存点一致，Cmd+W 关闭文档让应用侧采样包落盘，再关窗口。

第二段（--phase reopen）：重新打开同一夹具；采样表的首批记录应等于磁盘 bytes
（关闭→重开载入的是已保存内容），随后关闭文档与窗口。

只经 macOS 无障碍接口投递真实输入，不触碰应用内部状态；应用侧证据由
`ALINE_M2_EVIDENCE_DIR` 显式启用的采样器在文档关闭路径落盘。
用法：drive_m2_save_lifecycle.py --phase save|reopen --app-pid <pid>
      --fixture <路径> --evidence <证据目录>

已知限制（2026-11-19 实测）：微信输入法组合期，Cmd+S / Cmd+. 等和弦被输入法
消费，应用收不到命令（见 logs/validation/m2-editable-core/2026-11-19-native-run/
m2-12/run-notes.md 与 _ime-chord-probe/）。因此本脚本段 2 的写盘实为提交后的
自动保存、段 3 的取消是无等待可取消；段 2/3 只作「组合提交/取消不破坏保存
一致性」的观察，不作为 ED-01 等待/取消通过证据。菜单路径见
drive_m2_menu_focus.py。
"""
import argparse
import hashlib
from pathlib import Path
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

SAVE_KEY = "cmd+s"
CANCEL_WAIT_KEYS = ("cmd+.", "cmd+period")


def fixture_state(path):
    """夹具当前字节数与 sha256；证据只记录摘要，不复制用户正文。"""
    data = Path(path).read_bytes()
    return {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def wait_disk_change(driver, fixture, previous_sha, timeout, note):
    """轮询夹具 bytes 直到与给定摘要不同；记录耗时与最终摘要。"""
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


def cancel_save_wait(driver):
    """按 Cmd+. 取消等待保存；键名不被接受时退回 cmd+period。"""
    last = None
    for key in CANCEL_WAIT_KEYS:
        try:
            driver.call(["hotkey", "--key", key], "取消等待保存（" + key + "）",
                        allow_missing_window=True)
            return
        except DriverError as error:
            last = error
    raise DriverError("取消等待保存的按键未被接受：" + str(last)[:400])


def clear_feedback_and_open_block(driver, body_point, note):
    """保存完成后的提示条会把正文下移；先用无副作用的 Left 输入结算清掉提示，
    再按正文标题的固定命中点打开/进入块编辑。"""
    driver.call(["press-key", "--key", "Left"], "方向键结算并清除提示条",
                allow_missing_window=True)
    time.sleep(0.3)
    driver.call(["click", "--x", str(round(body_point[0], 1)),
                 "--y", str(round(body_point[1], 1))], note)
    time.sleep(0.7)
    driver.snapshot()


def run_save_phase(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    # 在干净布局（没有提示条）里取一次标题命中点，后续一律复用该点；
    # 提示条会把正文整体下移，重新取点会先命中提示文字。
    body_point = driver.heading_point()
    notes = {"body_point": [round(body_point[0], 1), round(body_point[1], 1)]}

    # 段 1：手工保存。提交一个中文候选后立刻 Cmd+S（早于 1 秒自动保存去抖）。
    notes["manual_save_start"] = driver.serial + 1
    driver.click_body("点击标题行打开块编辑")
    driver.type_keys(["n", "i"], "拼音组合 ni", pause=0.3)
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(0.15)
    driver.call(["hotkey", "--key", SAVE_KEY], "提交后立即手工保存")
    time.sleep(0.7)
    driver.snapshot()
    saved1 = wait_disk_change(driver, fixture, before["sha256"], 8.0,
                              "手工保存后磁盘应变化")
    notes["manual_save_end"] = driver.serial
    notes["saved1"] = saved1
    if not saved1["changed"]:
        raise DriverError("手工保存后磁盘 bytes 未变化")

    # 段 2：ED-01 正向——组合进行中保存等待，组合结束后自动结算写盘。
    notes["wait_complete_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑")
    driver.type_keys(["h", "a"], "拼音组合 ha", pause=0.3)
    driver.call(["hotkey", "--key", SAVE_KEY], "组合进行中手工保存（进入等待）")
    time.sleep(0.8)
    driver.snapshot()
    driver.call(["press-key", "--key", "o"], "继续组合：o")
    time.sleep(0.3)
    driver.snapshot()
    driver.call(["press-key", "--key", "space"], "空格确认候选，等待自动结算")
    time.sleep(0.4)
    driver.snapshot()
    saved2 = wait_disk_change(driver, fixture, saved1["sha256"], 8.0,
                              "等待保存结算后磁盘应变化")
    notes["wait_complete_end"] = driver.serial
    notes["saved2"] = saved2
    if not saved2["changed"]:
        raise DriverError("组合结束后的等待保存没有写盘")

    # 段 3：ED-01 取消——组合中保存等待后按 Cmd+. 取消，磁盘不得变化。
    notes["wait_cancel_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑")
    driver.type_keys(["s", "h", "i"], "拼音组合 shi", pause=0.3)
    driver.call(["hotkey", "--key", SAVE_KEY], "组合进行中手工保存（进入等待）")
    time.sleep(0.8)
    driver.snapshot()
    cancel_save_wait(driver)
    time.sleep(0.8)
    driver.snapshot()
    after_cancel = fixture_state(fixture)
    driver.call(["press-key", "--key", "Escape"], "Escape 取消组合")
    time.sleep(0.6)
    driver.snapshot()
    after_escape = fixture_state(fixture)
    notes["wait_cancel_end"] = driver.serial
    write_json(driver.evidence / "wait-cancel-proof.json", {
        "before_cancel": saved2,
        "after_cancel": after_cancel,
        "after_escape": after_escape,
        "cancel_kept_disk": after_cancel["sha256"] == saved2["sha256"],
        "escape_kept_disk": after_escape["sha256"] == saved2["sha256"],
        "note": "取消等待与取消组合都不得写盘；磁盘摘要应保持上一步保存结果",
    })
    if after_cancel["sha256"] != saved2["sha256"]:
        raise DriverError("取消等待后磁盘反而发生变化")
    if after_escape["sha256"] != saved2["sha256"]:
        raise DriverError("取消组合后磁盘发生变化")

    # 段 4：自动保存。不按任何保存键，提交候选后等 1 秒去抖直接写盘。
    notes["autosave_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑")
    driver.type_keys(["m", "a"], "拼音组合 ma", pause=0.3)
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(0.2)
    immediate = fixture_state(fixture)
    saved3 = wait_disk_change(driver, fixture, saved2["sha256"], 8.0,
                              "自动保存去抖到期后磁盘应变化")
    time.sleep(0.6)
    driver.snapshot()
    notes["autosave_end"] = driver.serial
    write_json(driver.evidence / "autosave-proof.json", {
        "before_debounce": immediate,
        "after_debounce": saved3,
        "changed_without_save_key": saved3["changed"]
        and immediate["sha256"] == saved2["sha256"],
        "note": "期间没有任何保存按键；变化只能来自 1000ms 自动保存去抖",
    })
    if not saved3["changed"]:
        raise DriverError("自动保存去抖到期后磁盘未变化")
    if immediate["sha256"] != saved2["sha256"]:
        raise DriverError("自动保存去抖未到磁盘就提前变化，无法归因")

    write_json(driver.evidence / "save-proof.json", notes)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    close_document_and_wait(driver)


def run_reopen_phase(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    current = fixture_state(fixture)
    write_json(driver.evidence / "fixture-at-reopen.json", current)
    driver.open_document(fixture)
    driver.snapshot()
    time.sleep(0.6)
    driver.snapshot()
    # 重开后的文档内容以应用侧采样表为准：首个记录的 text_hex 应等于当前磁盘
    # bytes。这里只记录打开/关闭时序与磁盘摘要。
    write_json(driver.evidence / "reopen-proof.json", {
        "fixture": current,
        "note": "采样表首条 text_len/text_hex 应与磁盘 bytes 一致；undo_depth 应为 0",
    })
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    close_document_and_wait(driver)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", choices=("save", "reopen"), required=True)
    parser.add_argument("--app-pid", type=int, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    driver = WindowDriver(args.app_pid, evidence)
    try:
        if args.phase == "save":
            run_save_phase(driver, fixture.resolve())
        else:
            run_reopen_phase(driver, fixture.resolve())
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:2000])
        print("驱动失败：" + str(error)[:800], file=sys.stderr)
        return 1
    print("保存生命周期取证完成（" + args.phase + "），步数：" + str(driver.serial),
          flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
