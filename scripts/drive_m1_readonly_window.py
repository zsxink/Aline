#!/usr/bin/env python3
"""M1 只读真实窗口七步驱动：打开→滚动→命中→主动跳转→两视图往返→关闭。

只经 macOS 无障碍接口投递真实输入，不触碰应用内部状态；证据由应用自身的
回调落盘，驱动只负责把动作与返回留在证据目录里。

与 `scripts/capture_m1_readonly_actions.py` 的分工：取证入口负责启动并持有
原生窗口、拼装标准入口需要的 `results.json`；本驱动只在该窗口上投递真实
七步。驱动作为独立进程运行，退出码非零即计入失败。

**打开面板不能靠无障碍树查找路径输入框。** macOS 打开面板会列出当前目录的
内容；当证据目录就在面板当前目录旁边时，面板把上百个证据文件列进无障碍
树，树节点数超过工具的遍历上限，路径输入框所在的 sheet 被截断，驱动就
找不到输入框。本驱动因此改用 `type-text` 直接向**当前焦点**投递路径：焦点
由 macOS 在打开 sheet 时就落在路径输入框上，不依赖无障碍树能找到它。

**第一个动作之前必须等窗口登记进无障碍树。** 应用进程刚启动时已经存在，
但无障碍树里还没有它；此刻投递会得到 `app_not_found`。驱动因此在投递
第一个动作前先探测窗口是否可寻址，等待窗口登记完成才动手。少了这一步，
头几个样本会在**打开之前**就失败——失败与样本本身无关，却是真实取证。

用法：drive_m1_readonly_window.py <app_pid> <夹具目录> <证据目录>
"""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import unittest

from evidence_paths import write_json, write_text
from validation_output import require_validation_path

ROOT = Path(__file__).resolve().parent.parent
ORCA = "/Applications/Orca.app/Contents/Resources/bin/orca"

# 与 scripts/run_m1_readonly.py 的 BYTES_CASES 同名同序。
CASES = ["bom", "mixed-eol", "lone-cr", "whitespace-only", "no-trailing-newline"]

# 正文区域在窗口内的纵向范围：标题栏与工具栏之下、状态栏之上。
CONTENT_TOP = 70.0
CONTENT_BOTTOM = 630.0

# 打开面板与「前往文件夹」的组合键。
OPEN_PANEL_KEY = "cmd+o"
GO_TO_FOLDER_KEY = "cmd+shift+g"
CLOSE_DOCUMENT_KEY = "cmd+w"

# 应用进程启动后，要等它真正登记进无障碍树才投递第一个动作。
# 启动瞬间进程已存在，但无障碍树里还没有它，此时投递会得到 app_not_found。
# 等待上限只防挂死；正常情况下窗口在数百毫秒内即可寻址。
STARTUP_TIMEOUT = 30.0
STARTUP_POLL = 0.2


class DriverError(RuntimeError):
    """真实输入未能按预期送达窗口。"""


