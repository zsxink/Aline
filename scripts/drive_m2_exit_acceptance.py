#!/usr/bin/env python3
"""M2 macOS 出口补证：混合行尾编辑/保存重开、Unicode/IME、900ms 输入组。

脚本只创建独立临时夹具，经真实文件面板、键盘与输入法操作生产窗口。
源码保留原行尾，编辑后存储按既定策略重放 BOM 和 CRLF 多数派；预期字节
由固定夹具与明确区间操作计算，不把应用输出直接当成金样。重开使用第二个
真实进程，两阶段由同一次脚本串行编排。原始输出全部留在 logs/validation。

用法：python3 scripts/drive_m2_exit_acceptance.py --evidence <新证据目录>
      python3 scripts/drive_m2_exit_acceptance.py --self-test
真实运行需要 macOS、已构建 target/debug/aline、Orca 辅助功能权限和微信输入法。
输入源选择只改当前来源，finally 恢复运行前来源；不修改用户启用列表。
"""
import argparse
import copy
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

import drive_m2_editable_window as native
from drive_m2_candidate_replace import ensure_frontmost
from drive_m2_external_missing import launch, settle_logs, store_has_generations
from drive_m2_find_modes import mode_button
from drive_m2_interaction_matrix import close_document_and_wait
from drive_m2_menu_focus import activate_ready
from evidence_paths import write_json, write_text
from validation_output import require_validation_path

BOM = b"\xef\xbb\xbf"
SOURCE = ("# M2 出口验证\r\n\r\n"
          "分组锚点 👩‍💻 é &amp; \\*终点 保存锚点。\r\n\r\n"
          "混合行尾保真：此行以 LF 结束。\n"
          "末行保持 CRLF。\r\n").encode("utf-8")
ORIGINAL = BOM + SOURCE
GROUP_MARKER = "分组锚点".encode("utf-8")
SAVE_MARKER = "保存锚点".encode("utf-8")
GROUP_POINT = SOURCE.index(GROUP_MARKER) + len(GROUP_MARKER)
GROUP_TEXTS = [SOURCE[:GROUP_POINT] + b"abc"[:n] + SOURCE[GROUP_POINT:]
              for n in range(4)]
AFTER_GROUPS = GROUP_TEXTS[3]
SAVE_POINT = AFTER_GROUPS.index(SAVE_MARKER) + len(SAVE_MARKER)
FINAL_SOURCE = AFTER_GROUPS[:SAVE_POINT] + b"qw" + AFTER_GROUPS[SAVE_POINT:]
# 固定夹具没有孤立 CR；明确把其中唯一 LF 行尾也改写为多数派 CRLF。
FINAL_DISK = BOM + FINAL_SOURCE.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
KEYCODES = {"a": 0, "b": 11, "c": 8, "q": 12, "w": 13,
            "n": 45, "i": 34, "space": 49}
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")


def require(condition, note):
    if not condition:
        raise native.DriverError(note)


def complete_rows(evidence):
    lines = (Path(evidence) / "session.tsv").read_text().splitlines()
    require(lines and lines[-1] == "truncated=false" and
            sum(line.startswith("truncated=") for line in lines) == 1,
            "采样表必须具有唯一完整尾行 truncated=false")
    return native.session_table(evidence)


def find_state(rows, start, text, depth=None, redo=None, note="状态", mode=None):
    for index in range(start, len(rows)):
        row = rows[index]
        if row["text_hex"] != text.hex() or row.get("draft_len", "0") != "0":
            continue
        if depth is not None and int(row["undo_depth"]) != depth:
            continue
        if redo is not None and int(row["redo_depth"]) != redo:
            continue
        if mode is not None and row["mode"] != mode:
            continue
        return index
    raise native.DriverError("采样缺少" + note)


def verify_groups(rows, evidence):
    """已提交文本和深度必须按 abc→ab→a→空→a→ab→abc 连续恢复。"""
    cursor = find_state(rows, 0, GROUP_TEXTS[3], 3, 0, "三组输入")
    result = [int(rows[cursor]["seq"])]
    for size, depth, redo in ((2, 2, 1), (1, 1, 2), (0, 0, 3),
                              (1, 1, 2), (2, 2, 1), (3, 3, 0)):
        cursor = find_state(rows, cursor + 1, GROUP_TEXTS[size], depth, redo,
                            "逐组撤销/重做 " + str(size))
        result.append(int(rows[cursor]["seq"]))
    commands = []
    for path in sorted(Path(evidence).glob("step-*.command.json")):
        value = json.loads(path.read_text())
        if str(value.get("note", "")).startswith("900ms 普通输入组："):
            commands.append(value)
    require(len(commands) == 3, "900ms 组必须恰有三次独立物理按键")
    for value, key in zip(commands, ("a", "b", "c")):
        require(value.get("virtual_keys") == [KEYCODES[key]], "普通输入组使用了替代输入")
    intervals = [(right["started_epoch_ns"] - left["finished_epoch_ns"]) / 1e6
                 for left, right in zip(commands, commands[1:])]
    require(all(interval > 900 for interval in intervals), "普通输入间隔没有实际跨过 900ms")
    return cursor, {"seq": result, "physical_key_gaps_ms": intervals}


