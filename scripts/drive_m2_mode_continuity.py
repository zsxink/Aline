#!/usr/bin/env python3
"""M2 3.1 真实窗口回归：开块输入后切模式，直接继续输入并关闭。

调用前启动带 ALINE_M2_EVIDENCE_DIR 的原生应用，并打开合成 Markdown 夹具。
本驱动只投递窗口事件；关闭后的应用采样独立核对文本、选区和共享历史。
不覆盖真实 IME、菜单借焦、文件故障或跨平台验收。
"""
import argparse
from pathlib import Path
import re
import sys
import tempfile
import time

import drive_m2_editable_window as native
from evidence_paths import write_json, write_text
from validation_output import require_validation_path

CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin", "stored-after.bin", "disk-after.bin")


def mode_button(driver, label):
    snapshot = driver.snapshot()
    button = driver.find_button(snapshot, label)
    if button is None:
        raise native.DriverError("找不到模式按钮：" + label)
    driver.call(["click", "--element-index", str(button)], "切模式：" + label)
    driver.snapshot()


def require_fresh(evidence):
    for name in CLOSE_FILES:
        if (evidence / name).exists():
            raise native.DriverError("证据目录含旧关闭产物，必须使用新运行目录：" + name)


def verify(evidence, baseline, live_disk, query=None):
    rows = native.session_table(evidence)
    opened = next((row for row in rows if row["open_block"] == "true"), None)
    if opened is None:
        raise native.DriverError("真实点击没有打开块")
    anchor = int(opened["selection_anchor"])
    head = int(opened["selection_head"])
    start, end = min(anchor, head), max(anchor, head)
    if query is None and start != end:
        raise native.DriverError("初始点击不是折叠选区")
    if query is not None:
        query_bytes = query.encode("utf-8")
        if baseline.count(query_bytes) != 1 or baseline[start:end] != query_bytes:
            raise native.DriverError("初始打开块未定位唯一合成查找词")
    expected = baseline[:start] + b"ABC" + baseline[end:]
    source = (evidence / "source-after.bin").read_bytes()
    stored = (evidence / "stored-after.bin").read_bytes()
    disk = (evidence / "disk-after.bin").read_bytes()
    if source != expected or stored != expected or disk != expected or live_disk != expected:
        raise native.DriverError("切模式后输入丢失或定位错误，源码/存储/磁盘不符预期")
    after_a = (baseline[:start] + b"A" + baseline[end:]).hex()
    source_entry = next((row for row in rows
                         if row["mode"] == "source" and row["text_hex"] == after_a), None)
    if source_entry is None:
        raise native.DriverError("切到源码时没有保留可视化输入")
    if source_entry["selection_anchor"] != str(start + 1):
        raise native.DriverError("切到源码没有恢复源码选区")
    if not any(row["mode"] == "source" and row["draft_hex"] == "42" for row in rows):
        raise native.DriverError("源码模式未接收直接输入 B")
    if not any(row["mode"] == "visual" and row["draft_hex"] == "43" for row in rows):
        raise native.DriverError("切回可视化后未接收直接输入 C")
    if rows[-1]["undo_depth"] != "3" or rows[-1]["history_nodes"] != "3":
        raise native.DriverError("三段输入未形成三步共享历史，切模式可能新增空步")
    write_json(evidence / "mode-continuity.json", {
        "passed": True, "anchor": anchor, "expected_hex": expected.hex(),
        "source_equals_stored_equals_disk": True, "undo_depth": 3,
        "note": "A→Source直接B→Visual直接C→Escape；模式切换不额外记步",
    })


def self_test():
    # 合成表只验证取证器的失败关闭，不作为原生窗口证据。
    baseline = b"# x\n"
    expected = b"# ABCx\n"
    columns = ("seq", "mode", "open_block", "selection_anchor", "selection_head",
               "text_hex", "draft_hex", "undo_depth", "history_nodes")
    rows = [
        ["0", "visual", "true", "2", "2", baseline.hex(), "", "0", "0"],
        ["1", "source", "false", "3", "3", b"# Ax\n".hex(), "", "1", "1"],
        ["2", "source", "true", "4", "4", b"# Ax\n".hex(), "42", "1", "1"],
        ["3", "visual", "true", "5", "5", b"# ABx\n".hex(), "43", "2", "2"],
        ["4", "visual", "false", "5", "5", expected.hex(), "", "3", "3"],
    ]
    with tempfile.TemporaryDirectory(dir=native.ROOT / "logs/validation",
                                     prefix="mode-continuity-self-test-") as temporary:
        evidence = Path(temporary)
        require_fresh(evidence)

        def install(records):
            (evidence / "session.tsv").write_text("\t".join(columns) + "\n" + "\n".join(
                "\t".join(row) for row in records) + "\ntruncated=false\n")
            for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
                (evidence / name).write_bytes(expected)

        def rejected(operation):
            try:
                operation()
            except (native.DriverError, OSError):
                return
            raise native.DriverError("最小违规反例被错误判为通过")

        install(rows)
        verify(evidence, baseline, expected)
        rejected(lambda: require_fresh(evidence))
        rejected(lambda: verify(evidence, baseline, baseline))
        install([row for row in rows if row[6] != "42"])
        rejected(lambda: verify(evidence, baseline, expected))
        broken = [row.copy() for row in rows]
        broken[1][3] = "2"
        install(broken)
        rejected(lambda: verify(evidence, baseline, expected))
        broken = [row.copy() for row in rows]
        broken[-1][-1] = "4"
        install(broken)
        rejected(lambda: verify(evidence, baseline, expected))
        (evidence / "source-after.bin").unlink()
        rejected(lambda: verify(evidence, baseline, expected))
    print("模式连续性取证器：1 正例、6 最小违规反例通过（合成，非平台证据）")


