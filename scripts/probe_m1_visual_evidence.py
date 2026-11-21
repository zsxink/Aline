#!/usr/bin/env python3
"""QMD-01 W1：核对锁定离屏 API 与候选特征图；准入前不构建或启动。"""

import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile

from check_engineering import ADMISSION_POLICY, POLICY, check_inputs, record_command
from dependency_admission import license_review
from engineering_dependencies import (Violation, fingerprint, gpui_snapshot, load_json, model,
                                      normalized_edge, package_key, parse_lock, parse_tree,
                                      reachable)
from evidence_paths import write_json
from validation_output import ROOT, output_directory


SURFACES = {
    "gpui-pre@0.3.9": {
        "src/app/visual_test_context.rs": (
            "pub struct VisualTestAppContext", "pub fn open_offscreen_window<",
            "pub fn capture_screenshot", "window.render_to_image()"),
        "src/platform.rs": ('feature = "test-support"', "visual_test"),
        "Cargo.toml": ('test-support = [', '"proptest"', '"dep:futures-concurrency"',
                       '"dep:postage"', '"dep:web-time"', '"wayland"', '"x11"'),
    },
    "gpui-pre-macos@0.3.9": {
        "src/window.rs": ('feature = "test-support"', "fn render_to_image",
                          "this.renderer.render_to_image(scene)"),
        "Cargo.toml": ('test-support = [', '"gpui/test-support"',
                       '"gpui_apple/test-support"'),
    },
}

MEMBER_PATHS = ("app", "crates/aline-app-core", "crates/aline-source", "crates/aline-buffer",
                "crates/aline-markdown", "crates/aline-edit", "crates/aline-plugin", "crates/aline-ui")


def inspect_archive(path, expected_checksum, files):
    """只读经过锁定校验的归档；不信任可变解包缓存，不向磁盘解包。"""
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected_checksum:
        raise Violation("归档 checksum 不匹配：" + path.name)
    snippets = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        prefix = path.name.removesuffix(".crate") + "/"
        for relative, needles in files.items():
            member = archive.getmember(prefix + relative)
            if not member.isfile() or member.size > 4 * 1024 * 1024:
                raise Violation("源码归档成员不是合理普通文件：" + relative)
            stream = archive.extractfile(member)
            if stream is None:
                raise Violation("源码归档成员不可读：" + relative)
            source = stream.read().decode("utf-8")
            if any(needle not in source for needle in needles):
                raise Violation("锁定源码缺少预期 API：" + relative)
            lines = source.splitlines()
            snippets[relative] = {
                "sha256": hashlib.sha256(source.encode()).hexdigest(),
                "matching_lines": [
                    {"line": index + 1, "source": line}
                    for index, line in enumerate(lines)
                    if any(needle in line for needle in needles)
                ],
            }
    return snippets


def archive_for(cargo_home, key, checksum):
    name, version = key.split("@", 1)
    matches = list((cargo_home / "registry" / "cache").glob("*/" + name + "-" + version + ".crate"))
    valid = [path for path in matches if hashlib.sha256(path.read_bytes()).hexdigest() == checksum]
    if len(valid) != 1:
        raise Violation("需要唯一与 Cargo.lock checksum 一致的本地归档：" + key)
    return valid[0]


@contextmanager
def selected_cargo_home(cargo_home):
    """源码归档与 Cargo 图使用同一显式缓存；退出时恢复调用方环境。"""
    previous = os.environ.get("CARGO_HOME")
    os.environ["CARGO_HOME"] = str(cargo_home.resolve())
    try:
        yield
    finally:
        if previous is None:
            os.environ.pop("CARGO_HOME", None)
        else:
            os.environ["CARGO_HOME"] = previous


def acquisition_result(output, label):
    """保留实际失败诊断，区分尚未取得候选图与已取得图后的准入拒绝。"""
    command = load_json(output / (label + ".command.json"))
    return {"exit_code": command["exit_code"],
            "command": str(output / (label + ".command.json")),
            "stderr": str(output / (label + ".stderr.log")),
            "diagnostic": (output / (label + ".stderr.log")).read_text(encoding="utf-8").strip()}


