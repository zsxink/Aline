#!/usr/bin/env python3
"""M1 只读视图的可复跑测量与核对入口。

三个套件：

- ``performance``：读取 Release 测量 JSONL，校验样本数/阶段归属/运行身份，
  汇总 nearest-rank P50/P95、最长主线程停顿、峰值内存与虚拟实例数；
  错误、缺指标、取消、旧运行或跨运行混合数据一律非零退出，不伪造成功。
- ``window-baseline``：输出 V-09 人工观测清单并核对记录；缺少人工结果或
  结果不完整时非零退出，脚本不能替代真实窗口观测。
- ``readonly-bytes``：生成 BOM/混合行尾/孤立 CR/空白/无尾换行夹具，核对
  打开→滚动→命中→主动跳转→双视图往返→关闭前后的磁盘字节与内存源码哈希；
  任一保真失败、出现草稿或历史即非零退出。

所有输出经 ``validation_output.output_directory`` 落在 ``logs/validation`` 内，
文本与结构化证据经 ``evidence_paths`` 归一化，不写入本机绝对路径。
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import re
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))

from evidence_paths import write_json, write_text
from validation_output import output_directory

ROOT = Path(__file__).resolve().parent.parent
PERF_SCHEMA = "aline-m1-perf/v1"
BYTES_SCHEMA = "aline-m1-readonly-bytes/v1"
BYTES_FIXTURE_SCHEMA = "aline-m1-readonly-bytes-fixtures/v1"
WINDOW_SCHEMA = "aline-m1-window-baseline/v1"
MIN_SAMPLES = 20
STAGES = (
    "read_decode",
    "snapshot",
    "parse_mapping",
    "first_layout",
    "main_thread_adopt",
    "first_present",
    "mode_switch",
    "scroll",
)
TEMPERATURES = ("cold", "hot")
CONDITIONS = (
    "32KiB",
    "128KiB",
    "1MiB",
    "4MiB",
    ">4MiB",
    "many-short-blocks",
    "single-giant-block",
    "giant-table",
    "dense-entities",
)
VIRTUAL_ITEM_STAGES = ("first_layout", "first_present", "scroll")
NON_MEASURED = (
    "编辑、输入法、查找与资源后端时延不适用 M1，未测且不填零。",
    "Windows 10、Intel Mac 与 Linux 未测，不以 macOS arm64 结果推断其他平台。",
    "完整设置、菜单与主题矩阵不在 M1，未测。",
    "20 次是首轮采样方案，不是新增发布性能门槛。",
)
REQUIRED_STEPS = (
    "open",
    "scroll",
    "hit",
    "jump",
    "visual_to_source",
    "source_to_visual",
    "close",
)
WINDOW_ITEMS = (
    ("no-document", "无文档态：工具栏/内容区/状态栏可见，状态栏无统计。"),
    ("loading", "加载态：打开大文件期间有可见反馈，不冻结。"),
    ("normal", "正常阅读态：内容、统计与模式标签完整。"),
    ("read-failure", "读取失败：可见错误，不崩溃、不留下半开文档。"),
    ("cancel", "取消打开或取消等待定位：无残留状态。"),
    ("fallback", ">4 MiB 回退 Source：显示原因，不伪装成可视化成功。"),
    ("narrow-900", "窗口宽 ≤900：内边距收缩，内容不横向溢出、不被裁切。"),
    ("narrow-600", "窗口宽 ≤600：内边距收缩，内容不横向溢出、不被裁切。"),
    ("high-dpi", "高 DPI/缩放：文字、滚动与命中正常。"),
    ("long-english-name", "长英文文件名：不破坏布局与状态栏。"),
    ("open-keyboard", "打开文件与双视图切换键盘可达，焦点可见。"),
    ("outline-keyboard", "大纲/脚注导航键盘激活，回跳有效。"),
    ("feedback-keyboard", "反馈/错误可被键盘读取，焦点归还。"),
    ("chinese-ui", "界面语言只核对中文，无缺 key、无插值漂移。"),
)
BYTES_CASES = {
    "bom": {
        "file": "bom.md",
        "bytes": b"\xef\xbb\xbf# BOM\r\n\r\n\xe4\xb8\xad\xe6\x96\x87\xe6\xae\xb5\xe8\x90\xbd\xe3\x80\x82\r\n",
        "note": "UTF-8 BOM + CRLF 中文正文",
    },
    "mixed-eol": {
        "file": "mixed-eol.md",
        "bytes": b"first\r\nsecond\nthird\rfourth\r\n",
        "note": "CRLF/LF/孤立 CR 混合",
    },
    "lone-cr": {
        "file": "lone-cr.md",
        "bytes": b"alpha\rbeta\rgamma",
        "note": "仅孤立 CR 分隔",
    },
    "whitespace-only": {
        "file": "whitespace-only.md",
        "bytes": b"   \n\t\n  \t  \n",
        "note": "仅空白与空行",
    },
    "no-trailing-newline": {
        "file": "no-trailing-newline.md",
        "bytes": "末尾没有换行".encode("utf-8"),
        "note": "无尾换行",
    },
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def _utc_now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def nearest_rank(values, percentile):
    """nearest-rank 分位数：升序第 ceil(p/100*n) 个值，1 起算。"""
    if not values:
        raise ValueError("空序列没有分位数")
    if not 0 < percentile <= 100:
        raise ValueError("分位数必须在 (0, 100] 内")
    ordered = sorted(values)
    if float(percentile).is_integer():
        # 整数百分位用整数算术，避免 0.95*n 的浮点误差改变名次。
        rank = (int(percentile) * len(ordered) + 99) // 100
    else:
        rank = math.ceil(percentile / 100.0 * len(ordered))
    rank = max(1, min(len(ordered), rank))
    return ordered[rank - 1]


def _is_sha256(value):
    return isinstance(value, str) and SHA256.fullmatch(value) is not None


def _parse_iso(value):
    """解析证据中的 ISO-8601 时间；失败返回 None，由调用方计为缺指标。"""
    if not isinstance(value, str) or not value:
        return None
    try:
        parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    # 不带时区的时间无法与运行时间可靠比较，按缺指标处理。
    return parsed if parsed.tzinfo is not None else None


def _is_non_negative_int(value):
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _display(value):
    """证据表里的缺值显示为短横线，不把 None 伪装成零。"""
    return "-" if value is None else str(value)


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _load_json(path):
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("无法读取 JSON 输入 " + path.name + "：" + str(error)) from error
    _require(isinstance(value, dict), "JSON 输入必须是对象")
    return value


def _load_jsonl(path):
    records = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise ValueError("无法读取测量输入 " + path.name + "：" + str(error)) from error
    for lineno, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError("第 " + str(lineno) + " 行不是 JSON：" + str(error)) from error
        _require(isinstance(value, dict), "第 " + str(lineno) + " 行必须是对象")
        records.append(value)
    _require(records, "测量输入为空")
    return records


def _sample_label(sample):
    return (
        str(sample.get("condition"))
        + "/"
        + str(sample.get("temperature"))
        + "/"
        + str(sample.get("stage"))
        + "#"
        + str(sample.get("sample_id"))
    )


def _performance_provenance(run, failures):
    """所有输入路径共享身份/测量含义校验，直接 JSONL 也不能跳过采集契约。"""
    methods = run.get("measurement_methods")
    if not isinstance(methods, dict):
        failures.append("缺少 measurement_methods；数值不能替代测量定义")
    else:
        for field in (*STAGES, "cold", "hot", "main_thread_stall_ns", "peak_rss_bytes"):
            value = methods.get(field)
            if not isinstance(value, str) or not value.strip():
                failures.append("measurement_methods 缺少 " + field)
        present_method = str(methods.get("first_present", "")).lower()
        if any(word in present_method for word in ("draw-submission", "draw_submission", "effect-callback")):
            failures.append("平台 draw 提交/effect 回调不能作为 first_present")
        stall_method = str(methods.get("main_thread_stall_ns", "")).lower()
        if any(word in stall_method for word in ("event_loop", "event-loop", "timer-gap", "jitter")):
            failures.append("event-loop/timer 抖动不能作为 main_thread_stall_ns")
    if not _is_sha256(run.get("spec_sha256")):
        failures.append("缺少有效事前 spec_sha256")
    fixtures = run.get("fixtures")
    if not isinstance(fixtures, dict) or not fixtures:
        failures.append("缺少事前 fixtures 清单")
        fixtures = {}
    for name, fixture in fixtures.items():
        if not isinstance(fixture, dict) or not _is_sha256(fixture.get("sha256")):
            failures.append("事前 fixture 条目无效：" + str(name))
    receipt = run.get("launch_receipt")
    if not isinstance(receipt, dict):
        failures.append("缺少实际启动器的 launch_receipt")
        return fixtures, None
    if receipt.get("schema") != "aline-m1-launch/v1":
        failures.append("launch_receipt schema 不匹配")
    for field in ("run_id", "app_sha256", "spec_sha256", "fixtures"):
        if receipt.get(field) != run.get(field):
            failures.append("launch_receipt 与运行身份不一致：" + field)
    if not _is_sha256(receipt.get("raw_sha256")):
        failures.append("launch_receipt 缺少有效原始输出哈希")
    if not _is_non_negative_int(receipt.get("pid")) or receipt.get("pid") == 0:
        failures.append("launch_receipt 缺少实际 pid")
    if not _is_non_negative_int(receipt.get("exit_code")) or receipt.get("exit_code") != 0:
        failures.append("launch_receipt 原生进程未成功退出")
    started, finished = (_parse_iso(receipt.get(field))
                         for field in ("started_utc", "finished_utc"))
    if started is None or finished is None or finished < started:
        failures.append("launch_receipt 运行时间无效")
    if receipt.get("started_utc") != run.get("run_started_utc"):
        failures.append("launch_receipt 与运行开始时间不一致")
    return fixtures, finished


def analyze_performance(records, require_matrix=False):
    """校验并汇总 Release 测量记录；任何不可信输入都进入 failures。"""
    failures = []
    for record in records:
        if record.get("kind") not in ("run", "sample"):
            failures.append("未登记或失败的测量记录 kind=" + str(record.get("kind")))
    run_ids = sorted({str(record.get("run_id")) for record in records})
    if len(run_ids) != 1:
        failures.append("测量记录必须来自同一个 run_id，实际为 " + ", ".join(run_ids))
    run_records = [record for record in records if record.get("kind") == "run"]
    if len(run_records) != 1:
        failures.append("必须且只能有一条 kind=run 的运行元数据")
    run = run_records[0] if len(run_records) == 1 else {}
    fixture_manifest, run_finished = _performance_provenance(run, failures)
    if run:
        _require(run.get("schema") == PERF_SCHEMA, "运行元数据 schema 不匹配")
        for field in ("run_id", "commit", "app_sha256", "build_profile", "run_started_utc"):
            if not run.get(field):
                failures.append("运行元数据缺少字段 " + field)
        if not _is_sha256(run.get("app_sha256")):
            failures.append("运行元数据 app_sha256 不是 64 位小写十六进制")
        if run.get("build_profile") != "release":
            failures.append("性能测量必须使用 Release 构建")
        if _parse_iso(run.get("run_started_utc")) is None:
            failures.append("运行开始时间必须是带时区的 ISO-8601")
        environment = run.get("environment")
        if not isinstance(environment, dict):
            failures.append("运行元数据缺少 environment 对象")
        else:
            for field in ("os", "arch", "cpu", "gpui"):
                if not environment.get(field):
                    failures.append("environment 缺少字段 " + field)

    samples = [record for record in records if record.get("kind") == "sample"]
    if not samples:
        failures.append("没有 kind=sample 的测量记录")
    seen = set()
    for sample in samples:
        label = _sample_label(sample)
        if sample.get("schema") != PERF_SCHEMA:
            failures.append(label + " schema 不匹配")
        for field in ("run_id", "condition", "temperature", "stage", "status",
                      "fixture_sha256", "commit", "build_profile", "started_utc",
                      "finished_utc"):
            if not sample.get(field):
                failures.append(label + " 缺少字段 " + field)
        if sample.get("status") != "ok":
            failures.append(label + " 状态为 " + str(sample.get("status")) + "，不得计为成功")
        if sample.get("build_profile") != "release":
            failures.append(label + " 不是 Release 构建")
        if sample.get("stage") not in STAGES:
            failures.append(label + " 阶段不在登记集合内")
        if sample.get("condition") not in CONDITIONS:
            failures.append(label + " 条件不在登记集合内")
        if sample.get("temperature") not in TEMPERATURES:
            failures.append(label + " 冷热标记不是 cold/hot")
        if not _is_sha256(sample.get("fixture_sha256")):
            failures.append(label + " fixture_sha256 不是 64 位小写十六进制")
        fixture = fixture_manifest.get(sample.get("case"))
        if not isinstance(fixture, dict):
            failures.append(label + " case 未绑定事前 fixture 清单")
        else:
            for field in ("condition", "temperature", "sample_id"):
                if sample.get(field) != fixture.get(field):
                    failures.append(label + " 与事前 fixture 的 " + field + " 不同")
            if fixture.get("suite") != "perf" or fixture.get("sha256") != sample.get("fixture_sha256"):
                failures.append(label + " 与事前 fixture 的套件或哈希不同")
        if sample.get("commit") and run.get("commit") and sample.get("commit") != run.get("commit"):
            failures.append(label + " 的 commit 与运行元数据不一致")
        started = _parse_iso(sample.get("started_utc"))
        finished = _parse_iso(sample.get("finished_utc"))
        run_started = _parse_iso(run.get("run_started_utc"))
        if started is None or finished is None:
            failures.append(label + " 时间戳不是可解析的 ISO-8601")
        else:
            if finished < started:
                failures.append(label + " 结束时间早于开始时间")
            if run_started is not None and started < run_started:
                failures.append(label + " 早于本轮运行开始时间，属于旧运行结果")
            if run_finished is not None and finished > run_finished:
                failures.append(label + " 晚于本轮原生进程退出时间")
        if not _is_non_negative_int(sample.get("duration_ns")):
            failures.append(label + " 缺少非负整数 duration_ns")
        mono_start, mono_end = sample.get("started_monotonic_ns"), sample.get("finished_monotonic_ns")
        if sample.get("timing_clock") != "instant-monotonic":
            failures.append(label + " 缺少单调时钟身份")
        if not _is_non_negative_int(mono_start) or not _is_non_negative_int(mono_end):
            failures.append(label + " 缺少真实单调起止边界")
        elif mono_end < mono_start or mono_end - mono_start != sample.get("duration_ns"):
            failures.append(label + " 单调起止边界与耗时不一致")
        wall_duration = sample.get("wall_duration_ns")
        if not _is_non_negative_int(wall_duration):
            failures.append(label + " 缺少独立墙钟起止差值")
        elif started is not None and finished is not None:
            elapsed = finished - started
            elapsed_ns = (elapsed.days * 86400 + elapsed.seconds) * 1_000_000_000 + elapsed.microseconds * 1000
            # ISO-8601输出截去纳秒尾数，两个端点差的表示误差小于1µs；
            # 此处只核对墙钟自身，不假定CLOCK_REALTIME与Instant具有相同分辨率/速率。
            if abs(elapsed_ns - wall_duration) > 1000:
                failures.append(label + " 墙钟起止与独立墙钟差值不一致")
        uncertainty = sample.get("clock_uncertainty_ns", 0)
        if not _is_non_negative_int(uncertainty) or uncertainty > 1_000_000:
            failures.append(label + " 墙钟捕获开销无效或超过 1 ms")
        if sample.get("timing_origin") != "measured-boundaries":
            failures.append(label + " 缺少真实阶段起止边界")
        for field in ("main_thread_stall_ns", "peak_rss_bytes"):
            if not _is_non_negative_int(sample.get(field)):
                failures.append(label + " 缺少逐样本指标 " + field)
        if _is_non_negative_int(sample.get("peak_rss_bytes")) and sample["peak_rss_bytes"] == 0:
            failures.append(label + " 进程峰值 RSS 必须大于零，不能以零代替缺指标")
        if sample.get("stage") in VIRTUAL_ITEM_STAGES and not _is_non_negative_int(
                sample.get("virtual_items")):
            failures.append(label + " 缺少非负整数 virtual_items")
        sample_id = sample.get("sample_id")
        if not _is_non_negative_int(sample_id):
            failures.append(label + " 缺少非负整数 sample_id")
        else:
            key = (sample.get("condition"), sample.get("temperature"),
                   sample.get("stage"), sample_id)
            if key in seen:
                failures.append(label + " 的样本编号重复")
            seen.add(key)

    ok_samples = [sample for sample in samples if sample.get("status") == "ok"]
    groups = {}
    for sample in ok_samples:
        key = (sample.get("condition"), sample.get("temperature"), sample.get("stage"))
        groups.setdefault(key, []).append(sample)
    group_rows = []
    for key in sorted(groups, key=lambda item: tuple(str(part) for part in item)):
        members = groups[key]
        durations = [sample["duration_ns"] for sample in members
                     if _is_non_negative_int(sample.get("duration_ns"))]
        if len(durations) != len(members):
            continue
        if len(members) < MIN_SAMPLES:
            failures.append(
                "/".join(str(part) for part in key) + " 只有 "
                + str(len(members)) + " 个成功样本，少于 " + str(MIN_SAMPLES))
        group_rows.append({
            "condition": key[0],
            "temperature": key[1],
            "stage": key[2],
            "count": len(members),
            "p50_ns": nearest_rank(durations, 50),
            "p95_ns": nearest_rank(durations, 95),
            "max_ns": max(durations),
            "virtual_items_max": max(
                (sample.get("virtual_items") for sample in members
                 if _is_non_negative_int(sample.get("virtual_items"))),
                default=None),
        })

    if require_matrix:
        # 边际集合齐全不表示交叉矩阵齐全；逐单元核验所有条件、冷热与阶段。
        present = {(row["condition"], row["temperature"], row["stage"])
                   for row in group_rows}
        for condition in CONDITIONS:
            for temperature in TEMPERATURES:
                for stage in STAGES:
                    if (condition, temperature, stage) not in present:
                        failures.append("测量矩阵缺少单元：" + "/".join(
                            (condition, temperature, stage)))

    # 同一条件不得把不同输入拼成一组；每次打开的所有阶段必须来自同一份夹具。
    fixture_by_condition = {}
    stages_by_sample = {}
    for sample in ok_samples:
        condition = sample.get("condition")
        fixture_by_condition.setdefault(condition, set()).add(sample.get("fixture_sha256"))
        key = (condition, sample.get("temperature"), sample.get("sample_id"))
        stages_by_sample.setdefault(key, set()).add(sample.get("stage"))
    for condition, fingerprints in fixture_by_condition.items():
        if len(fingerprints) != 1:
            failures.append(str(condition) + " 混合了不同 fixture_sha256")
    if require_matrix:
        for key, stages in stages_by_sample.items():
            if stages != set(STAGES):
                failures.append("/".join(str(part) for part in key)
                                + " 同次打开的阶段不完整")

    stalls = [sample["main_thread_stall_ns"] for sample in ok_samples
              if _is_non_negative_int(sample.get("main_thread_stall_ns"))]
    peaks = [sample["peak_rss_bytes"] for sample in ok_samples
             if _is_non_negative_int(sample.get("peak_rss_bytes"))]
    if not stalls:
        failures.append("缺少 main_thread_stall_ns，不能报告最长主线程停顿")
    if not peaks:
        failures.append("缺少 peak_rss_bytes，不能报告峰值内存")

    summary = {
        "suite": "performance",
        "schema": PERF_SCHEMA,
        "passed": not failures,
        "generated_utc": _utc_now(),
        "run": {
            "run_id": run.get("run_id"),
            "commit": run.get("commit"),
            "build_profile": run.get("build_profile"),
            "app_sha256": run.get("app_sha256"),
            "run_started_utc": run.get("run_started_utc"),
            "environment": run.get("environment"),
            "measurement_methods": run.get("measurement_methods"),
            "spec_sha256": run.get("spec_sha256"),
            "fixtures": run.get("fixtures"),
            "launch_receipt": run.get("launch_receipt"),
        },
        "method": {
            "quantile": "nearest-rank，升序第 ceil(p/100*n) 个值",
            "min_samples_per_group": MIN_SAMPLES,
            "required_matrix": require_matrix,
            "record_count": len(records),
            "sample_count": len(samples),
            "ok_sample_count": len(ok_samples),
        },
        "groups": group_rows,
        "longest_main_thread_stall_ns": max(stalls) if stalls else None,
        "peak_rss_bytes": max(peaks) if peaks else None,
        "virtual_items_max": max(
            (row["virtual_items_max"] for row in group_rows
             if row["virtual_items_max"] is not None), default=None),
        "non_measured": list(NON_MEASURED),
        "failures": failures,
    }
    return summary


def _performance_markdown(summary, input_name, command):
    lines = [
        "# M1 只读性能测量汇总",
        "",
        "- 结果：" + ("通过" if summary["passed"] else "未通过"),
        "- 输入：`" + input_name + "`",
        "- 运行：`" + str(summary["run"].get("run_id")) + "`",
        "- commit：`" + str(summary["run"].get("commit")) + "`",
        "- 构建：`" + str(summary["run"].get("build_profile")) + "`",
        "- 应用 SHA-256：`" + str(summary["run"].get("app_sha256")) + "`",
        "- 方法：" + summary["method"]["quantile"] + "；每组至少 "
        + str(MIN_SAMPLES) + " 个成功样本。",
        "",
        "## 环境",
        "",
        "```json",
        json.dumps(summary["run"].get("environment"), ensure_ascii=False, indent=2),
        "```",
        "",
        "## 原始测量含义与启动身份",
        "",
        "```json",
        json.dumps({field: summary["run"].get(field) for field in (
            "measurement_methods", "spec_sha256", "fixtures", "launch_receipt")},
            ensure_ascii=False, indent=2),
        "```",
        "",
        "## 分阶段结果",
        "",
        "| 条件 | 冷热 | 阶段 | 样本数 | P50 (ms) | P95 (ms) | 最大 (ms) | 虚拟实例上限 |",
        "|---|---|---|---:|---:|---:|---:|---:|",
    ]
    for row in summary["groups"]:
        lines.append(
            "| " + str(row["condition"]) + " | " + str(row["temperature"]) + " | "
            + str(row["stage"]) + " | " + str(row["count"]) + " | "
            + "{:.3f}".format(row["p50_ns"] / 1_000_000) + " | "
            + "{:.3f}".format(row["p95_ns"] / 1_000_000) + " | "
            + "{:.3f}".format(row["max_ns"] / 1_000_000) + " | "
            + ("-" if row["virtual_items_max"] is None else str(row["virtual_items_max"]))
            + " |")
    lines.extend([
        "",
        "## 汇总指标",
        "",
        "- 最长主线程停顿："
        + ("缺指标" if summary["longest_main_thread_stall_ns"] is None
           else "{:.3f} ms".format(summary["longest_main_thread_stall_ns"] / 1_000_000)),
        "- 峰值内存："
        + ("缺指标" if summary["peak_rss_bytes"] is None
           else str(summary["peak_rss_bytes"]) + " bytes"),
        "- 虚拟实例上限："
        + ("缺指标" if summary["virtual_items_max"] is None
           else str(summary["virtual_items_max"])),
        "",
        "## 未测项",
        "",
    ])
    lines.extend("- " + item for item in summary["non_measured"])
    if summary["failures"]:
        lines.extend(["", "## 失败原因", ""])
        lines.extend("- " + item for item in summary["failures"])
    lines.extend([
        "",
        "## 复跑",
        "",
        "```text",
        command,
        "```",
        "",
    ])
    return "\n".join(lines)


def run_performance(args, output):
    input_path = args.samples or (output / "samples.jsonl")
    if not input_path.is_file():
        summary = {
            "suite": "performance",
            "schema": PERF_SCHEMA,
            "passed": False,
            "generated_utc": _utc_now(),
            "run": {},
            "method": {"quantile": "nearest-rank，升序第 ceil(p/100*n) 个值",
                       "min_samples_per_group": MIN_SAMPLES,
                       "required_matrix": args.require_matrix},
            "groups": [],
            "longest_main_thread_stall_ns": None,
            "peak_rss_bytes": None,
            "virtual_items_max": None,
            "non_measured": list(NON_MEASURED),
            "failures": ["缺少测量输入 " + input_path.name + "，不能自动通过"],
        }
        write_json(output / "summary.json", summary)
        write_text(output / "summary.md",
                   _performance_markdown(summary, input_path.name,
                                         _repro_command(args, output)))
        print("performance: 缺少测量输入，未通过")
        return 1
    try:
        records = _load_jsonl(input_path)
        write_text(output / "samples.jsonl", input_path.read_text(encoding="utf-8"))
        summary = analyze_performance(records, args.require_matrix)
    except ValueError as error:
        summary = {
            "suite": "performance",
            "schema": PERF_SCHEMA,
            "passed": False,
            "generated_utc": _utc_now(),
            "run": {},
            "method": {"quantile": "nearest-rank，升序第 ceil(p/100*n) 个值",
                       "min_samples_per_group": MIN_SAMPLES,
                       "required_matrix": args.require_matrix},
            "groups": [],
            "longest_main_thread_stall_ns": None,
            "peak_rss_bytes": None,
            "virtual_items_max": None,
            "non_measured": list(NON_MEASURED),
            "failures": [str(error)],
        }
    write_json(output / "summary.json", summary)
    write_text(output / "summary.md",
               _performance_markdown(summary, input_path.name, _repro_command(args, output)))
    print("performance: " + ("通过" if summary["passed"] else "未通过"))
    for failure in summary["failures"]:
        print("  - " + failure)
    return 0 if summary["passed"] else 1


def analyze_window(output, observations_path, observations):
    """核对人工观测；observations 为 None 或缺少条目都不能通过。"""
    failures = []
    if observations is None:
        failures.append("缺少人工观测输入 " + observations_path.name + "，不能自动通过")
        items = [{"id": item_id, "requirement": requirement, "status": "missing",
                  "observed_zh": "", "evidence": []}
                 for item_id, requirement in WINDOW_ITEMS]
        return {"suite": "window-baseline", "schema": WINDOW_SCHEMA, "passed": False,
                "generated_utc": _utc_now(), "observations": observations_path.name,
                "items": items, "failures": failures}
    _require(observations.get("schema") == WINDOW_SCHEMA, "观测输入 schema 不匹配")
    _require(isinstance(observations.get("items"), list), "观测输入缺少 items 数组")
    by_id = {}
    for entry in observations["items"]:
        _require(isinstance(entry, dict), "观测项必须是对象")
        item_id = entry.get("id")
        _require(isinstance(item_id, str) and item_id, "观测项缺少 id")
        if item_id in by_id:
            failures.append("观测项重复：" + item_id)
        by_id[item_id] = entry
    items = []
    known_ids = {item_id for item_id, _ in WINDOW_ITEMS}
    for item_id, requirement in WINDOW_ITEMS:
        entry = by_id.get(item_id)
        if entry is None:
            failures.append("缺少观测项：" + item_id)
            items.append({"id": item_id, "requirement": requirement,
                          "status": "missing", "observed_zh": "", "evidence": []})
            continue
        status = entry.get("status")
        if status not in ("pass", "fail", "blocked"):
            failures.append(item_id + " 状态必须是 pass/fail/blocked")
        if status != "pass":
            failures.append(item_id + " 状态为 " + str(status) + "，不能计为通过")
        observed_zh = entry.get("observed_zh")
        if not isinstance(observed_zh, str) or not observed_zh.strip():
            failures.append(item_id + " 缺少中文观测记录")
            observed_zh = ""
        evidence = entry.get("evidence")
        if not isinstance(evidence, list) or not evidence:
            failures.append(item_id + " 缺少证据文件")
            evidence = []
        for path in evidence:
            if not isinstance(path, str) or not path:
                failures.append(item_id + " 证据路径无效")
                continue
            candidate = Path(path)
            if not candidate.is_absolute():
                candidate = output / candidate
            if not candidate.is_file():
                failures.append(item_id + " 证据文件不存在：" + path)
        items.append({
            "id": item_id,
            "requirement": requirement,
            "status": status if status in ("pass", "fail", "blocked") else "invalid",
            "observed_zh": observed_zh,
            "evidence": [path for path in evidence
                         if isinstance(path, str) and path],
        })
    for item_id in by_id:
        if item_id not in known_ids:
            failures.append("未登记的观测项：" + item_id)
    return {"suite": "window-baseline", "schema": WINDOW_SCHEMA, "passed": not failures,
            "generated_utc": _utc_now(), "observations": observations_path.name,
            "items": items, "failures": failures}


def _window_markdown(summary):
    lines = [
        "# M1 主窗 V-09 人工观测",
        "",
        "- 结果：" + ("通过" if summary["passed"] else "未通过"),
        "- 观测输入：`" + str(summary["observations"]) + "`",
        "",
        "| 编号 | 观测项 | 状态 | 中文记录 | 证据 |",
        "|---|---|---|---|---|",
    ]
    for item in summary["items"]:
        lines.append(
            "| " + item["id"] + " | " + item["requirement"] + " | "
            + item["status"] + " | " + item["observed_zh"] + " | "
            + ", ".join(item["evidence"]) + " |")
    if summary["failures"]:
        lines.extend(["", "## 失败原因", ""])
        lines.extend("- " + failure for failure in summary["failures"])
    lines.append("")
    return "\n".join(lines)


def run_window_baseline(args, output):
    checklist = {
        "schema": WINDOW_SCHEMA,
        "generated_utc": _utc_now(),
        "instruction": "在真实 macOS 窗口逐项观测并填写 status/observed_zh/evidence；"
                       "status 仅接受 pass/fail/blocked，缺少人工记录不能自动通过。",
        "items": [{"id": item_id, "requirement": requirement}
                  for item_id, requirement in WINDOW_ITEMS],
    }
    write_json(output / "checklist.json", checklist)
    observations_path = args.observations or (output / "observations.json")
    observed = None
    error = None
    if observations_path.is_file():
        try:
            observed = _load_json(observations_path)
            write_text(output / "observations.json",
                       observations_path.read_text(encoding="utf-8"))
        except ValueError as load_error:
            error = str(load_error)
    try:
        summary = analyze_window(output, observations_path, observed)
    except ValueError as analyze_error:
        error = str(analyze_error)
        summary = analyze_window(output, observations_path, None)
    if error:
        summary["failures"].insert(0, error)
        summary["passed"] = False
    write_json(output / "summary.json", summary)
    write_text(output / "summary.md", _window_markdown(summary))
    print("window-baseline: " + ("通过" if summary["passed"] else "未通过"))
    for failure in summary["failures"]:
        print("  - " + failure)
    return 0 if summary["passed"] else 1


def _sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def generate_bytes_fixtures(output):
    """生成确定性字节夹具与期望清单；只写证据目录。"""
    fixtures = output / "fixtures"
    fixtures.mkdir(parents=True, exist_ok=True)
    manifest = []
    for name, case in BYTES_CASES.items():
        data = case["bytes"]
        path = fixtures / case["file"]
        path.write_bytes(data)
        manifest.append({
            "name": name,
            "file": case["file"],
            "note": case["note"],
            "size_bytes": len(data),
            "sha256": _sha256_bytes(data),
            # 编辑源码只移除一个文件头 BOM；其余 CR/LF 和尾部字节原样比较。
            "source_sha256": _sha256_bytes(data[3:] if data.startswith(b"\xef\xbb\xbf")
                                           else data),
            "first_32_bytes_hex": data[:32].hex(),
            "last_32_bytes_hex": data[-32:].hex(),
        })
    write_json(output / "expected.json", {
        "schema": BYTES_FIXTURE_SCHEMA,
        "generated_utc": _utc_now(),
        "cases": manifest,
    })
    return manifest


def _bytes_markdown(summary):
    lines = [
        "# M1 只读字节保真核对",
        "",
        "- 结果：" + ("通过" if summary["passed"] else "未通过"),
        "- 结果输入：`" + str(summary["results"]) + "`",
        "",
        "| 用例 | 步骤 | 磁盘 SHA-256 | 内存 SHA-256 | 草稿 | 历史 | revision |",
        "|---|---|---|---|---|---|---|",
    ]
    for case in summary["cases"]:
        lines.append(
            "| " + str(case["name"]) + " | "
            + ",".join(str(step) for step in case["steps"]) + " | "
            + _display(case["disk_sha256_after"]) + " | "
            + _display(case["memory_sha256_after"]) + " | "
            + _display(case["has_draft"]) + " | " + _display(case["history_len"]) + " | "
            + _display(case["revision_before"]) + "→"
            + _display(case["revision_after"]) + " |")
    lines.extend(["", "内存列为存储编码结果；实际源码另按去除一个头 BOM 后的精确字节核对。", "",
                  "| 用例 | 初始源码 SHA-256 | 结束源码 SHA-256 | 源码逐字节相等 | 磁盘逐字节相等 |",
                  "|---|---|---|---|---|"])
    for case in summary["cases"]:
        lines.append("| " + str(case["name"]) + " | "
            + _display(case.get("source_sha256_before")) + " | "
            + _display(case.get("source_sha256_after")) + " | "
            + _display(case.get("source_bytes_equal")) + " | "
            + _display(case.get("disk_bytes_equal")) + " |")
    if summary["failures"]:
        lines.extend(["", "## 失败原因", ""])
        lines.extend("- " + failure for failure in summary["failures"])
    lines.append("")
    return "\n".join(lines)


def _case_result(case):
    return {
        "name": case.get("name"),
        "steps": case.get("steps") if isinstance(case.get("steps"), list) else [],
        "disk_sha256_after": case.get("disk_sha256_after"),
        "memory_sha256_after": case.get("memory_sha256_after"),
        "source_sha256_before": case.get("source_sha256_before"),
        "source_sha256_after": case.get("source_sha256_after"),
        "source_bytes_equal": case.get("source_bytes_equal"),
        "disk_bytes_equal": case.get("disk_bytes_equal"),
        "has_draft": case.get("has_draft"),
        "history_len": case.get("history_len"),
        "revision_before": case.get("revision_before"),
        "revision_after": case.get("revision_after"),
    }


def _bytes_provenance(expected_by_name, results, failures):
    """直接 JSON 输入同样校验启动身份，不能跳过 collector 的运行契约。"""
    if not _is_sha256(results.get("spec_sha256")):
        failures.append("字节结果缺少有效事前 spec_sha256")
    fixtures = results.get("fixtures")
    if not isinstance(fixtures, dict) or set(fixtures) != set(expected_by_name):
        failures.append("字节结果缺少完整事前 fixture 清单")
        fixtures = {} if not isinstance(fixtures, dict) else fixtures
    for name, expected in expected_by_name.items():
        fixture = fixtures.get(name)
        if not isinstance(fixture, dict) or fixture.get("sha256") != expected["sha256"]:
            failures.append("字节事前 fixture 哈希与独立夹具不同：" + name)
        elif fixture.get("suite") != "bytes":
            failures.append("字节事前 fixture 套件不同：" + name)
    receipt = results.get("launch_receipt")
    if not isinstance(receipt, dict):
        failures.append("字节结果缺少实际启动器的 launch_receipt")
        return
    if receipt.get("schema") != "aline-m1-launch/v1":
        failures.append("字节启动收据 schema 不匹配")
    for field in ("run_id", "app_sha256", "spec_sha256", "fixtures"):
        if receipt.get(field) != results.get(field):
            failures.append("字节启动收据与结果身份不一致：" + field)
    if not _is_sha256(receipt.get("raw_sha256")):
        failures.append("字节启动收据缺少原始输出哈希")
    if not _is_non_negative_int(receipt.get("pid")) or receipt.get("pid") == 0:
        failures.append("字节启动收据缺少实际 pid")
    if not _is_non_negative_int(receipt.get("exit_code")) or receipt.get("exit_code") != 0:
        failures.append("字节原生进程未成功退出")
    started, finished = (_parse_iso(receipt.get(field)) for field in ("started_utc", "finished_utc"))
    if started is None or finished is None or finished < started:
        failures.append("字节启动收据运行时间无效")


def analyze_bytes(expected, results):
    """核对生命周期前后字节与内存源码；返回汇总与 failures。"""
    failures = []
    _require(results.get("schema") == BYTES_SCHEMA, "字节结果 schema 不匹配")
    for field in ("run_id", "commit", "build_profile", "app_sha256"):
        if not results.get(field):
            failures.append("字节结果缺少字段 " + field)
    if results.get("build_profile") != "release":
        failures.append("字节核对必须使用 Release 构建")
    if not _is_sha256(results.get("app_sha256")):
        failures.append("字节结果 app_sha256 不是 64 位小写十六进制")
    expected_by_name = {case["name"]: case for case in expected}
    _bytes_provenance(expected_by_name, results, failures)
    actual = results.get("cases")
    if not isinstance(actual, list):
        failures.append("字节结果缺少 cases 数组")
        actual = []
    actual_by_name = {}
    for case in actual:
        if not isinstance(case, dict):
            failures.append("字节用例必须是对象")
            continue
        name = case.get("name")
        if name in actual_by_name:
            failures.append("字节用例重复：" + str(name))
        actual_by_name[name] = case
    rows = []
    for name, expected_case in expected_by_name.items():
        case = actual_by_name.get(name)
        if case is None:
            failures.append("缺少字节用例：" + name)
            rows.append(_case_result({"name": name}))
            continue
        row = _case_result(case)
        rows.append(row)
        if case.get("run_id") != results.get("run_id"):
            failures.append(name + " 字节用例与本轮 run_id 不一致")
        steps = case.get("steps")
        if not isinstance(steps, list) or tuple(steps) != REQUIRED_STEPS:
            failures.append(name + " 步骤必须依次为 " + "→".join(REQUIRED_STEPS))
        if case.get("disk_sha256_after") != expected_case["sha256"]:
            failures.append(name + " 磁盘字节在阅读生命周期后改变")
        if case.get("memory_sha256_after") != expected_case["sha256"]:
            failures.append(name + " 内存源码的存储编码结果改变")
        for field in ("source_sha256_before", "source_sha256_after"):
            if case.get(field) != expected_case["source_sha256"]:
                failures.append(name + " " + field + " 与精确解码源码不一致")
        if case.get("source_bytes_equal") is not True:
            failures.append(name + " 缺少真实内存源码逐字节相等证据")
        if case.get("disk_bytes_equal") is not True:
            failures.append(name + " 缺少实际磁盘逐字节相等证据")
        if case.get("has_draft") is not False:
            failures.append(name + " 出现草稿")
        if not _is_non_negative_int(case.get("history_len")) or case.get("history_len") != 0:
            failures.append(name + " 历史不为空或缺少非负整数 history_len")
        if not _is_non_negative_int(case.get("revision_before")):
            failures.append(name + " 缺少非负整数 revision_before")
        if not _is_non_negative_int(case.get("revision_after")):
            failures.append(name + " 缺少非负整数 revision_after")
        if case.get("revision_before") != case.get("revision_after"):
            failures.append(name + " revision 在阅读生命周期内改变")
    for name in actual_by_name:
        if name not in expected_by_name:
            failures.append("未登记的字节用例：" + str(name))
    summary = {
        "suite": "readonly-bytes",
        "schema": BYTES_SCHEMA,
        "passed": not failures,
        "generated_utc": _utc_now(),
        "run_id": results.get("run_id"),
        "commit": results.get("commit"),
        "build_profile": results.get("build_profile"),
        "app_sha256": results.get("app_sha256"),
        "spec_sha256": results.get("spec_sha256"),
        "fixtures": results.get("fixtures"),
        "launch_receipt": results.get("launch_receipt"),
        "results": "results.json",
        "cases": rows,
        "failures": failures,
    }
    return summary


def run_readonly_bytes(args, output):
    expected = generate_bytes_fixtures(output)
    results_path = args.results or (output / "results.json")
    if not results_path.is_file():
        failures = ["缺少字节结果 " + results_path.name + "，不能自动通过"]
        summary = {
            "suite": "readonly-bytes",
            "schema": BYTES_SCHEMA,
            "passed": False,
            "generated_utc": _utc_now(),
            "run_id": None,
            "commit": None,
            "build_profile": None,
            "app_sha256": None,
            "results": results_path.name,
            "cases": [_case_result({"name": name}) for name in BYTES_CASES],
            "failures": failures,
        }
    else:
        try:
            results = _load_json(results_path)
            write_text(output / "results.json",
                       results_path.read_text(encoding="utf-8"))
            summary = analyze_bytes(expected, results)
        except ValueError as error:
            summary = {
                "suite": "readonly-bytes",
                "schema": BYTES_SCHEMA,
                "passed": False,
                "generated_utc": _utc_now(),
                "run_id": None,
                "commit": None,
                "build_profile": None,
                "app_sha256": None,
                "results": results_path.name,
                "cases": [_case_result({"name": name}) for name in BYTES_CASES],
                "failures": [str(error)],
            }
    write_json(output / "summary.json", summary)
    write_text(output / "summary.md", _bytes_markdown(summary))
    print("readonly-bytes: " + ("通过" if summary["passed"] else "未通过"))
    for failure in summary["failures"]:
        print("  - " + failure)
    return 0 if summary["passed"] else 1


def _repro_command(args, output):
    parts = ["python3", "scripts/run_m1_readonly.py", "--suite", args.suite,
             "--output-dir", str(output)]
    if args.samples:
        parts.extend(["--samples", str(args.samples)])
    if args.observations:
        parts.extend(["--observations", str(args.observations)])
    if args.results:
        parts.extend(["--results", str(args.results)])
    if args.require_matrix:
        parts.append("--require-matrix")
    return " ".join(parts)


class SelfTest:
    """自测记录：每个用例单独留证，失败不影响其他用例继续执行。"""

    def __init__(self, output):
        self.output = output
        self.cases = []

    def check(self, name, function):
        try:
            function()
            self.cases.append({"case": name, "passed": True})
        except Exception as error:  # 自测需要收集所有失败，不中断后续用例
            self.cases.append({"case": name, "passed": False,
                               "error": type(error).__name__ + ": " + str(error)})

    def expect(self, condition, message):
        if not condition:
            raise AssertionError(message)

    def finish(self):
        passed = all(case["passed"] for case in self.cases)
        write_json(self.output / "self-test.json", {
            "passed": passed,
            "generated_utc": _utc_now(),
            "cases": self.cases,
        })
        print("self-test: " + ("通过" if passed else "未通过"))
        for case in self.cases:
            if not case["passed"]:
                print("  - " + case["case"] + ": " + case.get("error", ""))
        return 0 if passed else 1


def _valid_run_record(run_id="run-1"):
    fixtures = {}
    for condition in CONDITIONS:
        for temperature in TEMPERATURES:
            for sample_id in range(MIN_SAMPLES):
                fixtures[_synthetic_case(condition, temperature, sample_id)] = {
                    "condition": condition, "temperature": temperature,
                    "sample_id": sample_id, "suite": "perf", "sha256": "c" * 64}
    run = {
        "kind": "run",
        "schema": PERF_SCHEMA,
        "run_id": run_id,
        "commit": "a" * 40,
        "app_sha256": "b" * 64,
        "build_profile": "release",
        "run_started_utc": "2026-11-06T00:00:00+00:00",
        "environment": {"os": "macOS", "arch": "arm64", "cpu": "Apple M", "gpui": "0.3.9"},
        "spec_sha256": "d" * 64,
        "fixtures": fixtures,
        "measurement_methods": {field: "仅供契约自测的合成方法，非原生测量证据"
            for field in (*STAGES, "cold", "hot", "main_thread_stall_ns", "peak_rss_bytes")},
    }
    run["launch_receipt"] = {
        "schema": "aline-m1-launch/v1", "pid": 1, "exit_code": 0,
        "started_utc": run["run_started_utc"], "finished_utc": "2026-11-06T01:00:00+00:00",
        "raw_sha256": "e" * 64,
        **{field: run[field] for field in ("run_id", "app_sha256", "spec_sha256", "fixtures")}}
    return run


def _synthetic_case(condition, temperature, sample_id):
    return "synthetic-" + str(CONDITIONS.index(condition)) + "-" + temperature + "-" + str(sample_id)


def _sample(run_id, sample_id, duration, stage="first_layout", condition="32KiB",
            temperature="cold", status="ok", **extra):
    record = {
        "kind": "sample",
        "schema": PERF_SCHEMA,
        "run_id": run_id,
        "sample_id": sample_id,
        "case": _synthetic_case(condition, temperature, sample_id),
        "condition": condition,
        "temperature": temperature,
        "stage": stage,
        "status": status,
        "duration_ns": duration,
        "fixture_sha256": "c" * 64,
        "commit": "a" * 40,
        "build_profile": "release",
        "started_utc": "2026-11-06T00:00:00+00:00",
        "finished_utc": (datetime.datetime(2026, 11, 6, tzinfo=datetime.timezone.utc)
                         + datetime.timedelta(microseconds=duration / 1000)).isoformat(),
        "timing_origin": "measured-boundaries",
        "timing_clock": "instant-monotonic",
        "started_monotonic_ns": 0,
        "finished_monotonic_ns": duration,
        "wall_duration_ns": round(duration / 1000) * 1000,
        "main_thread_stall_ns": 1000,
        "peak_rss_bytes": 1024,
        "virtual_items": 10,
    }
    record.update(extra)
    return record


def _twenty_samples(stage="first_layout", condition="32KiB", temperature="cold"):
    return [_sample("run-1", index, index + 1, stage=stage, condition=condition,
                    temperature=temperature) for index in range(20)]


def self_test_performance(output):
    test = SelfTest(output)
    test.check("nearest-rank 已知序列", lambda: (
        test.expect(nearest_rank(list(range(1, 101)), 50) == 50, "P50 应为 50"),
        test.expect(nearest_rank(list(range(1, 101)), 95) == 95, "P95 应为 95"),
        test.expect(nearest_rank(list(range(1, 21)), 50) == 10, "20 样本 P50 应为第 10 个"),
        test.expect(nearest_rank(list(range(1, 21)), 95) == 19, "20 样本 P95 应为第 19 个"),
        test.expect(nearest_rank([7], 95) == 7, "单样本应返回自身"),
    ))
    test.check("20 个成功样本通过并汇总指标", lambda: (
        test.expect(analyze_performance([_valid_run_record()] + _twenty_samples())["passed"],
                    "20 个成功样本应通过"),
        test.expect(analyze_performance(
            [_valid_run_record()] + _twenty_samples())["groups"][0]["p50_ns"] == 10,
            "P50 应为 10"),
    ))
    test.check("19 个样本被拒绝", lambda: test.expect(
        not analyze_performance([_valid_run_record()] + _twenty_samples()[:19])["passed"],
        "19 个样本不得通过"))
    test.check("错误样本被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 19, 20, status="error")])["passed"],
        "错误状态不得通过"))
    test.check("取消样本被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 19, 20, status="cancelled")])["passed"],
        "取消状态不得通过"))
    test.check("缺 duration 被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 19, 20, duration_ns=None)])["passed"],
        "缺 duration 不得通过"))
    test.check("缺主线程停顿被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()]
            + [_sample("run-1", index, index + 1, main_thread_stall_ns=None)
               for index in range(20)])["passed"],
        "缺主线程停顿不得通过"))
    test.check("缺峰值内存被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()]
            + [_sample("run-1", index, index + 1, peak_rss_bytes=None)
               for index in range(20)])["passed"],
        "缺峰值内存不得通过"))
    test.check("跨 run_id 被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-2", 19, 20)])["passed"],
        "混合 run_id 不得通过"))
    test.check("旧运行时间戳被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 19, 20,
                       started_utc="2020-01-01T00:00:00+00:00",
                       finished_utc="2020-01-01T00:00:01+00:00")])["passed"],
        "早于运行开始时间的记录不得通过"))
    test.check("无时区时间戳被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 19, 20,
                       started_utc="2026-11-06T00:00:00",
                       finished_utc="2026-11-06T00:00:01")])["passed"],
        "无时区时间戳不得通过"))
    test.check("重复样本编号被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 0, 20)])["passed"],
        "重复 sample_id 不得通过"))
    test.check("非 Release 被拒绝", lambda: test.expect(
        not analyze_performance(
            [_valid_run_record()] + _twenty_samples()[:19]
            + [_sample("run-1", 19, 20, build_profile="debug")])["passed"],
        "debug 构建不得进入性能套件"))
    test.check("阶段归属不串组", lambda: (
        test.expect(len(analyze_performance(
            [_valid_run_record()] + _twenty_samples() + _twenty_samples(
                stage="scroll", condition="1MiB"))["groups"]) == 2,
            "两个阶段应分成两组"),
        test.expect(any(
            group["stage"] == "scroll" and group["condition"] == "1MiB"
            and group["count"] == 20
            for group in analyze_performance(
                [_valid_run_record()] + _twenty_samples() + _twenty_samples(
                    stage="scroll", condition="1MiB"))["groups"]),
            "scroll 组应独立汇总且样本数为 20"),
    ))
    sparse = [_valid_run_record()]
    for index, condition in enumerate(CONDITIONS):
        sparse.extend(_twenty_samples(stage=STAGES[index % len(STAGES)],
                                      condition=condition,
                                      temperature=TEMPERATURES[index % len(TEMPERATURES)]))
    test.check("边际齐全但交叉矩阵缺失被拒绝", lambda: test.expect(
        not analyze_performance(sparse, require_matrix=True)["passed"],
        "各集合出现过不能替代逐单元矩阵"))
    complete = [_valid_run_record()]
    for condition in CONDITIONS:
        for temperature in TEMPERATURES:
            for stage in STAGES:
                complete.extend(_twenty_samples(stage, condition, temperature))
    test.check("完整交叉矩阵通过", lambda: test.expect(
        analyze_performance(complete, require_matrix=True)["passed"], "完整矩阵应通过"))
    for field in ("measurement_methods", "spec_sha256", "fixtures", "launch_receipt"):
        without_provenance = [_valid_run_record()] + _twenty_samples()
        without_provenance[0].pop(field)
        test.check("直接 samples 缺少 " + field + " 被拒绝",
            lambda value=without_provenance: test.expect(
                not analyze_performance(value)["passed"], "直接输入不能绕过身份与方法校验"))
    one_metric = json.loads(json.dumps(complete))
    for sample in one_metric[2:]:
        sample.pop("main_thread_stall_ns")
        sample.pop("peak_rss_bytes")
    test.check("2880 样本仅一条 RSS 和停顿被拒绝", lambda: test.expect(
        not analyze_performance(one_metric, require_matrix=True)["passed"],
        "单条指标不能代表全部样本覆盖"))
    zero_rss = [_valid_run_record()] + _twenty_samples()
    zero_rss[-1]["peak_rss_bytes"] = 0
    test.check("零RSS被拒绝", lambda: test.expect(
        not analyze_performance(zero_rss)["passed"], "运行进程峰值RSS必须大于零"))
    event_gap = [_valid_run_record()] + _twenty_samples()
    event_gap[0]["measurement_methods"]["main_thread_stall_ns"] = "event_loop_gap_ns"
    test.check("事件循环抖动冒充主线程停顿被拒绝", lambda: test.expect(
        not analyze_performance(event_gap)["passed"], "事件循环抖动没有精确停顿含义"))
    draw_present = [_valid_run_record()] + _twenty_samples()
    draw_present[0]["measurement_methods"]["first_present"] = "platform-draw-submission-effect-callback"
    test.check("draw 提交冒充屏幕呈现被拒绝", lambda: test.expect(
        not analyze_performance(draw_present)["passed"], "draw effect 不能证明屏幕呈现"))
    wrong_receipt = [_valid_run_record()] + _twenty_samples()
    wrong_receipt[0]["launch_receipt"]["app_sha256"] = "f" * 64
    test.check("启动收据应用哈希不一致被拒绝", lambda: test.expect(
        not analyze_performance(wrong_receipt)["passed"], "启动收据必须与运行身份一致"))
    wrong_boundary = [_valid_run_record()] + _twenty_samples()
    wrong_boundary[-1]["finished_utc"] = "2026-11-06T00:00:01+00:00"
    test.check("直接 samples 计时边界不一致被拒绝", lambda: test.expect(
        not analyze_performance(wrong_boundary)["passed"], "不能绕过采集器计时边界校验"))
    wrong_monotonic = [_valid_run_record()] + _twenty_samples()
    wrong_monotonic[-1]["finished_monotonic_ns"] += 1
    test.check("单调起止与duration不一致被拒绝", lambda: test.expect(
        not analyze_performance(wrong_monotonic)["passed"], "单调边界必须精确对应耗时"))
    quantized_wall = [_valid_run_record()] + _twenty_samples()
    quantized_wall[-1]["finished_utc"] = "2026-11-06T00:00:00.000001+00:00"
    quantized_wall[-1]["wall_duration_ns"] = 1000
    test.check("墙钟量化不重构单调耗时", lambda: test.expect(
        analyze_performance(quantized_wall)["passed"], "实测墙钟1µs刻度不等于单调耗时"))
    missing_monotonic = [_valid_run_record()] + _twenty_samples()
    missing_monotonic[-1].pop("started_monotonic_ns")
    test.check("缺少单调起止拒绝", lambda: test.expect(
        not analyze_performance(missing_monotonic)["passed"], "不能从墙钟回推单调边界"))
    missing_boundary = [_valid_run_record()] + _twenty_samples()
    missing_boundary[-1].pop("timing_origin")
    test.check("直接 samples 缺阶段边界来源被拒绝", lambda: test.expect(
        not analyze_performance(missing_boundary)["passed"], "阶段边界来源不可省略"))
    bad_run_time = [_valid_run_record()] + _twenty_samples()
    bad_run_time[0]["run_started_utc"] = "not-a-time"
    test.check("运行开始时间无效被拒绝", lambda: test.expect(
        not analyze_performance(bad_run_time)["passed"], "无效运行开始时间必须拒绝"))
    test.check("独立错误记录被拒绝", lambda: test.expect(
        not analyze_performance([_valid_run_record()] + _twenty_samples()
            + [{"kind": "error", "run_id": "run-1"}])["passed"],
        "错误记录不得被过滤后通过"))
    mixed_fixture = [_valid_run_record()] + _twenty_samples()
    mixed_fixture[-1]["fixture_sha256"] = "d" * 64
    test.check("同条件混用夹具被拒绝", lambda: test.expect(
        not analyze_performance(mixed_fixture)["passed"], "不同夹具不能混入同一组"))
    return test.finish()


def self_test_window(output):
    test = SelfTest(output)
    evidence = output / "window-evidence.txt"
    evidence.write_text("fake screenshot placeholder", encoding="utf-8")
    valid = {
        "schema": WINDOW_SCHEMA,
        "items": [
            {"id": item_id, "status": "pass", "observed_zh": "已观测",
             "evidence": [evidence.name]}
            for item_id, _ in WINDOW_ITEMS
        ],
    }
    test.check("缺少人工观测不得通过", lambda: test.expect(
        not _window_ok(output, None), "缺少观测必须失败"))
    test.check("完整人工观测通过", lambda: test.expect(
        _window_ok(output, valid), "完整观测应通过"))
    broken = {"schema": WINDOW_SCHEMA, "items": valid["items"][:-1]}
    test.check("缺少观测项被拒绝", lambda: test.expect(
        not _window_ok(output, broken), "缺少观测项必须失败"))
    blocked = {"schema": WINDOW_SCHEMA,
               "items": [dict(item) for item in valid["items"]]}
    blocked["items"][0] = dict(blocked["items"][0], status="blocked")
    test.check("blocked 不得计为通过", lambda: test.expect(
        not _window_ok(output, blocked), "blocked 必须失败"))
    return test.finish()


def _window_ok(output, observations):
    """自测复用正式核对逻辑；输入路径只用于证据名。"""
    path = output / "self-test-observations.json"
    if observations is not None:
        write_json(path, observations)
    return analyze_window(output, path, observations)["passed"]


def self_test_bytes(output):
    test = SelfTest(output)
    expected = generate_bytes_fixtures(output)
    valid = {
        "schema": BYTES_SCHEMA,
        "run_id": "run-1",
        "commit": "a" * 40,
        "build_profile": "release",
        "app_sha256": "b" * 64,
        "spec_sha256": "c" * 64,
        "fixtures": {case["name"]: {"sha256": case["sha256"], "suite": "bytes"} for case in expected},
        "cases": [
            {"name": case["name"], "run_id": "run-1", "steps": list(REQUIRED_STEPS),
             "disk_sha256_after": case["sha256"],
             "memory_sha256_after": case["sha256"],
             "source_sha256_before": case["source_sha256"],
             "source_sha256_after": case["source_sha256"],
             "source_bytes_equal": True, "disk_bytes_equal": True,
             "has_draft": False, "history_len": 0,
             "revision_before": 0, "revision_after": 0}
            for case in expected
        ],
    }
    valid["launch_receipt"] = {
        "schema": "aline-m1-launch/v1", "pid": 42, "exit_code": 0,
        "raw_sha256": "d" * 64, "started_utc": "2026-11-08T00:00:00+00:00",
        "finished_utc": "2026-11-08T00:00:01+00:00",
        **{key: valid[key] for key in ("run_id", "app_sha256", "spec_sha256", "fixtures")},
    }
    test.check("字节一致通过", lambda: test.expect(
        analyze_bytes(expected, valid)["passed"], "一致结果应通过"))
    for field in ("spec_sha256", "fixtures", "launch_receipt"):
        broken = json.loads(json.dumps(valid))
        broken.pop(field)
        test.check("直接字节输入缺少 " + field + " 被拒绝", lambda value=broken: test.expect(
            not analyze_bytes(expected, value)["passed"], "直接输入不能绕过启动身份"))
    for label, mutate in (
        ("跨轮收据被拒绝", lambda value: value["launch_receipt"].update(run_id="old-run")),
        ("spec漂移被拒绝", lambda value: value.update(spec_sha256="e" * 64)),
        ("旧fixture被拒绝", lambda value: value["fixtures"]["bom"].update(sha256="f" * 64)),
        ("失败进程被拒绝", lambda value: value["launch_receipt"].update(exit_code=1)),
        ("跨轮字节用例被拒绝", lambda value: value["cases"][0].update(run_id="old-run")),
    ):
        broken = json.loads(json.dumps(valid))
        mutate(broken)
        test.check(label, lambda value=broken: test.expect(
            not analyze_bytes(expected, value)["passed"], "漂移身份不能通过"))
    for label, mutate in (
        ("磁盘字节变化被拒绝",
         lambda case: case.update(disk_sha256_after="0" * 64)),
        ("内存源码变化被拒绝",
         lambda case: case.update(memory_sha256_after="0" * 64)),
        ("源码原始字节变化被拒绝",
         lambda case: case.update(source_sha256_after="0" * 64)),
        ("编码输出不能替代源码比较",
         lambda case: case.pop("source_bytes_equal")),
        ("缺少磁盘精确比较被拒绝",
         lambda case: case.pop("disk_bytes_equal")),
        ("草稿被拒绝", lambda case: case.update(has_draft=True)),
        ("历史非空被拒绝", lambda case: case.update(history_len=1)),
        ("revision 变化被拒绝", lambda case: case.update(revision_after=1)),
        ("步骤缺失被拒绝", lambda case: case.update(steps=["open"])),
    ):
        broken = json.loads(json.dumps(valid))
        mutate(broken["cases"][0])
        test.check(label, lambda broken=broken: test.expect(
            not analyze_bytes(expected, broken)["passed"], label))
    missing = json.loads(json.dumps(valid))
    missing["cases"].pop()
    test.check("缺少用例被拒绝", lambda: test.expect(
        not analyze_bytes(expected, missing)["passed"], "缺少用例必须失败"))
    return test.finish()


def run_self_test(args, output):
    suites = {
        "performance": self_test_performance,
        "window-baseline": self_test_window,
        "readonly-bytes": self_test_bytes,
    }
    if args.suite:
        return suites[args.suite](output)
    codes = []
    for name, function in suites.items():
        suite_output = output / name
        suite_output.mkdir(parents=True, exist_ok=True)
        codes.append(function(suite_output))
    return 0 if all(code == 0 for code in codes) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", choices=("performance", "window-baseline",
                                            "readonly-bytes"))
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--samples", type=Path)
    parser.add_argument("--observations", type=Path)
    parser.add_argument("--results", type=Path)
    parser.add_argument("--require-matrix", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        output = output_directory("m1-single-window-readonly", args.output_dir)
    except ValueError as error:
        print(str(error), file=sys.stderr)
        return 1
    if not args.suite and not args.self_test:
        parser.error("必须指定 --suite 或 --self-test")
    started = _utc_now()
    if args.self_test:
        code = run_self_test(args, output)
    elif args.suite == "performance":
        code = run_performance(args, output)
    elif args.suite == "window-baseline":
        code = run_window_baseline(args, output)
    else:
        code = run_readonly_bytes(args, output)
    write_json(output / "command.json", {
        "argv": sys.argv,
        "cwd": str(ROOT),
        "started_utc": started,
        "finished_utc": _utc_now(),
        "exit_code": code,
        "python": sys.version.split()[0],
    })
    return code


if __name__ == "__main__":
    raise SystemExit(main())
