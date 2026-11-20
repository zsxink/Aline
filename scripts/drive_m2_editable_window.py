#!/usr/bin/env python3
"""M2 可编辑真实窗口取证驱动：打开→块编辑→直接文本插入/退格→中文 IME 组合
（确认/取消）→撤销/重做→选区/复制/粘贴→代码块横向滚动→模式往返→查找→
回到基线→关闭文档→关闭窗口。关闭后按采样表核对块内删除后块保持打开、后续
按键进入待定输入（修复前同位置为 `open_block=false`）。

只经 macOS 无障碍接口投递真实输入，不触碰应用内部状态；应用侧由
`ALINE_M2_EVIDENCE_DIR` 显式启用的采样器记录真实回调造成的状态。驱动只负责
把动作、返回、截图与无障碍树留在证据目录里。

中文组合依赖当前系统输入法；脚本在组合前后各记录一次输入源快照。无法确认
候选与提交时只记录实际观察，不臆造组合结果。

用法：drive_m2_editable_window.py --app-pid <pid> --fixture <路径> --evidence <证据目录>
"""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from PIL import Image

from evidence_paths import write_json, write_text
from validation_output import require_validation_path

ROOT = Path(__file__).resolve().parent.parent
ORCA = "/Applications/Orca.app/Contents/Resources/bin/orca"

OPEN_PANEL_KEY = "cmd+o"
GO_TO_FOLDER_KEY = "cmd+shift+g"
CLOSE_DOCUMENT_KEY = "cmd+w"
UNDO_KEY = "cmd+z"
REDO_KEY = "cmd+shift+z"
COPY_KEY = "cmd+c"
PASTE_KEY = "cmd+v"
FIND_KEY = "cmd+f"
SHIFT_LEFT_KEY = "shift+Left"

OPEN_BUTTON_LABEL = "打开 Markdown 文档…"
CONTENT_TOP = 70.0
CONTENT_BOTTOM = 630.0
# 代码块滚动容器（两行代码的可滚动区）在 960×672 窗口中的命中点：
# 容器上沿约在画面 y=493.5，高度 42，取中点保证指针落在容器内。
CODE_BLOCK_X = 440.0
CODE_BLOCK_Y = 514.0
SCROLL_INJECTOR_SOURCE = ROOT / "scripts" / "inject_scroll_wheel.swift"
STARTUP_TIMEOUT = 30.0
STARTUP_POLL = 0.2


class DriverError(RuntimeError):
    """真实输入未能按预期送达窗口。"""


