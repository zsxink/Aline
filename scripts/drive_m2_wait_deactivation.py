#!/usr/bin/env python3
"""W5 3.4「组合等待保存 × 真实窗口失活」真实窗口取证。

被测事实：真实中文组合进行中经「文件 → 保存」进入 ED-01 等待
（采样 wait=pending / save=waiting-for-composition），等待期间由
System Events 让 Finder 真实成为前台（窗口失活）。平台把 marked `ni`
原样自然提交一次，应用不得自行结束或重放；该自然确认必须经
`settle_after_input` → `finish_save_wait_intent` 结算保存等待并写盘
原目标一次（wait=idle、撤销深度 +1）。失活期间不按键、不读取窗口
（失活期读取会让 Orca 稍后抢回前台，见 deactivation run-1 失败轮）。

重新激活后原块上下文仍可用：继续 h/a/o 组合并由空格提交候选文本
（实际文本由采样读取，不写死首候选或字数——run-5 实测整词「会红」）；
显式 cmd+s 后关闭采样 source/stored/disk 与磁盘目标逐字节一致，恢复存储
无 Active 完整代。

附加观测（不作断言）：进入等待后、失活前读取「另存为…」「取消等待保存」
菜单项的 AX enabled 状态。gpui 锁定后端的 NSMenu autoenablesItems 会在
菜单展开时按 validateMenuItem 覆盖静态 disabled，该观测用于记录平台
实际状态；行为判定以点击是否打开选择器/改盘为准（见 menu_focus 场景 4）。

输入经真实系统事件投递；应用由脚本以 `ALINE_M2_EVIDENCE_DIR` 启动，
采样器在文档关闭路径落盘 session.tsv/final.tsv 与三份字节包。
用法：drive_m2_wait_deactivation.py --fixture <路径> --evidence <证据目录>
      drive_m2_wait_deactivation.py --self-test
"""
import argparse
from pathlib import Path
import subprocess
import sys
import tempfile
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
from drive_m2_deactivation import (  # noqa: E402  复用前台核对与时间线原语
    frontmost_unix_id,
    session_timeline,
    single_insertion,
)
from drive_m2_candidate_menu_deactivation import (  # noqa: E402  复用失活/前台原语
    ensure_frontmost,
    set_frontmost_timed,
    single_delta,
)
from drive_m2_menu_focus import (  # noqa: E402  复用菜单与关闭核对原语
    activate_ready,
    clear_feedback_and_open_block,
    fixture_state,
    menubar_click,
    menubar_item_enabled,
    verify_close_packet,
    wait_disk_change,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_PROCESS = "aline"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
KEY_A = 4
KEY_H = 4
KEY_O = 31
KEY_I = 34
KEY_N = 45
KEY_SPACE = 49


def ensure_our_frontmost(driver, note):
    """反复确认被测实例在前台；本机其他应用（Chrome）会 sporadic 抢前台。

    复用 ensure_frontmost 的切换与记录，切换后短暂复查；连续被抢则失败。
    """
    last = None
    for attempt in range(5):
        try:
            last = ensure_frontmost(driver, note + "（第 " + str(attempt + 1)
                                    + " 次确认）")
        except DriverError as error:
            last = {"error": str(error)[:300]}
        time.sleep(0.5)
        if frontmost_unix_id() == str(driver.pid):
            return last
        time.sleep(0.8)
    raise DriverError("前台反复被其他应用抢占，无法稳定注入："
                      + str(frontmost_unix_id()))


def enter_save_wait(driver, fixture, baseline_sha, label):
    """组合保持 marked 时经菜单保存进入 ED-01 等待；未进入则有限重试。

    run-7/run-8 实测：System Events 一步式菜单项点击偶发让输入法先把 marked
    原文自然提交（菜单展开路径与 run-4/run-5 相同但平台时序不稳定），保存
    随即直接写盘。该路径本身是合法平台行为（自然确认→立即保存），不是本
    分项的被测对象；这里识别后取消保存意图、Escape 结算草稿、撤销回基线，
    重新组合再试。等待是否真实建立以磁盘不变 + 关闭采样的 wait=pending 为准。
    """
    for attempt in range(1, 4):
        menubar_click(driver, "保存", label + "（第 " + str(attempt) + " 次尝试）")
        time.sleep(0.8)
        driver.snapshot()
        disk = fixture_state(fixture)
        if disk["sha256"] == baseline_sha:
            return {"attempt": attempt, "disk": disk}
        # 组合被菜单路径提前自然提交：先 Left 结算待定草稿（不改动文本），
        # 再撤销该提交并写回基线；顺序反了会把残余草稿一起写盘（自查修正）。
        write_json(driver.evidence / ("wait-retry-" + str(attempt) + ".json"), {
            "disk": disk,
            "note": "菜单点击使组合提前自然提交并直接写盘；结算草稿、撤销后重新组合",
        })
        # 与 clear_feedback_and_open_block 相同的无副作用结算。
        driver.call(["press-key", "--key", "Left"], "方向键结算残余草稿并清提示",
                    allow_missing_window=True)
        time.sleep(0.4)
        driver.call(["hotkey", "--key", "cmd+z"], "撤销提前提交的保存内容")
        time.sleep(0.6)
        driver.call(["hotkey", "--key", "cmd+s"], "把撤销后的基线写回磁盘")
        time.sleep(1.0)
        restored = fixture_state(fixture)
        if restored["sha256"] != baseline_sha:
            raise DriverError("提前提交撤销后磁盘未回到基线")
        clear_feedback_and_open_block(driver, driver.heading_point(),
                                      "重新进入块编辑（等待重试）")
        ensure_our_frontmost(driver, "重新组合前确认前台")
        driver.inject_keys([KEY_N, KEY_I], "重新 HID 拼音组合 ni",
                           names=["n", "i"])
        time.sleep(0.6)
        driver.snapshot()
    raise DriverError("三次尝试仍未能建立组合期保存等待（菜单路径均提前提交）")


def growth_events(rows):
    """采样中每次文本增长的单点插入事件；非单点插入记 None。"""
    events = []
    for index in range(1, len(rows)):
        previous_hex = rows[index - 1]["text_hex"]
        current_hex = rows[index]["text_hex"]
        if not previous_hex or not current_hex:
            continue
        previous = bytes.fromhex(previous_hex)
        current = bytes.fromhex(current_hex)
        if len(current) <= len(previous):
            continue
        row = rows[index]
        events.append({
            "seq": int(row["seq"]),
            "from_size": len(previous),
            "to_size": len(current),
            "inserted": single_delta(previous, current),
            "undo_depth": int(row["undo_depth"]),
            "redo_depth": int(row["redo_depth"]),
            "wait": row["wait"],
            "save": row["save"],
        })
    return events


def composition_episodes(rows):
    """把连续 composition_len>0 的采样行归并为组合段。"""
    episodes = []
    current = None
    for row in rows:
        length = int(row["composition_len"] or 0)
        if length > 0:
            if current is None:
                current = {"start_seq": int(row["seq"]),
                           "end_seq": int(row["seq"]), "hexes": []}
            current["end_seq"] = int(row["seq"])
            if row["composition_hex"]:
                current["hexes"].append(row["composition_hex"])
        elif current is not None:
            episodes.append(current)
            current = None
    if current is not None:
        episodes.append(current)
    return episodes


def candidate_text(inserted, label):
    """继续组合的提交必须是非空、非 ASCII、可打印的候选文本；不写死字数。

    微信输入法会按上下文整词提交（run-5 键入 hao 实际提交「会红」两字），
    单字断言会把正常候选提交误判为失败；这里只限定形态，实际文本由采样记录。
    """
    text = inserted.decode("utf-8")
    if not text or text.isascii() or not text.isprintable():
        raise DriverError(label + "提交不是可打印候选文本：" + inserted.hex())
    return text


def pinyin_matches(hexes, syllables):
    """组合段 marked 文本须是真实拼音的容错变体（IM 分词/吞键/采样时序）。

    微信输入法在 marked 里插入分词撇号并可能吞掉字母（run-5 实测 h'ha、
    run-10/run-11 实测 h'hao，候选提交仍为「会红」）。判定：去撇号后每个
    字符都属于所键音节字母表，且首字符等于首个音节首字母；仍拒绝任何
    非拼音内容（如 xyz）。
    """
    alphabet = set("".join(syllables))
    for item in hexes:
        text = bytes.fromhex(item).decode("utf-8").replace("'", "")
        if not text or text[0] != syllables[0][0]:
            return False
        if not set(text) <= alphabet:
            return False
    return bool(hexes)


def verify_session(evidence, baseline_bytes):
    """核对等待→失活自然提交→结算→继续组合的完整状态机与终态。"""
    rows = session_table(evidence)
    events = growth_events(rows)
    episodes = composition_episodes(rows)
    lengths = [int(row["text_len"]) for row in rows]
    decreases = [int(rows[index]["seq"])
                 for index in range(1, len(rows))
                 if lengths[index] < lengths[index - 1]]
    pending_seqs = [int(row["seq"]) for row in rows if row["wait"] == "pending"]
    waiting_save_seqs = [int(row["seq"]) for row in rows
                         if row["save"] == "waiting-for-composition"]
    last_text = (bytes.fromhex(rows[-1]["text_hex"])
                 if rows[-1]["text_hex"] else b"")
    source_bytes = (Path(evidence) / "source-after.bin").read_bytes()
    result = {
        "rows": len(rows),
        "pending_wait_seqs": pending_seqs,
        "waiting_save_seqs": waiting_save_seqs,
        "growth_events": [
            {
                "seq": event["seq"],
                "from_size": event["from_size"],
                "to_size": event["to_size"],
                "position": (event["inserted"]["position"]
                             if event["inserted"] else None),
                "hex": (event["inserted"]["bytes"].hex()
                        if event["inserted"] else None),
                "undo_depth": event["undo_depth"],
                "redo_depth": event["redo_depth"],
                "wait": event["wait"],
                "save": event["save"],
            }
            for event in events
        ],
        "episodes": episodes,
        "length_decreases": decreases,
        "last_wait": rows[-1]["wait"],
        "last_save": rows[-1]["save"],
        "last_undo_depth": int(rows[-1]["undo_depth"]),
        "last_text_equals_source": last_text == source_bytes,
        "note": "wait=pending 必须先于第一次文本增长；失活自然提交恰为原始 ni"
                "且该行 wait=idle（等待已结算）；第二次增长为非空可打印候选文本"
                "（字数随输入法上下文变化，不写死）；撤销深度 1,2、文本长度不得"
                "回落、终态 wait/save 均 idle",
    }
    write_json(Path(evidence) / "session-check.json", result)

    if decreases:
        raise DriverError("采样出现文本长度回落（可能有输入或提交丢失）")
    if not pending_seqs or not waiting_save_seqs:
        raise DriverError("采样未记录组合期保存等待（wait=pending 且 "
                          "save=waiting-for-composition）")
    if len(events) != 2:
        raise DriverError("采样文本增长事件数应为 2（失活自然提交 + 候选提交），"
                          "实际 " + str(len(events)))
    first = events[0]["inserted"]
    second = events[1]["inserted"]
    if first is None or first["bytes"] != b"ni":
        raise DriverError("失活自然提交应为原始 marked ni，实际 "
                          + (first["bytes"].hex() if first else "非单点插入"))
    if events[0]["wait"] != "idle":
        raise DriverError("失活自然提交行 wait 应为 idle（等待已随自然确认结算），"
                          "实际 " + events[0]["wait"])
    if max(pending_seqs) >= events[0]["seq"]:
        raise DriverError("wait=pending 未先于失活自然提交出现")
    if second is None:
        raise DriverError("继续组合的提交不是单点插入")
    candidate = candidate_text(second["bytes"], "重新激活后继续组合")
    if [event["undo_depth"] for event in events] != [1, 2]:
        raise DriverError("两次提交的撤销深度应为 1,2，实际 "
                          + str([event["undo_depth"] for event in events]))
    if len(episodes) != 2:
        raise DriverError("组合段数应为 2，实际 " + str(len(episodes)))
    if not pinyin_matches(episodes[0]["hexes"], ("n", "i")):
        raise DriverError("第一段组合内容与真实拼音 ni 不符")
    if not pinyin_matches(episodes[1]["hexes"], ("h", "a", "o")):
        raise DriverError("第二段组合内容与真实拼音 hao 不符")
    start = first["position"]
    if second["position"] != start + len(first["bytes"]):
        raise DriverError("两次插入未首尾相接：期望位置 "
                          + str(start + len(first["bytes"]))
                          + "，实际 " + str(second["position"]))
    if rows[-1]["wait"] != "idle" or rows[-1]["save"] != "idle":
        raise DriverError("终态 wait/save 未回到 idle")
    if int(rows[-1]["undo_depth"]) != 2:
        raise DriverError("终态撤销深度应为 2")
    if not result["last_text_equals_source"]:
        raise DriverError("采样末尾文本与关闭源字节不一致")

    result["start_position"] = start
    result["candidate_hex"] = second["bytes"].hex()
    result["candidate_text"] = candidate
    write_json(Path(evidence) / "session-check.json", result)
    return result


def verify_insertion(evidence, baseline_bytes, expected_insertion, position):
    """最终源字节必须是基线在采样插入点的单点连续插入。"""
    final = (Path(evidence) / "source-after.bin").read_bytes()
    inserted = single_insertion(baseline_bytes, final)
    expected_final = (baseline_bytes[:position] + expected_insertion
                      + baseline_bytes[position:])
    result = {
        "expected_insertion": {"position": position,
                               "size": len(expected_insertion),
                               "hex": expected_insertion.hex()},
        "inserted": (None if inserted is None else {
            "position": inserted["position"],
            "size": len(inserted["bytes"]),
            "hex": inserted["bytes"].hex(),
        }),
        "final_size": len(final),
        "note": "失活自然提交与继续组合候选应合并为同一插入点的连续字节",
    }
    write_json(Path(evidence) / "insertion-check.json", result)
    if inserted is None:
        raise DriverError("最终源字节不是基线的单点连续插入")
    if inserted["position"] != position:
        raise DriverError("插入点应为 " + str(position)
                          + "，实际 " + str(inserted["position"]))
    if inserted["bytes"] != expected_insertion:
        raise DriverError("插入字节应为 " + expected_insertion.hex()
                          + "，实际 " + inserted["bytes"].hex())
    if final != expected_final:
        raise DriverError("最终源字节与预期重建不一致")
    return result


def self_test():
    """合成表只验证取证器的失败关闭，不作为原生窗口证据。"""
    baseline = "# 等待失活夹具\n".encode("utf-8")
    position = 2
    after_ni = baseline[:position] + b"ni" + baseline[position:]
    candidate = "好".encode("utf-8")
    final = baseline[:position] + b"ni" + candidate + baseline[position:]
    columns = ("seq", "text_len", "text_hex", "composition_len",
               "composition_hex", "wait", "save", "undo_depth", "redo_depth")

    def row(seq, text, comp_len, comp_hex, wait, save, undo):
        return [str(seq), str(len(text)), text.hex(), str(comp_len), comp_hex,
                wait, save, str(undo), "0"]

    rows = [
        row(0, baseline, 0, "", "idle", "idle", 0),
        row(1, baseline, 1, "6e", "idle", "idle", 0),
        row(2, baseline, 2, "6e69", "idle", "idle", 0),
        row(3, baseline, 2, "6e69", "pending", "waiting-for-composition", 0),
        row(4, after_ni, 0, "", "idle", "idle", 1),
        row(5, after_ni, 3, "68616f", "idle", "idle", 1),
        row(6, final, 0, "", "idle", "idle", 2),
    ]
    with tempfile.TemporaryDirectory(dir=ROOT / "logs/validation",
                                     prefix="wait-deactivation-self-test-") as temporary:
        evidence = Path(temporary)

        def install(records, source=None):
            (evidence / "session.tsv").write_text(
                "\t".join(columns) + "\n"
                + "\n".join("\t".join(record) for record in records)
                + "\ntruncated=false\n", encoding="utf-8")
            (evidence / "source-after.bin").write_bytes(
                final if source is None else source)

        def rejected(operation):
            try:
                operation()
            except DriverError:
                return
            raise DriverError("最小违规反例被错误判为通过")

        install(rows)
        result = verify_session(evidence, baseline)
        verify_insertion(evidence, baseline, b"ni" + candidate, position)
        if result["start_position"] != position:
            raise DriverError("正例插入点判定错误")
        # 正例 2：输入法按上下文整词提交（run-5 实测「会红」两字）。
        word = "会红".encode("utf-8")
        word_final = baseline[:position] + b"ni" + word + baseline[position:]
        word_rows = [record.copy() for record in rows]
        word_rows[6] = row(6, word_final, 0, "", "idle", "idle", 2)
        install(word_rows, source=word_final)
        word_result = verify_session(evidence, baseline)
        verify_insertion(evidence, baseline, b"ni" + word, position)
        if word_result["candidate_text"] != "会红":
            raise DriverError("多字候选正例判定错误")

        # 反例 1：从未进入保存等待。
        broken = [record.copy() for record in rows]
        broken[3][5] = "idle"
        broken[3][6] = "idle"
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 2：失活提交不是原始 ni（应用伪造候选或重放）。
        broken = [record.copy() for record in rows]
        forged = baseline[:position] + "你".encode("utf-8") + baseline[position:]
        broken[4] = row(4, forged, 0, "", "idle", "idle", 1)
        broken[5] = row(5, forged, 3, "68616f", "idle", "idle", 1)
        broken[6] = row(6, forged[:position + 3] + candidate
                      + forged[position + 3:], 0, "", "idle", "idle", 2)
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 3：自然提交行 wait 仍 pending（等待未随失活结算）。
        broken = [record.copy() for record in rows]
        broken[4][5] = "pending"
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 4：缺少继续组合的第二次提交。
        install(rows[:5])
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 5：撤销深度丢步。
        broken = [record.copy() for record in rows]
        broken[6][7] = "1"
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 6：文本长度回落（输入丢失）。
        broken = [record.copy() for record in rows]
        broken[5] = row(5, baseline, 3, "68616f", "idle", "idle", 1)
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 7：终态 wait 未回 idle。
        broken = [record.copy() for record in rows]
        broken[6][5] = "pending"
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 8：源字节不是基线单点插入（插入位置漂移）。
        install(rows, source=b"ni" + baseline)
        rejected(lambda: verify_insertion(evidence, baseline,
                                          b"ni" + candidate, position))
        # 反例 9：两次插入未首尾相接。
        broken = [record.copy() for record in rows]
        split = (baseline[:position + 4] + b"ni" + candidate
                 + baseline[position + 4:])
        broken[6] = row(6, split, 0, "", "idle", "idle", 2)
        install(broken, source=split)
        rejected(lambda: verify_session(evidence, baseline))
        # 反例 10：继续组合提交为 ASCII（marked 原文而非候选文本）。
        broken = [record.copy() for record in rows]
        ascii_final = baseline[:position] + b"nihao" + baseline[position:]
        broken[6] = row(6, ascii_final, 0, "", "idle", "idle", 2)
        install(broken, source=ascii_final)
        rejected(lambda: verify_session(evidence, baseline))
        # 正例 3：marked 带分词撇号且吞字母（run-10/run-11 实测 h'ho）。
        apostrophe_rows = [record.copy() for record in rows]
        apostrophe_rows[5] = row(5, after_ni, 4, "6827686f", "idle", "idle", 1)
        install(apostrophe_rows)
        verify_session(evidence, baseline)
        # 反例 11：组合段 marked 内容不是拼音（去撇号后仍不匹配）。
        broken = [record.copy() for record in rows]
        broken[5] = row(5, after_ni, 3, "78797a", "idle", "idle", 1)
        install(broken)
        rejected(lambda: verify_session(evidence, baseline))
    print("等待失活取证器：3 正例、11 最小违规反例通过（合成，非平台证据）")


def reset_fixture(fixture):
    """每轮运行前把夹具复位到规范字节。

    本场景按设计以写盘收尾；断言失败轮会把提交结果留在夹具里（run-5 失败
    保留后 run-6 误把污染字节当基线）。规范内容只在此处定义一份。
    """
    canonical = "# 等待失活组合夹具\n\n第一段用于组合输入与保存等待。\n\n第二段保持不动。\n"
    Path(fixture).write_bytes(canonical.encode("utf-8"))


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    reset_fixture(fixture)
    baseline_bytes = Path(fixture).read_bytes()
    (driver.evidence / "baseline.bin").write_bytes(baseline_bytes)
    before = fixture_state(fixture)
    write_json(driver.evidence / "fixture-before.json", before)
    driver.open_document(fixture)
    driver.snapshot()
    # 打开面板经行容器回退路径关闭后，前台偶发落到其他应用（run-9）；
    # 任何正文点击/按键前先重新确认被测实例在前台。
    ensure_our_frontmost(driver, "打开文档后确认被测实例在前台")
    body_point = driver.heading_point()
    notes = {"body_point": [round(body_point[0], 1), round(body_point[1], 1)]}

    clear_feedback_and_open_block(driver, body_point, "点击标题行打开块编辑")
    ensure_our_frontmost(driver, "组合前确认被测实例在前台")
    driver.inject_keys([KEY_N, KEY_I], "HID 拼音组合 ni（等待失活）",
                       names=["n", "i"])
    time.sleep(0.6)
    if frontmost_unix_id() != str(driver.pid):
        raise DriverError("组合期间被测实例不在前台")
    driver.snapshot()

    # 组合进行中经真实菜单保存进入 ED-01 等待（提前自然提交时有限重试）。
    notes["wait_entry"] = enter_save_wait(
        driver, fixture, before["sha256"], "组合进行中经菜单保存（等待失活）")
    disk_wait = notes["wait_entry"]["disk"]
    # 附加观测：菜单项 AX enabled（autoenable 覆盖静态 disabled 的平台事实）。
    notes["save_as_enabled_observation"] = menubar_item_enabled(
        driver, "另存为…", "等待中读取另存为菜单项状态（观测，不作断言）")
    notes["cancel_wait_enabled_observation"] = menubar_item_enabled(
        driver, "取消等待保存", "等待中读取取消等待菜单项状态（观测，不作断言）")

    # 等待期间真实失活：Finder 成为前台。失活期间不按键、不读取窗口。
    notes["deactivate"] = set_frontmost_timed(
        driver, "Finder", 'process "Finder"',
        "保存等待中：Finder 成为前台（窗口真实失活）", 0.6)
    time.sleep(1.2)
    during = frontmost_unix_id()
    if during == str(driver.pid):
        raise DriverError("失活未生效：前台仍是本实例")
    disk_during = fixture_state(fixture)
    notes["deactivation_frontmost"] = during

    # 重新激活；平台自然提交与等待结算由关闭采样核对。
    notes["reactivate"] = set_frontmost_timed(
        driver, APP_PROCESS,
        f'(first process whose unix id is {driver.pid})',
        "重新激活被测实例", 1.0)
    if notes["reactivate"]["frontmost_unix_id"] != str(driver.pid):
        raise DriverError("重新激活后前台不是被测实例")
    driver.snapshot()
    settled = wait_disk_change(driver, fixture, before["sha256"], 8.0,
                               "失活自然确认应结算保存等待并写盘原目标")
    notes["settled"] = settled
    write_json(driver.evidence / "wait-deactivation-proof.json", {
        "disk_at_wait": disk_wait,
        "disk_during_deactivation": disk_during,
        "frontmost_during": during,
        "expected_pid": driver.pid,
        "settlement": settled,
        "note": "等待中真实失活；平台自然提交 ni 后等待应结算写盘一次。"
                "失活瞬间的写盘时序无法从外部逐毫秒判定，状态机以关闭采样核对",
    })
    if not settled["changed"]:
        raise DriverError("失活自然确认后保存等待未结算写盘")

    # 重新激活后继续组合并提交一个候选字，证明原块上下文仍可用。
    ensure_our_frontmost(driver, "继续输入前确认前台")
    driver.inject_keys([KEY_H, KEY_A, KEY_O],
                       "失活提交后继续 HID 拼音组合 hao",
                       names=["h", "a", "o"])
    time.sleep(0.6)
    driver.snapshot()
    ensure_our_frontmost(driver, "空格提交前确认前台")
    driver.inject_keys([KEY_SPACE], "空格提交继续组合的候选字", names=["space"])
    time.sleep(1.2)
    driver.snapshot()

    time.sleep(2.2)
    auto_save = fixture_state(fixture)
    driver.call(["hotkey", "--key", "cmd+s"], "继续提交后显式保存")
    time.sleep(1.0)
    post_save = fixture_state(fixture)
    write_json(driver.evidence / "save-proof.json", {
        "baseline": before,
        "wait_settled_disk": settled,
        "before_explicit_save": auto_save,
        "after_explicit_save": post_save,
        "note": "等待结算与继续提交后的磁盘状态；逐字节核对在关闭采样后进行",
    })
    if post_save["sha256"] == before["sha256"]:
        raise DriverError("场景结束后磁盘仍是基线（提交未落盘）")

    write_json(driver.evidence / "scenario-notes.json", notes)
    close_document_and_wait(driver)
    verify_close_packet(driver.evidence, fixture)
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    session_result = verify_session(driver.evidence, baseline_bytes)
    session_timeline(driver.evidence)
    candidate = bytes.fromhex(session_result["candidate_hex"])
    verify_insertion(driver.evidence, baseline_bytes, b"ni" + candidate,
                     session_result["start_position"])
    write_json(driver.evidence / "store-after.json", {
        "active_generations": store_has_generations(),
        "note": "干净关闭后的恢复存储 Active 完整代；应清空",
    })
    if store_has_generations():
        raise DriverError("干净关闭后恢复存储仍有 Active 完整代")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.fixture is None or args.evidence is None:
        print("真实取证需要 --fixture 与 --evidence", file=sys.stderr)
        return 2
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
    driver = None
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
    print("等待失活取证完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())