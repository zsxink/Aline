#!/usr/bin/env python3
"""M2 交互矩阵真实窗口取证：组合期拒绝矩阵 + 查找面板逐项跳转。

场景 R（组合期拒绝与平台自然结束）：在真实中文组合进行中先用无障碍按压
切源码模式、再按撤销——两者都必须被拒且组合、源码、选区、历史不变。
随后用真实鼠标点击语法块行：macOS 输入上下文在按下时原样提交预编辑，应用
必须先提交「ni」一次；点击后块若仍打开，后续 h/a/o 只能形成新一轮组合，
不得复用或丢弃旧草稿。

场景 F（查找面板）：真实输入查找词后逐项「下一项/上一项」跳转，记录当前
项与计数显示；替换当前后旧结果按新文档刷新（计数变化），关闭面板后撤销
回基线。

只经 macOS 无障碍接口投递真实输入；应用侧由 `ALINE_M2_EVIDENCE_DIR`
采样的 session.tsv 记录每次回调的源码长度、选区、组合、版本与历史。
关闭后断言拒绝段零变化、真实点击自然提交恰好一次、撤销回基线。
用法：drive_m2_interaction_matrix.py --app-pid <pid> --fixture <路径> --evidence <证据目录>
"""
import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    FIND_KEY,
    ROOT,
    UNDO_KEY,
    WindowDriver,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

# 组合期真实点击的命中点：基线夹具语法块首行（窗口坐标）。
# 真实鼠标按下会先进入输入上下文，输入法据此自然提交预编辑。
OTHER_BLOCK_X = 440.0
OTHER_BLOCK_Y = 484.0
EXPECTED_CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
                        "stored-after.bin", "disk-after.bin")


def _session_table(evidence):
    """读应用侧采样表，返回 (列名索引, 行)。"""
    path = evidence / "session.tsv"
    if not path.is_file():
        raise DriverError("缺少应用侧采样表 session.tsv")
    lines = path.read_text(encoding="utf-8").splitlines()
    if len(lines) < 2:
        raise DriverError("session.tsv 没有数据行")
    header = lines[0].split("\t")
    columns = {name: index for index, name in enumerate(header)}
    rows = []
    for line in lines[1:]:
        if line.startswith("truncated="):
            # 采样器在文件末尾追加的完整性标记，不是数据行。
            if line.split("=", 1)[1] == "true":
                raise DriverError("session.tsv 已达记录上限被截断，证据不完整")
            continue
        fields = line.split("\t")
        if len(fields) != len(header):
            raise DriverError("session.tsv 列数与表头不一致")
        rows.append(fields)
    return columns, rows