class WindowDriver:
    def __init__(self, pid, evidence):
        self.pid = pid
        self.evidence = evidence
        self.serial = 0
        self._injector = None
        self._window_origin = None

    def call(self, args, note="", allow_missing_window=False):
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
        snapshot = payload.get("snapshot")
        if isinstance(snapshot, dict) and snapshot.get("treeText"):
            write_text(self.evidence / f"step-{self.serial:03d}.tree.txt",
                       snapshot["treeText"])
        return payload

    def probe(self):
        argv = [ORCA, "computer", "get-app-state", "--app", "pid:" + str(self.pid), "--json"]
        result = subprocess.run(argv, text=True, capture_output=True)
        try:
            value = json.loads(result.stdout)
        except json.JSONDecodeError:
            return False
        return bool(value.get("ok"))

    def wait_until_ready(self, timeout=STARTUP_TIMEOUT):
        started = time.monotonic()
        attempts = 0
        while time.monotonic() - started < timeout:
            attempts += 1
            if self.probe():
                write_json(self.evidence / "startup-wait.json",
                           {"attempts": attempts, "ready": True,
                            "waited_seconds": round(time.monotonic() - started, 3)})
                return
            time.sleep(STARTUP_POLL)
        raise DriverError("窗口未在期限内登记进无障碍树")

    def snapshot(self):
        snapshot = self.call(["get-app-state"])["snapshot"]
        window = snapshot.get("window") or {}
        if "x" in window and "y" in window:
            self._window_origin = (float(window["x"]), float(window["y"]))
        return snapshot

    def inject_scroll_wheel(self, frame_x, frame_y, delta_x, delta_y, note):
        """在窗口画面坐标处注入滚轮事件（屏幕坐标由窗口原点换算）。

        注入器编译自仓库内 ``scripts/inject_scroll_wheel.swift``，走系统事件
        流的真实 NSEvent 通道；命令记录只引用仓库相对脚本名，不写机器路径。
        """
        if self._window_origin is None:
            self.snapshot()
        if self._window_origin is None:
            raise DriverError("缺少窗口屏幕原点，无法注入滚轮")
        if self._injector is None:
            cache = Path(tempfile.mkdtemp(prefix="aline-scroll-injector-"))
            binary = cache / "inject_scroll_wheel"
            build = subprocess.run(
                ["swiftc", "-O", str(SCROLL_INJECTOR_SOURCE), "-o", str(binary)],
                text=True, capture_output=True)
            if build.returncode != 0:
                raise DriverError("滚动注入器编译失败：" + build.stderr[-800:])
            self._injector = binary
        screen_x = round(self._window_origin[0] + frame_x, 1)
        screen_y = round(self._window_origin[1] + frame_y, 1)
        self.serial += 1
        started = time.time_ns()
        result = subprocess.run(
            [str(self._injector), str(screen_x), str(screen_y),
             str(delta_x), str(delta_y)],
            text=True, capture_output=True)
        write_json(self.evidence / f"step-{self.serial:03d}.command.json", {
            "argv": ["scripts/inject_scroll_wheel.swift", "<screenX>", "<screenY>",
                     str(delta_x), str(delta_y)],
            "note": note,
            "exit_code": result.returncode,
            "started_epoch_ns": started,
            "finished_epoch_ns": time.time_ns(),
            "frame_point": [frame_x, frame_y],
            "screen_point": [screen_x, screen_y],
            "window_origin": [self._window_origin[0], self._window_origin[1]],
        })
        if result.stdout.strip():
            write_text(self.evidence / f"step-{self.serial:03d}.stdout", result.stdout)
        if result.returncode != 0:
            raise DriverError("滚轮注入失败：" + result.stderr[-600:])

    def title(self):
        return (self.snapshot().get("window") or {}).get("title") or ""

    def tree_text(self, snapshot):
        return snapshot.get("treeText", "")

    def find_button(self, snapshot, label):
        match = re.search(r'^\s*(\d+) button ' + re.escape(label) + r'\s*$',
                          self.tree_text(snapshot), re.M)
        return int(match.group(1)) if match else None

    def find_enabled_button(self, snapshot, label):
        match = re.search(r'^\s*(\d+) button ' + re.escape(label) + r'\s*$',
                          self.tree_text(snapshot), re.M)
        return int(match.group(1)) if match else None

    def find_row(self, snapshot, name):
        match = re.search(r'^\s*(\d+) text field \((?:selected, )?settable\), Value: '
                          + re.escape(name) + r'(?:, Secondary Actions: .*)?\s*$',
                          self.tree_text(snapshot), re.M)
        return int(match.group(1)) if match else None

    def find_panel_indices(self, snapshot):
        """从无障碍树读查找面板的输入框与无标签按钮。

        面板按钮在树中没有文字标签，按文档顺序识别：查询行里「查找」字段之后
        依次是上一项、下一项、区分大小写、关闭；「替换为」字段之后依次是
        替换当前、全部替换。Escape 只在焦点仍位于面板内时才会关闭面板
        （host.rs 的 close_focused_find 检查 find_contains_focus），点过替换
        按钮后焦点已回到窗口根，必须改用「关闭」按钮。
        """
        lines = self.tree_text(snapshot).splitlines()
        query = replacement = None
        stage = None
        query_buttons = []
        buttons_after_replacement = []
        for line in lines:
            match = re.match(r'^\s*(\d+) text field \(settable\) 查找(?:, Value: .*)?\s*$', line)
            if match:
                query = int(match.group(1))
                stage = "query"
                continue
            match = re.match(r'^\s*(\d+) text field \(settable\) 替换为(?:, Value: .*)?\s*$', line)
            if match:
                replacement = int(match.group(1))
                stage = "replacement"
                continue
            match = re.match(r'^\s*(\d+) button\s*$', line)
            if not match:
                continue
            if stage == "query":
                query_buttons.append(int(match.group(1)))
            elif stage == "replacement":
                buttons_after_replacement.append(int(match.group(1)))
        indices = {
            "query": query,
            "replacement": replacement,
            "previous": query_buttons[0] if len(query_buttons) > 0 else None,
            "next": query_buttons[1] if len(query_buttons) > 1 else None,
            "case": query_buttons[2] if len(query_buttons) > 2 else None,
            "close": query_buttons[3] if len(query_buttons) > 3 else None,
            "replace_current": buttons_after_replacement[0]
            if len(buttons_after_replacement) > 0 else None,
            "replace_all": buttons_after_replacement[1]
            if len(buttons_after_replacement) > 1 else None,
        }
        write_json(self.evidence / f"step-{self.serial:03d}.find-panel.json", indices)
        return indices

    def find_row_container(self, snapshot, name):
        """列视图行向无障碍树广告 `open` 辅助动作的是文本域外层的容器行；
        文本域自身执行 AXOpen 会失败。按缩进回溯定位最近的容器行。"""
        lines = self.tree_text(snapshot).splitlines()
        row = re.compile(r'^\s*(\d+) text field \((?:selected, )?settable\), Value: '
                         + re.escape(name) + r'(?:, Secondary Actions: .*)?\s*$')
        container = re.compile(r'^(\s*)(\d+) container\b')
        for index, line in enumerate(lines):
            if row.match(line):
                width = len(line) - len(line.lstrip())
                for previous in reversed(lines[max(0, index - 5):index]):
                    match = container.match(previous)
                    if match and len(match.group(1)) < width:
                        return int(match.group(2))
        return None

    def open_document(self, path):
        """打开面板 → 前往文件夹 → 粘贴绝对路径 → 定位夹具 → 打开。

        路径逐次核对：第一次回车后若面板仍停在列表且夹具未选中，先按 Right
        进入所选文件夹的下一列（列视图会选中首个文件）；仍未选中再点击无障碍
        树里的夹具行。「打开」按钮仅在未禁用时点击，否则回车。
        """
        self.call(["hotkey", "--key", OPEN_PANEL_KEY], "打开文件面板")
        time.sleep(1.2)
        self.call(["hotkey", "--key", GO_TO_FOLDER_KEY], "前往文件夹")
        time.sleep(1.2)
        self.call(["paste-text", "--text", str(path)], "粘贴夹具绝对路径")
        time.sleep(1.5)
        self.call(["press-key", "--key", "Return"], "确认路径")
        time.sleep(1.5)
        for _attempt in range(3):
            snapshot = self.snapshot()
            if Path(path).name in ((snapshot.get("window") or {}).get("title") or ""):
                return
            # 列视图的 `open` 辅助动作挂在行容器上；文本域执行 AXOpen 会报错。
            container = self.find_row_container(snapshot, Path(path).name)
            if container is not None:
                try:
                    self.call(["perform-secondary-action", "--element-index", str(container),
                               "--action", "open"], "通过行容器辅助动作打开")
                except DriverError as error:
                    write_text(self.evidence / "secondary-action-fallback.txt", str(error)[:600])
                time.sleep(1.2)
                if Path(path).name in self.title():
                    return
                self.call(["click", "--element-index", str(container)], "点击行容器打开")
                time.sleep(1.2)
                if Path(path).name in self.title():
                    return
            self.call(["press-key", "--key", "Right"], "列视图进入下一列")
            time.sleep(0.6)
            snapshot = self.snapshot()
            if Path(path).name in ((snapshot.get("window") or {}).get("title") or ""):
                return
            tree = self.tree_text(snapshot)
            row = self.find_row(snapshot, Path(path).name)
            selected = row is not None and re.search(
                r'^\s*' + str(row) + r' text field \(selected', tree, re.M)
            if row is not None and not selected:
                container = self.find_row_container(snapshot, Path(path).name)
                target = container if container is not None else row
                self.call(["click", "--element-index", str(target)], "选中夹具行")
                time.sleep(0.8)
                snapshot = self.snapshot()
            button = self.find_enabled_button(snapshot, OPEN_BUTTON_LABEL)
            if button is not None:
                self.call(["click", "--element-index", str(button)], "点击打开按钮")
            else:
                self.call(["press-key", "--key", "Return"], "回车打开")
            time.sleep(1.5)
            if Path(path).name in self.title():
                return
        raise DriverError("打开的文档与目标不符：" + self.title())

    def heading_point(self):
        shot = sorted(self.evidence.glob("step-*.png"))[-1]
        image = Image.open(shot).convert("L")
        width, height = image.size
        window = self.snapshot().get("window", {})
        scale = width / float(window["width"]) if window.get("width") else 1.0
        pixels = image.load()
        top = int(CONTENT_TOP * scale)
        bottom = min(height, int(CONTENT_BOTTOM * scale))
        band = None
        for y in range(top, bottom):
            count = sum(1 for x in range(0, width, 2) if pixels[x, y] < 140)
            if count > 0:
                band = [y, y] if band is None else [band[0], y]
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

    def click_body(self, note):
        point = self.heading_point()
        self.call(["click", "--x", str(round(point[0], 1)), "--y", str(round(point[1], 1))],
                  note)
        time.sleep(0.8)

    def type_keys(self, keys, note, pause=0.3):
        for key in keys:
            self.call(["press-key", "--key", key], note + "：" + key)
            time.sleep(pause)

    def backspace(self, note):
        try:
            self.call(["press-key", "--key", "Backspace"], note)
        except DriverError:
            self.call(["press-key", "--key", "Delete"], note + "：Delete 键名回退")

    def pixel_diff(self, first, second):
        """两帧截图的灰度差像素数；尺寸不同或无图时返回 null。"""
        try:
            from PIL import ImageChops
            left = Image.open(self.evidence / first).convert("L")
            right = Image.open(self.evidence / second).convert("L")
            if left.size != right.size:
                return None
            difference = ImageChops.difference(left, right)
            return sum(1 for value in difference.getdata() if value > 16)
        except OSError:
            return None

    def input_source(self):
        result = subprocess.run(
            ["defaults", "read", "com.apple.HIToolbox", "AppleSelectedInputSources"],
            text=True, capture_output=True)
        return result.stdout.strip() or result.stderr.strip()

    def close_window(self):
        snapshot = self.snapshot()
        match = re.search(r'^\s*(\d+) close button', self.tree_text(snapshot), re.M)
        if match is None:
            raise DriverError("没有找到窗口关闭按钮")
        self.call(["click", "--element-index", match.group(1)], "关闭窗口",
                  allow_missing_window=True)


