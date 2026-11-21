#!/usr/bin/env python3
"""W4 文件被外部删除（AC-06 Missing）的真实窗口取证。

被测事实：脏文档的磁盘目标被外部删除后，保存失败转为静默外改检查；窗口呈现
「文件已被删除」冲突面板，只保留「重试检查磁盘」与「收起冲突提示」，不提供
「载入磁盘内容」或「确认覆盖磁盘」。自动保存暂停期间不得重建或写回目标文件；
收起面板不解除冲突与暂停，重新激活窗口会重新呈现；仍缺失时重试保持呈现；
写回保存基线字节后重试清除冲突与暂停，本地文本随后自动/显式写盘。关闭后
应用侧采样包与磁盘目标逐字节相同。

删除由脚本直接执行，删除前把字节副本写入证据目录，供恢复基线使用；输入与
按钮动作只经 macOS 无障碍接口投递。应用由脚本以 `ALINE_M2_EVIDENCE_DIR`
启动，关闭路径由应用侧采样器落盘。
用法：drive_m2_external_missing.py --fixture <路径> --evidence <证据目录>
"""
import argparse
import csv
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    ROOT,
    WindowDriver,
)
from drive_m2_external_conflict import (  # noqa: E402  复用冲突取证原语
    activate_cycle,
    check_single_instance,
    fixture_state,
    menu_states,
    wait_disk_change,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m2_save_lifecycle import clear_feedback_and_open_block  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_BINARY = ROOT / "target" / "debug" / "aline"
STORE_ROOT = (Path.home() / "Library" / "Application Support" / "Aline"
              / "recovery" / "v1")
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
SAVE_KEY = "cmd+s"
MISSING_TITLE = "磁盘上的文件已被删除"
MISSING_BUTTONS = {
    "载入磁盘内容": False,
    "确认覆盖磁盘": False,
    "重试检查磁盘": True,
    "收起冲突提示": True,
}
NO_BUTTONS = {label: False for label in MISSING_BUTTONS}
# 激活前台后 Orca 偶尔短暂报 app_not_found（run-2 失败轮记录）；仅对这两类
# 瞬时错误重试，其余错误照常失败并保留证据。
TRANSIENT_ERRORS = ("app_not_found", "runtime_unavailable")


def transient(error):
    return any(code in str(error) for code in TRANSIENT_ERRORS)


def snapshot_retry(driver, timeout=6.0, note="读取窗口状态"):
    """瞬时不可见时轮询窗口状态；真实退出仍按原错误失败。"""
    started = time.monotonic()
    attempts = 0
    while True:
        attempts += 1
        try:
            return driver.snapshot()
        except DriverError as error:
            if not transient(error) or time.monotonic() - started >= timeout:
                raise
            write_text(driver.evidence
                       / f"snapshot-retry-{driver.serial:03d}-{attempts}.txt",
                       note + "：" + str(error)[:800] + "\n")
            time.sleep(0.5)


def activate_with_retry(driver):
    """切换前台并留出 AX 重新登记时间；瞬时 app_not_found 时重试一次。"""
    for attempt in range(3):
        try:
            activate_cycle(driver)
            time.sleep(0.8)
            return
        except DriverError as error:
            if not transient(error) or attempt == 2:
                raise
            time.sleep(0.8)


def panel_click(driver, label, note):
    """点击冲突面板自身按钮；元素索引按当次快照重找。"""
    snapshot = snapshot_retry(driver, note="读取冲突面板按钮")
    index = driver.find_button(snapshot, label)
    if index is None:
        raise DriverError("冲突面板缺少按钮：" + label)
    for attempt in range(3):
        try:
            driver.call(["click", "--element-index", str(index)], note)
            break
        except DriverError as error:
            if not transient(error) or attempt == 2:
                raise
            time.sleep(0.8)
            snapshot = snapshot_retry(driver, note="重找冲突面板按钮")
            index = driver.find_button(snapshot, label)
            if index is None:
                raise DriverError("重试后冲突面板缺少按钮：" + label)
    time.sleep(0.6)


def conflict_save(driver, note):
    """缺失目标下显式保存；失败后静默检查呈现面板，读快照容忍瞬时不可见。"""
    for attempt in range(2):
        try:
            driver.call(["hotkey", "--key", SAVE_KEY], note)
            break
        except DriverError as error:
            if not transient(error) or attempt == 1:
                raise
            time.sleep(0.8)
    time.sleep(1.2)
    return snapshot_retry(driver, note="冲突保存后读取窗口")


def launch(evidence):
    """脚本直接启动真实 aline 进程；环境只设 ALINE_M2_EVIDENCE_DIR。"""
    log_dir = Path(tempfile.mkdtemp(prefix="aline-missing-run-"))
    stdout = open(log_dir / "app.stdout", "wb")
    stderr = open(log_dir / "app.stderr", "wb")
    env = os.environ.copy()
    env["ALINE_M2_EVIDENCE_DIR"] = str(evidence)
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(ROOT), env=env,
                            stdout=stdout, stderr=stderr, start_new_session=True)
    write_text(evidence / "app.pid", str(proc.pid) + "\n")
    write_json(evidence / "launch.json", {
        "pid": proc.pid,
        "started_epoch_ns": time.time_ns(),
        "binary": APP_BINARY.name,
        "cwd": str(ROOT),
        "note": "脚本直接启动真实 aline 进程，环境只设 ALINE_M2_EVIDENCE_DIR",
    })
    return proc, stdout, stderr, log_dir


