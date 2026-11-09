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


def collect(raw, done, metadata, spec, suite, memory_dir, output):
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
    cases = {}
    stages = []
    for row in rows:
        kind = row.get("kind")
        require(kind in ("run", "stage", "bytes", "diagnostic", "method", "done"),
                "原始输出含未登记的记录 kind=" + str(kind))
        if kind in ("stage", "bytes", "diagnostic"):
            require(row.get("run_id") == meta["run_id"], "原始记录未绑定本轮 run_id")
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
                "commit": meta["commit"], "build_profile": "release", "started_utc": iso_epoch_ns(started_ns),
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
            if "peak_rss_bytes" not in stage and rss.get("method") == "getrusage-single-native-child-high-water":
                require(isinstance(rss.get("peak_rss_bytes"), int) and not isinstance(rss["peak_rss_bytes"], bool)
                        and rss["peak_rss_bytes"] > 0, "进程峰值 RSS 无效")
                require(rss.get("pid") == meta["launch_receipt"]["pid"], "RSS 未绑定实际原生 pid")
                stage["peak_rss_bytes"] = rss["peak_rss_bytes"]
                stage["peak_rss_scope"] = "whole-native-process-including-warmup"
            stages.append(stage)
    output.parent.mkdir(parents=True, exist_ok=True)
    if suite == "readonly-bytes":
        payload = {"schema": BYTES_SCHEMA, **{k: meta[k] for k in ("run_id", "commit", "build_profile", "app_sha256", "spec_sha256", "fixtures", "launch_receipt")}, "cases": list(cases.values())}
        write_json(output, payload)
    else:
        run = {"schema": PERF_SCHEMA, "kind": "run", **{k: meta[k] for k in ("run_id", "commit", "app_sha256", "build_profile", "run_started_utc", "environment", "spec_sha256", "fixtures", "launch_receipt")},
               "measurement_methods": meta.get("measurement_methods", {})}
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", type=Path)
    parser.add_argument("--done", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--spec", type=Path)
    parser.add_argument("--suite", choices=("performance", "readonly-bytes"))
    parser.add_argument("--memory-dir", type=Path, default=Path("."))
    parser.add_argument("--output-dir", type=Path)
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
        print("self-test: 通过")
        return 0
    if not all((args.raw, args.done, args.metadata, args.spec, args.suite)):
        parser.error("采集需提供 --raw --done --metadata --spec --suite")
    output = output_dir / ("results.json" if args.suite == "readonly-bytes" else "measurements.jsonl")
    try:
        collect(args.raw, args.done, args.metadata, args.spec, args.suite, args.memory_dir, output)
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


if __name__ == "__main__":
    raise SystemExit(main())
