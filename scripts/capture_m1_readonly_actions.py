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


REQUIRED_RECEIPT_FIELDS = ("schema", "pid", "exit_code", "raw_sha256",
                           "started_utc", "finished_utc", "driver_exit")


def build_results_payload(*, run_id, commit, app_sha256, spec_sha256,
                          fixtures, cases, receipt, evidence_root=None):
    """组装标准入口可直接消费的字节结果；收据必须绑定真实原始输出。

    ``fixtures`` 是用例名到原始字节的映射，键必须与标准入口登记的用例名一致。
    收据缺字段、pid 非正或退出码非零时直接失败：身份声明本身不构成运行证据。
    ``evidence_root`` 是本轮证据目录的仓库内相对路径；标准入口据此回读
    原始字节与回调记录，缺它就只能看到自述，不能判定真实运行。
    """
    if not isinstance(receipt, dict):
        raise ValueError("启动收据必须是对象")
    for field in REQUIRED_RECEIPT_FIELDS:
        if field not in receipt:
            raise ValueError("启动收据缺少字段 " + field)
    if receipt["schema"] != "aline-m1-launch/v1":
        raise ValueError("启动收据 schema 不匹配")
    if not isinstance(receipt["pid"], int) or isinstance(receipt["pid"], bool) or receipt["pid"] <= 0:
        raise ValueError("启动收据缺少实际 pid")
    if receipt["exit_code"] != 0:
        raise ValueError("原生进程未成功退出")
    # 驱动退出码必须显式为零：驱动失败时收据同样要留下失败状态，
    # 标准入口据此判定，不能靠「没写」当作成功。
    if not isinstance(receipt["driver_exit"], int) or isinstance(receipt["driver_exit"], bool):
        raise ValueError("启动收据缺少驱动退出码")
    if receipt["driver_exit"] != 0:
        raise ValueError("真实输入驱动未成功完成全部样本")
    manifest = {}
    for name, data in fixtures.items():
        manifest[name] = {"sha256": sha(data), "bytes": len(data), "suite": "bytes"}
    payload = {
        "schema": "aline-m1-readonly-bytes/v1",
        "run_id": run_id,
        "commit": commit,
        "build_profile": "release",
        "app_sha256": app_sha256,
        "spec_sha256": spec_sha256,
        "fixtures": manifest,
        "cases": cases,
        "launch_receipt": dict(receipt),
    }
    if evidence_root is not None:
        payload["evidence_root"] = evidence_root
    for field in ("run_id", "app_sha256", "spec_sha256", "fixtures"):
        payload["launch_receipt"][field] = payload[field]
    return payload


def canonical_steps(steps):
    """把真实回调归一为标准七步；滚动来源并入 scroll，缺失步骤不补。

    折叠只合并**连续**的滚动事件：一次真实滚轮滚动会产生多条
    ``scroll-wheel``，它们属于同一步。其余步骤的重复一律保留——关闭后
    又发生一次滚动、或同一步被记录两次，都是次序异常，不能被折叠掩盖。

    它不重排、不新增，因此不能凭空造出观察到的步骤。
    """
    folded = []
    for step in steps:
        value = "scroll" if step in ("scroll-key", "scroll-wheel") else step
        if value == "scroll" and folded and folded[-1] == "scroll":
            continue
        folded.append(value)
    return folded