class WindowDriver:
    """在已启动的原生窗口上投递真实输入，并把每次动作留证。"""

    def __init__(self, pid, evidence):
        self.pid = pid
        self.evidence = evidence
        self.serial = 0

    def call(self, args, note="", allow_missing_window=False):
        """投递一个真实输入动作；原始返回、截图与命令都留在证据目录内。

        ``allow_missing_window`` 用于关闭窗口：点击关闭按钮后窗口立刻消失，
        工具回报 ``window_not_found``，但窗口确实已经关掉（应用随即自行退出）。
        把这种回报当成失败会误判一次成功的收尾。
        """
        self.serial += 1
        argv = [ORCA, "computer"] + args + ["--app", "pid:" + str(self.pid), "--json"]
        started = time.time_ns()
        result = subprocess.run(argv, text=True, capture_output=True)
        write_json(self.evidence / f"step-{self.serial:03d}.command.json",
                   {"argv": argv, "note": note, "exit_code": result.returncode,
                    "started_epoch_ns": started, "finished_epoch_ns": time.time_ns()})
        if result.stdout.strip():
            write_text(self.evidence / f"step-{self.serial:03d}.stdout", result.stdout)
        if result.stderr.strip():
            write_text(self.evidence / f"step-{self.serial:03d}.stderr", result.stderr)
        try:
            value = json.loads(result.stdout)
        except json.JSONDecodeError:
            raise DriverError("无法解析返回：" + result.stdout[:400] + result.stderr[:400])
        if not value.get("ok"):
            code = (value.get("error") or {}).get("code")
            if not (allow_missing_window and code == "window_not_found"):
                raise DriverError(json.dumps(value, ensure_ascii=False)[:1500])
            return {}
        payload = value.get("result") or {}
        shot = (payload.get("screenshot") or {}).get("path")
        if shot and Path(shot).is_file():
            shutil.copyfile(str(shot), str(self.evidence / f"step-{self.serial:03d}.png"))
        return payload

    def probe(self):
        """问一次窗口状态，返回 (是否已登记, 错误码)；不投递任何输入。

        启动瞬间进程存在但无障碍树里还没有它，此时报 ``app_not_found``。
        """
        argv = [ORCA, "computer", "get-app-state", "--app", "pid:" + str(self.pid), "--json"]
        result = subprocess.run(argv, text=True, capture_output=True)
        return interpret_probe(result.stdout)

    def wait_until_ready(self, timeout=STARTUP_TIMEOUT):
        """等窗口登记进无障碍树，再投递第一个动作。

        启动竞争是真实存在的：进程刚起来时无障碍树里还没有它，此刻投递
        会得到 ``app_not_found``。此前驱动在第一步即失败，头几个样本因此
        全军覆没——失败发生在打开之前，与样本本身无关。等待到窗口可寻址
        才动作，才是对同一批样本的真实取证。

        等待轮数一并留证：等待本身也是证据的一部分。
        """
        started = time.monotonic()
        try:
            attempts = wait_for_ready(probe=self.probe, timeout=timeout,
                                       poll=STARTUP_POLL, monotonic=time.monotonic)
        except DriverError as error:
            write_json(self.evidence / "startup-wait.json",
                       {"attempts": None, "ready": False, "error": str(error)[:300]})
            raise
        write_json(self.evidence / "startup-wait.json",
                   {"attempts": attempts, "ready": True,
                    "waited_seconds": round(time.monotonic() - started, 3)})
        print("窗口已可寻址，等待轮次：" + str(attempts), flush=True)
    def snapshot(self):
        return self.call(["get-app-state"])["snapshot"]

    def tree_text(self, snapshot):
        return snapshot.get("treeText", "")

    def find_button(self, snapshot, label):
        """按可见名称找按钮；索引取自本次树，不跨动作复用。"""
        match = re.search(r'^\s*(\d+) button ' + re.escape(label) + r'\s*$',
                          self.tree_text(snapshot), re.M)
        return int(match.group(1)) if match else None

    def find_close_button(self, snapshot):
        match = re.search(r'^\s*(\d+) close button', self.tree_text(snapshot), re.M)
        return int(match.group(1)) if match else None

    def open_document(self, path):
        """打开面板 → 前往文件夹 → 向焦点投递路径 → 两次回车 → 打开。

        路径**不**通过无障碍树查找输入框：面板列出证据目录时树会被截断，
        输入框不在树里。`type-text` 作用于当前焦点，焦点由 macOS 自行落在
        路径输入框上，因此与树是否被截断无关。

        这条路线的风险是焦点没落在路径框时，路径会被打进别处。该风险由
        末尾的标题断言兜底：文件名不在窗口标题里即判失败，不会静默通过。

        路径输入框里的回车只会收起「前往文件夹」并把面板定位到该文件，
        真正打开文件需要第二次回车（落在「打开」按钮上）。
        """
        self.call(["hotkey", "--key", OPEN_PANEL_KEY, "--restore-window"], "打开文件面板")
        time.sleep(1)
        self.call(["hotkey", "--key", GO_TO_FOLDER_KEY, "--restore-window"], "前往文件夹")
        time.sleep(1)
        self.call(["type-text", "--text", str(path), "--restore-window"], "输入路径")
        time.sleep(1)
        self.call(["press-key", "--key", "Return", "--restore-window"], "确认路径")
        time.sleep(1)
        self.call(["press-key", "--key", "Return", "--restore-window"], "打开文档")
        time.sleep(2)
        snapshot = self.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        if Path(path).name not in title:
            raise DriverError("打开的文档与目标不符：" + title)

    def heading_point(self):
        """从最新截图定位正文首个文本带（标题行）的中心，返回窗口相对坐标。

        标题是正文里最大的字形，位于工具栏之下、状态栏之上；按暗像素聚类找
        第一条文本带，再取该带内暗像素的包围盒中心。截图缩放由窗口尺寸与
        截图像素尺寸推出，不假定固定倍率。
        """
        from PIL import Image

        shot = sorted(self.evidence.glob("step-*.png"))[-1]
        image = Image.open(shot).convert("L")
        width, height = image.size
        window = self.snapshot().get("window", {})
        scale = width / float(window["width"]) if window.get("width") else 1.0
        pixels = image.load()
        top = int(CONTENT_TOP * scale)
        bottom = min(height, int(CONTENT_BOTTOM * scale))
        rows = []
        for y in range(top, bottom):
            count = sum(1 for x in range(0, width, 2) if pixels[x, y] < 140)
            rows.append((y, count))
        band = None
        for y, count in rows:
            if count > 0:
                if band is None:
                    band = [y, y]
                else:
                    band[1] = y
            elif band is not None and y - band[1] > 6:
                break
        if band is None:
            raise DriverError("正文区域没有找到可命中的文本")
        xs = []
        for y in range(band[0], band[1] + 1):
            for x in range(0, width):
                if pixels[x, y] < 140:
                    xs.append(x)
        if not xs:
            raise DriverError("文本带内没有暗像素")
        point = ((min(xs) + max(xs)) / 2 / scale, (band[0] + band[1]) / 2 / scale)
        write_json(self.evidence / "hit-point.json",
                   {"step": self.serial, "point": [point[0], point[1]],
                    "band_px": band, "text_pixels": len(xs)})
        return point

    def hit_body(self):
        """像素点击正文标题，产生真实命中事件。"""
        point = self.heading_point()
        self.call(["click", "--x", str(round(point[0], 1)), "--y", str(round(point[1], 1))],
                  "像素命中正文")
        time.sleep(1)

    def jump_via_toc(self):
        """点击目录项，产生主动跳转事件。"""
        snapshot = self.snapshot()
        links = re.findall(r'^\s*(\d+) link(?:\s|$)', self.tree_text(snapshot), re.M)
        if not links:
            raise DriverError("没有找到目录跳转项：" + self.tree_text(snapshot)[:600])
        self.call(["click", "--element-index", links[0]], "点击目录项跳转")
        time.sleep(1)

    def round_trip(self):
        """源码 → 可视化往返。"""
        for label in ("源码", "可视化"):
            snapshot = self.snapshot()
            index = self.find_button(snapshot, label)
            if index is None:
                raise DriverError("没有找到按钮 " + label + "：" + self.tree_text(snapshot)[:600])
            self.call(["click", "--element-index", str(index)], "切换到 " + label)
            time.sleep(1)


