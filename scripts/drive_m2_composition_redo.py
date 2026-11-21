#!/usr/bin/env python3
"""W5 3.2「组合期 Redo 门控」真实窗口取证。

被测事实：先做一次可撤销提交并撤销，留下非空 redo 分支；重新打开块开始真实
中文组合后，经「编辑 → 重做」菜单执行 Redo——应用须拒绝且零副作用：组合、
草稿、源码、模式、版本与撤销/重做深度全部不变，并反馈中文组合待定提示。
空格提交组合后，新输入应只形成一次提交并清除旧 redo 分支；随后自动保存/
显式保存写盘，关闭采样包与磁盘目标逐字节一致，恢复存储无残留 Active 完整代。

菜单入口绕开组合期可能被输入法消费的 `Cmd+Shift+Z` 和弦（同 run-4 的
「文件 → 保存」路径）；真实输入仍经无障碍接口投递。应用由脚本以
`ALINE_M2_EVIDENCE_DIR` 启动，采样器在文档关闭路径落盘，脚本随后核对
session.tsv 的 composition/undo_depth/redo_depth/revision 列。
用法：drive_m2_composition_redo.py --fixture <路径> --evidence <证据目录>
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

APP_PROCESS = "aline"
EDIT_MENU = "编辑"
REDO_ITEM = "重做"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
COMPOSITION_FEEDBACK = "输入法组合尚未结束"


def click_edit_menu(driver, item, note):
    """经 System Events 点击「编辑」菜单项；记录命令与返回码。"""
    driver.serial += 1
    script = (f'tell application "System Events" to tell process "{APP_PROCESS}" '
              f'to click menu item "{item}" of menu "{EDIT_MENU}" of menu bar item '
              f'"{EDIT_MENU}" of menu bar 1')
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", f"<System Events: {EDIT_MENU} > {item}>"],
        "note": note,
        "exit_code": result.returncode,
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise DriverError("菜单点击失败（" + item + "）：" + result.stderr[-600:])


def verify_redo_gate(evidence, baseline_bytes):
    """核对组合期 Redo 零副作用、提交一次且旧 redo 分支被清除。"""
    rows = session_table(evidence)
    gated = [row for row in rows
             if int(row["composition_len"] or 0) > 0
             and row["redo_depth"] == "1"]
    if not gated:
        raise DriverError("采样未记录「组合进行中且存在 redo 分支」的状态")
    first_index = rows.index(gated[0])
    gated_revision = int(gated[0]["revision"])
    commit_index = None
    for index in range(first_index + 1, len(rows)):
        row = rows[index]
        if (row["composition_len"] in ("", "0")
                and row["redo_depth"] == "0"
                and int(row["undo_depth"]) == 1
                and int(row["text_len"]) > len(baseline_bytes)):
            commit_index = index
            break
    if commit_index is None:
        raise DriverError("采样缺少「组合提交后 redo 分支清除」的状态")

    # 从首个受门控组合态到提交前：文本、模式、块、版本与历史深度不得变化。
    for row in rows[first_index:commit_index]:
        if int(row["text_len"]) != len(baseline_bytes):
            raise DriverError("组合期 Redo 改动了源码（seq " + row["seq"] + "）")
        if row["open_block"] != "true" or row["mode"] != "visual":
            raise DriverError("组合期 Redo 改变了块或模式（seq " + row["seq"] + "）")
        if int(row["undo_depth"]) != 0 or row["redo_depth"] != "1":
            raise DriverError("组合期 Redo 改动了历史深度（seq " + row["seq"] + "）")
        if int(row["revision"]) != gated_revision:
            raise DriverError("组合期 Redo 推进了文档版本（seq " + row["seq"] + "）")

    commit_row = rows[commit_index]
    if int(commit_row["revision"]) != gated_revision + 1:
        raise DriverError("组合提交的版本号不是组合前 +1："
                          + commit_row["revision"])
    if int(commit_row["undo_depth"]) != 1:
        raise DriverError("组合提交的撤销深度不是 1："
                          + commit_row["undo_depth"])

    # 最终源字节必须是在组合起始位置的单点连续插入。
    final = (Path(evidence) / "source-after.bin").read_bytes()
    position = int(gated[0]["composition_start"] or -1)
    if position < 0 or position > len(baseline_bytes):
        raise DriverError("组合起始位置不可用：" + gated[0]["composition_start"])
    suffix = baseline_bytes[position:]
    if (len(final) <= len(baseline_bytes)
            or not final.startswith(baseline_bytes[:position])
            or not final.endswith(suffix)):
        raise DriverError("最终源字节不是组合起始位置的连续插入")
    inserted = final[position:len(final) - len(suffix)]
    if not inserted:
        raise DriverError("组合提交没有产生插入字节")
    try:
        inserted.decode("utf-8")
        final.decode("utf-8")
    except UnicodeDecodeError as error:
        raise DriverError("最终源字节不是完整 UTF-8：" + str(error))
    last_text = next((row["text_hex"] for row in reversed(rows)
                      if row["text_hex"]), "")
    if not last_text:
        raise DriverError("采样缺少文本快照")
    if bytes.fromhex(last_text) != final:
        raise DriverError("采样末尾文本与关闭源字节不一致")

    result = {
        "gated_rows": [int(row["seq"]) for row in gated],
        "gated_revision": gated_revision,
        "commit_seq": int(commit_row["seq"]),
        "commit_text_len": int(commit_row["text_len"]),
        "commit_undo_depth": int(commit_row["undo_depth"]),
        "commit_redo_depth": int(commit_row["redo_depth"]),
        "insertion": {
            "position": position,
            "size": len(inserted),
            "hex": inserted.hex(),
            "text": inserted.decode("utf-8"),
        },
        "note": "组合期 Redo 零副作用；提交一次后旧 redo 分支清除",
    }
    write_json(Path(evidence) / "redo-gate-check.json", result)
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

    # 建立可撤销提交：组合 ni 并空格确认首候选。
    clear_feedback_and_open_block(driver, body_point, "点击标题行打开块编辑")
    driver.type_keys(["n", "i"], "拼音组合 ni（建立历史）", pause=0.35)
    time.sleep(0.5)
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(1.2)
    driver.snapshot()

    # 撤销该提交，留下 redo 分支；命令会关闭块，随后重新打开。
    driver.call(["hotkey", "--key", "cmd+z"], "撤销提交，形成 redo 分支")
    time.sleep(0.9)
    driver.snapshot()
    notes["redo_branch_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新打开块（redo 分支保留）")

    # 组合进行中执行 Redo：必须被拒且零副作用，并显示组合待定反馈。
    driver.type_keys(["n", "i"], "组合期拼音 ni（redo 分支存在）", pause=0.35)
    time.sleep(0.5)
    driver.snapshot()
    notes["redo_click"] = driver.serial + 1
    click_edit_menu(driver, REDO_ITEM, "组合期经「编辑 → 重做」（应拒绝）")
    time.sleep(0.9)
    snapshot = driver.snapshot()
    feedback_visible = COMPOSITION_FEEDBACK in driver.tree_text(snapshot)
    write_json(driver.evidence / "redo-gate-proof.json", {
        "menu": EDIT_MENU,
        "item": REDO_ITEM,
        "feedback": COMPOSITION_FEEDBACK,
        "feedback_in_tree": feedback_visible,
        "note": "AX 树仅暴露窗口框架；GPUI 自绘反馈以点击后的 step 截图为准",
    })
    if not feedback_visible:
        print("提示：AX 树仅暴露窗口框架，GPUI 自绘反馈以点击后的 step 截图为准",
              file=sys.stderr)

    # 空格提交组合：文本增长一次、redo 分支清除。
    notes["commit_start"] = driver.serial + 1
    driver.call(["press-key", "--key", "space"], "空格确认组合候选（清除 redo）")
    time.sleep(2.2)
    driver.snapshot()
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    notes["saved"] = post_save
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "autosave_changed_disk": auto_save["sha256"] != before["sha256"],
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "组合提交可能先由自动保存写盘；显式保存补足未落盘内容",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("组合提交后磁盘仍是基线（提交未落盘）")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    verify_redo_gate(driver.evidence, baseline_bytes)
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
    print("组合期 Redo 门控取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
