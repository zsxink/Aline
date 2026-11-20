#!/usr/bin/env python3
"""W5 3.2/3.4「组合等待保存 × 跨虚拟滚动」真实窗口取证。

被测事实：长文档首行块内开始真实拼音组合后，经真实「文件 → 保存」菜单进入
等待状态；等待期间用系统事件流投递滚轮把原行滚出虚拟列表渲染范围，再滚回
文档顶部。整个过程保存不得提前写盘，组合不得被菜单或滚动提交/丢弃，文本、
版本与历史保持不变。随后空格确认只提交一次，等待保存结算并写盘；关闭采样
包与磁盘目标逐字节一致，恢复存储无残留 Active 完整代。

组合期 `cmd-alt-l` 被输入上下文吞掉、不产生布局报告，因此虚拟化事实分两步
取证：先在无组合时用同一滚轮序列取得 `top_row` 前进且 `rendered<rows` 的
debug 布局报告，再在组合等待中执行同一序列，由采样表在等待与提交之间保持
组合状态、磁盘不变与关闭后单点插入共同证明原虚拟行卸载后组合仍存活且保存
未提前发生。
用法：drive_m2_scroll_save_wait.py --fixture <路径> --evidence <证据目录>
"""
import argparse
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
from drive_m2_menu_focus import (  # noqa: E402  复用菜单保存与关闭核对原语
    activate_ready,
    clear_feedback_and_open_block,
    fixture_state,
    menubar_click,
    verify_close_packet,
    wait_disk_change,
)
from drive_m2_virtual_scroll import (  # noqa: E402  复用布局报告与单点插入核对
    report_layout,
    runtime_layout_reports,
    single_insertion,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
SCROLL_EVENTS = 3
SCROLL_BACK_ROUNDS = 4
SCROLL_BACK_EVENTS = SCROLL_BACK_ROUNDS * SCROLL_EVENTS
SCROLL_DELTA = -900


def candidate_text(inserted, label):
    """要求提交字节是单个非 ASCII 可打印候选字，不依赖具体候选。"""
    text = inserted.decode("utf-8")
    if len(text) != 1 or ord(text) < 0x80 or not text.isprintable():
        raise DriverError(label + "不是单个非 ASCII 可打印候选字")
    return text


def verify_scroll_save_wait(evidence, baseline_bytes, log_dir):
    """核对等待×滚动期间零变化、等待结束后单次提交且保存结算。"""
    rows = session_table(evidence)
    waiting = [(index, row) for index, row in enumerate(rows)
               if row["wait"] == "pending"
               and row["save"] == "waiting-for-composition"
               and int(row["composition_len"] or 0) > 0]
    if not waiting:
        raise DriverError("采样未记录组合期保存等待（wait=pending / "
                          "save=waiting-for-composition）")
    first_index, first = waiting[0]
    gated_revision = int(first["revision"])
    start = int(first["composition_start"] or -1)
    if start < 0:
        raise DriverError("等待中的组合缺少起始位置")
    last_wait_index = waiting[-1][0]

    commit_index = None
    for index in range(last_wait_index + 1, len(rows)):
        row = rows[index]
        if (row["composition_len"] in ("", "0")
                and int(row["revision"]) == gated_revision + 1
                and int(row["undo_depth"]) == 1
                and row["redo_depth"] == "0"
                and row["wait"] == "idle"
                and row["save"] == "idle"):
            commit_index = index
            break
    if commit_index is None:
        raise DriverError("采样缺少等待结束后的组合提交行")

    # 从首个等待采样到提交之前：源码、版本、历史与块状态必须不变，等待
    # 状态必须贯穿；滚动期间不得出现任何文本增长或切断。
    for row in rows[first_index:commit_index]:
        if row["wait"] != "pending" or row["save"] != "waiting-for-composition":
            raise DriverError("等待状态在提交前中断（seq " + row["seq"] + "）")
        if int(row["composition_len"] or 0) <= 0:
            raise DriverError("等待期间组合丢失（seq " + row["seq"] + "）")
        if int(row["composition_start"]) != start:
            raise DriverError("等待期间组合起始位置漂移（seq " + row["seq"] + "）")
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("等待×滚动期间源码长度变化（seq " + row["seq"] + "）")
        if int(row["revision"]) != gated_revision:
            raise DriverError("等待×滚动期间版本推进（seq " + row["seq"] + "）")
        if int(row["undo_depth"]) != 0 or row["redo_depth"] != "0":
            raise DriverError("等待×滚动期间历史深度变化（seq " + row["seq"] + "）")
        if row["open_block"] != "true":
            raise DriverError("等待×滚动期间块被关闭（seq " + row["seq"] + "）")

    reports = runtime_layout_reports(log_dir)
    final = (Path(evidence) / "source-after.bin").read_bytes()
    inserted = single_insertion(final, baseline_bytes, start)
    text = candidate_text(inserted, "等待后空格提交")
    result = {
        "composition_start": start,
        "waiting_rows": [int(row["seq"]) for _, row in waiting],
        "gated_revision": gated_revision,
        "commit_seq": int(rows[commit_index]["seq"]),
        "commit_undo_depth": int(rows[commit_index]["undo_depth"]),
        "inserted": {
            "size": len(inserted),
            "hex": inserted.hex(),
            "text": text,
        },
        "layout_reports": reports,
        "note": "组合期保存等待与跨虚拟滚动交错期间零文本/版本/历史变化；"
                "提交一次落在原组合起始位置；等待在提交行前保持 pending",
    }
    write_json(Path(evidence) / "scroll-save-wait-check.json", result)
    return result


def run(driver, fixture, log_dir):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    baseline_bytes = Path(fixture).read_bytes()
    (driver.evidence / "baseline.bin").write_bytes(baseline_bytes)
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    snapshot = driver.snapshot()
    window = snapshot.get("window") or {}
    body_point = driver.heading_point()
    notes = {
        "body_point": [round(body_point[0], 1), round(body_point[1], 1)],
        "window": {"width": window.get("width"), "height": window.get("height")},
        "baseline": before,
    }

    # 对照：无组合时菜单保存送达应用；未修改文档不应改盘。
    notes["control_menu_save"] = driver.serial + 1
    menubar_click(driver, "保存", "对照：无组合时菜单保存")
    time.sleep(0.8)
    driver.snapshot()
    control_disk = fixture_state(fixture)
    write_json(driver.evidence / "control-disk.json", {
        "after_menu_save": control_disk,
        "unchanged_bytes": control_disk["sha256"] == before["sha256"],
    })
    if control_disk["sha256"] != before["sha256"]:
        raise DriverError("对照菜单保存改写了未修改文档")

    clear_feedback_and_open_block(driver, body_point, "点击首行标题打开块编辑")
    report_layout(driver, "滚动前布局报告（无组合）")
    driver.snapshot()
    top_report = runtime_layout_reports(log_dir)[-1:]
    if not top_report or top_report[0]["top_row"] != 0 \
            or top_report[0]["rendered"] >= top_report[0]["rows"]:
        raise DriverError("滚动前布局报告未显示首屏虚拟化")

    scroll_x = round(body_point[0], 1)
    scroll_y = 420.0
    # 先无组合地执行同一滚轮序列，取得虚拟化布局报告。
    for index in range(SCROLL_EVENTS):
        driver.inject_scroll_wheel(scroll_x, scroll_y, 0, SCROLL_DELTA,
                                   "向下滚动，把首行移出虚拟渲染范围（"
                                   + str(index + 1) + "/" + str(SCROLL_EVENTS) + "）")
        time.sleep(0.7)
    report_layout(driver, "滚出后布局报告（无组合）")
    driver.snapshot()
    down_report = runtime_layout_reports(log_dir)[-1:]
    if not down_report or down_report[0]["top_row"] < 1 \
            or down_report[0]["rendered"] >= down_report[0]["rows"]:
        raise DriverError("滚出后布局报告未显示原行移出虚拟渲染范围")
    notes["down_report"] = down_report[0]
    # 像素滚轮在系统加速下并不对称，按布局报告分批补滚回虚拟化首屏。
    back_report = None
    back_rounds = 0
    for _ in range(SCROLL_BACK_ROUNDS):
        for index in range(SCROLL_EVENTS):
            driver.inject_scroll_wheel(scroll_x, scroll_y, 0, -SCROLL_DELTA,
                                       "滚回文档顶部（第 " + str(back_rounds + 1)
                                       + " 轮 " + str(index + 1) + "/"
                                       + str(SCROLL_EVENTS) + "）")
            time.sleep(0.7)
        back_rounds += 1
        report_layout(driver, "滚回顶部布局报告（第 " + str(back_rounds)
                      + " 轮，无组合）")
        driver.snapshot()
        back_report = runtime_layout_reports(log_dir)[-1:]
        if back_report and back_report[0]["top_row"] == 0 \
                and back_report[0]["rendered"] < back_report[0]["rows"]:
            break
    if not back_report or back_report[0]["top_row"] != 0 \
            or back_report[0]["rendered"] >= back_report[0]["rows"]:
        raise DriverError("滚回顶部后的布局报告未回到虚拟化首屏（已补滚 "
                          + str(back_rounds) + " 轮）")
    notes["back_rounds"] = back_rounds
    notes["back_report"] = back_report[0]

    # 组合进行中经真实菜单保存进入等待。
    driver.type_keys(["n", "i"], "拼音组合 ni（等待保存 × 跨滚动）", pause=0.35)
    time.sleep(0.5)
    driver.snapshot()
    notes["menu_wait_start"] = driver.serial + 1
    menubar_click(driver, "保存", "组合进行中经菜单保存（等待）")
    time.sleep(0.8)
    driver.snapshot()
    disk_wait = fixture_state(fixture)
    if disk_wait["sha256"] != before["sha256"]:
        raise DriverError("进入保存等待时磁盘已被提前写入")

    # 等待期间执行跨虚拟滚动：原行卸载后再重挂，组合与等待须保持。
    for index in range(SCROLL_EVENTS):
        driver.inject_scroll_wheel(scroll_x, scroll_y, 0, SCROLL_DELTA,
                                   "等待中向下滚动，移出首行（"
                                   + str(index + 1) + "/" + str(SCROLL_EVENTS) + "）")
        time.sleep(0.7)
    driver.snapshot()
    disk_mid = fixture_state(fixture)
    if disk_mid["sha256"] != before["sha256"]:
        raise DriverError("等待中向下滚动期间磁盘被写入")
    for index in range(SCROLL_BACK_EVENTS):
        driver.inject_scroll_wheel(scroll_x, scroll_y, 0, -SCROLL_DELTA,
                                   "等待中滚回文档顶部（" + str(index + 1) + "/"
                                   + str(SCROLL_BACK_EVENTS) + "）")
        time.sleep(0.45)
    driver.snapshot()
    disk_back = fixture_state(fixture)
    if disk_back["sha256"] != before["sha256"]:
        raise DriverError("等待中滚回顶部期间磁盘被写入")
    notes["wait_disk_unchanged"] = True
    write_json(driver.evidence / "wait-disk.json", {
        "disk_at_wait": disk_wait,
        "disk_after_scroll_down": disk_mid,
        "disk_after_scroll_back": disk_back,
        "unchanged_through_scroll": True,
        "note": "组合保存等待 × 跨虚拟滚动期间磁盘保持基线",
    })

    # 空格确认组合，等待保存应结算写盘一次。
    notes["commit_step"] = driver.serial + 1
    driver.call(["press-key", "--key", "space"], "空格确认组合候选（等待结算）")
    time.sleep(1.2)
    driver.snapshot()
    saved = wait_disk_change(driver, fixture, before["sha256"], 8.0,
                             "等待保存应在组合提交后结算写盘")
    notes["saved"] = saved
    if not saved["changed"]:
        raise DriverError("组合提交后等待保存未写盘")

    time.sleep(2.2)
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "wait_settled_disk": saved,
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "等待保存结算与显式保存后的磁盘状态",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("等待保存结算后磁盘仍是基线")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    result = verify_scroll_save_wait(driver.evidence, baseline_bytes, log_dir)
    write_json(driver.evidence / "layout-reports.json", {
        "reports": runtime_layout_reports(log_dir),
        "note": "cmd-alt-l 触发的 debug m1-layout 报告；top_row/rendered 证明虚拟化",
    })
    if not result["layout_reports"]:
        raise DriverError("缺少滚动期间的虚拟化布局报告")
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
        run(driver, fixture.resolve(), log_dir)
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
    print("组合等待保存 × 跨虚拟滚动取证完成，步数：" + str(driver.serial),
          flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
