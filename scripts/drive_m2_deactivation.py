#!/usr/bin/env python3
"""W5 3.2「窗口失活」组合生命周期真实窗口取证。

被测事实：真实中文组合进行中，窗口被其他应用（Finder）真实夺焦；应用须等待
平台自然结果，不得自行伪造结束。重新激活后原块上下文仍可用，后续输入只形成
一段连续插入，不出现重放或重复提交；随后显式保存写盘、关闭文档，关闭采样包
与磁盘目标逐字节一致，恢复存储无残留 Active 完整代。

失活/激活由 System Events 真实切换前台完成（Finder → 本实例），输入与保存
经无障碍接口投递。应用由脚本以 `ALINE_M2_EVIDENCE_DIR` 启动，采样器在文档
关闭路径落盘，脚本随后核对 session.tsv 的组合行与终态。
用法：drive_m2_deactivation.py --fixture <路径> --evidence <证据目录>
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
    fixture_state,
    verify_close_packet,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")


def frontmost_unix_id():
    result = subprocess.run(
        ["osascript", "-e",
         'tell application "System Events" to get unix id of first application '
         'process whose frontmost is true'],
        text=True, capture_output=True)
    return result.stdout.strip()


def set_frontmost(driver, name, target, note, pause):
    """经 System Events 真实切换前台；记录命令与切换后的前台 unix id。"""
    driver.serial += 1
    script = f'tell application "System Events" to set frontmost of {target} to true'
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", f"<System Events: frontmost {name}>"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise DriverError("前台切换失败（" + name + "）：" + result.stderr[-600:])
    time.sleep(pause)
    return {
        "name": name,
        "frontmost_unix_id": frontmost_unix_id(),
        "note": note,
    }


def single_insertion(baseline, final):
    """final 是否为 baseline 的单点连续插入；返回插入位置与 UTF-8 字节。

    组合提交与失活后的继续输入若落在同一插入点，应合并为一段连续插入；出现
    两段以上分离插入即说明重新激活后输入没有落回原上下文（或发生重放）。
    """
    if len(final) <= len(baseline):
        return None
    for position in range(len(baseline) + 1):
        tail = baseline[position:]
        if final.startswith(baseline[:position]) and final.endswith(tail):
            inserted = final[position:len(final) - len(tail)]
            if not inserted:
                continue
            try:
                inserted.decode("utf-8")
            except UnicodeDecodeError:
                continue
            return {"position": position, "bytes": inserted}
    return None


def repeated_unit(inserted):
    """插入段内相邻重复子串（长度 ≥2 字节），用于人工复核重复提交嫌疑。"""
    for width in range(2, len(inserted) // 2 + 1):
        for start in range(0, len(inserted) - 2 * width + 1):
            if inserted[start:start + width] == inserted[start + width:start + 2 * width]:
                return {"width": width, "position": start,
                        "unit_hex": inserted[start:start + width].hex()}
    return None


def verify_session(evidence, baseline_bytes):
    """核对采样：真实出现过组合、文本长度不回落、终态干净且与源字节一致。"""
    rows = session_table(evidence)
    composing = [row for row in rows
                 if int(row["composition_len"] or 0) > 0]
    settled = [row for row in rows
               if row["wait"] == "idle"
               and row["save"] in ("idle", "none")
               and row["draft_len"] == "0"
               and row["composition_len"] in ("", "0")]
    lengths = [int(row["text_len"]) for row in rows]
    decreases = [int(rows[index]["seq"])
                 for index in range(1, len(rows))
                 if lengths[index] < lengths[index - 1]]
    source_bytes = (Path(evidence) / "source-after.bin").read_bytes()
    last_text = bytes.fromhex(rows[-1]["text_hex"]) if rows[-1]["text_hex"] else b""
    grew_after_composition = bool(composing) and lengths[-1] > len(baseline_bytes)
    result = {
        "rows": len(rows),
        "composition_rows": [int(row["seq"]) for row in composing],
        "composition_hex": sorted({row["composition_hex"] for row in composing}),
        "length_decreases": decreases,
        "settled_rows": [int(row["seq"]) for row in settled],
        "last_text_equals_source": last_text == source_bytes,
        "text_grew_after_composition": grew_after_composition,
        "note": "真实组合应被采样；文本长度不得回落；终态应干净且末尾文本等于源字节",
    }
    write_json(Path(evidence) / "session-check.json", result)
    if not composing:
        raise DriverError("采样未记录真实组合（composition_len>0）：失活场景未成立")
    if decreases:
        raise DriverError("采样出现文本长度回落（可能有输入或提交丢失）")
    if not grew_after_composition:
        raise DriverError("组合结束后文本没有增长（提交未落地）")
    if not settled:
        raise DriverError("采样没有干净终态（wait/save idle、无草稿与组合）")
    if not result["last_text_equals_source"]:
        raise DriverError("采样末尾文本与关闭源字节不一致")
    return result


def verify_insertion(evidence, baseline_bytes):
    """最终源字节必须是基线的单点连续插入；记录重复子串供人工复核。"""
    final = (Path(evidence) / "source-after.bin").read_bytes()
    inserted = single_insertion(baseline_bytes, final)
    result = {
        "baseline": {"size": len(baseline_bytes),
                     "sha256": hashlib.sha256(baseline_bytes).hexdigest()},
        "final": {"size": len(final),
                  "sha256": hashlib.sha256(final).hexdigest()},
        "inserted": (None if inserted is None else {
            "position": inserted["position"],
            "size": len(inserted["bytes"]),
            "hex": inserted["bytes"].hex(),
            "text": inserted["bytes"].decode("utf-8"),
        }),
        "repeated_unit": (None if inserted is None
                          else repeated_unit(inserted["bytes"])),
        "note": "失活与重新激活后的输入应合并为一段连续插入；重复子串仅记录供复核",
    }
    write_json(Path(evidence) / "insertion-check.json", result)
    if inserted is None:
        raise DriverError("最终源字节不是基线的单点连续插入（上下文未保留或发生重放）")
    return result


def session_timeline(evidence):
    """把采样表压缩为状态变化序列，供逐条人工核对失活前后的组合生命周期。"""
    rows = session_table(evidence)
    timeline = []
    previous = None
    for row in rows:
        key = (row["text_len"], row["draft_len"], row["composition_start"],
               row["composition_end"], row["composition_len"], row["wait"],
               row["save"], row["open_block"], row["undo_depth"])
        if key == previous:
            continue
        previous = key
        timeline.append({name: row[name] for name in
                         ("seq", "elapsed_ms", "text_len", "draft_len",
                          "composition_start", "composition_end",
                          "composition_len", "composition_hex", "undo_depth",
                          "wait", "save", "open_block", "dirty")})
    write_json(Path(evidence) / "session-timeline.json", {
        "transitions": timeline,
        "note": "仅列状态变化行（按 text/draft/composition/wait/save/块状态的组合去重）",
    })
    return timeline


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

    # 进入块编辑并开始真实拼音组合。
    driver.call(["click", "--x", str(round(body_point[0], 1)),
                 "--y", str(round(body_point[1], 1))], "点击标题行打开块编辑")
    time.sleep(0.7)
    driver.snapshot()
    notes["composition_start"] = driver.serial + 1
    driver.type_keys(["n", "i"], "拼音组合 ni（失活前）", pause=0.35)
    time.sleep(0.4)
    driver.snapshot()

    # 失活：Finder 成为前台；组合期窗口应保持，等待平台自然结果。
    # 失活期间不调用 orca 读取窗口：run-1 失败轮记录到该读取会让 Orca 稍后
    # 抢回前台，重新激活被误判失败；组合生命周期改由关闭采样表核对。
    notes["deactivation_start"] = driver.serial + 1
    deactivate = set_frontmost(driver, "Finder", 'process "Finder"',
                               "组合进行中：Finder 成为前台（窗口失活）", 0.6)
    time.sleep(1.0)
    frontmost_during = frontmost_unix_id()
    write_json(driver.evidence / "deactivation-proof.json", {
        "deactivate": deactivate,
        "frontmost_during": frontmost_during,
        "expected_pid": driver.pid,
        "note": "Finder 前台期间不调用 orca 读取窗口，避免其稍后抢回前台",
    })
    if frontmost_during == str(driver.pid):
        raise DriverError("失活未生效：前台仍是本实例")

    # 重新激活本实例并核对前台归属；进程定位与原 activate_cycle 一致。
    notes["reactivation_start"] = driver.serial + 1
    reactivate = set_frontmost(
        driver, APP_PROCESS,
        f'(first process whose unix id is {driver.pid})',
        "重新激活被测实例", 1.0)
    write_json(driver.evidence / "reactivation-proof.json", {
        "reactivate": reactivate,
        "frontmost_after": frontmost_unix_id(),
        "expected_pid": driver.pid,
        "note": "重新激活后前台必须是本实例，否则后续输入不可归因",
    })
    if reactivate["frontmost_unix_id"] != str(driver.pid):
        raise DriverError("重新激活后前台实例应为 "
                          f"{driver.pid}，实际 {reactivate['frontmost_unix_id'] or '未取得'}")
    driver.snapshot()

    # 重新激活后继续输入并提交。失活提交可能已触发 1 秒自动保存；先等自动
    # 保存窗口，再显式保存，分别记录两次落盘事实。
    driver.type_keys(["h", "a", "o"], "重新激活后继续拼音组合 hao", pause=0.3)
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["press-key", "--key", "space"], "空格确认候选（失活后）")
    time.sleep(2.2)
    driver.snapshot()
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    notes["auto_save"] = auto_save
    notes["saved"] = post_save
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "autosave_changed_disk": auto_save["sha256"] != before["sha256"],
        "explicit_save_changed_disk": post_save["sha256"] != auto_save["sha256"],
        "note": "失活提交可能触发自动保存；显式保存补足未落盘内容",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("失活场景结束后磁盘仍是基线（提交未落盘）")

    write_json(driver.evidence / "deactivation-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    verify_session(driver.evidence, baseline_bytes)
    session_timeline(driver.evidence)
    verify_insertion(driver.evidence, baseline_bytes)
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
    print("窗口失活取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