def copy_audit_workspace(workspace, root=ROOT):
    """只复制元数据输入；桩入口不构建，不反向修改正式 workspace。"""
    identities = {}
    for relative in ("Cargo.toml", "Cargo.lock", *(member + "/Cargo.toml" for member in MEMBER_PATHS)):
        source = root / relative
        data = source.read_bytes()
        target = workspace / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        identities[relative] = hashlib.sha256(data).hexdigest()
    for member in MEMBER_PATHS:
        source = workspace / member / "src" / ("main.rs" if member == "app" else "lib.rs")
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text("//! 仅供候选依赖 metadata 发现目标，不构建。\n", encoding="utf-8")
    return identities


def activate_audit_feature(workspace):
    """仅改隔离清单的精确 feature 字段，便于生成自洽候选锁。"""
    path = workspace / "Cargo.toml"
    text = path.read_text(encoding="utf-8")
    before = 'gpui_macos = { package = "gpui-pre-macos", version = "=0.3.9", default-features = false, features = ["font-kit"] }'
    after = before.replace('["font-kit"]', '["font-kit", "test-support"]')
    if text.count(before) != 1:
        raise Violation("隔离候选清单的 GPUI 声明已变化，需要重新核对精确 feature 激活位置")
    path.write_text(text.replace(before, after), encoding="utf-8")
    return {"before": before, "after": after}


def graph_rows(metadata, lock):
    packages, nodes, roots, _ = model(metadata, lock)
    rows = {}
    for identity, package in packages.items():
        key = package_key(package)
        rows[key] = {"package": key, "source": package.get("source"),
                     "checksum": lock[key].get("checksum"), "license": package.get("license"),
                     "features": sorted(nodes[identity]["features"]),
                     "edges": sorted((normalized_edge(packages, identity, edge)
                                      for edge in nodes[identity]["deps"]), key=lambda edge: json.dumps(edge, sort_keys=True))}
    return rows, packages, nodes, roots


def compare_candidate(baseline, candidate, baseline_lock, candidate_lock, candidate_tree):
    """按真实包身份比较全图；候选材料不构成任何准入批准。"""
    before, _, _, _ = graph_rows(baseline, baseline_lock)
    after, packages, nodes, roots = graph_rows(candidate, candidate_lock)
    _, _, _, identities = model(candidate, candidate_lock)
    selected = parse_tree(candidate_tree, identities, roots)
    new = sorted(set(after) - set(before))
    policy = load_json(ADMISSION_POLICY)
    licenses = []
    for key in new:
        package = packages[identities[key]]
        try:
            verdict = license_review(package, policy["license"], False)
        except Violation as error:
            verdict = {"verdict": "rejected", "reason": str(error)}
        licenses.append({"package": key, "review": verdict,
                         "selected_macos_arm": identities[key] in selected,
                         "engine_consumers": sorted(packages[root]["name"] for root in roots
                                                    if packages[root]["name"] in ("aline-source", "aline-buffer", "aline-markdown", "aline-edit", "aline-app-core")
                                                    and identities[key] in reachable(nodes, [root]))})
    gpui = next((identity for identity, package in packages.items() if package["name"] == "gpui-pre"), None)
    if gpui is None:
        raise Violation("候选图缺少锁定 GPUI，不能计算子图身份")
    return {"added_packages": [after[key] for key in new],
            "removed_packages": [before[key] for key in sorted(set(before) - set(after))],
            "changed_packages": [{"package": key, "before": before[key], "after": after[key]}
                                 for key in sorted(set(before) & set(after)) if before[key] != after[key]],
            "candidate_metadata_packages": len(after), "macos_arm_selected_packages": len(selected),
            "candidate_gpui_subtree_sha256": fingerprint(gpui_snapshot(packages, nodes, gpui, selected, candidate_lock)),
            "added_package_license_checks": licenses}


