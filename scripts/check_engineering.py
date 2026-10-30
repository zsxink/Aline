#!/usr/bin/env python3
"""八成员 workspace 的依赖门禁，Cargo 采集失败与规则拒绝都返回非零。

默认在线程外同步采集完整图及登记桌面目标图；不构建平台后端、不声明平台支持。
--self-test 使用隔离本地 Cargo workspace 与明确标记的图变异，不污染正式清单。
"""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from engineering_dependencies import Violation, load_json, validate

ROOT = Path(__file__).resolve().parent.parent
POLICY = ROOT / "scripts/engineering_policy.json"
SCENARIOS = {"all": None, "macos-arm": "aarch64-apple-darwin", "macos-intel": "x86_64-apple-darwin",
             "windows-x64": "x86_64-pc-windows-msvc", "linux-x64": "x86_64-unknown-linux-gnu",
             "linux-arm": "aarch64-unknown-linux-gnu", "all-features": None}


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def record_command(argv, directory, label, cwd=ROOT):
    """记录完整参数、时间、退出码与输出 hash；失败仍保存原输出。"""
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    try:
        completed = subprocess.run(argv, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        code, stdout, stderr = completed.returncode, completed.stdout, completed.stderr
    except OSError as error:
        code, stdout, stderr = 127, "", str(error)
    out, err = directory / (label + ".stdout.log"), directory / (label + ".stderr.log")
    out.write_text(stdout)
    err.write_text(stderr)
    record = {"argv": argv, "cwd": str(cwd), "started_utc": started,
              "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "exit_code": code,
              "stdout_sha256": sha256(out), "stderr_sha256": sha256(err),
              "env_overrides": {key: os.environ[key] for key in ("CARGO_HOME", "RUSTUP_TOOLCHAIN") if key in os.environ}}
    command = directory / (label + ".command.json")
    command.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
    return out, command


def successful_acquisition(path, command, tool):
    """失败的 Cargo 输出不可以被复用为合规证据。"""
    record = load_json(command)
    if record.get("exit_code") != 0:
        raise Violation("Cargo 采集失败: " + str(command))
    argv = record.get("argv", [])
    if not isinstance(argv, list) or not argv or Path(argv[0]).name != "cargo" or tool not in argv:
        raise Violation("采集命令类型不符: " + str(command))
    if record.get("stdout_sha256") != sha256(path):
        raise Violation("采集输出 hash 不符: " + str(path))


def argument_value(args, flag):
    if args.count(flag) > 1:
        raise Violation("重复采集参数: " + flag)
    try:
        return args[args.index(flag) + 1]
    except (ValueError, IndexError):
        return None


def check_inputs(metadata, tree, lock, policy, scenario, metadata_command, tree_command,
                 target_metadata=None, target_metadata_command=None):
    successful_acquisition(metadata, metadata_command, "metadata")
    successful_acquisition(tree, tree_command, "tree")
    target = SCENARIOS[scenario]
    meta_args, tree_args = load_json(metadata_command)["argv"], load_json(tree_command)["argv"]
    if argument_value(meta_args, "--format-version") != "1":
        raise Violation("metadata 必须使用 format-version 1")
    # 分目标 metadata 会删除 host proc-macro/build 真正需要的 cfg 边，不能用作完整闭包。
    if "--filter-platform" in meta_args or argument_value(tree_args, "--target") != (target or "all"):
        raise Violation("采集 target 与场景标签不符: " + scenario)
    if any(("--all-features" in args) != (scenario == "all-features") for args in (meta_args, tree_args)):
        raise Violation("采集 feature 与场景标签不符: " + scenario)
    if argument_value(tree_args, "-e") != "normal,build,dev" or "--workspace" not in tree_args:
        raise Violation("tree 必须覆盖 workspace normal/build/dev")
    manifest = argument_value(meta_args, "--manifest-path")
    if manifest != argument_value(tree_args, "--manifest-path") or not manifest or Path(manifest).parent / "Cargo.lock" != Path(lock):
        raise Violation("metadata/tree/lock 所属 manifest 不一致")
    data = load_json(metadata)
    if target:
        if not target_metadata or not target_metadata_command:
            raise Violation("分目标场景缺少 target-filter metadata 交叉证据")
        successful_acquisition(target_metadata, target_metadata_command, "metadata")
        target_args = load_json(target_metadata_command)["argv"]
        if argument_value(target_args, "--filter-platform") != target or argument_value(target_args, "--format-version") != "1" or argument_value(target_args, "--manifest-path") != manifest or "--all-features" in target_args:
            raise Violation("target-filter metadata 参数与场景不符")
        filtered = load_json(target_metadata)
        full_packages = {p["id"]: p for p in data["packages"]}
        full_nodes = {n["id"]: n for n in data["resolve"]["nodes"]}
        if set(filtered["workspace_members"]) != set(data["workspace_members"]):
            raise Violation("target-filter workspace 身份不符")
        for package in filtered["packages"]:
            if package != full_packages.get(package["id"]):
                raise Violation("target-filter package 身份或声明与完整图不符")
        for node in filtered["resolve"]["nodes"]:
            full_node = full_nodes.get(node["id"])
            if full_node is None or not set(node["features"]).issubset(full_node["features"]):
                raise Violation("target-filter node/feature 不是完整图子集")
            for edge in node["deps"]:
                # Cargo 可将同 pkg 的多个 target/kind 聚合为一条边，逐项比较条件子集。
                matching = [e for e in full_node["deps"] if e["pkg"] == edge["pkg"] and e["name"] == edge["name"]]
                if not matching or any(k not in matching[0]["dep_kinds"] for k in edge["dep_kinds"]):
                    raise Violation("target-filter edge 不是完整图子集")
    result = validate(data, tree, lock, policy, scenario)
    result["graph_model"] = "完整 all-target metadata 约束身份/边/保守闭包；实际 tree 包含目标及本机 host 宏/构建选包；filtered metadata 仅作交叉证据"
    return result


def collect(manifest, output, policy):
    acquisitions = []
    for scenario, target in SCENARIOS.items():
        common = ["--manifest-path", str(manifest), "--locked", "--offline"]
        feature = ["--all-features"] if scenario == "all-features" else []
        meta_args = ["cargo", "metadata"] + common + ["--format-version", "1"] + feature
        tree_args = ["cargo", "tree"] + common + ["--workspace", "--target", target or "all", "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}"] + feature
        metadata, meta_command = record_command(meta_args, output, scenario + "-metadata")
        tree, tree_command = record_command(tree_args, output, scenario + "-tree")
        filtered = filtered_command = None
        if target:
            filtered, filtered_command = record_command(meta_args + ["--filter-platform", target], output, scenario + "-target-metadata")
        acquisitions.append((metadata, tree, manifest.parent / "Cargo.lock", policy, scenario, meta_command, tree_command, filtered, filtered_command))
    # 当前无自有 feature；未来任何新增组合必须登记，不让全特征并集掩盖互斥组合。
    data = load_json(Path(output) / "all-metadata.stdout.log")
    for package in data["packages"]:
        if package["id"] in data["workspace_members"] and package.get("features") != policy["workspace_features"].get(package["name"]):
            raise Violation("自有 feature 组合未经登记: " + package["name"])
    return [check_inputs(*args) for args in acquisitions]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--manifest-path", type=Path, default=ROOT / "Cargo.toml")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--policy", type=Path, default=POLICY)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--tree", type=Path)
    parser.add_argument("--lock", type=Path)
    parser.add_argument("--scenario", choices=SCENARIOS)
    parser.add_argument("--metadata-command", type=Path)
    parser.add_argument("--tree-command", type=Path)
    parser.add_argument("--target-metadata", type=Path)
    parser.add_argument("--target-metadata-command", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            from engineering_self_test import run
            return run(args.output_dir)
        policy = load_json(args.policy)
        if args.metadata:
            if not all((args.tree, args.lock, args.scenario, args.metadata_command, args.tree_command)):
                raise Violation("离线图核查必须提供 tree/lock/scenario/两条成功采集记录")
            result = check_inputs(args.metadata, args.tree, args.lock, policy, args.scenario, args.metadata_command, args.tree_command, args.target_metadata, args.target_metadata_command)
        else:
            output = args.output_dir or Path(tempfile.mkdtemp(prefix="aline-engineering-"))
            result = collect(args.manifest_path.resolve(), output, policy)
        print(json.dumps({"status": "依赖门禁通过", "results": result}, ensure_ascii=False, indent=2))
        return 0
    except (Violation, OSError, KeyError, TypeError, ValueError) as error:
        print("依赖门禁拒绝: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