def settle_logs(evidence, stdout, stderr, log_dir):
    for stream in (stdout, stderr):
        stream.flush()
        stream.close()
    for name in ("app.stdout", "app.stderr"):
        source = log_dir / name
        if source.is_file() and source.stat().st_size:
            write_text(evidence / name, source.read_text(errors="replace"))
        else:
            write_text(evidence / name, "")
    shutil.rmtree(log_dir, ignore_errors=True)


def store_has_generations():
    """启动扫描会呈现任何 Active 完整代；有残留时拒绝混用场景。"""
    active = STORE_ROOT / "active"
    if not active.is_dir():
        return []
    return sorted(path.relative_to(STORE_ROOT).as_posix()
                  for path in active.glob("*/g/*/manifest"))


def delete_target(driver, fixture, note):
    """删除夹具并把原字节复制进证据目录，供恢复基线使用。"""
    path = Path(fixture)
    before = fixture_state(path)
    data = path.read_bytes()
    backup = driver.evidence / "deleted-bytes.bin"
    backup.write_bytes(data)
    path.unlink()
    if path.exists():
        raise DriverError("删除夹具失败：" + note)
    result = {"note": note, "before": before, "deleted": True,
              "bytes_file": backup.name, "size": len(data), "step": driver.serial}
    write_json(driver.evidence / f"missing-delete-{driver.serial:03d}.json", result)
    return {"state": before, "bytes": data}


def missing_panel_state(driver):
    """读取缺失面板的按钮呈现；面板标题由 GPUI 自绘，AX 树不暴露。"""
    snapshot = snapshot_retry(driver, note="读取缺失面板")
    present = {label: driver.find_button(snapshot, label) is not None
               for label in MISSING_BUTTONS}
    text = driver.tree_text(snapshot)
    titles = [line.strip() for line in text.splitlines() if MISSING_TITLE in line]
    return snapshot, present, titles


