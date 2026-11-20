#!/usr/bin/env python3
"""M2 5.2 真实窗口回归：查找面板两模式共享结果、大小写开关与全部替换。

调用前启动带 ALINE_M2_EVIDENCE_DIR 的原生应用，并打开合成 Markdown 夹具。
场景：打开查找并输入「ABC」→ 回车跳到首个匹配 → 切区分大小写（无匹配时
回车不移动选区）→ 切回不区分大小写再回车 → 切源码模式用同一批结果跳下一项
→ 切回可视化跳上一项 → 查找「甲」并全部替换为「乙」→ 关闭面板、单步撤销回
基线 → 关闭文档。关闭后按应用侧采样表核对模式/选区/文本/历史，截图核对面板
状态与计数。不覆盖真实 IME、英文界面截图或非 macOS 平台。
"""
import argparse
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

import drive_m2_editable_window as native
from evidence_paths import write_json, write_text
from validation_output import require_validation_path

CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin", "stored-after.bin", "disk-after.bin")
PANEL_KEYS = ("query", "replacement", "previous", "next", "case", "close",
              "replace_current", "replace_all")


def require_fresh(evidence):
    for name in CLOSE_FILES:
        if (evidence / name).exists():
            raise native.DriverError("证据目录含旧关闭产物，必须使用新运行目录：" + name)


def panel_indices(driver):
    """按最新快照重读面板元素下标；模式切换后下标可能变化，不能复用旧值。"""
    indices = driver.find_panel_indices(driver.snapshot())
    missing = [key for key in PANEL_KEYS if indices.get(key) is None]
    if missing:
        raise native.DriverError("查找面板缺少元素：" + ",".join(missing))
    return indices


def mode_button(driver, label):
    snapshot = driver.snapshot()
    button = driver.find_button(snapshot, label)
    if button is None:
        raise native.DriverError("找不到模式按钮：" + label)
    driver.call(["click", "--element-index", str(button)], "切模式：" + label)
    driver.snapshot()


def focus_query(driver, note):
    """点过面板按钮后焦点留在按钮上，回车可能激活按钮而不是导航。

    每次需要「在查找框内回车」前都重新聚焦查询字段，保证按键进入查找
    输入协议。
    """
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["query"])], note)
    time.sleep(0.2)


def collapse(states):
    result = []
    for state in states:
        if not result or result[-1] != state:
            result.append(state)
    return result


def verify(evidence, baseline, expected_after, matches):
    rows = native.session_table(evidence)
    base_hex = baseline.hex()
    expected_hex = expected_after.hex()

    # 替换前的采样段：文本未变，折叠相邻重复状态后应恰好是
    # 初始 → 可视化首匹配 → 源码同匹配 → 源码下一项 → 可视化同下一项 → 可视化上一项。
    prefix = []
    for row in rows:
        if row["text_hex"] != base_hex:
            break
        prefix.append(row)
    if not prefix:
        raise native.DriverError("替换前没有基线文本采样")
    states = collapse([(row["mode"], row["selection_anchor"], row["selection_head"])
                       for row in prefix])
    first, second, third = matches[0], matches[1], matches[2]
    expected_states = [
        states[0],
        ("visual", str(first), str(first + 3)),
        ("visual", str(second), str(second + 3)),
        ("source", str(second), str(second + 3)),
        ("source", str(third), str(third + 3)),
        ("visual", str(third), str(third + 3)),
        ("visual", str(second), str(second + 3)),
    ]
    if states != expected_states:
        raise native.DriverError(
            "模式/选区序列不符：实际 " + repr(states) + " 预期 " + repr(expected_states))

    # 全部替换：一次原子发布、单步历史，撤销后回到基线文本。
    changed = [row for row in rows if row["text_hex"] == expected_hex]
    if not changed:
        raise native.DriverError("采样中没有出现全部替换后的文本")
    if changed[0]["mode"] != "visual":
        raise native.DriverError("全部替换不是发生在可视化模式")
    before_depth = int(prefix[-1]["undo_depth"])
    if int(changed[0]["undo_depth"]) != before_depth + 1:
        raise native.DriverError("全部替换未形成单步历史")
    seen = {row["text_hex"] for row in rows}
    if not seen <= {base_hex, expected_hex}:
        raise native.DriverError("采样中出现预期之外的文本状态")
    if rows[-1]["text_hex"] != base_hex or int(rows[-1]["undo_depth"]) != before_depth:
        raise native.DriverError("撤销没有回到基线文本与撤销深度")

    source = (evidence / "source-after.bin").read_bytes()
    stored = (evidence / "stored-after.bin").read_bytes()
    disk = (evidence / "disk-after.bin").read_bytes()
    if source != baseline or stored != baseline or disk != baseline:
        raise native.DriverError("关闭后源码/存储/磁盘不是基线字节")
    write_json(evidence / "find-modes.json", {
        "passed": True,
        "states": [list(state) for state in states],
        "match_offsets": matches[:2],
        "replace_before_depth": before_depth,
        "replace_after_depth": before_depth + 1,
        "note": "Enter/大小写开关/两模式跳转共享同一批结果；全部替换单步可撤销",
    })


