#!/usr/bin/env python3
"""真实窗口取 M1 1 MiB 文档的可视化滚动证据。

8.2 的第 6 行要求「Release 下真实 1 MiB 文档可从头到尾滚动并操作」。此前的
1 MiB 窗口滚动记录是 2026-11-06 的 debug 构建，早于 2026-11-09 的渲染重写；
2026-11-10 的 Release 性能矩阵虽覆盖 1 MiB，但那是程序化探针、无窗口截图，
其口径自述「不代表屏幕呈现」。两者都不满足这一行。

本入口在当前 Release 二进制上驱动真实窗口滚动，把每一段的证据留在仓库内：

- 打开 1 MiB 夹具后先确认窗口标题就是该夹具（不是别的文档）；
- 在**可视化视图**下用真实滚轮向下滚动，核对滚动比例确实前进；
- 再按 End 滚到文档末尾，核对两件事：滚动比例接近 1，且**夹具末尾的围栏代码
  块出现在画面上**——后者才证明滚到了文档**末尾**，而不只是「画面变了」；
- 末尾标记块整块浅底，与滚动条同色但不同形状，两者互不冒充；
- 反向滚轮与 Home 回顶分别证明「能往回滚」和「能回到同一位置」；
- 底层回调事件由应用自身落盘（`ALINE_M1_READONLY_EVIDENCE_DIR`），
  与截图证据互相独立。

口径边界：本入口证明当前 Release 二进制在真实窗口里能滚过 1 MiB 文档的
可视化内容，**不**证明首屏呈现时延或帧率；那些由 V-05 性能矩阵负责。
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import unittest

from evidence_paths import display_path, write_json, write_text
from validation_output import output_directory

ROOT = Path(__file__).resolve().parent.parent
ORCA = "/Applications/Orca.app/Contents/Resources/bin/orca"

# 夹具：1 MiB，每段正文带唯一编号（`## Section 000123`）。
#
# 2026-11-06 的 1 MiB 窗口滚动记录用的是 many-short-blocks 夹具，全文只有
# 三种不同的行——滚到任何位置看到的字都一样，**无法从截图判定滚动是否真的
# 改变了可见内容**，只能证明「事件被投递了」。本夹具每段编号唯一，滚到不同
# 位置必然看到不同的字，指纹才能真正区分位置。
#
# 字节与哈希在脚本内固定：夹具被改动即拒绝，避免证据与夹具对不上。
FIXTURE_NAME = "size-1MiB-unique-sections.md"
FIXTURE_SHA256 = "b739ff309357b7004020023c53f03ec77a7c52c3fe760f3ecccad36077cdb00f"

# 文档末尾的唯一标记：放在围栏代码块里，整块渲染为浅底。
#
# 「滚过一段」只能证明画面变了；「末段标记出现在画面上」才能证明已经滚到
# **文档末尾**。§13 第 6 行要求「可从头到尾滚动」，没有末尾标记就只能证明
# 「滚了一段」。
END_MARKER = b"ALINE_M1_END_OF_DOCUMENT"
END_BLOCK = b"```\n" + END_MARKER + b"\n```\n"

# 正文区域在窗口内的纵向范围：标题栏与工具栏之下、状态栏之上。
CONTENT_TOP = 70.0
CONTENT_BOTTOM = 630.0

# 真实滚轮一次滚动的方向与幅度。
SCROLL_STEP = 12
# 到底后再向上滚回顶部，同样次数。
SCROLL_BACK_STEP = 12
# 真实滚轮投递的次数。
#
# 应用把回调事件记在一条 256 条上限的序列里（见 readonly_evidence.rs），而一次
# 滚轮会随视口移动**每渲染一行**触发一次滚动回调：实测一次 12 页滚轮产生
# 71 条事件，一次 1 页滚轮也产生 43 条。滚轮次数因此必须压到三条以内，
# 否则事件序列先被滚轮占满，Home/End 的键盘事件就记不进去——上限一到，后续
# 事件一律丢弃并只留一条 ``event-limit-exceeded``。
WHEEL_CALLS = 2


class DriverError(RuntimeError):
    """真实输入未能按预期送达窗口。"""


def sha(data):
    return hashlib.sha256(data).hexdigest()


def build_fixture(path):
    """生成 1 MiB 唯一编号夹具；内容与哈希固定，可重复生成。

    每段是 ``## Section NNNNNN`` 加一行带同编号的正文。编号唯一，因此任意
    两个不同滚动位置的可见正文必然不同，截图指纹才能真正区分位置。

    末尾追加一段围栏代码块，块内是全篇唯一的 ``END_MARKER``。代码块渲染成
    整块浅底，在画面上可直接认出：它出现就说明**已经滚到文档末尾**，而不只是
    「画面变了」。
    """
    size = 1024 * 1024
    parts = []
    total = 0
    index = 0
    while total < size:
        index += 1
        part = f"## Section {index:06d}\n\nalpha beta gamma delta epsilon {index:06d}.\n\n"
        parts.append(part)
        total += len(part)
    # 截断可能落在多字节字符中间：按字节截断后补空格，使长度与内容都确定。
    body = "".join(parts).encode("utf-8")[: size - len(END_BLOCK)]
    data = body + END_BLOCK
    data = data + b" " * (size - len(data))
    if sha(data) != FIXTURE_SHA256:
        raise SystemExit("生成的夹具哈希与固定值不符：" + sha(data))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return data


def text_band(pixels, width, height, scale):
    """找出正文里第一条有暗像素的文本带，返回该带的像素行范围与暗像素数。

    正文是深色字、浅色底；按行统计暗像素，第一条连续有暗像素的带就是第一行
    正文。返回 ``(top, bottom, count)``，没有正文时返回 ``None``。
    """
    top = int(CONTENT_TOP * scale)
    bottom = min(height, int(CONTENT_BOTTOM * scale))
    band = None
    for y in range(top, bottom):
        count = sum(1 for x in range(0, width, 2) if pixels[x, y] < 140)
        if count > 0:
            if band is None:
                band = [y, y, 0]
            band[1] = y
            band[2] += count
        elif band is not None and y - band[1] > 6:
            break
    return tuple(band) if band else None


# 滚动条轨道与拇指的颜色。应用把轨道画成 ``GRAY_050``、拇指画成
# ``GRAY_500``（见 crates/aline-ui/src/native_window/tokens.rs）。两者在灰度下
# 只差 7 级，与白色（255）更接近，因此必须**按 RGB 逐通道**匹配：轨道是实心
# 填充，没有抗锯齿，逐通道 4 级容差既能命中又不会把白色误认成轨道。
RAIL_RGB = (246, 248, 250)
THUMB_RGB = (140, 149, 159)
LEVEL_TOLERANCE = 4


def _matches(pixel, target, tolerance=LEVEL_TOLERANCE):
    """逐通道比较一个 RGB 像素是否落在目标色附近。"""
    if not isinstance(pixel, tuple):
        return False
    return all(abs(actual - expected) <= tolerance
               for actual, expected in zip(pixel, target))


def _column_bands(pixels, width, height, scale):
    """读出窗口最右侧滚动条列上的分段：从上到下的类别与行区间。

    ``pixels`` 是 RGB 访问器（``pixels[x, y]`` 返回三元组）。
    """
    column = width - int(8 * scale)
    if column < 0:
        return []
    segments = []
    current = None
    for y in range(height):
        pixel = pixels[column, y]
        if _matches(pixel, RAIL_RGB):
            level = "rail"
        elif _matches(pixel, THUMB_RGB):
            level = "thumb"
        else:
            level = None
        if level != current:
            if current is not None:
                segments[-1][2] = y - 1
            current = level
            if level is not None:
                segments.append([level, y, y])
        elif level is not None:
            segments[-1][2] = y
    return segments


def scroll_fraction(pixels, width, height, scale):
    """从滚动条拇指的位置读出当前滚动比例。

    应用按 ``thumb_top = 轨道上沿 + (轨道高 - 拇指高) × 滚动比例`` 画拇指
    （见 ``readonly_reader.rs``），所以拇指上沿在轨道可用行程里的位置就是滚动
    比例的量化读数：顶端 0、底端 1。

    拇指画在轨道之上，它遮住的那几行轨道是看不见的：比例 0 时遮住顶部、
    比例 1 时遮住底部。但**轨道与拇指的并集**始终是完整的轨道（拇指只可能
    落在轨道之内），所以并集端点就是轨道的真实上下沿，行程才能复原。

    没有滚动条时返回 ``None``：把「测不到」写成 0 就等于谎称「在顶部」。
    """
    segments = _column_bands(pixels, width, height, scale)
    rail = [item for item in segments if item[0] == "rail"]
    thumb = [item for item in segments if item[0] == "thumb"]
    if not rail or not thumb:
        return None
    rail_top = min(min(item[1] for item in rail), min(item[1] for item in thumb))
    rail_bottom = max(max(item[2] for item in rail), max(item[2] for item in thumb))
    thumb_top = min(item[1] for item in thumb)
    thumb_height = max(item[2] for item in thumb) - thumb_top + 1
    travel = (rail_bottom - rail_top + 1) - thumb_height
    if travel <= 0:
        return None
    return max(0.0, min(1.0, (thumb_top - rail_top) / float(travel)))


def code_band(pixels, width, height, scale):
    """找出正文里整行浅底的代码块，返回它的行范围。

    围栏代码块渲染成整块 ``GRAY_050`` 浅底（见 ``readonly_reader.rs`` 的
    ``Block::Code``）。文档末尾放一段这样的块，它出现在画面上就说明已经滚到
    末尾——比「指纹变了」更能说明滚到了**哪里**。

    右侧滚动条本身也是浅底，必须排除；正文区又宽得多，所以要求该行在正文
    列里绝大多数采样点都是浅底。
    """
    top = int(CONTENT_TOP * scale)
    bottom = min(height, int(CONTENT_BOTTOM * scale))
    left = int(40 * scale)
    right = width - int(40 * scale)
    step = max(1, int(8 * scale))
    columns = list(range(left, right, step))
    if not columns:
        return None
    required = len(columns) * 3 // 5
    band = None
    for y in range(top, bottom):
        count = sum(1 for x in columns if _matches(pixels[x, y], RAIL_RGB))
        if count >= required:
            if band is None:
                band = [y, y]
            else:
                band[1] = y
        elif band is not None and y - band[1] > 4:
            break
    return tuple(band) if band else None


def text_signature(pixels, width, height, scale, bands=3):
    """取正文前若干条文本带的暗像素列轮廓，作为「当前可见内容」的指纹。

    滚动到不同位置时可见文本不同，指纹随之改变；指纹不变即说明可见内容
    没动，滚动没有真正发生。返回每条带的 ``(top, bottom, 左缘, 右缘)``。
    """
    top = int(CONTENT_TOP * scale)
    bottom = min(height, int(CONTENT_BOTTOM * scale))
    result = []
    current = None
    for y in range(top, bottom):
        count = sum(1 for x in range(0, width, 2) if pixels[x, y] < 140)
        if count > 0:
            if current is None:
                current = [y, y]
            else:
                current[1] = y
        elif current is not None and y - current[1] > 6:
            result.append(current)
            current = None
            if len(result) >= bands:
                break
    if current is not None and len(result) < bands:
        result.append(current)
    signature = []
    for band_top, band_bottom in result:
        xs = [x for y in range(band_top, band_bottom + 1)
              for x in range(width) if pixels[x, y] < 140]
        signature.append((band_top, band_bottom, min(xs), max(xs)) if xs else (band_top, band_bottom, -1, -1))
    return signature


class WindowDriver:
    """在已启动的原生窗口上投递真实输入，并把每次动作留证。"""

    def __init__(self, pid, evidence):
        self.pid = pid
        self.evidence = evidence
        self.serial = 0

    def call(self, args, note="", allow_missing_window=False):
        self.serial += 1
        argv = [ORCA, "computer"] + args + ["--app", "pid:" + str(self.pid), "--json"]
        result = subprocess.run(argv, text=True, capture_output=True)
        write_json(self.evidence / ("step-%03d.command.json" % self.serial),
                   {"argv": argv, "note": note, "exit_code": result.returncode,
                    "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "stdout": result.stdout, "stderr": result.stderr})
        if result.returncode != 0:
            raise DriverError("动作失败：" + note + "：" + result.stderr[:200])
        payload = json.loads(result.stdout) if result.stdout.strip() else {}
        ok = payload.get("ok")
        if ok is False:
            code = (payload.get("error") or {}).get("code", "")
            if not (allow_missing_window and code == "window_not_found"):
                raise DriverError("动作未被接受：" + note + "：" + str(payload.get("error"))[:200])
        screenshot = ((payload.get("result") or {}).get("screenshot") or {})
        path = screenshot.get("path")
        if path and Path(path).is_file():
            shutil.copyfile(path, self.evidence / ("step-%03d.png" % self.serial))
        return payload

    def snapshot(self):
        return self.call(["get-app-state"])["result"]["snapshot"]

    def tree_text(self, snapshot):
        return snapshot.get("treeText", "")

    def open_document(self, path):
        """打开面板 → 前往文件夹 → 向焦点投递路径 → 两次回车 → 打开。"""
        self.call(["hotkey", "--key", "cmd+o", "--restore-window"], "打开文件面板")
        time.sleep(1)
        self.call(["hotkey", "--key", "cmd+shift+g", "--restore-window"], "前往文件夹")
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
        return title

    def visible_signature(self):
        """读出当前可见正文的指纹；截图是最新一张动作留证。"""
        from PIL import Image

        shot = sorted(self.evidence.glob("step-*.png"))[-1]
        image = Image.open(shot).convert("L")
        width, height = image.size
        window = self.snapshot().get("window", {})
        scale = width / float(window["width"]) if window.get("width") else 1.0
        pixels = image.load()
        return text_signature(pixels, width, height, scale)

    def measure(self):
        """读出当前画面里的位置证据：滚动比例与末尾标记。

        两个读数互相独立：比例说「大概在哪」，末尾标记说「确实到头了」。
        """
        from PIL import Image

        shot = sorted(self.evidence.glob("step-*.png"))[-1]
        image = Image.open(shot).convert("RGB")
        width, height = image.size
        window = self.snapshot().get("window", {})
        scale = width / float(window["width"]) if window.get("width") else 1.0
        pixels = image.load()
        return {
            "fraction": scroll_fraction(pixels, width, height, scale),
            "end_marker": code_band(pixels, width, height, scale),
            "screenshot": shot.name,
        }

    def scroll_down(self):
        """真实滚轮向下滚动一次。"""
        self.call(["scroll", "--x", "400", "--y", "300", "--direction", "down",
                   "--pages", str(SCROLL_STEP)], "真实滚轮向下滚动")
        time.sleep(0.4)

    def scroll_up(self):
        """真实滚轮向上滚动一次。"""
        self.call(["scroll", "--x", "400", "--y", "300", "--direction", "up",
                   "--pages", str(SCROLL_BACK_STEP)], "真实滚轮向上滚动")
        time.sleep(0.4)

    def scroll_to_top(self):
        """按 Home 回到顶部。

        滚回去用**相同次数**的反向滚轮并不可靠：向下与向上的一次滚动步长
        未必对称，最后一步又落在行内偏移上，回到的位置与初始不完全一致。
        Home 是「回到顶部」的确定操作，回到的就是同一个位置，才能用来核对
        「滚回去之后可见正文与初始一致」。
        """
        self.call(["press-key", "--key", "Home", "--restore-window"], "按 Home 回到顶部")
        time.sleep(0.6)

    def scroll_to_end(self):
        """按 End 滚到文档末尾。

        真实滚轮一次只推进约 0.2%，1 MiB 文档要滚几千次才到底；End 是应用
        自身提供的「跳到末尾」操作（``ReadEnd``），走的是同一条滚动路径。
        """
        self.call(["press-key", "--key", "End", "--restore-window"], "按 End 滚到末尾")
        time.sleep(1.5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true",
                        help="只跑合成自测：不启动窗口、不投递真实输入")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromTestCase(ScrollTests))
        return 0 if result.wasSuccessful() else 1
    output = output_directory("m1-single-window-readonly", args.output_dir)
    fixtures = output / "fixtures"
    driver_evidence = output / "driver"
    driver_evidence.mkdir(parents=True, exist_ok=True)
    fixture = fixtures / FIXTURE_NAME
    if not fixture.is_file():
        build_fixture(fixture)
    actual = sha(fixture.read_bytes())
    if actual != FIXTURE_SHA256:
        raise SystemExit("夹具字节与固定哈希不符：" + actual)

    app = (ROOT / "target/release/aline").resolve(strict=True)
    env = dict(os.environ, ALINE_M1_READONLY_EVIDENCE_DIR=str((output / "snapshots").resolve()))
    for key in list(env):
        if key.startswith("ALINE_M1_PROBE_"):
            del env[key]
    (output / "snapshots").mkdir(exist_ok=True)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    process = subprocess.Popen([str(app)], cwd=ROOT, env=env, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    write_json(output / "launch.json", {
        "schema": "aline-m1-scroll/v1", "pid": process.pid,
        "app_sha256": sha(app.read_bytes()), "fixture": fixture.name,
        "fixture_sha256": actual, "started_utc": started})
    failures = []
    try:
        driver = WindowDriver(process.pid, driver_evidence)
        # 等待窗口登记进无障碍树：进程刚起时树里还没有它。
        deadline = time.monotonic() + 30.0
        while time.monotonic() < deadline:
            try:
                driver.snapshot()
                break
            except DriverError:
                time.sleep(0.3)
        title = driver.open_document(fixture)
        top = driver.visible_signature()
        top_measure = driver.measure()
        write_json(output / "signature-top.json",
                   {"title": title, "signature": top, "measure": top_measure})
        # 真实滚轮向下滚若干次：证明滚轮真的能推动长文档。
        for _ in range(WHEEL_CALLS):
            driver.scroll_down()
        wheeled = driver.measure()
        wheeled_signature = driver.visible_signature()
        write_json(output / "signature-wheeled.json",
                   {"signature": wheeled_signature, "measure": wheeled})
        if wheeled["fraction"] is None:
            failures.append("读不到滚动条位置：滚轮之后无法量化滚动比例")
        elif top_measure["fraction"] is not None and \
                wheeled["fraction"] <= top_measure["fraction"]:
            failures.append("真实滚轮之后滚动比例没有前进：" +
                            str(top_measure["fraction"]) + " → " + str(wheeled["fraction"]))
        if wheeled_signature == top:
            failures.append("真实滚轮之后可见正文与初始相同：滚动没有真正发生")
        # 反向滚轮：向上滚一次必须真的改变可见内容。
        driver.scroll_up()
        after_up = driver.visible_signature()
        after_up_measure = driver.measure()
        write_json(output / "signature-after-up.json",
                   {"signature": after_up, "measure": after_up_measure})
        if after_up == wheeled_signature:
            failures.append("向上滚动后可见正文与滚之前相同：反向滚动没有真正发生")
        # 滚到文档末尾：这是「从头到尾」里「到尾」的证据。
        driver.scroll_to_end()
        end = driver.visible_signature()
        end_measure = driver.measure()
        write_json(output / "signature-end.json",
                   {"signature": end, "measure": end_measure})
        if end_measure["fraction"] is None:
            failures.append("读不到滚动条位置：无法确认是否滚到末尾")
        elif end_measure["fraction"] < 0.98:
            failures.append("滚到末尾时滚动比例只有 " +
                            str(end_measure["fraction"]) + "：没有真正滚到底")
        if end_measure["end_marker"] is None:
            failures.append("末尾标记没有出现在画面上：没有真正滚到文档末尾")
        # 再按 Home 回到顶部：这是确定回到同一位置的操作。
        driver.scroll_to_top()
        back = driver.visible_signature()
        back_measure = driver.measure()
        write_json(output / "signature-back.json",
                   {"signature": back, "measure": back_measure})
        # 回到顶部后应与初始一致，否则回滚没到位。
        if back != top:
            failures.append("回到顶部后可见正文与初始不一致")
        if back_measure["fraction"] is not None and back_measure["fraction"] > 0.02:
            failures.append("回到顶部后滚动比例仍有 " +
                            str(back_measure["fraction"]) + "：没有真正回到顶部")
        # cmd-w 关闭文档：应用在关闭时把自己记录的回调序列与字节落盘，
        # 这是**与截图相互独立**的第二来源——截图证「看见的内容变了」，
        # 应用回调证「滚动事件真的到达了窗口」。
        driver.call(["hotkey", "--key", "cmd+w", "--restore-window"], "关闭文档")
        time.sleep(3)
    except DriverError as error:
        failures.append(str(error)[:300])
    finally:
        try:
            driver_close = WindowDriver(process.pid, driver_evidence)
            snapshot = driver_close.snapshot()
            match = re.search(r'^\s*(\d+) close button', driver_close.tree_text(snapshot), re.M)
            if match:
                driver_close.call(["click", "--element-index", match.group(1)],
                                  "点击窗口关闭按钮", allow_missing_window=True)
                time.sleep(2)
        except (DriverError, OSError, ValueError):
            pass
        try:
            stdout, stderr = process.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            stdout, stderr = process.communicate()
        write_text(output / "app.stdout", stdout)
        write_text(output / "app.stderr", stderr)
    failures.extend(check_app_evidence(output))
    write_json(output / "result.json", {
        "schema": "aline-m1-scroll/v1", "passed": not failures, "failures": failures,
        "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()})
    print(json.dumps({"passed": not failures, "failures": failures}, ensure_ascii=False))
    return 1 if failures else 0


def check_app_evidence(output):
    """核对应用自己落盘的回调记录；没有它就只剩截图一个来源。

    截图证「可见内容变了」，应用回调证「滚动事件真的到达了窗口」。两者
    独立：截图可能被误读或误配，回调记录由应用在关闭文档时写落。
    """
    failures = []
    snapshots = output / "snapshots"
    directories = sorted(path for path in snapshots.iterdir() if path.is_dir()) if snapshots.is_dir() else []
    if not directories:
        failures.append("应用没有落盘任何回调记录：滚动事件没有到达窗口")
        return failures
    state = directories[0] / "state.tsv"
    if not state.is_file():
        failures.append("应用回调记录缺少 state.tsv")
        return failures
    fields = {}
    for line in state.read_text(encoding="utf-8").splitlines():
        for item in line.split("\t"):
            key, value = item.split("=", 1)
            fields[key] = value
    steps = [step for step in fields.get("steps", "").split(",") if step]
    if "event-limit-exceeded" in steps:
        failures.append("应用回调记录超出 256 条上限：序列不完整，不能当作完整证据")
    if not any(step in ("scroll-wheel", "scroll-key") for step in steps):
        failures.append("应用回调记录里没有滚动事件：" + ",".join(steps))
    if "scroll-key" not in steps:
        failures.append("应用回调记录里没有键盘滚动事件：Home/End 未到达窗口")
    if fields.get("has_draft") != "false":
        failures.append("应用回调记录出现草稿")
    if fields.get("history_len") != "0":
        failures.append("应用回调记录历史不为空")
    if fields.get("document_closed") != "true":
        failures.append("应用回调记录未确认文档关闭")
    if fields.get("disk_bytes_equal") != "true":
        failures.append("应用回调记录磁盘字节不相等")
    return failures


class ScrollTests(unittest.TestCase):
    """合成自测：只证明指纹与判定逻辑，不证明真实窗口滚动。"""

    @staticmethod
    def _pixels(rows):
        """按 PIL ``Image.load()`` 的接口造一张灰度图：``pixels[x, y]``。"""
        width = len(rows[0])
        height = len(rows)
        buffer = {}

        class _PixelAccess:
            def __getitem__(self, point):
                return buffer[point]

        for y, row in enumerate(rows):
            for x, value in enumerate(row):
                buffer[(x, y)] = value
        return width, height, _PixelAccess()

    @staticmethod
    def _rail_image(height=600, rail_top=40, rail_bottom=560, fraction=0.0,
                    thumb_height=40, thumb_left=390, thumb_right=396):
        """造一张带滚动条轨道的 RGB 图：轨道浅、拇指深。

        真实截图里轨道是 ``GRAY_050``（246/248/250），拇指是 ``GRAY_500``
        （140/149/159），两者都是实心填充，逐通道能稳定区分。

        拇指位置按应用的算法摆：``thumb_top = 轨道上沿 + (轨道高 - 拇指高) ×
        比例``，所以比例 0 时拇指贴着轨道顶端、比例 1 时贴着底端。
        """
        rows = [[(255, 255, 255)] * 400 for _ in range(height)]
        for y in range(rail_top, rail_bottom + 1):
            for x in range(thumb_left, thumb_right + 1):
                rows[y][x] = RAIL_RGB
        travel = (rail_bottom - rail_top + 1) - thumb_height
        thumb_top = rail_top + int(round(travel * fraction))
        for y in range(thumb_top, thumb_top + thumb_height):
            for x in range(thumb_left, thumb_right + 1):
                rows[y][x] = THUMB_RGB
        return ScrollTests._pixels(rows)

    def test_scroll_fraction_reads_the_thumb_position(self):
        # 位置必须能被量化：拇指停在顶端读出 0，停在底端读出 1。
        width, height, pixels = self._rail_image(fraction=0.0)
        self.assertAlmostEqual(0.0, scroll_fraction(pixels, width, height, 1.0), places=3)
        width, height, pixels = self._rail_image(fraction=1.0)
        self.assertAlmostEqual(1.0, scroll_fraction(pixels, width, height, 1.0), places=3)
        # 停在中间：拇指上沿应落在轨道可用行程的一半附近。
        width, height, pixels = self._rail_image(fraction=0.5)
        middle = scroll_fraction(pixels, width, height, 1.0)
        self.assertAlmostEqual(0.5, middle, places=2)

    def test_scroll_fraction_is_none_without_a_scrollbar(self):
        # 没有滚动条就没有位置证据，必须显式说「没有」，不能返回 0 冒充顶部。
        width, height, pixels = self._pixels([[(255, 255, 255)] * 400 for _ in range(200)])
        self.assertIsNone(scroll_fraction(pixels, width, height, 1.0))

    def test_code_band_detects_a_wide_shaded_block(self):
        # 末段围栏代码块渲染成整行浅底，是「已到文档末尾」的可见标记。
        rows = [[(255, 255, 255)] * 400 for _ in range(200)]
        for y in range(100, 160):
            for x in range(20, 300):
                rows[y][x] = RAIL_RGB
        width, height, pixels = self._pixels(rows)
        band = code_band(pixels, width, height, 1.0)
        self.assertIsNotNone(band)
        self.assertEqual((100, 159), band)
        # 没有浅底块时不得凭空报出一个带。
        width, height, pixels = self._pixels([[(255, 255, 255)] * 400 for _ in range(200)])
        self.assertIsNone(code_band(pixels, width, height, 1.0))

    def test_white_is_not_mistaken_for_the_scrollbar_rail(self):
        # 白色与轨道色在灰度下只差 9 级：按灰度匹配会把整片正文误认成轨道，
        # 位置读数随之失真。这里锁住「必须逐通道匹配」这一条。
        width, height, pixels = self._pixels([[(255, 255, 255)] * 400 for _ in range(200)])
        self.assertEqual([], [item for item in _column_bands(pixels, width, height, 1.0)
                              if item[0] == "rail"])

    def test_build_fixture_ends_with_a_unique_marker(self):
        # 夹具末尾必须是唯一的末段标记，且与其它段落文字不同。
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            data = build_fixture(Path(directory) / "a.md")
            self.assertTrue(data.rstrip().endswith(b"```"))
            self.assertEqual(1, data.count(END_MARKER))
            self.assertEqual(FIXTURE_SHA256, sha(data))

    def test_text_signature_distinguishes_different_visible_content(self):
        rows = [[255] * 400 for _ in range(200)]
        for y in range(80, 90):
            for x in range(20, 200):
                rows[y][x] = 0
        width, height, pixels = self._pixels(rows)
        first = text_signature(pixels, width, height, 1.0)
        self.assertTrue(first)
        # 同一画面重复读，指纹不变。
        self.assertEqual(first, text_signature(pixels, width, height, 1.0))
        # 文本带整体下移（滚动了），指纹必须变。
        moved = [[255] * 400 for _ in range(200)]
        for y in range(120, 130):
            for x in range(20, 200):
                moved[y][x] = 0
        mwidth, mheight, mpixels = self._pixels(moved)
        self.assertNotEqual(first, text_signature(mpixels, mwidth, mheight, 1.0))

    def test_text_signature_is_empty_for_blank_content(self):
        rows = [[255] * 400 for _ in range(200)]
        width, height, pixels = self._pixels(rows)
        self.assertEqual([], text_signature(pixels, width, height, 1.0))

    def test_build_fixture_is_deterministic_and_unique_per_section(self):
        # 夹具必须可重复生成且哈希固定：证据与夹具对不上时无从复核。
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            first = Path(directory) / "a.md"
            second = Path(directory) / "b.md"
            data = build_fixture(first)
            build_fixture(second)
            self.assertEqual(FIXTURE_SHA256, sha(data))
            self.assertEqual(data, second.read_bytes())
            self.assertEqual(1024 * 1024, len(data))
            # 编号唯一：任意两段的标题文本都不同。
            lines = data.decode("utf-8").splitlines()
            headings = [line for line in lines if line.startswith("## Section ")]
            self.assertGreater(len(headings), 1000)
            self.assertEqual(len(headings), len(set(headings)))

    def test_app_evidence_accepts_a_real_scroll_record(self):
        # 正例：完整的真实回调记录必须通过。
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            state = output / "snapshots" / "case-001"
            state.mkdir(parents=True)
            (state / "state.tsv").write_text(
                "has_open_block_before=false\thas_draft_before=false\thas_composition_before=false"
                "\tundo_depth_before=0\tredo_depth_before=0\thistory_len_before=0\n"
                "revision_before=0\trevision_after=0\thas_open_block=false\thas_draft=false"
                "\thas_composition=false\tundo_depth=0\tredo_depth=0\thistory_len=0"
                "\tsource_bytes_equal=true\n"
                "document_closed=true\tdisk_bytes_equal=true"
                "\tsteps=open,scroll-wheel,scroll-wheel,scroll-key,close\n",
                encoding="utf-8")
            self.assertEqual([], check_app_evidence(output))

    def test_app_evidence_rejects_missing_and_violating_records(self):
        # 反例：每条守卫都要有最小违规反例，证明它真的能拒绝。
        import tempfile

        valid = ("has_open_block_before=false\thas_draft_before=false\thas_composition_before=false"
                 "\tundo_depth_before=0\tredo_depth_before=0\thistory_len_before=0\n"
                 "revision_before=0\trevision_after=0\thas_open_block=false\thas_draft=false"
                 "\thas_composition=false\tundo_depth=0\tredo_depth=0\thistory_len=0"
                 "\tsource_bytes_equal=true\n"
                 "document_closed=true\tdisk_bytes_equal=true"
                 "\tsteps=open,scroll-wheel,scroll-key,close\n")
        cases = (
            ("没有落盘记录", None, "应用没有落盘任何回调记录"),
            ("缺滚动事件", valid.replace("open,scroll-wheel,scroll-key,close", "open,close"),
             "没有滚动事件"),
            ("缺键盘滚动事件", valid.replace("open,scroll-wheel,scroll-key,close",
                                           "open,scroll-wheel,close"),
             "没有键盘滚动事件"),
            ("出现草稿", valid.replace("has_draft=false", "has_draft=true"), "出现草稿"),
            ("历史不为空", valid.replace("history_len=0", "history_len=1"), "历史不为空"),
            ("文档未关闭", valid.replace("document_closed=true", "document_closed=false"),
             "未确认文档关闭"),
            ("磁盘字节不相等", valid.replace("disk_bytes_equal=true", "disk_bytes_equal=false"),
             "磁盘字节不相等"),
            ("事件序列超上限", valid.replace("open,scroll-wheel,scroll-key,close",
                                          "open,scroll-wheel,event-limit-exceeded,close"),
             "超出 256 条上限"),
        )
        for label, content, expected in cases:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as directory:
                output = Path(directory)
                if content is not None:
                    state = output / "snapshots" / "case-001"
                    state.mkdir(parents=True)
                    (state / "state.tsv").write_text(content, encoding="utf-8")
                failures = check_app_evidence(output)
                self.assertTrue(failures, "违规必须被拒绝：" + label)
                self.assertTrue(any(expected in item for item in failures),
                                "拒绝原因要指向具体违规：" + label)


if __name__ == "__main__":
    sys.exit(main())
