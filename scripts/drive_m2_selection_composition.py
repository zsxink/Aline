#!/usr/bin/env python3
"""W5 4.1「选中文字上开始组合」真实窗口取证。

被测事实：在可编辑块内用 Shift+Left 真实扩展选区后，直接开始拼音组合并由
空格确认；采样须记录组合前存在非空选区，且提交只形成一步历史。最终源字节
按实际平台结果分类记录为「替换选区」「选区起点插入」或「选区终点插入」，
不预设平台语义；分类不成立或插入字节非法即判失败。

候选窗由输入法自绘，窗口截图不捕获系统候选窗；本驱动只声称选中文字上的
组合与提交结果。运行后经自动保存/显式保存写盘，并核对关闭采样包与磁盘
目标逐字节一致、恢复存储无残留 Active 完整代。
用法：drive_m2_selection_composition.py --fixture <路径> --evidence <证据目录>
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


def classify_result(baseline, final, start, end):
    """把最终字节归类为替换选区或选区边界插入，返回 (kind, inserted)。"""
    removed = end - start
    delta = len(final) - len(baseline)
    if final[:start] == baseline[:start] and \
            final[len(final) - (len(baseline) - end):] == baseline[end:]:
        inserted = final[start:len(final) - (len(baseline) - end)]
        if len(inserted) - removed == delta:
            return "replace", inserted
    if delta > 0:
        inserted = final[start:start + delta]
        if final[:start] == baseline[:start] and \
                final[start + delta:] == baseline[start:]:
            return "insert_at_start", inserted
        inserted = final[end:end + delta]
        if final[:end] == baseline[:end] and \
                final[end + delta:] == baseline[end:]:
            return "insert_at_end", inserted
    raise DriverError("最终字节既不是替换选区也不是选区边界插入")


def verify_selection_composition(evidence, baseline_bytes):
    """核对组合前选区、单步提交与最终字节分类。"""
    rows = session_table(evidence)
    composition_index = None
    for index, row in enumerate(rows):
        if int(row["composition_len"] or 0) > 0:
            composition_index = index
            break
    if composition_index is None:
        raise DriverError("采样未记录组合状态")
    selected = None
    for index in range(composition_index - 1, -1, -1):
        row = rows[index]
        anchor, head = row["selection_anchor"], row["selection_head"]
        if anchor and head and anchor != head:
            selected = row
            break
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("组合前文本已偏离基线（seq " + row["seq"] + "）")
    if selected is None:
        raise DriverError("组合前采样未记录非空选区")
    start = min(int(selected["selection_anchor"]), int(selected["selection_head"]))
    end = max(int(selected["selection_anchor"]), int(selected["selection_head"]))
    if end > len(baseline_bytes):
        raise DriverError("选区超出基线范围")
    if baseline_bytes[start:end].decode("utf-8", "strict") == "":
        raise DriverError("选区为空")

    gated_revision = int(rows[composition_index]["revision"])
    commit_index = None
    for index in range(composition_index + 1, len(rows)):
        row = rows[index]
        if (row["composition_len"] in ("", "0")
                and int(row["revision"]) == gated_revision + 1
                and int(row["undo_depth"]) >= 1
                and row["redo_depth"] == "0"):
            commit_index = index
            break
    if commit_index is None:
        raise DriverError("采样缺少组合提交行")
    commit = rows[commit_index]

    # 提交前（从组合开始到提交前）文本必须保持基线，且不得出现第二份历史。
    for row in rows[composition_index:commit_index]:
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("组合期间源文本被提前改写（seq " + row["seq"] + "）")
        if int(row["undo_depth"]) != 0 or row["redo_depth"] != "0":
            raise DriverError("组合期间历史深度发生变化（seq " + row["seq"] + "）")

    final = (Path(evidence) / "source-after.bin").read_bytes()
    kind, inserted = classify_result(baseline_bytes, final, start, end)
    inserted.decode("utf-8")

    result = {
        "selection": {
            "anchor": int(selected["selection_anchor"]),
            "head": int(selected["selection_head"]),
            "reversed": selected["selection_reversed"] == "true",
            "range": [start, end],
            "text": baseline_bytes[start:end].decode("utf-8"),
        },
        "composition_revision": gated_revision,
        "commit_seq": int(commit["seq"]),
        "commit_undo_depth": int(commit["undo_depth"]),
        "commit_redo_depth": int(commit["redo_depth"]),
        "outcome": kind,
        "inserted": {
            "size": len(inserted),
            "hex": inserted.hex(),
            "text": inserted.decode("utf-8"),
        },
        "final_size": len(final),
        "note": "选中文字上开始组合；结果按实际平台语义分类",
    }
    write_json(Path(evidence) / "selection-composition-check.json", result)
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

    clear_feedback_and_open_block(driver, body_point, "点击标题行打开块编辑")
    driver.call(["hotkey", "--key", "shift+Left"], "Shift+Left 选中前一个字符")
    time.sleep(0.5)
    snapshot = driver.snapshot()
    notes["selection_snapshot"] = driver.serial

    notes["composition_start"] = driver.serial + 1
    driver.type_keys(["n", "i"], "选中文字上开始拼音组合 ni", pause=0.35)
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["press-key", "--key", "space"], "空格确认组合候选")
    time.sleep(1.0)
    driver.snapshot()

    time.sleep(2.2)
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "组合提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "autosave_changed_disk": auto_save["sha256"] != before["sha256"],
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "选中文字上组合提交后写盘",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("选中文字组合提交后磁盘仍是基线")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    verify_selection_composition(driver.evidence, baseline_bytes)
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
    print("选中文字组合取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