def assert_composition_lifecycle(evidence, fixture):
    """核对组合期拒绝与平台自然结束的真实时序（关闭后采样已落盘）。

    实测时序（macOS 27.2 / gpui-pre-macos 0.3.9 `window.rs:2883-2888`）：
    真实鼠标按下会先交给输入上下文，微信输入法在按下时把 marked 预编辑
    「ni」原样自然提交为文本；GPUI 的 `prevent_default` 只能拦内部焦点转移，
    拦不住这次平台提交。点击后块可能保持打开也可能结束，后续 h/a/o 因而
    可能是新一轮组合。W5 任务 3.2 的「旧目标拒绝无丢字/重复提交」断言为：

    1. 无障碍按压（AXPress）切源码模式与组合期撤销被拒后，采样表在
       「ni」与点击提交之间没有任何状态变化行；
    2. 真实点击先原样提交「ni」一次（440→442，版本 1、撤销深度 1）；
    3. 若点击后块仍打开，h/a/o 只能形成一次新的「hao→好」提交
       （442→445，版本 2）；绝不许可「ni」被丢弃、只提交「好」（旧缺陷）；
    4. 撤销链回到 440 字节基线，final.tsv 同为基线。
    """
    baseline = fixture.read_bytes()
    ni_commit = baseline[:8] + b"ni" + baseline[8:]
    late_commit = baseline[:8] + b"ni" + "好".encode("utf-8") + baseline[8:]
    allowed_lengths = {len(baseline), len(ni_commit), len(late_commit)}
    columns, rows = _session_table(evidence)

    def field(row, name):
        return row[columns[name]]

    ni_seq = next((int(field(row, "seq")) for row in rows
                   if field(row, "composition_hex") == "6e69"), None)
    if ni_seq is None:
        raise DriverError("采样表缺少拼音「ni」的组合态记录")
    commit_seq = next(
        (int(field(row, "seq")) for row in rows
         if field(row, "text_hex") == ni_commit.hex()), None)
    if commit_seq is None:
        raise DriverError(
            "采样表缺少真实点击后「ni」原样自然提交的 %d 字节状态"
            "（预编辑被丢弃？）" % len(ni_commit))

    # 拒绝零变化：AXPress 与组合期撤销不得在「ni」与点击提交之间留下
    # 任何采样行；且提交前源码只能是基线。
    for row in rows:
        seq = int(field(row, "seq"))
        if ni_seq < seq < commit_seq:
            raise DriverError(
                "真实点击之前组合态被中间动作改写（seq %d）" % seq)
        if seq < commit_seq and int(field(row, "text_len")) != len(baseline):
            raise DriverError(
                "「ni」自然提交之前源码已变化（seq %d）" % seq)

    commit_row = next(row for row in rows
                      if int(field(row, "seq")) == commit_seq)
    if field(commit_row, "composition_hex"):
        raise DriverError("自然提交行仍带组合草稿："
                          + field(commit_row, "composition_hex"))
    if field(commit_row, "mode") != "visual":
        raise DriverError("自然提交改变了编辑模式：" + field(commit_row, "mode"))
    if int(field(commit_row, "revision")) != 1:
        raise DriverError("自然提交的版本号不是 1："
                          + field(commit_row, "revision"))
    if int(field(commit_row, "undo_depth")) != 1:
        raise DriverError("自然提交的撤销深度不是 1："
                          + field(commit_row, "undo_depth"))

    # 撤销链把提交撤回基线（版本增长、撤销深度归零）。此前的文本状态必须
    # 是基线 / 提交「ni」/ 新提交「hao→好」之一；此后的查找替换场景是独立
    # 阶段，只约束长度。
    baseline_returned = [int(field(row, "seq")) for row in rows
                         if int(field(row, "seq")) > commit_seq
                         and int(field(row, "text_len")) == len(baseline)
                         and int(field(row, "revision")) >= 2
                         and field(row, "undo_depth") == "0"]
    if not baseline_returned:
        raise DriverError("采样表缺少撤销回基线的状态（440 字节、版本≥2）")
    baseline_returned_seq = min(baseline_returned)

    expected_text = {len(baseline): baseline.hex(),
                     len(ni_commit): ni_commit.hex(),
                     len(late_commit): late_commit.hex()}
    unexpected = []
    for row in rows:
        seq = int(field(row, "seq"))
        length = int(field(row, "text_len"))
        if length not in allowed_lengths:
            unexpected.append((seq, length))
        elif seq <= baseline_returned_seq and field(row, "text_hex") != expected_text[length]:
            unexpected.append((seq, length))
    if unexpected:
        raise DriverError("采样表出现预期之外的源码状态："
                          + json.dumps(unexpected[:6]))

    # 旧缺陷判别：点击提交必须发生在任何「hao」新组合之前。
    early_recompose = [int(field(row, "seq")) for row in rows
                       if int(field(row, "seq")) < commit_seq
                       and field(row, "composition_hex").startswith("68")]
    if early_recompose:
        raise DriverError("「ni」提交之前就出现了从零重组的组合："
                          + json.dumps(early_recompose[:6]))

    block_stayed_open = field(commit_row, "open_block") == "true"
    late_commit_seen = any(field(row, "text_hex") == late_commit.hex()
                           for row in rows)

    final_fields = {}
    final_path = evidence / "final.tsv"
    if not final_path.is_file():
        raise DriverError("缺少关闭态 final.tsv")
    for part in final_path.read_text(encoding="utf-8").split():
        if "=" in part:
            key, value = part.split("=", 1)
            final_fields[key] = value
    if final_fields.get("source_len") != str(len(baseline)):
        raise DriverError("最终源码长度不是基线 %d 字节：%s"
                          % (len(baseline), final_fields.get("source_len")))
    write_json(evidence / "composition-lifecycle.json", {
        "platform_note": "真实鼠标按下先交给输入上下文，输入法原样提交 marked "
                         "预编辑；GPUI prevent_default 拦不住这次平台提交",
        "ni_seq": ni_seq,
        "commit_seq": commit_seq,
        "commit_text_hex": ni_commit.hex(),
        "commit_offset": 8,
        "block_stayed_open": block_stayed_open,
        "late_commit_seen": late_commit_seen,
        "allowed_text_len": sorted(allowed_lengths),
        "final_source_len": final_fields.get("source_len"),
        "note": "AXPress/撤销拒绝保留组合；真实点击先提交「ni」一次；"
                "块若仍打开则最多再有一次新组合提交；撤销回基线",
    })