def assert_missing_panel(driver, expected, note):
    """按预期按钮集合判定面板呈现，并把观察写入证据。

    标题/预览是 GPUI 自绘文本，无障碍树只暴露按钮；缺失状态由「仅重试/收起」
    这一按钮集合与关闭后的 session.tsv `external=missing` 共同证明（run-1
    首次断言标题导致误判，失败轮保留）。
    """
    snapshot, present, titles = missing_panel_state(driver)
    states = menu_states(driver, note + "（菜单 AX 可用性，仅记录）")
    shown = expected["重试检查磁盘"]
    result = {"note": note, "panel_present": shown, "buttons": present,
              "expected": expected, "title_in_ax": bool(titles),
              "title_exposed": False, "menu_ax": states, "step": driver.serial}
    write_json(driver.evidence / f"missing-panel-{driver.serial:03d}.json", result)
    if present != expected:
        raise DriverError(f"{note}：缺失面板按钮呈现应为 {expected}，实际 {present}")
    return result


def wait_missing_panel(driver, expected, timeout, note):
    """轮询后台静默检查结算后的缺失面板按钮呈现。"""
    started = time.monotonic()
    polls = 0
    while True:
        _snapshot, present, titles = missing_panel_state(driver)
        polls += 1
        elapsed = round(time.monotonic() - started, 3)
        ready = present == expected
        if ready or elapsed >= timeout:
            break
        time.sleep(0.4)
    write_json(driver.evidence / f"missing-wait-{driver.serial:03d}.json", {
        "note": note, "expected": expected, "present": present,
        "title_in_ax": bool(titles), "title_exposed": False, "ready": ready,
        "polls": polls, "waited_seconds": elapsed, "step": driver.serial,
    })
    if not ready:
        raise DriverError(note + "：期限内缺失面板呈现不符合预期")
    return {"present": present, "title_in_ax": bool(titles),
            "waited_seconds": elapsed}


def restore_bytes(driver, fixture, data, note):
    path = Path(fixture)
    path.write_bytes(bytes(data))
    restored = fixture_state(path)
    write_json(driver.evidence / f"restore-{driver.serial:03d}.json", {
        "note": note, "restored": restored, "step": driver.serial,
    })
    return restored


def require_absent(driver, fixture, note):
    """暂停期写盘核对：目标必须仍不存在，记录等待时长与观察。"""
    state = Path(fixture).exists()
    write_json(driver.evidence / f"missing-absent-{driver.serial:03d}.json", {
        "note": note, "exists": state, "step": driver.serial,
    })
    if state:
        raise DriverError(note + "：目标文件被重建或写回")


def read_final_tsv(path):
    """final.tsv 是多行 `key=value` 字段（非表头表格），逐行拆 token。"""
    values = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        for part in line.split("\t"):
            if "=" in part:
                key, value = part.split("=", 1)
                values[key.strip()] = value.strip()
    if not values:
        raise DriverError("final.tsv 没有字段")
    return values


def single_insertion_position(baseline, actual, marker=b"Z"):
    """实际字节是否为基线在单个位置插入 marker；返回位置或 None。"""
    if len(actual) != len(baseline) + len(marker):
        return None
    for position in range(len(baseline) + 1):
        if actual == baseline[:position] + marker + baseline[position:]:
            return position
    return None


def truthy(value):
    return str(value).strip().lower() in ("1", "true", "yes")


def verify_close_packet(driver, fixture, baseline_bytes):
    """关闭后逐字节核对采样包与磁盘目标，并读取 final.tsv。"""
    disk = Path(fixture).read_bytes()
    payloads = {}
    for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
        data = (driver.evidence / name).read_bytes()
        payloads[name] = {"size": len(data),
                          "sha256": hashlib.sha256(data).hexdigest(),
                          "equals_disk": data == disk}
    final = read_final_tsv(driver.evidence / "final.tsv")
    insertion = single_insertion_position(baseline_bytes, disk)
    result = {
        "disk": {"size": len(disk), "sha256": hashlib.sha256(disk).hexdigest()},
        "payloads": payloads,
        "final": final,
        "disk_equals_source": truthy(final.get("disk_equals_source")),
        "disk_equals_stored": truthy(final.get("disk_equals_stored")),
        "marker": "Z",
        "marker_insert_at": insertion,
        "note": "关闭路径采样包与磁盘目标逐字节对照；磁盘应为基线单点插入 Z",
    }
    write_json(driver.evidence / "final-check.json", result)
    for name, payload in payloads.items():
        if not payload["equals_disk"]:
            raise DriverError(name + " 与磁盘目标不一致")
    lengths = {final.get("source_len"), final.get("stored_len"),
               final.get("disk_len"), str(len(disk))}
    if len(lengths) != 1:
        raise DriverError("final.tsv 的 source/stored/disk 长度不一致：" + str(lengths))
    if not truthy(final.get("disk_equals_source")):
        raise DriverError("final.tsv disk_equals_source 不为真")
    if insertion is None:
        raise DriverError("关闭后磁盘不是基线单点插入 Z 的字节")
    return result