def audit_candidate(output, cargo_home):
    """显式联网只形成 logs 内的候选锁；锁定复采后仍交正式规则拒绝。"""
    preflight = output / "formal-preflight"
    preflight.mkdir(parents=True, exist_ok=True)
    probe(preflight, cargo_home)
    workspace = Path(tempfile.mkdtemp(prefix="candidate-workspace-", dir=output))
    inputs = copy_audit_workspace(workspace)
    activation = activate_audit_feature(workspace)
    write_json(output / "audit-inputs.json", {"inputs": inputs, "workspace": str(workspace),
               "candidate_manifest_change": activation, "targets_are_metadata_stubs": True, "build_executed": False})
    common = ["--manifest-path", str(workspace / "Cargo.toml")]
    selected = ["--features", "gpui_macos/test-support"]
    previous_offline = os.environ.get("CARGO_NET_OFFLINE")
    try:
        with selected_cargo_home(cargo_home):
            os.environ["CARGO_NET_OFFLINE"] = "false"
            _, generated = record_command(["cargo", "update", "--workspace", *common], output, "audit-lock")
            if load_json(generated)["exit_code"] != 0:
                raise Violation("隔离候选锁解析失败：" + str(generated))
            _, command = record_command(["cargo", "metadata", *common, *selected,
                                         "--format-version", "1"], output, "audit-resolve")
            if load_json(command)["exit_code"] != 0:
                raise Violation("隔离候选解析失败：" + str(command))
            os.environ["CARGO_NET_OFFLINE"] = "true"
            metadata, metadata_command = record_command(["cargo", "metadata", *common, *selected,
                "--locked", "--offline", "--format-version", "1"], output, "audit-metadata")
            filtered, filtered_command = record_command(["cargo", "metadata", *common, *selected,
                "--locked", "--offline", "--format-version", "1", "--filter-platform", "aarch64-apple-darwin"], output, "audit-filtered")
            tree, tree_command = record_command(["cargo", "tree", *common, *selected, "--locked", "--offline",
                "--workspace", "--target", "aarch64-apple-darwin", "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}"], output, "audit-tree")
            baseline, baseline_command = record_command(["cargo", "metadata", "--manifest-path", str(ROOT / "Cargo.toml"),
                "--locked", "--offline", "--format-version", "1"], output, "formal-metadata")
    finally:
        if previous_offline is None:
            os.environ.pop("CARGO_NET_OFFLINE", None)
        else:
            os.environ["CARGO_NET_OFFLINE"] = previous_offline
    for command in (metadata_command, filtered_command, tree_command, baseline_command):
        if load_json(command)["exit_code"] != 0:
            raise Violation("锁定复采失败：" + str(command))
    lock = workspace / "Cargo.lock"
    difference = compare_candidate(load_json(baseline), load_json(metadata), parse_lock(ROOT / "Cargo.lock"),
                                   parse_lock(lock), tree)
    write_json(output / "candidate-diff.json", difference)
    summary = {"schema": "aline-qmd-w1-candidate-audit/v1", "runtime_executed": False,
               "build_executed": False, "decision": "no-go", "candidate_graph_acquired": True,
               "candidate_workspace": str(workspace), "candidate_lock_sha256": hashlib.sha256(lock.read_bytes()).hexdigest(),
               "added_packages": len(difference["added_packages"]), "changed_packages": len(difference["changed_packages"]),
               "removed_packages": len(difference["removed_packages"]), "source_inputs_unchanged": all(
                   hashlib.sha256((ROOT / relative).read_bytes()).hexdigest() == checksum for relative, checksum in inputs.items()),
               "limits": "仅形成准入材料；未构建、未运行离屏、未修改或批准正式依赖策略"}
    try:
        check_inputs(metadata, tree, lock, load_json(POLICY), "macos-arm", metadata_command,
                     tree_command, filtered, filtered_command, load_json(ADMISSION_POLICY))
        summary["admission_passed"] = True
        summary["reason"] = "候选通过现行规则，但仍需具体批准与真实离屏实验"
    except Violation as error:
        summary["admission_passed"] = False
        summary["reason"] = str(error)
    if not summary["source_inputs_unchanged"]:
        summary["reason"] = "正式输入在隔离审计期间发生变化，候选材料需要重新采集"
    write_json(output / "audit-summary.json", summary)
    print("no-go: " + summary["reason"])
    return 1


