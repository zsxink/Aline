#!/usr/bin/env python3
"""把逐样本 Metal System Trace 表转换成两个 M1 性能指标。

本模块只接受外部原生 trace 的原始表，不把 draw 提交后的 effect 或事件循环
间隔改名为屏幕呈现/主线程停顿：

- ``first_present``：内容 layout 结束后第一个真正到达屏幕的 display 帧。
  以 ``metal-command-buffer-frame-assignment`` 的
  commit-time/frame-number/present-surface-id 为帧身份，要求该 command
  buffer 有唯一 present request。``displayed-surfaces-interval`` 是一次
  合成扫描：event-label 列出本次扫描的次要帧，主帧由
  ``start - cpu-to-display-latency`` 反推。一个候选帧在本次扫描上屏，
  当且仅当它是主帧（latency 精确对应其 commit）或它的精确身份出现在
  event-label 中；主帧的 ``start-commit`` 必须精确等于 latency，次要帧的
  latency 属于主帧，不能用它校验次要帧的 commit。event-label 匹配要求
  身份后不再跟数字，避免 Surface 17 误匹配 Surface 171。首内容帧可能因
  drawable/合成时序未上屏（``has-present=0`` 或无 display 事件），此时取
  其后第一个真正上屏的 present 帧；draw effect 与 present request 都不算
  呈现。TOC start 的毫秒截断使换算 epoch 最多早于真值 1 ms，因此
  commit/request 允许最多早于 layout 结束 1 ms。request 只要求
  ``commit < request < display.start``：真实首个内容帧的 CA present
  request 可晚于 commit 2–5 ms，固定 2 ms 上限会拒绝真实上屏帧；display
  扫描开始是因果上界。draw 提交回调可能晚于实际上屏（回调在主线程排队
  执行），因此它只作诊断，不构成 display 的下界；display 事件本身才是
  呈现证据。
- ``main_thread_stall_ns``：主线程顶层 ``individual_iteration`` 的 recorded
  起止扣除 ``waiting_for_events`` 空闲等待后的最长连续忙碌段。

trace 的 TOC start-date 只有毫秒精度，转换到探针 epoch 时单侧不确定度按
1 ms 记录；它只影响窗口包含判断，不改变 trace 内已记录的时长。
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET

TRACE_SCHEMA = "aline-m1-trace-metrics/v1"
TRACE_UNCERTAINTY_NS = 1_000_000
MAIN_IDENT = re.compile(r"^r\d+_i\d+$")
WAIT_IDENT = re.compile(r"^r\d+_i\d+_w\d+$")


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _non_negative(value, field):
    _require(isinstance(value, int) and not isinstance(value, bool) and value >= 0,
             "缺少非负整数字段 " + field)
    return value


def _raw_int(row, name):
    value = row[name]["raw"]
    _require(isinstance(value, str) and value.isascii() and value.isdecimal(),
             "trace 字段 " + name + " 不是非负十进制整数")
    return int(value)


def _raw(row, name):
    return row[name]["raw"]


def _fmt(row, name):
    return row[name]["fmt"]


def _local(tag):
    return tag.rsplit("}", 1)[-1]


def parse_table(path):
    """解析 xctrace export 的 schema/row；列按 schema 顺序绑定，ref 解析到定义。"""
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as error:
        raise ValueError("trace 表不是有效 XML：" + str(error)) from error
    schema = None
    for element in root.iter():
        if _local(element.tag) == "schema":
            schema = element
            break
    _require(schema is not None, "trace 表缺少 schema")
    columns = []
    for col in schema:
        if _local(col.tag) != "col":
            continue
        mnemonic = next((child.text for child in col
                         if _local(child.tag) == "mnemonic"), None)
        _require(isinstance(mnemonic, str) and mnemonic, "trace schema 列缺少 mnemonic")
        columns.append(mnemonic)
    _require(columns, "trace schema 没有列")
    definitions = {}
    for element in root.iter():
        identifier = element.attrib.get("id")
        if identifier is not None:
            definitions[identifier] = element
    rows = []
    for row in root.iter():
        if _local(row.tag) != "row":
            continue
        # sentinel 也占一列；按原始子元素位置绑定，不能压缩。
        raw_children = list(row)
        _require(len(raw_children) == len(columns),
                 "trace 行列数与 schema 不同：%d != %d" % (len(raw_children), len(columns)))
        values = {}
        for column, child in zip(columns, raw_children):
            if _local(child.tag) == "sentinel":
                values[column] = {"raw": "", "fmt": ""}
                continue
            target = definitions.get(child.attrib.get("ref"), child)
            text = (target.text or "").strip()
            values[column] = {"raw": text, "fmt": target.attrib.get("fmt", "")}
        rows.append(values)
    return {"columns": columns, "rows": rows}


def parse_toc(path):
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as error:
        raise ValueError("trace TOC 不是有效 XML：" + str(error)) from error
    values = {}

    def walk(element):
        tag = _local(element.tag)
        if tag in ("start-date", "end-date", "duration", "end-reason",
                   "template-name", "instruments-version", "recording-mode"):
            text = (element.text or "").strip()
            if text and tag not in values:
                values[tag] = text
        for child in element:
            walk(child)

    walk(root)
    for field in ("start-date", "end-date", "template-name"):
        _require(field in values, "trace TOC 缺少 " + field)
    start = _parse_date(values["start-date"])
    end = _parse_date(values["end-date"])
    _require(end >= start, "trace TOC 结束早于开始")
    process = {"name": None, "pid": None, "type": None}
    for element in root.iter():
        if _local(element.tag) == "process" and process["pid"] is None:
            process = {"name": element.attrib.get("name"),
                       "pid": element.attrib.get("pid"),
                       "type": element.attrib.get("type")}
    _require(process["pid"] is not None and process["pid"].isdecimal(),
             "trace TOC 缺少目标 pid")
    return {"start_epoch_ns": start, "end_epoch_ns": end,
            "duration_seconds": values.get("duration"),
            "end_reason": values.get("end-reason"),
            "template": values["template-name"],
            "instruments_version": values.get("instruments-version"),
            "recording_mode": values.get("recording-mode"),
            "process_name": process["name"], "pid": int(process["pid"]),
            "process_type": process["type"]}


def _parse_date(text):
    try:
        parsed = dt.datetime.fromisoformat(text.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError("trace TOC 时间无效：" + text) from error
    _require(parsed.tzinfo is not None, "trace TOC 时间缺少时区")
    return int(parsed.timestamp()) * 1_000_000_000 + parsed.microsecond * 1000


def parse_tsv(path):
    records = []
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = {}
        for field in line.split("\t"):
            if "=" not in field:
                raise ValueError("第 %d 行字段格式无效" % line_no)
            key, value = field.split("=", 1)
            _require(key and key not in fields, "第 %d 行字段为空或重复" % line_no)
            fields[key] = value
        records.append(fields)
    return records


def _stage_rows(raw_rows, case):
    stages = {}
    diagnostics = {}
    for row in raw_rows:
        if row.get("case") != case:
            continue
        if row.get("kind") == "stage":
            _require(row.get("stage") not in stages,
                     "同一用例阶段重复：" + str(row.get("stage")))
            stages[row["stage"]] = row
        elif row.get("kind") == "diagnostic":
            diagnostics[row.get("metric")] = row
    return stages, diagnostics


def _pair_interval(rows, identifier, kind, event_field="event-type"):
    starts = [row for row in rows
              if _raw(row, "interval-type") == kind and _fmt(row, event_field) == "START"
              and _raw(row, "interval-identifier") == identifier]
    ends = [row for row in rows
            if _raw(row, "interval-type") == kind and _fmt(row, event_field) == "END"
            and _raw(row, "interval-identifier") == identifier]
    return starts, ends


def _pairs(rows, kind, identifier_pattern):
    """按 identifier 配对 START/END；同标识符多次出现时取时间上最近的 END。"""
    grouped = {}
    for row in rows:
        if _raw(row, "interval-type") != kind:
            continue
        identifier = _raw(row, "interval-identifier")
        if not identifier_pattern.fullmatch(identifier):
            continue
        event = _fmt(row, "event-type")
        _require(event in ("START", "END"), "runloop 事件类型未知：" + event)
        grouped.setdefault(identifier, {"START": [], "END": []})[event].append(row)
    pairs = []
    unpaired = []
    for identifier, events in grouped.items():
        starts = sorted(events["START"], key=lambda row: _raw_int(row, "timestamp"))
        ends = sorted(events["END"], key=lambda row: _raw_int(row, "timestamp"))
        for start in starts:
            start_ns = _raw_int(start, "timestamp")
            after = [end for end in ends if _raw_int(end, "timestamp") > start_ns]
            if not after:
                unpaired.append(start)
                continue
            end = after[0]
            _require(_raw(start, "timestamp-accuracy") == "recorded"
                     and _raw(end, "timestamp-accuracy") == "recorded",
                     "runloop 区间不是 recorded：" + identifier)
            pairs.append({"identifier": identifier, "start": start_ns,
                          "end": _raw_int(end, "timestamp"),
                          "start_row": start, "end_row": end})
    return pairs, unpaired


def _longest_busy(start, end, waits):
    merged = []
    for wait_start, wait_end in sorted(waits):
        if wait_start < start or wait_end > end or wait_end <= wait_start:
            raise ValueError("waiting_for_events 未完整落在主线程区间内")
        if merged and wait_start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], wait_end))
        else:
            merged.append((wait_start, wait_end))
    segments = []
    cursor = start
    for wait_start, wait_end in merged:
        if wait_start > cursor:
            segments.append((cursor, wait_start))
        cursor = max(cursor, wait_end)
    if cursor < end:
        segments.append((cursor, end))
    _require(segments, "主线程区间扣除等待后没有忙碌段")
    longest = max(segment_end - segment_start for segment_start, segment_end in segments)
    return longest, segments, sum(b - a for a, b in segments)


def _table_rows(table):
    if isinstance(table, dict) and "rows" in table:
        return table["rows"]
    return table


def derive(raw_rows, toc, tables, expected):
    """从原始表推导两个指标；任何缺失/多义/不精确都失败关闭。"""
    for field in ("run_id", "app_sha256", "spec_sha256", "pid", "case",
                  "condition", "temperature", "sample_id", "fixture_sha256"):
        _require(field in expected, "expected 缺少 " + field)
    _require(toc["pid"] == expected["pid"], "trace 目标 pid 与采样收据不同")
    _require(toc["process_name"] and "aline" in toc["process_name"],
             "trace 目标不是 aline")
    _require("Metal" in toc["template"], "trace 模板不是 Metal System Trace")
    start_epoch = _non_negative(toc["start_epoch_ns"], "trace start")
    end_epoch = _non_negative(toc["end_epoch_ns"], "trace end")
    _require(end_epoch > start_epoch, "trace 起止无效")

    stages, diagnostics = _stage_rows(raw_rows, expected["case"])
    for stage in ("read_decode", "snapshot", "parse_mapping", "main_thread_adopt",
                  "first_layout", "scroll", "mode_switch"):
        _require(stage in stages, "原始输出缺少阶段：" + stage)
    _require("first_draw_submission_ns" in diagnostics,
             "原始输出缺少 first_draw_submission_ns 诊断")
    for row in [*stages.values(), diagnostics["first_draw_submission_ns"]]:
        _require(row.get("run_id") == expected["run_id"], "原始阶段未绑定本轮 run_id")
        _require(row.get("case") == expected["case"], "原始阶段用例不同")
    for stage, row in stages.items():
        _require(row.get("condition") == expected["condition"],
                 stage + " 条件与 spec 不同")
        _require(row.get("temperature") == expected["temperature"],
                 stage + " 冷热与 spec 不同")
        _require(row.get("sample_id") == str(expected["sample_id"]),
                 stage + " 编号与 spec 不同")
    gate_rows = [row for row in raw_rows if row.get("kind") == "gate"]
    _require(len(gate_rows) == 1, "原始输出必须且只能有一条 gate 记录")
    gate = gate_rows[0]
    _require(gate.get("run_id") == expected["run_id"], "gate 未绑定本轮 run_id")
    gate_epoch = _non_negative(int(gate["epoch_ns"]), "gate epoch")
    _require(gate.get("pid") == str(expected["pid"]), "gate pid 与采样收据不同")
    open_start = _non_negative(int(diagnostics["first_draw_submission_ns"]["started_epoch_ns"]),
                               "open start")
    effect_end = _non_negative(int(diagnostics["first_draw_submission_ns"]["epoch_ns"]),
                               "effect end")
    layout_start = _non_negative(int(stages["first_layout"]["started_epoch_ns"]),
                                 "layout start")
    layout_end = _non_negative(int(stages["first_layout"]["epoch_ns"]), "layout end")
    virtual_items = _non_negative(int(stages["first_layout"]["virtual_items"]),
                                  "virtual_items")
    _require(layout_start < layout_end <= effect_end,
             "首内容帧 layout/effect 边界顺序无效")
    window_end = max(_non_negative(int(row["epoch_ns"]), "stage end")
                     for row in stages.values())
    _require(start_epoch + TRACE_UNCERTAINTY_NS <= gate_epoch <= open_start,
             "trace 起点没有在 gate 前覆盖测量窗口")
    _require(window_end <= end_epoch, "trace 结束早于最后阶段")
    _require(end_epoch - start_epoch > TRACE_UNCERTAINTY_NS,
             "trace 持续时间不足")

    # 1) 首内容帧：内容 layout 结束后第一个真正到达屏幕的 display 帧。
    #    帧身份来自 assignment；首内容帧可能未上屏（has-present=0 或无
    #    display），此时取其后第一个上屏的 present 帧。commit/request 允许
    #    最多早于 layout_end 1 ms（TOC start 毫秒截断的单侧不确定度）。
    #    draw 提交回调可能晚于实际上屏，只作诊断，不构成 display 的下界。
    #
    #    display 区间是合成扫描：event-label 列出本次扫描的次要帧，主帧
    #    由 start - cpu-to-display-latency 反推。候选帧在本次扫描上屏，
    #    当且仅当它是主帧（latency 精确对应其 commit）或精确身份出现在
    #    event-label 中。主帧必须满足 start-commit == latency；次要帧的
    #    latency 属于主帧，不能用它校验次要帧的 commit。
    #    request 只要求 commit < request < display.start：真实首个内容帧的
    #    CA present request 可晚于 commit 2–5 ms，固定 2 ms 上限会拒绝
    #    真实上屏帧；display 扫描开始是因果上界。
    assignments = []
    for row in _table_rows(tables["assignment"]):
        if _raw(row, "pid") != str(expected["pid"]):
            continue
        if _raw(row, "has-present") != "1":
            continue
        commit = _raw_int(row, "commit-time")
        cmdbuffer = _raw_int(row, "cmdbuffer-id")
        frame = _raw_int(row, "frame-number")
        surface = _raw_int(row, "present-surface-id")
        _require(_raw(row, "present-surface-id") != "",
                 "present-surface-id 缺失")
        # layout 结束前的帧不是首内容帧候选；它们的 request 时序不参与
        # 本指标，不能因为更早的帧抖动而拒绝整个样本。
        if start_epoch + commit + TRACE_UNCERTAINTY_NS < layout_end:
            continue
        requests = [request for request in _table_rows(tables["request"])
                    if _raw(request, "cmdbuffer-id") == str(cmdbuffer)
                    and _request_belongs_to(request, expected["pid"])]
        _require(len(requests) == 1,
                 "command buffer 没有唯一 present request：%s" % _fmt(row, "cmdbuffer-id"))
        request = requests[0]
        request_ns = _raw_int(request, "timestamp")
        _require(commit < request_ns,
                 "present request 不晚于 commit")
        request_epoch = start_epoch + request_ns
        commit_epoch = start_epoch + commit
        _require(request_epoch + TRACE_UNCERTAINTY_NS >= layout_end,
                 "present request 早于 layout 结束")
        assignments.append({"row": row, "request": request, "commit_ns": commit,
                            "request_ns": request_ns, "commit_epoch": commit_epoch,
                            "request_epoch": request_epoch, "frame": frame,
                            "surface": surface,
                            "cmdbuffer": _fmt(row, "cmdbuffer-id")})
    _require(assignments,
             "首内容帧没有 layout 结束后的 present command buffer")
    # display 区间的 latency 必须能归属到 assignment 表中的某个真实帧；
    # 否则该区间的主帧身份不可核验，不能把 label 里的候选帧当作已上屏。
    assignment_commits = set()
    for row in _table_rows(tables["assignment"]):
        if _raw(row, "commit-time"):
            assignment_commits.add(_raw_int(row, "commit-time"))
    displays = []
    for item in assignments:
        identity = "aline (%d):Frame %d:Surface %d" % (
            expected["pid"], item["frame"], item["surface"])
        # xctrace 的 event-label 对 frame/surface 使用千位分隔符
        # （如 Surface 1,032）；去掉逗号后按十进制整数匹配身份。身份后
        # 不允许再跟数字，避免 Surface 17 误匹配 Surface 171。
        identity_pattern = re.compile(re.escape(identity) + r"(?![0-9])")
        for display in _table_rows(tables["display"]):
            label = _fmt(display, "event-label").replace(",", "")
            labeled = identity_pattern.search(label) is not None
            # 无 latency 的 display 行只可能是其他表面的空扫描；若它
            # 明确列出了本帧，则证据不完整，失败关闭。
            if not _raw(display, "cpu-to-display-latency"):
                _require(not labeled,
                         "display 区间缺少 cpu-to-display-latency")
                continue
            display_ns = _raw_int(display, "start")
            latency = _raw_int(display, "cpu-to-display-latency")
            primary_commit_ns = display_ns - latency
            primary = primary_commit_ns == item["commit_ns"]
            if not (primary or labeled):
                continue
            _require(primary_commit_ns >= 0,
                     "display 主帧 commit 早于 trace 起点")
            _require(primary_commit_ns in assignment_commits,
                     "display 主帧 commit 不在 assignment 表中")
            _require(display_ns > item["request_ns"],
                     "display start 不晚于 present request")
            if primary:
                _require(display_ns - item["commit_ns"] == latency,
                         "primary display 的 latency 与 start-commit 不一致")
            displays.append({"assignment": item, "display_ns": display_ns,
                             "latency": latency, "primary": primary,
                             "primary_commit_ns": primary_commit_ns,
                             "labeled": labeled, "display": display})
    _require(displays, "首内容帧没有 displayed-surfaces-interval")
    earliest = min(item["display_ns"] for item in displays)
    first = [item for item in displays if item["display_ns"] == earliest]
    # 同一次合成扫描可以包含多个候选帧，呈现时刻相同；取主帧优先、
    # 其次帧号/表面号最小的作为记录身份。同一时刻出现在不同显示器
    # 才是无法归一的歧义。
    display_names = {_raw(item["display"], "display-name") for item in first}
    _require(len(display_names) == 1,
             "首内容帧 display 不是唯一扫描：%d" % len(display_names))
    record_keys = [(item["display_ns"], item["assignment"]["frame"],
                    item["assignment"]["surface"], item["primary"])
                   for item in first]
    _require(len(record_keys) == len(set(record_keys)),
             "首内容帧 display 出现重复记录")
    first.sort(key=lambda item: (not item["primary"],
                                 item["assignment"]["frame"],
                                 item["assignment"]["surface"]))
    chosen = first[0]
    item = chosen["assignment"]
    frame = item["frame"]
    surface = item["surface"]
    commit_ns = item["commit_ns"]
    request_ns = item["request_ns"]
    display_ns = chosen["display_ns"]
    latency = chosen["latency"]
    primary = chosen["primary"]
    primary_commit_ns = chosen["primary_commit_ns"]
    first_present_epoch = start_epoch + display_ns
    first_present_duration = first_present_epoch - open_start
    _require(first_present_duration > 0, "first_present 时长为非正数")
    # draw 提交回调可能晚于实际上屏（回调在主线程排队执行），不能作为
    # display 的下界；帧身份与上屏时刻由 commit/request/display 链锁定。

    # 2) 主线程顶层 iteration：窗口内最长连续忙碌段，排除 waiting_for_events。
    iterations, unpaired_iterations = _pairs(_table_rows(tables["runloop"]),
                                             "individual_iteration", MAIN_IDENT)
    waits, unpaired_waits = _pairs(_table_rows(tables["runloop"]),
                                   "waiting_for_events", WAIT_IDENT)
    main_waits = [wait for wait in waits
                  if _fmt(wait["start_row"], "is-main") == "Yes"
                  and _thread_belongs_to(_fmt(wait["start_row"], "thread"),
                                         expected["pid"])]
    window_start_trace = open_start - start_epoch
    window_end_trace = window_end - start_epoch
    _require(window_start_trace >= TRACE_UNCERTAINTY_NS,
             "trace 起点与打开请求的余量不足")
    _require(window_end_trace <= end_epoch - start_epoch,
             "trace 未覆盖最后阶段")
    for start in unpaired_iterations:
        if (_fmt(start, "is-main") != "Yes"
                or not _thread_belongs_to(_fmt(start, "thread"), expected["pid"])):
            continue
        start_ns = _raw_int(start, "timestamp")
        _require(not (window_start_trace + TRACE_UNCERTAINTY_NS <= start_ns
                      <= window_end_trace),
                 "测量窗口内主线程区间缺少结束："
                 + _raw(start, "interval-identifier"))
    for start in unpaired_waits:
        if (_fmt(start, "is-main") != "Yes"
                or not _thread_belongs_to(_fmt(start, "thread"), expected["pid"])):
            continue
        start_ns = _raw_int(start, "timestamp")
        _require(not (window_start_trace <= start_ns <= window_end_trace),
                 "测量窗口内等待区间缺少结束："
                 + _raw(start, "interval-identifier"))
    candidates = []
    for iteration in iterations:
        if _fmt(iteration["start_row"], "is-main") != "Yes":
            continue
        _require(_thread_belongs_to(_fmt(iteration["start_row"], "thread"),
                                    expected["pid"]),
                 "iteration 不属于目标进程主线程")
        if iteration["start"] < window_start_trace + TRACE_UNCERTAINTY_NS:
            continue
        if iteration["end"] > window_end_trace:
            continue
        if iteration["end"] <= iteration["start"]:
            continue
        overlapping = [(wait["start"], wait["end"]) for wait in main_waits
                       if wait["start"] < iteration["end"]
                       and wait["end"] > iteration["start"]]
        inside = [(wait_start, wait_end) for wait_start, wait_end in overlapping
                  if wait_start >= iteration["start"] and wait_end <= iteration["end"]]
        _require(len(inside) == len(overlapping),
                 "waiting_for_events 跨越主线程区间边界：" + iteration["identifier"])
        longest, segments, total = _longest_busy(iteration["start"],
                                                 iteration["end"], inside)
        candidates.append({
            "interval_identifier": iteration["identifier"],
            "interval_start_trace_ns": iteration["start"],
            "interval_end_trace_ns": iteration["end"],
            "interval_duration_ns": iteration["end"] - iteration["start"],
            "waiting_ns": sum(b - a for a, b in inside),
            "longest_busy_ns": longest,
            "total_busy_ns": total,
            "segments": [[a, b] for a, b in segments],
            "thread": _fmt(iteration["start_row"], "thread"),
            "timestamp_accuracy": "recorded",
            "is_main": True,
        })
    _require(candidates, "测量窗口内没有主线程 individual_iteration")
    chosen_stall = max(candidates, key=lambda item: item["longest_busy_ns"])
    _require(chosen_stall["longest_busy_ns"] > 0, "主线程最长忙碌段必须为正")
    _require(all(item["longest_busy_ns"] <= chosen_stall["longest_busy_ns"]
                 for item in candidates), "主线程停顿候选不是最大值")

    raw_sha = {}
    for name, table in tables.items():
        if isinstance(table, dict) and table.get("sha256"):
            raw_sha[name + ".xml"] = table["sha256"]
    return {
        "schema": TRACE_SCHEMA,
        "run_id": expected["run_id"],
        "case": expected["case"],
        "condition": expected["condition"],
        "temperature": expected["temperature"],
        "sample_id": expected["sample_id"],
        "pid": expected["pid"],
        "app_sha256": expected["app_sha256"],
        "spec_sha256": expected["spec_sha256"],
        "fixture_sha256": expected["fixture_sha256"],
        "trace": {
            "template": toc["template"],
            "start_epoch_ns": start_epoch,
            "end_epoch_ns": end_epoch,
            "start_uncertainty_ns": TRACE_UNCERTAINTY_NS,
            "process_name": toc["process_name"],
            "process_type": toc["process_type"],
            "pid": toc["pid"],
            "end_reason": toc.get("end_reason"),
            "recording_mode": toc.get("recording_mode"),
            "raw_sha256": raw_sha,
        },
        "first_present": {
            "stage": "first_present",
            "started_epoch_ns": open_start,
            "epoch_ns": first_present_epoch,
            "duration_ns": first_present_duration,
            "timing_origin": "trace-display-chain",
            "timing_clock": "trace-relative",
            "trace_uncertainty_ns": TRACE_UNCERTAINTY_NS,
            "virtual_items": virtual_items,
            "layout_start_epoch_ns": layout_start,
            "layout_end_epoch_ns": layout_end,
            "effect_end_epoch_ns": effect_end,
            "content_frame": {
                "command_buffer": item["cmdbuffer"],
                "frame_number": frame,
                "surface_id": surface,
                "commit_trace_ns": commit_ns,
                "commit_epoch_ns": item["commit_epoch"],
                "request_trace_ns": request_ns,
                "request_epoch_ns": item["request_epoch"],
                "display_start_trace_ns": display_ns,
                "display_start_epoch_ns": first_present_epoch,
                "cpu_to_display_latency_ns": latency,
                "primary": primary,
                "primary_commit_trace_ns": primary_commit_ns,
                "primary_commit_epoch_ns": start_epoch + primary_commit_ns,
            },
        },
        "main_thread_stall": {
            "stage": "main_thread_stall",
            "duration_ns": chosen_stall["longest_busy_ns"],
            "timing_origin": "trace-runloop-busy",
            "timing_clock": "trace-relative",
            "trace_uncertainty_ns": TRACE_UNCERTAINTY_NS,
            "window_start_epoch_ns": open_start,
            "window_end_epoch_ns": window_end,
            "window_start_trace_ns": window_start_trace,
            "window_end_trace_ns": window_end_trace,
            "interval_identifier": chosen_stall["interval_identifier"],
            "interval_start_trace_ns": chosen_stall["interval_start_trace_ns"],
            "interval_end_trace_ns": chosen_stall["interval_end_trace_ns"],
            "interval_duration_ns": chosen_stall["interval_duration_ns"],
            "waiting_ns": chosen_stall["waiting_ns"],
            "longest_busy_ns": chosen_stall["longest_busy_ns"],
            "total_busy_ns": chosen_stall["total_busy_ns"],
            "segments": chosen_stall["segments"],
            "thread": chosen_stall["thread"],
            "timestamp_accuracy": "recorded",
            "is_main": True,
            "candidates": candidates,
        },
        "observation_limits": [
            "trace TOC start-date 为毫秒精度；epoch 换算单侧不确定度 <= 1 ms，只影响窗口包含判断。",
            "first_present 取 displayed-surfaces-interval 的 start，不声称像素已进入人眼。",
            "main_thread_stall_ns 是窗口内主线程 runloop 忙碌段，不含 waiting_for_events；不是 hang 阈值事件。",
            "trace 录制本身有开销，阶段耗时与无 trace 矩阵不可直接混比。",
        ],
    }


def _request_belongs_to(row, pid):
    process = _fmt(row, "process")
    thread = _fmt(row, "thread")
    return ("(%d)" % pid) in process or ("pid: %d" % pid) in thread


def _thread_belongs_to(thread, pid):
    return ("aline, pid: %d)" % pid) in thread or ("aline (%d)" % pid) in thread


def _sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_tables(paths):
    tables = {}
    for name, path in paths.items():
        table = parse_table(path)
        table["sha256"] = _sha256(path)
        tables[name] = table
    return tables


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", type=Path)
    parser.add_argument("--toc", type=Path)
    parser.add_argument("--assignment", type=Path)
    parser.add_argument("--request", type=Path)
    parser.add_argument("--display", type=Path)
    parser.add_argument("--runloop", type=Path)
    parser.add_argument("--expected", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    try:
        for field in ("raw", "toc", "assignment", "request", "display", "runloop",
                      "expected", "output"):
            if getattr(args, field) is None:
                parser.error("需要 --" + field)
        expected = json.loads(args.expected.read_text(encoding="utf-8"))
        _require(isinstance(expected, dict), "expected 必须是对象")
        tables = load_tables({
            "assignment": args.assignment, "request": args.request,
            "display": args.display, "runloop": args.runloop,
        })
        metrics = derive(parse_tsv(args.raw), parse_toc(args.toc), tables, expected)
        args.output.write_text(json.dumps(metrics, ensure_ascii=False, indent=2) + "\n",
                               encoding="utf-8")
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print("trace-metrics: 未通过：" + str(error), file=sys.stderr)
        return 1
    print("trace-metrics: 已写入 " + args.output.name)
    return 0


def _v(raw, fmt=""):
    return {"raw": raw, "fmt": fmt or raw}


def _synthetic():
    start = 1_794_170_269_973_000_000
    gate = start + 1_155_000_000
    open_start = start + 1_155_554_000
    layout_start = start + 1_165_703_000
    layout_end = start + 1_167_058_000
    effect_end = start + 1_170_088_000
    commit = 1_169_800_875
    request = 1_169_963_416
    display = 1_192_966_333
    window_end = start + 1_175_329_000
    trace_end = start + 1_590_164_000

    def stage(name, started, finished):
        return {"kind": "stage", "run_id": "run", "case": "case", "condition": "32KiB",
                "temperature": "cold", "sample_id": "0", "stage": name,
                "started_epoch_ns": str(started), "epoch_ns": str(finished),
                "virtual_items": "16" if name == "first_layout" else "0"}

    raw = [
        {"kind": "run", "run_id": "run"},
        {"kind": "gate", "run_id": "run", "pid": "1", "epoch_ns": str(gate)},
        stage("read_decode", open_start - 10_000_000, open_start - 5_000_000),
        stage("snapshot", open_start - 5_000_000, open_start - 4_000_000),
        stage("parse_mapping", open_start - 4_000_000, open_start - 1_000_000),
        stage("main_thread_adopt", open_start - 1_000_000, open_start - 500_000),
        stage("first_layout", layout_start, layout_end),
        {"kind": "diagnostic", "run_id": "run", "case": "case",
         "metric": "first_draw_submission_ns", "value": str(effect_end - open_start),
         "started_epoch_ns": str(open_start), "epoch_ns": str(effect_end)},
        stage("scroll", effect_end + 10_000_000, effect_end + 12_000_000),
        stage("mode_switch", window_end - 2_000_000, window_end),
        {"kind": "method"}, {"kind": "done"},
    ]
    toc = {"start_epoch_ns": start, "end_epoch_ns": trace_end,
           "duration_seconds": "1.590164", "end_reason": "Target app exited",
           "template": "Metal System Trace", "instruments_version": "27.2",
           "recording_mode": "Deferred", "process_name": "aline",
           "pid": 1, "process_type": "attached"}

    def assignment(cmdbuffer, commit_ns, frame, surface):
        return {"timestamp": _v(str(commit_ns - 64_000), ""),
                "cmdbuffer-id": _v(str(cmdbuffer), hex(cmdbuffer)),
                "commit-time": _v(str(commit_ns), "{:,}".format(commit_ns)),
                "commit-id": _v("1"), "frame-number": _v(str(frame)),
                "frame-color": _v("0"), "cmdbuffer-index": _v("0"),
                "has-present": _v("1"), "is-compositor": _v("0"),
                "type-name": _v("Metal"), "present-surface-id": _v(str(surface), hex(surface)),
                "compositor-cmdbuffer-id": _v("0"), "xr-frame": _v("0"),
                "pid": _v("1"), "process": _v("aline (1)")}

    def request_row(cmdbuffer, timestamp):
        return {"timestamp": _v(str(timestamp)), "cmdbuffer-id": _v(str(cmdbuffer)),
                "surface-id": _v("99"), "at-time": _v("0"),
                "minimal-duration": _v("0"),
                "thread": _v("Main Thread (0x1) (aline, pid: 1)"),
                "process": _v("aline (1)")}

    def display_row(frame, surface, start_ns, commit_ns):
        return {"start": _v(str(start_ns)), "duration": _v("8333167"),
                "cpu-to-display-latency": _v(str(start_ns - commit_ns)),
                "display-name": _v("Built-In Display"),
                "connection-UUID": _v("1"), "surface-id": _v("100"),
                "pixel-format": _v(""), "color": _v("1002"),
                "event-priority": _v("1"), "category": _v("Display"),
                "event-depth": _v("0"), "direct-to-display": _v("0"),
                "resolution": _v("3024 1964"),
                "detachment-reason": _v(""),
                "detachment-suggestion": _v(""),
                "event-label": _v("Surface 100 (aline (1):Frame {:,}:Surface {:,})"
                                  .format(frame, surface))}

    def runloop(identifier, kind, event, timestamp, main="Yes"):
        return {"timestamp": _v(str(timestamp)), "timestamp-accuracy": _v("recorded"),
                "interval-type": _v(kind), "event-type": _v(event, event.upper()),
                "interval-identifier": _v(identifier), "nesting-level": _v("1"),
                "mode": _v("default"), "is-main": _v(main, main),
                "thread": _v("Main Thread (0x1) (aline, pid: 1)"),
                "runloop-pointer": _v("1"), "timeout": _v("0"), "other-arg": _v("0")}

    tables = {
        "assignment": [assignment(6992724031, 1_160_170_833, 3, 337),
                       assignment(6992724033, commit, 4, 1032)],
        "request": [request_row(6992724031, 1_160_724_708),
                    request_row(6992724033, request)],
        "display": [display_row(3, 337, 1_184_633_166, 1_160_170_833),
                    display_row(4, 1032, display, commit)],
        "runloop": [
            runloop("r18_i1", "individual_iteration", "START", 1_164_428_000),
            runloop("r18_i1_w1", "waiting_for_events", "START", 1_164_428_000),
            runloop("r18_i1_w1", "waiting_for_events", "END", 1_165_490_000),
            runloop("r18_i1", "individual_iteration", "END", 1_169_946_000),
            runloop("r19_i1", "individual_iteration", "START", 1_169_950_000),
            runloop("r19_i1_w1", "waiting_for_events", "START", 1_170_000_000),
            runloop("r19_i1_w1", "waiting_for_events", "END", 1_170_001_000),
            runloop("r19_i1", "individual_iteration", "END", 1_174_434_000),
        ],
    }
    expected = {"run_id": "run", "app_sha256": "a" * 64, "spec_sha256": "b" * 64,
                "pid": 1, "case": "case", "condition": "32KiB",
                "temperature": "cold", "sample_id": 0, "fixture_sha256": "c" * 64}
    return raw, toc, tables, expected


def self_test():
    raw, toc, tables, expected = _synthetic()
    metrics = derive(raw, toc, tables, expected)
    fp = metrics["first_present"]
    assert fp["epoch_ns"] - fp["started_epoch_ns"] == fp["duration_ns"]
    assert fp["content_frame"]["frame_number"] == 4
    assert fp["content_frame"]["surface_id"] == 1032
    assert fp["content_frame"]["display_start_trace_ns"] - \
        fp["content_frame"]["commit_trace_ns"] == \
        fp["content_frame"]["cpu_to_display_latency_ns"]
    assert fp["content_frame"]["primary"] is True
    assert fp["content_frame"]["primary_commit_trace_ns"] == \
        fp["content_frame"]["commit_trace_ns"]
    stall = metrics["main_thread_stall"]
    assert stall["interval_identifier"] == "r18_i1"
    assert stall["longest_busy_ns"] == 4_456_000
    assert stall["waiting_ns"] == 1_062_000
    assert stall["duration_ns"] == stall["longest_busy_ns"]

    def expect_fail(label, mutate):
        raw2, toc2, tables2, expected2 = _synthetic()
        mutate(raw2, toc2, tables2, expected2)
        try:
            derive(raw2, toc2, tables2, expected2)
        except (ValueError, KeyError):
            return
        raise AssertionError(label + " accepted")

    def expect_ok(label, mutate):
        raw2, toc2, tables2, expected2 = _synthetic()
        mutate(raw2, toc2, tables2, expected2)
        try:
            derive(raw2, toc2, tables2, expected2)
        except (ValueError, KeyError) as error:
            raise AssertionError(label + " rejected: " + str(error))

    expect_fail("missing gate", lambda r, t, tb, e: r.pop(1))
    expect_fail("trace starts after gate",
                lambda r, t, tb, e: r[1].update(
                    epoch_ns=str(t["start_epoch_ns"] + 500_000)))
    expect_fail("trace ends before window",
                lambda r, t, tb, e: t.update(end_epoch_ns=t["start_epoch_ns"] + 1_000_000))
    expect_fail("pid mismatch", lambda r, t, tb, e: t.update(pid=2))
    expect_fail("no content commit",
                lambda r, t, tb, e: tb["assignment"].pop(1))
    expect_fail("two content commits",
                lambda r, t, tb, e: tb["assignment"].append(dict(tb["assignment"][1])))
    expect_fail("request before layout",
                lambda r, t, tb, e: (
                    tb["assignment"][1].update(**{"commit-time": _v("1165058000")}),
                    tb["request"][1].update(timestamp=_v("1165558000")),
                    tb["display"][1].update(
                        **{"cpu-to-display-latency": _v("27908333")})))
    expect_fail("display missing",
                lambda r, t, tb, e: tb["display"].pop(1))
    expect_fail("display frame mismatch",
                lambda r, t, tb, e: tb["display"][1].update(
                    **{"event-label": _v("Surface 100 (aline (1):Frame 5:Surface 99)"),
                       "cpu-to-display-latency": _v(
                           str(1_192_966_333 - 1_160_170_833))}))
    # draw 提交回调可能晚于上屏，display 早于回调结束仍是有效证据。
    expect_ok("display before effect callback",
              lambda r, t, tb, e: r[7].update(
                  epoch_ns=str(t["start_epoch_ns"] + 1_195_000_000)))
    # layout 结束前的帧不参与首内容帧指标，其时序抖动不得拒绝样本。
    expect_ok("pre-layout request delay ignored",
              lambda r, t, tb, e: tb["request"][0].update(
                  timestamp=_v(str(1_160_170_833 + 2_500_000))))
    # 首个内容帧的 CA present request 可晚于 commit 超过 2 ms；display
    # 扫描开始才是因果上界，不能被旧的固定上限拒绝。
    expect_ok("candidate request beyond old 2ms bound",
              lambda r, t, tb, e: tb["request"][1].update(
                  timestamp=_v(str(1_169_800_875 + 3_000_000))))
    expect_fail("candidate request after display",
                lambda r, t, tb, e: tb["request"][1].update(
                    timestamp=_v(str(1_192_966_334))))
    expect_fail("candidate request missing",
                lambda r, t, tb, e: tb["request"].pop(1))
    expect_fail("display before commit",
                lambda r, t, tb, e: tb["display"][1].update(
                    start=_v(str(1_169_800_000)),
                    **{"cpu-to-display-latency": _v("1000")}))
    expect_fail("duplicate display",
                lambda r, t, tb, e: tb["display"].append(dict(tb["display"][1])))
    # 主帧 latency 必须能归属到 assignment 表；差 1 ns 也不得冒充。
    expect_fail("latency mismatch",
                lambda r, t, tb, e: tb["display"][1].update(
                    **{"cpu-to-display-latency": _v("23165459")}))
    # 次要帧：label 列出本帧，latency 属于同一扫描的主帧（frame 3）。
    expect_ok("secondary composite frame",
              lambda r, t, tb, e: tb["display"][1].update(
                  **{"cpu-to-display-latency": _v(
                      str(1_192_966_333 - 1_160_170_833))}))
    # label 身份后跟数字时不得按前缀误匹配（Surface 1032 vs 10321）。
    expect_fail("label surface prefix collision",
                lambda r, t, tb, e: tb["display"][1].update(
                    **{"event-label": _v(
                        "Surface 100 (aline (1):Frame 4:Surface 10321)"),
                       "cpu-to-display-latency": _v(
                           str(1_192_966_333 - 1_160_170_833))}))
    # 主帧不在 assignment 表中的 display 区间不可核验。
    expect_fail("primary frame not in assignment",
                lambda r, t, tb, e: tb["display"][1].update(
                    **{"cpu-to-display-latency": _v(
                        str(1_192_966_333 - 1_169_800_874))}))
    expect_fail("no main iteration",
                lambda r, t, tb, e: tb.update(runloop=[]))
    expect_fail("estimated iteration",
                lambda r, t, tb, e: tb["runloop"][0].update(
                    **{"timestamp-accuracy": _v("estimated")}))
    def move_iterations_outside(r, t, tb, e):
        for row in tb["runloop"]:
            if row["interval-type"]["raw"] != "individual_iteration":
                continue
            row["timestamp"] = _v("1150000000" if row["event-type"]["fmt"] == "START"
                                  else "1154000000")

    expect_fail("iteration outside window", move_iterations_outside)
    expect_fail("waiting outside iteration",
                lambda r, t, tb, e: tb["runloop"][2].update(
                    timestamp=_v(str(1_174_500_000))))
    expect_fail("missing stage",
                lambda r, t, tb, e: r.pop(9))
    expect_fail("wrong sample id",
                lambda r, t, tb, e: e.update(sample_id=1))
    # XML 解析正例：schema 顺序 + ref 解析 + sentinel 占位。
    import tempfile
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "table.xml"
        path.write_text(
            '<trace-query-result><node><schema name="x">'
            '<col><mnemonic>timestamp</mnemonic></col>'
            '<col><mnemonic>flag</mnemonic></col>'
            '<col><mnemonic>pid</mnemonic></col>'
            '</schema><row><start-time id="1" fmt="00:00.001.000">1000</start-time>'
            '<boolean id="2" fmt="Yes">1</boolean><sentinel /></row>'
            '<row><start-time ref="1" /><boolean ref="2" />'
            '<uint64 id="3" fmt="7">7</uint64></row>'
            '</node></trace-query-result>', encoding="utf-8")
        table = parse_table(path)
        assert table["columns"] == ["timestamp", "flag", "pid"]
        assert table["rows"][0]["timestamp"]["raw"] == "1000"
        assert table["rows"][0]["flag"]["fmt"] == "Yes"
        assert table["rows"][0]["pid"]["raw"] == ""
        assert table["rows"][1]["timestamp"]["raw"] == "1000"
        assert table["rows"][1]["pid"]["raw"] == "7"
        path.write_text("<broken>", encoding="utf-8")
        try:
            parse_table(path)
        except ValueError:
            pass
        else:
            raise AssertionError("broken XML accepted")
    print("trace-metrics self-test: 通过")


if __name__ == "__main__":
    raise SystemExit(main())
