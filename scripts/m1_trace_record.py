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
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8")


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


def self_test():
    import tempfile
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
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
        # 没有 recording 标记的进程即使 exit 0 也不得算成功。
        marker.unlink()
        silent = [sys.executable, "-c", "print('nothing')"]
        assert record(1, root / "trace", 4, marker, done, stdout, stderr,
                      command=silent) == 1
        assert not marker.exists()
        assert json.loads(done.read_text())["recording_started"] is False
        # 标记已出现但进程失败也不得算成功。
        failing = [sys.executable, "-c",
                   "import sys; print('Starting recording'); sys.exit(2)"]
        assert record(1, root / "trace", 4, marker, done, stdout, stderr,
                      command=failing) == 1
        assert json.loads(done.read_text())["recording_started"] is True
        assert json.loads(done.read_text())["exit_code"] == 2
        # 导出正例/反例只验证命令与落盘，不声称真实 trace。
        xml, err, command = root / "table.xml", root / "table.stderr", root / "cmd.json"
        good = [sys.executable, "-c", "print('<trace-query-result/>')"]
        run_export(good, xml, err, command)
        assert xml.read_text().strip() == "<trace-query-result/>"
        assert json.loads(command.read_text())["exit_code"] == 0
        bad = [sys.executable, "-c", "import sys; sys.exit(1)"]
        try:
            run_export(bad, xml, err, command)
        except ValueError:
            pass
        else:
            raise AssertionError("failed export accepted")
        assert build_record_argv(7, "out.trace", 9) == [
            "xcrun", "xctrace", "record", "--attach", "7",
            "--template", "Metal System Trace", "--time-limit", "9s",
            "--no-prompt", "--output", "out.trace"]
        assert build_export_argv("native.trace") == [
            "xcrun", "xctrace", "export", "--input", "native.trace", "--toc"]
        assert build_export_argv("native.trace", "runloop-events")[-1] == (
            '/trace-toc/run[@number="1"]/data/table[@schema="runloop-events"]')
    print("trace-record self-test: 通过")


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
    if args.self_test:
        self_test()
        return 0
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