def close_document_and_wait(driver):
    """关闭文档并等应用侧采样文件落盘；窗口最后才关。"""
    driver.call(["hotkey", "--key", "cmd+w"], "关闭文档并写证据",
                allow_missing_window=True)
    deadline = time.monotonic() + 12.0
    while True:
        present = {name: (driver.evidence / name).is_file()
                   for name in EXPECTED_CLOSE_FILES}
        if all(present.values()) or time.monotonic() >= deadline:
            break
        time.sleep(0.5)
    try:
        driver.snapshot()
    except DriverError as error:
        write_text(driver.evidence / "close-state.txt", str(error)[:600])
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    driver.close_window()
    write_json(driver.evidence / "close-proof.json", {
        "expected": list(EXPECTED_CLOSE_FILES),
        "present": present,
        "note": "应用侧采样器在文档关闭路径落盘的文件（缺一即判本次运行失败）",
    })
    if not all(present.values()):
        raise DriverError("关闭文档后应用侧证据文件缺失：" + json.dumps(present))


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    driver.open_document(fixture)
    driver.snapshot()
    notes = {"refusal_start": driver.serial + 1}

    # 场景 R：组合期拒绝。真实拼音组合进入 marked 状态后，先用两路非鼠标
    # 动作证明拒绝零副作用；再用真实鼠标点击语法块行，记录平台自然提交。
    driver.click_body("像素点击标题行，打开块编辑")
    driver.snapshot()
    driver.type_keys(["n", "i"], "拼音组合输入 ni", pause=0.35)
    driver.snapshot()

    snapshot = driver.snapshot()
    source_button = driver.find_button(snapshot, "源码")
    if source_button is None:
        raise DriverError("没有找到源码模式按钮")
    driver.call(["click", "--element-index", str(source_button)],
                "组合期无障碍按压源码模式（应拒绝并反馈）")
    time.sleep(0.8)
    driver.snapshot()

    driver.call(["hotkey", "--key", UNDO_KEY], "组合期撤销（应拒绝）",
                allow_missing_window=True)
    time.sleep(0.6)
    driver.snapshot()

    driver.call(["click", "--x", str(OTHER_BLOCK_X), "--y", str(OTHER_BLOCK_Y)],
                "组合期真实点击语法块行（平台自然提交预编辑）")
    time.sleep(0.8)
    driver.snapshot()

    # 平台已在点击时提交预编辑；块可能仍打开，继续按键只能是新一轮组合，
    # 不得复用旧 marked 草稿。
    driver.type_keys(["h", "a", "o"], "点击后继续输入 hao（新一轮组合）",
                     pause=0.35)
    driver.snapshot()
    driver.call(["press-key", "--key", "space"], "空格（新一轮组合则确认候选）")
    time.sleep(0.8)
    driver.snapshot()
    notes["refusal_end"] = driver.serial

    # 离开块并撤销本次自然提交，回到基线。
    driver.call(["press-key", "--key", "Escape"], "Escape 离开块",
                allow_missing_window=True)
    time.sleep(0.5)
    for _ in range(2):
        driver.call(["hotkey", "--key", UNDO_KEY], "撤销自然提交回基线",
                    allow_missing_window=True)
        time.sleep(0.3)
    time.sleep(2.0)
    driver.snapshot()

    # 场景 F：查找面板逐项跳转与计数。基线夹具「段」有 3 个匹配。
    notes["find_start"] = driver.serial + 1
    driver.call(["hotkey", "--key", FIND_KEY], "打开查找面板")
    time.sleep(0.8)
    driver.call(["paste-text", "--text", "段"], "粘贴查找词「段」")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Return"], "跳到首个匹配")
    time.sleep(0.6)
    snapshot = driver.snapshot()
    indices = driver.find_panel_indices(snapshot)
    for label in ("下一项", "下一项", "上一项", "上一项"):
        key = "next" if label == "下一项" else "previous"
        if indices[key] is None:
            raise DriverError("查找面板缺少导航按钮：" + label)
        driver.call(["click", "--element-index", str(indices[key])],
                    "逐项跳转：" + label)
        time.sleep(0.7)
        driver.snapshot()

    # 替换当前：文档变化后旧结果按新源码刷新，计数应随之改变。
    if indices["replacement"] is None or indices["replace_current"] is None:
        raise DriverError("查找面板缺少替换输入框或替换按钮")
    driver.call(["click", "--element-index", str(indices["replacement"])],
                "聚焦替换输入框")
    time.sleep(0.3)
    driver.call(["paste-text", "--text", "节"], "粘贴替换词「节」")
    time.sleep(0.4)
    driver.call(["click", "--element-index", str(indices["replace_current"])],
                "替换当前匹配")
    time.sleep(0.8)
    driver.snapshot()

    # 关闭面板后撤销替换，回到基线；等待自动保存把基线写回磁盘。
    snapshot = driver.snapshot()
    indices = driver.find_panel_indices(snapshot)
    if indices["close"] is None:
        raise DriverError("查找面板缺少关闭按钮")
    driver.call(["click", "--element-index", str(indices["close"])], "关闭查找面板")
    time.sleep(0.6)
    snapshot = driver.snapshot()
    if "查找" in driver.tree_text(snapshot):
        raise DriverError("查找面板未关闭")
    driver.click_body("点击正文恢复文档焦点")
    for _ in range(3):
        driver.call(["hotkey", "--key", UNDO_KEY], "撤销替换回基线",
                    allow_missing_window=True)
        time.sleep(0.3)
    time.sleep(2.0)
    driver.snapshot()
    notes["find_end"] = driver.serial
    write_json(driver.evidence / "scenario-notes.json", notes)

    close_document_and_wait(driver)
    assert_composition_lifecycle(driver.evidence, fixture)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app-pid", type=int, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    driver = WindowDriver(args.app_pid, evidence)
    try:
        run(driver, fixture.resolve())
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:2000])
        print("驱动失败：" + str(error)[:800], file=sys.stderr)
        return 1
    print("交互矩阵完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