def verify_selected_ime(rows, cursor, cluster, confirm):
    """源码范围是独立字节预期；marked 必须绑定整个 Unicode 簇而非半字符。"""
    encoded = cluster.encode("utf-8")
    start = AFTER_GROUPS.index(encoded)
    end = start + len(encoded)
    marked = next((i for i in range(cursor + 1, len(rows))
                   if int(rows[i].get("composition_len") or "0") > 0), None)
    require(marked is not None, "未取得选中 Unicode 簇的 marked 回调")
    require(any(row["mode"] == "visual" and int(row["selection_anchor"]) == start
                and int(row["selection_head"]) == end for row in rows[cursor + 1:marked]),
            "组合前没有完整 Unicode 簇选区")
    row = rows[marked]
    require(row["text_hex"] == AFTER_GROUPS.hex() and int(row["undo_depth"]) == 3,
            "Unicode 组合期修改了源码/历史")
    require(int(row["composition_start"]) == start and int(row["composition_end"]) == end,
            "marked 未绑定选中 Unicode 簇的完整源码范围")
    settled = next((i for i in range(marked + 1, len(rows))
                    if not int(rows[i].get("composition_len") or "0")), None)
    require(settled is not None, "Unicode 组合未取得平台结束结果")
    row = rows[settled]
    if confirm:
        text = bytes.fromhex(row["text_hex"])
        prefix, suffix = AFTER_GROUPS[:start], AFTER_GROUPS[end:]
        require(text.startswith(prefix) and text.endswith(suffix), "Unicode 组合修改了选区外字节")
        candidate = text[len(prefix):len(text) - len(suffix)].decode("utf-8")
        require(candidate and not candidate.isascii() and all(c.isprintable() for c in candidate),
                "Unicode 组合确认未替换为中文候选")
        require(int(row["undo_depth"]) == 4 and int(row["redo_depth"]) == 0,
                "Unicode 簇替换没有形成独立单步")
        restored = find_state(rows, settled + 1, AFTER_GROUPS, 3, 1, "撤销 Unicode 簇中文替换")
    else:
        require(row["text_hex"] == AFTER_GROUPS.hex() and row["undo_depth"] == rows[marked]["undo_depth"]
                and row["redo_depth"] == rows[marked]["redo_depth"], "Unicode 组合取消改变了文本/历史")
        restored = settled
    require(int(rows[restored]["selection_anchor"]) == start and
            int(rows[restored]["selection_head"]) == end, "Unicode 簇替换撤销/取消未恢复原选区")
    return restored, {"cluster_bytes": len(encoded), "source_range": [start, end],
                      "marked_seq": int(rows[marked]["seq"]),
                      "settled_seq": int(rows[settled]["seq"]),
                      "restored_seq": int(rows[restored]["seq"]), "confirmed": confirm}


