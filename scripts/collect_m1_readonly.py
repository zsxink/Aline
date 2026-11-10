#!/usr/bin/env python3
"""把原生窗口探针输出转换成 M1 汇总器输入。"""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from validation_output import output_directory
from evidence_paths import resolve_path, write_json, write_text

PERF_SCHEMA = "aline-m1-perf/v1"
BYTES_SCHEMA = "aline-m1-readonly-bytes/v1"
STEPS = ["open", "scroll", "hit", "jump", "visual_to_source", "source_to_visual", "close"]
STAGES = {"read_decode", "snapshot", "parse_mapping", "first_layout", "main_thread_adopt", "first_present", "mode_switch", "scroll"}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
TRACE_SCHEMA = "aline-m1-trace-metrics/v1"
TRACE_UNCERTAINTY_NS = 1_000_000
TRACE_TABLES = ("assignment.xml", "request.xml", "display.xml", "runloop.xml")
TRACE_METHOD = "external-trace-display-chain"
STALL_METHOD = "external-trace-runloop-busy"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def non_negative(value, field):
    require(isinstance(value, str) and value.isascii() and value.isdecimal(),
            "缺少非负整数字段 " + field)
    return int(value)


def parse_tsv(path):
    records = []
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = {}
        for field in line.split("\t"):
            if "=" not in field:
                raise ValueError(f"第 {line_no} 行字段格式无效")
            key, value = field.split("=", 1)
            require(key and key not in fields, f"第 {line_no} 行字段为空或重复")
            fields[key] = value
        records.append(fields)
    return records