def run_cases(driver, fixtures):
    """逐个样本走完七步；返回失败清单。"""
    failures = []
    for index, name in enumerate(CASES, 1):
        path = fixtures / (name + ".md")
        print("=== 用例", name, "===", flush=True)
        try:
            driver.open_document(path)
            # 滚动：真实滚轮事件，产生 scroll-wheel。
            driver.call(["scroll", "--x", "400", "--y", "300", "--direction", "down"],
                        "真实滚轮滚动")
            time.sleep(1)
            driver.hit_body()
            driver.jump_via_toc()
            driver.round_trip()
            # 关闭文档：cmd-w 关闭文档，证据在关闭后落盘。
            driver.call(["hotkey", "--key", CLOSE_DOCUMENT_KEY], "关闭文档")
            time.sleep(3)
            snapshot = driver.snapshot()
            print("关闭后标题:", (snapshot.get("window") or {}).get("title"), flush=True)
        except DriverError as error:
            failures.append(name + "：" + str(error)[:300])
            print("用例失败：" + name + "：" + str(error)[:300], flush=True)
    return failures


def close_window(driver):
    """关闭最后一个窗口让原生进程自行退出。

    cmd-w 只关闭文档、不关窗口，所以这里点窗口的关闭按钮；应用以
    `QuitMode::LastWindowClosed` 自行退出，取证入口因此拿到真实退出码 0。
    """
    try:
        snapshot = driver.snapshot()
        index = driver.find_close_button(snapshot)
        if index is None:
            return ["没有找到窗口关闭按钮"]
        driver.call(["click", "--element-index", str(index)], "点击窗口关闭按钮",
                    allow_missing_window=True)
        time.sleep(3)
        return []
    except DriverError as error:
        return ["关闭窗口失败：" + str(error)[:200]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int, nargs="?", help="取证入口已启动的原生窗口进程号")
    parser.add_argument("fixtures", type=Path, nargs="?", help="夹具目录")
    parser.add_argument("evidence", type=Path, nargs="?", help="证据目录")
    parser.add_argument("--self-test", action="store_true",
                        help="只跑合成自测：不启动窗口、不投递真实输入")
    parser.add_argument("--startup-timeout", type=float, default=STARTUP_TIMEOUT,
                        help="等待窗口登记进无障碍树的秒数上限")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromTestCase(DriverTests))
        return 0 if result.wasSuccessful() else 1
    if args.pid is None or args.fixtures is None or args.evidence is None:
        parser.error("需要 <app_pid> <夹具目录> <证据目录>")
    # 证据落点必须在仓库 logs/validation 内：驱动会写文件，越界即拒绝。
    try:
        evidence = require_validation_path(args.evidence)
    except ValueError as error:
        print("只读窗口驱动：" + str(error), file=sys.stderr)
        return 1
    evidence.mkdir(parents=True, exist_ok=True)
    driver = WindowDriver(args.pid, evidence)
    failures = []
    try:
        # 第一个动作之前先等窗口登记进无障碍树：进程刚起来时树里还没有它，
        # 此刻投递只会得到 app_not_found，样本会在打开之前就失败。
        driver.wait_until_ready(timeout=args.startup_timeout)
    except DriverError as error:
        failures.append("等待窗口可寻址失败：" + str(error)[:300])
        print("用例失败：等待窗口可寻址：" + str(error)[:300], flush=True)
    if not failures:
        failures.extend(run_cases(driver, args.fixtures.resolve()))
    failures.extend(close_window(driver))
    # 驱动退出码落证：标准入口回读本目录据此判定驱动是否完成全部样本，
    # 不采信 results.json 的自述。
    write_json(evidence / "driver-exit.json", {"exit_code": 1 if failures else 0})
    print(json.dumps({"failures": failures}, ensure_ascii=False))
    return 1 if failures else 0


