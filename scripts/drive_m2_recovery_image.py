#!/usr/bin/env python3
"""W4 5.2 真实窗口图片恢复取证：播种内嵌 PNG 完整代 → 扫描 → 恢复 → 解码。

恢复记录只由 crate 内 `#[ignore]` 播种入口用生产恢复编码器写入一个隔离的
临时 HOME；应用进程仍走真实启动扫描、真实恢复面板点击与真实资源物化路径，
不新增生产测试钩子。取证步骤：

1. 在隔离 HOME 的 `Library/Application Support/Aline/recovery` 发布一条
   含 1x1 PNG 资源的完整代，源夹具只提供原路径，不参与编码。
2. 以该 HOME 启动真实 aline；启动扫描应逐文档呈现「恢复/丢弃」。
3. 点击「恢复」：文档接管窗口；图片应物化到临时根，并可由 PIL 独立解码，
   源夹具字节不变。
4. 恢复文档按窗口根焦点回归路径关闭（激活后首次 cmd+w）；关闭后条目只
   提供丢弃，临时图片由资源清理路径删除。
5. 丢弃并二次确认：恢复代退役清理；源夹具始终不变。

证据只记相对路径或 `<tmp>`/`<repo>` 占位。用法：
  drive_m2_recovery_image.py --evidence <证据目录>
"""
import argparse
import hashlib
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

from drive_m2_editable_window import (  # noqa: E402  复用同一真实输入驱动
    DriverError,
    ROOT,
    WindowDriver,
    session_table,
)
from drive_m2_recovery_crash import (  # noqa: E402
    activate,
    close_recovered_document,
    fixture_state,
    panel_buttons,
    require_button,
    settle_logs,
)
from evidence_paths import write_json, write_text  # noqa: E402
from validation_output import require_validation_path  # noqa: E402

APP_BINARY = ROOT / "target" / "debug" / "aline"
SEED_TEST = "seed_recovery_generation_with_embedded_png_for_real_window"
CLOSE_FILES = ("session.tsv", "final.tsv", "source-after.bin",
               "stored-after.bin", "disk-after.bin")
LOCATOR_RE = re.compile(
    r"^aline-temp:([0-9a-f]{16})/([0-9a-f]{16})\.([a-z0-9]{1,8})$")
FIXTURE_TEXT = "# 原文件\n\n原文件保持不变。\n"


def sanitize(text, home, fixture):
    """把机器路径替换为占位符；证据与日志不得写绝对本地路径。"""
    text = text.replace(str(ROOT), "<repo>")
    text = text.replace(str(home), "<tmp-home>")
    text = text.replace(str(fixture), fixture.name)
    text = text.replace(tempfile.gettempdir(), "<tmp>")
    return text


def parse_seed_output(output):
    for line in output.splitlines():
        marker = "ALINE-RECOVERY-SEED "
        if marker not in line:
            continue
        fields = {}
        for token in line.split(marker, 1)[1].split():
            key, _sep, value = token.partition("=")
            fields[key] = value
        if {"document", "generation", "locator"} <= fields.keys():
            return fields
    raise DriverError("播种测试没有输出 ALINE-RECOVERY-SEED 记录")