def probe(output, cargo_home):
    lock_path = ROOT / "Cargo.lock"
    lock_before = hashlib.sha256(lock_path.read_bytes()).hexdigest()
    locks = parse_lock(lock_path)
    sources = {}
    for key, files in SURFACES.items():
        checksum = locks[key]["checksum"]
        archive = archive_for(cargo_home, key, checksum)
        sources[key] = {"archive": str(archive), "checksum": checksum,
                        "files": inspect_archive(archive, checksum, files)}
    write_json(output / "locked-source.json", sources)
    common = ["--manifest-path", str(ROOT / "Cargo.toml"), "--locked", "--offline",
              "--features", "gpui_macos/test-support"]
    with selected_cargo_home(cargo_home):
        metadata, metadata_command = record_command(
            ["cargo", "metadata", *common, "--format-version", "1"], output, "candidate-metadata")
        tree, tree_command = record_command(
            ["cargo", "tree", *common, "--workspace", "--target", "all", "-e", "normal,build,dev",
             "--prefix", "none", "--format", "{p}"], output, "candidate-tree")
    acquisitions = {label: acquisition_result(output, "candidate-" + label)
                    for label in ("metadata", "tree")}
    summary = {"schema": "aline-qmd-w1-preflight/v1", "runtime_executed": False,
               "locked_source_surfaces_present": True,
               "lock_sha256_before": lock_before,
               "cargo_home": str(cargo_home.resolve()),
               "candidate_acquisition": acquisitions,
               "candidate_graph_acquired": all(item["exit_code"] == 0 for item in acquisitions.values()),
               "candidate_features": ["gpui_macos/test-support"],
               "not_measured": ["真实离屏像素", "连续两次稳定性", "真实窗口/焦点/IME/跨平台"]}
    try:
        check_inputs(metadata, tree, lock_path, load_json(POLICY), "all",
                     metadata_command, tree_command, admission=load_json(ADMISSION_POLICY))
        summary["admission_passed"] = True
        summary["decision"] = "runtime-required"
        summary["reason"] = "候选图通过；仍需真实主线程离屏实验才能判定 go"
    except Violation as error:
        summary["admission_passed"] = False
        summary["decision"] = "no-go"
        summary["reason"] = str(error)
        summary["blocked_on"] = ("dependency-admission" if summary["candidate_graph_acquired"]
                                 else "candidate-acquisition")
    lock_after = hashlib.sha256(lock_path.read_bytes()).hexdigest()
    summary["lock_unchanged"] = lock_before == lock_after
    summary["lock_sha256_after"] = lock_after
    if not summary["lock_unchanged"]:
        summary["decision"] = "no-go"
        summary["reason"] = "锁文件在采集期间变化；候选证据无效，不回滚并发工作"
    if load_json(metadata_command)["exit_code"] == 0:
        candidate = load_json(metadata)
        packages = {package["id"]: package for package in candidate["packages"]}
        summary["candidate_gpui_features"] = {
            packages[node["id"]]["name"]: node["features"]
            for node in candidate["resolve"]["nodes"]
            if packages[node["id"]]["name"].startswith("gpui-pre")
        }
    write_json(output / "summary.json", summary)
    print(summary["decision"] + ": " + summary["reason"])
    return 1  # 预检本身永远不能宣告真实平台能力通过。


