#!/usr/bin/env python3
"""启动期判别信号的原始证据探针（2026-11-12 startup 4.3 迁入 scripts/）。

在探针进程内真的建一个 `NSWindow`，然后同时读取四个候选信号，在三种条件下
各跑一次：

- ``baseline``：无沙箱；
- ``deny-windowserver``：只禁 ``com.apple.windowserver[.active]``；
- ``deny-mach``：禁掉全部 ``mach-lookup``。

四个信号与含义：

- ``cg_active_display_count``：``CGGetActiveDisplayList`` 计数，即 GPUI
  ``Platform::displays()`` 的 macOS 依据；
- ``nsscreen_count``：``NSScreen.screens`` 数量；
- ``window_number``：``NSWindow.windowNumber``，0 表示窗口从未注册进系统；
- ``occlusion_visible``：``NSWindow.occlusionState.contains(.visible)``，
  即 GPUI ``Window::is_visible()`` 的依据。

``--self-test`` 只验证信号解析、剖面配置与用例清单，不编译或运行 Swift 探针，
也不代表真实平台信号。输出目录必须是仓库内 ``logs/validation/`` 下的路径；
证据只追加。显式 ``--output-dir`` 由调用方负责唯一性（门禁按轮次目录运行）；
同一目录重跑会覆盖该目录内的本轮文件。
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

import evidence_paths  # noqa: E402
from validation_output import output_directory  # noqa: E402

SOURCE = ROOT / "scripts/signal_probe.swift"

PROFILE_ORDER = ("baseline", "deny-windowserver", "deny-mach")
PROFILES = {
    "baseline": None,
    "deny-windowserver": '''(version 1)
(allow default)
(deny mach-lookup (global-name "com.apple.windowserver.active"))
(deny mach-lookup (global-name "com.apple.windowserver"))
''',
    "deny-mach": '''(version 1)
(allow default)
(deny mach-lookup)
''',
}

# 自测用独立字面量绑定剖面正文；修改 PROFILES 必须同步这里，否则自测失败。
EXPECTED_DENY_WINDOWSERVER = '''(version 1)
(allow default)
(deny mach-lookup (global-name "com.apple.windowserver.active"))
(deny mach-lookup (global-name "com.apple.windowserver"))
'''
EXPECTED_DENY_MACH = '''(version 1)
(allow default)
(deny mach-lookup)
'''

EXPECTED_CASE_NAMES = (
    "parse-valid-last-line",
    "parse-invalid-keeps-raw",
    "parse-empty-keeps-raw",
    "parse-number-keeps-raw",
    "parse-list-keeps-raw",
    "parse-null-keeps-raw",
    "profile-order",
    "profile-baseline-no-sandbox",
    "profile-deny-windowserver-exact",
    "profile-deny-mach-exact",
)


def parse_signals(stdout_text):
    """读取探针 stdout 最后一行的 JSON 对象；解析失败保留原文，不伪造信号。"""
    line = stdout_text.strip().splitlines()[-1] if stdout_text.strip() else ""
    try:
        value = json.loads(line)
    except json.JSONDecodeError:
        return {"signals": None, "stdout_raw": stdout_text}
    if not isinstance(value, dict):
        return {"signals": None, "stdout_raw": stdout_text}
    return {"signals": value}


def profile_sha256(profile):
    text = PROFILES[profile]
    return None if text is None else hashlib.sha256(text.encode("utf-8")).hexdigest()


def run(profile, binary, out, workdir):
    command = [str(binary)]
    if profile != "baseline":
        profile_path = workdir / f"{profile}.sb"
        profile_path.write_text(PROFILES[profile], encoding="utf-8")
        command = ["sandbox-exec", "-f", str(profile_path), str(binary)]
    result = subprocess.run(command, capture_output=True, text=True, timeout=120)
    stderr_path = out / f"{profile}.stderr"
    stderr_text = evidence_paths.normalize_text(result.stderr)
    stderr_path.write_text(stderr_text, encoding="utf-8")
    record = {
        "profile": profile,
        "profile_sha256": profile_sha256(profile),
        "exit_code": result.returncode,
        "stderr_excerpt": stderr_text.splitlines()[:6],
        "stderr_path": evidence_paths.display_path(stderr_path),
        **parse_signals(result.stdout),
    }
    return evidence_paths.normalize_record(record)


def run_all(out):
    if sys.platform != "darwin":
        raise ValueError("本实验只适用于 macOS")
    if not SOURCE.is_file():
        raise ValueError(f"缺少信号探针源码: {evidence_paths.display_path(SOURCE)}")

    workdir = Path(tempfile.mkdtemp(prefix="signal-probe-", dir=out))
    binary = workdir / "signal_probe"
    build = subprocess.run(
        ["swiftc", "-O", str(SOURCE), "-o", str(binary)],
        capture_output=True,
        text=True,
    )
    if build.returncode != 0:
        raise ValueError("探针编译失败: " + build.stderr)

    records = [run(profile, binary, out, workdir) for profile in PROFILE_ORDER]
    report = {
        "schema": "aline-m1-startup-signal-probe/v1",
        "source": evidence_paths.display_path(SOURCE),
        "profiles": records,
        "host": {
            "platform": sys.platform,
            "machine": subprocess.run(
                ["uname", "-m"], capture_output=True, text=True
            ).stdout.strip(),
            "macos": subprocess.run(
                ["sw_vers", "-productVersion"], capture_output=True, text=True
            ).stdout.strip(),
        },
    }
    evidence_paths.write_json(out / "results.json", report)
    for record in records:
        print(f"{record['profile']}: {record['signals']}")
    return 0


def self_test(output):
    cases = []

    def check(name, actual, expected):
        cases.append({
            "name": name,
            "passed": actual == expected,
            "actual": actual,
            "expected": expected,
        })

    check("parse-valid-last-line", parse_signals('noise\n{"cg_active_display_count": 1}\n'),
          {"signals": {"cg_active_display_count": 1}})
    check("parse-invalid-keeps-raw", parse_signals("not json"),
          {"signals": None, "stdout_raw": "not json"})
    check("parse-empty-keeps-raw", parse_signals(""),
          {"signals": None, "stdout_raw": ""})
    # 非对象 JSON 不是有效信号，保留原文而不是写成 signals。
    check("parse-number-keeps-raw", parse_signals("123"),
          {"signals": None, "stdout_raw": "123"})
    check("parse-list-keeps-raw", parse_signals("[]"),
          {"signals": None, "stdout_raw": "[]"})
    check("parse-null-keeps-raw", parse_signals("null"),
          {"signals": None, "stdout_raw": "null"})
    check("profile-order", tuple(PROFILES), PROFILE_ORDER)
    check("profile-baseline-no-sandbox", PROFILES["baseline"], None)
    # 剖面正文用独立字面量比对；修改 PROFILES 而不改这里会失败。
    check("profile-deny-windowserver-exact",
          PROFILES["deny-windowserver"], EXPECTED_DENY_WINDOWSERVER)
    check("profile-deny-mach-exact", PROFILES["deny-mach"], EXPECTED_DENY_MACH)

    # 用例清单必须与显式名单一致：删除/新增用例而不更新名单即失败。
    check("case-names-exact", [case["name"] for case in cases], list(EXPECTED_CASE_NAMES))

    passed = bool(cases) and all(case["passed"] for case in cases)
    evidence_paths.write_json(output / "summary.json", {
        "schema": "aline-m1-startup-signal-probe-self-test/v1",
        "passed": passed,
        "cases": cases,
        "blind_spots": [
            "自测只验证信号解析、剖面配置与用例清单，不编译或运行 Swift 探针",
            "剖面正文用独立字面量比对，不证明 sandbox-exec 实际施加限制",
            "不证明真实平台信号、窗口注册或显示器计数",
        ],
    })
    for case in cases:
        print(f"{case['name']}: {'通过' if case['passed'] else '失败'}")
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            output = output_directory("signal-probe-self-test", args.output_dir)
            return self_test(output)
        if args.output_dir is None:
            raise ValueError("真实探针必须提供 --output-dir")
        output = output_directory("signal-probe", args.output_dir)
        return run_all(output)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("信号探针失败：" + evidence_paths.normalize_text(str(error)), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