def self_test():
    # 合成表只验证取证器的失败关闭，不作为原生窗口证据。
    baseline = b"x abc abc abc " + "甲乙".encode() + b"\n"
    matches = [i for i in range(len(baseline) - 2) if baseline[i:i + 3] == b"abc"]
    expected_after = baseline.replace("甲".encode(), "乙".encode())
    columns = ("seq", "mode", "text_hex", "selection_anchor", "selection_head", "undo_depth")
    first, second, third = matches[0], matches[1], matches[2]
    base_hex = baseline.hex()
    rows = [
        ["0", "visual", base_hex, "0", "0", "0"],
        ["1", "visual", base_hex, str(first), str(first + 3), "0"],
        ["2", "visual", base_hex, str(second), str(second + 3), "0"],
        ["3", "source", base_hex, str(second), str(second + 3), "0"],
        ["4", "source", base_hex, str(third), str(third + 3), "0"],
        ["5", "visual", base_hex, str(third), str(third + 3), "0"],
        ["6", "visual", base_hex, str(second), str(second + 3), "0"],
        ["7", "visual", expected_after.hex(), str(second), str(second + 3), "1"],
        ["8", "visual", base_hex, str(second), str(second + 3), "0"],
    ]
    with tempfile.TemporaryDirectory(dir=native.ROOT / "logs/validation",
                                     prefix="find-modes-self-test-") as temporary:
        evidence = Path(temporary)
        require_fresh(evidence)

        def install(records):
            (evidence / "session.tsv").write_text("\t".join(columns) + "\n" + "\n".join(
                "\t".join(row) for row in records) + "\ntruncated=false\n")
            for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
                (evidence / name).write_bytes(baseline)

        def rejected(operation):
            try:
                operation()
            except (native.DriverError, OSError):
                return
            raise native.DriverError("最小违规反例被错误判为通过")

        install(rows)
        verify(evidence, baseline, expected_after, matches)
        rejected(lambda: require_fresh(evidence))
        rejected(lambda: verify(evidence, baseline, baseline, matches))
        broken = [row.copy() for row in rows]
        broken[2][3] = str(first + 1)
        broken[2][4] = str(first + 4)
        install(broken)
        rejected(lambda: verify(evidence, baseline, expected_after, matches))
        broken = [row.copy() for row in rows]
        broken[3][3] = str(first)
        broken[3][4] = str(first + 3)
        install(broken)
        rejected(lambda: verify(evidence, baseline, expected_after, matches))
        broken = [row.copy() for row in rows]
        broken[4][3] = str(second)
        broken[4][4] = str(second + 3)
        install(broken)
        rejected(lambda: verify(evidence, baseline, expected_after, matches))
        broken = [row.copy() for row in rows]
        broken[6][5] = "2"
        install(broken)
        rejected(lambda: verify(evidence, baseline, expected_after, matches))
        install(rows)
        (evidence / "disk-after.bin").write_bytes(expected_after)
        rejected(lambda: verify(evidence, baseline, expected_after, matches))
    print("查找两模式取证器：1 正例、7 最小违规反例通过（合成，非平台证据）")


def activate(driver):
    """把 aline 置前：热键与粘贴由系统按前台应用投递，不置前会落到终端。"""
    driver.serial += 1
    started = time.time_ns()
    script = 'tell application "System Events" to set frontmost of process "aline" to true'
    result = subprocess.run(["osascript", "-e", script], text=True, capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", "<System Events: frontmost aline>"],
        "note": "激活 aline 窗口",
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise native.DriverError("激活失败：" + result.stderr[-600:])
    time.sleep(1.0)


def system_events(driver, script, note):
    """文件面板是系统面板服务窗口，orca 热键到不了；按键走 System Events。"""
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e",
                             'tell application "System Events" to ' + script],
                            text=True, capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", "<System Events: " + note + ">"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise native.DriverError(note + "失败：" + result.stderr[-600:])


def open_document(driver, fixture):
    """经文件面板打开夹具：前往文件夹 → 粘贴绝对路径 → 回车 → 回车。

    面板内热键与回车由 System Events 投递（面板窗口不属于 orca 可定向的
    窗口）；不依赖面板上次停留的目录。
    """
    driver.call(["hotkey", "--key", native.OPEN_PANEL_KEY], "打开文件面板")
    time.sleep(1.2)
    system_events(driver, 'keystroke "g" using {command down, shift down}',
                  "打开前往文件夹")
    time.sleep(1.0)
    system_events(driver, 'set the clipboard to "' + str(fixture) + '"',
                  "复制夹具绝对路径到剪贴板")
    system_events(driver, 'keystroke "a" using command down', "全选路径输入框")
    system_events(driver, 'keystroke "v" using command down', "粘贴夹具绝对路径")
    time.sleep(0.8)
    system_events(driver, "key code 36", "回车确认路径")
    time.sleep(1.2)
    system_events(driver, "key code 36", "回车打开所选夹具")
    time.sleep(1.5)


