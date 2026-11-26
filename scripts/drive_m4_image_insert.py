#!/usr/bin/env python3
"""M4 3.2 正文图片生产插入真实窗口取证（命名文档）。

被测事实：
- 命名文档：源码视图 Cmd+A 选中全文后粘贴剪贴板 PNG，正文提交
  `![pasted](./assets/pasted.png)`；资产写入文档目录 `assets/`，字节与
  剪贴板一致；保存前磁盘文档保持基线字节。
- 撤销只撤销引用（IMAGE-08）：文本回到基线，已发布资产保留；重做恢复引用。
- 保存后磁盘文档等于引用文本，关闭时 source/disk 一致。

未命名文档插入（受检临时定位值、首次保存迁移、恢复与清理）没有生产 UI
入口——应用尚无「新建」命令——因此只由文档层测试覆盖；本脚本不宣称该
路径的真实窗口证据，只在摘要中如实记录。

输入只经 macOS 无障碍接口投递；剪贴板由 osascript 写入 PNG 数据，运行前
记录原文本剪贴板并在结束时恢复。应用侧采样器由 `ALINE_M2_EVIDENCE_DIR`
与 `ALINE_M1_READONLY_EVIDENCE_DIR` 显式启用，只在关闭路径落盘，故全部
判定在关闭后做。

用法：drive_m4_image_insert.py --evidence <证据目录>
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))

import drive_m2_editable_window as native  # noqa: E402  复用同一真实输入驱动
from drive_m2_find_modes import (  # noqa: E402
    activate, mode_button, require_fresh, system_events,
)
from drive_m2_interaction_matrix import close_document_and_wait  # noqa: E402
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_BINARY = native.ROOT / "target" / "debug" / "aline"
ORIGINAL_TEXT = "# 标题\n\n正文段落。\n"
REFERENCE_TEXT = "![pasted](./assets/pasted.png)"
PNG_SIZE = (64, 32)
PNG_COLOR = (200, 30, 200)


def _assert(condition, note):
    if not condition:
        raise native.DriverError(note)


def build_fixture(root):
    """确定性生成文档与剪贴板 PNG；返回文档路径与图片字节。"""
    document_dir = root / "doc"
    document_dir.mkdir(parents=True, exist_ok=True)
    document = document_dir / "notes.md"
    document.write_text(ORIGINAL_TEXT, encoding="utf-8")
    png = root / "clipboard.png"
    Image.new("RGB", PNG_SIZE, PNG_COLOR).save(png, format="PNG")
    return document, png.read_bytes()


def clipboard_text():
    result = subprocess.run(["pbpaste"], text=True, capture_output=True)
    return result.stdout if result.returncode == 0 else ""


def set_clipboard_png(path):
    script = ('set the clipboard to (read (POSIX file "%s") as «class PNGf»)' % path)
    result = subprocess.run(["osascript", "-e", script], text=True, capture_output=True)
    if result.returncode != 0:
        raise native.DriverError("写入 PNG 剪贴板失败：" + result.stderr[-400:])


def restore_clipboard(text):
    if text:
        subprocess.run(["pbcopy"], input=text, text=True, capture_output=True)
    else:
        subprocess.run(["osascript", "-e", "set the clipboard to \"\""],
                       text=True, capture_output=True)


def clipboard_png_readback(evidence):
    """用 AppleScript 把剪贴板 PNG 数据写回文件，独立确认写入生效。"""
    target = evidence / "clipboard-readback.png"
    script = (
        'set theData to the clipboard as «class PNGf»\n'
        'set f to open for access (POSIX file "%s") with write permission\n'
        'set eof f to 0\n'
        'write theData to f\n'
        'close access f' % target
    )
    result = subprocess.run(["osascript", "-e", script], text=True, capture_output=True)
    if result.returncode != 0:
        return None, result.stderr[-400:]
    return target.read_bytes(), None


def raise_panel(driver):
    """把文件面板提为 key window；System Events 的按键只发给 key window。"""
    driver.serial += 1
    started = time.time_ns()
    script = ('tell application "System Events" to tell process "aline" '
              'to perform action "AXRaise" of window "Open"')
    result = subprocess.run(["osascript", "-e", script], text=True, capture_output=True)
    write_json(driver.evidence / f"step-{driver.serial:03d}.command.json", {
        "argv": ["osascript", "-e", "<System Events: AXRaise Open>"],
        "note": "把文件面板提为 key window",
        "exit_code": result.returncode,
        "started_epoch_ns": started,
        "finished_epoch_ns": time.time_ns(),
    })
    time.sleep(0.3)


def close_residual_panel(driver):
    """关闭上一轮残留的文件面板：优先点面板自身的 Cancel，ESC 兜底。"""
    snapshot = driver.snapshot()
    index = driver.find_button(snapshot, "Cancel")
    if index is not None:
        driver.call(["click", "--element-index", str(index)], "关闭残留文件面板（Cancel）")
        time.sleep(1.0)
        return
    system_events(driver, "key code 53", "关闭残留文件面板（ESC）")
    time.sleep(1.0)


def wait_runtime_recovered(driver, attempts=4, pause=3.0):
    """Orca 运行时无响应时有限次等待其恢复，避免每轮再触发长超时。"""
    for _ in range(attempts):
        if driver.probe():
            return True
        time.sleep(pause)
    return False


def find_panel_path_field(driver, timeout=8.0):
    """轮询「前往文件夹」弹层的路径字段；返回 (索引或 None, 最后一次快照是否截断)。

    面板列视图会从根列开始列出整条祖先链；路径较深时列视图本身接近
    1200 节点快照上限，弹层整体被截掉（run-preview-5 复用轮：截图里弹层
    已打开且值正确，树里却只剩列视图）。快照的 truncation 元数据随调用
    返回，调用方据此区分「弹层没开」与「弹层被截掉」。
    """
    deadline = time.monotonic() + timeout
    truncated = False
    while time.monotonic() < deadline:
        snapshot = driver.snapshot()
        truncated = bool((snapshot.get("truncation") or {}).get("truncated"))
        match = re.search(r'^\s*(\d+) text field \(settable\), Value: /',
                          driver.tree_text(snapshot), re.M)
        if match:
            return int(match.group(1)), truncated
        time.sleep(0.2)
    return None, truncated


def shrink_panel_listing(driver):
    """快照被截断时把面板导航到浅目录，让弹层重新落进快照上限。

    先 ESC 关闭弹层（弹层有焦点时只关弹层），再点边栏「文稿」；返回面板
    是否仍然打开（ESC 若把面板也关了则返回 False，交由外层重试）。
    """
    system_events(driver, "key code 53", "关闭前往文件夹弹层（快照截断）")
    time.sleep(1.0)
    if driver.title() != "Open":
        return False
    snapshot = driver.snapshot()
    match = re.search(r'^\s*(\d+) cell (?:Documents|文稿)(?:,|$)',
                      driver.tree_text(snapshot), re.M)
    if match is None:
        return False
    driver.call(["click", "--element-index", match.group(1)],
                "点击边栏「文稿」缩小面板列视图")
    time.sleep(1.5)
    return driver.title() == "Open"


def open_document_robust(driver, document, rounds=3):
    """经文件面板打开夹具；面板偶发不响应「前往文件夹」或不上报选中时重试。

    打开机制本身不是被测行为，但每一轮的观测都写入证据目录；面板行容器
    辅助动作与点击是共享驱动里已有的回退手段。面板是系统面板服务的远程
    视图，无障碍快照可能截断行容器，此时退回「点选同名文本域 + 回车」；
    单轮自动化异常记为 round_error，关闭残留面板后进入下一轮。
    """
    target = str(document.resolve())
    attempts = []
    for round_index in range(1, rounds + 1):
        record = {"round": round_index}
        attempts.append(record)
        try:
            if driver.title() == "Open":
                close_residual_panel(driver)
                if driver.title() == "Open":
                    record["panel_still_open"] = True
                    continue
            activate(driver)
            driver.call(["hotkey", "--key", native.OPEN_PANEL_KEY], "打开文件面板")
            deadline = time.monotonic() + 6.0
            while time.monotonic() < deadline and driver.title() != "Open":
                time.sleep(0.2)
            record["panel_opened"] = driver.title() == "Open"
            if not record["panel_opened"]:
                continue
            time.sleep(0.4)
            raise_panel(driver)
            system_events(driver, 'keystroke "g" using {command down, shift down}',
                          "打开前往文件夹")
            field, truncated = find_panel_path_field(driver)
            if field is None and truncated:
                record["panel_shrunk"] = shrink_panel_listing(driver)
                if record["panel_shrunk"]:
                    raise_panel(driver)
                    system_events(driver, 'keystroke "g" using {command down, shift down}',
                                  "重新打开前往文件夹")
                    field, truncated = find_panel_path_field(driver)
            record["path_field"] = field is not None
            if field is None:
                continue
            driver.call(["set-value", "--element-index", str(field), "--value", target],
                        "写入夹具绝对路径")
            time.sleep(0.4)
            system_events(driver, "key code 36", "回车确认路径")
            container = None
            snapshot = None
            deadline = time.monotonic() + 6.0
            while time.monotonic() < deadline:
                snapshot = driver.snapshot()
                container = driver.find_row_container(snapshot, document.name)
                if container is not None:
                    break
                time.sleep(0.2)
            record["row_container"] = container
            if container is None:
                named = (driver.find_named_field(snapshot, document.name)
                         if snapshot is not None else None)
                record["named_field"] = named
                if named is not None:
                    record["opened_by"] = "field-click"
                    driver.call(["click", "--element-index", str(named)],
                                "点选目标文件行")
                    time.sleep(0.8)
                else:
                    record["opened_by"] = "enter-after-path"
                system_events(driver, "key code 36", "回车打开目标文件")
                time.sleep(1.5)
                if driver.title() == document.name:
                    break
                continue
            record["opened_by"] = "secondary-action"
            try:
                driver.call(["perform-secondary-action", "--element-index", str(container),
                             "--action", "open"], "行容器 open 辅助动作")
            except native.DriverError as error:
                record["secondary_action_error"] = str(error)[:300]
            time.sleep(1.5)
            if driver.title() == document.name:
                break
            record["opened_by"] = "row-click"
            driver.call(["click", "--element-index", str(container)], "点击行容器打开")
            time.sleep(1.5)
            if driver.title() == document.name:
                break
            record["opened_by"] = "enter"
            system_events(driver, "key code 36", "回车打开所选夹具")
            time.sleep(1.5)
            if driver.title() == document.name:
                break
        except native.DriverError as error:
            record["round_error"] = str(error)[:500]
            if "runtime_timeout" in record["round_error"]:
                record["runtime_recovered"] = wait_runtime_recovered(driver)
                if not record["runtime_recovered"]:
                    raise
            else:
                try:
                    close_residual_panel(driver)
                except native.DriverError:
                    pass
            time.sleep(1.0)
    else:
        write_json(driver.evidence / "open-panel.json", {
            "attempts": attempts,
            "note": "文件面板多轮重试后仍未打开夹具",
        })
        raise native.DriverError("文件面板重试后仍未打开夹具")
    write_json(driver.evidence / "open-panel.json", {
        "attempts": attempts,
        "note": "文件面板打开夹具的尝试记录",
    })


def session_rows(evidence):
    rows = native.session_table(evidence)
    decoded = []
    for row in rows:
        text = bytes.fromhex(row["text_hex"]).decode("utf-8")
        decoded.append({
            "seq": int(row["seq"]),
            "mode": row["mode"],
            "text": text,
            "undo_depth": int(row["undo_depth"]),
            "redo_depth": int(row["redo_depth"]),
            "open_block": row["open_block"],
        })
    return decoded


def distinct_texts(rows):
    sequence = []
    for row in rows:
        if not sequence or sequence[-1] != row["text"]:
            sequence.append(row["text"])
    return sequence


def run(driver, document, png_path):
    driver.wait_until_ready()
    write_text(driver.evidence / "input-source-before.txt", driver.input_source())
    open_document_robust(driver, document)
    # 打开是异步读取：标题先变、视图后接管；轮询模式按钮出现再继续。
    deadline = time.monotonic() + 10.0
    view_ready = False
    while time.monotonic() < deadline:
        tree = driver.tree_text(driver.snapshot())
        if "可视化" in tree and "源码" in tree:
            view_ready = True
            break
        time.sleep(0.3)
    write_json(driver.evidence / "document-view-wait.json", {
        "ready": view_ready,
        "note": "打开后轮询模式按钮出现，确认文档视图已接管窗口",
    })
    _assert(view_ready, "打开后文档视图没有出现")
    time.sleep(1.0)
    write_text(driver.evidence / "window-title-after-open.txt", driver.title())

    # 源码视图：点击正文定位光标，再用 Cmd+A 选中全文，粘贴图片替换整个选区。
    mode_button(driver, "源码")
    time.sleep(0.6)
    driver.click_body("源码视图点击正文定位光标")
    driver.call(["hotkey", "--key", "cmd+a"], "选中全文")
    time.sleep(0.6)

    set_clipboard_png(png_path)
    source_bytes = png_path.read_bytes()
    readback, error = clipboard_png_readback(driver.evidence)
    write_json(driver.evidence / "clipboard-write.json", {
        "png_bytes": len(source_bytes),
        "clipboard_png_readable": readback is not None,
        "clipboard_bytes": len(readback) if readback is not None else None,
        "equal_to_source": readback == source_bytes,
        "error": error,
        "note": "osascript 写入 PNG 后把剪贴板 PNG 数据写回文件读回，逐字节确认剪贴板确为图片数据",
    })
    _assert(readback is not None, "剪贴板没有 PNG 数据：" + str(error))
    _assert(readback == source_bytes, "剪贴板 PNG 与源文件不一致")

    activate(driver)
    driver.call(["hotkey", "--key", "cmd+v"], "粘贴剪贴板图片")
    time.sleep(1.2)
    driver.call(["hotkey", "--key", "cmd+z"], "撤销插入")
    time.sleep(1.2)
    driver.call(["hotkey", "--key", "cmd+shift+z"], "重做插入")
    time.sleep(1.2)
    driver.call(["hotkey", "--key", "cmd+s"], "保存文档")
    # 保存异步落盘：等磁盘文档变成引用文本再继续，避免关闭时仍在写。
    deadline = time.monotonic() + 10.0
    saved = False
    while time.monotonic() < deadline:
        if document.read_bytes() == REFERENCE_TEXT.encode("utf-8"):
            saved = True
            break
        time.sleep(0.3)
    write_json(driver.evidence / "save-wait.json", {
        "saved": saved,
        "note": "Cmd+S 后轮询磁盘文档等于引用文本",
    })
    _assert(saved, "保存后磁盘文档没有变成引用文本")

    asset = document.parent / "assets" / "pasted.png"
    write_json(driver.evidence / "asset-after-save.json", {
        "path": str(asset),
        "exists": asset.is_file(),
        "bytes": asset.stat().st_size if asset.is_file() else None,
        "note": "撤销/重做与保存之后，已发布资产应仍在文档目录 assets/ 下",
    })
    _assert(asset.is_file(), "保存后资产文件缺失")


def verify(evidence, document, png_bytes):
    rows = session_rows(evidence)
    sequence = distinct_texts(rows)
    write_json(evidence / "text-sequence.json", {
        "distinct_texts": sequence,
        "note": "关闭后按采样表还原的不同文本序列：基线→引用→撤销→重做→保存",
    })
    expected = [ORIGINAL_TEXT, REFERENCE_TEXT, ORIGINAL_TEXT, REFERENCE_TEXT]
    _assert(sequence == expected,
            "文本序列与预期不符：" + json.dumps(sequence, ensure_ascii=False))
    for text in sequence:
        _assert("aline-temp:" not in text, "命名文档不应出现临时定位引用")

    undo_rows = [row for row in rows if row["text"] == ORIGINAL_TEXT
                 and row["undo_depth"] == 0 and row["redo_depth"] > 0]
    redo_rows = [row for row in rows if row["text"] == REFERENCE_TEXT
                 and row["redo_depth"] == 0 and row["undo_depth"] > 0]
    write_json(evidence / "history.json", {
        "undo_rows": len(undo_rows),
        "redo_rows": len(redo_rows),
        "note": "撤销后文本回基线且 redo 可用；重做后引用恢复且 undo 可用",
    })
    _assert(undo_rows, "撤销状态没有被采样到")
    _assert(redo_rows, "重做状态没有被采样到")

    asset = document.parent / "assets" / "pasted.png"
    _assert(asset.is_file(), "撤销/重做后资产文件缺失")
    asset_bytes = asset.read_bytes()
    write_json(evidence / "asset-bytes.json", {
        "size": len(asset_bytes),
        "sha256": hashlib.sha256(asset_bytes).hexdigest(),
        "clipboard_sha256": hashlib.sha256(png_bytes).hexdigest(),
        "equal": asset_bytes == png_bytes,
        "note": "资产必须与写入剪贴板的 PNG 逐字节一致（IMAGE-02）",
    })
    _assert(asset_bytes == png_bytes, "资产与剪贴板 PNG 不一致")

    disk = document.read_bytes()
    write_json(evidence / "disk-after-save.json", {
        "size": len(disk),
        "sha256": hashlib.sha256(disk).hexdigest(),
        "expected": REFERENCE_TEXT,
        "equal": disk == REFERENCE_TEXT.encode("utf-8"),
        "note": "保存后磁盘文档等于引用文本",
    })
    _assert(disk == REFERENCE_TEXT.encode("utf-8"),
            "保存后磁盘文档不是引用文本：" + disk.decode("utf-8", "replace"))

    final = (evidence / "final.tsv").read_text(encoding="utf-8")
    write_text(evidence / "final-state.txt", final)
    _assert("disk_equals_source=true" in final, "关闭时源码与磁盘不一致")
    _assert("disk_equals_stored=true" in final, "关闭时基线存储与磁盘不一致")

    case_state = (evidence / "case-001" / "state.tsv").read_text(encoding="utf-8")
    write_text(evidence / "case-state.txt", case_state)
    _assert("document_closed=true" in case_state, "只读取证未确认文档关闭")
    # M1 的 source_bytes_equal/disk_bytes_equal 比较的是打开时基线；本文档被编辑并
    # 保存，二者应为 false——它们恰好证明比较不是空转。关闭后的真实字节另列断言。
    _assert("source_bytes_equal=false" in case_state, "编辑后源码不应等于打开时基线")
    _assert("disk_bytes_equal=false" in case_state, "保存后磁盘不应等于打开时基线")
    reference_bytes = REFERENCE_TEXT.encode("utf-8")
    for name in ("source-after.bin", "stored.bin", "disk-after.bin"):
        case_bytes = (evidence / "case-001" / name).read_bytes()
        write_json(evidence / ("case-" + name.replace(".bin", ".json")), {
            "name": name,
            "size": len(case_bytes),
            "sha256": hashlib.sha256(case_bytes).hexdigest(),
            "equal_reference": case_bytes == reference_bytes,
            "note": "关闭后 M1 取证字节应等于引用文本",
        })
        _assert(case_bytes == reference_bytes,
                "关闭后取证字节不是引用文本：" + name)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             text=True, capture_output=True)
    if running.stdout.strip():
        print("已有 aline 进程占用，拒绝干扰：" + running.stdout.strip(), file=sys.stderr)
        return 2
    if not APP_BINARY.is_file():
        print("缺少 target/debug/aline", file=sys.stderr)
        return 2
    require_fresh(evidence)

    fixture_root = Path(tempfile.mkdtemp(prefix="aline-m4-image-insert-"))
    document, png_bytes = build_fixture(fixture_root)
    fixture_copy = evidence / "fixture"
    fixture_copy.mkdir(exist_ok=True)
    shutil.copyfile(str(document), str(fixture_copy / document.name))
    (fixture_copy / "clipboard.png").write_bytes(png_bytes)
    write_json(evidence / "fixture.json", {
        "document": document.name,
        "original_text": ORIGINAL_TEXT,
        "reference_text": REFERENCE_TEXT,
        "png_size": list(PNG_SIZE),
        "png_bytes": len(png_bytes),
        "note": "夹具在系统临时目录生成；证据目录保留同一份副本",
    })

    original_clipboard = clipboard_text()
    env = dict(os.environ)
    env["ALINE_M2_EVIDENCE_DIR"] = str(evidence)
    env["ALINE_M1_READONLY_EVIDENCE_DIR"] = str(evidence)
    # 应用输出落文件而不是管道：无人读取的管道写满会阻塞应用主线程。
    app_stdout = open(evidence / "app-stdout.txt", "wb")
    app_stderr = open(evidence / "app-stderr.txt", "wb")
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(native.ROOT), env=env,
                            stdout=app_stdout, stderr=app_stderr)
    write_json(evidence / "launch.json", {
        "pid": proc.pid,
        "binary": "target/debug/aline",
        "env": {"ALINE_M2_EVIDENCE_DIR": "<evidence>",
                "ALINE_M1_READONLY_EVIDENCE_DIR": "<evidence>"},
        "note": "不带文档参数启动；证据目录由环境变量显式启用；应用输出写入 app-stdout.txt/app-stderr.txt",
    })
    try:
        driver = native.WindowDriver(proc.pid, evidence)
        run(driver, document, fixture_root / "clipboard.png")
        close_document_and_wait(driver)
        verify(evidence, document, png_bytes)
        write_json(evidence / "result.json", {
            "status": "通过",
            "facts": [
                "命名文档粘贴 PNG 提交相对资产引用",
                "资产与剪贴板字节一致",
                "撤销只撤销引用且资产保留；重做恢复引用",
                "保存后磁盘文档等于引用文本",
            ],
            "limits": [
                "未命名文档插入无生产 UI 入口（应用尚无新建命令），只由文档层测试覆盖",
                "拖放适配器只由单元测试覆盖；真实 Finder 拖拽未自动化",
                "绝对图片目录设置无设置页入口，只由文档层测试覆盖",
            ],
        })
    finally:
        restore_clipboard(original_clipboard)
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        app_stdout.close()
        app_stderr.close()
        shutil.rmtree(fixture_root, ignore_errors=True)
    print("通过：证据目录 " + str(evidence))
    return 0


if __name__ == "__main__":
    sys.exit(main())