def required_steps_observed(steps):
    """按标准七步的次序核对真实回调；缺步、乱序与多余动作都判为未观察到。

    判定与标准入口一致：折叠后的序列必须恰好等于 ``REQUIRED``。子序列
    判定会放行「先命中后滚动」，多余动作（关闭之后又滚动）同样不放行。

    返回值第二项是首个不符位置之后仍缺的标准步骤；序列完全吻合时为空。
    """
    folded = canonical_steps(steps)
    if folded == list(REQUIRED):
        return True, []
    for index, step in enumerate(REQUIRED):
        if index >= len(folded) or folded[index] != step:
            return False, list(REQUIRED[index:])
    return False, []


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
    observed, missing = required_steps_observed(steps)
    return {"fixture": matches[0], "directory": display_path(directory), "steps": steps,
            "checks": checks, "required_sequence_observed": observed,
            "missing_sequence_suffix": missing, "state": fields,
            "hashes": {
                # 哈希一律取自实际落盘的证据文件，不采信任何自报值。
                "disk_sha256_after": sha((directory / "disk-after.bin").read_bytes()),
                "memory_sha256_after": sha((directory / "stored.bin").read_bytes()),
                "source_sha256_before": sha((directory / "source-before.bin").read_bytes()),
                "source_sha256_after": sha((directory / "source-after.bin").read_bytes()),
            },
            "counts": {
                "has_draft": fields["has_draft"] == "true",
                "history_len": int(fields["history_len"]),
                "revision_before": int(fields["revision_before"]),
                "revision_after": int(fields["revision_after"]),
            },
            "equal": {
                "source_bytes_equal": checks["source_before_equal"] and checks["source_after_equal"],
                "disk_bytes_equal": checks["disk_after_equal"],
            }}


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

    def test_results_payload_binds_a_real_launch_receipt(self):
        # 合成记录只验证载荷契约，不是平台操作或保真证据。
        fixtures = {"bom": b"\xef\xbb\xbfx"}
        cases = [{
            "name": "bom", "run_id": "run-1", "steps": list(REQUIRED),
            "disk_sha256_after": "a" * 64, "memory_sha256_after": "a" * 64,
            "source_sha256_before": "b" * 64, "source_sha256_after": "b" * 64,
            "source_bytes_equal": True, "disk_bytes_equal": True,
            "has_draft": False, "history_len": 0,
            "revision_before": 0, "revision_after": 0,
        }]
        receipt = {
            "schema": "aline-m1-launch/v1", "pid": 42, "exit_code": 0,
            "raw_sha256": "c" * 64, "raw_output": "driver.stdout", "driver_exit": 0,
            "started_utc": "2026-11-11T00:00:00+00:00",
            "finished_utc": "2026-11-11T00:00:01+00:00",
        }
        payload = build_results_payload(
            run_id="run-1", commit="d" * 40, app_sha256="e" * 64,
            spec_sha256="f" * 64, fixtures=fixtures, cases=cases, receipt=receipt,
            evidence_root="logs/validation/example")
        self.assertEqual(payload["schema"], "aline-m1-readonly-bytes/v1")
        self.assertEqual(payload["build_profile"], "release")
        self.assertEqual(payload["fixtures"]["bom"]["suite"], "bytes")
        self.assertEqual(payload["launch_receipt"]["raw_sha256"], "c" * 64)
        self.assertEqual(payload["evidence_root"], "logs/validation/example")
        for field in ("run_id", "app_sha256", "spec_sha256", "fixtures"):
            self.assertEqual(payload["launch_receipt"][field], payload[field])

    def test_results_payload_rejects_a_receipt_that_binds_nothing(self):
        # 收据必须绑定真实原始输出文件；没有绑定对象的收据不能进入结果。
        for receipt in (None, {}, {"schema": "aline-m1-launch/v1"},
                        {"schema": "aline-m1-launch/v1", "raw_sha256": "c" * 64}):
            with self.subTest(receipt=receipt), self.assertRaises(ValueError):
                build_results_payload(
                    run_id="run-1", commit="d" * 40, app_sha256="e" * 64,
                    spec_sha256="f" * 64, fixtures={}, cases=[], receipt=receipt)

    def test_results_payload_rejects_a_receipt_without_a_real_process(self):
        # pid 必须是非正整数之外的真实进程号，退出码必须为零：
        # 两条守卫各自都要有反例，不能只靠缺字段的通用反例覆盖。
        base = {
            "schema": "aline-m1-launch/v1", "pid": 42, "exit_code": 0,
            "raw_sha256": "c" * 64, "raw_output": "driver.stdout", "driver_exit": 0,
            "started_utc": "2026-11-11T00:00:00+00:00",
            "finished_utc": "2026-11-11T00:00:01+00:00",
        }
        for label, mutate in (
            ("pid 为零", lambda receipt: receipt.update(pid=0)),
            ("pid 为负", lambda receipt: receipt.update(pid=-1)),
            ("pid 不是整数", lambda receipt: receipt.update(pid="42")),
            ("pid 是布尔", lambda receipt: receipt.update(pid=True)),
            ("退出码非零", lambda receipt: receipt.update(exit_code=1)),
            ("退出码缺失", lambda receipt: receipt.pop("exit_code")),
            ("schema 不匹配", lambda receipt: receipt.update(schema="aline-m1-launch/v2")),
        ):
            receipt = dict(base)
            mutate(receipt)
            with self.subTest(label=label), self.assertRaises(ValueError):
                build_results_payload(
                    run_id="run-1", commit="d" * 40, app_sha256="e" * 64,
                    spec_sha256="f" * 64, fixtures={}, cases=[], receipt=receipt)

    def test_results_payload_rejects_a_failed_driver(self):
        # 驱动退出码必须显式为零：驱动失败时收据要留下失败状态，
        # 不能靠「没写」或「写了非零」进入标准入口的通过路径。
        base = {
            "schema": "aline-m1-launch/v1", "pid": 42, "exit_code": 0,
            "raw_sha256": "c" * 64, "raw_output": "driver.stdout", "driver_exit": 0,
            "started_utc": "2026-11-11T00:00:00+00:00",
            "finished_utc": "2026-11-11T00:00:01+00:00",
        }
        for label, mutate in (
            ("驱动退出码非零", lambda receipt: receipt.update(driver_exit=1)),
            ("驱动退出码缺失", lambda receipt: receipt.pop("driver_exit")),
            ("驱动退出码不是整数", lambda receipt: receipt.update(driver_exit="0")),
            ("驱动退出码是布尔", lambda receipt: receipt.update(driver_exit=True)),
        ):
            receipt = dict(base)
            mutate(receipt)
            with self.subTest(label=label), self.assertRaises(ValueError):
                build_results_payload(
                    run_id="run-1", commit="d" * 40, app_sha256="e" * 64,
                    spec_sha256="f" * 64, fixtures={}, cases=[], receipt=receipt)

    def test_canonical_steps_folds_scroll_and_keeps_missing_steps_missing(self):
        # 折叠只并入滚动来源；缺步不能被折叠补出。
        self.assertEqual(
            canonical_steps(["open", "scroll-key", "hit", "jump",
                             "visual_to_source", "source_to_visual", "close"]),
            list(REQUIRED))
        # 一次滚轮滚动产生多条连续滚动事件：属于同一步。
        self.assertEqual(
            canonical_steps(["open", "scroll-wheel", "scroll-wheel", "hit", "jump",
                             "visual_to_source", "source_to_visual", "close"]),
            list(REQUIRED))
        # 缺一步：折叠不能补出缺失的步骤。
        self.assertEqual(
            canonical_steps(["open", "scroll", "hit", "visual_to_source",
                             "source_to_visual", "close"]),
            ["open", "scroll", "hit", "visual_to_source", "source_to_visual", "close"])
        # 关闭之后又滚动：多余动作不能被折叠掉。
        self.assertEqual(
            canonical_steps(["open", "scroll", "hit", "jump", "visual_to_source",
                             "source_to_visual", "close", "scroll"]),
            ["open", "scroll", "hit", "jump", "visual_to_source",
             "source_to_visual", "close", "scroll"])

    def test_required_steps_observed_rejects_missing_and_reordered(self):
        # 完整七步：观察到。
        observed, missing = required_steps_observed(
            ["open", "scroll-key", "hit", "jump",
             "visual_to_source", "source_to_visual", "close"])
        self.assertTrue(observed)
        self.assertEqual(list(missing), [])
        # 缺步：未观察到，且指明从首个不符位置起仍缺的标准步骤。
        observed, missing = required_steps_observed(["open", "scroll", "hit", "close"])
        self.assertFalse(observed)
        self.assertEqual(list(missing),
                         ["jump", "visual_to_source", "source_to_visual", "close"])
        # 乱序（先命中后滚动）：未观察到；子序列判定不能放行。
        observed, missing = required_steps_observed(
            ["open", "hit", "scroll", "jump",
             "visual_to_source", "source_to_visual", "close"])
        self.assertFalse(observed)
        self.assertEqual(list(missing), ["scroll", "hit", "jump",
                                          "visual_to_source", "source_to_visual", "close"])
        # 关闭之后又滚动：未观察到。
        observed, missing = required_steps_observed(
            ["open", "scroll", "hit", "jump", "visual_to_source",
             "source_to_visual", "close", "scroll"])
        self.assertFalse(observed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, default=ROOT / "target/release/aline")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--driver", type=Path,
                        help="驱动脚本：在本入口已启动的窗口上执行真实七步；"
                             "参数为 <app_pid> <夹具目录> <证据目录>")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        output = output_directory("m1-single-window-readonly", args.output_dir)
        if args.self_test:
            result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(StateTests))
            write_json(output / "self-test.json", {"passed": result.wasSuccessful(), "tests": result.testsRun})
            return 0 if result.wasSuccessful() else 1
        from run_m1_readonly import BYTES_CASES, generate_bytes_fixtures
        generate_bytes_fixtures(output)
        fixtures = {path.name: path.read_bytes() for path in (output / "fixtures").glob("*.md")}
        # 标准入口按用例名核对事前夹具；文件名只用于证据目录里的实际文件。
        named_fixtures = {name: case["bytes"] for name, case in BYTES_CASES.items()}
        snapshots = output / "snapshots"
        snapshots.mkdir()
        app = args.app.resolve(strict=True)
        # 事前 spec：夹具字节与其哈希的清单，先于窗口启动固定下来。
        spec_text = "".join(
            f"{name}|{case['file']}|{sha(case['bytes'])}\n"
            for name, case in BYTES_CASES.items())
        spec_path = output / "spec.txt"
        write_text(spec_path, spec_text)
        spec_sha256 = sha(spec_text.encode("utf-8"))
        meta = {"schema": "aline-m1-readonly-actions/v1", "run_id": output.name,
                "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                "app": display_path(app), "app_sha256": sha(app.read_bytes()),
                "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "spec_sha256": spec_sha256,
                "fixtures": {name: {"sha256": sha(data), "bytes": len(data)} for name, data in fixtures.items()},
                "scope": "真实平台回调的补充证据；原样本须逐个完整七步，缺步不通过。"}
        write_json(output / "launch.json", meta)
        env = dict(os.environ, ALINE_M1_READONLY_EVIDENCE_DIR=str(snapshots.resolve()))
        for key in list(env):
            if key.startswith("ALINE_M1_PROBE_"):
                del env[key]
        # 驱动路径先解析：不存在时不得留下已启动的原生窗口。
        driver = args.driver.resolve(strict=True) if args.driver is not None else None
        process = subprocess.Popen([str(app)], cwd=ROOT, env=env, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        meta["pid"] = process.pid
        write_json(output / "launch.json", meta)
        print("真实窗口已启动；夹具目录：" + display_path(output / "fixtures"), flush=True)
        # 真实七步由外部驱动执行：驱动只投递平台输入，证据仍由应用回调落盘。
        # 无论驱动成败，窗口都必须收尾：失败路径同样要拿到真实退出码。
        # 驱动的动作留证单独放一个子目录：驱动每一步都写文件，若与运行目录
        # 同级，打开面板会把这些文件列进无障碍树，树节点数超过工具上限，
        # 「前往文件夹」的路径输入框被截断，真实打开步骤随后必然失败。
        driver_exit = None
        driver_failures = []
        try:
            if driver is not None:
                driver_evidence = output / "driver"
                driver_evidence.mkdir()
                driver_result = subprocess.run(
                    [sys.executable, str(driver), str(process.pid),
                     str((output / "fixtures").resolve()), str(driver_evidence.resolve())],
                    cwd=ROOT, text=True, capture_output=True)
                driver_exit = driver_result.returncode
                write_text(output / "driver.stdout", driver_result.stdout)
                write_text(output / "driver.stderr", driver_result.stderr)
                # 驱动退出码由驱动自己落证（driver/driver-exit.json），本入口只回读核对：
                # 单一写入方，避免两处各写一份而互相覆盖。驱动没写或写的不一致，
                # 说明留证不可信，计入失败。
                driver_receipt = output / "driver" / "driver-exit.json"
                if not driver_receipt.is_file():
                    driver_failures.append("驱动未留下退出码留证")
                else:
                    try:
                        recorded = json.loads(driver_receipt.read_text(encoding="utf-8"))
                    except (OSError, ValueError):
                        recorded = None
                    if not isinstance(recorded, dict) or recorded.get("exit_code") != driver_exit:
                        driver_failures.append("驱动留证的退出码与实际不符")
                print("驱动退出码：" + str(driver_exit), flush=True)
            stdout, stderr = process.communicate(timeout=60)
        finally:
            # 驱动失败或超时时不能让原生窗口留在后台：先终止再收割输出。
            if process.poll() is None:
                process.terminate()
                try:
                    stdout, stderr = process.communicate(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    stdout, stderr = process.communicate()
            else:
                stdout, stderr = process.communicate()
        write_text(output / "app.stdout", stdout)
        write_text(output / "app.stderr", stderr)
        meta.update(exit_code=process.returncode,
                    finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
        write_json(output / "launch.json", meta)
        cases = [inspect_case(path, fixtures) for path in sorted(snapshots.iterdir()) if path.is_dir()]
        seen = [case["fixture"] for case in cases]
        failures = ["缺少完整且唯一的逐样本记录"] if sorted(seen) != sorted(fixtures) else []
        failures.extend(driver_failures)
        if process.returncode:
            failures.append("原生窗口进程非零退出")
        if driver_exit:
            failures.append("真实输入驱动未完成全部样本")
        for case in cases:
            if not all(case["checks"].values()):
                failures.append(case["fixture"] + " 字节或核心状态改变")
            if not case["required_sequence_observed"]:
                failures.append(case["fixture"] + " 未观察到完整七步")
        write_json(output / "observations.json", {"launch": meta, "cases": cases,
                   "all_required_observed": not failures, "failures": failures,
                   "boundary": "记录完整性仍须结合真实GUI截图和操作日志独立复核，不替代7.3。"})
        # 标准入口需要 results.json；收据的 raw_sha256 绑定真实原始输出。
        # 绑定对象必须是**本轮非空**的原始输出：release 原生进程正常退出时
        # stdout 为空，哈希空文件等于绑定了一个常量，谁都能写出来，等于没绑定。
        # 因此改绑驱动留证目录：它每轮都有内容，且内容随本轮动作变化。
        raw_path = output / "driver.stdout"
        if not raw_path.is_file() or not raw_path.read_bytes().strip():
            raise ValueError("原始输出留证缺失或为空，收据无法绑定真实运行")
        receipt = {
            "schema": "aline-m1-launch/v1",
            "pid": meta["pid"],
            "exit_code": process.returncode,
            "raw_sha256": sha(raw_path.read_bytes()),
            "raw_output": "driver.stdout",
            "driver_exit": driver_exit if driver_exit is not None else -1,
            "started_utc": meta["started_utc"],
            "finished_utc": meta["finished_utc"],
        }
        result_cases = []
        for case in cases:
            folded = canonical_steps(case["steps"])
            result_cases.append({
                "name": case["fixture"][:-3],
                "run_id": meta["run_id"],
                "steps": folded,
                "disk_sha256_after": case["hashes"]["disk_sha256_after"],
                "memory_sha256_after": case["hashes"]["memory_sha256_after"],
                "source_sha256_before": case["hashes"]["source_sha256_before"],
                "source_sha256_after": case["hashes"]["source_sha256_after"],
                "source_bytes_equal": case["equal"]["source_bytes_equal"],
                "disk_bytes_equal": case["equal"]["disk_bytes_equal"],
                "has_draft": case["counts"]["has_draft"],
                "history_len": case["counts"]["history_len"],
                "revision_before": case["counts"]["revision_before"],
                "revision_after": case["counts"]["revision_after"],
            })
        payload = build_results_payload(
            run_id=meta["run_id"], commit=meta["commit"], app_sha256=meta["app_sha256"],
            spec_sha256=spec_sha256, fixtures=named_fixtures, cases=result_cases,
            receipt=receipt, evidence_root=display_path(output))
        write_json(output / "results.json", payload)
        print(json.dumps({"cases": len(cases), "failures": failures}, ensure_ascii=False))
        return 1 if failures else 0
    except (OSError, ValueError, KeyError) as error:
        print("只读取证失败：" + normalize_text(str(error)), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
