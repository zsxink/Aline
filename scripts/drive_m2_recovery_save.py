#!/usr/bin/env python3
"""W4 5.2 / M2 4.2 真实窗口取证：异常恢复后继续输入、首次保存关联与重开。

阶段 A（run-a）：打开夹具进入块编辑，拼音组合保持未提交；等待独立 10 秒
恢复计时发布完整代（组合候选写入恢复记录），源文件字节不变；随后 SIGKILL。

阶段 B（run-b）：重启呈现逐文档恢复面板 → 恢复入内存（源文件仍不变）→
继续中文组合输入 → Cmd+S 首次保存：恢复文档没有协调器，必须走系统保存
位置选择器；选择新文件名 `recovered.md`，确认后完成资源迁移、首次写盘并
关联目标 → 不再按保存键，提交一次组合等待 1 秒自动保存，磁盘应再次更新 →
Cmd+W 关闭文档，采样包落盘；核对 source/stored/disk 三方字节与夹具不变 →
按面板丢弃记录并二次确认，退役清理。

阶段 C（run-c）：新进程重开 `recovered.md`，采样表首条文本应等于磁盘
bytes，关闭后窗口退出。

恢复记录位于 $HOME/Library/Application Support/Aline/recovery/v1，证据只记
存储相对路径。用法：
  drive_m2_recovery_save.py --phase all --fixture <路径> --evidence <证据目录>
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
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
from drive_m2_recovery_crash import (  # noqa: E402
    activate,
    fixture_state,
    launch,
    panel_buttons,
    require_button,
    run_phase_a,
    settle_logs,
    store_snapshot,
    tallest_text_point,
)
from drive_m2_save_lifecycle import wait_disk_change  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

SAVE_KEY = "cmd+s"
END_OF_LINE_KEY = "cmd+Right"
SAVE_TARGET_NAME = "recovered.md"
COMPOSITION_KEYS = ["n", "i"]
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")


def wait_until_buttons(driver, labels, timeout):
    deadline = time.monotonic() + timeout
    snapshot = driver.snapshot()
    buttons = panel_buttons(driver, snapshot)
    while (any(buttons[label] is None for label in labels)
           and time.monotonic() < deadline):
        time.sleep(0.5)
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
    return snapshot, buttons


def wait_sampler_files(evidence, timeout):
    deadline = time.monotonic() + timeout
    present = {}
    while True:
        present = {name: (Path(evidence) / name).is_file() for name in CLOSE_FILES}
        if all(present.values()) or time.monotonic() >= deadline:
            return present
        time.sleep(0.4)


def click_body_point(driver, note):
    point = tallest_text_point(driver)
    driver.call(["click", "--x", str(round(point[0], 1)),
                 "--y", str(round(point[1], 1))], note)
    return point


def save_panel_view(driver):
    """读取当前无障碍树中的保存面板字段与按钮；不假设控件索引稳定。"""
    snapshot = driver.snapshot()
    tree = driver.tree_text(snapshot)
    fields = [(int(index), value) for index, value in
              re.findall(r'^\s*(\d+) text field \(settable\)(?:, Value: ([^\n]*))?$',
                         tree, re.M)]
    buttons = {}
    for label in ("存储", "保存", "Save", "替换", "Replace", "取消", "Cancel"):
        match = re.search(r'^\s*(\d+) button (?:default )?' + re.escape(label)
                          + r'(?:\s+\(.*\))?\s*$', tree, re.M)
        if match:
            buttons[label] = int(match.group(1))
    return snapshot, tree, fields, buttons


def wait_save_panel(driver, timeout=10.0):
    deadline = time.monotonic() + timeout
    last = None
    while True:
        last = save_panel_view(driver)
        _snapshot, tree, fields, buttons = last
        if fields and any(label in buttons for label in ("存储", "保存", "Save")):
            return last
        if time.monotonic() >= deadline:
            write_json(driver.evidence / "save-panel-timeout.json", {
                "tree_tail": tree[-4000:],
                "fields": fields,
                "buttons": buttons,
                "note": "Cmd+S 后未在期限内识别出系统保存位置选择器",
            })
            raise DriverError("没有出现系统保存位置选择器；字段=" + str(fields)
                              + " 按钮=" + str(buttons))
        time.sleep(0.4)


def choose_save_target(driver, name):
    snapshot, tree, fields, buttons = wait_save_panel(driver)
    field_index, current = fields[0]
    write_json(driver.evidence / "save-panel.json", {
        "fields": fields,
        "buttons": buttons,
        "window": snapshot.get("window"),
        "suggested_value": current,
        "note": "恢复文档无协调器，Cmd+S 必须进入系统保存位置选择器",
    })
    if current == name:
        written = current
        write_json(driver.evidence / "save-name.json", {
            "field": field_index, "before": current, "after": written,
            "changed_by": "already-suggested",
        })
    else:
        written = None
        try:
            driver.call(["set-value", "--element-index", str(field_index),
                         "--value", name], "在保存面板写入新文件名 " + name)
            deadline = time.monotonic() + 4.0
            while time.monotonic() < deadline:
                _snapshot, _tree, current_fields, _buttons = save_panel_view(driver)
                if current_fields and current_fields[0][1] == name:
                    written = current_fields[0][1]
                    break
                time.sleep(0.3)
        except DriverError as error:
            write_json(driver.evidence / "save-name-set-value-failed.json",
                       {"error": str(error)[:800], "field": field_index,
                        "wanted": name})
        if written != name:
            # set-value 未生效（或面板拒绝直接赋值）时，走聚焦、全选、替换键入。
            driver.call(["click", "--element-index", str(field_index)],
                        "点击保存面板文件名输入框")
            driver.call(["hotkey", "--key", "cmd+a"], "全选当前文件名")
            driver.call(["type-text", "--text", name],
                        "键入新文件名（回退路径）")
            time.sleep(0.5)
            _snapshot, _tree, current_fields, _buttons = save_panel_view(driver)
            written = current_fields[0][1] if current_fields else None
        write_json(driver.evidence / "save-name.json", {
            "field": field_index, "before": current, "after": written,
            "changed_by": "set-value-then-type-text",
        })
        if written != name:
            # 面板可能隐藏扩展名：值等于去扩展名的主干也可以接受。
            if written != Path(name).stem:
                raise DriverError("保存面板文件名未写入期望值：" + str(written))

    _snapshot, _tree, _fields, buttons = save_panel_view(driver)
    save_button = next((buttons[label] for label in ("存储", "保存", "Save")
                        if label in buttons), None)
    if save_button is None:
        raise DriverError("保存面板没有可用的保存按钮：" + str(buttons))
    driver.call(["click", "--element-index", str(save_button)], "确认保存目标")
    return field_index, written


def verify_saved_bytes(evidence, path, steps):
    """保存完成后的独立字节核对：目标文件必须存在且非空。"""
    deadline = time.monotonic() + 10.0
    state = None
    while time.monotonic() < deadline:
        if path.is_file():
            data = path.read_bytes()
            if data:
                state = {"size": len(data),
                         "sha256": hashlib.sha256(data).hexdigest()}
                break
        time.sleep(0.2)
    write_json(evidence / "saved-target.json", {
        "target": path.name,
        "state": state,
        "steps": steps,
        "note": "首次保存关联后目标文件应由应用创建并写入恢复文本",
    })
    if state is None:
        raise DriverError("首次保存后目标文件不存在或为空：" + path.name)
    return state


def run_phase_b(fixture, evidence, expect_source_sha256):
    fixture_at_start = fixture_state(fixture)
    target = fixture.parent / SAVE_TARGET_NAME
    if target.exists():
        write_json(evidence / "target-preexisting.json", {
            "target": target.name,
            "size": target.stat().st_size,
            "note": "目标已存在，拒绝覆盖旧文件；请换夹具目录或先移走",
        })
        raise DriverError("首次保存目标已存在，拒绝覆盖：" + target.name)
    write_json(evidence / "fixture-before.json", fixture_at_start)
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate(driver)
        snapshot, buttons = wait_until_buttons(driver, ("恢复", "丢弃"), 10.0)
        startup_store = store_snapshot()
        write_json(evidence / "startup-panel.json", {
            "buttons": buttons,
            "store": startup_store,
            "title": (snapshot.get("window") or {}).get("title"),
            "expected_source_sha256": expect_source_sha256,
            "note": "重启后的启动扫描；恢复面板应逐文档呈现恢复/丢弃",
        })
        if buttons["恢复"] is None or buttons["丢弃"] is None:
            raise DriverError("启动扫描没有呈现恢复面板：" + str(buttons))
        complete = [generation for generation in startup_store["generations"]
                    if generation["manifest"] and generation["source_bytes"]]
        if len(complete) != 1 or complete[0]["source_sha256"] != expect_source_sha256:
            raise DriverError("启动扫描完整代与阶段 A 不符："
                              + str(complete))

        restore = require_button(driver, snapshot, "恢复", "启动面板")
        driver.call(["click", "--element-index", str(restore)],
                    "恢复记录到内存文档")
        deadline = time.monotonic() + 8.0
        while time.monotonic() < deadline:
            title = (driver.snapshot().get("window") or {}).get("title") or ""
            if fixture.name in title:
                break
            time.sleep(0.4)
        snapshot = driver.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        fixture_after_restore = fixture_state(fixture)
        write_json(evidence / "restored.json", {
            "title": title,
            "buttons": panel_buttons(driver, snapshot),
            "fixture": fixture_after_restore,
            "store": store_snapshot(),
            "note": "恢复只入内存；源文件保持不写，记录不被消费",
        })
        if fixture.name not in title:
            raise DriverError("恢复后窗口标题不含夹具名：" + title)
        if fixture_after_restore["sha256"] != fixture_at_start["sha256"]:
            raise DriverError("恢复内存文档时源文件被改写")

        # 继续输入：提示条会把正文下移，按最高文本带取点；行尾再组合输入。
        point = click_body_point(driver, "点击正文进入块编辑（恢复后继续输入）")
        time.sleep(0.8)
        driver.call(["hotkey", "--key", END_OF_LINE_KEY], "光标移到当前行尾")
        time.sleep(0.3)
        driver.type_keys(COMPOSITION_KEYS, "恢复后拼音组合 ni", pause=0.3)
        driver.call(["press-key", "--key", "space"], "空格确认候选，提交继续输入")
        time.sleep(0.5)
        driver.snapshot()
        write_text(evidence / "input-source-after-restore.txt",
                   driver.input_source())
        write_json(evidence / "continued-input.json", {
            "point": [round(point[0], 1), round(point[1], 1)],
            "keys": COMPOSITION_KEYS + ["space"],
            "fixture": fixture_state(fixture),
            "note": "恢复后继续中文组合输入；此时文档仍无协调器、未写盘",
        })
        if fixture_state(fixture)["sha256"] != fixture_at_start["sha256"]:
            raise DriverError("恢复后继续输入改写了源文件")

        # 首次保存：Cmd+S 打开系统保存位置选择器，选择新目标 recovered.md。
        driver.call(["hotkey", "--key", SAVE_KEY],
                    "首次保存（恢复文档无协调器，应打开保存面板）")
        time.sleep(0.6)
        choose_save_target(driver, SAVE_TARGET_NAME)
        saved = verify_saved_bytes(evidence, target, driver.serial)
        time.sleep(0.5)
        post_save = fixture_state(target)
        store_after_save = store_snapshot()
        write_json(evidence / "post-save.json", {
            "target": target.name,
            "state": post_save,
            "window_title": (driver.snapshot().get("window") or {}).get("title"),
            "store": store_after_save,
            "note": "首次保存关联完成；恢复记录仍保留至关闭/显式丢弃",
        })
        if post_save["sha256"] != saved["sha256"]:
            raise DriverError("保存完成后目标文件仍在变化")

        # 关联后的自动保存：再提交一次组合，不按任何保存键，等待 1 秒去抖。
        click_body_point(driver, "关联后点击正文继续输入")
        time.sleep(0.8)
        driver.call(["hotkey", "--key", END_OF_LINE_KEY], "光标移到当前行尾")
        driver.type_keys(["h", "a", "o"], "自动保存用拼音组合 hao", pause=0.3)
        driver.call(["press-key", "--key", "space"], "空格确认候选")
        time.sleep(0.2)
        before_autosave = fixture_state(target)
        after_autosave = wait_disk_change(driver, target, saved["sha256"], 8.0,
                                          "关联后自动保存应更新目标文件")
        write_json(evidence / "autosave-after-association.json", {
            "before_debounce": before_autosave,
            "after_debounce": after_autosave,
            "changed_without_save_key": after_autosave["changed"]
            and before_autosave["sha256"] == saved["sha256"],
            "note": "首次关联成功后自动保存应重新布防；期间不按保存键",
        })
        if not after_autosave["changed"]:
            raise DriverError("首次关联后自动保存没有更新目标文件")
        if before_autosave["sha256"] != saved["sha256"]:
            raise DriverError("自动保存去抖前目标文件已提前变化，无法归因")

        # 关闭文档：先按一次 cmd+w；未关闭时点正文再试，并记录每次尝试。
        def close_after_focus():
            point = click_body_point(driver, "点击正文建立文档焦点")
            time.sleep(0.6)
            driver.call(["hotkey", "--key", "cmd+w"], "再次关闭文档",
                        allow_missing_window=True)
            return point

        attempts = []
        for label, action in (
            ("关联后 cmd+w", lambda: driver.call(
                ["hotkey", "--key", "cmd+w"], "关闭已关联文档：cmd+w",
                allow_missing_window=True)),
            ("正文聚焦后 cmd+w", close_after_focus),
        ):
            action()
            time.sleep(1.5)
            title = (driver.snapshot().get("window") or {}).get("title") or ""
            attempts.append({"label": label, "title": title,
                             "closed": SAVE_TARGET_NAME not in title
                             and fixture.name not in title})
            if attempts[-1]["closed"]:
                break
        write_json(evidence / "close-attempts.json", {
            "attempts": attempts,
            "closed": any(attempt["closed"] for attempt in attempts),
        })
        if not any(attempt["closed"] for attempt in attempts):
            raise DriverError("关联文档无法关闭：" + str(attempts))
        present = wait_sampler_files(evidence, 12.0)
        if not all(present.values()):
            raise DriverError("关闭文档后采样证据缺失：" + str(present))

        final = (Path(evidence) / "final.tsv").read_text(encoding="utf-8")
        rows = session_table(evidence)
        source_after = (Path(evidence) / "source-after.bin").read_bytes()
        stored_after = (Path(evidence) / "stored-after.bin").read_bytes()
        disk_after = (Path(evidence) / "disk-after.bin").read_bytes()
        disk_now = target.read_bytes()
        fixture_final = fixture_state(fixture)
        write_json(evidence / "close-bytes.json", {
            "final_tsv": final,
            "session_rows": len(rows),
            "source_size": len(source_after),
            "stored_size": len(stored_after),
            "disk_after_size": len(disk_after),
            "target_now_size": len(disk_now),
            "source_sha256": hashlib.sha256(source_after).hexdigest(),
            "disk_sha256": hashlib.sha256(disk_after).hexdigest(),
            "target_now_sha256": hashlib.sha256(disk_now).hexdigest(),
            "fixture_unchanged": fixture_final["sha256"] == fixture_at_start["sha256"],
            "note": "source/stored/disk 三方字节与磁盘当前文件必须一致",
        })
        if source_after != stored_after or source_after != disk_after \
                or source_after != disk_now:
            raise DriverError("关闭证据包 source/stored/disk 不一致")
        if "disk_equals_source=true" not in final:
            raise DriverError("final.tsv 未记录 disk_equals_source=true：" + final)
        if fixture_final["sha256"] != fixture_at_start["sha256"]:
            raise DriverError("异常恢复→继续输入→首次保存整链改写了源夹具")

        # 记录保留至显式丢弃：关闭恢复文档后条目应只提供丢弃。
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        write_json(evidence / "restored-entry.json", {
            "buttons": buttons,
            "store": store_snapshot(),
            "note": "已保存并关闭的恢复条目仍保留，需显式丢弃才退役",
        })
        if buttons["丢弃"] is None:
            raise DriverError("关闭已关联文档后没有出现丢弃按钮")
        discard = require_button(driver, snapshot, "丢弃", "已保存恢复条目")
        driver.call(["click", "--element-index", str(discard)],
                    "丢弃已保存恢复记录（进入二次确认）")
        time.sleep(0.5)
        snapshot = driver.snapshot()
        confirm = require_button(driver, snapshot, "确认丢弃", "丢弃二次确认")
        driver.call(["click", "--element-index", str(confirm)], "确认丢弃")
        deadline = time.monotonic() + 8.0
        cleared = False
        while time.monotonic() < deadline:
            if not store_snapshot()["generations"]:
                cleared = True
                break
            time.sleep(0.4)
        write_json(evidence / "discard-confirmed.json", {
            "generations_cleared": cleared,
            "store": store_snapshot(),
            "note": "显式丢弃后恢复代退役清理；源文件与已保存目标不受影响",
        })
        if not cleared:
            raise DriverError("确认丢弃后恢复代未清理")
        if target.read_bytes() != disk_now:
            raise DriverError("丢弃恢复记录改写了已保存目标")

        driver.close_window()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            write_json(evidence / "window-close-timeout.json",
                       {"pid": proc.pid, "note": "关闭窗口后进程未退出，记录后终止"})
            proc.terminate()
            proc.wait(timeout=10)
        write_json(evidence / "process-exit.json", {
            "pid": proc.pid, "exit_code": proc.returncode,
            "note": "保存关联、自动保存与丢弃完成后的正常退出状态",
        })
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        settle_logs(evidence, stdout, stderr, log_dir)
    return target


def run_phase_c(target, evidence):
    expected = target.read_bytes()
    write_json(evidence / "target-at-reopen.json", {
        "size": len(expected),
        "sha256": hashlib.sha256(expected).hexdigest(),
        "note": "重开前目标文件的独立字节摘要",
    })
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate(driver)
        driver.open_document(target)
        driver.snapshot()
        time.sleep(0.6)
        driver.snapshot()
        driver.call(["hotkey", "--key", "cmd+w"], "关闭重开的文档",
                    allow_missing_window=True)
        present = wait_sampler_files(evidence, 12.0)
        if not all(present.values()):
            raise DriverError("重开文档关闭后采样证据缺失：" + str(present))
        final = (Path(evidence) / "final.tsv").read_text(encoding="utf-8")
        rows = session_table(evidence)
        first = rows[0]
        source_after = (Path(evidence) / "source-after.bin").read_bytes()
        disk_after = (Path(evidence) / "disk-after.bin").read_bytes()
        write_json(evidence / "reopen-proof.json", {
            "final_tsv": final,
            "session_rows": len(rows),
            "first_text_len": first["text_len"],
            "first_undo_depth": first["undo_depth"],
            "source_sha256": hashlib.sha256(source_after).hexdigest(),
            "disk_sha256": hashlib.sha256(disk_after).hexdigest(),
            "expected_sha256": hashlib.sha256(expected).hexdigest(),
            "note": "重开后采样首条文本、关闭字节包与磁盘三方应一致",
        })
        if source_after != expected or disk_after != expected:
            raise DriverError("重开后的字节与应用/磁盘期望不一致")
        if first["text_len"] != str(len(expected)):
            raise DriverError("重开采样首条 text_len 与磁盘不一致")
        if first["undo_depth"] != "0":
            raise DriverError("重开文档不应带历史：" + first["undo_depth"])
        driver.close_window()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.terminate()
            proc.wait(timeout=10)
        write_json(evidence / "process-exit.json", {
            "pid": proc.pid, "exit_code": proc.returncode,
            "note": "重开核验后的正常退出状态",
        })
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        settle_logs(evidence, stdout, stderr, log_dir)


def expect_sha_from_phase_a(evidence):
    published = Path(evidence) / "recovery-published.json"
    if not published.is_file():
        raise DriverError("缺少阶段 A 的 recovery-published.json，无法绑定谱系")
    value = json.loads(published.read_text(encoding="utf-8"))
    return value["generation"]["source_sha256"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", choices=("a", "b", "c", "all"), default="all")
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--expect-source-sha256", default=None)
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
    if not fixture.is_file():
        print("夹具不存在：" + str(fixture), file=sys.stderr)
        return 2
    try:
        run_a = evidence / "run-a"
        run_b = evidence / "run-b"
        run_c = evidence / "run-c"
        if args.phase in ("a", "all"):
            run_a.mkdir(parents=True, exist_ok=True)
            run_phase_a(fixture.resolve(), run_a)
        if args.phase in ("b", "all"):
            run_b.mkdir(parents=True, exist_ok=True)
            expected = args.expect_source_sha256 or expect_sha_from_phase_a(run_a)
            target = run_phase_b(fixture.resolve(), run_b, expected)
            if args.phase == "all":
                run_c.mkdir(parents=True, exist_ok=True)
                run_phase_c(target, run_c)
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:3000])
        print("驱动失败：" + str(error)[:1200], file=sys.stderr)
        return 1
    print("恢复后继续输入→首次保存关联→自动保存→重开取证完成", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
