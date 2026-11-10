#!/usr/bin/env python3
"""离线重推已保存 trace 的 M1 性能矩阵样本。

只处理具备完整原始证据的用例：raw.tsv / done / metadata.json / spec.txt /
memory/ 与已导出的 trace 表。用当前 m1_trace_metrics.derive 与
collect_m1_readonly.collect 重推 trace-metrics.json 与 measurements.jsonl，
不启动应用、不重新录制，也不把缺失或推导失败填成值。

默认只处理未 collected 的失败用例；--include-collected 连已收集用例也用
同一推导器重推，使整个矩阵来自同一证据代（旧推导器产出的记录缺少新字段）。
默认只检查（check-only）；--apply 才写文件；--rebuild-samples 按矩阵顺序
拼接主矩阵与 --source-dir 补采矩阵并重建 samples.jsonl，跨来源运行身份或
样本编号不一致即失败。写入都留在矩阵目录，并生成 rederive-report.json /
samples-sources.json / progress-reconciled.json 记录来源与哈希。
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

import collect_m1_readonly
import m1_trace_metrics
from m1_trace_record import TABLES
from run_m1_readonly import CONDITIONS, MIN_SAMPLES, STAGES, TEMPERATURES

REQUIRED_FILES = ("raw.tsv", "done", "metadata.json", "spec.txt")
TRACE_TABLES = tuple(TABLES)
DEFAULT_TOOLS = {
    "parse_tsv": m1_trace_metrics.parse_tsv,
    "parse_toc": m1_trace_metrics.parse_toc,
    "load_tables": m1_trace_metrics.load_tables,
    "derive": m1_trace_metrics.derive,
    "collect": collect_m1_readonly.collect,
}


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n",
                          encoding="utf-8")


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def artifact_state(directory):
    """返回缺失证据清单；空清单表示可以尝试离线重推。"""
    missing = [name for name in REQUIRED_FILES if not (directory / name).is_file()]
    if not (directory / "memory").is_dir():
        missing.append("memory/")
    trace_dir = directory / "trace"
    for name in ("toc", *TRACE_TABLES):
        xml_path = trace_dir / (name + ".xml")
        command_path = trace_dir / (name + ".command.json")
        if not xml_path.is_file() or xml_path.stat().st_size == 0:
            missing.append("trace/" + name + ".xml")
            continue
        if not command_path.is_file():
            missing.append("trace/" + name + ".command.json")
            continue
        command = read_json(command_path)
        if command.get("exit_code") != 0:
            missing.append("trace/" + name + ".command.json(exit!=0)")
    return missing


def expected_identity(directory):
    meta = read_json(directory / "metadata.json")
    case = directory.name
    fixture = meta["fixtures"][case]
    receipt = meta["launch_receipt"]
    return {
        "run_id": meta["run_id"],
        "app_sha256": meta["app_sha256"],
        "spec_sha256": meta["spec_sha256"],
        "pid": receipt["pid"],
        "case": case,
        "condition": fixture["condition"],
        "temperature": fixture["temperature"],
        "sample_id": fixture["sample_id"],
        "fixture_sha256": fixture["sha256"],
    }


def rederive_case(directory, write, tools=None):
    """对一个用例重推；write=False 时只验证不落盘。"""
    tools = tools or DEFAULT_TOOLS
    missing = artifact_state(directory)
    if missing:
        return {"case": directory.name, "status": "unavailable",
                "missing": missing}
    measurements = directory / "measurements.jsonl"
    previous_sha256 = sha256(measurements) if measurements.is_file() else None
    overwritten = previous_sha256 is not None
    try:
        expected = expected_identity(directory)
        trace_dir = directory / "trace"
        tables = tools["load_tables"](
            {name: trace_dir / (name + ".xml") for name in TRACE_TABLES})
        toc = tools["parse_toc"](trace_dir / "toc.xml")
        raw_rows = tools["parse_tsv"](directory / "raw.tsv")
        metrics = tools["derive"](raw_rows, toc, tables, expected)
    except (ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        return {"case": directory.name, "status": "failed", "error": str(error)}
    result = {"case": directory.name, "status": "recovered",
              "overwritten": overwritten, "previous_sha256": previous_sha256,
              "expected": expected,
              "first_present": metrics.get("first_present", {}).get("duration_ns"),
              "main_thread_stall_ns": metrics.get("main_thread_stall", {}).get("duration_ns")}
    if not write:
        return result
    try:
        metrics_path = directory / "trace-metrics.json"
        tools["collect"](directory / "raw.tsv", directory / "done",
                         directory / "metadata.json", directory / "spec.txt",
                         "performance", directory / "memory", measurements,
                         trace_metrics=metrics)
        rows = [json.loads(line) for line in
                measurements.read_text(encoding="utf-8").splitlines() if line]
        samples = [row for row in rows if row.get("kind") == "sample"]
        stages = sorted(row.get("stage") for row in samples)
        if (sum(row.get("kind") == "run" for row in rows) != 1
                or stages != sorted(STAGES)):
            raise ValueError("重推 measurements 不是唯一 run 加全部阶段")
        write_json(metrics_path, metrics)
        result["measurements_sha256"] = sha256(measurements)
        result["trace_metrics_sha256"] = sha256(metrics_path)
    except (ValueError, KeyError, TypeError, json.JSONDecodeError, OSError) as error:
        result = {"case": directory.name, "status": "failed",
                  "error": str(error)}
    return result


def run_rederive(output, write, tools=None, include_collected=False):
    progress = read_json(output / "progress.json")
    entries = progress.get("runs", [])
    results = []
    collected = {entry["case"] for entry in entries if entry.get("collected")}
    for entry in entries:
        if entry.get("collected") and not include_collected:
            continue
        directory = output / entry["case"]
        if not directory.is_dir():
            results.append({"case": entry["case"], "status": "unavailable",
                            "missing": ["case 目录"]})
            continue
        result = rederive_case(directory, write, tools=tools)
        if result["status"] == "recovered" and entry.get("collected"):
            result["status"] = "rederived"
        results.append(result)
    report = {
        "schema": "aline-m1-performance-rederive/v1",
        "output": str(output),
        "write": bool(write),
        "include_collected": bool(include_collected),
        "cases": results,
        "recovered": sorted(item["case"] for item in results
                            if item["status"] == "recovered"),
        "rederived": sorted(item["case"] for item in results
                            if item["status"] == "rederived"),
        "failed": sorted(item["case"] for item in results
                         if item["status"] == "failed"),
        "unavailable": sorted(item["case"] for item in results
                              if item["status"] == "unavailable"),
        "excluded_collected": sorted(
            item["case"] for item in results
            if item["case"] in collected
            and item["status"] not in ("recovered", "rederived")),
    }
    if write:
        write_json(output / "rederive-report.json", report)
    return report


def _identity_check(merged):
    runs = [row for row in merged if row.get("kind") == "run"]
    if not runs:
        raise ValueError("合并记录缺少运行记录")
    identity = {key: runs[0].get(key) for key in
                ("run_id", "commit", "app_sha256", "build_profile", "environment")}
    for key, value in identity.items():
        if value in (None, "", {}):
            raise ValueError("合并记录缺少运行身份：" + key)
    for row in runs[1:]:
        for key, value in identity.items():
            if row.get(key) != value:
                raise ValueError("跨来源运行身份不一致：" + key)
    fixtures = {}
    for row in runs:
        for fixture in row.get("fixtures", {}).values():
            fixtures.setdefault(fixture["condition"], set()).add(fixture["sha256"])
    for condition, hashes in fixtures.items():
        if len(hashes) != 1:
            raise ValueError(condition + " 跨来源混合了不同 fixture_sha256")
    seen = set()
    for row in merged:
        if row.get("kind") != "sample":
            continue
        key = (row.get("condition"), row.get("temperature"),
               row.get("stage"), row.get("sample_id"))
        if key in seen:
            raise ValueError("合并后样本编号重复：" + "/".join(map(str, key)))
        seen.add(key)


def rebuild_samples(output, report, source_dirs=()):
    """按矩阵顺序拼接已收集、已重推与补采用例；身份不一致即失败。"""
    recovered = (set(report.get("recovered", []))
                 | set(report.get("rederived", [])))
    excluded = set(report.get("excluded_collected", []))
    groups = [(output, recovered, excluded)]
    groups.extend((Path(source), set(), set()) for source in source_dirs)
    merged = []
    sources = []
    progress = None
    for directory, recovered, excluded in groups:
        progress = read_json(directory / "progress.json")
        for entry in progress.get("runs", []):
            case = entry["case"]
            if entry.get("collected"):
                if case in excluded:
                    continue
            elif case not in recovered:
                continue
            measurements = directory / case / "measurements.jsonl"
            if not measurements.is_file():
                raise ValueError("已收集/已重推用例缺少 measurements.jsonl："
                                 + str(directory / case))
            rows = [json.loads(line) for line in
                    measurements.read_text(encoding="utf-8").splitlines() if line]
            if not rows or sum(row.get("kind") == "run" for row in rows) != 1:
                raise ValueError("用例测量缺少唯一运行记录：" + str(directory / case))
            merged.extend(rows)
            sources.append({"dir": str(directory), "case": case,
                            "sha256": sha256(measurements),
                            "recovered": case in recovered})
    if not merged:
        raise ValueError("没有可重建的测量记录")
    _identity_check(merged)
    write_json(output / "samples-sources.json",
               {"schema": "aline-m1-performance-samples-sources/v1",
                "sources": sources})
    current = output / "samples.jsonl"
    if current.is_file() and not (output / "samples.initial.jsonl").is_file():
        (output / "samples.initial.jsonl").write_bytes(current.read_bytes())
    current.write_text(
        "\n".join(json.dumps(row, ensure_ascii=False) for row in merged) + "\n",
        encoding="utf-8")
    reconciled = []
    for entry in read_json(output / "progress.json").get("runs", []):
        item = dict(entry)
        if entry["case"] in set(report.get("recovered", [])):
            item["collected"] = True
            item["rederived"] = True
        reconciled.append(item)
    write_json(output / "progress-reconciled.json",
               {"runs": reconciled,
                "missing": read_json(output / "progress.json").get("missing")})
    return {"cases": len(sources), "rows": len(merged)}


def build_refill_plan(output, report, source_dirs=()):
    """矩阵跑满后，为每组成功样本不足 20 的单元生成新编号补采计划。

    主矩阵必须已跑满 20 次尝试；补采来源目录只用来累计成功数与已用编号。
    """
    progress = read_json(output / "progress.json")
    entries = progress.get("runs", [])
    recovered = (set(report.get("recovered", []))
                 | set(report.get("rederived", [])))
    excluded = set(report.get("excluded_collected", []))
    primary_attempted = {}
    attempted = {}
    succeeded = {}
    seen = set()

    def account(entry, valid):
        key = (entry["condition"], entry["temperature"])
        ident = (key, entry["sample_id"])
        if ident in seen:
            raise ValueError("重复的采样编号：%s/%s/%d" % (
                key[0], key[1], entry["sample_id"]))
        seen.add(ident)
        attempted.setdefault(key, []).append(entry["sample_id"])
        if valid:
            succeeded[key] = succeeded.get(key, 0) + 1

    for entry in entries:
        key = (entry["condition"], entry["temperature"])
        primary_attempted.setdefault(key, []).append(entry["sample_id"])
        account(entry, entry["case"] in recovered
                or (entry.get("collected") and entry["case"] not in excluded))
    for source in source_dirs:
        for entry in read_json(Path(source) / "progress.json").get("runs", []):
            account(entry, bool(entry.get("collected")))
    incomplete = []
    for condition in CONDITIONS:
        for temperature in TEMPERATURES:
            key = (condition, temperature)
            if len(primary_attempted.get(key, [])) < MIN_SAMPLES:
                incomplete.append("/".join(key))
    if incomplete:
        raise ValueError("矩阵尚未完成，拒绝生成补采计划：" + ", ".join(incomplete))
    plan = []
    for condition in CONDITIONS:
        for temperature in TEMPERATURES:
            key = (condition, temperature)
            ids = attempted[key]
            needed = MIN_SAMPLES - succeeded.get(key, 0)
            next_id = max(ids) + 1
            for offset in range(max(0, needed)):
                plan.append({"condition": condition, "temperature": temperature,
                             "sample_id": next_id + offset})
    return plan


def self_test(output):
    checks = []

    def check(name, condition):
        checks.append({"name": name, "passed": bool(condition)})
        if not condition:
            raise AssertionError(name)

    root = output / "rederive-self-test"
    shutil.rmtree(root, ignore_errors=True)
    root.mkdir(parents=True, exist_ok=True)
    good = root / "c0-cold-00"
    (good / "memory").mkdir(parents=True, exist_ok=True)
    (good / "trace").mkdir(exist_ok=True)
    for name in REQUIRED_FILES:
        (good / name).write_text("x", encoding="utf-8")
    for name in ("toc", *TRACE_TABLES):
        (good / "trace" / (name + ".xml")).write_text("<x/>", encoding="utf-8")
        write_json(good / "trace" / (name + ".command.json"), {"exit_code": 0})
    write_json(good / "metadata.json", {
        "run_id": "run-1", "app_sha256": "a" * 64, "spec_sha256": "b" * 64,
        "fixtures": {"c0-cold-00": {"condition": "32KiB", "temperature": "cold",
                                    "sample_id": 0, "sha256": "c" * 64}},
        "launch_receipt": {"pid": 7}})

    def stub_tools(fail_derive=False):
        def derive(raw_rows, toc, tables, expected):
            if fail_derive:
                raise ValueError("合成推导失败")
            return {"first_present": {"duration_ns": 5},
                    "main_thread_stall": {"duration_ns": 2}}

        def collect(raw, done, metadata, spec, suite, memory_dir, output_path,
                    trace_metrics=None):
            rows = [{"kind": "run"}] + [
                {"kind": "sample", "stage": stage} for stage in STAGES]
            Path(output_path).write_text(
                "\n".join(json.dumps(row) for row in rows) + "\n",
                encoding="utf-8")

        return {"parse_tsv": lambda path: [], "parse_toc": lambda path: {},
                "load_tables": lambda paths: {}, "derive": derive,
                "collect": collect}

    check("完整证据检查模式重推成功",
          rederive_case(good, False, stub_tools())["status"] == "recovered")
    check("检查模式不写 measurements", not (good / "measurements.jsonl").exists())
    applied = rederive_case(good, True, stub_tools())
    check("写入模式生成完整 measurements",
          applied["status"] == "recovered"
          and (good / "measurements.jsonl").is_file()
          and applied.get("measurements_sha256"))
    again = rederive_case(good, True, stub_tools())
    check("重复重推幂等且记录覆盖",
          again["status"] == "recovered" and again.get("overwritten") is True)

    broken = root / "c0-cold-01"
    broken.mkdir(exist_ok=True)
    (broken / "raw.tsv").write_text("x", encoding="utf-8")
    state = rederive_case(broken, True, stub_tools())
    check("缺失证据拒绝且不写入",
          state["status"] == "unavailable" and not (broken / "measurements.jsonl").exists())

    failing = root / "c0-cold-02"
    (failing / "memory").mkdir(parents=True, exist_ok=True)
    (failing / "trace").mkdir(exist_ok=True)
    for name in REQUIRED_FILES:
        (failing / name).write_text("x", encoding="utf-8")
    for name in ("toc", *TRACE_TABLES):
        (failing / "trace" / (name + ".xml")).write_text("<x/>", encoding="utf-8")
        write_json(failing / "trace" / (name + ".command.json"), {"exit_code": 0})
    (failing / "metadata.json").write_text(json.dumps({
        "run_id": "run-1", "app_sha256": "a" * 64, "spec_sha256": "b" * 64,
        "fixtures": {"c0-cold-02": {"condition": "32KiB", "temperature": "cold",
                                    "sample_id": 2, "sha256": "c" * 64}},
        "launch_receipt": {"pid": 7}}), encoding="utf-8")
    state = rederive_case(failing, True, stub_tools(fail_derive=True))
    check("推导失败被记录且不落盘",
          state["status"] == "failed" and state.get("error") == "合成推导失败"
          and not (failing / "measurements.jsonl").exists())

    export_bad = root / "c0-cold-03"
    (export_bad / "memory").mkdir(parents=True, exist_ok=True)
    (export_bad / "trace").mkdir(exist_ok=True)
    for name in REQUIRED_FILES:
        (export_bad / name).write_text("x", encoding="utf-8")
    for name in ("toc", *TRACE_TABLES):
        (export_bad / "trace" / (name + ".xml")).write_text("<x/>", encoding="utf-8")
        write_json(export_bad / "trace" / (name + ".command.json"),
                   {"exit_code": 0 if name != "display" else 1})
    check("导出命令失败即不可用",
          rederive_case(export_bad, False, stub_tools())["status"] == "unavailable")

    matrix = root / "matrix"
    matrix.mkdir(exist_ok=True)
    identity = {"run_id": "performance", "commit": "a" * 40,
                "app_sha256": "b" * 64, "build_profile": "release",
                "environment": {"os": "macOS"}}

    def measurement_rows(case, sample_id, app_sha256="b" * 64):
        return (json.dumps({"kind": "run", "case": case,
                            **{**identity, "app_sha256": app_sha256}}) + "\n"
                + json.dumps({"kind": "sample", "stage": "first_present",
                              "case": case, "condition": "32KiB",
                              "temperature": "cold", "sample_id": sample_id}) + "\n")

    for case, collected in (("c0-cold-00", True), ("c0-cold-01", False),
                            ("c0-cold-02", False)):
        (matrix / case).mkdir(exist_ok=True)
        if collected:
            (matrix / case / "measurements.jsonl").write_text(
                measurement_rows(case, 0), encoding="utf-8")
    (matrix / "c0-cold-02" / "measurements.jsonl").write_text(
        measurement_rows("c0-cold-02", 2), encoding="utf-8")
    write_json(matrix / "progress.json", {"runs": [
        {"case": "c0-cold-00", "collected": True},
        {"case": "c0-cold-01", "collected": False},
        {"case": "c0-cold-02", "collected": False}], "missing": []})
    report = {"recovered": ["c0-cold-02"]}
    rebuilt = rebuild_samples(matrix, report)
    rows = [json.loads(line) for line in
            (matrix / "samples.jsonl").read_text(encoding="utf-8").splitlines()]
    check("重建 samples 只合并已收集与已重推",
          rebuilt["cases"] == 2 and len(rows) == 4
          and [row["case"] for row in rows if row["kind"] == "run"]
          == ["c0-cold-00", "c0-cold-02"])
    check("重建记录来源哈希", len(read_json(matrix / "samples-sources.json")["sources"]) == 2)
    source = root / "source-matrix"
    (source / "c0-hot-20").mkdir(parents=True, exist_ok=True)
    (source / "c0-hot-20" / "measurements.jsonl").write_text(
        measurement_rows("c0-hot-20", 20), encoding="utf-8")
    write_json(source / "progress.json", {"runs": [
        {"case": "c0-hot-20", "collected": True}]})
    merged = rebuild_samples(matrix, report, source_dirs=[source])
    check("跨来源补采按顺序合并",
          merged["cases"] == 3
          and [json.loads(line)["case"] for line in
               (matrix / "samples.jsonl").read_text(encoding="utf-8").splitlines()
               if json.loads(line)["kind"] == "run"]
          == ["c0-cold-00", "c0-cold-02", "c0-hot-20"])
    check("重建保留首次 samples 副本",
          (matrix / "samples.initial.jsonl").is_file())
    excluded = rebuild_samples(matrix, {"recovered": ["c0-cold-02"],
                                        "excluded_collected": ["c0-cold-00"]})
    check("重推失败的已收集用例被排除",
          excluded["cases"] == 1
          and [json.loads(line)["case"] for line in
               (matrix / "samples.jsonl").read_text(encoding="utf-8").splitlines()
               if json.loads(line)["kind"] == "run"] == ["c0-cold-02"])
    bad_source = root / "bad-source"
    (bad_source / "c0-hot-21").mkdir(parents=True, exist_ok=True)
    (bad_source / "c0-hot-21" / "measurements.jsonl").write_text(
        measurement_rows("c0-hot-21", 21, app_sha256="c" * 64), encoding="utf-8")
    write_json(bad_source / "progress.json", {"runs": [
        {"case": "c0-hot-21", "collected": True}]})
    try:
        rebuild_samples(matrix, report, source_dirs=[bad_source])
        check("跨来源应用身份不一致被拒绝", False)
    except ValueError:
        check("跨来源应用身份不一致被拒绝", True)
    bad_matrix = root / "bad-matrix"
    bad_matrix.mkdir(exist_ok=True)
    write_json(bad_matrix / "progress.json", {"runs": [
        {"case": "c0-cold-00", "collected": True}]})
    try:
        rebuild_samples(bad_matrix, {"recovered": []})
        check("缺 measurements 的重建被拒绝", False)
    except ValueError:
        check("缺 measurements 的重建被拒绝", True)
    plan_matrix = root / "plan-matrix"
    plan_matrix.mkdir(exist_ok=True)
    runs = []
    for condition in CONDITIONS:
        for temperature in TEMPERATURES:
            for sample_id in range(MIN_SAMPLES):
                runs.append({
                    "case": "c%d-%s-%02d" % (CONDITIONS.index(condition),
                                             temperature, sample_id),
                    "condition": condition, "temperature": temperature,
                    "sample_id": sample_id,
                    "collected": not (condition == CONDITIONS[0]
                                      and temperature == "cold"
                                      and sample_id == 7)})
    write_json(plan_matrix / "progress.json", {"runs": runs})
    plan = build_refill_plan(plan_matrix, {"recovered": []})
    check("补采计划只补不足组且用新编号",
          plan == [{"condition": CONDITIONS[0], "temperature": "cold",
                    "sample_id": MIN_SAMPLES}])
    write_json(plan_matrix / "progress.json", {"runs": runs[:-1]})
    try:
        build_refill_plan(plan_matrix, {"recovered": []})
        check("未跑满矩阵拒绝生成补采计划", False)
    except ValueError:
        check("未跑满矩阵拒绝生成补采计划", True)
    plan_matrix2 = root / "plan-matrix2"
    plan_matrix2.mkdir(exist_ok=True)
    runs2 = []
    for condition in CONDITIONS:
        for temperature in TEMPERATURES:
            for sample_id in range(MIN_SAMPLES):
                runs2.append({
                    "case": "c%d-%s-%02d" % (CONDITIONS.index(condition),
                                             temperature, sample_id),
                    "condition": condition, "temperature": temperature,
                    "sample_id": sample_id,
                    "collected": not (condition == CONDITIONS[0]
                                      and temperature == "cold"
                                      and sample_id in (7, 8))})
    write_json(plan_matrix2 / "progress.json", {"runs": runs2})
    plan2 = build_refill_plan(plan_matrix2, {"recovered": []})
    check("补采计划为不足组分配连续新编号",
          plan2 == [{"condition": CONDITIONS[0], "temperature": "cold",
                     "sample_id": MIN_SAMPLES},
                    {"condition": CONDITIONS[0], "temperature": "cold",
                     "sample_id": MIN_SAMPLES + 1}])
    source_plan = root / "plan-source"
    source_plan.mkdir(exist_ok=True)
    write_json(source_plan / "progress.json", {"runs": [
        {"case": "c0-cold-20", "condition": CONDITIONS[0],
         "temperature": "cold", "sample_id": MIN_SAMPLES,
         "collected": True}]})
    plan3 = build_refill_plan(plan_matrix2, {"recovered": []},
                              source_dirs=[source_plan])
    check("补采来源成功计入计划且编号连续",
          plan3 == [{"condition": CONDITIONS[0], "temperature": "cold",
                     "sample_id": MIN_SAMPLES + 1}])
    write_json(source_plan / "progress.json", {"runs": [
        {"case": "c0-cold-20", "condition": CONDITIONS[0],
         "temperature": "cold", "sample_id": MIN_SAMPLES,
         "collected": False}]})
    plan4 = build_refill_plan(plan_matrix2, {"recovered": []},
                              source_dirs=[source_plan])
    check("补采来源失败不计数但占用编号",
          plan4 == [{"condition": CONDITIONS[0], "temperature": "cold",
                     "sample_id": MIN_SAMPLES + 1},
                    {"condition": CONDITIONS[0], "temperature": "cold",
                     "sample_id": MIN_SAMPLES + 2}])
    write_json(source_plan / "progress.json", {"runs": [
        {"case": "c0-cold-19", "condition": CONDITIONS[0],
         "temperature": "cold", "sample_id": MIN_SAMPLES - 1,
         "collected": True}]})
    try:
        build_refill_plan(plan_matrix2, {"recovered": []},
                          source_dirs=[source_plan])
        check("跨来源重复采样编号被拒绝", False)
    except ValueError:
        check("跨来源重复采样编号被拒绝", True)
    write_json(output / "rederive-self-test.json", {"checks": checks})
    print("rederive self-test: 通过（%d 项）" % len(checks))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True,
                        help="采样器输出目录（含 progress.json）")
    parser.add_argument("--apply", action="store_true",
                        help="写入 trace-metrics.json / measurements.jsonl")
    parser.add_argument("--include-collected", action="store_true",
                        help="连已收集用例也用当前推导器重推（统一证据代）")
    parser.add_argument("--rebuild-samples", action="store_true",
                        help="全部可恢复项处理后按矩阵顺序重建 samples.jsonl")
    parser.add_argument("--source-dir", type=Path, action="append", default=[],
                        help="补采矩阵目录；--rebuild-samples 时按顺序并入，"
                             "--write-refill-plan 时累计成功数与编号")
    parser.add_argument("--write-refill-plan", type=Path,
                        help="矩阵跑满后写出成功样本不足组的补采计划 JSON")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test(args.output_dir)
    output = args.output_dir
    if not (output / "progress.json").is_file():
        print("缺少 progress.json：" + str(output), file=sys.stderr)
        return 2
    report = run_rederive(output, args.apply,
                          include_collected=args.include_collected)
    print("可恢复 %d，统一重推 %d，推导失败 %d，证据不可用 %d" % (
        len(report["recovered"]), len(report["rederived"]),
        len(report["failed"]), len(report["unavailable"])))
    for item in report["cases"]:
        print("  " + item["case"] + ": " + item["status"]
              + ((" — " + item["error"]) if item.get("error") else "")
              + ((" — 缺 " + ",".join(item["missing"])) if item.get("missing") else ""))
    rebuilt = False
    if args.rebuild_samples:
        if report["failed"] and not args.source_dir:
            print("仍有推导失败项且无补采来源，拒绝重建 samples.jsonl",
                  file=sys.stderr)
            return 1
        stats = rebuild_samples(output, report, source_dirs=args.source_dir)
        print("已重建 samples.jsonl：%d 个用例，%d 行" % (stats["cases"], stats["rows"]))
        rebuilt = True
    if args.write_refill_plan:
        try:
            plan = build_refill_plan(output, report,
                                     source_dirs=args.source_dir)
        except ValueError as error:
            print(str(error), file=sys.stderr)
            return 1
        write_json(args.write_refill_plan, plan)
        print("补采计划：%d 项 -> %s" % (len(plan), args.write_refill_plan))
    # 重建成功表示合并完成；各组是否够 20 由 runner 验收，不在此处代判。
    return 0 if (rebuilt or not report["failed"]) else 1


if __name__ == "__main__":
    sys.exit(main())