def seed_record(home, fixture, evidence):
    env = os.environ.copy()
    env["ALINE_RECOVERY_SEED_HOME"] = str(home)
    env["ALINE_RECOVERY_SEED_TARGET"] = str(fixture)
    result = subprocess.run(
        ["cargo", "test", "-p", "aline-ui", "--lib", SEED_TEST, "--",
         "--ignored", "--nocapture", "--test-threads=1"],
        cwd=str(ROOT), env=env, text=True, capture_output=True)
    write_text(evidence / "seed.log",
               sanitize(result.stdout + result.stderr, home, fixture))
    if result.returncode != 0:
        raise DriverError("播种测试失败：" + result.stderr[-800:])
    info = parse_seed_output(result.stdout + result.stderr)
    source_path = (home / "Library" / "Application Support" / "Aline"
                   / "recovery" / "v1" / "active" / info["document"]
                   / "g" / info["generation"] / "source.utf8")
    if not source_path.is_file():
        raise DriverError("播种后缺少 source.utf8：" + info["document"])
    source = source_path.read_bytes()
    if info["locator"].encode() not in source:
        raise DriverError("播种 source 不含 locator：" + info["locator"])
    resource_root = source_path.parent / "r"
    resource_files = sorted(
        path for path in resource_root.rglob("*") if path.is_file())
    if len(resource_files) != 1:
        raise DriverError("播种代应只有一条资源文件：" + str(resource_files))
    png = resource_files[0].read_bytes()
    with Image.open(resource_files[0]) as image:
        image.load()
        seeded_image = {
            "format": image.format,
            "dimensions": list(image.size),
            "mode": image.mode,
        }
    if seeded_image["format"] != "PNG" \
            or seeded_image["dimensions"] != [1, 1]:
        raise DriverError("播种资源不是 1x1 PNG：" + str(seeded_image))
    write_json(evidence / "seed-summary.json", {
        "document": info["document"],
        "generation": info["generation"],
        "locator": info["locator"],
        "target_name": fixture.name,
        "source_size": len(source),
        "source_sha256": hashlib.sha256(source).hexdigest(),
        "png_size": len(png),
        "png_sha256": hashlib.sha256(png).hexdigest(),
        "png_format": seeded_image["format"],
        "png_dimensions": seeded_image["dimensions"],
        "note": "记录由 crate 内 #[ignore] 播种入口经生产恢复编码器写入隔离"
                " HOME；仅资源来自测试内嵌字节，应用未提供图片插入入口",
    })
    return info, source, png


def store_snapshot(home):
    """隔离 HOME 下恢复存储的只读快照；键为相对 recovery/v1 的路径。"""
    root = (home / "Library" / "Application Support" / "Aline"
            / "recovery" / "v1")
    entries = []
    if root.is_dir():
        for base, _dirs, files in os.walk(root):
            for name in files:
                path = Path(base) / name
                try:
                    stat = path.stat()
                    data = path.read_bytes()
                except OSError as error:
                    entries.append({
                        "path": path.relative_to(root).as_posix(),
                        "error": type(error).__name__})
                    continue
                entries.append({
                    "path": path.relative_to(root).as_posix(),
                    "size": stat.st_size,
                    "sha256": hashlib.sha256(data).hexdigest(),
                })
    entries.sort(key=lambda entry: entry["path"])
    generations = []
    active = root / "active"
    if active.is_dir():
        for lineage in sorted(active.iterdir()):
            groups = lineage / "g"
            if not groups.is_dir():
                continue
            for generation in sorted(groups.iterdir()):
                if not generation.is_dir():
                    continue
                manifest = generation / "manifest"
                source = generation / "source.utf8"
                generations.append({
                    "document": lineage.name,
                    "generation": generation.name,
                    "manifest": manifest.is_file(),
                    "source_bytes": source.stat().st_size
                    if source.is_file() else None,
                    "source_sha256": hashlib.sha256(
                        source.read_bytes()).hexdigest()
                    if source.is_file() else None,
                })
    return {"files": entries, "generations": generations}


def launch(evidence, home):
    log_dir = Path(tempfile.mkdtemp(prefix="aline-recovery-image-run-"))
    stdout = open(log_dir / "app.stdout", "wb")
    stderr = open(log_dir / "app.stderr", "wb")
    env = os.environ.copy()
    env["HOME"] = str(home)
    env["ALINE_M2_EVIDENCE_DIR"] = str(evidence)
    proc = subprocess.Popen([str(APP_BINARY)], cwd=str(ROOT), env=env,
                            stdout=stdout, stderr=stderr,
                            start_new_session=True)
    write_text(evidence / "app.pid", str(proc.pid) + "\n")
    write_json(evidence / "launch.json", {
        "pid": proc.pid,
        "started_epoch_ns": time.time_ns(),
        "binary": "target/debug/aline",
        "cwd": "<repo>",
        "home": "<tmp-home>",
        "note": "真实 aline 进程；HOME 指向隔离临时目录，证据目录只由 "
                "ALINE_M2_EVIDENCE_DIR 注入",
    })
    return proc, stdout, stderr, log_dir


