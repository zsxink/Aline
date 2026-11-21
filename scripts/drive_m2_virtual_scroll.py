#!/usr/bin/env python3
"""W5 3.2「组合跨虚拟滚动」真实窗口取证。

被测事实：在长文档首行块内开始真实拼音组合后，用系统事件流投递滚轮把
原行滚出虚拟列表渲染范围，再滚回文档顶部；整个过程组合不得被平台提交或
丢弃，文本、版本与历史保持不变。随后空格确认只提交一次，插入点仍在原组合
起始位置。最后自动保存/显式保存写盘，关闭采样包与磁盘目标逐字节一致，
恢复存储无残留 Active 完整代。

组合期 `cmd-alt-l` 被输入上下文吞掉、不产生布局报告，因此虚拟化事实分两步
取证：先在无组合时用同一滚轮序列取得 `top_row` 前进且 `rendered<rows` 的
debug 布局报告，再在组合中执行同一序列，由采样表在滚动与提交之间保持组合
状态证明原虚拟行卸载后组合仍存活。最后自动保存/显式保存写盘，关闭采样包
与磁盘目标逐字节一致，恢复存储无残留 Active 完整代。
用法：drive_m2_virtual_scroll.py --fixture <路径> --evidence <证据目录>
"""
import argparse
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
from drive_m2_external_missing import (  # noqa: E402  复用启动与存储快照原语
    launch,
    settle_logs,
    store_has_generations,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
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
LAYOUT_REPORT = re.compile(
    r"m1-layout mode=(\S+) rows=(\d+) rendered=(\d+) height_cache=\S+ "
    r"top_row=(\d+)")
SCROLL_EVENTS = 3
SCROLL_BACK_ROUNDS = 4
SCROLL_DELTA = -900


def parse_layout_reports(text):
    """解析 debug 布局报告文本。"""
    reports = []
    for line in text.splitlines():
        hit = LAYOUT_REPORT.search(line)
        if hit:
            reports.append({
                "mode": hit.group(1),
                "rows": int(hit.group(2)),
                "rendered": int(hit.group(3)),
                "top_row": int(hit.group(4)),
            })
    return reports


def layout_reports(evidence):
    """解析证据目录中应用 stderr 的 debug 布局报告。"""
    path = Path(evidence) / "app.stderr"
    if not path.is_file():
        return []
    return parse_layout_reports(path.read_text(errors="replace"))


def runtime_layout_reports(log_dir):
    """读取启动日志目录中尚未归档的 stderr 布局报告。"""
    path = Path(log_dir) / "app.stderr"
    if not path.is_file():
        return []
    return parse_layout_reports(path.read_text(errors="replace"))


def single_insertion(text, baseline, position):
    """核对 text 是 baseline 在 position 处的单点连续插入。"""
    if position < 0 or position > len(baseline):
        raise DriverError("组合起始位置越界：" + str(position))
    suffix = baseline[position:]
    if (len(text) <= len(baseline) or not text.startswith(baseline[:position])
            or not text.endswith(suffix)):
        raise DriverError("最终源字节不是组合起始位置的连续插入")
    inserted = text[position:len(text) - len(suffix)]
    if not inserted:
        raise DriverError("组合提交没有产生插入字节")
    inserted.decode("utf-8")
    return inserted


def verify_virtual_scroll(evidence, baseline_bytes, log_dir):
    """核对组合跨滚动零变化、提交一次且虚拟化报告成立。"""
    rows = session_table(evidence)
    composition_rows = [(index, row) for index, row in enumerate(rows)
                        if int(row["composition_len"] or 0) > 0]
    if not composition_rows:
        raise DriverError("采样未记录组合状态")
    first_index, first = composition_rows[0]
    last_index = composition_rows[-1][0]
    gated_revision = int(first["revision"])
    start = int(first["composition_start"] or -1)
    if start < 0:
        raise DriverError("组合起始位置缺失")

    commit_index = None
    for index in range(last_index + 1, len(rows)):
        row = rows[index]
        if (row["composition_len"] in ("", "0")
                and int(row["revision"]) == gated_revision + 1
                and int(row["undo_depth"]) == 1
                and row["redo_depth"] == "0"):
            commit_index = index
            break
    if commit_index is None:
        raise DriverError("采样缺少组合提交行")

    # 从组合开始到提交前：源码、版本、历史与块状态必须不变。
    for row in rows[first_index:commit_index]:
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("组合期间源码发生变化（seq " + row["seq"] + "）")
        if int(row["revision"]) != gated_revision:
            raise DriverError("组合期间版本推进（seq " + row["seq"] + "）")
        if int(row["undo_depth"]) != 0 or row["redo_depth"] != "0":
            raise DriverError("组合期间历史深度变化（seq " + row["seq"] + "）")
        if row["open_block"] != "true":
            raise DriverError("组合期间块被关闭（seq " + row["seq"] + "）")

    reports = runtime_layout_reports(log_dir)
    scrolled = [report for report in reports
                if report["top_row"] >= 1 and report["rendered"] < report["rows"]]
    if not scrolled:
        raise DriverError("布局报告未记录原行滚出渲染范围（缺 top_row>=1 且 "
                          "rendered<rows）")
    if reports[-1]["top_row"] != 0 or reports[-1]["rendered"] >= reports[-1]["rows"]:
        raise DriverError("滚回顶部后的布局报告未回到 top_row=0 的虚拟化首屏")

    final = (Path(evidence) / "source-after.bin").read_bytes()
    inserted = single_insertion(final, baseline_bytes, start)
    # 长文超过采样器 4096 字节记录上限时 text_hex 为空，源头是否一致由
    # final-check.json 的三方字节对照负责；有采样文本时则必须与关闭源一致。
    last_text = next((row["text_hex"] for row in reversed(rows)
                      if row["text_hex"]), "")
    if last_text and bytes.fromhex(last_text) != final:
        raise DriverError("采样末尾文本与关闭源字节不一致")

    result = {
        "composition_start": start,
        "gated_revision": gated_revision,
        "gated_rows": [int(row["seq"]) for _, row in composition_rows],
        "commit_seq": int(rows[commit_index]["seq"]),
        "commit_undo_depth": int(rows[commit_index]["undo_depth"]),
        "inserted": {
            "size": len(inserted),
            "hex": inserted.hex(),
            "text": inserted.decode("utf-8"),
        },
        "scrolled_reports": scrolled,
        "final_report": reports[-1],
        "text_hex_sampled": bool(last_text),
        "note": "组合期滚动期间零文本/版本/历史变化；提交一次落在原组合起始位置；"
                "长文 text_hex 超采样上限时由关闭三方字节对照",
    }
    write_json(Path(evidence) / "virtual-scroll-check.json", result)
    return result


def report_layout(driver, note):
    """触发 debug 布局报告；报告写入应用 stderr。"""
    driver.call(["hotkey", "--key", "cmd+alt+l"], note)
    time.sleep(0.8)


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
    }

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
    # 像素滚轮在系统加速下并不对称（run-4 实测：3×-900 实际偏移 -7173px，
    # 3×+900 只回滚约 2391px），改为按布局报告分批补滚，直到回到虚拟化首屏。
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

    driver.type_keys(["n", "i"], "拼音组合 ni（长文档首行）", pause=0.35)
    time.sleep(0.5)
    driver.snapshot()
    before_shot = sorted(driver.evidence.glob("step-*.png"))[-1]

    # 组合进行中执行同一滚动序列：原行卸载再重挂，组合须由应用侧采样保持。
    for index in range(SCROLL_EVENTS):
        driver.inject_scroll_wheel(scroll_x, scroll_y, 0, SCROLL_DELTA,
                                   "组合中向下滚动，移出首行（"
                                   + str(index + 1) + "/" + str(SCROLL_EVENTS) + "）")
        time.sleep(0.7)
    driver.snapshot()
    after_shot = sorted(driver.evidence.glob("step-*.png"))[-1]
    notes["scroll_pixel_diff"] = driver.pixel_diff(before_shot, after_shot)

    # 组合期无法用 cmd-alt-l 观测布局，按探针测得的加速不对称补足向上滚轮，
    # 越过顶部会被列表夹住，不改变提交位置；截图记录滚回后的可见区域。
    composition_back_events = SCROLL_BACK_ROUNDS * SCROLL_EVENTS
    for index in range(composition_back_events):
        driver.inject_scroll_wheel(scroll_x, scroll_y, 0, -SCROLL_DELTA,
                                   "组合中滚回文档顶部（" + str(index + 1) + "/"
                                   + str(composition_back_events) + "）")
        time.sleep(0.45)
    notes["composition_back_events"] = composition_back_events
    driver.snapshot()

    driver.call(["press-key", "--key", "space"], "空格确认组合候选")
    time.sleep(1.2)
    driver.snapshot()

    time.sleep(2.2)
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "autosave_changed_disk": auto_save["sha256"] != before["sha256"],
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "虚拟滚动组合提交后写盘",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("跨滚动组合提交后磁盘仍是基线")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    result = verify_virtual_scroll(driver.evidence, baseline_bytes, log_dir)
    write_json(driver.evidence / "layout-reports.json", {
        "reports": runtime_layout_reports(log_dir),
        "note": "cmd-alt-l 触发的 debug m1-layout 报告；top_row/rendered 证明虚拟化",
    })
    if not result["scrolled_reports"]:
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
    print("跨虚拟滚动组合取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