def verify_unicode(rows, cursor):
    """整簇删除与撤销不允许重建相邻字节；组合确认仅接受该锚点插入。"""
    deleted = []
    for cluster in ("👩‍💻", "é"):
        encoded = cluster.encode("utf-8")
        expected = AFTER_GROUPS.replace(encoded, b"", 1)
        cursor = find_state(rows, cursor + 1, expected, 4, 0, "整簇删除 " + cluster)
        deleted.append(int(rows[cursor]["seq"]))
        cursor = find_state(rows, cursor + 1, AFTER_GROUPS, 3, 1, "整簇撤销 " + cluster)
    cursor, emoji_ime = verify_selected_ime(rows, cursor, "👩‍💻", True)
    cursor, combining_ime = verify_selected_ime(rows, cursor, "é", False)
    point = AFTER_GROUPS.index("终点".encode("utf-8"))
    marked = None
    for index in range(cursor + 1, len(rows)):
        row = rows[index]
        if int(row.get("composition_len") or "0") > 0:
            require(row["text_hex"] == AFTER_GROUPS.hex(), "组合期修改了源文本")
            require(int(row["undo_depth"]) == 3, "组合期产生历史步骤")
            marked = index
            break
    require(marked is not None, "未取得实体/转义相邻的真实 marked 回调")
    committed = None
    candidate = None
    for index in range(marked + 1, len(rows)):
        row = rows[index]
        if int(row["undo_depth"]) != 4 or int(row.get("composition_len") or "0"):
            continue
        text = bytes.fromhex(row["text_hex"])
        if text == AFTER_GROUPS:
            continue
        prefix, suffix = AFTER_GROUPS[:point], AFTER_GROUPS[point:]
        require(text.startswith(prefix) and text.endswith(suffix), "IME 修改了实体、转义或锚点外字节")
        inserted = text[len(prefix):len(text) - len(suffix)]
        value = inserted.decode("utf-8")
        require(value and not value.isascii() and all(c.isprintable() for c in value),
                "IME 未确认可辨认的中文候选")
        require(row.get("draft_len", "0") == "0", "IME 确认后仍有未结算草稿")
        committed, candidate = index, inserted
        break
    require(committed is not None, "未取得真实 IME 确认单步历史")
    cursor = find_state(rows, committed + 1, AFTER_GROUPS, 3, 1, "撤销中文候选")
    cancel_marked = None
    for index in range(cursor + 1, len(rows)):
        if int(rows[index].get("composition_len") or "0"):
            cancel_marked = index
            break
    require(cancel_marked is not None, "未取得第二轮待取消 marked")
    before = rows[cancel_marked]
    require(before["text_hex"] == AFTER_GROUPS.hex(), "取消轮组合前文本已漂移")
    cancelled = None
    for index in range(cancel_marked + 1, len(rows)):
        row = rows[index]
        if int(row.get("composition_len") or "0") == 0:
            require(row["text_hex"] == AFTER_GROUPS.hex(), "Escape 取消改变了源码")
            require(row["undo_depth"] == before["undo_depth"] and
                    row["redo_depth"] == before["redo_depth"], "取消产生或丢弃历史")
            cancelled = index
            break
    require(cancelled is not None, "未取得平台组合取消结果")
    return cancelled, {"delete_seq": deleted, "confirm_seq": int(rows[committed]["seq"]),
                       "cancel_seq": int(rows[cancelled]["seq"]),
                       "emoji_ime": emoji_ime, "combining_ime": combining_ime,
                       "candidate_utf8_bytes": len(candidate),
                       "unchanged_entity_escape_and_surrounding_bytes": True}


def verify_edit(evidence, fixture):
    rows = complete_rows(evidence)
    require(rows and rows[0]["text_hex"] == SOURCE.hex(), "初始源文本未保留混合行尾/去头 BOM")
    cursor, groups = verify_groups(rows, evidence)
    cursor, unicode = verify_unicode(rows, cursor)
    source_q = AFTER_GROUPS[:SAVE_POINT] + b"q" + AFTER_GROUPS[SAVE_POINT:]
    source_input = None
    for index in range(cursor + 1, len(rows)):
        row = rows[index]
        if row["mode"] == "source" and (
                (row["text_hex"] == AFTER_GROUPS.hex() and row.get("draft_hex") == b"q".hex())
                or (row["text_hex"] == source_q.hex() and row.get("draft_len", "0") == "0")):
            source_input = index
            break
    require(source_input is not None, "未取得 q 实际在 Source 模式接收的采样")
    cursor = source_input
    cursor = find_state(rows, cursor + 1, source_q, 4, 0, "源码 q 随切换到可视化保留", mode="visual")
    require(any(row["mode"] == "visual" and row.get("draft_hex") == b"w".hex()
                and row["text_hex"] == source_q.hex() for row in rows[cursor + 1:]),
            "未取得 w 实际在 Visual 模式直接续输的采样")
    cursor = find_state(rows, cursor + 1, FINAL_SOURCE, 5, 0, "可视化续输 w", mode="visual")
    for name, expected in (("source-after.bin", FINAL_SOURCE),
                           ("stored-after.bin", FINAL_DISK), ("disk-after.bin", FINAL_DISK)):
        require((Path(evidence) / name).read_bytes() == expected, "关闭包字节不符独立预期：" + name)
    require(Path(fixture).read_bytes() == FINAL_DISK, "实际磁盘未重放 BOM/CRLF 多数派")
    result = {"passed": True, "groups": groups, "unicode": unicode,
              "final_undo_depth": 5, "mixed_endings_preserved_in_source": True,
              "edited_storage_bom_crlf_replayed": True}
    write_json(Path(evidence) / "exit-edit-check.json", result)
    return result