class DriverTests(unittest.TestCase):
    """驱动的合成自测：不启动窗口、不投递真实输入。

    自测只证明驱动的判定与等待策略本身，不证明真实窗口上的七步成立。
    """

    def test_probe_reads_the_error_code_from_a_failed_lookup(self):
        # 合成返回：窗口尚未登记时工具给出 app_not_found。
        ready, code = interpret_probe('{"ok": false, "error": {"code": "app_not_found"}}')
        self.assertFalse(ready)
        self.assertEqual(code, "app_not_found")
        # 合成返回：窗口可寻址。
        ready, code = interpret_probe('{"ok": true, "result": {"snapshot": {}}}')
        self.assertTrue(ready)
        self.assertEqual(code, "")
        # 合成返回：无法解析时不得当成可寻址。
        ready, code = interpret_probe("not json")
        self.assertFalse(ready)
        self.assertEqual(code, "unparsable")

    def test_wait_policy_retries_until_the_window_is_reachable(self):
        # 正例：前三次未登记，第四次可寻址；必须等到可寻址才返回。
        sequence = [(False, "app_not_found"), (False, "app_not_found"),
                    (False, "app_not_found"), (True, "")]
        cursor = [0]

        def probe():
            value = sequence[cursor[0]]
            cursor[0] += 1
            return value

        clock = FakeClock()
        attempts = wait_for_ready(probe=probe, timeout=10.0, poll=0.0,
                                  monotonic=clock.now)
        self.assertEqual(attempts, 4)
        # 反例：始终未登记且已超时，必须报错而不是静默放行。
        with self.assertRaises(DriverError):
            wait_for_ready(probe=lambda: (False, "app_not_found"),
                           timeout=0.0, poll=0.0, monotonic=FakeClock().now)

    def test_evidence_directory_outside_the_repository_is_rejected(self):
        # 正例：仓库 logs/validation 内的落点被接受。
        accepted = require_validation_path(
            ROOT / 'logs' / 'validation' / 'm1-single-window-readonly' / 'driver-selftest')
        self.assertTrue(str(accepted).startswith(str(ROOT / 'logs' / 'validation')))
        # 反例：仓库外的落点必须拒绝，且不创建任何目录。
        outside = ROOT / 'docs' / 'driver-must-not-create'
        with self.assertRaisesRegex(ValueError, 'logs/validation'):
            require_validation_path(outside)
        self.assertFalse(outside.exists())

    def test_main_writes_the_driver_exit_code_as_evidence(self):
        # 驱动必须把自己的退出码落证：标准入口回读该留证判定驱动是否完成全部样本，
        # 不采信 results.json 的自述。走**真实入口**，只测驱动判定本身不足以证明落证。
        evidence = ROOT / 'logs' / 'validation' / 'm1-single-window-readonly' / 'driver-exit-selftest'
        result = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), '1', str(ROOT), str(evidence),
             '--startup-timeout', '1'],
            cwd=ROOT, text=True, capture_output=True)
        # 进程号 1 不存在，驱动会失败；无论成败都必须留下退出码留证。
        recorded = evidence / 'driver-exit.json'
        try:
            self.assertTrue(recorded.is_file())
            value = json.loads(recorded.read_text(encoding='utf-8'))
            self.assertIn(value.get('exit_code'), (0, 1))
            self.assertEqual(value.get('exit_code'), result.returncode)
        finally:
            shutil.rmtree(evidence, ignore_errors=True)

    def test_main_rejects_an_evidence_directory_outside_the_repository(self):
        # 反例走**真实入口**：命令行给出仓库外的证据落点时必须拒绝并退出非零。
        # 只测 require_validation_path 本身不足以证明入口真的调用了它。
        outside = ROOT / 'docs' / 'driver-cli-must-not-create'
        result = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), '1', str(ROOT), str(outside)],
            cwd=ROOT, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('logs/validation', result.stdout + result.stderr)
        self.assertFalse(outside.exists())


class FakeClock:
    """合成时钟：每次取时间前进一秒，用于复现等待序列而不真等。"""

    def __init__(self):
        self.calls = 0

    def now(self):
        value = float(self.calls)
        self.calls += 1
        return value


def interpret_probe(stdout):
    """把一次探测的原始返回解析成 (是否可寻址, 错误码)。"""
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError:
        return False, "unparsable"
    if value.get("ok"):
        return True, ""
    return False, (value.get("error") or {}).get("code") or "unknown"


def wait_for_ready(probe, timeout, poll, monotonic):
    """等窗口可寻址；返回探测轮数，超时则抛 :class:`DriverError`。

    策略只有一条：先探测一次，可寻址即返回；不可寻址且未超时则等待后
    重试。合成自测注入固定时钟与 ``poll=0``，复现同一序列而不真等。
    """
    deadline = monotonic() + timeout
    attempts = 0
    code = ""
    while True:
        ready, code = probe()
        attempts += 1
        if ready:
            return attempts
        if monotonic() >= deadline:
            raise DriverError("等待窗口可寻址超时，最后错误码：" + str(code))
        time.sleep(poll)


if __name__ == "__main__":
    raise SystemExit(main())
