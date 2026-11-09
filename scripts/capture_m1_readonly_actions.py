#!/usr/bin/env python3
"""启动真实窗口收集只读回调证据；缺失步骤保持未完成，不生成验收通过结果。"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

from evidence_paths import display_path, normalize_text, write_json, write_text
from validation_output import output_directory

ROOT = Path(__file__).resolve().parent.parent
REQUIRED = ("open", "scroll", "hit", "jump", "visual_to_source", "source_to_visual", "close")
CALLBACK_STEPS = {"open", "scroll-key", "scroll-wheel", "hit", "jump", "visual_to_source", "source_to_visual", "close"}
COUNTS = {"revision_before", "revision_after", "undo_depth", "redo_depth", "history_len",
          "undo_depth_before", "redo_depth_before", "history_len_before"}
BOOLEANS = {"has_open_block", "has_draft", "has_composition", "has_open_block_before",
            "has_draft_before", "has_composition_before", "source_bytes_equal", "document_closed", "disk_bytes_equal"}


def validate_state(fields):
    """回调状态失败关闭；溢出、未知步骤及无效数字不能被子序列核对吞掉。"""
    if set(fields) != COUNTS | BOOLEANS | {"steps"}:
        raise ValueError("状态字段缺失或未知")
    for name in COUNTS:
        value = fields[name]
        if not isinstance(value, str) or not value.isascii() or not value.isdecimal():
            raise ValueError("状态计数不是非负整数：" + name)
    if any(fields[name] not in ("true", "false") for name in BOOLEANS):
        raise ValueError("状态布尔值无效")
    steps = fields["steps"].split(",")
    if not steps or any(step not in CALLBACK_STEPS for step in steps):
        raise ValueError("真实回调记录含未知动作或事件溢出")
    return steps


def sha(data):
    return hashlib.sha256(data).hexdigest()


def inspect_case(directory, fixtures):
    fields = {}
    for line in (directory / "state.tsv").read_text().splitlines():
        for item in line.split("\t"):
            key, value = item.split("=", 1)
            if key in fields:
                raise ValueError("重复状态字段")
            fields[key] = value
    steps = validate_state(fields)
    opened = (directory / "opened.bin").read_bytes()
    matches = [name for name, data in fixtures.items() if data == opened]
    if len(matches) != 1:
        raise ValueError("实际打开字节未对应唯一事前夹具")
    source = opened[3:] if opened.startswith(b"\xef\xbb\xbf") else opened
    checks = {
        "source_before_equal": (directory / "source-before.bin").read_bytes() == source,
        "source_after_equal": (directory / "source-after.bin").read_bytes() == source,
        "stored_equal": (directory / "stored.bin").read_bytes() == opened,
        "disk_after_equal": (directory / "disk-after.bin").read_bytes() == opened,
        "revision_equal": fields["revision_before"] == fields["revision_after"],
        "document_closed": fields["document_closed"] == "true",
    }
    for suffix in ("", "_before"):
        for field in ("has_open_block", "has_draft", "has_composition"):
            checks[field + suffix] = fields[field + suffix] == "false"
        for field in ("undo_depth", "redo_depth", "history_len"):
            checks[field + suffix] = fields[field + suffix] == "0"
    # 保留所有真实回调；仅在核对次序时把真实滚动来源归为 scroll，不补不存在动作。
    canonical = ["scroll" if value in ("scroll-key", "scroll-wheel") else value for value in steps]
    cursor = 0
    for step in canonical:
        if cursor < len(REQUIRED) and step == REQUIRED[cursor]:
            cursor += 1
    return {"fixture": matches[0], "directory": display_path(directory), "steps": steps,
            "checks": checks, "required_sequence_observed": cursor == len(REQUIRED),
            "missing_sequence_suffix": REQUIRED[cursor:], "state": fields}


class StateTests(unittest.TestCase):
    def test_valid_state_and_minimal_violations(self):
        # 合成记录只验证拒绝逻辑，不是平台操作或保真证据。
        valid = {**{key: "0" for key in COUNTS}, **{key: "false" for key in BOOLEANS},
                 "steps": "open,scroll-wheel,hit,jump,visual_to_source,source_to_visual,close"}
        self.assertEqual(len(validate_state(valid)), 7)
        for field, value in (("revision_before", "invalid"), ("history_len", "-1"),
                             ("has_draft", "0"), ("steps", valid["steps"] + ",event-limit-exceeded"),
                             ("steps", "open,programmatic-hit,close")):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                validate_state({**valid, field: value})
        with self.assertRaises(ValueError):
            validate_state({key: value for key, value in valid.items() if key != "has_draft"})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, default=ROOT / "target/release/aline")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        output = output_directory("m1-single-window-readonly", args.output_dir)
        if args.self_test:
            result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(StateTests))
            write_json(output / "self-test.json", {"passed": result.wasSuccessful(), "tests": result.testsRun})
            return 0 if result.wasSuccessful() else 1
        from run_m1_readonly import generate_bytes_fixtures
        generate_bytes_fixtures(output)
        fixtures = {path.name: path.read_bytes() for path in (output / "fixtures").glob("*.md")}
        snapshots = output / "snapshots"
        snapshots.mkdir()
        app = args.app.resolve(strict=True)
        meta = {"schema": "aline-m1-readonly-actions/v1", "run_id": output.name,
                "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                "app": display_path(app), "app_sha256": sha(app.read_bytes()),
                "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "fixtures": {name: {"sha256": sha(data), "bytes": len(data)} for name, data in fixtures.items()},
                "scope": "真实平台回调的补充证据；原样本须逐个完整七步，缺步不通过。"}
        write_json(output / "launch.json", meta)
        env = dict(os.environ, ALINE_M1_READONLY_EVIDENCE_DIR=str(snapshots.resolve()))
        for key in list(env):
            if key.startswith("ALINE_M1_PROBE_"):
                del env[key]
        process = subprocess.Popen([str(app)], cwd=ROOT, env=env, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        meta["pid"] = process.pid
        write_json(output / "launch.json", meta)
        print("真实窗口已启动；夹具目录：" + display_path(output / "fixtures"), flush=True)
        stdout, stderr = process.communicate()
        write_text(output / "app.stdout", stdout)
        write_text(output / "app.stderr", stderr)
        meta.update(exit_code=process.returncode,
                    finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
        write_json(output / "launch.json", meta)
        cases = [inspect_case(path, fixtures) for path in sorted(snapshots.iterdir()) if path.is_dir()]
        seen = [case["fixture"] for case in cases]
        failures = ["缺少完整且唯一的逐样本记录"] if sorted(seen) != sorted(fixtures) else []
        if process.returncode:
            failures.append("原生窗口进程非零退出")
        for case in cases:
            if not all(case["checks"].values()):
                failures.append(case["fixture"] + " 字节或核心状态改变")
            if not case["required_sequence_observed"]:
                failures.append(case["fixture"] + " 未观察到完整七步")
        write_json(output / "observations.json", {"launch": meta, "cases": cases,
                   "all_required_observed": not failures, "failures": failures,
                   "boundary": "记录完整性仍须结合真实GUI截图和操作日志独立复核，不替代7.3。"})
        print(json.dumps({"cases": len(cases), "failures": failures}, ensure_ascii=False))
        return 1 if failures else 0
    except (OSError, ValueError, KeyError) as error:
        print("只读取证失败：" + normalize_text(str(error)), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