def run(driver, fixture):
    require_fresh(driver.evidence)
    driver.wait_until_ready()
    activate(driver)
    baseline = fixture.read_bytes()
    matches = [i for i in range(len(baseline) - 2) if baseline[i:i + 3] == b"abc"]
    if len(matches) < 3:
        raise native.DriverError("夹具至少需要三个小写 abc 匹配")
    if b"ABC" in baseline:
        raise native.DriverError("合成夹具不能预先含有大写 ABC")
    chinese = "甲".encode()
    if baseline.count(chinese) < 1:
        raise native.DriverError("夹具缺少中文替换目标「甲」")
    expected_after = baseline.replace(chinese, "乙".encode())
    write_text(driver.evidence / "fixture-before.md", baseline.decode("utf-8"))
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    snapshot = driver.snapshot()
    if snapshot["window"]["title"] != fixture.name:
        open_document(driver, fixture)
        snapshot = driver.snapshot()
        if snapshot["window"]["title"] != fixture.name:
            raise native.DriverError("原生窗口没有打开合成夹具")

    driver.call(["hotkey", "--key", "cmd+f"], "打开查找面板")
    time.sleep(0.5)
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["query"])], "聚焦查找框")
    time.sleep(0.2)
    driver.call(["paste-text", "--text", "ABC"], "输入大小写混合查找词")
    time.sleep(0.6)
    driver.snapshot()
    driver.call(["press-key", "--key", "Return"], "回车跳到首个匹配")
    time.sleep(0.5)
    driver.snapshot()

    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["case"])], "切换为区分大小写")
    time.sleep(0.7)
    driver.snapshot()
    focus_query(driver, "聚焦查找框")
    driver.call(["press-key", "--key", "Return"], "无匹配时回车不移动选区")
    time.sleep(0.5)
    driver.snapshot()

    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["case"])], "切回不区分大小写")
    time.sleep(0.7)
    driver.snapshot()
    focus_query(driver, "聚焦查找框")
    driver.call(["press-key", "--key", "Return"], "回车再次跳到首个匹配")
    time.sleep(0.5)
    driver.snapshot()

    # 大小写切换会丢弃 current；上面的回车把它重新落在首个匹配，再点
    # 「下一项」才应看到选区前移。此时已在可视化模式，不再点模式按钮
    # （无变化的点击会抢走焦点并触发一次刷新）。
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["next"])], "可视化模式下跳到下一项")
    time.sleep(0.6)
    driver.snapshot()
    mode_button(driver, "源码")
    # 模式切换会刷新查找快照；结果为异步任务，等就绪后再导航，避免把
    # 「尚未就绪」误判成共享结果失效。
    time.sleep(2.5)
    driver.snapshot()
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["next"])], "源码模式下跳到下一项")
    time.sleep(0.8)
    driver.snapshot()
    mode_button(driver, "可视化")
    time.sleep(2.5)
    driver.snapshot()
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["previous"])], "可视化模式下跳回上一项")
    time.sleep(0.6)
    driver.snapshot()

    # 全部替换：清空查找词后改为「甲」，替换词「乙」。
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["query"])], "聚焦查找框")
    time.sleep(0.2)
    for _ in range(3):
        driver.call(["press-key", "--key", "Backspace"], "清空查找词：Backspace")
        time.sleep(0.15)
    driver.call(["paste-text", "--text", "甲"], "输入多匹配中文查找词")
    time.sleep(0.7)
    driver.snapshot()
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["replacement"])], "聚焦替换框")
    time.sleep(0.2)
    driver.call(["paste-text", "--text", "乙"], "输入替换词")
    time.sleep(0.5)
    driver.snapshot()
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["replace_all"])], "全部替换")
    time.sleep(0.9)
    driver.snapshot()

    # 点过替换按钮后焦点已回到窗口根，Escape 不再关闭面板，必须点面板关闭按钮。
    indices = panel_indices(driver)
    driver.call(["click", "--element-index", str(indices["close"])], "关闭查找面板")
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["hotkey", "--key", "cmd+z"], "撤销全部替换")
    time.sleep(0.7)
    driver.snapshot()
    driver.call(["hotkey", "--key", "cmd+w"], "关闭文档并落盘采样")
    snapshot = driver.snapshot()
    if snapshot["window"]["title"] != "Aline":
        raise native.DriverError("文档未关闭")
    deadline = time.monotonic() + 12.0
    while not all((driver.evidence / name).is_file() for name in CLOSE_FILES):
        if time.monotonic() >= deadline:
            raise native.DriverError("本次关闭采样产物未齐备")
        time.sleep(0.4)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    verify(driver.evidence, baseline, expected_after, matches)
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
    previous = list(evidence.glob("step-*.command.json"))
    if previous:
        driver.serial = max(int(path.name.split("-")[1].split(".")[0]) for path in previous)
    try:
        run(driver, fixture)
    except (native.DriverError, OSError, ValueError, KeyError) as error:
        write_text(evidence / "driver-error.txt", str(error))
        print("查找两模式回归失败：" + str(error), file=sys.stderr)
        return 1
    print("查找两模式真实窗口回归通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
