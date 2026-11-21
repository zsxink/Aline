#!/usr/bin/env python3
"""W5 3.4「外部重命名/移动目标」真实窗口取证。

被测事实（两段，共用一份文档与一个插入标记 Z）：

  A) 组合等待 × 外部重命名：真实拼音组合 `ni` 经「文件 → 保存」进入
     ED-01 等待后，磁盘目标被外部 `mv` 到同目录新名。等待不得提前写盘、
     不得在新名重建文件；空格自然确认结算等待时写盘失败转冲突缺失态
     （external=missing/save=conflict/自动保存暂停），面板只提供
     「重试检查磁盘」「收起冲突提示」。暂停期不重建原目标；把基线字节
     写回原目标后重试解除冲突，本地文本（基线+你）写回原目标。

  B) 脏文档 × 外部移动：再次变脏（一个 Z）并把目标 `mv` 到子目录新名，
     显式保存同样进入缺失态；这次把基线字节恢复到**移动后的新路径**并
     重试——应用绑定的是原路径，必须保持缺失（不得跟随重命名到新目标
     写盘）；随后把基线写回原路径重试才解除，本地文本写回原路径。

关闭采样 source/stored/disk 与磁盘原目标逐字节一致（基线+你+Z）；新路径
文件保持恢复时的基线字节（应用从未写它）。输入经真实系统事件/无障碍接口投递；应用由
脚本以 `ALINE_M2_EVIDENCE_DIR` 启动。
用法：drive_m2_external_rename.py --fixture <路径> --evidence <证据目录>
      drive_m2_external_rename.py --self-test
"""
import argparse
import csv
from pathlib import Path
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
from drive_m2_external_missing import (  # noqa: E402  复用缺失冲突原语
    MISSING_BUTTONS,
    NO_BUTTONS,
    activate_with_retry,
    assert_missing_panel,
    conflict_save,
    launch,
    panel_click,
    require_absent,
    settle_logs,
    snapshot_retry,
    store_has_generations,
    wait_missing_panel,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m2_candidate_menu_deactivation import (  # noqa: E402  复用前台原语
    set_frontmost_timed,
)
from drive_m2_menu_focus import (  # noqa: E402  复用菜单与磁盘原语
    fixture_state,
    menubar_click,
    verify_close_packet,
    wait_disk_change,
)
from drive_m2_save_lifecycle import clear_feedback_and_open_block  # noqa: E402
from drive_m2_wait_deactivation import (  # noqa: E402  复用等待建立与前台复查
    enter_save_wait,
    ensure_our_frontmost,
    reset_fixture,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
KEY_I = 34
KEY_N = 45


def move_target(driver, source, destination, note):
    """外部重命名/移动目标；原字节备份进证据目录供恢复基线。"""
    source = Path(source)
    destination = Path(destination)
    data = source.read_bytes()
    before = fixture_state(source)
    backup = driver.evidence / ("moved-bytes-" + str(driver.serial) + ".bin")
    backup.write_bytes(data)
    destination.parent.mkdir(parents=True, exist_ok=True)
    source.rename(destination)
    if source.exists() or not destination.is_file():
        raise DriverError("外部移动目标失败：" + note)
    result = {"note": note, "before": before,
              "destination_name": destination.name,
              "destination_size": len(data), "step": driver.serial}
    write_json(driver.evidence / ("rename-" + str(driver.serial) + ".json"),
               result)
    return {"state": before, "bytes": data, "destination": destination}


def restore_bytes(driver, path, data, note):
    """把基线字节写回指定路径（原目标或移动后目标），模拟外部恢复。"""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    state = fixture_state(path)
    write_json(driver.evidence / ("restore-" + str(driver.serial) + ".json"),
               {"note": note, "path_name": path.name, **state})
    return state


def truthy(value):
    return str(value).strip().lower() == "true"


def verify_session(evidence, baseline_size):
    """采样必须出现两轮 missing 冲突链，且新路径从未被应用写盘。

    第一轮（等待结算失败）与第二轮（脏文档保存失败）各要求
    external=missing/save=conflict/暂停 的采样行；两轮之间与末尾各有
    解除后的干净可保存行。文本尺寸链：基线 → +你(3) → +Z(1) → +Z(1)。
    """
    path = Path(evidence) / "session.tsv"
    with open(path, newline="", encoding="utf-8") as handle:
        rows = [row for row in csv.DictReader(handle, delimiter="\t")
                if (row.get("seq") or "").isdigit()]
    if not rows:
        raise DriverError("session.tsv 没有采样行")

    def seqs(predicate):
        return [int(row["seq"]) for row in rows if predicate(row)]

    missing = seqs(lambda row: row["external"] == "missing"
                   and row["save"] == "conflict"
                   and truthy(row["autosave_paused"]))
    # 按「已保存解除行」（save=idle、external=none、autosave 未暂停）切分
    # missing 段：收起冲突提示只把 external 置 none，冲突/暂停保持，不算
    # 解除。missing 行之间只要没有真实解除行就并入同一段（哪怕中间夹着
    # 收起行）；两轮外部重命名/移动之间必须真实保存解除过一次。
    episodes = []
    seen_clear = False
    for row in rows:
        seq = int(row["seq"])
        cleared_row = (row["save"] == "idle" and row["external"] == "none"
                       and not truthy(row["autosave_paused"]))
        if seq in missing:
            if episodes and not seen_clear:
                episodes[-1].append(seq)
            else:
                episodes.append([seq])
            seen_clear = False
        elif cleared_row:
            seen_clear = True
    size_chain = sorted({int(row["text_len"]) for row in rows})
    expected_sizes = [baseline_size, baseline_size + 3,
                      baseline_size + 4]
    result = {
        "rows": len(rows),
        "missing_episodes": episodes,
        "text_sizes": size_chain,
        "expected_sizes": expected_sizes,
        "last_text_len": int(rows[-1]["text_len"]),
        "last_external": rows[-1]["external"],
        "last_save": rows[-1]["save"],
        "note": "两轮外部重命名/移动各产生一段 missing 冲突；尺寸链为"
                "基线→+你→+Z；终态干净",
    }
    write_json(Path(evidence) / "session-check.json", result)
    if len(episodes) != 2:
        raise DriverError("missing 冲突段数应为 2（重命名/移动各一），实际 "
                          + str(len(episodes)))
    if size_chain != expected_sizes:
        raise DriverError("文本尺寸链应为 " + str(expected_sizes)
                          + "，实际 " + str(size_chain))
    if int(rows[-1]["text_len"]) != baseline_size + 4:
        raise DriverError("终态文本尺寸应为基线+4")
    if rows[-1]["external"] == "missing" or rows[-1]["save"] == "conflict":
        raise DriverError("终态仍处于缺失冲突")
    return result


def verify_new_path_untouched(evidence, moved_bytes, destination):
    """移动后的新路径必须保持恢复时的基线字节（应用绑定原路径）。"""
    current = Path(destination).read_bytes()
    result = {"destination_name": Path(destination).name,
              "size": len(current), "equals_restored_baseline":
              current == moved_bytes}
    write_json(Path(evidence) / "new-path-check.json", result)
    if current != moved_bytes:
        raise DriverError("应用写动了移动后的新路径文件（应只写原目标）")
    return result


def self_test():
    """合成表只验证取证器的失败关闭，不作为原生窗口证据。"""
    baseline_size = 100
    columns = ("seq", "text_len", "external", "save", "autosave_paused")

    def row(seq, size, external, save, paused):
        return [str(seq), str(size), external, save, str(paused).lower()]

    rows = [
        row(0, baseline_size, "none", "idle", False),
        row(1, baseline_size, "none", "waiting-for-composition", False),
        row(2, baseline_size + 3, "missing", "conflict", True),
        row(3, baseline_size + 3, "missing", "conflict", True),
        row(4, baseline_size + 3, "none", "idle", False),
        row(5, baseline_size + 4, "none", "idle", False),
        row(6, baseline_size + 4, "missing", "conflict", True),
        row(7, baseline_size + 4, "none", "idle", False),
    ]
    with tempfile.TemporaryDirectory(dir=ROOT / "logs/validation",
                                     prefix="external-rename-self-test-") as temporary:
        evidence = Path(temporary)

        def install(records):
            (evidence / "session.tsv").write_text(
                "\t".join(columns) + "\n"
                + "\n".join("\t".join(record) for record in records)
                + "\ntruncated=false\n", encoding="utf-8")

        def rejected(operation):
            try:
                operation()
            except DriverError:
                return
            raise DriverError("最小违规反例被错误判为通过")

        install(rows)
        verify_session(evidence, baseline_size)
        # 反例 1：只有一轮 missing（移动段未进入冲突）。
        install([record for record in rows
                 if record[0] not in ("6",)])
        rejected(lambda: verify_session(evidence, baseline_size))
        # 反例 2：两轮之间只有「收起」（external=none 但冲突/暂停保持），
        # 没有真实保存解除——不得算两段（seq5 也保持暂停）。
        broken = [record.copy() for record in rows]
        broken[4] = row(4, baseline_size + 3, "none", "conflict", True)
        broken[5] = row(5, baseline_size + 4, "none", "conflict", True)
        install(broken)
        rejected(lambda: verify_session(evidence, baseline_size))
        # 反例 3：尺寸链缺步（等待结算未提交「你」）。
        broken = [record.copy() for record in rows]
        for record in broken:
            if int(record[1]) >= baseline_size + 3:
                record[1] = str(int(record[1]) - 3)
        install(broken)
        rejected(lambda: verify_session(evidence, baseline_size))
        # 反例 4：终态仍缺失冲突（末行并入第二段，段数仍 2，由终态检查拒绝）。
        broken = [record.copy() for record in rows]
        broken[-1] = row(7, baseline_size + 4, "missing", "conflict", True)
        install(broken)
        rejected(lambda: verify_session(evidence, baseline_size))
        # 新路径核对：正例与反例。
        destination = evidence / "moved.md"
        destination.write_bytes(b"baseline")
        verify_new_path_untouched(evidence, b"baseline", destination)
        destination.write_bytes(b"app-wrote-here")
        rejected(lambda: verify_new_path_untouched(evidence, b"baseline",
                                                   destination))
    print("外部重命名取证器：2 正例、5 最小违规反例通过（合成，非平台证据）")


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    reset_fixture(fixture)
    baseline_bytes = Path(fixture).read_bytes()
    (driver.evidence / "baseline.bin").write_bytes(baseline_bytes)
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    ensure_our_frontmost(driver, "打开文档后确认被测实例在前台")
    body_point = driver.heading_point()
    notes = {"body_point": [round(body_point[0], 1), round(body_point[1], 1)]}

    # 场景 A：组合等待 × 外部重命名。
    clear_feedback_and_open_block(driver, body_point, "进入块编辑（等待重命名）")
    ensure_our_frontmost(driver, "组合前确认前台")
    driver.inject_keys([KEY_N, KEY_I], "HID 拼音组合 ni（等待重命名）",
                       names=["n", "i"])
    time.sleep(0.6)
    driver.snapshot()
    notes["wait_entry"] = enter_save_wait(
        driver, fixture, before["sha256"], "组合进行中经菜单保存（等待重命名）")
    renamed = move_target(driver, fixture,
                          Path(fixture).with_name("renamed-away.md"),
                          "等待中外部重命名目标")
    notes["renamed"] = {"name": renamed["destination"].name}
    time.sleep(2.0)
    require_absent(driver, fixture, "重命名后等待期 2 秒（不得重建原目标）")
    if renamed["destination"].read_bytes() != renamed["bytes"]:
        raise DriverError("等待期应用写动了重命名后的新路径")
    driver.snapshot()
    ensure_our_frontmost(driver, "空格提交前确认前台")
    driver.call(["press-key", "--key", "space"], "空格自然确认，等待结算写盘应失败")
    time.sleep(1.2)
    presented = wait_missing_panel(driver, MISSING_BUTTONS, 6.0,
                                   "等待结算失败后缺失面板呈现")
    notes["phase_a_presented"] = presented
    time.sleep(2.0)
    require_absent(driver, fixture, "缺失冲突暂停期 2 秒（不得重建原目标）")
    restored_a = restore_bytes(driver, fixture, renamed["bytes"],
                               "把基线字节写回原目标（场景 A 恢复）")
    panel_click(driver, "重试检查磁盘", "重试检查磁盘（原目标恢复基线）")
    time.sleep(1.4)
    cleared_a = assert_missing_panel(driver, NO_BUTTONS, "场景 A 恢复后重试")
    saved_a = wait_disk_change(driver, fixture, restored_a["sha256"], 8.0,
                               "场景 A 冲突解除后本地文本应写回原目标")
    if not saved_a["changed"]:
        driver.call(["hotkey", "--key", "cmd+s"], "场景 A 显式保存")
        time.sleep(0.8)
        saved_a = wait_disk_change(driver, fixture, restored_a["sha256"], 8.0,
                                   "场景 A 显式保存后磁盘应变化")
    if not saved_a["changed"] or saved_a["size"] != before["size"] + 3:
        raise DriverError("场景 A 解除后写盘尺寸应为基线+3（你）")
    notes["phase_a_saved"] = saved_a
    notes["phase_a_cleared"] = cleared_a

    # 场景 B：脏文档 × 外部移动到子目录；恢复新路径不得解除原路径冲突。
    clear_feedback_and_open_block(driver, body_point, "进入块编辑（移动目标）")
    driver.call(["type-text", "--text", "Z"], "插入 ASCII 标记 Z 使文档再次变脏")
    time.sleep(0.3)
    moved = move_target(driver, fixture,
                        Path(fixture).parent / "elsewhere" / "moved-away.md",
                        "脏文档目标外部移动到子目录")
    notes["moved"] = {"name": moved["destination"].name}
    conflict_save(driver, "移动目标下 Cmd+S（保存失败后静默检查）")
    presented_b = wait_missing_panel(driver, MISSING_BUTTONS, 6.0,
                                     "移动冲突面板呈现")
    notes["phase_b_presented"] = presented_b
    time.sleep(2.0)
    require_absent(driver, fixture, "移动冲突暂停期 2 秒（不得重建原目标）")
    # 把基线恢复到移动后的新路径：应用绑定原路径，重试必须保持缺失。
    restore_bytes(driver, moved["destination"], moved["bytes"],
                  "把基线恢复到移动后的新路径（不得解除原路径冲突）")
    panel_click(driver, "重试检查磁盘", "重试检查磁盘（仅新路径恢复）")
    time.sleep(1.2)
    still_b = assert_missing_panel(driver, MISSING_BUTTONS,
                                   "仅新路径恢复后重试仍缺失")
    notes["phase_b_retry_still_missing"] = still_b
    require_absent(driver, fixture, "仅新路径恢复后原目标仍不得重建")
    verify_new_path_untouched(driver.evidence, moved["bytes"],
                              moved["destination"])
    # 恢复原路径才解除。
    restored_b = restore_bytes(driver, fixture, moved["bytes"],
                               "把基线写回原目标（场景 B 恢复）")
    panel_click(driver, "重试检查磁盘", "重试检查磁盘（原目标恢复基线）")
    time.sleep(1.4)
    cleared_b = assert_missing_panel(driver, NO_BUTTONS, "场景 B 恢复后重试")
    saved_b = wait_disk_change(driver, fixture, restored_b["sha256"], 8.0,
                               "场景 B 冲突解除后本地文本应写回原目标")
    if not saved_b["changed"]:
        driver.call(["hotkey", "--key", "cmd+s"], "场景 B 显式保存")
        time.sleep(0.8)
        saved_b = wait_disk_change(driver, fixture, restored_b["sha256"], 8.0,
                                   "场景 B 显式保存后磁盘应变化")
    if not saved_b["changed"] or saved_b["size"] != before["size"] + 4:
        raise DriverError("场景 B 解除后写盘尺寸应为基线+4（你+Z）")
    notes["phase_b_saved"] = saved_b
    notes["phase_b_cleared"] = cleared_b

    # 结算待定草稿后干净关闭（Escape 封口保留输入，先点回正文取得焦点）。
    driver.call(["click", "--x", str(round(body_point[0], 1)),
                 "--y", str(round(body_point[1], 1))], "结算前点回正文取得焦点")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Escape"], "结算待定组：Escape 封口并保留输入")
    time.sleep(1.0)
    snapshot_retry(driver, note="结算待定组后读取窗口")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    verify_session(driver.evidence, before["size"])
    verify_new_path_untouched(driver.evidence, moved["bytes"],
                              moved["destination"])
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    write_json(driver.evidence / "store-after.json", {
        "active_generations": store_has_generations(),
        "note": "干净关闭后的恢复存储 Active 完整代；应清空",
    })
    if store_has_generations():
        raise DriverError("干净关闭后恢复存储仍有 Active 完整代")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.fixture is None or args.evidence is None:
        print("真实取证需要 --fixture 与 --evidence", file=sys.stderr)
        return 2
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
    driver = None
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
    print("外部重命名/移动取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())