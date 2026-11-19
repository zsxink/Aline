#!/usr/bin/env python3
"""清理历史测试运行遗留的恢复记录（逐文档丢弃 + 二次确认，无批量入口）。

此前真实运行在关闭窗口前未等后台退役完成，`fixture.md` 的测试谱系残留在
$HOME/Library/Application Support/Aline/recovery/v1。本脚本只经应用自己的启动
恢复面板逐条「丢弃 → 确认丢弃」，并逐轮核对存储代确实减少；不直接删除文件。

用法：drive_m2_recovery_cleanup.py --evidence <证据目录>
"""
import argparse
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import DriverError, ROOT, WindowDriver  # noqa: E402
from drive_m2_recovery_crash import (  # noqa: E402
    activate,
    launch,
    panel_buttons,
    require_button,
    settle_logs,
    store_snapshot,
)
from evidence_paths import write_json  # noqa: E402
from validation_output import require_validation_path  # noqa: E402


def generation_count():
    return len(store_snapshot()["generations"])


def run(evidence):
    proc, stdout, stderr, log_dir = launch(evidence)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate(driver)
        deadline = time.monotonic() + 8.0
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        while buttons["丢弃"] is None and time.monotonic() < deadline:
            time.sleep(0.5)
            snapshot = driver.snapshot()
            buttons = panel_buttons(driver, snapshot)
        write_json(evidence / "cleanup-start.json", {
            "buttons": buttons,
            "store": store_snapshot(),
            "note": "历史测试谱系清理前的启动面板",
        })
        rounds = []
        for round_index in range(12):
            snapshot = driver.snapshot()
            discard = panel_buttons(driver, snapshot)["丢弃"]
            if discard is None:
                break
            before = generation_count()
            driver.call(["click", "--element-index", str(discard)],
                        f"第 {round_index + 1} 条丢弃（进入二次确认）")
            time.sleep(0.5)
            snapshot = driver.snapshot()
            confirm = require_button(driver, snapshot, "确认丢弃",
                                     f"第 {round_index + 1} 条二次确认")
            driver.call(["click", "--element-index", str(confirm)],
                        f"第 {round_index + 1} 条确认丢弃")
            deadline = time.monotonic() + 8.0
            after = before
            while time.monotonic() < deadline:
                after = generation_count()
                if after < before:
                    break
                time.sleep(0.4)
            rounds.append({"round": round_index + 1, "before": before,
                           "after": after, "store": store_snapshot()})
            if after >= before:
                raise DriverError(f"第 {round_index + 1} 条丢弃后存储代未减少："
                                  f"{before} -> {after}")
            time.sleep(0.3)
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        store = store_snapshot()
        write_json(evidence / "cleanup-summary.json", {
            "rounds": rounds,
            "buttons": buttons,
            "store": store,
            "note": "逐条丢弃并要求二次确认；结束后无条目、无恢复代",
        })
        if buttons["丢弃"] is not None or store["generations"]:
            raise DriverError("清理后仍存在条目或恢复代")
        driver.close_window()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.terminate()
            proc.wait(timeout=10)
        write_json(evidence / "process-exit.json", {
            "pid": proc.pid, "exit_code": proc.returncode,
        })
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        settle_logs(evidence, stdout, stderr, log_dir)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             text=True, capture_output=True)
    if running.stdout.strip():
        print("已有 aline 进程占用，拒绝干扰", file=sys.stderr)
        return 2
    try:
        run(evidence)
    except DriverError as error:
        from evidence_paths import write_text
        write_text(evidence / "driver-error.txt", str(error)[:3000])
        print("清理失败：" + str(error)[:800], file=sys.stderr)
        return 1
    print("历史恢复谱系清理完成", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
