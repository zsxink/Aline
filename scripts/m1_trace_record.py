#!/usr/bin/env python3
"""Metal System Trace 的 attach 录制与表导出辅助入口。

本脚本只负责让 xctrace 附着到已启动的原生 aline 进程，并在确认
"Starting recording" 后写出 marker；它不启动应用、不读取探针输出，
也不把 trace 结果改写成指标。主采样器以兄弟进程启动本脚本，使 worker
的 RUSAGE_CHILDREN 仍只覆盖唯一原生子进程。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from evidence_paths import write_json as _write_json
from validation_output import output_directory, require_validation_path

MARKER = "Starting recording"
TABLES = {
    "assignment": "metal-command-buffer-frame-assignment",
    "request": "ca-client-present-request",
    "display": "displayed-surfaces-interval",
    "runloop": "runloop-events",
}


def build_record_argv(pid, output, time_limit, template="Metal System Trace"):
    return ["xcrun", "xctrace", "record", "--attach", str(pid),
            "--template", template, "--time-limit", str(int(time_limit)) + "s",
            "--no-prompt", "--output", str(output)]


def build_export_argv(trace, schema=None):
    argv = ["xcrun", "xctrace", "export", "--input", str(trace)]
    if schema is None:
        return argv + ["--toc"]
    return argv + ["--xpath",
                   '/trace-toc/run[@number="1"]/data/table[@schema="%s"]' % schema]


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    """写证据 JSON：先建父目录，再交给 evidence_paths 规范化，不记录本机绝对路径。"""
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    _write_json(path, value)


def _contains(path, text):
    try:
        return text in path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False


def record(pid, output, time_limit, marker, done, stdout, stderr,
           template="Metal System Trace", command=None):
    """录制到 output；只有确认 recording 已开始才返回 0。"""
    argv = command or build_record_argv(pid, output, time_limit, template)
    started = time.time_ns()
    marker_written = False
    exit_code = None
    error = None
    try:
        with open(stdout, "wb") as out, open(stderr, "wb") as err:
            process = subprocess.Popen(argv, stdout=out, stderr=err)
            while process.poll() is None:
                if not marker_written and _contains(stdout, MARKER):
                    write_json(marker, {"schema": "aline-m1-trace-recording/v1",
                                        "pid": pid, "epoch_ns": time.time_ns(),
                                        "marker": MARKER, "argv": argv})
                    marker_written = True
                time.sleep(0.02)
            if not marker_written and _contains(stdout, MARKER):
                write_json(marker, {"schema": "aline-m1-trace-recording/v1",
                                    "pid": pid, "epoch_ns": time.time_ns(),
                                    "marker": MARKER, "argv": argv})
                marker_written = True
            exit_code = process.returncode
    except OSError as exc:
        error = str(exc)
    finished = time.time_ns()
    write_json(done, {
        "schema": "aline-m1-trace-record/v1", "argv": argv, "pid": pid,
        "started_epoch_ns": started, "finished_epoch_ns": finished,
        "exit_code": exit_code, "marker": MARKER,
        "recording_started": marker_written, "error": error,
        "output": str(output),
        "stdout_sha256": sha256(stdout) if Path(stdout).is_file() else None,
        "stderr_sha256": sha256(stderr) if Path(stderr).is_file() else None,
    })
    if error is not None or exit_code != 0 or not marker_written:
        return 1
    return 0


def run_export(argv, xml_path, stderr_path, command_path):
    result = subprocess.run(argv, capture_output=True, check=False)
    Path(xml_path).parent.mkdir(parents=True, exist_ok=True)
    Path(xml_path).write_bytes(result.stdout)
    Path(stderr_path).write_bytes(result.stderr)
    write_json(command_path, {"argv": argv, "exit_code": result.returncode,
                              "stdout_sha256": sha256(xml_path)})
    if result.returncode != 0 or not result.stdout.strip():
        raise ValueError("trace 表导出失败：" + Path(xml_path).name)


def export_trace(trace, out_dir):
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=False)
    paths = {}
    run_export(build_export_argv(trace), out_dir / "toc.xml",
               out_dir / "toc.stderr", out_dir / "toc.command.json")
    paths["toc"] = out_dir / "toc.xml"
    for name, schema in TABLES.items():
        xml_path = out_dir / (name + ".xml")
        run_export(build_export_argv(trace, schema), xml_path,
                   out_dir / (name + ".stderr"),
                   out_dir / (name + ".command.json"))
        paths[name] = xml_path
    return paths


def self_test(output=None):
    """合成子进程验证录制/导出契约；不启动应用，也不声称取得真实 trace。"""
    import tempfile

    cases = []

    def record_case(name, kind, function):
        """按用例留证：kind 区分必须接受的正例与必须拒绝的反例。"""
        try:
            function()
        except Exception as error:  # 自测收集全部失败，不中断后续用例
            cases.append({"name": name, "kind": kind, "passed": False,
                          "error": type(error).__name__ + ": " + str(error)})
        else:
            cases.append({"name": name, "kind": kind, "passed": True})

    def scratch():
        directory = tempfile.mkdtemp()
        return Path(directory)

    def recording_success():
        root = scratch()
        marker, done = root / "marker", root / "done.json"
        stdout, stderr = root / "stdout", root / "stderr"
        fake = [sys.executable, "-c",
                "import time; print('Starting recording'); time.sleep(0.05)"]
        assert record(1, root / "trace", 4, marker, done, stdout, stderr,
                      command=fake) == 0
        assert marker.is_file()
        payload = json.loads(done.read_text())
        assert payload["recording_started"] is True and payload["exit_code"] == 0
        assert payload["stdout_sha256"] == sha256(stdout)

    def recording_without_marker():
        root = scratch()
        marker, done = root / "marker", root / "done.json"
        stdout, stderr = root / "stdout", root / "stderr"
        # 没有 recording 标记的进程即使 exit 0 也不得算成功。
        silent = [sys.executable, "-c", "print('nothing')"]
        assert record(1, root / "trace", 4, marker, done, stdout, stderr,
                      command=silent) == 1
        assert not marker.exists()
        assert json.loads(done.read_text())["recording_started"] is False

    def recording_failing_after_marker():
        root = scratch()
        marker, done = root / "marker", root / "done.json"
        stdout, stderr = root / "stdout", root / "stderr"
        # 标记已出现但进程失败也不得算成功。
        failing = [sys.executable, "-c",
                   "import sys; print('Starting recording'); sys.exit(2)"]
        assert record(1, root / "trace", 4, marker, done, stdout, stderr,
                      command=failing) == 1
        assert json.loads(done.read_text())["recording_started"] is True
        assert json.loads(done.read_text())["exit_code"] == 2

    def export_success():
        root = scratch()
        xml, err, command = root / "table.xml", root / "table.stderr", root / "cmd.json"
        good = [sys.executable, "-c", "print('<trace-query-result/>')"]
        run_export(good, xml, err, command)
        assert xml.read_text().strip() == "<trace-query-result/>"
        assert json.loads(command.read_text())["exit_code"] == 0

    def export_failure():
        root = scratch()
        xml, err, command = root / "table.xml", root / "table.stderr", root / "cmd.json"
        bad = [sys.executable, "-c", "import sys; sys.exit(1)"]
        try:
            run_export(bad, xml, err, command)
        except ValueError:
            return
        raise AssertionError("failed export accepted")

    def command_contract():
        assert build_record_argv(7, "out.trace", 9) == [
            "xcrun", "xctrace", "record", "--attach", "7",
            "--template", "Metal System Trace", "--time-limit", "9s",
            "--no-prompt", "--output", "out.trace"]
        assert build_export_argv("native.trace") == [
            "xcrun", "xctrace", "export", "--input", "native.trace", "--toc"]
        assert build_export_argv("native.trace", "runloop-events")[-1] == (
            '/trace-toc/run[@number="1"]/data/table[@schema="runloop-events"]')

    record_case("录制确认 Starting recording 后写 marker 与收据", "accept",
                recording_success)
    record_case("无 recording 标记的 exit 0 不算成功", "reject",
                recording_without_marker)
    record_case("标记已出现但进程失败不算成功", "reject",
                recording_failing_after_marker)
    record_case("导出成功写入 XML 与命令记录", "accept", export_success)
    record_case("导出命令非零退出必须拒绝", "reject", export_failure)
    record_case("录制/导出命令构造", "accept", command_contract)

    passed = all(case["passed"] for case in cases)
    if output is not None:
        write_json(Path(output) / "summary.json", {
            "passed": passed,
            "cases": cases,
            "blind_spots": [
                "合成子进程只验证 marker/收据/导出的失败关闭；不是 xctrace 真实录制",
                "Metal System Trace 模板、权限与 attach 时序只在真实矩阵期间验证",
                "本自测不启动应用、不读探针输出，也不把导出结果改写成指标",
                "真实 trace 表 schema 漂移须由采集器与推导器的真实样本复核发现",
            ],
            "synthetic_contract_test_only": True,
        })
    failed = [case for case in cases if not case["passed"]]
    print("trace-record self-test: %d/%d 通过" % (len(cases) - len(failed), len(cases)))
    for case in failed:
        print("  - " + case["name"] + ": " + case.get("error", ""))
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--export", action="store_true")
    parser.add_argument("--pid", type=int)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--time-limit", type=float, default=30)
    parser.add_argument("--marker", type=Path)
    parser.add_argument("--done", type=Path)
    parser.add_argument("--stdout", type=Path)
    parser.add_argument("--stderr", type=Path)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    # 落点校验先于模式分发：越界必须在建目录或读参数前 exit 1。
    # 导出目录由 export_trace 以 create_new 语义创建，故只校验不创建。
    evidence = None
    try:
        if args.self_test:
            evidence = output_directory("m1-trace-record", args.output_dir)
        elif args.output_dir is not None:
            require_validation_path(args.output_dir)
        if args.output is not None:
            require_validation_path(args.output)
    except ValueError as error:
        print("trace-record: " + str(error), file=sys.stderr)
        return 1
    if args.self_test:
        return self_test(evidence)
    try:
        if args.record:
            if None in (args.pid, args.output, args.marker, args.done,
                        args.stdout, args.stderr):
                parser.error("录制需要 --pid --output --marker --done --stdout --stderr")
            return record(args.pid, args.output, args.time_limit, args.marker,
                          args.done, args.stdout, args.stderr)
        if args.export:
            if args.input is None or args.output_dir is None:
                parser.error("导出需要 --input --output-dir")
            export_trace(args.input, args.output_dir)
            print("trace-record: 已导出 " + args.output_dir.name)
            return 0
        parser.error("需要 --record 或 --export")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("trace-record: 未通过：" + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