def session_table(evidence):
    """读应用侧采样表；末尾 `truncated=` 行是完整性标记，不是数据行。"""
    path = Path(evidence) / "session.tsv"
    if not path.is_file():
        raise DriverError("缺少应用侧采样表 session.tsv")
    lines = path.read_text(encoding="utf-8").splitlines()
    if len(lines) < 2:
        raise DriverError("session.tsv 没有数据行")
    header = lines[0].split("\t")
    rows = []
    for line in lines[1:]:
        if line.startswith("truncated="):
            if line.split("=", 1)[1] == "true":
                raise DriverError("session.tsv 已达记录上限被截断，证据不完整")
            continue
        fields = line.split("\t")
        if len(fields) != len(header):
            raise DriverError("session.tsv 列数与表头不一致")
        rows.append(dict(zip(header, fields)))
    return rows


def assert_delete_keeps_block(evidence):
    """核对块内删除后块保持打开、随后的按键进入待定输入（关闭后采样已落盘）。

    块内删除若关闭块，后续输入就没有可重建的输入站点而被静默丢弃；修复前
    实现在同一位置记录 `open_block=false`。识别方式是相邻两条采样：前一条
    块打开、后一条文本长度少 3（一个中文字符）。
    """
    rows = session_table(evidence)
    for index in range(1, len(rows)):
        previous, current = rows[index - 1], rows[index]
        if previous["open_block"] != "true":
            continue
        if int(current["text_len"]) != int(previous["text_len"]) - 3:
            continue
        if current["open_block"] != "true":
            raise DriverError("块内退格后块被关闭：后续输入会失去输入站点")
        follow = rows[index + 1:index + 6]
        accepted = any(int(row["draft_len"] or 0) > 0
                       or row["composition_len"] not in ("", "0")
                       for row in follow)
        if not accepted:
            raise DriverError("删除后按键未进入待定输入")
        write_json(Path(evidence) / "delete-keeps-block.json", {
            "before": previous,
            "after": current,
            "after_typing": follow,
            "note": "块内删除后块保持打开并接受后续输入；修复前 open_block=false",
        })
        return
    raise DriverError("采样表未记录块内删除（块打开且文本长度减 3 的相邻记录）")