def verify_session_rows(driver, baseline_size, expected_size):
    """从关闭路径 session.tsv 证明应用侧经历 dirty→missing→dismiss→cleared。"""
    path = driver.evidence / "session.tsv"
    with open(path, newline="", encoding="utf-8") as handle:
        rows = [row for row in csv.DictReader(handle, delimiter="\t")
                if (row.get("seq") or "").isdigit()]

    def seqs(predicate):
        return [int(row["seq"]) for row in rows if predicate(row)]

    dirty_marker = seqs(
        lambda row: row["draft_len"] == "1" and row["draft_hex"] == "5a"
        and row["text_len"] == str(baseline_size) and truthy(row["dirty"]))
    missing = seqs(
        lambda row: row["external"] == "missing"
        and row["save"] == "conflict"
        and truthy(row["autosave_paused"]))
    dismissed = seqs(
        lambda row: row["external"] == "none"
        and row["save"] == "conflict"
        and truthy(row["autosave_paused"]))
    cleared = seqs(
        lambda row: row["external"] == "none"
        and row["save"] == "idle"
        and not truthy(row["dirty"])
        and not truthy(row["autosave_paused"])
        and row["draft_len"] == "0"
        and row["text_len"] == str(expected_size))
    result = {
        "rows": len(rows),
        "dirty_marker_rows": dirty_marker,
        "missing_rows": missing,
        "dismissed_rows": dismissed,
        "cleared_rows": cleared,
        "note": "应用侧采样：脏标记→external=missing/save=conflict/暂停→"
                "收起后 external=none 且冲突暂停保持→恢复后 idle/干净/可保存",
    }
    write_json(driver.evidence / "session-check.json", result)
    if not rows:
        raise DriverError("session.tsv 没有采样行")
    for name, found in (("脏标记", dirty_marker), ("缺失冲突", missing),
                        ("收起后冲突保持", dismissed),
                        ("恢复后可保存", cleared)):
        if not found:
            raise DriverError("session.tsv 缺少" + name + "采样")
    if not (min(missing) > min(dirty_marker)
            and min(dismissed) > min(missing)
            and max(cleared) > max(missing)):
        raise DriverError("session.tsv 采样顺序不符合删除冲突链")
    return result


