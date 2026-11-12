#!/usr/bin/env python3
"""窗口失败反馈修复的真实窗口复查（2026-11-12 startup 4.3 迁入 scripts/）。

对照 2026-11-12-startup-failure 的负结果：那一轮证明「窗口服务不可达时无窗口、
无提示、进程不退」。修复后，同一条件下产品应输出本地化 `startup.no_display`
并以状态码 1 退出；正常条件下应无该文案、无额外延迟。

判定是**独立事实**，不互相替代：

- ``window_probe_run``：是否发起了窗口枚举调用（应用在枚举前自行退出时不发起）。
- ``window_probe_ok``：枚举是否成功返回 ``onscreen_count`` / ``all_count``。
- ``window_absent``：仅在 ``window_probe_ok`` 为真时有值；否则为 ``null``，
  不能把「未探测」或「探测失败」当成「窗口数为 0」。
- ``no_display_reported``：stderr 出现本地化 ``startup.no_display`` 文案。
- ``exit_code``：进程自身的退出码。

``--self-test`` 只验证文案分类、locale 绑定与证据解释逻辑，不执行真实沙箱、
窗口枚举、Swift 编译或 Release 构建；真实结论仍以既有运行收据为准。输出目录
必须是仓库内 ``logs/validation/`` 下的路径；证据只追加。显式 ``--output-dir``
由调用方负责唯一性（门禁按轮次目录运行）；同一目录重跑会覆盖该目录内的本轮文件。
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

import evidence_paths  # noqa: E402
from validation_output import output_directory  # noqa: E402

APP = ROOT / "target/release/aline"
PROBE_SWIFT = ROOT / "scripts/window_list.swift"

PROFILES = {
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

# 本轮新增的本地化文案（zh-CN.toml / en.toml 的 [startup] no_display）。
NO_DISPLAY_PREFIXES = (
    "未检测到可用显示器，未显示窗口",
    "No display was available; no window was shown",
)
# 旧的「窗口打开失败」文案，用于确认两条失败路径没有被混为一谈。
WINDOW_FAILED_PREFIXES = (
    "无法打开应用窗口",
    "The application window could not be opened",
)

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

# locale 文件是文案前缀的独立来源：自测读取实际 catalog 值，而不是用常量自证。
LOCALES = (
    (
        ROOT / "crates/aline-app-core/locales/zh-CN.toml",
        NO_DISPLAY_PREFIXES[0],
        WINDOW_FAILED_PREFIXES[0],
    ),
    (
        ROOT / "crates/aline-app-core/locales/en.toml",
        NO_DISPLAY_PREFIXES[1],
        WINDOW_FAILED_PREFIXES[1],
    ),
)


def startup_message(path, key):
    """读取 locale TOML ``[startup]`` 段的简单字符串值；缺失时返回 ``None``。"""
    section = ""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped[1:-1].strip()
            continue
        if section != "startup":
            continue
        name, separator, value = stripped.partition("=")
        if separator and name.strip() == key:
            value = value.strip()
            if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
                return value[1:-1]
    return None


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build_probe(workdir):
    binary = workdir / "window_list"
    result = subprocess.run(
        ["swiftc", "-O", str(PROBE_SWIFT), "-o", str(binary)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise ValueError(f"探针编译失败: {result.stderr}")
    return binary


def count_windows(probe):
    result = subprocess.run([str(probe), "aline"], capture_output=True, text=True)
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def classify(stderr_text):
    no_display = [p for p in NO_DISPLAY_PREFIXES if p in stderr_text]
    window_failed = [p for p in WINDOW_FAILED_PREFIXES if p in stderr_text]
    return no_display, window_failed


def window_counts(windows):
    """返回 ``(onscreen_count, all_count)``；缺字段或类型不符时返回 ``(None, None)``。"""
    if not isinstance(windows, dict):
        return None, None
    if "onscreen_count" not in windows or "all_count" not in windows:
        return None, None
    onscreen = windows["onscreen_count"]
    all_count = windows["all_count"]
    if type(onscreen) is not int or type(all_count) is not int:
        return None, None
    return onscreen, all_count


def probe_fields(exited_early, windows):
    """把退出/枚举结果解释成证据字段；未探测或探测失败时窗口数为 null。"""
    probe_run = exited_early is None
    onscreen, all_count = window_counts(windows) if probe_run else (None, None)
    probe_ok = probe_run and onscreen is not None
    return {
        "window_probe_run": probe_run,
        "window_probe_ok": probe_ok,
        "window_probe": windows,
        "onscreen_count": onscreen,
        "all_count": all_count,
        "window_absent": None if not probe_ok else all_count == 0,
    }


def run_case(name, profile_text, probe, workdir, settle_seconds=8.0):
    profile_path = workdir / f"{name}.sb"
    profile_path.write_text(profile_text, encoding="utf-8")
    stdout_path = workdir / f"{name}.stdout"
    stderr_path = workdir / f"{name}.stderr"

    started = time.monotonic()
    with open(stdout_path, "wb") as out, open(stderr_path, "wb") as err:
        process = subprocess.Popen(
            ["sandbox-exec", "-f", str(profile_path), str(APP)],
            stdout=out,
            stderr=err,
        )
        exited_early = None
        try:
            deadline = started + settle_seconds
            while time.monotonic() < deadline:
                code = process.poll()
                if code is not None:
                    exited_early = code
                    break
                time.sleep(0.25)
            elapsed_when_exited = time.monotonic() - started
            windows = count_windows(probe) if exited_early is None else None
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    exit_code = process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    exit_code = process.wait(timeout=10)
                self_exited = False
            else:
                exit_code = process.poll()
                self_exited = True

    # 原始 stdout/stderr 也是证据：写盘前按统一规则替换本机绝对路径。
    stderr_text = evidence_paths.normalize_text(
        stderr_path.read_text(encoding="utf-8", errors="replace"))
    stdout_text = evidence_paths.normalize_text(
        stdout_path.read_text(encoding="utf-8", errors="replace"))
    stderr_path.write_text(stderr_text, encoding="utf-8")
    stdout_path.write_text(stdout_text, encoding="utf-8")
    no_display, window_failed = classify(stderr_text)
    fields = probe_fields(exited_early, windows)

    return {
        "case": name,
        "profile": profile_text.splitlines(),
        "self_exited": self_exited,
        "exit_code": exit_code,
        "elapsed_seconds_before_exit": round(elapsed_when_exited, 2),
        # 进程在探测前自行退出时，窗口枚举根本没有执行；此时 ``window_probe``
        # 为 null，窗口数不能填 0——0 是缺省值，不是测量值。
        **fields,
        "no_display_reported": bool(no_display),
        "no_display_prefix_hits": no_display,
        "window_failed_reported": bool(window_failed),
        "stderr_sha256": sha256(stderr_path),
        "stdout_sha256": sha256(stdout_path),
        "stderr_excerpt": stderr_text.splitlines()[:12],
        "stdout_excerpt": stdout_text.splitlines()[:12],
        "stdout_path": evidence_paths.display_path(stdout_path),
        "stderr_path": evidence_paths.display_path(stderr_path),
    }


def run_baseline(probe, workdir):
    stdout_path = workdir / "baseline.stdout"
    stderr_path = workdir / "baseline.stderr"
    started = time.monotonic()
    first_onscreen = None
    with open(stdout_path, "wb") as out, open(stderr_path, "wb") as err:
        process = subprocess.Popen([str(APP)], stdout=out, stderr=err)
        try:
            deadline = started + 8.0
            while time.monotonic() < deadline and process.poll() is None:
                # 轮询窗口出现时刻：用来量化「正常路径是否付额外延迟」，
                # 而不是只从代码路径推断。
                if first_onscreen is None:
                    snapshot = count_windows(probe)
                    onscreen, _ = window_counts(snapshot)
                    if onscreen is not None and onscreen >= 1:
                        first_onscreen = time.monotonic() - started
                time.sleep(0.25)
            windows = count_windows(probe)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    exit_code = process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    exit_code = process.wait(timeout=10)
                self_exited = False
            else:
                exit_code = process.poll()
                self_exited = True
    stderr_text = evidence_paths.normalize_text(
        stderr_path.read_text(encoding="utf-8", errors="replace"))
    stderr_path.write_text(stderr_text, encoding="utf-8")
    no_display, window_failed = classify(stderr_text)
    return {
        "case": "baseline",
        "profile": None,
        "self_exited": self_exited,
        "exit_code": exit_code,
        # baseline 一定会发起窗口枚举；失败与否则由 window_probe_ok 表示。
        **probe_fields(None, windows),
        "seconds_to_first_onscreen_window": (
            None if first_onscreen is None else round(first_onscreen, 2)
        ),
        "no_display_reported": bool(no_display),
        "window_failed_reported": bool(window_failed),
        "stderr_empty": stderr_text == "",
        "stderr_sha256": sha256(stderr_path),
        "stderr_excerpt": stderr_text.splitlines()[:12],
        "stdout_path": evidence_paths.display_path(stdout_path),
        "stderr_path": evidence_paths.display_path(stderr_path),
    }


EXPECTED_CASE_NAMES = (
    "classify-zh-no-display",
    "classify-zh-no-window-failed",
    "classify-en-no-display",
    "classify-en-no-window-failed",
    "classify-old-no-display",
    "classify-old-window-failed",
    "self-exited-probe-not-run",
    "self-exited-probe-not-ok",
    "self-exited-window-absent-null",
    "self-exited-all-count-null",
    "probe-failed-probe-run",
    "probe-failed-probe-not-ok",
    "probe-failed-window-absent-null",
    "probe-zero-windows-ok",
    "probe-zero-windows-absent",
    "probe-present-windows",
    "probe-present-all-count",
    "probe-missing-all-count-not-ok",
    "probe-missing-all-count-absent-null",
    "probe-non-int-count-not-ok",
    "profile-names-exact",
    "profile-deny-windowserver-exact",
    "profile-deny-mach-exact",
    "locale-zh-CN-no-display-prefix",
    "locale-zh-CN-window-failed-prefix",
    "locale-zh-CN-classify-no-display",
    "locale-zh-CN-classify-window-failed",
    "locale-en-no-display-prefix",
    "locale-en-window-failed-prefix",
    "locale-en-classify-no-display",
    "locale-en-classify-window-failed",
)


def self_test(output):
    cases = []

    def check(name, actual, expected):
        cases.append({
            "name": name,
            "passed": actual == expected,
            "actual": actual,
            "expected": expected,
        })

    zh_no_display, zh_window_failed = classify(NO_DISPLAY_PREFIXES[0])
    check("classify-zh-no-display", zh_no_display, [NO_DISPLAY_PREFIXES[0]])
    check("classify-zh-no-window-failed", zh_window_failed, [])
    en_no_display, en_window_failed = classify(NO_DISPLAY_PREFIXES[1])
    check("classify-en-no-display", en_no_display, [NO_DISPLAY_PREFIXES[1]])
    check("classify-en-no-window-failed", en_window_failed, [])
    old_no_display, old_window_failed = classify(WINDOW_FAILED_PREFIXES[0])
    check("classify-old-no-display", old_no_display, [])
    check("classify-old-window-failed", old_window_failed, [WINDOW_FAILED_PREFIXES[0]])

    # 未探测不能写成 0 个窗口；探测失败也不能写成 0。
    fields = probe_fields(1, None)
    check("self-exited-probe-not-run", fields["window_probe_run"], False)
    check("self-exited-probe-not-ok", fields["window_probe_ok"], False)
    check("self-exited-window-absent-null", fields["window_absent"], None)
    check("self-exited-all-count-null", fields["all_count"], None)
    fields = probe_fields(None, None)
    check("probe-failed-probe-run", fields["window_probe_run"], True)
    check("probe-failed-probe-not-ok", fields["window_probe_ok"], False)
    check("probe-failed-window-absent-null", fields["window_absent"], None)
    fields = probe_fields(None, {"onscreen_count": 0, "all_count": 0})
    check("probe-zero-windows-ok", fields["window_probe_ok"], True)
    check("probe-zero-windows-absent", fields["window_absent"], True)
    fields = probe_fields(None, {"onscreen_count": 1, "all_count": 2})
    check("probe-present-windows", fields["window_absent"], False)
    check("probe-present-all-count", fields["all_count"], 2)
    # 成功返回但缺字段/类型不符时，同样不能把缺省值当成 0 个窗口。
    fields = probe_fields(None, {"onscreen_count": 0})
    check("probe-missing-all-count-not-ok", fields["window_probe_ok"], False)
    check("probe-missing-all-count-absent-null", fields["window_absent"], None)
    fields = probe_fields(None, {"onscreen_count": "0", "all_count": "0"})
    check("probe-non-int-count-not-ok", fields["window_probe_ok"], False)

    # 剖面正文用独立字面量比对；修改 PROFILES 而不改这里会失败。
    check("profile-names-exact", tuple(PROFILES), ("deny-windowserver", "deny-mach"))
    check("profile-deny-windowserver-exact",
          PROFILES["deny-windowserver"], EXPECTED_DENY_WINDOWSERVER)
    check("profile-deny-mach-exact", PROFILES["deny-mach"], EXPECTED_DENY_MACH)

    # 文案前缀绑定到实际 locale 文件，而不是只用常量自证。
    for locale, no_display_prefix, window_failed_prefix in LOCALES:
        no_display = startup_message(locale, "no_display")
        window_failed = startup_message(locale, "window_failed")
        label = locale.stem
        check(f"locale-{label}-no-display-prefix",
              no_display is not None and no_display.startswith(no_display_prefix), True)
        check(f"locale-{label}-window-failed-prefix",
              window_failed is not None and window_failed.startswith(window_failed_prefix), True)
        check(f"locale-{label}-classify-no-display",
              classify(no_display or ""), ([no_display_prefix], []))
        check(f"locale-{label}-classify-window-failed",
              classify(window_failed or ""), ([], [window_failed_prefix]))

    # 用例清单必须与显式名单一致：删除/新增用例而不更新名单即失败。
    check("case-names-exact", [case["name"] for case in cases], list(EXPECTED_CASE_NAMES))

    passed = bool(cases) and all(case["passed"] for case in cases)
    evidence_paths.write_json(output / "summary.json", {
        "schema": "aline-m1-window-failure-probe-self-test/v1",
        "passed": passed,
        "cases": cases,
        "blind_spots": [
            "自测只验证文案分类、locale 前缀绑定与证据解释逻辑，不执行真实沙箱、窗口枚举、Swift 编译或 Release 构建",
            "locale 绑定只读取 TOML 字面值，不证明运行时 catalog 加载或语言选择",
            "剖面正文用独立字面量比对，不证明 sandbox-exec 实际施加限制",
            "不证明 startup.no_display 在真实失败路径出现，也不证明退出码或延迟",
        ],
    })
    for case in cases:
        print(f"{case['name']}: {'通过' if case['passed'] else '失败'}")
    return 0 if passed else 1


def run(output):
    if not APP.is_file():
        raise ValueError(f"缺少 Release 二进制: {evidence_paths.display_path(APP)}")
    if sys.platform != "darwin":
        raise ValueError("本实验只适用于 macOS")
    if not PROBE_SWIFT.is_file():
        raise ValueError(f"缺少窗口枚举探针源码: {evidence_paths.display_path(PROBE_SWIFT)}")

    workdir = Path(tempfile.mkdtemp(prefix="window-failure-", dir=output))
    probe = build_probe(workdir)

    results = {"baseline": run_baseline(probe, workdir), "cases": {}}
    for name, text in PROFILES.items():
        results["cases"][name] = run_case(name, text, probe, workdir)

    results["app"] = {
        "path": evidence_paths.display_path(APP),
        "sha256": sha256(APP),
    }
    results["probe"] = {
        "source": evidence_paths.display_path(PROBE_SWIFT),
        "sha256": sha256(PROBE_SWIFT),
    }
    results["host"] = {
        "platform": sys.platform,
        "machine": subprocess.run(["uname", "-m"], capture_output=True, text=True).stdout.strip(),
        "macos": subprocess.run(
            ["sw_vers", "-productVersion"], capture_output=True, text=True
        ).stdout.strip(),
    }

    evidence_paths.write_json(output / "results.json", results)
    for key, value in results["cases"].items():
        print(
            f"{key}: exit={value['exit_code']} self_exited={value['self_exited']} "
            f"no_display={value['no_display_reported']} "
            f"window_probe_run={value['window_probe_run']} "
            f"all_windows={value['all_count']}"
        )
    baseline = results["baseline"]
    print(
        f"baseline: exit={baseline['exit_code']} self_exited={baseline['self_exited']} "
        f"no_display={baseline['no_display_reported']} all_windows={baseline['all_count']} "
        f"first_onscreen={baseline['seconds_to_first_onscreen_window']}"
    )
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            output = output_directory("window-failure-probe-self-test", args.output_dir)
            return self_test(output)
        if args.output_dir is None:
            raise ValueError("真实复查必须提供 --output-dir")
        output = output_directory("window-failure-probe", args.output_dir)
        return run(output)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("窗口失败复查失败：" + evidence_paths.normalize_text(str(error)), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