def run(driver, fixture):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    driver.open_document(fixture)
    driver.snapshot()

    # 段 A：块打开状态下的真实中文组合：拼音字母进入 marked 状态，空格确认首候选。
    # 注意：粘贴/撤销等命令会按设计关闭可视化块（跨块修改后必须重新解析），
    # 所以每段输入前都重新点击标题行打开块。
    driver.click_body("像素点击标题行，打开块编辑")
    driver.snapshot()
    driver.type_keys(["n", "i", "h", "a", "o"], "拼音组合输入", pause=0.35)
    driver.snapshot()
    driver.call(["press-key", "--key", "space"], "空格确认首个候选")
    time.sleep(0.8)
    driver.snapshot()

    # 段 B：撤销/重做命令（命令会关闭块），随后重开块验证 Escape 取消组合。
    driver.call(["hotkey", "--key", UNDO_KEY], "撤销中文提交")
    time.sleep(0.6)
    driver.snapshot()
    driver.call(["hotkey", "--key", REDO_KEY], "重做中文提交")
    time.sleep(0.6)
    driver.snapshot()
    driver.click_body("重开块验证取消组合")
    driver.type_keys(["s", "h", "i"], "再次拼音组合", pause=0.35)
    driver.snapshot()
    driver.call(["press-key", "--key", "Escape"], "Escape 取消组合")
    time.sleep(0.6)
    driver.snapshot()
    driver.call(["press-key", "--key", "Escape"], "无组合 Escape 离开块")
    time.sleep(0.6)
    driver.snapshot()

    # 段 C：选区与复制/粘贴。复制标题选区后先按 Right 收起选区，再在同一块
    # 粘贴：插入文本可观察且不依赖跨块点击的命中。粘贴作为命令关闭块，
    # 随后撤销并把文档还原。
    driver.click_body("再次点击标题行打开块")
    for _ in range(3):
        driver.call(["hotkey", "--key", SHIFT_LEFT_KEY], "Shift+Left 扩展选区")
        time.sleep(0.2)
    driver.snapshot()
    driver.call(["hotkey", "--key", COPY_KEY], "复制选区")
    time.sleep(0.3)
    driver.call(["press-key", "--key", "Right"], "Right 收起选区")
    time.sleep(0.3)
    driver.snapshot()
    driver.call(["hotkey", "--key", PASTE_KEY], "块内粘贴")
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["hotkey", "--key", UNDO_KEY], "撤销粘贴")
    time.sleep(0.5)
    driver.snapshot()

    # 段 D：块内退格删除一个字符。块内删除与普通输入一样必须留在打开块内；
    # 关闭块会让后续输入失去可重建的输入站点（修复前实测 open_block=false）。
    # 采样表只在关闭文档时落盘，判定放在关闭后的 assert_delete_keeps_block。
    driver.click_body("重新打开块执行退格")
    time.sleep(0.4)
    driver.backspace("退格删除一个字符")
    time.sleep(0.5)
    driver.snapshot()
    driver.type_keys(["a"], "删除后继续输入")
    time.sleep(0.5)
    driver.snapshot()
    driver.call(["press-key", "--key", "Escape"], "Escape 取消删除后的待定输入")
    time.sleep(0.4)
    driver.snapshot()

    # 段 E：代码块横向滚动。打开态按块源码折行渲染，没有横向位移；先按
    # Escape 离开块让代码块回到可视渲染（nowrap + overflow_x_scroll），
    # 再在代码块容器内向右、向左注入系统级像素滚轮事件，并跨截图做像素差对照。
    driver.call(["click", "--x", str(CODE_BLOCK_X), "--y", str(CODE_BLOCK_Y)],
                "点击代码块长行打开块")
    time.sleep(0.6)
    driver.call(["press-key", "--key", "Escape"], "离开块恢复代码块可视渲染")
    time.sleep(0.8)
    driver.snapshot()
    before = f"step-{driver.serial:03d}.png"
    driver.inject_scroll_wheel(CODE_BLOCK_X, CODE_BLOCK_Y, -300, 0, "代码块向右滚动")
    time.sleep(0.8)
    driver.snapshot()
    scrolled = f"step-{driver.serial:03d}.png"
    driver.inject_scroll_wheel(CODE_BLOCK_X, CODE_BLOCK_Y, 300, 0, "代码块向左滚动")
    time.sleep(0.8)
    driver.snapshot()
    restored = f"step-{driver.serial:03d}.png"
    write_json(driver.evidence / "scroll-proof.json", {
        "before": before,
        "after_right": scrolled,
        "after_left": restored,
        "diff_before_after_right": driver.pixel_diff(before, scrolled),
        "diff_after_right_after_left": driver.pixel_diff(scrolled, restored),
        "diff_before_after_left": driver.pixel_diff(before, restored),
        "input_path": "scripts/inject_scroll_wheel.swift (系统事件流像素滚轮)",
        "code_block_point": [CODE_BLOCK_X, CODE_BLOCK_Y],
    })

    # 段 F：两模式往返。
    snapshot = driver.snapshot()
    for label in ("源码", "可视化"):
        index = driver.find_button(snapshot, label)
        if index is None:
            write_text(driver.evidence / "mode-button-missing.txt",
                       driver.tree_text(snapshot))
            raise DriverError("没有找到模式按钮：" + label)
        driver.call(["click", "--element-index", str(index)], "切换到 " + label)
        time.sleep(0.8)
        snapshot = driver.snapshot()

    # 段 G：查找面板逐项跳转、当前项/计数显示、替换当前与全部替换。
    # 替换不经面板内撤销：最终统一按文档历史撤销回基线。
    driver.call(["hotkey", "--key", FIND_KEY], "打开查找面板")
    time.sleep(0.8)
    driver.call(["paste-text", "--text", "第二段"], "粘贴查找词")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Return"], "跳到首个匹配")
    time.sleep(0.6)
    snapshot = driver.snapshot()
    indices = driver.find_panel_indices(snapshot)
    if indices["replacement"] is None or indices["replace_current"] is None:
        raise DriverError("查找面板缺少替换输入框或替换按钮")
    driver.call(["click", "--element-index", str(indices["replacement"])],
                "聚焦替换输入框")
    time.sleep(0.3)
    driver.call(["paste-text", "--text", "第三段"], "粘贴替换词")
    time.sleep(0.4)
    driver.snapshot()
    driver.call(["click", "--element-index", str(indices["replace_current"])],
                "替换当前匹配")
    time.sleep(0.8)
    driver.snapshot()

    # 全部替换：清空查询词后改为「甲」，夹具第二段有两个「甲」。
    driver.call(["click", "--element-index", str(indices["query"])],
                "聚焦查找输入框")
    time.sleep(0.3)
    for _ in range(3):
        driver.call(["press-key", "--key", "Backspace"], "清空查找词：Backspace")
        time.sleep(0.15)
    driver.call(["paste-text", "--text", "甲"], "粘贴多匹配查找词")
    time.sleep(0.5)
    driver.call(["press-key", "--key", "Return"], "跳到首个匹配")
    time.sleep(0.6)
    driver.snapshot()
    driver.call(["click", "--element-index", str(indices["replace_all"])],
                "全部替换")
    time.sleep(0.8)
    driver.snapshot()

    # 段 H：关闭查找面板、撤销回基线，关闭文档写证据，关闭窗口。
    # 点过替换按钮后焦点已回到窗口根，此时 Escape 不再关闭面板；必须点面板
    # 自己的「关闭」按钮（关闭后会话把焦点还给打开面板前的持有者）。
    snapshot = driver.snapshot()
    indices = driver.find_panel_indices(snapshot)
    if indices["close"] is None:
        raise DriverError("查找面板缺少关闭按钮")
    driver.call(["click", "--element-index", str(indices["close"])], "关闭查找面板")
    time.sleep(0.6)
    snapshot = driver.snapshot()
    if "查找" in driver.tree_text(snapshot):
        raise DriverError("查找面板未关闭")
    driver.click_body("点击正文恢复文档焦点")
    for _ in range(14):
        driver.call(["hotkey", "--key", UNDO_KEY], "撤销回基线", allow_missing_window=True)
        time.sleep(0.3)
    # 自动保存去抖 1000ms：等最后一次撤销后的到期写盘先落地，关闭时
    # disk-after 才能反映还原后的基线。
    time.sleep(2.0)
    driver.snapshot()
    driver.call(["hotkey", "--key", CLOSE_DOCUMENT_KEY], "关闭文档并写证据",
                allow_missing_window=True)
    # 应用侧采样器（ALINE_M2_EVIDENCE_DIR）在文档关闭路径把证据写给同一目录；
    # 等文件落盘后再关窗口，否则窗口一关应用退出，后台写任务的产物会丢。
    expected = ("session.tsv", "final.tsv", "source-after.bin",
                "stored-after.bin", "disk-after.bin")
    deadline = time.monotonic() + 12.0
    while True:
        present = {name: (driver.evidence / name).is_file() for name in expected}
        if all(present.values()) or time.monotonic() >= deadline:
            break
        time.sleep(0.5)
    try:
        driver.snapshot()
    except DriverError as error:
        write_text(driver.evidence / "close-state.txt", str(error)[:600])
    write_text(driver.evidence / "input-source-after.txt", driver.input_source())
    driver.close_window()
    write_json(driver.evidence / "close-proof.json", {
        "expected": list(expected),
        "present": present,
        "note": "应用侧采样器在文档关闭路径落盘的文件（缺一即判本次运行失败）",
    })
    if not all(present.values()):
        raise DriverError("关闭文档后应用侧证据文件缺失：" + json.dumps(present))
    assert_delete_keeps_block(driver.evidence)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app-pid", type=int, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    evidence = require_validation_path(args.evidence)
    driver = WindowDriver(args.app_pid, evidence)
    try:
        run(driver, fixture.resolve())
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:2000])
        print("驱动失败：" + str(error)[:800], file=sys.stderr)
        return 1
    print("驱动完成，步数：" + str(driver.serial), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