def verify_reopen(evidence, fixture):
    expected_source = FINAL_DISK[len(BOM):]
    rows = complete_rows(evidence)
    require(rows and all(row["text_hex"] == expected_source.hex() for row in rows),
            "重开源码不等于已保存字节去头 BOM")
    require(all(int(row["undo_depth"]) == 0 and int(row["revision"]) == 0 for row in rows),
            "重开或切模式产生编辑/历史")
    require({row["mode"] for row in rows} >= {"source", "visual"}, "重开未核查双模式")
    for name, expected in (("source-after.bin", expected_source),
                           ("stored-after.bin", FINAL_DISK), ("disk-after.bin", FINAL_DISK)):
        require((Path(evidence) / name).read_bytes() == expected, "重开关闭包不符：" + name)
    require(Path(fixture).read_bytes() == FINAL_DISK, "重开改变磁盘字节")
    write_json(Path(evidence) / "exit-reopen-check.json", {"passed": True, "zero_history": True,
               "both_modes": True, "unchanged_stored_disk_bytes": True})


def locate(driver, query, collapse=None):
    """仅查询框用粘贴；正文输入始终真实按键，按源码选区定位后切回 Visual。"""
    mode_button(driver, "源码")
    driver.call(["hotkey", "--key", "cmd+f"], "查找独立源码锚点")
    indices = driver.find_panel_indices(driver.snapshot())
    require(indices["query"] is not None, "找不到查找框")
    driver.call(["click", "--element-index", str(indices["query"])], "聚焦查询框")
    driver.call(["hotkey", "--key", "cmd+a"], "全选旧查询")
    driver.call(["paste-text", "--text", query], "只在查询框粘贴独立定位词")
    driver.call(["press-key", "--key", "Return"], "跳到唯一源码匹配")
    driver.snapshot()
    driver.call(["press-key", "--key", "Escape"], "关闭查询且保留源码选区")
    if collapse:
        driver.call(["press-key", "--key", collapse], "收起选区到明确字节边界")
    mode_button(driver, "可视化")
    driver.snapshot()


def physical(driver, key, note):
    ensure_frontmost(driver, "物理按键前复核被测实例")
    driver.inject_keys([KEYCODES[key]], note, [key])
    driver.snapshot()


def validate_focused_path(role, parent_role, value, wanted=None):
    require(role == "AXTextField" and parent_role == "AXSheet",
            "前往文件夹焦点不在 sheet 路径字段，拒绝向未知焦点输入")
    if wanted is not None:
        require(value == wanted, "前往文件夹实际输入值与目标不符")


def focused_path(driver, wanted=None):
    """只读当前 AXFocusedUIElement；不受大型文件列表的树节点上限影响。"""
    result = driver.osascript(
        'tell process "aline"\n'
        'set targetField to value of attribute "AXFocusedUIElement"\n'
        'set parentField to value of attribute "AXParent" of targetField\n'
        'return (role of targetField as text) & tab & '
        '(role of parentField as text) & tab & (value of targetField as text)\n'
        'end tell', "只读前往文件夹当前聚焦字段/父级/值")
    values = result.stdout.rstrip("\n").split("\t", 2)
    require(len(values) == 3, "无法解析前往文件夹焦点证明")
    validate_focused_path(*values, wanted)
    write_json(driver.evidence / f"focus-{driver.serial:03d}.json",
               {"role": values[0], "parent_role": values[1], "value": values[2],
                "verified_target": wanted is not None})


def panel_target(snapshot, fixture):
    tree = snapshot.get("treeText", "")
    require(not (snapshot.get("truncation") or {}).get("truncated"),
            "文件面板树截断，无法证明所选文件")
    where = re.search(r'^\s*\d+ pop up button Where:, Value: (.*)$', tree, re.M)
    files = re.findall(r'^\s*(\d+) text field \((?:selected, )?settable\), Value: '
                       + re.escape(fixture.name) + r'(?:, Secondary Actions: .*)?\s*$', tree, re.M)
    preview = re.search(r'^\s*\d+ image Preview\s*\n\s*\d+ text '
                        + re.escape(fixture.name) + r'\s*$', tree, re.M)
    open_button = re.search(r'^\s*(\d+) button 打开 Markdown 文档…\s*$', tree, re.M)
    # NSBrowser 的当前文件可能不在 text field 上广告 selected；已加载的同名预览
    # 与启用的打开按钮证明当前语义选择，不从普通列表中的同名文字推断选择。
    require(where is not None and where.group(1) == fixture.parent.name and len(files) == 1
            and preview is not None and open_button is not None,
            "文件面板缺少专属目录、唯一文件、同名预览或启用的打开按钮")
    return {"file_index": int(files[0]), "open_button_index": int(open_button.group(1)),
            "preview_name": fixture.name}