def iso_epoch_ns(value):
    seconds, nanoseconds = divmod(int(value), 1_000_000_000)
    instant = dt.datetime.fromtimestamp(seconds, dt.timezone.utc) + dt.timedelta(
        microseconds=nanoseconds // 1000)
    return instant.isoformat()


def parse_spec(path):
    cases = {}
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        parts = line.split("|")
        if len(parts) != 6 or parts[5] not in ("perf", "bytes", "warmup"):
            raise ValueError(f"spec 第 {line_no} 行无效")
        name, condition, temperature, sample_id, source, suite = parts
        require(re.fullmatch(r"[A-Za-z0-9_-]+", name) is not None,
                f"spec 第 {line_no} 行名称无效")
        require(name not in cases, "spec 用例重复：" + name)
        require(temperature in ("cold", "hot"), "spec 冷热条件无效")
        identifier = non_negative(sample_id, "sample_id")
        cases[name] = {"condition": condition, "temperature": temperature,
                       "sample_id": identifier, "path": Path(resolve_path(source)), "suite": suite}
    if not cases:
        raise ValueError("spec 无用例")
    return cases


def collect(raw, done, metadata, spec, suite, memory_dir, output, trace_metrics=None):
    status = done.read_text(encoding="utf-8").strip()
    rows = parse_tsv(raw)
    errors = [row for row in rows if row.get("kind") == "error"]
    if status != "ok" or errors or not rows or rows[-1].get("kind") != "done":
        raise ValueError("探针未以成功状态结束，或输出含错误")
    require(sum(row.get("kind") == "done" for row in rows) == 1,
            "原始输出包含重复完成标记")
    meta = json.loads(metadata.read_text(encoding="utf-8"))
    require(isinstance(meta, dict), "运行元数据必须是对象")
    for key in ("run_id", "commit", "app_sha256", "build_profile", "environment", "run_started_utc"):
        if key not in meta:
            raise ValueError("运行元数据缺少 " + key)
    if meta["build_profile"] != "release":
        raise ValueError("仅接受 Release 运行")
    specs = parse_spec(spec)
    verify_launch(raw, meta, spec, specs, rows)
    trace_evidence = None
    if trace_metrics is not None:
        perf_cases = [name for name, case_spec in specs.items()
                      if case_spec["suite"] == "perf"]
        require(len(perf_cases) == 1, "trace 指标需要唯一 perf 用例")
        perf_case = perf_cases[0]
        perf_spec = specs[perf_case]
        stage_rows = {}
        diagnostic = None
        for row in rows:
            if row.get("kind") == "stage" and row.get("case") == perf_case:
                require(row["stage"] not in stage_rows,
                        "trace 绑定的阶段重复：" + row["stage"])
                stage_rows[row["stage"]] = row
            elif (row.get("kind") == "diagnostic"
                  and row.get("case") == perf_case
                  and row.get("metric") == "first_draw_submission_ns"):
                diagnostic = row
        require("first_layout" in stage_rows, "trace 绑定缺少 first_layout 阶段")
        require(diagnostic is not None,
                "trace 绑定缺少 first_draw_submission_ns 诊断")
        stage_epochs = {
            "open_start": non_negative(diagnostic["started_epoch_ns"], "open start"),
            "layout_start": non_negative(stage_rows["first_layout"]["started_epoch_ns"],
                                         "layout start"),
            "layout_end": non_negative(stage_rows["first_layout"]["epoch_ns"],
                                       "layout end"),
            "effect_end": non_negative(diagnostic["epoch_ns"], "effect end"),
            "window_end": max(non_negative(row["epoch_ns"], "stage end")
                              for row in stage_rows.values()),
            "trace_end": trace_metrics.get("trace", {}).get("end_epoch_ns"),
        }
        gate_rows = [row for row in rows if row.get("kind") == "gate"]
        require(len(gate_rows) == 1, "trace 证据需要且只能有一条 gate 记录")
        gate_epoch = non_negative(gate_rows[0]["epoch_ns"], "gate epoch")
        require(trace_metrics.get("trace", {}).get("start_epoch_ns",
                                                   -TRACE_UNCERTAINTY_NS)
                + TRACE_UNCERTAINTY_NS <= gate_epoch <= stage_epochs["open_start"],
                "trace 起点没有在 gate 前覆盖测量窗口")
        trace_evidence = validate_trace_metrics(
            trace_metrics, run_id=meta["run_id"], app_sha256=meta["app_sha256"],
            spec_sha256=meta["spec_sha256"], pid=meta["launch_receipt"]["pid"],
            case=perf_case, condition=perf_spec["condition"],
            temperature=perf_spec["temperature"],
            sample_id=perf_spec["sample_id"],
            fixture_sha256=meta["fixtures"][perf_case]["sha256"],
            stage_epochs=stage_epochs)
        methods = meta.get("measurement_methods", {})
        present_method = str(methods.get("first_present", "")).lower()
        require(TRACE_METHOD in present_method
                and not any(word in present_method for word in
                            ("draw-submission", "draw_submission", "effect-callback")),
                "first_present 测量方法不是外部 trace display 链")
        stall_method = str(methods.get("main_thread_stall_ns", "")).lower()
        require(STALL_METHOD in stall_method
                and not any(word in stall_method for word in
                            ("event_loop", "event-loop", "timer-gap", "jitter")),
                "main_thread_stall_ns 测量方法不是 trace runloop 忙碌段")
    cases = {}
    stages = []
    for row in rows:
        kind = row.get("kind")
        require(kind in ("run", "stage", "bytes", "diagnostic", "method", "done",
                         "gate"),
                "原始输出含未登记的记录 kind=" + str(kind))
        if kind in ("stage", "bytes", "diagnostic", "gate"):
            require(row.get("run_id") == meta["run_id"], "原始记录未绑定本轮 run_id")
        if kind == "gate":
            require(row.get("pid") == str(meta["launch_receipt"]["pid"]),
                    "gate pid 与启动收据不同")
            non_negative(row["epoch_ns"], "gate epoch")
        if kind == "bytes" and suite == "readonly-bytes":
            name = row["case"]
            case_spec = specs.get(name)
            if not case_spec or case_spec["suite"] != "bytes":
                raise ValueError("TSV 用例未登记于 spec：" + name)
            fixture = case_spec["path"]
            memory = memory_dir / (name + ".bin")
            source_before = memory_dir / (name + ".source-before.bin")
            source_after = memory_dir / (name + ".source.bin")
            opened = memory_dir / (name + ".opened.bin")
            require(name not in cases, "字节用例重复：" + name)
            require(all(path.is_file() for path in (memory, source_before, source_after, opened)),
                    "缺少实际打开字节、初始/结束内存源码或存储编码字节：" + name)
            original = opened.read_bytes()
            require(hashlib.sha256(original).hexdigest() == meta["fixtures"][name]["sha256"],
                    "实际打开字节与事前 fixture 哈希不同：" + name)
            decoded = original[3:] if original.startswith(b"\xef\xbb\xbf") else original
            require(row["has_draft"] in ("true", "false"), "草稿布尔值无效：" + name)
            cases[name] = {"name": name, "run_id": meta["run_id"], "steps": row["steps"].split(","),
                "disk_sha256_after": sha256(fixture), "memory_sha256_after": sha256(memory),
                "source_sha256_before": sha256(source_before),
                "source_sha256_after": sha256(source_after),
                "source_bytes_equal": source_before.read_bytes() == decoded == source_after.read_bytes(),
                "disk_bytes_equal": fixture.read_bytes() == original,
                "has_draft": row["has_draft"] == "true",
                "history_len": non_negative(row["history_len"], "history_len"),
                "revision_before": non_negative(row["revision_before"], "revision_before"),
                "revision_after": non_negative(row["revision_after"], "revision_after")}
        elif kind == "stage" and suite == "performance":
            name = row["case"]
            case_spec = specs.get(name)
            if not case_spec or case_spec["suite"] != "perf":
                raise ValueError("TSV 用例未登记于 spec：" + name)
            if row["stage"] not in STAGES:
                raise ValueError("未知阶段：" + row["stage"])
            for field in ("condition", "temperature"):
                require(row[field] == case_spec[field], "原始记录与 spec 的 " + field + " 不同")
            require(non_negative(row["sample_id"], "sample_id") == case_spec["sample_id"],
                    "原始记录与 spec 的 sample_id 不同")
            opened = memory_dir / (name + ".opened.bin")
            fixture_hash = meta["fixtures"][name]["sha256"]
            require(opened.is_file() and sha256(opened) == fixture_hash,
                    "缺少与事前哈希一致的实际打开字节：" + name)
            require(sha256(case_spec["path"]) == fixture_hash,
                    "采集期间磁盘夹具改变：" + name)
            end_ns = non_negative(row["epoch_ns"], "epoch_ns")
            duration = non_negative(row["duration_ns"], "duration_ns")
            require(row.get("timing_origin") == "measured-boundaries",
                    "阶段缺少真实起止边界，不能把采纳时刻回推成后台时间线")
            started_ns = non_negative(row["started_epoch_ns"], "started_epoch_ns")
            uncertainty = non_negative(row.get("clock_uncertainty_ns", "0"), "clock_uncertainty_ns")
            require(uncertainty <= 1_000_000, "墙钟捕获开销超过 1 ms，需重采")
            require(row.get("timing_clock") == "instant-monotonic", "阶段缺少单调时钟身份")
            monotonic_start = non_negative(row.get("started_monotonic_ns"), "started_monotonic_ns")
            monotonic_end = non_negative(row.get("finished_monotonic_ns"), "finished_monotonic_ns")
            require(monotonic_end >= monotonic_start and monotonic_end - monotonic_start == duration,
                    "阶段单调起止边界与耗时不一致")
            require(end_ns >= started_ns, "墙钟倒退，无法关联本轮外部时间线")
            stage = {"schema": PERF_SCHEMA, "kind": "sample", "run_id": meta["run_id"],
                "case": name,
                "condition": row["condition"], "temperature": row["temperature"], "stage": row["stage"],
                "sample_id": int(row["sample_id"]), "status": "ok", "fixture_sha256": fixture_hash,
                "commit": meta["commit"], "app_sha256": meta["app_sha256"],
                "native_pid": meta["launch_receipt"]["pid"],
                "build_profile": "release", "started_utc": iso_epoch_ns(started_ns),
                "finished_utc": iso_epoch_ns(end_ns), "duration_ns": duration,
                "timing_origin": row["timing_origin"], "clock_uncertainty_ns": uncertainty,
                "timing_clock": "instant-monotonic", "started_monotonic_ns": monotonic_start,
                "finished_monotonic_ns": monotonic_end, "wall_duration_ns": end_ns - started_ns}
            if row["stage"] in ("first_layout", "first_present", "scroll"):
                stage["virtual_items"] = non_negative(row["virtual_items"], "virtual_items")
            # 缺失指标保持缺失，由汇总器拒绝；调度间隙不得改名为主线程停顿。
            for field in ("main_thread_stall_ns", "peak_rss_bytes"):
                if field in row:
                    method = meta.get("measurement_methods", {}).get(field)
                    require(isinstance(method, str) and method.strip()
                            and "event_loop_gap" not in method,
                            field + " 缺少可信测量方法")
                    stage[field] = non_negative(row[field], field)
                    if field == "peak_rss_bytes":
                        require(stage[field] > 0, "进程峰值 RSS 必须大于零")
            rss = meta.get("process_metrics", {})
            if "peak_rss_bytes" not in stage:
                _attach_rss(stage, meta)
            if trace_evidence is not None:
                require(row["stage"] != "first_present",
                        "原始探针不得把 draw effect 写成 first_present")
                stall_ns = trace_evidence["main_thread_stall"]["duration_ns"]
                if "main_thread_stall_ns" in stage:
                    require(stage["main_thread_stall_ns"] == stall_ns,
                            "阶段行内停顿与 trace 证据不一致")
                stage["main_thread_stall_ns"] = stall_ns
                stage["main_thread_stall_origin"] = STALL_METHOD
                stage["trace"] = _trace_block(trace_evidence["trace"])
                require(started_ns >= trace_evidence["trace"]["start_epoch_ns"]
                        and end_ns <= trace_evidence["trace"]["end_epoch_ns"],
                        "阶段不在 trace 覆盖范围内")
            stages.append(stage)
    if trace_evidence is not None:
        fp = trace_evidence["first_present"]
        stall_ns = trace_evidence["main_thread_stall"]["duration_ns"]
        sample = {"schema": PERF_SCHEMA, "kind": "sample", "run_id": meta["run_id"],
                  "case": perf_case, "condition": perf_spec["condition"],
                  "temperature": perf_spec["temperature"], "stage": "first_present",
                  "sample_id": perf_spec["sample_id"], "status": "ok",
                  "fixture_sha256": meta["fixtures"][perf_case]["sha256"],
                  "commit": meta["commit"], "app_sha256": meta["app_sha256"],
                  "native_pid": meta["launch_receipt"]["pid"],
                  "build_profile": "release",
                  "started_utc": iso_epoch_ns(fp["started_epoch_ns"]),
                  "finished_utc": iso_epoch_ns(fp["epoch_ns"]),
                  "duration_ns": fp["duration_ns"],
                  "timing_origin": "trace-display-chain",
                  "timing_clock": "trace-relative",
                  "clock_uncertainty_ns": TRACE_UNCERTAINTY_NS,
                  "started_epoch_ns": fp["started_epoch_ns"],
                  "finished_epoch_ns": fp["epoch_ns"],
                  "layout_end_epoch_ns": fp["layout_end_epoch_ns"],
                  "effect_end_epoch_ns": fp["effect_end_epoch_ns"],
                  "content_frame": {
                      "command_buffer": fp["content_frame"]["command_buffer"],
                      "frame_number": fp["content_frame"]["frame_number"],
                      "surface_id": fp["content_frame"]["surface_id"],
                      "commit_trace_ns": fp["content_frame"]["commit_trace_ns"],
                      "commit_epoch_ns": fp["content_frame"]["commit_epoch_ns"],
                      "request_trace_ns": fp["content_frame"]["request_trace_ns"],
                      "request_epoch_ns": fp["content_frame"]["request_epoch_ns"],
                      "display_start_trace_ns": fp["content_frame"]["display_start_trace_ns"],
                      "display_start_epoch_ns": fp["content_frame"]["display_start_epoch_ns"],
                      "cpu_to_display_latency_ns": fp["content_frame"]["cpu_to_display_latency_ns"],
                      "primary": fp["content_frame"]["primary"],
                      "primary_commit_trace_ns": fp["content_frame"]["primary_commit_trace_ns"],
                      "primary_commit_epoch_ns": fp["content_frame"]["primary_commit_epoch_ns"],
                  },
                  "virtual_items": fp["virtual_items"],
                  "main_thread_stall_ns": stall_ns,
                  "main_thread_stall_origin": STALL_METHOD,
                  "trace": _trace_block(trace_evidence["trace"]),
                  "wall_duration_ns": fp["epoch_ns"] - fp["started_epoch_ns"]}
        _attach_rss(sample, meta)
        stages.append(sample)
    output.parent.mkdir(parents=True, exist_ok=True)
    if suite == "readonly-bytes":
        payload = {"schema": BYTES_SCHEMA, **{k: meta[k] for k in ("run_id", "commit", "build_profile", "app_sha256", "spec_sha256", "fixtures", "launch_receipt")}, "cases": list(cases.values())}
        write_json(output, payload)
    else:
        run = {"schema": PERF_SCHEMA, "kind": "run", **{k: meta[k] for k in ("run_id", "commit", "app_sha256", "build_profile", "run_started_utc", "environment", "spec_sha256", "fixtures", "launch_receipt")},
               "measurement_methods": meta.get("measurement_methods", {})}
        if trace_metrics is not None:
            run["trace_metrics"] = trace_metrics
        write_text(output, "\n".join(json.dumps(x, ensure_ascii=False) for x in [run, *stages])+"\n")


def verify_launch(raw, meta, spec, specs, rows):
    """核对实际 runner 的启动收据；环境变量中的身份声明本身不构成证据。"""
    require(SHA256.fullmatch(str(meta.get("app_sha256", ""))) is not None,
            "app_sha256 无效")
    require(meta.get("spec_sha256") == sha256(spec), "spec 与事前哈希不一致")
    manifests = meta.get("fixtures")
    require(isinstance(manifests, dict) and set(manifests) == set(specs),
            "缺少完整事前 fixture 清单")
    for name, case in specs.items():
        manifest = manifests[name]
        require(isinstance(manifest, dict), "fixture 清单条目不是对象")
        for field in ("condition", "temperature", "sample_id", "suite"):
            require(manifest.get(field) == case[field], "事前 fixture 清单与 spec 不同：" + name)
        require(SHA256.fullmatch(str(manifest.get("sha256", ""))) is not None,
                "事前 fixture 哈希无效：" + name)
    header = rows[0]
    require(header.get("kind") == "run" and sum(row.get("kind") == "run" for row in rows) == 1,
            "缺少唯一原生运行头")
    for field in ("run_id", "app_sha256", "spec_sha256"):
        require(header.get(field) == meta.get(field), "原生运行头与元数据身份不一致：" + field)
    receipt = meta.get("launch_receipt")
    require(isinstance(receipt, dict), "缺少实际启动器的 launch_receipt；身份声明不能证明运行")
    require(receipt.get("schema") == "aline-m1-launch/v1", "启动收据 schema 无效")
    for field in ("run_id", "app_sha256", "spec_sha256", "fixtures"):
        require(receipt.get(field) == meta.get(field), "启动收据与元数据不一致：" + field)
    require(receipt.get("raw_sha256") == sha256(raw), "原始输出与启动收据哈希不一致")
    require(isinstance(receipt.get("pid"), int) and not isinstance(receipt["pid"], bool)
            and receipt["pid"] > 0, "启动收据缺少实际 pid")
    require(receipt.get("exit_code") == 0 and not isinstance(receipt.get("exit_code"), bool),
            "原生进程未成功退出")
    times = {}
    for field in ("started_utc", "finished_utc"):
        require(isinstance(receipt.get(field), str), "启动收据缺少 " + field)
        instant = dt.datetime.fromisoformat(receipt[field].replace("Z", "+00:00"))
        require(instant.tzinfo is not None, "启动收据时间缺少时区")
        times[field] = instant
    require(times["finished_utc"] >= times["started_utc"], "启动收据结束早于开始")
    require(receipt["started_utc"] == meta["run_started_utc"], "启动收据开始时间不一致")
    if "pid" in header:
        require(non_negative(header["pid"], "pid") == receipt["pid"], "原生运行头 pid 与启动收据不同")


def _trace_int(value, field):
    require(isinstance(value, int) and not isinstance(value, bool) and value >= 0,
            "trace 指标缺少非负整数字段 " + field)
    return value


def _trace_block(trace):
    return {"schema": TRACE_SCHEMA, "template": trace["template"],
            "start_epoch_ns": trace["start_epoch_ns"],
            "end_epoch_ns": trace["end_epoch_ns"],
            "start_uncertainty_ns": trace["start_uncertainty_ns"],
            "pid": trace["pid"], "raw_sha256": trace["raw_sha256"]}


def _attach_rss(sample, meta):
    rss = meta.get("process_metrics", {})
    require(rss.get("method") == "getrusage-single-native-child-high-water",
            "缺少进程峰值 RSS 测量方法")
    require(isinstance(rss.get("peak_rss_bytes"), int)
            and not isinstance(rss["peak_rss_bytes"], bool)
            and rss["peak_rss_bytes"] > 0, "进程峰值 RSS 无效")
    require(rss.get("pid") == meta["launch_receipt"]["pid"],
            "RSS 未绑定实际原生 pid")
    sample["peak_rss_bytes"] = rss["peak_rss_bytes"]
    sample["peak_rss_scope"] = "whole-native-process-including-warmup"


def validate_trace_metrics(metrics, *, run_id, app_sha256, spec_sha256, pid, case,
                           condition, temperature, sample_id, fixture_sha256,
                           stage_epochs=None):
    """外部 trace 证据的独立校验；伪 present/伪 stall 或身份漂移都失败关闭。"""
    require(isinstance(metrics, dict), "trace 指标必须是对象")
    require(metrics.get("schema") == TRACE_SCHEMA, "trace 指标 schema 不匹配")
    for field, expected in (("run_id", run_id), ("app_sha256", app_sha256),
                            ("spec_sha256", spec_sha256), ("pid", pid),
                            ("case", case), ("condition", condition),
                            ("temperature", temperature), ("sample_id", sample_id),
                            ("fixture_sha256", fixture_sha256)):
        require(metrics.get(field) == expected, "trace 指标身份与运行不一致：" + field)
    trace = metrics.get("trace")
    require(isinstance(trace, dict), "trace 指标缺少 trace 块")
    template = trace.get("template")
    require(isinstance(template, str) and "Metal" in template,
            "trace 模板不是 Metal System Trace")
    start = _trace_int(trace.get("start_epoch_ns"), "trace.start_epoch_ns")
    end = _trace_int(trace.get("end_epoch_ns"), "trace.end_epoch_ns")
    require(end > start, "trace 起止无效")
    require(_trace_int(trace.get("start_uncertainty_ns"),
                       "trace.start_uncertainty_ns") == TRACE_UNCERTAINTY_NS,
            "trace 起点不确定度不是 1 ms")
    require(trace.get("pid") == pid, "trace 目标 pid 与运行收据不同")
    require(isinstance(trace.get("process_name"), str)
            and "aline" in trace["process_name"], "trace 目标不是 aline")
    hashes = trace.get("raw_sha256")
    require(isinstance(hashes, dict) and set(TRACE_TABLES) <= set(hashes),
            "trace 缺少原始表哈希")
    for name in TRACE_TABLES:
        require(SHA256.fullmatch(str(hashes[name])) is not None,
                "trace 原始表哈希无效：" + name)

    fp = metrics.get("first_present")
    require(isinstance(fp, dict), "trace 指标缺少 first_present 块")
    require(fp.get("stage") == "first_present", "first_present 阶段名无效")
    require(fp.get("timing_origin") == "trace-display-chain",
            "first_present 不是外部 trace display 链")
    require(fp.get("timing_clock") == "trace-relative",
            "first_present 时钟不是 trace-relative")
    require(_trace_int(fp.get("trace_uncertainty_ns"),
                       "first_present.trace_uncertainty_ns") == TRACE_UNCERTAINTY_NS,
            "first_present 不确定度不是 1 ms")
    fp_started = _trace_int(fp.get("started_epoch_ns"), "first_present.started_epoch_ns")
    fp_end = _trace_int(fp.get("epoch_ns"), "first_present.epoch_ns")
    fp_duration = _trace_int(fp.get("duration_ns"), "first_present.duration_ns")
    require(fp_end > fp_started and fp_end - fp_started == fp_duration,
            "first_present 起止与耗时不一致")
    _trace_int(fp.get("virtual_items"), "first_present.virtual_items")
    layout_start = _trace_int(fp.get("layout_start_epoch_ns"), "layout_start")
    layout_end = _trace_int(fp.get("layout_end_epoch_ns"), "layout_end")
    effect_end = _trace_int(fp.get("effect_end_epoch_ns"), "effect_end")
    require(start <= fp_started <= fp_end <= end, "first_present 不在 trace 覆盖范围内")
    require(layout_start < layout_end <= effect_end,
            "首内容帧 layout/effect 边界顺序无效")
    # draw 提交回调可能晚于实际上屏，effect_end 只作诊断，不是 display 下界。
    frame = fp.get("content_frame")
    require(isinstance(frame, dict), "first_present 缺少 content_frame")
    commit_trace = _trace_int(frame.get("commit_trace_ns"), "commit_trace_ns")
    request_trace = _trace_int(frame.get("request_trace_ns"), "request_trace_ns")
    display_trace = _trace_int(frame.get("display_start_trace_ns"),
                               "display_start_trace_ns")
    latency = _trace_int(frame.get("cpu_to_display_latency_ns"),
                         "cpu_to_display_latency_ns")
    commit_epoch = _trace_int(frame.get("commit_epoch_ns"), "commit_epoch_ns")
    request_epoch = _trace_int(frame.get("request_epoch_ns"), "request_epoch_ns")
    display_epoch = _trace_int(frame.get("display_start_epoch_ns"),
                               "display_start_epoch_ns")
    primary = frame.get("primary")
    require(isinstance(primary, bool), "content_frame.primary 必须是布尔值")
    primary_commit_trace = _trace_int(frame.get("primary_commit_trace_ns"),
                                      "primary_commit_trace_ns")
    primary_commit_epoch = _trace_int(frame.get("primary_commit_epoch_ns"),
                                      "primary_commit_epoch_ns")
    require(commit_epoch == start + commit_trace, "commit epoch 与 trace 起点不一致")
    require(request_epoch == start + request_trace, "request epoch 与 trace 起点不一致")
    require(display_epoch == start + display_trace, "display epoch 与 trace 起点不一致")
    require(primary_commit_epoch == start + primary_commit_trace,
            "primary commit epoch 与 trace 起点不一致")
    require(display_epoch == fp_end, "first_present 结束不是 display start")
    # request 的因果上界是 display 扫描开始；首个内容帧的 CA request 可
    # 晚于 commit 2–5 ms，不能再用固定 2 ms 上限拒绝真实上屏帧。
    require(commit_trace < request_trace < display_trace,
            "present request 与 commit/display 顺序无效")
    require(display_trace - primary_commit_trace == latency,
            "cpu-to-display-latency 与 start-primary_commit 不一致")
    require(primary == (primary_commit_trace == commit_trace),
            "content_frame.primary 与 commit 身份不一致")
    require(commit_epoch + TRACE_UNCERTAINTY_NS >= layout_end,
            "首内容帧 commit 早于 layout 结束")
    require(request_epoch + TRACE_UNCERTAINTY_NS >= layout_end,
            "首内容帧 request 早于 layout 结束")
    require(isinstance(frame.get("command_buffer"), str)
            and frame["command_buffer"], "content_frame 缺少 command buffer")
    _trace_int(frame.get("frame_number"), "frame_number")
    _trace_int(frame.get("surface_id"), "surface_id")

    stall = metrics.get("main_thread_stall")
    require(isinstance(stall, dict), "trace 指标缺少 main_thread_stall 块")
    require(stall.get("stage") == "main_thread_stall", "main_thread_stall 阶段名无效")
    require(stall.get("timing_origin") == "trace-runloop-busy",
            "main_thread_stall 不是 trace runloop 忙碌段")
    require(stall.get("timing_clock") == "trace-relative",
            "main_thread_stall 时钟不是 trace-relative")
    require(_trace_int(stall.get("trace_uncertainty_ns"),
                       "stall.trace_uncertainty_ns") == TRACE_UNCERTAINTY_NS,
            "main_thread_stall 不确定度不是 1 ms")
    stall_duration = _trace_int(stall.get("duration_ns"), "stall.duration_ns")
    require(stall_duration > 0, "main_thread_stall 必须为正")
    require(stall.get("timestamp_accuracy") == "recorded",
            "main_thread_stall 不是 recorded")
    require(stall.get("is_main") is True, "main_thread_stall 不是主线程")
    thread = stall.get("thread")
    require(isinstance(thread, str) and "aline" in thread and str(pid) in thread,
            "main_thread_stall 线程不属于目标进程")
    window_start_epoch = _trace_int(stall.get("window_start_epoch_ns"),
                                    "stall.window_start_epoch_ns")
    window_end_epoch = _trace_int(stall.get("window_end_epoch_ns"),
                                  "stall.window_end_epoch_ns")
    require(window_start_epoch == fp_started, "主线程停顿窗口起点不是打开请求")
    require(window_start_epoch < window_end_epoch <= end, "主线程停顿窗口无效")
    window_start_trace = _trace_int(stall.get("window_start_trace_ns"),
                                    "stall.window_start_trace_ns")
    window_end_trace = _trace_int(stall.get("window_end_trace_ns"),
                                  "stall.window_end_trace_ns")
    require(window_start_trace == window_start_epoch - start,
            "停顿窗口起点与 trace 起点不一致")
    require(window_end_trace == window_end_epoch - start,
            "停顿窗口终点与 trace 起点不一致")
    require(window_start_trace >= TRACE_UNCERTAINTY_NS, "停顿窗口起点余量不足")
    require(window_end_trace <= end - start, "停顿窗口超出 trace")
    interval_start = _trace_int(stall.get("interval_start_trace_ns"),
                                "stall.interval_start_trace_ns")
    interval_end = _trace_int(stall.get("interval_end_trace_ns"),
                              "stall.interval_end_trace_ns")
    interval_duration = _trace_int(stall.get("interval_duration_ns"),
                                   "stall.interval_duration_ns")
    waiting = _trace_int(stall.get("waiting_ns"), "stall.waiting_ns")
    longest = _trace_int(stall.get("longest_busy_ns"), "stall.longest_busy_ns")
    total = _trace_int(stall.get("total_busy_ns"), "stall.total_busy_ns")
    require(interval_end > interval_start
            and interval_end - interval_start == interval_duration,
            "停顿区间起止与耗时不一致")
    require(window_start_trace + TRACE_UNCERTAINTY_NS <= interval_start,
            "停顿区间早于窗口起点")
    require(interval_end <= window_end_trace, "停顿区间晚于窗口终点")
    require(interval_duration == waiting + total, "停顿区间未精确扣除等待")
    require(longest == stall_duration, "main_thread_stall 时长不是最长忙碌段")
    require(0 < longest <= total <= interval_duration, "停顿忙碌段关系无效")
    segments = stall.get("segments")
    require(isinstance(segments, list) and segments, "停顿缺少忙碌段")
    total_segments = 0
    last_end = interval_start
    for segment in segments:
        require(isinstance(segment, list) and len(segment) == 2, "忙碌段格式无效")
        seg_start = _trace_int(segment[0], "segment.start")
        seg_end = _trace_int(segment[1], "segment.end")
        require(seg_start >= last_end and seg_start < seg_end <= interval_end,
                "忙碌段顺序/范围无效")
        total_segments += seg_end - seg_start
        last_end = seg_end
    require(total_segments == total, "忙碌段总长与 total_busy_ns 不一致")
    require(max(segment[1] - segment[0] for segment in segments) == longest,
            "忙碌段最大长度与 longest_busy_ns 不一致")
    candidates = stall.get("candidates")
    require(isinstance(candidates, list) and candidates, "停顿缺少候选区间")
    for candidate in candidates:
        require(isinstance(candidate, dict), "停顿候选不是对象")
        c_start = _trace_int(candidate.get("interval_start_trace_ns"), "candidate.start")
        c_end = _trace_int(candidate.get("interval_end_trace_ns"), "candidate.end")
        c_duration = _trace_int(candidate.get("interval_duration_ns"), "candidate.duration")
        c_wait = _trace_int(candidate.get("waiting_ns"), "candidate.waiting")
        c_longest = _trace_int(candidate.get("longest_busy_ns"), "candidate.longest")
        c_total = _trace_int(candidate.get("total_busy_ns"), "candidate.total")
        require(c_end > c_start and c_end - c_start == c_duration,
                "候选区间起止与耗时不一致")
        require(c_duration == c_wait + c_total, "候选区间未精确扣除等待")
        require(0 < c_longest <= c_total, "候选忙碌段关系无效")
        require(candidate.get("timestamp_accuracy") == "recorded"
                and candidate.get("is_main") is True, "候选不是 recorded 主线程")
        c_segments = candidate.get("segments")
        require(isinstance(c_segments, list) and c_segments, "候选缺少忙碌段")
        require(sum(b - a for a, b in c_segments) == c_total,
                "候选忙碌段总长不一致")
        require(max(b - a for a, b in c_segments) == c_longest,
                "候选最长忙碌段不一致")
        require(c_start >= window_start_trace + TRACE_UNCERTAINTY_NS
                and c_end <= window_end_trace, "候选不在窗口内")
    require(max(candidate["longest_busy_ns"] for candidate in candidates) == longest,
            "选中的停顿不是候选最大值")
    chosen = [candidate for candidate in candidates
              if candidate.get("interval_identifier") == stall.get("interval_identifier")]
    require(len(chosen) == 1, "停顿区间标识不是唯一候选")
    require(chosen[0]["longest_busy_ns"] == longest
            and chosen[0]["interval_start_trace_ns"] == interval_start
            and chosen[0]["interval_end_trace_ns"] == interval_end,
            "停顿区间与候选不一致")

    if stage_epochs is not None:
        require(fp_started == stage_epochs["open_start"],
                "first_present 起点不是原始打开请求")
        require(layout_start == stage_epochs["layout_start"],
                "layout_start 与原始阶段不一致")
        require(layout_end == stage_epochs["layout_end"],
                "layout_end 与原始阶段不一致")
        require(effect_end == stage_epochs["effect_end"],
                "effect_end 与原始诊断不一致")
        require(window_end_epoch == stage_epochs["window_end"],
                "停顿窗口终点不是最后阶段")
        require(fp_end <= stage_epochs["trace_end"],
                "first_present 晚于 trace 结束")
    return {"trace": trace, "first_present": fp, "main_thread_stall": stall}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", type=Path)
    parser.add_argument("--done", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--spec", type=Path)
    parser.add_argument("--suite", choices=("performance", "readonly-bytes"))
    parser.add_argument("--memory-dir", type=Path, default=Path("."))
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--trace-metrics", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        output_dir = output_directory("m1-readonly-collector", args.output_dir)
    except ValueError as error:
        print("collect: 输出目录必须位于 logs/validation：" + str(error), file=sys.stderr)
        return 1
    if args.self_test:
        self_test()
        self_test_performance_boundaries()
        self_test_trace_metrics()
        print("self-test: 通过")
        return 0
    if not all((args.raw, args.done, args.metadata, args.spec, args.suite)):
        parser.error("采集需提供 --raw --done --metadata --spec --suite")
    output = output_dir / ("results.json" if args.suite == "readonly-bytes" else "measurements.jsonl")
    try:
        trace_metrics = (json.loads(args.trace_metrics.read_text(encoding="utf-8"))
                         if args.trace_metrics else None)
        collect(args.raw, args.done, args.metadata, args.spec, args.suite,
                args.memory_dir, output, trace_metrics=trace_metrics)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print("collect: 未通过：" + str(error), file=sys.stderr)
        return 1
    print("collect: 已写入 " + output.name)
    return 0


def parse_tsv_text(text):
    import tempfile
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "input.tsv"
        path.write_text(text, encoding="utf-8")
        return parse_tsv(path)


def self_test():
    assert parse_tsv_text("kind=done") == [{"kind": "done"}]
    try:
        parse_tsv_text("broken")
        raise AssertionError("invalid TSV accepted")
    except ValueError:
        pass
    for invalid in ("kind=done\tkind=stage", "=done"):
        try:
            parse_tsv_text(invalid)
            raise AssertionError("duplicate/empty field accepted")
        except ValueError:
            pass
    import tempfile
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "raw").write_text("kind=error\tcase=x\treason=bad\nkind=done\n", encoding="utf-8")
        (root / "done").write_text("failed", encoding="utf-8")
        (root / "meta").write_text("{}", encoding="utf-8")
        (root / "spec").write_text("x|32KiB|cold|0|fixture.md|bytes\n", encoding="utf-8")
        try:
            collect(root / "raw", root / "done", root / "meta", root / "spec", "readonly-bytes", root, root / "out")
            raise AssertionError("failed probe accepted")
        except ValueError:
            pass
    # 合成夹具仅校验采集契约，不作为原生运行或保真验收成绩。
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        fixture = root / "fixture.md"
        original = b"\xef\xbb\xbf# x\r\n"
        fixture.write_bytes(original)
        (root / "x.opened.bin").write_bytes(original)
        (root / "x.bin").write_bytes(original)
        for name in ("x.source-before.bin", "x.source.bin"):
            (root / name).write_bytes(original[3:])
        raw, done, meta_path, spec = (root / name for name in ("raw", "done", "meta", "spec"))
        spec.write_text("x|32KiB|cold|0|" + str(fixture) + "|bytes\n", encoding="utf-8")
        meta = {"run_id": "synthetic", "commit": "a" * 40, "app_sha256": "b" * 64,
                "build_profile": "release", "environment": {},
                "run_started_utc": "2026-11-08T00:00:00+00:00", "spec_sha256": sha256(spec),
                "fixtures": {"x": {"condition": "32KiB", "temperature": "cold",
                    "sample_id": 0, "suite": "bytes", "sha256": sha256(fixture)}}}
        raw.write_text("kind=run\trun_id=synthetic\tapp_sha256=" + meta["app_sha256"]
            + "\tspec_sha256=" + meta["spec_sha256"] + "\nkind=bytes\trun_id=synthetic"
            "\tcase=x\tsteps=" + ",".join(STEPS) + "\thas_draft=false\thistory_len=0"
            "\trevision_before=0\trevision_after=0\nkind=done\n", encoding="utf-8")
        done.write_text("ok", encoding="utf-8")
        receipt = {"schema": "aline-m1-launch/v1", "pid": 1, "exit_code": 0,
                   "started_utc": meta["run_started_utc"],
                   "finished_utc": "2026-11-08T00:00:01+00:00", "raw_sha256": sha256(raw),
                   **{key: meta[key] for key in ("run_id", "app_sha256", "spec_sha256", "fixtures")}}
        meta["launch_receipt"] = receipt
        meta_path.write_text(json.dumps(meta), encoding="utf-8")
        collect(raw, done, meta_path, spec, "readonly-bytes", root, root / "out")
        payload = json.loads((root / "out").read_text(encoding="utf-8"))
        assert payload["cases"][0]["source_bytes_equal"] is True
        assert payload["cases"][0]["source_sha256_after"] == hashlib.sha256(original[3:]).hexdigest()
        # 同一编码结果仍可能掩盖源码 CRLF→LF 变化；实际源码字节必须另行比较。
        (root / "x.source.bin").write_bytes(original[3:].replace(b"\r\n", b"\n"))
        collect(raw, done, meta_path, spec, "readonly-bytes", root, root / "changed-source")
        changed = json.loads((root / "changed-source").read_text(encoding="utf-8"))["cases"][0]
        assert changed["memory_sha256_after"] == hashlib.sha256(original).hexdigest()
        assert changed["source_bytes_equal"] is False
        (root / "x.source.bin").write_bytes(original[3:])
        for label, mutate in (
            ("missing launch receipt", lambda value: value.pop("launch_receipt")),
            ("stale raw identity", lambda value: value.update(run_id="another-run")),
            ("spec drift", lambda value: value.update(spec_sha256="0" * 64)),
            ("missing prior fixture", lambda value: value.update(fixtures={})),
            ("failed process", lambda value: value["launch_receipt"].update(exit_code=1)),
        ):
            broken = json.loads(json.dumps(meta))
            mutate(broken)
            meta_path.write_text(json.dumps(broken), encoding="utf-8")
            try:
                collect(raw, done, meta_path, spec, "readonly-bytes", root, root / "bad-out")
                raise AssertionError(label + " accepted")
            except ValueError:
                pass
            assert not (root / "bad-out").exists()


def self_test_performance_boundaries():
    """固定量化时钟反例：真实单调100ns与墙钟1000ns分别保留，不能互相倒推。"""
    import tempfile
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        fixture = root / "fixture.md"
        fixture.write_bytes(b"x")
        (root / "x.opened.bin").write_bytes(b"x")
        spec = root / "spec"
        spec.write_text("x|32KiB|cold|0|" + str(fixture) + "|perf\n")
        raw, done, metadata = (root / name for name in ("raw", "done", "metadata"))
        done.write_text("ok")
        start = int(dt.datetime(2026, 11, 8, tzinfo=dt.timezone.utc).timestamp()) * 1_000_000_000
        meta = {"run_id": "synthetic-p", "commit": "a" * 40, "app_sha256": "b" * 64,
                "spec_sha256": sha256(spec), "build_profile": "release", "environment": {},
                "run_started_utc": "2026-11-08T00:00:00+00:00",
                "measurement_methods": {"peak_rss_bytes": "synthetic contract only"},
                "fixtures": {"x": {"condition": "32KiB", "temperature": "cold", "sample_id": 0,
                                   "suite": "perf", "sha256": sha256(fixture)}},
                "process_metrics": {"method": "getrusage-single-native-child-high-water", "pid": 1, "peak_rss_bytes": 4096}}
        header = "kind=run\trun_id=synthetic-p\tapp_sha256=" + meta["app_sha256"] + "\tspec_sha256=" + meta["spec_sha256"] + "\tpid=1\n"
        row = {"kind": "stage", "run_id": "synthetic-p", "case": "x", "condition": "32KiB",
               "temperature": "cold", "sample_id": "0", "stage": "first_layout", "duration_ns": "100",
               "started_epoch_ns": str(start), "epoch_ns": str(start + 1000), "clock_uncertainty_ns": "84",
               "timing_origin": "measured-boundaries", "timing_clock": "instant-monotonic",
               "started_monotonic_ns": "0", "finished_monotonic_ns": "100", "virtual_items": "1"}
        def exercise(value, output):
            raw.write_text(header + "\t".join(key + "=" + item for key, item in value.items()) + "\nkind=done\n")
            meta["launch_receipt"] = {"schema": "aline-m1-launch/v1", "pid": 1, "exit_code": 0,
                "started_utc": meta["run_started_utc"], "finished_utc": "2026-11-08T00:00:01+00:00",
                "raw_sha256": sha256(raw), **{key: meta[key] for key in ("run_id", "app_sha256", "spec_sha256", "fixtures")}}
            metadata.write_text(json.dumps(meta))
            collect(raw, done, metadata, spec, "performance", root, output)
        exercise(row, root / "ok")
        sample = json.loads((root / "ok").read_text().splitlines()[1])
        assert sample["duration_ns"] == 100 and sample["wall_duration_ns"] == 1000
        for label, change in (
            ("missing monotonic boundary", lambda value: value.pop("started_monotonic_ns")),
            ("mismatched monotonic boundary", lambda value: value.update(finished_monotonic_ns="101")),
            ("missing clock identity", lambda value: value.pop("timing_clock")),
            ("zero RSS", lambda value: value.update(peak_rss_bytes="0")),
        ):
            broken = dict(row)
            change(broken)
            try:
                exercise(broken, root / "bad")
                raise AssertionError(label + " accepted")
            except ValueError:
                pass
            assert not (root / "bad").exists()


def self_test_trace_metrics():
    """外部 trace 证据的正例与最小违规反例；合成数据不是原生测量成绩。"""
    import tempfile
    import m1_trace_metrics as trace

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        fixture = root / "fixture.md"
        fixture.write_bytes(b"x")
        (root / "x.opened.bin").write_bytes(b"x")
        spec = root / "spec"
        spec.write_text("x|32KiB|cold|0|" + str(fixture) + "|perf\n",
                        encoding="utf-8")
        spec_hash = sha256(spec)
        fixture_hash = sha256(fixture)
        run_id = "synthetic-trace"
        app_hash = "b" * 64
        raw_rows, toc, tables, _ = trace._synthetic()
        for row in raw_rows:
            if "run_id" in row:
                row["run_id"] = run_id
            if "case" in row:
                row["case"] = "x"
        for index, row in enumerate(raw_rows):
            if row.get("kind") == "run":
                row.update(app_sha256=app_hash, spec_sha256=spec_hash, pid="1")
            elif row.get("kind") == "stage":
                started = int(row["started_epoch_ns"])
                ended = int(row["epoch_ns"])
                row.update(duration_ns=str(ended - started),
                           clock_uncertainty_ns="100",
                           timing_origin="measured-boundaries",
                           timing_clock="instant-monotonic",
                           started_monotonic_ns=str(index * 1_000_000_000),
                           finished_monotonic_ns=str(index * 1_000_000_000
                                                     + ended - started),
                           virtual_items=row.get("virtual_items", "0"))
            elif row.get("kind") == "diagnostic":
                row.update(clock_uncertainty_ns="100",
                           timing_clock="instant-monotonic",
                           started_monotonic_ns="0",
                           finished_monotonic_ns=str(int(row["epoch_ns"])
                                                     - int(row["started_epoch_ns"])))
        expected = {"run_id": run_id, "app_sha256": app_hash,
                    "spec_sha256": spec_hash, "pid": 1, "case": "x",
                    "condition": "32KiB", "temperature": "cold",
                    "sample_id": 0, "fixture_sha256": fixture_hash}
        metrics = trace.derive(raw_rows, toc, tables, expected)
        metrics["trace"]["raw_sha256"] = {name: "0" * 64 for name in TRACE_TABLES}
        raw = root / "raw"
        raw.write_text("\n".join("\t".join(key + "=" + value
                                           for key, value in row.items())
                                 for row in raw_rows) + "\n", encoding="utf-8")
        (root / "done").write_text("ok", encoding="utf-8")
        meta = {"run_id": run_id, "commit": "a" * 40, "app_sha256": app_hash,
                "spec_sha256": spec_hash, "build_profile": "release",
                "environment": {},
                "run_started_utc": "2026-11-08T00:00:00+00:00",
                "measurement_methods": {
                    "first_present": TRACE_METHOD + "; synthetic contract only",
                    "main_thread_stall_ns": STALL_METHOD + "; synthetic contract only",
                    "peak_rss_bytes": "synthetic contract only"},
                "fixtures": {"x": {"condition": "32KiB", "temperature": "cold",
                                   "sample_id": 0, "suite": "perf",
                                   "sha256": fixture_hash}},
                "process_metrics": {
                    "method": "getrusage-single-native-child-high-water",
                    "pid": 1, "peak_rss_bytes": 4096}}
        meta["launch_receipt"] = {
            "schema": "aline-m1-launch/v1", "pid": 1, "exit_code": 0,
            "started_utc": meta["run_started_utc"],
            "finished_utc": "2026-11-08T00:00:01+00:00",
            "raw_sha256": sha256(raw),
            **{key: meta[key] for key in ("run_id", "app_sha256",
                                          "spec_sha256", "fixtures")}}
        metadata = root / "metadata"
        metadata.write_text(json.dumps(meta), encoding="utf-8")

        def exercise(value, name):
            output = root / name
            collect(raw, root / "done", metadata, spec, "performance", root,
                    output, trace_metrics=value)
            return output

        output = exercise(metrics, "trace-out")
        records = [json.loads(line) for line in output.read_text().splitlines()]
        assert records[0]["kind"] == "run"
        assert records[0]["trace_metrics"]["first_present"]["duration_ns"] == 37_412_333
        samples = records[1:]
        assert len(samples) == 8, len(samples)
        first_present = [sample for sample in samples
                         if sample["stage"] == "first_present"]
        assert len(first_present) == 1
        first_present = first_present[0]
        assert first_present["timing_origin"] == "trace-display-chain"
        assert first_present["timing_clock"] == "trace-relative"
        assert first_present["duration_ns"] == 37_412_333
        assert first_present["main_thread_stall_ns"] == 4_456_000
        assert first_present["main_thread_stall_origin"] == STALL_METHOD
        assert first_present["trace"]["pid"] == 1
        assert first_present["peak_rss_bytes"] == 4096
        assert all(sample["main_thread_stall_ns"] == 4_456_000
                   and sample["main_thread_stall_origin"] == STALL_METHOD
                   and sample["peak_rss_bytes"] == 4096
                   and sample["native_pid"] == 1
                   and sample["trace"]["pid"] == 1 for sample in samples)

        def frame(value):
            return value["first_present"]["content_frame"]

        def consistent_commit(value):
            fp = value["first_present"]
            commit_epoch = fp["layout_end_epoch_ns"] - 2_000_000
            request_epoch = commit_epoch + 1_000_000
            frame(value)["commit_epoch_ns"] = commit_epoch
            frame(value)["commit_trace_ns"] = commit_epoch - value["trace"]["start_epoch_ns"]
            frame(value)["request_epoch_ns"] = request_epoch
            frame(value)["request_trace_ns"] = request_epoch - value["trace"]["start_epoch_ns"]
            frame(value)["primary"] = True
            frame(value)["primary_commit_trace_ns"] = frame(value)["commit_trace_ns"]
            frame(value)["primary_commit_epoch_ns"] = commit_epoch
            frame(value)["cpu_to_display_latency_ns"] = (
                frame(value)["display_start_trace_ns"] - frame(value)["commit_trace_ns"])

        def display_before_effect(value):
            fp = value["first_present"]
            commit_epoch = fp["layout_end_epoch_ns"] - 958_000
            request_epoch = commit_epoch + 100_000
            display_epoch = commit_epoch + 400_000
            frame(value)["commit_epoch_ns"] = commit_epoch
            frame(value)["commit_trace_ns"] = commit_epoch - value["trace"]["start_epoch_ns"]
            frame(value)["request_epoch_ns"] = request_epoch
            frame(value)["request_trace_ns"] = request_epoch - value["trace"]["start_epoch_ns"]
            frame(value)["display_start_epoch_ns"] = display_epoch
            frame(value)["display_start_trace_ns"] = (
                display_epoch - value["trace"]["start_epoch_ns"])
            frame(value)["primary"] = True
            frame(value)["primary_commit_trace_ns"] = frame(value)["commit_trace_ns"]
            frame(value)["primary_commit_epoch_ns"] = commit_epoch
            frame(value)["cpu_to_display_latency_ns"] = (
                frame(value)["display_start_trace_ns"] - frame(value)["commit_trace_ns"])
            fp["epoch_ns"] = display_epoch
            fp["duration_ns"] = display_epoch - fp["started_epoch_ns"]

        def extra_candidate(value):
            stall = value["main_thread_stall"]
            start = stall["interval_start_trace_ns"] + 1
            longest = stall["longest_busy_ns"] + 1
            stall["candidates"].append({
                "interval_identifier": "synthetic-extra",
                "interval_start_trace_ns": start,
                "interval_end_trace_ns": start + longest,
                "interval_duration_ns": longest, "waiting_ns": 0,
                "longest_busy_ns": longest, "total_busy_ns": longest,
                "segments": [[start, start + longest]],
                "thread": stall["thread"], "timestamp_accuracy": "recorded",
                "is_main": True})

        # draw 回调可能晚于上屏：display 早于 effect_end 仍是有效证据。
        early_display = json.loads(json.dumps(metrics))
        display_before_effect(early_display)
        exercise(early_display, "trace-early-display")

        # 首个内容帧的 CA request 可晚于 commit 超过 2 ms，只要早于扫描开始。
        delayed_request = json.loads(json.dumps(metrics))
        delayed_frame = frame(delayed_request)
        delayed_frame.update(
            request_trace_ns=delayed_frame["commit_trace_ns"] + 3_000_000,
            request_epoch_ns=delayed_request["trace"]["start_epoch_ns"]
            + delayed_frame["commit_trace_ns"] + 3_000_000)
        exercise(delayed_request, "trace-delayed-request")

        # 次要帧：latency 属于同一扫描的主帧，不要求等于本帧 commit 差。
        composite = json.loads(json.dumps(metrics))
        composite_frame = frame(composite)
        composite_frame.update(
            primary=False,
            primary_commit_trace_ns=composite_frame["commit_trace_ns"] - 1_000_000,
            primary_commit_epoch_ns=composite["trace"]["start_epoch_ns"]
            + composite_frame["commit_trace_ns"] - 1_000_000)
        composite_frame.update(
            cpu_to_display_latency_ns=composite_frame["display_start_trace_ns"]
            - composite_frame["primary_commit_trace_ns"])
        exercise(composite, "trace-composite")

        negatives = [
            ("schema", lambda value: value.update(schema="wrong"), "schema"),
            ("run_id", lambda value: value.update(run_id="other"), "run_id"),
            ("app_sha256", lambda value: value.update(app_sha256="c" * 64), "app_sha256"),
            ("pid", lambda value: value.update(pid=2), "pid"),
            ("case", lambda value: value.update(case="other"), "case"),
            ("fixture", lambda value: value.update(fixture_sha256="d" * 64), "fixture_sha256"),
            ("pseudo present", lambda value: value["first_present"].update(
                timing_origin="measured-boundaries"), "first_present 不是外部 trace"),
            ("pseudo present duration", lambda value: value["first_present"].update(
                duration_ns=value["first_present"]["duration_ns"] + 1),
             "first_present 起止与耗时不一致"),
            ("latency", lambda value: frame(value).update(
                cpu_to_display_latency_ns=frame(value)["cpu_to_display_latency_ns"] + 1),
             "cpu-to-display-latency"),
            ("commit before layout", consistent_commit, "commit 早于 layout"),
            ("request after display", lambda value: frame(value).update(
                request_trace_ns=frame(value)["display_start_trace_ns"] + 1,
                request_epoch_ns=value["trace"]["start_epoch_ns"]
                + frame(value)["display_start_trace_ns"] + 1),
             "present request"),
            ("primary flag mismatch", lambda value: (
                frame(value).update(
                    primary=True,
                    primary_commit_trace_ns=frame(value)["commit_trace_ns"] - 1,
                    primary_commit_epoch_ns=value["trace"]["start_epoch_ns"]
                    + frame(value)["commit_trace_ns"] - 1),
                frame(value).update(
                    cpu_to_display_latency_ns=frame(value)["display_start_trace_ns"]
                    - frame(value)["primary_commit_trace_ns"])),
             "primary"),
            ("display epoch", lambda value: frame(value).update(
                display_start_epoch_ns=frame(value)["display_start_epoch_ns"] + 1),
             "display epoch"),
            ("pseudo stall", lambda value: value["main_thread_stall"].update(
                timing_origin="event_loop_gap_ns"), "main_thread_stall 不是 trace"),
            ("stall duration", lambda value: value["main_thread_stall"].update(
                duration_ns=value["main_thread_stall"]["duration_ns"] + 1),
             "不是最长忙碌段"),
            ("segment sum", lambda value: value["main_thread_stall"]["segments"][0].__setitem__(
                1, value["main_thread_stall"]["segments"][0][1] - 1),
             "忙碌段总长"),
            ("window start", lambda value: value["main_thread_stall"].update(
                window_start_epoch_ns=value["main_thread_stall"]["window_start_epoch_ns"] + 1),
             "窗口起点"),
            ("candidate max", extra_candidate, "不是候选最大值"),
            ("trace uncertainty", lambda value: value["trace"].update(
                start_uncertainty_ns=2), "不确定度"),
            ("trace hash", lambda value: value["trace"]["raw_sha256"].pop("runloop.xml"),
             "原始表哈希"),
            ("trace pid", lambda value: value["trace"].update(pid=2), "trace 目标 pid"),
            ("layout drift", lambda value: value["first_present"].update(
                layout_end_epoch_ns=value["first_present"]["layout_end_epoch_ns"] + 1),
             "layout_end 与原始阶段不一致"),
        ]
        for index, (label, mutate, keyword) in enumerate(negatives):
            broken = json.loads(json.dumps(metrics))
            mutate(broken)
            try:
                exercise(broken, "bad-" + str(index))
            except ValueError as error:
                assert keyword in str(error), label + ": " + str(error)
            else:
                raise AssertionError(label + " accepted")
            assert not (root / ("bad-" + str(index))).exists()


if __name__ == "__main__":
    raise SystemExit(main())