def self_test():
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "sample-1.crate"
        with tarfile.open(path, "w:gz") as archive:
            payload = b"pub fn capture_screenshot() {}\n"
            info = tarfile.TarInfo("sample-1/src/lib.rs")
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))
        checksum = hashlib.sha256(path.read_bytes()).hexdigest()
        assert inspect_archive(path, checksum, {"src/lib.rs": ["capture_screenshot"]})
        for expected, needles in (("0" * 64, ["capture_screenshot"]),
                                  (checksum, ["missing_api"])):
            try:
                inspect_archive(path, expected, {"src/lib.rs": needles})
            except Violation:
                continue
            raise AssertionError("不完整锁定能力被接受")
        previous = os.environ.get("CARGO_HOME")
        try:
            for initial in (None, "unchanged-test-cache"):
                if initial is None:
                    os.environ.pop("CARGO_HOME", None)
                else:
                    os.environ["CARGO_HOME"] = initial
                for failure in (False, True):
                    try:
                        with selected_cargo_home(Path(directory)):
                            assert os.environ["CARGO_HOME"] == str(Path(directory).resolve())
                            if failure:
                                raise ValueError("模拟采集失败")
                    except ValueError:
                        pass
                    assert os.environ.get("CARGO_HOME") == initial
        finally:
            if previous is None:
                os.environ.pop("CARGO_HOME", None)
            else:
                os.environ["CARGO_HOME"] = previous
        root = Path(directory) / "formal"
        for relative in ("Cargo.toml", "Cargo.lock", *(member + "/Cargo.toml" for member in MEMBER_PATHS)):
            source = root / relative
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text("冻结输入\n", encoding="utf-8")
        isolated = Path(directory) / "isolated"
        inputs = copy_audit_workspace(isolated, root)
        (isolated / "Cargo.lock").write_text("仅候选改变\n", encoding="utf-8")
        assert all(hashlib.sha256((root / relative).read_bytes()).hexdigest() == checksum
                   for relative, checksum in inputs.items())
        before = (isolated / "Cargo.toml").read_text(encoding="utf-8")
        try:
            activate_audit_feature(isolated)
        except Violation:
            assert (isolated / "Cargo.toml").read_text(encoding="utf-8") == before
        else:
            raise AssertionError("未知候选声明被替换")
        original = 'gpui_macos = { package = "gpui-pre-macos", version = "=0.3.9", default-features = false, features = ["font-kit"] }'
        (isolated / "Cargo.toml").write_text(original + "\n", encoding="utf-8")
        activation = activate_audit_feature(isolated)
        assert activation["before"] == original
        assert (isolated / "Cargo.toml").read_text(encoding="utf-8") == original.replace('["font-kit"]', '["font-kit", "test-support"]') + "\n"
        assert (root / "Cargo.toml").read_text(encoding="utf-8") == "冻结输入\n"

        roots = ["aline" if member == "app" else Path(member).name for member in MEMBER_PATHS]
        packages = [{"id": name, "name": name, "version": "0.1.0", "source": None}
                    for name in roots]
        packages.append({"id": "gpui", "name": "gpui-pre", "version": "0.3.9",
                         "source": "registry+https://github.com/rust-lang/crates.io-index", "license": "Apache-2.0"})
        nodes = [{"id": package["id"], "features": [], "deps": []} for package in packages]
        next(node for node in nodes if node["id"] == "aline-ui")["deps"] = [
            {"pkg": "gpui", "name": "gpui", "dep_kinds": [{"kind": None, "target": None}]}]
        baseline = {"packages": packages, "workspace_members": roots, "resolve": {"nodes": nodes, "root": None}}
        candidate = json.loads(json.dumps(baseline))
        locks = {package_key(package): {"name": package["name"], "version": package["version"],
                  "source": package["source"], "checksum": "a" * 64 if package["source"] else None}
                 for package in packages}
        tree = Path(directory) / "tree.txt"
        tree.write_text("\n".join(package["name"] + " v" + package["version"] for package in packages))
        same = compare_candidate(baseline, candidate, locks, locks, tree)
        assert not same["added_packages"] and not same["changed_packages"]
        helper = {"id": "helper", "name": "spike-helper", "version": "1.0.0", "source": packages[-1]["source"], "license": "MIT"}
        candidate["packages"].append(helper)
        candidate["resolve"]["nodes"].append({"id": "helper", "features": [], "deps": []})
        candidate["resolve"]["nodes"][-2]["features"] = ["test-support"]
        candidate["resolve"]["nodes"][-2]["deps"].append(
            {"pkg": "helper", "name": "spike_helper", "dep_kinds": [{"kind": None, "target": None}]})
        candidate_locks = dict(locks, **{"spike-helper@1.0.0": {"name": "spike-helper", "version": "1.0.0", "source": helper["source"], "checksum": "b" * 64}})
        with tree.open("a") as stream:
            stream.write("\nspike-helper v1.0.0\n")
        difference = compare_candidate(baseline, candidate, locks, candidate_locks, tree)
        assert [row["package"] for row in difference["added_packages"]] == ["spike-helper@1.0.0"]
        assert [row["package"] for row in difference["changed_packages"]] == ["gpui-pre@0.3.9"]
        assert difference["added_package_license_checks"][0]["engine_consumers"] == []
        assert difference["added_package_license_checks"][0]["review"]["verdict"] == "allowed"
        helper["license"] = "GPL-3.0-only"
        assert compare_candidate(baseline, candidate, locks, candidate_locks, tree)["added_package_license_checks"][0]["review"]["verdict"] == "rejected"
        candidate["resolve"]["nodes"].pop()
        try:
            compare_candidate(baseline, candidate, locks, candidate_locks, tree)
        except Violation:
            pass
        else:
            raise AssertionError("缺少候选节点的图被接受")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--cargo-home", type=Path,
                        default=Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")))
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--audit-candidate", action="store_true",
                        help="仅在 logs 隔离副本联网解析候选锁并生成准入差异，不构建")
    args = parser.parse_args()
    try:
        output = output_directory("m1-visual-evidence-spike", args.output_dir)
        if args.self_test:
            self_test()
            write_json(output / "self-test.json", {"passed": True, "checks": 14})
            print("self-test: 14 项通过")
            return 0
        return (audit_candidate(output, args.cargo_home) if args.audit_candidate
                else probe(output, args.cargo_home))
    except (OSError, ValueError, KeyError, tarfile.TarError, Violation) as error:
        if "output" in locals():
            write_json(output / "summary.json", {"schema": "aline-qmd-w1-preflight/v1",
                       "decision": "no-go", "runtime_executed": False, "reason": str(error)})
        print("no-go: " + str(error))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