def open_document(driver, fixture):
    """真实路径输入触发 Go sheet 自动补全，避免 AXSetValue 留下旧建议选择。

    先核查当前焦点的角色与 sheet 父级，才投递全选和真实键盘文本；大型旧目录的
    AX 截断不能作为选中证明，导航后只在专属小目录核对 Where 与唯一文件名。
    """
    target = str(fixture.resolve())
    # Orca 动作会附带整个面板快照；上次目录是大型 tmp 时一次可耗时三十多秒。
    # 已激活被测实例，面板阶段只投递真实系统键盘事件，直到导航到专属小目录。
    driver.osascript('keystroke "o" using command down', "打开真实文件面板")
    time.sleep(0.8)
    driver.osascript('keystroke "g" using {command down, shift down}', "打开前往文件夹 sheet")
    time.sleep(0.5)
    focused_path(driver)
    driver.osascript('keystroke "a" using command down', "全选已核查的路径字段")
    literal = '"' + target.replace("\\", "\\\\").replace('"', '\\"') + '"'
    driver.osascript("keystroke " + literal, "真实路径输入以刷新系统导航建议")
    time.sleep(0.6)
    focused_path(driver, target)
    driver.osascript("key code 36", "接受实际输入路径")
    time.sleep(1.0)
    snapshot = driver.snapshot()
    # 不在巨大/错误目录上重复读全树；一次不完整或路径不符即留下失败证据退出。
    target_proof = panel_target(snapshot, fixture)
    write_json(driver.evidence / "open-panel.json", {"fixture_name": fixture.name,
               "where": fixture.parent.name, "selected": True,
               "path_input": "system-events-keystroke", "focused_sheet_verified": True,
               "selection_proof": "唯一文件行+同名预览+启用打开按钮", **target_proof})
    # 用户已授权本轮前台原生取证；打开按钮会结束系统面板，Orca 必须显式允许
    # 恢复窗口才能投递这一语义动作。其它操作继续保持原有投递方式。
    driver.call(["click", "--element-index", str(target_proof["open_button_index"]),
                 "--restore-window"],
                "语义打开已核查的唯一夹具文件")
    deadline = time.monotonic() + 8
    while driver.title() != fixture.name:
        require(time.monotonic() < deadline, "打开后的窗口标题不是本轮唯一夹具")
        time.sleep(0.3)


def input_source_id(raw):
    hit = re.search(r'(?:"Input Mode"|InputSourceID)\s*=\s*"?([^";\s]+)', raw)
    if hit:
        return hit.group(1)
    hit = re.search(r'"KeyboardLayout Name"\s*=\s*"?([^";\s]+)', raw)
    require(hit is not None, "无法确定需恢复的当前输入源 ID")
    return "com.apple.keylayout." + hit.group(1)


