#!/usr/bin/env python3
"""M2 3.2 删除变体真实窗口取证：选中文字删除、两步删除、Source 删除、
跨块删除与删除后保存/关闭。

调用前启动带 `ALINE_M2_EVIDENCE_DIR` 的原生应用（不带文档参数）；本驱动写入
合成夹具后经文件面板打开，按五段真实输入取证：

- 段 A（可视化）：点击标题行打开块 → Shift+Left 两次 → 退格。删除后块保持
  打开，删除字节必须等于删除前一瞬的选区，单步历史；撤销回基线。
- 段 B（可视化两步删除）：点击 → 输入 Z 形成待定草稿 → Left 移开草稿尾部 →
  退格。先封口草稿再删除，文本应为「插入点前段 + Z + 插入点后段」，历史
  深度 +2，块仍打开；两次撤销依次恢复「基线 + Z」与基线。
- 段 C（源码）：切源码模式 → 点击首行 → Shift+Left 两次 → 退格。删除字节
  等于选区，单步历史，没有打开块；撤销回基线。
- 段 D（跨块）：源码模式 Cmd+A 全选整篇再 Cmd+X 剪切（跨块选区无法由
  Shift+Left 跨越块边界，整篇全选是文档级跨块路径）。文本清空、单步历史；
  撤销恢复基线文本与 0..len 跨块选区。
- 段 E（删除后保存/关闭）：回到可视化删除选中的两个字符，删除后 0.4 秒内
  Cmd+S（早于 1 秒自动保存去抖），等磁盘从删除前摘要变化；关闭文档后核对
  源码/存储/磁盘三份字节都等于采样表推出的删除后文本。

采样器只在文档关闭路径落盘，因此全部状态断言在关闭后按 session.tsv 时间线
核对：每个段落的期望状态必须在游标之后按顺序出现，且采样中不得出现预期外
的文本状态。只经 macOS 无障碍接口投递真实输入。
用法：drive_m2_delete_variants.py --app-pid <pid> --fixture <路径> --evidence <证据目录>
"""
import argparse
from pathlib import Path
import re
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

