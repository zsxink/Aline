#!/usr/bin/env python3
"""M2 3.2 真实剪贴板取证：选中复制、外部文本粘贴、空选区复制保留、
剪切写剪贴板，两条模式各走一遍。

调用前启动带 `ALINE_M2_EVIDENCE_DIR` 的原生应用（不带文档参数）；本驱动写入
合成夹具后经文件面板打开，用系统 pbpaste/pbcopy 独立读写剪贴板：

- 段 A（可视化复制）：点击标题行打开块 → Shift+Left 两次 → 先写入哨兵剪贴板
  → Cmd+C。剪贴板必须变成选中源码字节，文本、历史与选区不变（复制是纯读取，
  不产生历史）。
- 段 B（可视化粘贴外部剪贴板）：Right 收起选区 → 用 pbcopy 写入外部文本 →
  Cmd+V。文本等于「插入点前段 + 剪贴板 + 插入点后段」，选区折叠到插入文本
  之后，单步历史；撤销回基线。
- 段 C（源码复制与粘贴）：切源码模式 → 点击首行 → Shift+Left 一次 → 哨兵 +
  Cmd+C 核对选中字节 → 外部写入「丁」→ Cmd+V 替换选区，单步历史；撤销恢复
  文本与反向选区。
- 段 D（空选区复制保留剪贴板）：Right 收起选区 → 写入哨兵 → Cmd+C。剪贴板
  保持哨兵，文本与历史不变。
- 段 E（剪切写剪贴板）：Shift+Left 选中一个字符 → 哨兵 → Cmd+X。剪贴板等于
  选中字节，文本删除、单步历史；撤销恢复基线与选区。

关闭文档后按 session.tsv 时间线核对各段文本/选区/历史/块状态，并与
clipboard-ops.json 的独立剪贴板读写逐项对照；运行结束恢复原剪贴板。采样器
只在关闭路径落盘，故判定全部在关闭后做。只经 macOS 无障碍接口投递真实输入。
用法：drive_m2_clipboard_ops.py --app-pid <pid> --fixture <路径> --evidence <目录>
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

import drive_m2_editable_window as native  # noqa: E402  复用同一真实输入驱动
from drive_m2_delete_variants import open_document  # noqa: E402  复用文件面板打开路径
from drive_m2_find_modes import (  # noqa: E402
    activate,
    mode_button,
    panel_indices,
    require_fresh,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

FIXTURE_TEXT = '# 中文标题\n\n第一段普通文字，包含中文标点：你好，世界。\n\n## 二级标题\n\n- 列表项一\n- 列表项二\n\n```rust\nfn main() { println!("你好"); }\nlet horizontal_scroll_probe = "0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz";\n```\n\n第二段，用于查找替换与选区测试：甲甲乙丙丁。\n\n> 引用段落。\n'
COPY_SENTINEL = "哨兵-复制前"
PASTE_TEXT = "粘贴乙"
SOURCE_PASTE_TEXT = "丁"
PASTE_CODE_TEXT = "码"
EMPTY_SENTINEL = "哨兵-空选区"
CUT_SENTINEL = "哨兵-剪切前"


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


def _range(row):
    return sorted((int(row["selection_anchor"]), int(row["selection_head"])))


def _shift_left(driver, times, note):
    for _ in range(times):
        driver.call(["hotkey", "--key", native.SHIFT_LEFT_KEY], note)
        time.sleep(0.35)


def clipboard_read(driver, note):
    """独立读取系统剪贴板；返回原始字节并记录命令。"""
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["pbpaste"], capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["pbpaste"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise native.DriverError(note + "失败：" + result.stderr.decode("utf-8", "replace"))
    return result.stdout


def clipboard_write(driver, text, note):
    """独立写入系统剪贴板（纯文本）。"""
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["pbcopy"], input=text.encode("utf-8"), capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["pbcopy"],
        "note": note,
        "clipboard_hex": text.encode("utf-8").hex(),
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise native.DriverError(note + "失败：" + result.stderr.decode("utf-8", "replace"))


def _clipboard_entry(raw, note):
    return {
        "note": note,
        "hex": raw.hex(),
        "text": raw.decode("utf-8", "replace"),
    }


def verify(evidence, baseline):
    rows = native.session_table(evidence)
    ops = json.loads((evidence / "clipboard-ops.json").read_text(encoding="utf-8"))
    base_hex = baseline.hex()
    _assert(len(rows) >= 10, "采样行数不足以覆盖剪贴板段")

    start = _find(rows, 0, lambda row: row["text_hex"] == base_hex, "基线文本")
    _assert(rows[start]["mode"] == "visual", "打开文档后不是可视化模式")
    _assert(int(rows[start]["undo_depth"]) == 0, "打开文档后历史深度不为 0")
    cursor = start + 1

    # 段 A：复制前有一行非空选区；复制不产生历史与文本变化，随后 Right 收起。
    first_change = _find(rows, cursor,
                         lambda row: row["text_hex"] != base_hex, "段 B：粘贴后文本")
    copy_pre = _last(rows, cursor, first_change,
                     lambda row: (row["text_hex"] == base_hex and row["mode"] == "visual"
                                  and row["open_block"] == "true"
                                  and not _collapsed(row)),
                     "段 A：复制前的可视化选区")
    copy_a = _range(rows[copy_pre])
    expected_copy = baseline[copy_a[0]:copy_a[1]]
    _assert(ops["copy_visual"]["before_hex"] == COPY_SENTINEL.encode("utf-8").hex(),
            "段 A：复制前哨兵剪贴板不符")
    _assert(ops["copy_visual"]["after_hex"] == expected_copy.hex(),
            "段 A：复制写入的剪贴板不等于选中源码字节")

    # 段 B：粘贴前的一行必须已收起选区（Right），且在文本变化之前。
    paste_index = first_change
    paste_pre = _last(rows, copy_pre, paste_index,
                      lambda row: (row["text_hex"] == base_hex and row["mode"] == "visual"
                                   and _collapsed(row)),
                      "段 B：粘贴前的折叠光标")
    caret = int(rows[paste_pre]["selection_head"])
    paste_bytes = bytes.fromhex(ops["paste_visual"]["set_hex"])
    expected_paste = baseline[:caret] + paste_bytes + baseline[caret:]
    post = rows[paste_index]
    _assert(post["text_hex"] == expected_paste.hex(),
            "段 B：粘贴文本不等于「插入点前段 + 剪贴板 + 后段」")
    _assert(_collapsed(post) and int(post["selection_head"]) == caret + len(paste_bytes),
            "段 B：粘贴后选区未折叠在插入文本之后")
    _assert(int(post["undo_depth"]) == int(rows[paste_pre]["undo_depth"]) + 1,
            "段 B：粘贴不是单步历史")
    _assert(post["open_block"] == "false", "段 B：粘贴作为命令应关闭可视化块")
    undo_b = _find(rows, paste_index + 1, lambda row: row["text_hex"] == base_hex,
                   "段 B：撤销粘贴")
    _assert(_collapsed(rows[undo_b]) and int(rows[undo_b]["selection_head"]) == caret,
            "段 B：撤销粘贴未回到粘贴发起时的折叠光标")

    # 段 C：源码复制与粘贴。粘贴替换非空选区，撤销恢复原选区。
    src_change = _find(rows, undo_b + 1, lambda row: row["text_hex"] != base_hex,
                       "段 C：源码粘贴后文本")
    copy_src_pre = _last(rows, undo_b + 1, src_change,
                         lambda row: (row["text_hex"] == base_hex and row["mode"] == "source"
                                      and not _collapsed(row)),
                         "段 C：源码复制前的选区")
    src_range = _range(rows[copy_src_pre])
    expected_src_copy = baseline[src_range[0]:src_range[1]]
    _assert(ops["copy_source"]["after_hex"] == expected_src_copy.hex(),
            "段 C：源码复制写入的剪贴板不等于选中字节")
    src_paste_index = src_change
    src_bytes = bytes.fromhex(ops["paste_source"]["set_hex"])
    expected_src_paste = baseline[:src_range[0]] + src_bytes + baseline[src_range[1]:]
    src_post = rows[src_paste_index]
    _assert(src_post["text_hex"] == expected_src_paste.hex(),
            "段 C：源码粘贴文本不等于「选区前段 + 剪贴板 + 选区后段」")
    _assert(src_post["mode"] == "source", "段 C：粘贴发生在非源码模式")
    _assert(_collapsed(src_post)
            and int(src_post["selection_head"]) == src_range[0] + len(src_bytes),
            "段 C：源码粘贴后选区未折叠在插入文本之后")
    _assert(int(src_post["undo_depth"]) == int(rows[copy_src_pre]["undo_depth"]) + 1,
            "段 C：源码粘贴不是单步历史")
    undo_c = _find(rows, src_paste_index + 1, lambda row: row["text_hex"] == base_hex,
                   "段 C：撤销源码粘贴")
    _assert(rows[undo_c]["selection_anchor"] == rows[copy_src_pre]["selection_anchor"]
            and rows[undo_c]["selection_head"] == rows[copy_src_pre]["selection_head"],
            "段 C：撤销源码粘贴未恢复原选区")

    # 段 D：空选区复制是纯读取——从撤销源码粘贴到剪切之间不得有文本或深度变化。
    cut_index = _find(rows, undo_c + 1, lambda row: row["text_hex"] != base_hex,
                      "段 E：剪切后文本")
    depth_d = rows[undo_c]["undo_depth"]
    _assert(all(row["text_hex"] == base_hex and row["undo_depth"] == depth_d
                for row in rows[undo_c + 1:cut_index]),
            "段 D：空选区复制引入了文本或历史变化")
    _assert(ops["copy_empty"]["before_hex"] == EMPTY_SENTINEL.encode("utf-8").hex(),
            "段 D：空选区复制前哨兵不符")
    _assert(ops["copy_empty"]["after_hex"] == EMPTY_SENTINEL.encode("utf-8").hex(),
            "段 D：空选区复制改写了剪贴板")

    # 段 E：剪切写剪贴板，文本删除、单步历史，撤销恢复。
    cut_pre = _last(rows, undo_c + 1, cut_index,
                    lambda row: (row["text_hex"] == base_hex and not _collapsed(row)),
                    "段 E：剪切前的选区")
    cut_range = _range(rows[cut_pre])
    expected_cut = baseline[cut_range[0]:cut_range[1]]
    expected_cut_text = baseline[:cut_range[0]] + baseline[cut_range[1]:]
    _assert(ops["cut_source"]["before_hex"] == CUT_SENTINEL.encode("utf-8").hex(),
            "段 E：剪切前哨兵不符")
    _assert(ops["cut_source"]["after_hex"] == expected_cut.hex(),
            "段 E：剪切写入的剪贴板不等于选中字节")
    cut_post = rows[cut_index]
    _assert(cut_post["text_hex"] == expected_cut_text.hex(), "段 E：剪切后文本不符")
    _assert(_collapsed(cut_post)
            and int(cut_post["selection_head"]) == cut_range[0],
            "段 E：剪切后选区未折叠在区间起点")
    _assert(int(cut_post["undo_depth"]) == int(rows[cut_pre]["undo_depth"]) + 1,
            "段 E：剪切不是单步历史")
    undo_e = _find(rows, cut_index + 1, lambda row: row["text_hex"] == base_hex,
                   "段 E：撤销剪切")
    _assert(int(rows[undo_e]["undo_depth"]) == int(rows[cut_pre]["undo_depth"]),
            "段 E：撤销剪切的深度不符")

    # 段 F：代码块内选中复制与外部文本粘贴。光标必须落在代码块源码区间，
    # 否则说明可视化块打开的落点不在代码块内，证据不成立。
    code_change = _find(rows, undo_e + 1, lambda row: row["text_hex"] != base_hex,
                        "段 F：代码块粘贴后文本")
    code_copy_pre = _last(rows, undo_e + 1, code_change,
                          lambda row: (row["text_hex"] == base_hex and row["mode"] == "visual"
                                       and row["open_block"] == "true"
                                       and not _collapsed(row)),
                          "段 F：代码块内复制前的选区")
    code_copy_range = _range(rows[code_copy_pre])
    code_caret_row = _last(rows, undo_e + 1, code_change,
                           lambda row: (row["text_hex"] == base_hex and row["mode"] == "visual"
                                        and row["open_block"] == "true"
                                        and _collapsed(row)),
                           "段 F：代码块粘贴前的折叠光标")
    code_caret = int(rows[code_caret_row]["selection_head"])
    fence = baseline.index(b"```rust")
    code_start = fence + len(b"```rust")
    code_end = baseline.index(b"```", code_start + 3)
    _assert(code_start <= code_caret < code_end,
            "段 F：粘贴发起的光标不在代码块源码区间")
    _assert(code_start <= code_copy_range[0] and code_copy_range[1] <= code_end,
            "段 F：复制的选区不在代码块源码区间")
    _assert(ops["copy_code"]["before_hex"] == COPY_SENTINEL.encode("utf-8").hex(),
            "段 F：代码块复制前哨兵不符")
    _assert(ops["copy_code"]["after_hex"]
            == baseline[code_copy_range[0]:code_copy_range[1]].hex(),
            "段 F：代码块复制写入的剪贴板不等于选中字节")
    code_bytes = bytes.fromhex(ops["paste_code"]["set_hex"])
    expected_code_paste = baseline[:code_caret] + code_bytes + baseline[code_caret:]
    code_post = rows[code_change]
    _assert(code_post["text_hex"] == expected_code_paste.hex(),
            "段 F：代码块粘贴文本不等于「光标前段 + 剪贴板 + 后段」")
    _assert(_collapsed(code_post)
            and int(code_post["selection_head"]) == code_caret + len(code_bytes),
            "段 F：代码块粘贴后选区未折叠在插入文本之后")
    _assert(code_post["mode"] == "visual", "段 F：代码块粘贴不在可视化模式")
    _assert(int(code_post["undo_depth"]) == int(rows[code_caret_row]["undo_depth"]) + 1,
            "段 F：代码块粘贴不是单步历史")
    undo_f = _find(rows, code_change + 1, lambda row: row["text_hex"] == base_hex,
                   "段 F：撤销代码块粘贴")
    _assert(int(rows[undo_f]["undo_depth"]) == int(rows[code_caret_row]["undo_depth"]),
            "段 F：撤销代码块粘贴的深度不符")

    seen = {row["text_hex"] for row in rows}
    allowed = {
        base_hex,
        expected_paste.hex(),
        expected_src_paste.hex(),
        expected_cut_text.hex(),
        expected_code_paste.hex(),
    }
    _assert(seen <= allowed, "采样中出现预期之外的文本状态："
            + repr(sorted(seen - allowed)))

    source = (evidence / "source-after.bin").read_bytes()
    stored = (evidence / "stored-after.bin").read_bytes()
    disk = (evidence / "disk-after.bin").read_bytes()
    _assert(source == baseline, "关闭后源码字节不是基线")
    _assert(stored == baseline, "关闭后存储字节不是基线")
    _assert(disk == baseline, "关闭后磁盘字节不是基线")
    _assert("disk_equals_source=true" in (evidence / "final.tsv").read_text(encoding="utf-8"),
            "final.tsv 未记录磁盘与源码一致")

    write_json(evidence / "clipboard-ops.json", {
        **ops,
        "passed": True,
        "copy_visual_bytes": copy_a,
        "paste_visual_caret": caret,
        "paste_visual_bytes": len(paste_bytes),
        "copy_source_bytes": src_range,
        "paste_source_bytes": len(src_bytes),
        "cut_bytes": cut_range,
        "note": "剪贴板内容由 pbpaste 独立读回；文本/选区/历史按采样时间线核对",
    })


def _record(rows, mode, text, anchor, head, depth, opened=False):
    rows.append({
        "seq": str(len(rows)),
        "mode": mode,
        "text_hex": text.hex(),
        "text_len": str(len(text)),
        "selection_anchor": str(anchor),
        "selection_head": str(head),
        "undo_depth": str(depth),
        "open_block": "true" if opened else "false",
        "draft_hex": "",
        "draft_len": "0",
    })


def self_test():
    # 合成表只验证取证器的失败关闭，不作为平台证据。
    baseline = FIXTURE_TEXT.encode("utf-8")
    click = 8
    copy_a = 5
    paste_bytes = PASTE_TEXT.encode("utf-8")
    src_a, src_b = 5, 8
    src_bytes = SOURCE_PASTE_TEXT.encode("utf-8")
    cut_low, cut_high = 5, 8
    paste_text = baseline[:click] + paste_bytes + baseline[click:]
    src_paste_text = baseline[:src_a] + src_bytes + baseline[src_b:]
    cut_text = baseline[:cut_low] + baseline[cut_high:]
    code_fence = baseline.index(b"```rust")
    code_start = code_fence + len(b"```rust")
    code_end = baseline.index(b"```", code_start + 3)
    code_click = baseline.index(b"horizontal_scroll_probe", code_start) + 10
    code_sel = code_click - 3
    code_bytes = PASTE_CODE_TEXT.encode("utf-8")
    code_paste_text = baseline[:code_click] + code_bytes + baseline[code_click:]

    def rows():
        result = []
        _record(result, "visual", baseline, 0, 0, 0)
        _record(result, "visual", baseline, click, click, 0, True)
        _record(result, "visual", baseline, click, copy_a, 0, True)
        _record(result, "visual", baseline, click, click, 0, True)
        _record(result, "visual", paste_text, click + len(paste_bytes),
                click + len(paste_bytes), 1)
        _record(result, "visual", baseline, click, click, 0)
        _record(result, "source", baseline, click, click, 0)
        _record(result, "source", baseline, click, src_a, 0)
        _record(result, "source", src_paste_text, src_a + len(src_bytes),
                src_a + len(src_bytes), 1)
        _record(result, "source", baseline, click, src_a, 0)
        _record(result, "source", baseline, cut_high, cut_low, 0)
        _record(result, "source", cut_text, cut_low, cut_low, 1)
        _record(result, "source", baseline, cut_high, cut_low, 0)
        _record(result, "visual", baseline, cut_high, cut_low, 0)
        _record(result, "visual", baseline, code_click, code_click, 0, True)
        _record(result, "visual", baseline, code_click, code_sel, 0, True)
        _record(result, "visual", code_paste_text, code_click + len(code_bytes),
                code_click + len(code_bytes), 1)
        _record(result, "visual", baseline, code_click, code_click, 0)
        return result

    def ops():
        return {
            "original": _clipboard_entry("原有内容".encode("utf-8"), "原剪贴板"),
            "copy_visual": {
                "before_hex": COPY_SENTINEL.encode("utf-8").hex(),
                "after_hex": baseline[copy_a:click].hex(),
            },
            "paste_visual": {"set_hex": paste_bytes.hex()},
            "copy_source": {
                "before_hex": COPY_SENTINEL.encode("utf-8").hex(),
                "after_hex": baseline[src_a:click].hex(),
            },
            "paste_source": {"set_hex": src_bytes.hex()},
            "copy_empty": {
                "before_hex": EMPTY_SENTINEL.encode("utf-8").hex(),
                "after_hex": EMPTY_SENTINEL.encode("utf-8").hex(),
            },
            "cut_source": {
                "before_hex": CUT_SENTINEL.encode("utf-8").hex(),
                "after_hex": baseline[cut_low:cut_high].hex(),
            },
            "copy_code": {
                "before_hex": COPY_SENTINEL.encode("utf-8").hex(),
                "after_hex": baseline[code_sel:code_click].hex(),
            },
            "paste_code": {"set_hex": code_bytes.hex()},
        }

    # 校验前提：合成夹具的字节位置符合预期。
    assert len(baseline) == 440
    assert baseline[copy_a:click] and baseline[src_a:click]
    assert baseline[cut_low] == 0xe6 and baseline[cut_high] == 0xe6
    assert len(baseline[cut_low:cut_high]) == 3
    assert code_start < code_click < code_end
    assert code_sel >= code_start

    with tempfile.TemporaryDirectory(dir=native.ROOT / "logs/validation",
                                     prefix="clipboard-ops-self-test-") as temporary:
        evidence = Path(temporary)

        def install(records, operations):
            (evidence / "session.tsv").write_text(
                "\t".join(("seq", "mode", "text_hex", "text_len", "selection_anchor",
                           "selection_head", "undo_depth", "open_block",
                           "draft_hex", "draft_len"))
                + "\n" + "\n".join(
                    "\t".join(row[column] for column in
                              ("seq", "mode", "text_hex", "text_len", "selection_anchor",
                               "selection_head", "undo_depth", "open_block",
                               "draft_hex", "draft_len"))
                    for row in records) + "\ntruncated=false\n")
            for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
                (evidence / name).write_bytes(baseline)
            length = len(baseline)
            (evidence / "final.tsv").write_text(
                f"name=clipboard-proof.md\tsource_len={length}\tstored_len={length}\n"
                f"disk_len={length}\tdisk_equals_source=true\tdisk_equals_stored=true\n")
            (evidence / "clipboard-ops.json").write_text(
                json.dumps(operations, ensure_ascii=False), encoding="utf-8")

        def rejected(operation):
            try:
                operation()
            except (native.DriverError, OSError):
                return
            raise native.DriverError("最小违规反例被错误判为通过")

        require_fresh(evidence)
        install(rows(), ops())
        verify(evidence, baseline)
        rejected(lambda: require_fresh(evidence))

        broken_ops = ops()
        broken_ops["copy_visual"]["after_hex"] = baseline[click:click + 3].hex()
        install(rows(), broken_ops)
        rejected(lambda: verify(evidence, baseline))

        broken_ops = ops()
        broken_ops["paste_visual"]["set_hex"] = "e4b999"
        install(rows(), broken_ops)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[4]["undo_depth"] = "2"
        install(broken, ops())
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[4]["open_block"] = "true"
        install(broken, ops())
        rejected(lambda: verify(evidence, baseline))

        broken_ops = ops()
        broken_ops["copy_empty"]["after_hex"] = "00".encode().hex()
        install(rows(), broken_ops)
        rejected(lambda: verify(evidence, baseline))

        broken_ops = ops()
        broken_ops["cut_source"]["after_hex"] = baseline[cut_low:cut_high + 1].hex()
        install(rows(), broken_ops)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[8]["text_hex"] = (baseline[:src_a] + b"X" + baseline[src_b:]).hex()
        install(broken, ops())
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[9]["selection_head"] = str(click)
        install(broken, ops())
        rejected(lambda: verify(evidence, baseline))

        broken_ops = ops()
        broken_ops["copy_code"]["after_hex"] = "00"
        install(rows(), broken_ops)
        rejected(lambda: verify(evidence, baseline))

        broken_ops = ops()
        broken_ops["paste_code"]["set_hex"] = "e4b881"
        install(rows(), broken_ops)
        rejected(lambda: verify(evidence, baseline))

        broken = rows()
        broken[14]["selection_anchor"] = str(click)
        broken[14]["selection_head"] = str(click)
        install(broken, ops())
        rejected(lambda: verify(evidence, baseline))

        install(rows(), ops())
        (evidence / "disk-after.bin").write_bytes(paste_text)
        rejected(lambda: verify(evidence, baseline))
    print("剪贴板取证器：1 正例、12 最小违规反例通过（合成，非平台证据）")


def run(driver, fixture):
    require_fresh(driver.evidence)
    driver.wait_until_ready()
    activate(driver)
    fixture.parent.mkdir(parents=True, exist_ok=True)
    fixture.write_bytes(FIXTURE_TEXT.encode("utf-8"))
    baseline = fixture.read_bytes()
    write_text(driver.evidence / "fixture-before.md", FIXTURE_TEXT)
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    original = clipboard_read(driver, "备份原剪贴板")
    ops = {"original": _clipboard_entry(original, "原剪贴板")}
    try:
        open_document(driver, fixture)
        driver.snapshot()

        # 段 A：可视化选中两个字符后复制，剪贴板必须等于选中源码字节。
        driver.click_body("点击标题行打开块编辑")
        driver.snapshot()
        _shift_left(driver, 2, "向左扩展选中两个字")
        driver.snapshot()
        clipboard_write(driver, COPY_SENTINEL, "写入复制前哨兵")
        driver.call(["hotkey", "--key", native.COPY_KEY], "复制选中文字")
        time.sleep(0.4)
        copied = clipboard_read(driver, "读回复制后的剪贴板")
        ops["copy_visual"] = {
            "before_hex": COPY_SENTINEL.encode("utf-8").hex(),
            "after_hex": copied.hex(),
            "text": copied.decode("utf-8", "replace"),
        }

        # 段 B：收起选区后用外部文本粘贴，替换应发生在折叠光标处。
        driver.call(["press-key", "--key", "Right"], "Right 收起选区")
        time.sleep(0.3)
        driver.snapshot()
        clipboard_write(driver, PASTE_TEXT, "写入外部粘贴文本")
        ops["paste_visual"] = {"set_hex": PASTE_TEXT.encode("utf-8").hex()}
        driver.call(["hotkey", "--key", native.PASTE_KEY], "粘贴外部文本")
        time.sleep(0.7)
        driver.snapshot()
        driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销粘贴")
        time.sleep(0.6)
        driver.snapshot()

        # 段 C：源码模式复制选中字节，再用外部文本替换选区。
        mode_button(driver, "源码")
        time.sleep(1.5)
        driver.click_body("点击源码首行")
        driver.snapshot()
        _shift_left(driver, 1, "源码模式下向左扩展选中一个字")
        driver.snapshot()
        clipboard_write(driver, COPY_SENTINEL, "写入源码复制前哨兵")
        driver.call(["hotkey", "--key", native.COPY_KEY], "源码模式复制选中文字")
        time.sleep(0.4)
        copied_src = clipboard_read(driver, "读回源码复制后的剪贴板")
        ops["copy_source"] = {
            "before_hex": COPY_SENTINEL.encode("utf-8").hex(),
            "after_hex": copied_src.hex(),
            "text": copied_src.decode("utf-8", "replace"),
        }
        clipboard_write(driver, SOURCE_PASTE_TEXT, "写入源码粘贴文本")
        ops["paste_source"] = {"set_hex": SOURCE_PASTE_TEXT.encode("utf-8").hex()}
        driver.call(["hotkey", "--key", native.PASTE_KEY], "源码模式粘贴替换选区")
        time.sleep(0.7)
        driver.snapshot()
        driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销源码粘贴")
        time.sleep(0.6)
        driver.snapshot()

        # 段 D：收起选区后复制，剪贴板保持哨兵且历史不变。
        driver.call(["press-key", "--key", "Right"], "Right 收起选区")
        time.sleep(0.3)
        driver.snapshot()
        clipboard_write(driver, EMPTY_SENTINEL, "写入空选区复制前哨兵")
        driver.call(["hotkey", "--key", native.COPY_KEY], "空选区复制")
        time.sleep(0.4)
        kept = clipboard_read(driver, "读回空选区复制后的剪贴板")
        ops["copy_empty"] = {
            "before_hex": EMPTY_SENTINEL.encode("utf-8").hex(),
            "after_hex": kept.hex(),
            "text": kept.decode("utf-8", "replace"),
        }

        # 段 E：选中一个字符后剪切，剪贴板等于选中字节。
        _shift_left(driver, 1, "向左扩展选中一个字，准备剪切")
        driver.snapshot()
        clipboard_write(driver, CUT_SENTINEL, "写入剪切前哨兵")
        ops["cut_source"] = {"before_hex": CUT_SENTINEL.encode("utf-8").hex()}
        driver.call(["hotkey", "--key", native.CUT_KEY], "剪切选中文字")
        time.sleep(0.5)
        cut = clipboard_read(driver, "读回剪切后的剪贴板")
        ops["cut_source"]["after_hex"] = cut.hex()
        ops["cut_source"]["text"] = cut.decode("utf-8", "replace")
        driver.snapshot()
        driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销剪切")
        time.sleep(0.6)
        driver.snapshot()

        # 段 F：代码块内选中复制与外部文本粘贴。用查找把源码选区定位到
        # 代码块内的唯一标记，再切可视化让块在选区处打开——本夹具的
        # (440, 514) 坐标经实测落在结束围栏，不能作为块内落点。
        driver.call(["hotkey", "--key", native.FIND_KEY], "打开查找面板")
        time.sleep(0.5)
        indices = panel_indices(driver)
        driver.call(["click", "--element-index", str(indices["query"])], "聚焦查找框")
        time.sleep(0.2)
        driver.call(["paste-text", "--text", "horizontal_scroll_probe"],
                    "输入代码块内唯一标记")
        time.sleep(0.6)
        driver.call(["press-key", "--key", "Return"], "跳到代码块内匹配")
        time.sleep(0.5)
        driver.snapshot()
        indices = panel_indices(driver)
        driver.call(["click", "--element-index", str(indices["close"])], "关闭查找面板")
        time.sleep(0.5)
        mode_button(driver, "可视化")
        time.sleep(2.5)
        driver.snapshot()
        clipboard_write(driver, COPY_SENTINEL, "写入代码块复制前哨兵")
        driver.call(["hotkey", "--key", native.COPY_KEY], "代码块内复制选中文字")
        time.sleep(0.4)
        copied_code = clipboard_read(driver, "读回代码块复制后的剪贴板")
        ops["copy_code"] = {
            "before_hex": COPY_SENTINEL.encode("utf-8").hex(),
            "after_hex": copied_code.hex(),
            "text": copied_code.decode("utf-8", "replace"),
        }
        driver.call(["press-key", "--key", "Right"], "Right 收起代码块选区")
        time.sleep(0.3)
        driver.snapshot()
        clipboard_write(driver, PASTE_CODE_TEXT, "写入代码块外部粘贴文本")
        ops["paste_code"] = {"set_hex": PASTE_CODE_TEXT.encode("utf-8").hex()}
        driver.call(["hotkey", "--key", native.PASTE_KEY], "代码块内粘贴外部文本")
        time.sleep(0.7)
        driver.snapshot()
        driver.call(["hotkey", "--key", native.UNDO_KEY], "撤销代码块粘贴")
        time.sleep(0.6)
        driver.snapshot()

        # 保存回到基线并关闭，证据判定在关闭后按采样时间线做。
        write_json(driver.evidence / "clipboard-ops.json", ops)
        driver.call(["hotkey", "--key", "cmd+s"], "保存回到基线")
        time.sleep(0.6)
        close_document_and_wait(driver)
        verify(driver.evidence, baseline)
    finally:
        try:
            restored = original.decode("utf-8")
        except UnicodeDecodeError:
            write_text(driver.evidence / "clipboard-restore.txt",
                       "原剪贴板不是 UTF-8 文本，未恢复")
        else:
            clipboard_write(driver, restored, "恢复原剪贴板")
            ops["restore_hex"] = restored.encode("utf-8").hex()


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
        print("剪贴板取证失败：" + str(error), file=sys.stderr)
        return 1
    print("剪贴板真实窗口取证通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
