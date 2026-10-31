#!/usr/bin/env python3
"""经单独授权与安全复核后运行固定 macOS 隔离探针；不改变 workspace。"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "macos-probe"
parser = argparse.ArgumentParser()
parser.add_argument("--execute", action="store_true", help="确认已获独立实验授权及源码安全复核")
parser.add_argument("--output-dir", type=Path, default=ROOT / "macos-probe-execution")
args = parser.parse_args()
if not args.execute:
    parser.error("需先完成实验授权和独立源码安全复核，再显式传 --execute")
args.output_dir.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
env.update(CARGO_HOME="/Users/xian/Library/Caches/aline-m0/cargo-home",
           RUSTUP_TOOLCHAIN="1.98.1", CARGO_NET_OFFLINE="true")


def utc():
    return datetime.now(timezone.utc).isoformat()


def run(name, command, cwd):
    started = utc()
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True)
    record = {"command": command, "cwd": str(cwd), "started_utc": started,
              "ended_utc": utc(), "exit_code": result.returncode,
              "environment": {key: env[key] for key in
                              ["CARGO_HOME", "RUSTUP_TOOLCHAIN", "CARGO_NET_OFFLINE"]}}
    (args.output_dir / f"{name}.command.json").write_text(json.dumps(record, indent=2) + "\n")
    (args.output_dir / f"{name}.stdout.log").write_text(result.stdout)
    (args.output_dir / f"{name}.stderr.log").write_text(result.stderr)
    print(json.dumps(record), flush=True)
    if result.returncode:
        print(result.stdout, flush=True)
        print(result.stderr, flush=True)
        raise SystemExit(result.returncode)
    return result.stdout


identity = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
            for path in SOURCE.iterdir() if path.is_file()}
(args.output_dir / "source-hashes.json").write_text(json.dumps(identity, indent=2) + "\n")
run("rustc-version", ["rustc", "-Vv"], ROOT)
run("cargo-version", ["cargo", "-V"], ROOT)
run("system", ["sw_vers"], ROOT)
cleanup_error = None
build_root = Path(tempfile.mkdtemp(prefix="aline-44-probe-build-"))
try:
    if build_root.stat().st_mode & 0o777 != 0o700:
        raise RuntimeError("构建根不是0700，停止执行")
    run("filesystem", ["/bin/df", "-P", str(build_root)], build_root)
    (build_root / "src").mkdir(mode=0o700)
    shutil.copyfile(SOURCE / "Cargo.toml", build_root / "Cargo.toml")
    shutil.copyfile(SOURCE / "main.rs", build_root / "src" / "main.rs")
    run("lock", ["cargo", "generate-lockfile", "--offline"], build_root)
    shutil.copyfile(build_root / "Cargo.lock", args.output_dir / "Cargo.lock")
    run("build", ["cargo", "build", "--locked", "--offline"], build_root)
    run("probe", [str(build_root / "target" / "debug" / "aline-macos-metadata-probe")], build_root)
finally:
    try:
        shutil.rmtree(build_root)
    except OSError as error:
        cleanup_error = str(error)
    record = {"path": str(build_root), "ended_utc": utc(), "cleanup_error": cleanup_error}
    (args.output_dir / "build-cleanup.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record), flush=True)
    if cleanup_error:
        raise RuntimeError(f"构建目录清理失败：{cleanup_error}")