import drive_m2_editable_window as native  # noqa: E402  复用同一真实输入驱动
from drive_m2_find_modes import (  # noqa: E402
    activate,
    mode_button,
    require_fresh,
    system_events,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from drive_m2_save_lifecycle import fixture_state, wait_disk_change  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

FIXTURE_TEXT = "# 删除变体\n\n第一段：甲乙丙丁。\n\n第二段：子丑寅卯。\n"
SAVE_KEY = "cmd+s"
DRAFT_Z_HEX = "5a"
COLUMNS = ("seq", "mode", "text_hex", "text_len", "selection_anchor",
           "selection_head", "undo_depth", "open_block", "draft_hex", "draft_len")


def _assert(condition, note):
    if not condition:
        raise native.DriverError(note)


def _find(rows, start, predicate, note):
    for index in range(start, len(rows)):
        if predicate(rows[index]):
            return index
    raise native.DriverError("采样表缺少状态：" + note)


def _last(rows, start, stop, predicate, note):
    for index in range(stop - 1, start - 1, -1):
        if predicate(rows[index]):
            return index
    raise native.DriverError("采样表缺少状态：" + note)


def _collapsed(row):
    return row["selection_anchor"] == row["selection_head"]


def _verify_selection_delete(rows, cursor, baseline, mode, pre_open, post_open, note,
                             expect_undo=True):
    """核对一次「选中文字后单步退格」：删除字节、模式、块状态、历史与撤销。"""
    base_hex = baseline.hex()
    post_index = _find(rows, cursor, lambda row: row["text_hex"] != base_hex,
                       note + "：删除后文本")
    pre_index = _last(
        rows, cursor, post_index,
        lambda row: (row["text_hex"] == base_hex and row["mode"] == mode
                     and row["open_block"] == pre_open
                     and row["selection_anchor"] != row["selection_head"]),
        note + "：删除前非空选区")
    pre, post = rows[pre_index], rows[post_index]
    start, end = sorted((int(pre["selection_anchor"]), int(pre["selection_head"])))
    expected = baseline[:start] + baseline[end:]
    _assert(post["text_hex"] == expected.hex(), note + "：删除字节不等于删除前选区")
    _assert(post["mode"] == mode, note + "：删除后模式不符")
    _assert(post["open_block"] == post_open, note + "：删除后块状态不符")
    _assert(_collapsed(post) and int(post["selection_head"]) == start,
            note + "：删除后选区未折叠到删除区间起点")
    _assert(int(post["undo_depth"]) == int(pre["undo_depth"]) + 1,
            note + "：选中文字删除不是单步历史")
    info = {
        "deleted_start": start,
        "deleted_end": end,
        "deleted_text": baseline[start:end].decode("utf-8"),
        "before_depth": int(pre["undo_depth"]),
        "after_depth": int(post["undo_depth"]),
        "open_block_after": post["open_block"],
        "mode": mode,
    }
    if not expect_undo:
        return post_index + 1, info
    undo_index = _find(rows, post_index + 1,
                       lambda row: row["text_hex"] == base_hex, note + "：撤销回基线")
    undo = rows[undo_index]
    _assert(undo["selection_anchor"] == pre["selection_anchor"]
            and undo["selection_head"] == pre["selection_head"],
            note + "：撤销未恢复删除前选区")
    _assert(int(undo["undo_depth"]) == int(pre["undo_depth"]), note + "：撤销深度不符")
    return undo_index + 1, info


def _verify_two_step_delete(rows, cursor, baseline, note):
    """核对两步删除：草稿在块内待定，封口与删除形成两条历史。"""
    base_hex = baseline.hex()
    typing_index = _find(
        rows, cursor,
        lambda row: row["text_hex"] == base_hex and row["draft_hex"] == DRAFT_Z_HEX,
        note + "：待定草稿 Z")
    typing = rows[typing_index]
    _assert(typing["open_block"] == "true", note + "：草稿不在打开块内")
    _assert(_collapsed(typing), note + "：草稿尾部选区不是折叠选区")
    insert_at = int(typing["selection_head"]) - 1
    left_index = _find(
        rows, typing_index + 1,
        lambda row: (row["text_hex"] == base_hex and row["draft_hex"] == DRAFT_Z_HEX
                     and _collapsed(row) and int(row["selection_head"]) == insert_at),
        note + "：左移后的折叠选区")
    post_index = _find(rows, left_index + 1, lambda row: row["text_hex"] != base_hex,
                       note + "：两步删除后文本")
    post = rows[post_index]
    after = int(post["selection_head"])
    _assert(_collapsed(post), note + "：两步删除后选区不是折叠选区")
    expected = baseline[:after] + b"Z" + baseline[insert_at:]
    _assert(post["text_hex"] == expected.hex(),
            note + "：文本不等于「删除插入点前一字并插入 Z」")
    _assert(after < insert_at, note + "：未删除插入点之前的字符")
    _assert(len(baseline[after:insert_at].decode("utf-8")) == 1,
            note + "：删除的不是恰好一个字符")
    _assert(int(post["undo_depth"]) == int(rows[left_index]["undo_depth"]) + 2,
            note + "：两步删除未形成两条历史")
    _assert(post["draft_len"] == "0", note + "：两步删除后仍有待定草稿")
    _assert(post["open_block"] == "true", note + "：两步删除后块被关闭")
    committed = baseline[:insert_at] + b"Z" + baseline[insert_at:]
    first_undo = _find(rows, post_index + 1,
                       lambda row: row["text_hex"] == committed.hex(),
                       note + "：撤销删除步")
    # Buffer::undo 恢复「该节点首个步骤之前的选区」：删除步发起时的光标
    # 位于插入点（左移后的折叠选区），撤销后应回到同一位置。
    _assert(_collapsed(rows[first_undo]), note + "：撤销删除步选区不是折叠选区")
    _assert(int(rows[first_undo]["selection_head"]) == insert_at,
            note + "：撤销删除步未回到删除发起时的光标")
    _assert(int(rows[first_undo]["undo_depth"]) == int(post["undo_depth"]) - 1,
            note + "：撤销删除步的深度不符")
    second_undo = _find(rows, first_undo + 1, lambda row: row["text_hex"] == base_hex,
                        note + "：再撤销输入步")
    _assert(int(rows[second_undo]["undo_depth"]) == 0, note + "：撤销回基线后深度不为 0")
    _assert(int(rows[second_undo]["selection_head"]) == insert_at,
            note + "：撤销输入步未回到插入点")
    return second_undo + 1, {
        "insert_at": insert_at,
        "deleted_start": after,
        "deleted_end": insert_at,
        "committed_text": committed.decode("utf-8"),
        "after_depth": int(post["undo_depth"]),
    }


def _verify_cross_block_cut(rows, cursor, baseline, note):
    """核对跨块删除走文档级全选/剪切：整篇清空、单步历史、撤销恢复选区。"""
    base_hex = baseline.hex()
    select_index = _find(
        rows, cursor,
        lambda row: (row["text_hex"] == base_hex and row["selection_anchor"] == "0"
                     and row["selection_head"] == str(len(baseline))),
        note + "：全选整篇")
    cut_index = _find(rows, select_index + 1, lambda row: row["text_hex"] != base_hex,
                      note + "：剪切后文本")
    cut = rows[cut_index]
    _assert(cut["text_len"] == "0" and cut["text_hex"] == "",
            note + "：跨块剪切后文本不为空")
    _assert(int(cut["undo_depth"]) == int(rows[select_index]["undo_depth"]) + 1,
            note + "：跨块剪切不是单步历史")
    _assert(cut["selection_anchor"] == "0" and cut["selection_head"] == "0",
            note + "：跨块剪切后选区未折叠到文首")
    undo_index = _find(rows, cut_index + 1, lambda row: row["text_hex"] == base_hex,
                       note + "：撤销跨块剪切")
    undo = rows[undo_index]
    _assert(undo["selection_anchor"] == "0"
            and undo["selection_head"] == str(len(baseline)),
            note + "：撤销跨块剪切未恢复整篇选区")
    _assert(int(undo["undo_depth"]) == int(rows[select_index]["undo_depth"]),
            note + "：撤销跨块剪切的深度不符")
    return undo_index + 1, {
        "cleared_after_depth": int(cut["undo_depth"]),
        "restored_selection": [0, len(baseline)],
    }


def verify(evidence, baseline):
    rows = native.session_table(evidence)
    _assert(len(rows) >= 10, "采样行数不足以覆盖五个删除段")
    for row in rows:
        if row["text_hex"] == "":
            _assert(row["text_len"] == "0", "采样记录为空但文本长度不为 0（可能截断）")
    start = _find(rows, 0, lambda row: row["text_hex"] == baseline.hex(), "基线文本")
    _assert(rows[start]["mode"] == "visual", "打开文档后不是可视化模式")
    _assert(int(rows[start]["undo_depth"]) == 0, "打开文档后历史深度不为 0")
    cursor = start + 1

    cursor, selection_a = _verify_selection_delete(
        rows, cursor, baseline, "visual", "true", "true", "段 A 选中文字删除")
    cursor, two_step = _verify_two_step_delete(rows, cursor, baseline, "段 B 两步删除")
    cursor, selection_c = _verify_selection_delete(
        rows, cursor, baseline, "source", "false", "false", "段 C 源码删除")
    cursor, cut = _verify_cross_block_cut(rows, cursor, baseline, "段 D 跨块删除")
    cursor, selection_e = _verify_selection_delete(
        rows, cursor, baseline, "visual", "true", "true", "段 E 删除后保存",
        expect_undo=False)

    expected_e = baseline[:selection_e["deleted_start"]] + baseline[selection_e["deleted_end"]:]
    source = (evidence / "source-after.bin").read_bytes()
    stored = (evidence / "stored-after.bin").read_bytes()
    disk = (evidence / "disk-after.bin").read_bytes()
    _assert(source == expected_e, "关闭后源码字节不等于段 E 删除后文本")
    _assert(stored == source, "关闭后存储字节不等于源码字节")
    _assert(disk == source, "关闭后磁盘字节不等于源码字节")
    _assert("disk_equals_source=true" in (evidence / "final.tsv").read_text(encoding="utf-8"),
            "final.tsv 未记录磁盘与源码一致")

    allowed = {
        baseline.hex(),
        (baseline[:selection_a["deleted_start"]] + baseline[selection_a["deleted_end"]:]).hex(),
        two_step["committed_text"].encode("utf-8").hex(),
        (baseline[:two_step["deleted_start"]] + b"Z"
         + baseline[two_step["deleted_end"]:]).hex(),
        (baseline[:selection_c["deleted_start"]] + baseline[selection_c["deleted_end"]:]).hex(),
        "",
        expected_e.hex(),
    }
    unexpected = {row["text_hex"] for row in rows} - allowed
    _assert(not unexpected, "采样中出现预期外的文本状态：" + repr(sorted(unexpected)))

    write_json(evidence / "delete-variants.json", {
        "passed": True,
        "selection_delete": selection_a,
        "two_step_delete": two_step,
        "source_delete": selection_c,
        "cross_block_cut": cut,
        "delete_then_save": {
            **selection_e,
            "saved_bytes": len(source),
            "disk_equals_source": True,
        },
        "note": "五个删除变体按采样时间线核对；删除字节等于删除前一瞬选区",
    })


def _record(rows, mode, text, anchor, head, depth, opened=False, draft=b""):
    rows.append({
        "seq": str(len(rows)),
        "mode": mode,
        "text_hex": text.hex(),
        "text_len": str(len(text)),
        "selection_anchor": str(anchor),
        "selection_head": str(head),
        "undo_depth": str(depth),
        "open_block": "true" if opened else "false",
        "draft_hex": draft.hex(),
        "draft_len": str(len(draft)),
    })


def self_test():
    # 合成表只验证取证器的失败关闭，不作为原生窗口证据。
    baseline = FIXTURE_TEXT.encode("utf-8")
    length = len(baseline)
    head_prefix = baseline[:2] + baseline[8:]           # 段 A/E：删「删除」两字
    committed = baseline[:5] + b"Z" + baseline[5:]      # 段 B：插入点前 + Z + 后段
    two_step_text = baseline[:2] + b"Z" + baseline[5:]  # 段 B：再删插入点前一字符
    source_text = baseline[:28] + baseline[34:]         # 段 C：源码删「甲乙」

    def rows():
        result = []
        _record(result, "visual", baseline, 0, 0, 0)
        # 段 A：8 处点击，左扩两次到 2..8，退格删除后块保持打开。
        _record(result, "visual", baseline, 8, 8, 0, True)
        _record(result, "visual", baseline, 8, 5, 0, True)
        _record(result, "visual", baseline, 8, 2, 0, True)
        _record(result, "visual", head_prefix, 2, 2, 1, True)
        _record(result, "visual", baseline, 8, 2, 0)
        # 段 B：插入点 5 输入 Z（草稿 5a），左移到 5，退格两步删除。
        _record(result, "visual", baseline, 5, 5, 0, True)
        _record(result, "visual", baseline, 6, 6, 0, True, b"Z")
        _record(result, "visual", baseline, 5, 5, 0, True, b"Z")
        _record(result, "visual", two_step_text, 2, 2, 2, True)
        _record(result, "visual", committed, 5, 5, 1)
        _record(result, "visual", baseline, 5, 5, 0)
        # 段 C：源码模式 34 处点击，左扩到 28..34，退格删除「甲乙」。
        _record(result, "source", baseline, 0, 0, 0)
        _record(result, "source", baseline, 34, 34, 0)
        _record(result, "source", baseline, 34, 31, 0)
        _record(result, "source", baseline, 34, 28, 0)
        _record(result, "source", source_text, 28, 28, 1)
        _record(result, "source", baseline, 34, 28, 0)
        # 段 D：全选整篇后剪切清空，撤销恢复整篇选区。
        _record(result, "source", baseline, 0, length, 0)
        _record(result, "source", b"", 0, 0, 1)
        _record(result, "source", baseline, 0, length, 0)
        # 段 E：回到可视化再次删除选中文字，随后保存并关闭。
        _record(result, "visual", baseline, 8, 8, 0, True)
        _record(result, "visual", baseline, 8, 5, 0, True)
        _record(result, "visual", baseline, 8, 2, 0, True)
        _record(result, "visual", head_prefix, 2, 2, 1, True)
        return result

    expected_e = head_prefix
    with tempfile.TemporaryDirectory(dir=native.ROOT / "logs/validation",
                                     prefix="delete-variants-self-test-") as temporary:
        evidence = Path(temporary)

        def install(records):
            (evidence / "session.tsv").write_text(
                "\t".join(COLUMNS) + "\n"
                + "\n".join("\t".join(row[column] for column in COLUMNS)
                            for row in records) + "\ntruncated=false\n")
            for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
                (evidence / name).write_bytes(expected_e)
            (evidence / "final.tsv").write_text(
                "name=fixture.md\tsource_len=79\tstored_len=79\n"
                "disk_len=79\tdisk_equals_source=true\tdisk_equals_stored=true\n")

        def rejected(operation):
            try:
                operation()
            except (native.DriverError, OSError):
                return
            raise native.DriverError("最小违规反例被错误判为通过")

        require_fresh(evidence)
        install(rows())
        verify(evidence, baseline)
        rejected(lambda: require_fresh(evidence))

        broken = rows()
        broken[4]["text_hex"] = (baseline[:2] + baseline[9:]).hex()
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[4]["open_block"] = "false"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[4]["undo_depth"] = "2"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[9]["undo_depth"] = "1"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[8]["selection_head"] = "4"
        broken[8]["selection_anchor"] = "4"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[10]["selection_head"] = "6"
        broken[10]["selection_anchor"] = "6"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[11]["selection_head"] = "6"
        broken[11]["selection_anchor"] = "6"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[16]["mode"] = "visual"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[20]["text_hex"] = baseline[:0].hex() + (baseline[:1]).hex()
        broken[20]["text_len"] = "1"
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[12]["text_hex"] = (baseline[:2] + baseline[9:]).hex()
        install(broken)
        rejected(lambda: verify(evidence, baseline))

        install(rows())
        (evidence / "disk-after.bin").write_bytes(baseline)
        rejected(lambda: verify(evidence, baseline))
    print("删除变体取证器：1 正例、11 最小违规反例通过（合成，非平台证据）")


def open_document(driver, fixture):
    """经文件面板打开夹具：置前 → Cmd+O → 前往文件夹 → 直接写路径字段 →
    回车导航 → 回车打开。

    面板属于系统面板服务窗口：热键与粘贴只有在前台焦点确实落到面板时才生效，
    实测复制/粘贴不可靠，而路径字段可写（set-value）。导航后先核对面板区位
    与选中文件，再回车打开，最后用唯一文件名核对窗口标题，避免误开同名文件。
    """
    target = str(fixture.resolve())
    for attempt in range(1, 3):
        activate(driver)
        driver.call(["hotkey", "--key", native.OPEN_PANEL_KEY], "打开文件面板")
        time.sleep(1.2)
        system_events(driver, 'keystroke "g" using {command down, shift down}',
                      "打开前往文件夹")
        time.sleep(1.0)

        def path_field_index():
            current = driver.snapshot()
            current_tree = driver.tree_text(current)
            for current_line in current_tree.splitlines():
                hit = re.match(r'^\s*(\d+) text field \(settable\), Value: /',
                               current_line)
                if hit:
                    return int(hit.group(1)), current, current_tree
            return None, current, current_tree

        field, snapshot, tree = path_field_index()
        if field is None:
            # System Events 偶发没有到达面板服务窗口；orca 直接向应用投递再试一次。
            driver.call(["hotkey", "--key", native.GO_TO_FOLDER_KEY],
                        "打开前往文件夹（orca 直投）")
            time.sleep(1.0)
            field, snapshot, tree = path_field_index()
        if field is not None:
            driver.call(["set-value", "--element-index", str(field), "--value", target],
                        "写入夹具绝对路径")
            time.sleep(0.4)
            system_events(driver, "key code 36", "回车确认路径")
            time.sleep(1.2)
        else:
            # 面板已按上次选择停到夹具目录：用行容器 open 辅助动作直接打开。
            where0 = re.search(r'^\s*(\d+) pop up button Where:, Value: (.*)$', tree, re.M)
            row = driver.find_row(snapshot, fixture.name)
            if where0 is None or where0.group(2) != fixture.parent.name or row is None:
                raise native.DriverError(
                    "文件面板既没有可写路径字段，也未停在夹具目录")
            container = driver.find_row_container(snapshot, fixture.name)
            if container is None:
                raise native.DriverError("面板行容器缺失，无法执行 open 辅助动作")
            try:
                driver.call(["perform-secondary-action", "--element-index", str(container),
                             "--action", "open"], "行容器 open 辅助动作")
            except native.DriverError as error:
                write_text(driver.evidence / "secondary-action-fallback.txt",
                           str(error)[:600])
            time.sleep(1.5)
            if driver.snapshot()["window"]["title"] == fixture.name:
                write_json(driver.evidence / "open-panel.json", {
                    "attempt": attempt,
                    "where": where0.group(2),
                    "opened_by": "secondary-action",
                    "note": "前往文件夹未弹出时经面板行容器辅助动作打开夹具",
                })
                return
            driver.call(["click", "--element-index", str(container)], "点击行容器打开")
            time.sleep(1.5)
            if driver.snapshot()["window"]["title"] == fixture.name:
                write_json(driver.evidence / "open-panel.json", {
                    "attempt": attempt,
                    "where": where0.group(2),
                    "opened_by": "row-click",
                    "note": "前往文件夹未弹出时经面板行容器点击打开夹具",
                })
                return
            raise native.DriverError("面板行容器打开夹具后窗口标题不符")
        where = None
        selected = None
        for _ in range(10):
            snapshot = driver.snapshot()
            tree = driver.tree_text(snapshot)
            where = re.search(r'^\s*(\d+) pop up button Where:, Value: (.*)$', tree, re.M)
            # 选中行后面还跟着 Secondary Actions，不能用行尾锚点。
            selected = re.search(
                r'^\s*(\d+) text field \(selected, settable\), Value: '
                + re.escape(fixture.name), tree, re.M)
            if where is not None and selected is not None:
                break
            time.sleep(0.2)
        write_json(driver.evidence / "open-panel.json", {
            "attempt": attempt,
            "where": where.group(2) if where else None,
            "selected": bool(selected),
            "note": "前往文件夹导航后的面板区位与选中文件",
        })
        if where is None or where.group(2) != fixture.parent.name:
            raise native.DriverError(
                "前往文件夹没有导航到夹具目录："
                + (where.group(2) if where else "面板区位缺失"))
        if selected is None:
            raise native.DriverError("前往文件夹没有选中夹具文件")
        system_events(driver, "key code 36", "回车打开所选夹具")
        time.sleep(1.5)
        snapshot = driver.snapshot()
        if snapshot["window"]["title"] == fixture.name:
            return
        driver.call(["hotkey", "--key", native.CLOSE_DOCUMENT_KEY], "关闭误开的文档",
                    allow_missing_window=True)
        time.sleep(1.0)
    raise native.DriverError("原生窗口没有打开合成夹具")


def _open_fixture(driver, fixture):
    snapshot = driver.snapshot()
    if snapshot["window"]["title"] != fixture.name:
        open_document(driver, fixture)


def _shift_left(driver, times, note):
    for _ in range(times):
        driver.call(["hotkey", "--key", native.SHIFT_LEFT_KEY], note)
        time.sleep(0.35)


def run(driver, fixture):
    require_fresh(driver.evidence)
    driver.wait_until_ready()
    activate(driver)
    fixture.parent.mkdir(parents=True, exist_ok=True)
    fixture.write_bytes(FIXTURE_TEXT.encode("utf-8"))
    baseline = fixture.read_bytes()
    write_text(driver.evidence / "fixture-before.md", FIXTURE_TEXT)
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    _open_fixture(driver, fixture)
    driver.snapshot()

    # 段 A：可视化模式下选中文字后单步删除，块必须保持打开。
    driver.click_body("点击标题行打开块编辑")
    driver.snapshot()
    _shift_left(driver, 2, "向左扩展选中两个字")
    driver.snapshot()
    driver.backspace("退格删除选中文字")
    time.sleep(1.0)
    driver.snapshot()
    driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销选中文字删除")
    time.sleep(0.8)
    driver.snapshot()

    # 段 B：先形成待定草稿再删除，验证「封口 + 删除」两步历史。
    driver.click_body("重开块编辑，准备两步删除")
    driver.snapshot()
    driver.call(["type-text", "--text", "Z"], "插入普通字符建立待定草稿")
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["press-key", "--key", "Left"], "光标左移离开草稿尾部")
    time.sleep(0.5)
    driver.snapshot()
    driver.backspace("退格：先封口草稿再删除，形成两步历史")
    time.sleep(1.0)
    driver.snapshot()
    driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销删除步")
    time.sleep(0.7)
    driver.snapshot()
    driver.call(["hotkey", "--key", native.UNDO_KEY], "再撤销输入步")
    time.sleep(0.7)
    driver.snapshot()

    # 段 C：源码模式删除选中文字（无打开块的一次性删除）。
    mode_button(driver, "源码")
    time.sleep(1.5)
    driver.click_body("点击源码首行进入源码输入")
    driver.snapshot()
    _shift_left(driver, 2, "源码模式下向左扩展选中两个字")
    driver.snapshot()
    driver.backspace("源码模式退格删除选中文字")
    time.sleep(1.0)
    driver.snapshot()
    driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销源码删除")
    time.sleep(0.8)
    driver.snapshot()

    # 段 D：跨块删除走文档级全选/剪切；Shift+Left 无法跨越块边界。
    driver.call(["hotkey", "--key", "cmd+a"], "全选整篇形成跨块选区")
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["hotkey", "--key", "cmd+x"], "剪切跨块选区")
    time.sleep(1.0)
    driver.snapshot()
    driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销跨块剪切")
    time.sleep(0.8)
    driver.snapshot()

    # 段 E：可视化删除后立即手工保存，随后关闭文档核对三份字节。
    mode_button(driver, "可视化")
    time.sleep(1.5)
    driver.click_body("回到可视化模式，点击标题行")
    driver.snapshot()
    before_delete = fixture_state(fixture)
    _shift_left(driver, 2, "向左扩展选中两个字")
    driver.snapshot()
    notes = {"delete_step": driver.serial + 1, "fixture_before": before_delete}
    driver.backspace("退格删除选中文字并准备保存")
    time.sleep(0.4)
    notes["save_step"] = driver.serial + 1
    driver.call(["hotkey", "--key", SAVE_KEY], "删除后立即手工保存（早于自动保存去抖）")
    disk = wait_disk_change(driver, fixture, before_delete["sha256"], 10.0,
                            "删除后手工保存的磁盘变化")
    notes["disk"] = disk
    write_json(driver.evidence / "delete-then-save.json", notes)
    if not disk["changed"]:
        raise native.DriverError("删除后手工保存没有改变磁盘内容")
    driver.snapshot()

    close_document_and_wait(driver)
    verify(driver.evidence, baseline)


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
        print("删除变体取证失败：" + str(error), file=sys.stderr)
        return 1
    print("删除变体真实窗口取证通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
