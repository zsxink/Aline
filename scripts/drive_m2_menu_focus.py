#!/usr/bin/env python3
"""W5 3.2「菜单借焦点」与 W5 3.4 ED-01 菜单入口的真实窗口取证。

背景：微信输入法组合期，Cmd+S/Cmd+. 等和弦被输入法消费，应用收不到命令
（见 logs/validation/m2-editable-core/2026-11-19-native-run/m2-12/run-notes.md
与 _ime-chord-probe/）。菜单动作不经键盘事件路由，是真实用户仍在使用的
保存入口；本脚本验证它与组合状态的交互。

场景：
  0) 对照：干净文档（无组合）点「文件 → 保存」，确认菜单动作确实送达应用；
  1) 组合进行中「文件 → 保存」：应用侧采样应出现 wait=pending 且
     save=waiting-for-composition；等待期间继续输入候选，磁盘不变；随后空格
     确认候选，等待结算写盘——ED-01 正向路径；
  2) 组合进行中「文件 → 保存」后「文件 → 取消等待保存」：采样应出现
     wait=cancelled，取消后磁盘不变；Escape 取消组合后再次核对磁盘——ED-01
     取消路径；
  3) 组合等待 × 10 秒恢复交错：等待保存期间不按键 11 秒，磁盘不得变化，
     独立恢复代应发布且在磁盘基线单点插入未提交拼音（组合区在正文中间时
     不是末尾追加），随后提交候选等待结算写盘；干净关闭后 Active 完整代
     应清空；
  4) 组合等待 × 目标/路径变化：等待保存期间「另存为…」菜单项必须禁用、
     「取消等待保存」保持可用，磁盘与保存目标不得变化；随后提交候选，
     等待仍结算写盘到原目标；
  5) 组合等待 × 关闭交错：等待保存期间点击窗口关闭按钮，窗口不得关闭、
     磁盘不得变化；随后空格提交候选，等待结算写盘，再干净关闭并逐字节
     核对关闭采样包与磁盘目标。
只经 macOS 无障碍接口投递真实输入（orca computer）与 System Events 菜单
点击；应用由脚本以 `ALINE_M2_EVIDENCE_DIR` 启动时，采样器在文档关闭路径落盘，
脚本随后核对 session.tsv 的 wait/save/composition 列。
用法：drive_m2_menu_focus.py --fixture <路径> --evidence <证据目录>
      （旧方式：--app-pid <已启动进程 pid> 复用外部进程，不核对采样）
"""
import argparse
import hashlib
from pathlib import Path
import re
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
from drive_m2_external_conflict import activate_cycle  # noqa: E402
from drive_m2_external_missing import (  # noqa: E402  复用启动与存储快照原语
    launch,
    settle_logs,
    store_has_generations,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
MENU = "文件"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
STORE_ROOT = (Path.home() / "Library" / "Application Support" / "Aline"
              / "recovery" / "v1")


def fixture_state(path):
    data = Path(path).read_bytes()
    return {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def recovery_candidate(generation, disk_bytes):
    """恢复代是否为当前磁盘基线单点插入一段未提交候选；返回插入位置与字节。

    组合区在正文中间时，恢复脚本把候选拼音插在组合起点，而不是追加到末尾
    （run-1 失败轮按“追加”比较而误判）；这里按“基线 + 单点插入”重建后核对。
    """
    source = generation["source"]
    baseline = generation["baseline"]
    if source is None or baseline != disk_bytes or len(source) <= len(baseline):
        return None
    prefix = 0
    while prefix < len(baseline) and source[prefix] == baseline[prefix]:
        prefix += 1
    suffix = 0
    while (suffix < len(baseline) - prefix
           and source[-1 - suffix] == baseline[-1 - suffix]):
        suffix += 1
    inserted = source[prefix:len(source) - suffix]
    if not inserted or len(inserted) > 64:
        return None
    if source != baseline[:prefix] + inserted + baseline[prefix:]:
        return None
    try:
        inserted.decode("utf-8")
    except UnicodeDecodeError:
        return None
    return {"position": prefix, "bytes": inserted}


def active_generations():
    """读取恢复存储 Active 完整代的 source/baseline 原始字节，仅用于核对。"""
    active = STORE_ROOT / "active"
    result = []
    if not active.is_dir():
        return result
    for lineage in sorted(active.iterdir()):
        groups = lineage / "g"
        if not groups.is_dir():
            continue
        ordered = sorted(groups.iterdir(),
                         key=lambda path: (0, int(path.name))
                         if path.name.isdigit() else (1, path.name))
        for generation in ordered:
            manifest = generation / "manifest"
            if not manifest.is_file():
                continue
            source = generation / "source.utf8"
            baseline = generation / "baseline.bytes"
            result.append({
                "document": lineage.name,
                "generation": generation.name,
                "source": source.read_bytes() if source.is_file() else None,
                "baseline": baseline.read_bytes() if baseline.is_file() else None,
            })
    return result


def verify_session_rows(evidence):
    """关闭后核对应用侧采样：等待、取消、组合与干净终态都必须真实出现。"""
    rows = session_table(evidence)

    def seqs(predicate):
        return [int(row["seq"]) for row in rows if predicate(row)]

    waiting = seqs(lambda row: row["wait"] == "pending")
    waiting_save = seqs(lambda row: row["wait"] == "pending"
                        and row["save"] == "waiting-for-composition")
    cancelled = seqs(lambda row: row["wait"] == "cancelled")
    composing = seqs(lambda row: int(row["composition_len"] or 0) > 0)
    # 无组合时采样器写空串（不是 "0"），run-3 失败轮按 "0" 比较而漏判。
    settled = seqs(lambda row: row["wait"] == "idle"
                   and row["save"] in ("idle", "none")
                   and row["draft_len"] == "0"
                   and row["composition_len"] in ("", "0"))
    result = {
        "rows": len(rows),
        "waiting_rows": waiting,
        "waiting_composition_rows": waiting_save,
        "cancelled_rows": cancelled,
        "composing_rows": composing,
        "settled_rows": settled,
        "note": "组合期菜单保存应留下 wait=pending/save=waiting-for-composition；"
                "取消等待应留下 wait=cancelled；组合候选应被采样；终态应干净",
    }
    write_json(Path(evidence) / "session-check.json", result)
    if not waiting_save:
        raise DriverError("采样未记录组合期保存等待（wait=pending 且 "
                          "save=waiting-for-composition）")
    if not cancelled:
        raise DriverError("采样未记录取消等待（wait=cancelled）")
    if not composing:
        raise DriverError("采样未记录组合候选（composition_len>0）")
    if not settled:
        raise DriverError("采样未出现干净终态（wait/save idle、无草稿与组合）")


def verify_close_packet(evidence, fixture):
    """关闭采样包逐字节对照磁盘目标，并要求 final.tsv 的相等标志为 true。"""
    disk = Path(fixture).read_bytes()
    payloads = {}
    for name in ("source-after.bin", "stored-after.bin", "disk-after.bin"):
        data = (Path(evidence) / name).read_bytes()
        payloads[name] = {"size": len(data),
                          "sha256": hashlib.sha256(data).hexdigest(),
                          "equals_disk": data == disk}
    final = (Path(evidence) / "final.tsv").read_text(encoding="utf-8")
    result = {
        "disk": {"size": len(disk), "sha256": hashlib.sha256(disk).hexdigest()},
        "payloads": payloads,
        "disk_equals_source": "disk_equals_source=true" in final,
        "disk_equals_stored": "disk_equals_stored=true" in final,
        "note": "关闭采样包与磁盘目标逐字节对照",
    }
    write_json(Path(evidence) / "final-check.json", result)
    if not all(entry["equals_disk"] for entry in payloads.values()):
        raise DriverError("关闭采样包与磁盘目标不一致")
    if not result["disk_equals_source"] or not result["disk_equals_stored"]:
        raise DriverError("final.tsv 未报告源/存储与磁盘相等")


def menubar_click(driver, item, note):
    """经 System Events 点击应用菜单项；记录命令、返回码与点击后快照。"""
    script = (f'tell application "System Events" to tell process "{APP_PROCESS}" '
              f'to click menu item "{item}" of menu "{MENU}" of menu bar item '
              f'"{MENU}" of menu bar 1')
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", f"<System Events: {MENU} > {item}>"],
        "note": note,
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.stdout.strip():
        write_text(driver.evidence / f"step-{driver.serial:03d}.stdout",
                   result.stdout)
    if result.stderr.strip():
        write_text(driver.evidence / f"step-{driver.serial:03d}.stderr",
                   result.stderr)
    if result.returncode != 0:
        raise DriverError("菜单点击失败（" + item + "）：" + result.stderr[-600:])


def menubar_item_enabled(driver, item, note):
    """读取应用菜单项的 AX enabled 状态；只读查询，不点击、不移动焦点。"""
    script = (f'tell application "System Events" to tell process "{APP_PROCESS}" '
              f'to get enabled of menu item "{item}" of menu "{MENU}" of '
              f'menu bar item "{MENU}" of menu bar 1')
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.ax.json", {
        "argv": ["osascript", "-e",
                 f"<System Events: enabled of {MENU} > {item}>"],
        "note": note,
        "exit_code": result.returncode,
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    if result.returncode != 0:
        raise DriverError("菜单项状态读取失败（" + item + "）："
                          + result.stderr[-600:])
    return result.stdout.strip().lower() == "true"


def menubar_click_soft(driver, item, note):
    """尝试点击菜单项但不因「已禁用」等失败中止；结果交证据判定。"""
    script = (f'tell application "System Events" to tell process "{APP_PROCESS}" '
              f'to click menu item "{item}" of menu "{MENU}" of menu bar item '
              f'"{MENU}" of menu bar 1')
    driver.serial += 1
    started = time.time_ns()
    result = subprocess.run(["osascript", "-e", script], text=True,
                            capture_output=True)
    record = {
        "argv": ["osascript", "-e", f"<System Events: {MENU} > {item}>"],
        "note": note,
        "exit_code": result.returncode,
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    }
    write_json(driver.evidence / f"step-{driver.serial:03d}.click-soft.json",
               record)
    return record


def save_panel_markers(driver):
    """识别系统保存位置选择器：可设置文本字段与保存类按钮。"""
    snapshot = driver.snapshot()
    tree = driver.tree_text(snapshot)
    fields = re.findall(r'^\s*\d+ text field \(settable\)', tree, re.M)
    buttons = [label for label in ("存储", "保存", "Save", "替换", "Replace")
               if re.search(r'^\s*\d+ button (?:default )?' + re.escape(label),
                            tree, re.M)]
    return len(fields), buttons


def wait_disk_change(driver, fixture, previous_sha, timeout, note):
    started = time.monotonic()
    while True:
        state = fixture_state(fixture)
        elapsed = round(time.monotonic() - started, 3)
        if state["sha256"] != previous_sha or elapsed >= timeout:
            result = {"note": note, "changed": state["sha256"] != previous_sha,
                      "waited_seconds": elapsed, **state, "step": driver.serial}
            write_json(driver.evidence / f"disk-{driver.serial:03d}.json", result)
            return result
        time.sleep(0.1)


def activate_ready(driver):
    """新启动实例偶尔在窗口就绪前无法取得前台（run-2 失败轮记录）；重试激活。"""
    for attempt in range(4):
        try:
            activate_cycle(driver)
            return
        except DriverError:
            if attempt == 3:
                raise
            time.sleep(1.2)


def clear_feedback_and_open_block(driver, body_point, note):
    """保存提示条会把正文下移；先用无副作用的 Left 结算清掉提示，再点正文。"""
    driver.call(["press-key", "--key", "Left"], "方向键结算并清除提示条",
                allow_missing_window=True)
    time.sleep(0.3)
    driver.call(["click", "--x", str(round(body_point[0], 1)),
                 "--y", str(round(body_point[1], 1))], note)
    time.sleep(0.7)
    driver.snapshot()


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    body_point = driver.heading_point()
    notes = {"body_point": [round(body_point[0], 1), round(body_point[1], 1)]}

    # 场景 0：对照——无组合时菜单保存应送达应用（提示条「已保存」）。
    notes["control_menu_save"] = driver.serial + 1
    menubar_click(driver, "保存", "对照：无组合时菜单保存")
    time.sleep(0.8)
    driver.snapshot()
    control_disk = fixture_state(fixture)
    write_json(driver.evidence / "control-disk.json",
               {"after_menu_save": control_disk,
                "unchanged_bytes": control_disk["sha256"] == before["sha256"]})

    # 场景 1：组合进行中菜单保存。对照保存的提示条会把正文下移，先结算清除
    # 提示条再按固定命中点进入块编辑。
    notes["menu_wait_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑（组合保存）")
    driver.type_keys(["n", "i"], "拼音组合 ni", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存")
    time.sleep(0.8)
    driver.snapshot()  # 组合是否仍在、是否出现等待提示，以截图为准
    wait_disk = fixture_state(fixture)
    # 等待期间继续更新候选：保存不得提前写盘，组合也不得被菜单打断。
    driver.type_keys(["h", "a", "o"], "等待期间继续拼音组合 hao", pause=0.3)
    time.sleep(0.6)
    driver.snapshot()
    wait_disk_updated = fixture_state(fixture)
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(0.5)
    driver.snapshot()
    saved1 = wait_disk_change(driver, fixture, control_disk["sha256"], 8.0,
                              "组合提交后等待保存应结算写盘")
    notes["menu_wait_end"] = driver.serial
    notes["saved1"] = saved1
    write_json(driver.evidence / "wait-update-proof.json", {
        "disk_at_wait": wait_disk,
        "disk_after_candidate_update": wait_disk_updated,
        "wait_kept_disk": wait_disk_updated["sha256"] == control_disk["sha256"],
        "note": "组合等待期间继续输入候选不得提前写盘",
    })
    if wait_disk["sha256"] != control_disk["sha256"] or \
            wait_disk_updated["sha256"] != control_disk["sha256"]:
        raise DriverError("组合等待期间磁盘被提前写入")
    if not saved1["changed"]:
        raise DriverError("组合期菜单保存等待未在任何组合结束后写盘")

    # 场景 2：组合进行中菜单保存后立即菜单取消等待。
    notes["menu_cancel_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑")
    driver.type_keys(["s", "h", "i"], "拼音组合 shi", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存（等待）")
    time.sleep(0.8)
    driver.snapshot()
    menubar_click(driver, "取消等待保存", "组合进行中经菜单取消等待")
    time.sleep(0.8)
    driver.snapshot()
    after_cancel = fixture_state(fixture)
    driver.call(["press-key", "--key", "Escape"], "Escape 取消组合")
    time.sleep(0.8)
    driver.snapshot()
    after_escape = fixture_state(fixture)
    notes["menu_cancel_end"] = driver.serial
    write_json(driver.evidence / "menu-cancel-proof.json", {
        "before_cancel": saved1,
        "after_cancel": after_cancel,
        "after_escape": after_escape,
        "cancel_kept_disk": after_cancel["sha256"] == saved1["sha256"],
        "escape_kept_disk": after_escape["sha256"] == saved1["sha256"],
        "note": "取消等待与取消组合都不得写盘；以 session.tsv 的 wait/save 列核对状态机",
    })
    if not (after_cancel["sha256"] == saved1["sha256"]
            and after_escape["sha256"] == saved1["sha256"]):
        raise DriverError("取消等待或取消组合改动了磁盘")

    # 场景 3：组合等待 × 独立 10 秒恢复交错。
    notes["recovery_wait_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑（恢复交错）")
    driver.type_keys(["h", "a", "o"], "拼音组合 hao（恢复交错）", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存（恢复交错）")
    time.sleep(0.8)
    driver.snapshot()
    disk_bytes = Path(fixture).read_bytes()
    disk_waiting = fixture_state(fixture)
    # 等待保存期间不按键 11 秒：独立恢复计时应发布完整代。
    time.sleep(11.0)
    disk_after_pause = fixture_state(fixture)
    generations = active_generations()
    records = []
    for generation in generations:
        candidate = recovery_candidate(generation, disk_bytes)
        records.append({
            "document": generation["document"],
            "generation": generation["generation"],
            "source_size": (len(generation["source"])
                            if generation["source"] is not None else None),
            "source_sha256": (hashlib.sha256(generation["source"]).hexdigest()
                              if generation["source"] is not None else None),
            "baseline_equals_disk": generation["baseline"] == disk_bytes,
            "inserted_hex": (candidate["bytes"].hex()
                             if candidate is not None else None),
            "inserted_text": (candidate["bytes"].decode("utf-8")
                              if candidate is not None else None),
            "insert_position": (candidate["position"]
                                if candidate is not None else None),
        })
    matching = [record for record in records
                if record["inserted_hex"] == b"hao".hex()]
    write_json(driver.evidence / "recovery-crossing.json", {
        "disk_at_wait": disk_waiting,
        "disk_after_11s": disk_after_pause,
        "active_generations": records,
        "matching_generations": len(matching),
        "note": "组合等待保存期间不按键 11 秒：磁盘不变，独立恢复代应在当前基线"
                "单点插入未提交候选 hao",
    })
    if disk_after_pause["sha256"] != disk_waiting["sha256"]:
        raise DriverError("组合等待期间磁盘被改动")
    if not generations:
        raise DriverError("组合等待期间没有发布独立恢复完整代")
    if not matching:
        raise DriverError("恢复代未保存等待中的候选输入（应为磁盘基线单点插入 hao）")
    notes["recovery_wait_end"] = driver.serial
    driver.call(["press-key", "--key", "space"], "空格确认候选，恢复交错等待结算")
    time.sleep(1.0)
    saved3 = wait_disk_change(driver, fixture, disk_after_pause["sha256"], 8.0,
                              "恢复交错等待保存应结算写盘")
    notes["saved3"] = saved3
    if not saved3["changed"]:
        raise DriverError("恢复交错场景的组合提交没有结算写盘")

    # 场景 4：组合等待 × 目标/路径变化——等待中「另存为…」必须禁用，
    # 不得打开系统目标选择器或更换保存目标；「取消等待保存」保持可用。
    # 放在关闭交错之前：关闭按钮点击会让输入法回到直接输入态，随后的
    # 拼音不再组合（run-6 失败轮记录），故目标变化场景先执行。
    notes["wait_target_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑（等待目标变化）")
    driver.type_keys(["n", "i"], "拼音组合 ni（等待目标变化）", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存（等待目标变化）")
    time.sleep(0.8)
    disk_before_target = fixture_state(fixture)
    # AX enabled 在菜单未展开时可能来自旧菜单，仅作观测记录，不作断言；
    # 行为判定以「另存为」尝试是否打开系统选择器、磁盘/目标是否变化为准。
    save_as_enabled = menubar_item_enabled(driver, "另存为…",
                                           "等待中读取另存为菜单项状态（观测）")
    cancel_wait_enabled = menubar_item_enabled(driver, "取消等待保存",
                                               "等待中读取取消等待菜单项状态（观测）")
    attempt = menubar_click_soft(driver, "另存为…",
                                 "等待中尝试另存为（目标/路径变化）")
    time.sleep(1.0)
    panel_fields, panel_buttons = save_panel_markers(driver)
    try:
        driver.snapshot()
        survived = True
    except DriverError:
        survived = False
    disk_after_target = fixture_state(fixture)
    write_json(driver.evidence / "wait-target-proof.json", {
        "disk_before": disk_before_target,
        "disk_after": disk_after_target,
        "window_survived": survived,
        "save_as_enabled_observation": save_as_enabled,
        "cancel_wait_enabled_observation": cancel_wait_enabled,
        "save_as_click": attempt,
        "save_panel_field_count": panel_fields,
        "save_panel_buttons": panel_buttons,
        "note": "组合等待保存期间尝试另存为：不得打开系统目标选择器、"
                "不得更换保存目标或改盘；随后提交仍写原目标",
    })
    if not survived:
        raise DriverError("等待期另存为尝试后窗口消失")
    if panel_fields or panel_buttons:
        raise DriverError("组合等待期间另存为打开了系统目标选择器")
    if disk_after_target["sha256"] != disk_before_target["sha256"]:
        raise DriverError("等待期另存为尝试改动了磁盘")
    notes["wait_target_end"] = driver.serial
    # 提交候选后保存必须仍落到原目标，证明等待期间目标未被更换。
    driver.call(["press-key", "--key", "space"], "空格确认候选，目标变化交错等待结算")
    time.sleep(1.0)
    saved4 = wait_disk_change(driver, fixture, disk_after_target["sha256"], 8.0,
                              "目标变化交错等待保存应结算写盘到原目标")
    notes["saved4"] = saved4
    if not saved4["changed"]:
        raise DriverError("目标变化交错场景的组合提交没有结算写盘")

    # 场景 5：组合等待 × 关闭交错——等待中点窗口关闭按钮不得关闭窗口或写盘，
    # 随后提交候选让等待结算，再干净关闭。
    notes["wait_close_start"] = driver.serial + 1
    clear_feedback_and_open_block(driver, body_point, "重新进入块编辑（等待关闭）")
    driver.type_keys(["n", "i"], "拼音组合 ni（等待关闭）", pause=0.3)
    menubar_click(driver, "保存", "组合进行中经菜单保存（等待关闭）")
    time.sleep(0.8)
    disk_before_close = fixture_state(fixture)
    snapshot = driver.snapshot()
    match = re.search(r'^\s*(\d+) close button',
                      driver.tree_text(snapshot), re.M)
    if match is None:
        raise DriverError("窗口缺少关闭按钮，无法验证等待期关闭交错")
    driver.call(["click", "--element-index", match.group(1)],
                "组合等待期间点击窗口关闭按钮", allow_missing_window=True)
    time.sleep(1.0)
    try:
        driver.snapshot()
        survived = True
    except DriverError:
        survived = False
    disk_after_close = fixture_state(fixture)
    write_json(driver.evidence / "wait-close-proof.json", {
        "disk_before": disk_before_close,
        "disk_after": disk_after_close,
        "window_survived": survived,
        "note": "组合等待保存期间点击关闭按钮：窗口应保持且磁盘不变",
    })
    if not survived:
        raise DriverError("组合等待期间点击关闭按钮关闭了窗口")
    if disk_after_close["sha256"] != disk_before_close["sha256"]:
        raise DriverError("等待期关闭尝试改动了磁盘")
    notes["wait_close_end"] = driver.serial
    # 提交候选让等待结算，随后干净关闭。
    driver.call(["press-key", "--key", "space"], "空格确认候选，关闭交错等待结算")
    time.sleep(1.0)
    saved5 = wait_disk_change(driver, fixture, disk_after_close["sha256"], 8.0,
                              "关闭交错等待保存应结算写盘")
    notes["saved5"] = saved5
    if not saved5["changed"]:
        raise DriverError("关闭交错场景的组合提交没有结算写盘")

    write_json(driver.evidence / "menu-focus-notes.json", notes)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    verify_session_rows(driver.evidence)
    write_json(driver.evidence / "store-after.json", {
        "active_generations": store_has_generations(),
        "note": "干净关闭后的恢复存储 Active 完整代；应清空",
    })
    if store_has_generations():
        raise DriverError("干净关闭后恢复存储仍有 Active 完整代")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app-pid", type=int)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    if args.app_pid is not None:
        driver = WindowDriver(args.app_pid, evidence)
        try:
            run(driver, fixture.resolve())
        except DriverError as error:
            write_text(evidence / "driver-error.txt", str(error)[:2000])
            print("驱动失败：" + str(error)[:800], file=sys.stderr)
            return 1
        print("菜单借焦点取证完成，步数：" + str(driver.serial), flush=True)
        return 0
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
    print("菜单借焦点取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