def select_source(selector, wanted, evidence, label):
    result = subprocess.run([str(selector), wanted], text=True, capture_output=True)
    write_json(Path(evidence) / (label + ".json"), {"argv": ["scripts/select_input_source.swift", wanted],
               "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    require(result.returncode == 0, "选择输入源失败：" + wanted)
    time.sleep(0.3)


def run_edit(driver, fixture, selector, ime_source):
    open_document(driver, fixture)
    locate(driver, "分组锚点", "Right")
    select_source(selector, "com.apple.keylayout.ABC", driver.evidence, "input-abc")
    for key in ("a", "b", "c"):
        physical(driver, key, "900ms 普通输入组：" + key)
        # 实验变量必须实际跨过 900ms；不是就绪等待的替代。
        time.sleep(1.25)
    driver.call(["press-key", "--key", "Escape"], "封口第三输入组")
    driver.snapshot()
    for key, note in (("cmd+z", "逐组撤销"),) * 3 + (("cmd+shift+z", "逐组重做"),) * 3:
        driver.call(["hotkey", "--key", key], note)
        driver.snapshot()
    for cluster in ("👩‍💻", "é"):
        locate(driver, cluster)
        driver.backspace("删除已选中完整 Unicode 簇")
        driver.snapshot()
        driver.call(["hotkey", "--key", "cmd+z"], "撤销完整 Unicode 簇删除")
        driver.snapshot()
    select_source(selector, ime_source, driver.evidence, "input-ime")
    for cluster, confirm in (("👩‍💻", True), ("é", False)):
        locate(driver, cluster)
        for key in ("n", "i"):
            physical(driver, key, "选中完整 Unicode 簇后真实组合：" + key)
            time.sleep(0.35)
        if confirm:
            physical(driver, "space", "确认中文候选替换完整 emoji 簇")
            time.sleep(0.7)
            driver.snapshot()
            driver.call(["hotkey", "--key", "cmd+z"], "撤销 emoji 中文替换并恢复原选区")
        else:
            driver.call(["press-key", "--key", "Escape"], "取消组合符选区上的 marked 并恢复原选区")
        time.sleep(0.5)
        driver.snapshot()
    locate(driver, "终点", "Left")
    for key in ("n", "i"):
        physical(driver, key, "实体/转义相邻真实组合：" + key)
        time.sleep(0.35)
    physical(driver, "space", "空格确认中文候选")
    time.sleep(0.7)
    driver.snapshot()
    driver.call(["hotkey", "--key", "cmd+z"], "撤销中文确认，恢复实体/转义与 Unicode")
    driver.snapshot()
    locate(driver, "终点", "Left")
    for key in ("n", "i"):
        physical(driver, key, "实体/转义相邻第二轮待取消组合：" + key)
        time.sleep(0.35)
    driver.call(["press-key", "--key", "Escape"], "由平台 Escape 取消 marked")
    time.sleep(0.5)
    driver.snapshot()
    select_source(selector, "com.apple.keylayout.ABC", driver.evidence, "input-abc-final")
    locate(driver, "保存锚点", "Right")
    mode_button(driver, "源码")
    physical(driver, "q", "源码模式普通输入 q")
    mode_button(driver, "可视化")
    physical(driver, "w", "切模式后直接续输 w，不重新点击正文")
    driver.call(["press-key", "--key", "Escape"], "封口并保留双模式续输")
    driver.call(["hotkey", "--key", "cmd+s"], "保存混合行尾编辑结果")
    deadline = time.monotonic() + 12
    while fixture.read_bytes() != FINAL_DISK:
        require(time.monotonic() < deadline, "保存未在期限内得到独立预期磁盘字节")
        driver.snapshot()
        time.sleep(0.2)
    (driver.evidence / "saved-live.bin").write_bytes(fixture.read_bytes())
    close_document_and_wait(driver)
    verify_edit(driver.evidence, fixture)


def run_phase(evidence, fixture, operation):
    evidence.mkdir(parents=True)
    proc, stdout, stderr, log_dir = launch(evidence)
    driver = native.WindowDriver(proc.pid, evidence)
    try:
        driver.wait_until_ready()
        activate_ready(driver)
        write_text(evidence / "input-source-before.txt", driver.input_source())
        operation(driver)
        proc.wait(timeout=10)
        require(proc.returncode == 0, "正常关闭后进程退出码非零")
        require(not store_has_generations(), "正常关闭后仍残留 Active 完整恢复代")
    finally:
        natural = proc.poll() is not None
        if not natural:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        write_json(evidence / "process-exit.json", {"exit_code": proc.returncode,
                   "natural_exit": natural})
        settle_logs(evidence, stdout, stderr, log_dir)


def run_reopen(driver, fixture):
    open_document(driver, fixture)
    mode_button(driver, "源码")
    mode_button(driver, "可视化")
    close_document_and_wait(driver)
    verify_reopen(driver.evidence, fixture)


def synthetic_row(sequence, text, depth, redo=0, mode="visual", composition=b"", draft=b"",
                  selection=(0, 0), composition_range=(0, 0)):
    return {"seq": str(sequence), "text_hex": text.hex(), "draft_len": str(len(draft)), "draft_hex": draft.hex(),
            "undo_depth": str(depth), "redo_depth": str(redo), "revision": "0",
            "mode": mode, "composition_len": str(len(composition)),
            "selection_anchor": str(selection[0]), "selection_head": str(selection[1]),
            "composition_start": str(composition_range[0]), "composition_end": str(composition_range[1])}


def self_test():
    """验证器正反例只证明判定有效；不构成任何真实平台证据。"""
    with tempfile.TemporaryDirectory(dir=native.ROOT / "logs/validation", prefix="m2-exit-self-test-") as tmp:
        evidence = Path(tmp)
        records = [synthetic_row(0, SOURCE, 0)]
        for size, depth, redo in ((3, 3, 0), (2, 2, 1), (1, 1, 2), (0, 0, 3),
                                  (1, 1, 2), (2, 2, 1), (3, 3, 0)):
            records.append(synthetic_row(len(records), GROUP_TEXTS[size], depth, redo))
        for cluster in ("👩‍💻", "é"):
            records.append(synthetic_row(len(records), AFTER_GROUPS.replace(cluster.encode(), b"", 1), 4))
            records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1))
        for cluster, confirm in (("👩‍💻", True), ("é", False)):
            start = AFTER_GROUPS.index(cluster.encode())
            end = start + len(cluster.encode())
            records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1, selection=(start, end)))
            records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1, composition=b"ni",
                                         composition_range=(start, end)))
            if confirm:
                records.append(synthetic_row(len(records), AFTER_GROUPS[:start] + "你".encode() + AFTER_GROUPS[end:], 4))
            records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1, selection=(start, end)))
        point = AFTER_GROUPS.index("终点".encode())
        records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1, composition=b"ni"))
        records.append(synthetic_row(len(records), AFTER_GROUPS[:point] + "你".encode() + AFTER_GROUPS[point:], 4))
        records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1))
        records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1, composition=b"ni"))
        records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1))
        records.append(synthetic_row(len(records), AFTER_GROUPS, 3, 1, mode="source", draft=b"q"))
        source_q = AFTER_GROUPS[:SAVE_POINT] + b"q" + AFTER_GROUPS[SAVE_POINT:]
        records.append(synthetic_row(len(records), source_q, 4))
        records.append(synthetic_row(len(records), source_q, 4, draft=b"w"))
        records.append(synthetic_row(len(records), FINAL_SOURCE, 5))
        commands = [{"note": "900ms 普通输入组：" + key, "virtual_keys": [KEYCODES[key]],
                     "started_epoch_ns": n * 2_000_000_000,
                     "finished_epoch_ns": n * 2_000_000_000 + 100_000_000}
                    for n, key in enumerate(("a", "b", "c"))]

        def install(rows, cmds=commands):
            header = list(rows[0])
            (evidence / "session.tsv").write_text("\t".join(header) + "\n" + "\n".join(
                "\t".join(row[name] for name in header) for row in rows) + "\ntruncated=false\n")
            for n, value in enumerate(cmds):
                write_json(evidence / (f"step-{n:03d}.command.json"), value)
            for name, data in (("source-after.bin", FINAL_SOURCE), ("stored-after.bin", FINAL_DISK),
                               ("disk-after.bin", FINAL_DISK), ("fixture.md", FINAL_DISK)):
                (evidence / name).write_bytes(data)

        def rejected(operation):
            try:
                operation()
            except (native.DriverError, OSError, ValueError):
                return
            raise native.DriverError("验证器放过最小违规反例")

        install(records)
        verify_edit(evidence, evidence / "fixture.md")
        negatives = 0
        for row_index, field, value in ((2, "undo_depth", "0"), (4, "redo_depth", "0"),
                                       (8, "text_hex", AFTER_GROUPS.hex()),
                                       (13, "composition_end", "0"),
                                       (14, "undo_depth", "5"), (18, "undo_depth", "4"),
                                       (len(records) - 1, "undo_depth", "6")):
            broken = copy.deepcopy(records)
            broken[row_index][field] = value
            install(broken)
            rejected(lambda: verify_edit(evidence, evidence / "fixture.md"))
            negatives += 1
        broken = copy.deepcopy(records)
        for row in broken:
            row["mode"] = "visual"
        install(broken)
        rejected(lambda: verify_edit(evidence, evidence / "fixture.md"))
        negatives += 1
        install(records)
        for tail in ("", "truncated=true\n", "truncated=false\ntruncated=false\n"):
            content = (evidence / "session.tsv").read_text().split("truncated=")[0]
            (evidence / "session.tsv").write_text(content + tail)
            rejected(lambda: verify_edit(evidence, evidence / "fixture.md"))
            negatives += 1
            install(records)
        validate_focused_path("AXTextField", "AXSheet", "/fixture.md", "/fixture.md")
        fixture_sample = Path("fixture-only") / "unique.md"
        panel = {"treeText": "1 pop up button Where:, Value: fixture-only\n"
                 "2 text field (settable), Value: unique.md, Secondary Actions: open\n"
                 "3 image Preview\n4 text unique.md\n5 button 打开 Markdown 文档…\n"}
        panel_target(panel, fixture_sample)
        for values in (("AXTextArea", "AXSheet", "/fixture.md", "/fixture.md"),
                       ("AXTextField", "AXWindow", "/fixture.md", "/fixture.md"),
                       ("AXTextField", "AXSheet", "/old.md", "/fixture.md")):
            rejected(lambda v=values: validate_focused_path(*v))
            negatives += 1
        for broken in ({"treeText": panel["treeText"], "truncation": {"truncated": True}},
                       {"treeText": panel["treeText"].replace("fixture-only", "old")},
                       {"treeText": panel["treeText"].replace("unique.md", "other.md")},
                       {"treeText": panel["treeText"].replace("4 text unique.md", "4 text other.md")},
                       {"treeText": panel["treeText"].replace("5 button", "5 button (disabled)")},
                       {"treeText": panel["treeText"] + "6 text field (settable), Value: unique.md\n"}):
            rejected(lambda b=broken: panel_target(b, fixture_sample))
            negatives += 1
        install(records)
        (evidence / "stored-after.bin").write_bytes(FINAL_SOURCE)
        rejected(lambda: verify_edit(evidence, evidence / "fixture.md"))
        negatives += 1
        broken_commands = copy.deepcopy(commands)
        broken_commands[1]["started_epoch_ns"] = 500_000_000
        install(records, broken_commands)
        rejected(lambda: verify_edit(evidence, evidence / "fixture.md"))
        negatives += 1
        expected_source = FINAL_DISK[len(BOM):]
        reopened = [synthetic_row(0, expected_source, 0), synthetic_row(1, expected_source, 0, mode="source")]
        install(reopened)
        (evidence / "source-after.bin").write_bytes(expected_source)
        verify_reopen(evidence, evidence / "fixture.md")
        reopened[1]["revision"] = "1"
        install(reopened)
        (evidence / "source-after.bin").write_bytes(expected_source)
        rejected(lambda: verify_reopen(evidence, evidence / "fixture.md"))
        negatives += 1
    print(f"M2 出口验证器：4 正例、{negatives} 最小违规反例通过（合成，非平台证据）")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--ime-source", default="com.tencent.inputmethod.wetype.pinyin")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        (native.ROOT / "logs/validation").mkdir(parents=True, exist_ok=True)
        self_test()
        return 0
    if args.evidence is None:
        parser.error("真实运行需要 --evidence")
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    require(not any(evidence.iterdir()), "证据目录必须为空，不混用旧运行")
    require(not subprocess.run(["pgrep", "-f", "target/debug/aline"], capture_output=True,
                               text=True).stdout.strip(), "已有 aline 进程，拒绝干扰")
    require(not store_has_generations(), "恢复存储已有 Active 完整代，拒绝混用场景")
    raw = native.WindowDriver(0, evidence).input_source()
    previous_source = input_source_id(raw)
    write_text(evidence / "input-source-original.txt", raw)
    with tempfile.TemporaryDirectory(prefix="aline-m2-exit-") as tmp, \
            tempfile.TemporaryDirectory(dir=evidence, prefix="fixture-") as fixture_tmp:
        temporary = Path(tmp)
        fixture_directory = Path(fixture_tmp)
        fixture = fixture_directory / (fixture_directory.name + ".md")
        fixture.write_bytes(ORIGINAL)
        (evidence / "original.bin").write_bytes(ORIGINAL)
        selector = temporary / "select_input_source"
        build = subprocess.run(["swiftc", "-O", str(native.ROOT / "scripts/select_input_source.swift"),
                                "-o", str(selector)], text=True, capture_output=True)
        write_json(evidence / "input-selector-build.json", {"exit_code": build.returncode,
                   "stderr": build.stderr})
        require(build.returncode == 0, "输入源选择器编译失败")
        try:
            select_source(selector, "com.apple.keylayout.ABC", evidence, "input-abc-opening")
            run_phase(evidence / "edit", fixture,
                      lambda driver: run_edit(driver, fixture, selector, args.ime_source))
            run_phase(evidence / "reopen", fixture, lambda driver: run_reopen(driver, fixture))
            write_json(evidence / "summary.json", {"passed": True,
                       "scope": "macOS 当前主机中文真实窗口；其它平台/缩放未覆盖",
                       "edit": "edit/exit-edit-check.json", "reopen": "reopen/exit-reopen-check.json"})
        except (native.DriverError, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
            write_text(evidence / "driver-error.txt", str(error)[:3000])
            write_json(evidence / "summary.json", {"passed": False,
                       "scope": "macOS 当前主机中文真实窗口", "error": str(error)[:1200]})
            raise
        finally:
            select_source(selector, previous_source, evidence, "input-source-restored")
    print("M2 出口补证通过：混合行尾/Unicode/真实 IME/900ms 分组/双模式保存重开")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (native.DriverError, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print("M2 出口取证失败：" + str(error)[:2000], file=sys.stderr)
        sys.exit(1)
