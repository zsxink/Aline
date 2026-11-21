#!/usr/bin/env python3
"""W5 4.1「候选替换」真实窗口取证。

被测事实：同一真实拼音组合 `ni`，先以空格提交首候选作出对照；撤销回基线后
重新组合，经数字键在系统候选窗内改选另一候选再空格提交。两次提交都须是
单点连续插入、各形成一个撤销步；第二次提交字符与首候选不同，才证明候选
替换真实发生。候选窗由输入法自绘，orca 窗口截图不捕获系统候选窗本身，
因此以「提交结果不同 + marked 状态截图」为证据，不声称候选窗可见位置。

运行后自动保存/显式保存写盘，并核对关闭采样包与磁盘目标逐字节一致、
恢复存储无残留 Active 完整代。组合中另用 CGWindowList 只记录候选窗几何
（owner/layer/bounds），不读取窗口名称或候选正文。
现状（2026-11-20）：run-1（方向键）与 run-2（数字键）都在窗口失活时用
orca 按 pid 定向投递按键，平台把 marked 原文提交而非选到候选，失败证据
保留在 `logs/validation/m2-editable-core/2026-11-20-candidate-replace/run-1`
（0x1f 占位）与 `run-2`（原始 `ni`）。run-3 在确认前台后用 HID 注入，
数字键仍提交原始 `ni`（`6e69`）；同机 NSTextView 探针显示候选窗真实存在，
下方向键可把选中项移到下一候选、空格提交，而数字键在探针里同样提交原文。
run-4 因此改为 HID 下方向键改选、空格提交，并记录候选窗几何。
用法：drive_m2_candidate_replace.py --fixture <路径> --evidence <证据目录>
"""
import argparse
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
from drive_m2_deactivation import (  # noqa: E402  复用前台核对原语
    frontmost_unix_id,
    set_frontmost,
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

CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
KEY_N = 45
KEY_I = 34
KEY_DOWN = 125
KEY_SPACE = 49


def single_insertion_at(text, baseline, position):
    """核对 text 是 baseline 在 position 处的单点连续插入。"""
    if position < 0 or position > len(baseline):
        raise DriverError("组合起始位置越界：" + str(position))
    prefix = baseline[:position]
    suffix = baseline[position:]
    if (len(text) <= len(baseline) or not text.startswith(prefix)
            or not text.endswith(suffix)):
        raise DriverError("结果不是组合起始位置的连续插入")
    inserted = text[position:len(text) - len(suffix)]
    if not inserted:
        raise DriverError("组合提交没有产生插入字节")
    inserted.decode("utf-8")
    return inserted


def candidate_character(inserted, label):
    """要求提交字节是单个非 ASCII 可打印候选字。

    失活或候选窗不可见时，平台会把 marked 拼音原文（如 `ni`）或控制占位
    （run-1 的 0x1f）直接提交；它们同样与首候选不同，仅比较“两次不同”会把
    这类失败误判为候选替换。这里先限定提交内容确为单个候选字。
    """
    text = inserted.decode("utf-8")
    if len(text) != 1 or text.isascii() or not text.isprintable():
        raise DriverError(label + "提交不是单个候选字：" + inserted.hex())
    return text


def ensure_frontmost(driver, note):
    """确认被测实例是前台；HID 按键只到前台应用。"""
    actual = frontmost_unix_id()
    if actual == str(driver.pid):
        return {"already_frontmost": True, "frontmost_unix_id": actual,
                "note": note}
    switched = set_frontmost(driver, "aline", our_process(driver), note, 0.8)
    actual = frontmost_unix_id()
    if actual != str(driver.pid):
        raise DriverError("前台不是被测实例（期望 pid " + str(driver.pid)
                          + "，实际 " + (actual or "未知") + "）")
    switched["already_frontmost"] = False
    switched["verified_unix_id"] = actual
    return switched


def candidate_window(evidence, our_pid):
    """从窗口几何记录中取输入法候选窗；不读取窗口名称/内容。"""
    path = Path(evidence) / "candidate-windows.json"
    if not path.is_file():
        raise DriverError("缺少组合中的窗口几何记录")
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
        raise DriverError("组合中未列出输入法候选窗")
    matches.sort(key=lambda item: int(item.get("layer") or 0), reverse=True)
    top = matches[0]
    bounds = top.get("bounds") or {}
    if (not top.get("onscreen") or int(bounds.get("w", 0)) <= 0
            or int(bounds.get("h", 0)) <= 0):
        raise DriverError("候选窗几何无效：" + json.dumps(top, ensure_ascii=False))
    return top


def find_commits(rows, baseline_len):
    """找「组合行之后的提交行」及对应的最后组合行，避免把自动保存行当提交。"""
    commits = []
    for index in range(1, len(rows)):
        previous, row = rows[index - 1], rows[index]
        if (int(previous["text_len"]) == baseline_len
                and int(previous["composition_len"] or 0) > 0
                and row["composition_len"] in ("", "0")
                and int(row["text_len"]) > baseline_len
                and int(row["undo_depth"]) >= 1
                and row["redo_depth"] == "0"):
            commits.append((index, index - 1))
    return commits


def verify_replace(evidence, baseline_bytes, our_pid):
    """核对两次提交为不同单字、插入位置一致且各自历史一步。"""
    rows = session_table(evidence)
    commits = find_commits(rows, len(baseline_bytes))
    if len(commits) < 2:
        raise DriverError("采样未记录两次独立提交（候选替换未发生）")
    (control_index, control_gated), (replace_index, replace_gated) = commits[0], commits[1]
    control = rows[control_index]
    replace = rows[replace_index]

    control_position = int(rows[control_gated]["composition_start"] or -1)
    replace_position = int(rows[replace_gated]["composition_start"] or -1)
    control_text = bytes.fromhex(control["text_hex"])
    replace_text = bytes.fromhex(replace["text_hex"])
    control_added = single_insertion_at(control_text, baseline_bytes,
                                        control_position)
    replace_added = single_insertion_at(replace_text, baseline_bytes,
                                        replace_position)
    control_char = candidate_character(control_added, "首候选")
    replace_char = candidate_character(replace_added, "替换候选")
    if control_position != replace_position:
        raise DriverError("两次提交的插入位置不同（" + str(control_position)
                          + " != " + str(replace_position) + "）")

    # 两次提交之间必须出现撤销（文本回基线、redo 分支出现）与重新组合。
    undo_index = None
    for index in range(control_index + 1, replace_index):
        row = rows[index]
        if (int(row["text_len"]) == len(baseline_bytes)
                and row["undo_depth"] == "0" and row["redo_depth"] == "1"):
            undo_index = index
            break
    if undo_index is None:
        raise DriverError("两次提交之间缺少撤销回基线的状态")
    if not any(int(row["composition_len"] or 0) > 0
               for row in rows[undo_index + 1:replace_index]):
        raise DriverError("两次提交之间缺少第二轮组合状态")
    for row in rows[undo_index + 1:replace_index]:
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("撤销后到替换提交前文本未保持基线（seq "
                              + row["seq"] + "）")

    if control_char == replace_char:
        raise DriverError("改选后提交的字符与首候选相同（候选替换未生效）："
                          + control_added.hex())
    if int(replace["revision"]) <= int(control["revision"]):
        raise DriverError("替换提交的版本号未推进")

    # 关闭源字节必须等于第二次提交后的文本。
    final = (Path(evidence) / "source-after.bin").read_bytes()
    if final != replace_text:
        raise DriverError("关闭源字节与第二次提交文本不一致")

    top_window = candidate_window(evidence, our_pid)
    bounds = top_window.get("bounds") or {}
    candidate = {
        "owner": top_window.get("owner"),
        "layer": top_window.get("layer"),
        "bounds": bounds,
        "onscreen": top_window.get("onscreen"),
    }
    notes_path = Path(evidence) / "scenario-notes.json"
    if notes_path.is_file():
        notes = json.loads(notes_path.read_text(errors="replace"))
        caret = notes.get("caret_screen_estimate")
        if caret:
            candidate["caret_screen_estimate"] = caret
            candidate["caret_offset"] = [
                round(float(bounds.get("x", 0)) - float(caret[0]), 1),
                round(float(bounds.get("y", 0)) - float(caret[1]), 1),
            ]

    result = {
        "control": {
            "seq": int(control["seq"]),
            "position": control_position,
            "hex": control_added.hex(),
            "text": control_char,
        },
        "replace": {
            "seq": int(replace["seq"]),
            "position": replace_position,
            "hex": replace_added.hex(),
            "text": replace_char,
        },
        "undo_seq": int(rows[undo_index]["seq"]),
        "candidate_window": candidate,
        "note": "两次提交字符不同，候选替换真实发生；候选窗几何来自已上屏窗口列表，"
                "不含窗口名称或候选正文",
    }
    write_json(Path(evidence) / "candidate-replace-check.json", result)
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

    # 对照：空格提交首候选。
    clear_feedback_and_open_block(driver, body_point, "点击标题行打开块编辑")
    driver.type_keys(["n", "i"], "拼音组合 ni（对照首候选）", pause=0.35)
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["press-key", "--key", "space"], "空格确认首候选（对照）")
    time.sleep(1.0)
    driver.snapshot()
    control = fixture_state(fixture)
    notes["control_commit"] = control

    # 撤销回基线，留出干净的第二次组合。
    driver.call(["hotkey", "--key", "cmd+z"], "撤销首候选提交")
    time.sleep(0.9)
    driver.snapshot()
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑（候选替换）")

    # 实测：组合后按数字键改选候选。orca 按 pid 投递会在窗口失活时绕过候选窗，
    # 故先确认前台，再用系统 HID 事件流投递按键；数字键若立即提交则不再补空格。
    notes["frontmost_before_replace"] = ensure_frontmost(
        driver, "确认被测实例在前台（候选替换前）")
    notes["replace_start"] = driver.serial + 1
    driver.inject_keys([KEY_N, KEY_I], "拼音组合 ni（候选替换，HID 注入）",
                       names=["n", "i"])
    time.sleep(0.6)
    notes["frontmost_during_composition"] = frontmost_unix_id()
    if notes["frontmost_during_composition"] != str(driver.pid):
        raise DriverError("组合期间被测实例不在前台，HID 按键去向不可确认")
    snapshot = driver.snapshot()
    window = snapshot.get("window") or {}
    notes["caret_screen_estimate"] = [
        round(float(window.get("x", 0)) + body_point[0], 1),
        round(float(window.get("y", 0)) + body_point[1], 1),
    ]
    notes["candidate_windows"] = driver.capture_windows(
        "组合中的窗口几何（仅被测窗口与输入法候选窗，不含窗口名称/候选内容）")
    notes["frontmost_before_select"] = ensure_frontmost(
        driver, "确认被测实例仍在前台（候选选择前）")
    driver.inject_keys([KEY_DOWN, KEY_SPACE],
                       "下方向键改选下一候选并空格提交（HID 注入）",
                       names=["Down", "space"])
    time.sleep(1.2)
    notes["frontmost_after_select"] = frontmost_unix_id()
    driver.snapshot()

    # 保存与关闭结算。
    time.sleep(2.2)
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "替换提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "autosave_changed_disk": auto_save["sha256"] != before["sha256"],
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "替换提交后写盘；候选窗自绘不入窗口截图",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("候选替换提交后磁盘仍是基线")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    verify_replace(driver.evidence, baseline_bytes, driver.pid)
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
    print("候选替换取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