def wait_files(evidence, names, timeout):
    deadline = time.monotonic() + timeout
    present = {}
    while True:
        present = {name: (Path(evidence) / name).is_file()
                   for name in names}
        if all(present.values()) or time.monotonic() >= deadline:
            return present
        time.sleep(0.4)


def wait_until_buttons(driver, labels, timeout):
    deadline = time.monotonic() + timeout
    snapshot = driver.snapshot()
    buttons = panel_buttons(driver, snapshot)
    while (any(buttons[label] is None for label in labels)
           and time.monotonic() < deadline):
        time.sleep(0.5)
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
    return snapshot, buttons


def materialized_path(home, locator):
    match = LOCATOR_RE.match(locator)
    if match is None:
        raise DriverError("locator 无法解析：" + locator)
    document, resource, extension = match.groups()
    return (home / "Library" / "Application Support" / "Aline"
            / "temporary" / document / (resource + "." + extension))


def wait_materialized(path, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            return True
        time.sleep(0.2)
    return path.is_file()


def image_state(path):
    """独立读取物化图片：字节摘要 + PIL 解码出的格式、尺寸与首像素。"""
    if not path.is_file():
        return None
    data = path.read_bytes()
    with Image.open(path) as image:
        image.load()
        state = {
            "size_bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
            "format": image.format,
            "dimensions": list(image.size),
            "mode": image.mode,
            "first_pixel_rgba": list(image.convert("RGBA").getpixel((0, 0))),
        }
    return state


def verify_close_samples(evidence, seed_source, fixture_bytes):
    """关闭后核对采样包：内存 source 等于恢复记录，磁盘仍是原夹具。"""
    rows = session_table(evidence)
    expected_hex = seed_source.hex()
    matched = [row for row in rows
               if row.get("text_len") == str(len(seed_source))
               and row.get("text_hex", "").lower() == expected_hex]
    source_after = (Path(evidence) / "source-after.bin").read_bytes()
    stored_after = (Path(evidence) / "stored-after.bin").read_bytes()
    disk_after = (Path(evidence) / "disk-after.bin").read_bytes()
    final_tsv = (Path(evidence) / "final.tsv").read_text(encoding="utf-8")
    write_json(evidence / "restored-source-sample.json", {
        "matched": bool(matched),
        "rows": len(rows),
        "text_len": len(seed_source),
        "text_sha256": hashlib.sha256(seed_source).hexdigest(),
        "mode": matched[-1].get("mode") if matched else None,
        "undo_depth": matched[-1].get("undo_depth") if matched else None,
        "source_after_sha256": hashlib.sha256(source_after).hexdigest(),
        "stored_after_sha256": hashlib.sha256(stored_after).hexdigest(),
        "disk_after_sha256": hashlib.sha256(disk_after).hexdigest(),
        "final_tsv": final_tsv,
        "note": "采样只在关闭时落盘；内存 source 应等于恢复记录，"
                "disk-after 应仍是未改写的源夹具",
    })
    if not matched:
        raise DriverError("采样表未出现恢复记录的 source 文本")
    if source_after != seed_source or stored_after != seed_source:
        raise DriverError("关闭采样 source/stored 与恢复记录不一致")
    if disk_after != fixture_bytes:
        raise DriverError("关闭采样 disk-after 不是原夹具字节")
    if f"source_len={len(seed_source)}" not in final_tsv:
        raise DriverError("final.tsv 的 source_len 与恢复记录不一致")
    if f"disk_len={len(fixture_bytes)}" not in final_tsv:
        raise DriverError("final.tsv 的 disk_len 与原夹具不一致")
    if "disk_equals_source=false" not in final_tsv:
        raise DriverError("final.tsv 未记录磁盘与恢复文本的差异")


def run(evidence):
    work = Path(tempfile.mkdtemp(prefix="aline-recovery-image-work-"))
    home = work / "home"
    (home / "Library" / "Application Support").mkdir(parents=True)
    fixture = work / "origin.md"
    fixture.write_bytes(FIXTURE_TEXT.encode())
    fixture_before = fixture_state(fixture)
    write_json(evidence / "fixture-before.json", {
        "name": fixture.name,
        "size": fixture_before["size"],
        "sha256": fixture_before["sha256"],
        "note": "源夹具只被记录为恢复代的原路径；场景不得改写它",
    })
    info, seed_source, seed_png = seed_record(home, fixture, evidence)
    proc, stdout, stderr, log_dir = launch(evidence, home)
    try:
        driver = WindowDriver(proc.pid, evidence)
        driver.wait_until_ready()
        activate(driver)
        snapshot, buttons = wait_until_buttons(driver, ("恢复", "丢弃"), 15.0)
        startup_store = store_snapshot(home)
        write_json(evidence / "startup-panel.json", {
            "buttons": buttons,
            "title": (snapshot.get("window") or {}).get("title"),
            "store": startup_store,
            "note": "隔离 HOME 的启动扫描应只呈现播种的完整代",
        })
        if buttons["恢复"] is None or buttons["丢弃"] is None:
            raise DriverError("启动扫描没有呈现恢复面板：" + str(buttons))
        if len(startup_store["generations"]) != 1:
            raise DriverError("启动扫描谱系数应为 1："
                              + str(startup_store["generations"]))
        generation = startup_store["generations"][0]
        if generation["source_sha256"] != hashlib.sha256(
                seed_source).hexdigest() or not generation["manifest"]:
            raise DriverError("启动扫描完整代与播种记录不符：" + str(generation))

        restore = require_button(driver, snapshot, "恢复", "启动面板")
        driver.call(["click", "--element-index", str(restore)],
                    "恢复含内嵌图片的记录到内存文档")
        deadline = time.monotonic() + 10.0
        title = ""
        while time.monotonic() < deadline:
            title = (driver.snapshot().get("window") or {}).get("title") or ""
            if fixture.name in title:
                break
            time.sleep(0.4)
        snapshot = driver.snapshot()
        title = (snapshot.get("window") or {}).get("title") or ""
        image_path = materialized_path(home, info["locator"])
        materialized = wait_materialized(image_path, 8.0)
        restored_image = image_state(image_path)
        restored_fixture = fixture_state(fixture)
        write_json(evidence / "restored.json", {
            "title": title,
            "buttons": panel_buttons(driver, snapshot),
            "fixture": restored_fixture,
            "materialized": materialized,
            "image": restored_image,
            "note": "恢复只入内存；图片按记录内嵌字节写回临时根并可独立解码",
        })
        if fixture.name not in title:
            raise DriverError("恢复后窗口标题不含夹具名：" + title)
        if restored_fixture["sha256"] != fixture_before["sha256"]:
            raise DriverError("恢复内存文档时源夹具被改写")
        if not materialized or restored_image is None:
            raise DriverError("恢复后临时图片未物化：" + info["locator"])
        if restored_image["sha256"] != hashlib.sha256(seed_png).hexdigest():
            raise DriverError("物化图片字节与恢复记录内嵌 PNG 不一致")
        if restored_image["format"] != "PNG" \
                or restored_image["dimensions"] != [1, 1]:
            raise DriverError("物化图片无法按 PNG 1x1 解码："
                              + str(restored_image))
        shutil.copyfile(image_path, evidence / "materialized-image.png")

        attempts = close_recovered_document(driver, fixture)
        write_json(evidence / "close-attempts.json", {
            "attempts": attempts,
            "closed": any(attempt["closed"] for attempt in attempts),
            "note": "恢复文档关闭的多路径投递记录；首条必须成功",
        })
        if not attempts[0]["closed"]:
            raise DriverError("恢复后首次 cmd+w 未关闭文档："
                              + str([attempt["label"] for attempt in attempts]))
        title = (driver.snapshot().get("window") or {}).get("title") or ""
        if fixture.name in title:
            raise DriverError("所有关闭投递路径都未关闭恢复文档")

        present = wait_files(evidence, CLOSE_FILES, 12.0)
        if not all(present.values()):
            raise DriverError("恢复文档关闭后采样证据缺失：" + str(present))
        verify_close_samples(evidence, seed_source, FIXTURE_TEXT.encode())
        deadline = time.monotonic() + 8.0
        image_after_close = image_state(image_path)
        while image_after_close is not None and time.monotonic() < deadline:
            time.sleep(0.4)
            image_after_close = image_state(image_path)
        snapshot = driver.snapshot()
        buttons = panel_buttons(driver, snapshot)
        write_json(evidence / "after-close.json", {
            "buttons": buttons,
            "close_files": present,
            "image_exists": image_after_close is not None,
            "store": store_snapshot(home),
            "note": "关闭恢复文档后条目只提供丢弃；无恢复守卫的资源清理应"
                    "删除临时图片",
        })
        if buttons["丢弃"] is None:
            raise DriverError("关闭恢复文档后没有出现丢弃按钮")
        if buttons["恢复"] is not None:
            raise DriverError("已恢复条目仍提供恢复按钮")
        if image_after_close is not None:
            raise DriverError("关闭恢复文档后临时图片未被清理")
        if fixture_state(fixture)["sha256"] != fixture_before["sha256"]:
            raise DriverError("关闭恢复文档改写了源夹具")

        discard = require_button(driver, snapshot, "丢弃", "已恢复条目")
        driver.call(["click", "--element-index", str(discard)],
                    "丢弃已恢复记录（进入二次确认）")
        time.sleep(0.5)
        snapshot = driver.snapshot()
        confirm = require_button(driver, snapshot, "确认丢弃", "丢弃二次确认")
        driver.call(["click", "--element-index", str(confirm)], "确认丢弃")
        deadline = time.monotonic() + 8.0
        cleaned = False
        while time.monotonic() < deadline:
            if not store_snapshot(home)["generations"]:
                cleaned = True
                break
            time.sleep(0.4)
        write_json(evidence / "discard-confirmed.json", {
            "generations_cleared": cleaned,
            "store": store_snapshot(home),
            "fixture": fixture_state(fixture),
            "note": "显式丢弃后恢复代退役清理；源夹具不受影响",
        })
        if not cleaned:
            raise DriverError("确认丢弃后恢复代未清理")
        if fixture_state(fixture)["sha256"] != fixture_before["sha256"]:
            raise DriverError("丢弃恢复记录改写了源夹具")

        driver.close_window()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            write_json(evidence / "window-close-timeout.json",
                       {"pid": proc.pid,
                        "note": "关闭窗口后进程未退出，记录后终止"})
            proc.terminate()
            proc.wait(timeout=10)
        write_json(evidence / "process-exit.json", {
            "pid": proc.pid,
            "exit_code": proc.returncode,
            "note": "图片恢复、清理与丢弃完成后的正常退出状态",
        })
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        settle_logs(evidence, stdout, stderr, log_dir)
        shutil.rmtree(work, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = require_validation_path(args.evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    running = subprocess.run(["pgrep", "-f", "target/debug/aline"],
                             text=True, capture_output=True)
    if running.stdout.strip():
        print("已有 aline 进程占用，拒绝干扰：" + running.stdout.strip(),
              file=sys.stderr)
        return 2
    try:
        run(evidence)
    except DriverError as error:
        write_text(evidence / "driver-error.txt", str(error)[:3000])
        print("驱动失败：" + str(error)[:1200], file=sys.stderr)
        return 1
    print("恢复图片播种→扫描→恢复→独立解码→清理→丢弃取证完成", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