def run(driver, fixture, query=None):
    require_fresh(driver.evidence)
    baseline = fixture.read_bytes()
    if b"ABC" in baseline:
        raise native.DriverError("合成夹具不能预先含有 ABC")
    write_text(driver.evidence / "fixture-before.md", baseline.decode("utf-8"))
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    snapshot = driver.snapshot()
    if snapshot["window"]["title"] != fixture.name:
        raise native.DriverError("请先在目标原生窗口打开合成夹具")
    if query is None:
        point = driver.heading_point()
        driver.call(["click", "--x", str(round(point[0], 1)),
                     "--y", str(round(point[1], 1))], "真实点击标题打开块")
    else:
        if baseline.count(query.encode("utf-8")) != 1:
            raise native.DriverError("合成定位词必须只有一个源码匹配")
        mode_button(driver, "源码")
        driver.call(["hotkey", "--key", "cmd+f"], "打开查找以定位多行块")
        query_field = driver.find_panel_indices(driver.snapshot())["query"]
        if query_field is None:
            raise native.DriverError("没有找到查找输入框")
        driver.call(["click", "--element-index", str(query_field)], "明确聚焦查找框")
        driver.call(["paste-text", "--text", query], "只向查找框输入合成定位词")
        driver.call(["press-key", "--key", "Return"], "定位唯一源码匹配")
        driver.snapshot()
        driver.call(["press-key", "--key", "Escape"], "关闭查找并保留源码选区")
        mode_button(driver, "可视化")
    driver.snapshot()
    driver.call(["type-text", "--text", "A"], "可视化输入 A")
    driver.snapshot()
    mode_button(driver, "源码")
    driver.call(["type-text", "--text", "B"], "切模式后直接输入 B，不重新点击正文")
    driver.snapshot()
    mode_button(driver, "可视化")
    driver.call(["type-text", "--text", "C"], "切回后直接输入 C，不重新点击正文")
    driver.snapshot()
    driver.call(["press-key", "--key", "Escape"], "非组合期封口且保留 C")
    driver.snapshot()
    driver.call(["hotkey", "--key", "cmd+s"], "显式保存当前源码")
    # 原生文本接收器不暴露正文 AX 值；等待磁盘事实，不能把命令投递当保存成功。
    disk_before_close = fixture.read_bytes()
    deadline = time.monotonic() + 10
    while b"ABC" not in disk_before_close:
        if time.monotonic() >= deadline:
            break
        driver.snapshot()
        disk_before_close = fixture.read_bytes()
    write_json(driver.evidence / "save-observation.json", {
        "contains_three_inputs": b"ABC" in disk_before_close,
        "disk_hex": disk_before_close.hex(),
    })
    driver.call(["hotkey", "--key", "cmd+w"], "关闭文档并落盘应用采样")
    snapshot = driver.snapshot()
    if snapshot["window"]["title"] != "Aline":
        raise native.DriverError("文档未关闭")
    deadline = time.monotonic() + 10
    while not all((driver.evidence / name).is_file() for name in CLOSE_FILES):
        if time.monotonic() >= deadline:
            raise native.DriverError("本次关闭采样产物未齐备")
        driver.snapshot()
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    if b"ABC" not in disk_before_close:
        raise native.DriverError("保存未在期限内写入三段输入，本次运行失败")
    verify(driver.evidence, baseline, fixture.read_bytes(), query)
    match = re.search(r"^\s*(\d+) close button", driver.tree_text(snapshot), re.M)
    if match:
        try:
            driver.call(["click", "--element-index", match.group(1)], "关闭空窗口",
                        allow_missing_window=True)
        except native.DriverError as error:
            write_text(driver.evidence / "window-close-observation.txt", str(error))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app-pid", type=int)
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--query", help="多行夹具的唯一合成定位词")
    parser.add_argument("--foreground", action="store_true", help="经用户同意后将指针操作置前")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.app_pid is None or args.fixture is None or args.evidence is None:
        parser.error("真实运行需要 --app-pid、--fixture 和 --evidence")
    native.ORCA = "orca"
    evidence = require_validation_path(args.evidence)
    fixture = args.fixture.resolve()
    driver = native.WindowDriver(args.app_pid, evidence)
    if args.foreground:
        original_call = driver.call

        def pointer_call(event_args, note="", allow_missing_window=False):
            flags = ["--restore-window"] if event_args[0] in ("click", "scroll") else []
            return original_call(event_args + flags, note, allow_missing_window)

        driver.call = pointer_call
    previous = list(evidence.glob("step-*.command.json"))
    if previous:
        driver.serial = max(int(path.name.split("-")[1].split(".")[0]) for path in previous)
    try:
        run(driver, fixture, args.query)
    except (native.DriverError, OSError, ValueError, KeyError) as error:
        write_text(evidence / "driver-error.txt", str(error))
        print("模式连续性回归失败：" + str(error), file=sys.stderr)
        return 1
    print("模式连续性真实窗口回归通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