def run(driver, fixture):
    check_single_instance(driver)
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    baseline = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", baseline)
    driver.open_document(fixture)
    driver.snapshot()
    body = driver.heading_point()
    body_point = [round(body[0], 1), round(body[1], 1)]
    notes = {"body_point": body_point, "fixture_before": baseline}

    clear_feedback_and_open_block(driver, body, "进入块编辑")
    # ASCII 标记使文本确定性变化（单字节 Z），不改动其它输入法状态。
    driver.call(["type-text", "--text", "Z"], "插入 ASCII 标记 Z 使文档变脏")
    time.sleep(0.3)
    # 立即删除目标，早于 1 秒自动保存去抖。
    deleted = delete_target(driver, fixture, "删除磁盘目标（保留字节副本）")
    conflict_save(driver, "缺失目标下 Cmd+S（保存失败后静默检查）")
    presented = wait_missing_panel(driver, MISSING_BUTTONS, 6.0,
                                  "等待删除冲突面板呈现")

    # 暂停期：自动保存不得重建目标文件。
    time.sleep(2.0)
    require_absent(driver, fixture, "冲突面板呈现后 2 秒")
    still_missing = {"absent_after_2s": not Path(fixture).exists()}

    # 收起 → 暂停保持 → 重新激活重新呈现。
    panel_click(driver, "收起冲突提示", "收起冲突提示")
    time.sleep(0.6)
    dismissed = assert_missing_panel(driver, NO_BUTTONS, "收起冲突提示后")
    time.sleep(2.0)
    require_absent(driver, fixture, "收起冲突提示后 2 秒")
    activate_with_retry(driver)
    represented = wait_missing_panel(driver, MISSING_BUTTONS, 6.0,
                                    "重新激活后等待删除冲突面板重现")

    # 仍缺失时重试保持呈现；恢复基线后重试清除冲突与暂停。
    panel_click(driver, "重试检查磁盘", "重试检查磁盘（目标仍缺失）")
    time.sleep(1.2)
    retry_still = assert_missing_panel(driver, MISSING_BUTTONS, "重试后目标仍缺失")
    restored = restore_bytes(driver, fixture, deleted["bytes"], "写回保存基线字节")
    panel_click(driver, "重试检查磁盘", "重试检查磁盘（目标恢复基线）")
    time.sleep(1.4)
    cleared = assert_missing_panel(driver, NO_BUTTONS, "目标恢复基线后重试")
    saved = wait_disk_change(driver, fixture, restored["sha256"], 8.0,
                             "冲突解除后本地文本应自动保存")
    if not saved["changed"]:
        driver.call(["hotkey", "--key", SAVE_KEY], "显式保存解除冲突后的本地文本")
        time.sleep(0.8)
        saved = wait_disk_change(driver, fixture, restored["sha256"], 8.0,
                                 "显式保存后磁盘应变化")
    if not saved["changed"]:
        raise DriverError("冲突解除后本地文本仍无法写盘")
    expected_size = baseline["size"] + len(b"Z")
    if saved["size"] != expected_size:
        raise DriverError(f"恢复后保存尺寸应为 {expected_size}，实际 {saved['size']}")

    # 保存包含待定草稿，但关闭包 source 只取已提交缓冲；先把 Z 组结算进源码，
    # 否则 final.tsv 的 source_len 会少于 stored/disk（run-3 失败轮记录）。
    # 结算键用 Escape：既有真实窗口取证（task-3.1 green-abc step-022）
    # 证明“非组合期封口且保留输入”；run-4 直接 Left、run-5 点正文再 Left
    # 均未结算（draft 仍为 5a），故这里先点回正文再按 Escape。
    driver.call(["click", "--x", str(round(body_point[0], 1)),
                 "--y", str(round(body_point[1], 1))], "结算前点回正文取得焦点")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Escape"], "结算待定组：Escape 封口并保留 Z")
    time.sleep(1.0)
    snapshot_retry(driver, note="结算待定组后读取窗口")

    write_json(driver.evidence / "cycle-missing.json", {
        "baseline": baseline, "deleted": deleted["state"],
        "presented": presented, "absent_after_pause": still_missing,
        "dismissed": dismissed, "represented_after_activation": represented,
        "retry_kept_conflict": retry_still, "restored": restored,
        "retry_cleared": cleared, "saved": saved,
        "note": "删除目标→冲突面板只提供重试/收起→暂停期不重建→重激活重现→"
                "恢复基线后重试解除并写回本地文本",
    })
    notes["expected_saved_size"] = expected_size
    write_json(driver.evidence / "missing-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver, fixture, deleted["bytes"])
    verify_session_rows(driver, baseline["size"], expected_size)
    write_json(driver.evidence / "store-after.json", {
        "active_generations": store_has_generations(),
        "note": "干净关闭后的恢复存储 Active 完整代；仅记录，不在此断言",
    })
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())


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
    if not APP_BINARY.is_file():
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
        activate_with_retry(driver)
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
    print("外部删除冲突取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
