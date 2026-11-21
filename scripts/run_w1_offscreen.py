#!/usr/bin/env python3
"""仅运行已批准的 W1 macOS arm64 离屏图；不修改生产依赖或正式策略。"""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import tempfile
import urllib.request

from check_engineering import ADMISSION_POLICY, POLICY, check_inputs, record_command
from engineering_dependencies import Violation, fingerprint, load_json, model, parse_lock, parse_tree
from evidence_paths import resolve_path, write_json
from probe_m1_visual_evidence import (activate_audit_feature, archive_for, copy_audit_workspace,
                                      graph_rows, selected_cargo_home)
from validation_output import ROOT, output_directory


FIXTURE = ROOT / "scripts/fixtures/w1-offscreen"


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise Violation(message)


@contextmanager
def run_environment(cargo_home, target):
    previous = {key: os.environ.get(key) for key in ("CARGO_NET_OFFLINE", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET")}
    try:
        with selected_cargo_home(cargo_home):
            os.environ["CARGO_NET_OFFLINE"] = "true"
            os.environ["CARGO_TARGET_DIR"] = str(target)
            os.environ.pop("CARGO_BUILD_TARGET", None)
            yield
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def full_graph(metadata, lock, tree):
    rows, _, _, _ = graph_rows(metadata, lock)
    _, _, roots, identities = model(metadata, lock)
    selected = parse_tree(tree, identities, roots)
    return [dict(row, selected=identities[key] in selected) for key, row in sorted(rows.items())]


def check_bound_graph(approval, rows, lock_hash, manifest_hash):
    """整个 sidecar/引擎/GPUI 图均绑定，不能只用 GPUI 子图掩盖其它漂移。"""
    require(approval.get("schema") == "aline-w1-offscreen-approval/v1"
            and approval.get("scope") == "isolated-offscreen-spike-only"
            and approval.get("target") == "aarch64-apple-darwin", "不是获批的 W1 隔离场景")
    require(lock_hash == approval["candidate_lock_sha256"], "W1 候选锁 checksum 漂移")
    require(manifest_hash == approval["candidate_root_manifest_sha256"], "W1 候选清单漂移")
    require(fingerprint(rows) == approval["full_graph_sha256"], "W1 完整身份/features/边/选包图漂移")
    require(len(rows) == approval["metadata_packages"], "W1 metadata 包数漂移")
    require(sum(row["selected"] for row in rows) == approval["selected_packages"], "W1 本机选包数漂移")


def scoped_policies(approval):
    """只在已绑定隔离图的内存副本登记明确批准的指纹和五条分支。"""
    graph_policy, admission = load_json(POLICY), load_json(ADMISSION_POLICY)
    graph_policy["scenarios"]["macos-arm"]["gpui_subtree_sha256"] = approval["gpui_subtree_sha256"]
    group = next(group for group in admission["groups"] if group["id"] == "gpui")
    group["branch_purposes"].update(approval["branch_purposes"])
    return graph_policy, admission


def verify_archive_bytes(data, expected):
    require(hashlib.sha256(data).hexdigest() == expected, "下载归档 checksum 与批准锁不符")


def prepare_cache(source, destination, lock, fetch_approved=False):
    """从逐项核对的原始归档创建新 src 缓存，拒绝复用可变解包源码。"""
    source = source.resolve()
    indexes = source / "registry/index"
    require(indexes.is_dir(), "缺少已核对 Cargo 索引缓存")
    shutil.copytree(indexes, destination / "registry/index")
    copied = []
    for key, identity in sorted(lock.items()):
        if not identity.get("source"):
            continue
        try:
            archive = archive_for(source, key, identity["checksum"])
            target = destination / "registry/cache" / archive.parent.name / archive.name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(archive, target)
            copied.append({"package": key, "sha256": sha256(target), "source": "verified-cache"})
        except Violation:
            if not fetch_approved:
                raise
            require(identity["source"] == "registry+https://github.com/rust-lang/crates.io-index", "禁止从其它来源补齐批准归档")
            name, version = key.split("@", 1)
            url = "https://static.crates.io/crates/" + name + "/" + name + "-" + version + ".crate"
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read(64 * 1024 * 1024 + 1)
            require(len(data) <= 64 * 1024 * 1024, "批准归档超过合理下载上限")
            verify_archive_bytes(data, identity["checksum"])
            namespaces = [path.name for path in (destination / "registry/index").iterdir()
                          if path.is_dir() and path.name.startswith("index.crates.io-")]
            require(namespaces, "缺少 crates.io 缓存命名空间")
            for namespace in namespaces:
                target = destination / "registry/cache" / namespace / (name + "-" + version + ".crate")
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            copied.append({"package": key, "sha256": hashlib.sha256(data).hexdigest(), "source": url,
                           "cache_namespaces": namespaces, "original_cache_unchanged": True})
    require(not (destination / "registry/src").exists(), "禁止预装未经重建的 registry/src")
    return copied


def command(argv, output, label):
    stdout, receipt = record_command(argv, output, label)
    require(load_json(receipt)["exit_code"] == 0, "命令失败，见 " + str(receipt))
    return stdout, receipt


def executable_artifact(messages, package_id, target_directory, target):
    """只采纳本轮 Cargo 明确报告的指定包/bin/profile/target 路径工件。"""
    rows = [json.loads(line) for line in messages.splitlines() if line.strip()]
    require(sum(row.get("reason") == "build-finished" and row.get("success") is True
                for row in rows) == 1, "缺少本轮 Cargo 成功完成回执")
    artifacts = [row for row in rows if row.get("reason") == "compiler-artifact"
                 and row.get("package_id") == package_id
                 and row.get("target", {}).get("name") == "aline"
                 and row.get("target", {}).get("kind") == ["bin"]
                 and row.get("executable") is not None]
    require(len(artifacts) == 1, "本轮 aline 二进制工件缺失或重复")
    artifact = artifacts[0]
    profile = artifact.get("profile", {})
    require(profile.get("opt_level") == "0" and profile.get("test") is False
            and profile.get("debug_assertions") is True, "离屏工件不是登记的 debug 非测试 profile")
    executable = resolve_path(artifact["executable"])
    require(isinstance(executable, str), "无法还原 Cargo 二进制工件路径")
    executable = Path(executable).resolve()
    expected = (target_directory / target / "debug/aline").resolve()
    require(executable == expected, "Cargo 工件不属于本轮显式 macOS arm64 target")
    return executable, artifact


def check_architectures(text):
    require(text.strip() == "arm64", "离屏工件不是单一 arm64 Mach-O")


def validate_captures(output):
    """消费真实进程生成的两份 RGBA 及尺寸记录，不接受单色或伪完成标记。"""
    records = []
    for line in (output / "capture.tsv").read_text(encoding="utf-8").splitlines():
        fields = dict(field.split("=", 1) for field in line.split("\t"))
        records.append(fields)
    captures = [row for row in records if row.get("kind") == "capture"]
    require(len(captures) == 2 and [row["sample"] for row in captures] == ["1", "2"], "缺少两次真实纹理记录")
    require(records[-1] == {"kind": "done", "stable_repeated_capture": "true"}, "离屏进程未完成稳定性验证")
    raw = []
    for row in captures:
        width, height, scale = int(row["pixel_width"]), int(row["pixel_height"]), float(row["scale"])
        require(width > 0 and height > 0 and scale > 0, "无效像素尺寸或缩放")
        require(abs(width - 640 * scale) <= 0.5 and abs(height - 400 * scale) <= 0.5, "像素尺寸与真实缩放不符")
        rgba = (output / ("capture-" + row["sample"] + ".rgba")).read_bytes()
        require(len(rgba) == width * height * 4 == int(row["bytes"]), "RGBA 字节数与记录不符")
        require(any(rgba[index:index + 4] != rgba[:4] for index in range(0, len(rgba), 4)), "纹理为单色")
        require(any(rgba[index] for index in range(3, len(rgba), 4)), "纹理全透明")
        raw.append(rgba)
    require(raw[0] == raw[1], "连续两次纹理不稳定")
    return {"captures": captures, "rgba_sha256": hashlib.sha256(raw[0]).hexdigest()}


def run(output, source_home, fetch_approved=False):
    require(sys.platform == "darwin" and platform.machine() in ("arm64", "aarch64"), "本入口仅批准 macOS arm64")
    approval = load_json(FIXTURE / "approval.json")
    require(sha256(FIXTURE / "Cargo.lock") == approval["candidate_lock_sha256"], "正式 W1 夹具锁被修改")
    for relative, expected in approval["source_inputs_sha256"].items():
        require(sha256(ROOT / relative) == expected, "正式清单/锁已变化，须重审 W1 构造输入：" + relative)
    workspace = Path(tempfile.mkdtemp(prefix="workspace-", dir=output))
    before = copy_audit_workspace(workspace)
    activate_audit_feature(workspace)
    shutil.copyfile(FIXTURE / "Cargo.lock", workspace / "Cargo.lock")
    shutil.copyfile(FIXTURE / "aline-ui-lib.rs", workspace / "crates/aline-ui/src/lib.rs")
    shutil.copyfile(FIXTURE / "app-main.rs", workspace / "app/src/main.rs")
    require(sha256(workspace / "Cargo.toml") == approval["candidate_root_manifest_sha256"], "生成清单与批准清单不符")
    cargo_home = output / "cargo-home"
    archives = prepare_cache(source_home, cargo_home, parse_lock(FIXTURE / "Cargo.lock"), fetch_approved)
    write_json(output / "archive-receipt.json", {"archives": archives, "source_cache": str(source_home),
               "runtime_cache": str(cargo_home), "registry_src_recreated_from_archives": True})
    common = ["--manifest-path", str(workspace / "Cargo.toml"), "--locked", "--offline"]
    target = ROOT / "target/w1-offscreen"
    with run_environment(cargo_home, target):
        metadata, meta_command = command(["cargo", "metadata", *common, "--format-version", "1"], output, "metadata")
        filtered, filtered_command = command(["cargo", "metadata", *common, "--format-version", "1",
            "--filter-platform", approval["target"]], output, "filtered-metadata")
        tree, tree_command = command(["cargo", "tree", *common, "--workspace", "--target", approval["target"],
            "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}"], output, "tree")
        rows = full_graph(load_json(metadata), parse_lock(workspace / "Cargo.lock"), tree)
        check_bound_graph(approval, rows, sha256(workspace / "Cargo.lock"), sha256(workspace / "Cargo.toml"))
        policy, admission = scoped_policies(approval)
        write_json(output / "scoped-graph-policy.json", policy)
        write_json(output / "scoped-admission-policy.json", admission)
        result = check_inputs(metadata, tree, workspace / "Cargo.lock", policy, "macos-arm", meta_command,
                              tree_command, filtered, filtered_command, admission, ledger=True)
        write_json(output / "admission.json", result)
        command(["cargo", "fmt", "--manifest-path", str(workspace / "Cargo.toml"), "--all", "--check"], output, "fmt")
        build_messages, build_receipt = command(["cargo", "build", *common, "-p", "aline", "--target",
            approval["target"], "--message-format=json-render-diagnostics"], output, "build")
        app_package = next(package["id"] for package in load_json(metadata)["packages"] if package["name"] == "aline")
        executable, artifact = executable_artifact(build_messages.read_text(encoding="utf-8"), app_package, target, approval["target"])
        require(executable.is_file(), "构建后缺少离屏二进制")
        architectures, _ = command(["lipo", "-archs", str(executable)], output, "architecture")
        check_architectures(architectures.read_text(encoding="utf-8"))
        executable_hash = sha256(executable)
        source_paths = {"app": workspace / "app/src/main.rs", "ui": workspace / "crates/aline-ui/src/lib.rs"}
        source_hashes = {name: sha256(path) for name, path in source_paths.items()}
        require(source_hashes == {"app": sha256(FIXTURE / "app-main.rs"), "ui": sha256(FIXTURE / "aline-ui-lib.rs")}, "本轮编译源码与正式离屏夹具不符")
        write_json(output / "binary-receipt.json", {"artifact": artifact, "command": str(build_receipt),
                   "executable": str(executable), "executable_sha256": executable_hash,
                   "architecture": "arm64", "explicit_target": approval["target"],
                   "source_sha256": source_hashes, "approval_sha256": sha256(FIXTURE / "approval.json"),
                   "lock_sha256": sha256(workspace / "Cargo.lock"), "source_cache_recreated": True,
                   "cargo_build_target_cleared": True})
        samples = []
        for iteration in (1, 2):
            sample = output / ("process-" + str(iteration))
            sample.mkdir()
            command([str(executable), str(sample.resolve())], output, "process-" + str(iteration))
            samples.append(validate_captures(sample))
        require(samples[0]["rgba_sha256"] == samples[1]["rgba_sha256"], "独立两进程的相同场景不稳定")
        require(sha256(executable) == executable_hash and all(sha256(path) == source_hashes[name]
                for name, path in source_paths.items()), "运行期间二进制或夹具源码变化")
        command(["cargo", "clippy", *common, "--workspace", "--all-targets", "--target",
            approval["target"], "--", "-D", "warnings"], output, "clippy")
    require(all(sha256(ROOT / relative) == expected for relative, expected in before.items()), "运行期间正式输入变化")
    summary = {"schema": "aline-w1-offscreen-runtime/v1", "passed": True, "runtime_executed": True,
               "build_profile": "debug", "target": approval["target"], "executable_sha256": sha256(executable),
               "approval": str(FIXTURE / "approval.json"), "candidate_lock_sha256": sha256(workspace / "Cargo.lock"),
               "full_graph_sha256": fingerprint(rows), "samples": samples, "source_inputs_unchanged": True,
               "not_measured": ["屏幕呈现", "真实点击/焦点/IME", "显示缩放变化", "锁屏", "非 macOS arm64 平台", "性能门槛"]}
    write_json(output / "summary.json", summary)
    print("W1 离屏能力通过：两进程各两次真实纹理读取，图与锁精确匹配；不代表真实屏幕或性能验证")


def self_test(output):
    rows = [{"package": "sidecar@1", "selected": True, "features": [], "edges": []}]
    approval = {"schema": "aline-w1-offscreen-approval/v1", "scope": "isolated-offscreen-spike-only",
                "target": "aarch64-apple-darwin", "candidate_lock_sha256": "a", "candidate_root_manifest_sha256": "b",
                "full_graph_sha256": fingerprint(rows), "metadata_packages": 1, "selected_packages": 1}
    tests = []
    check_bound_graph(approval, rows, "a", "b")
    tests.append({"name": "精确完整图通过", "passed": True})
    changes = [("完整 sidecar 特性漂移", [dict(rows[0], features=["extra"])], "a", "b", approval),
               ("完整边漂移", [dict(rows[0], edges=[{"to": "unknown"}])], "a", "b", approval),
               ("本机选包漂移", [dict(rows[0], selected=False)], "a", "b", approval),
               ("候选锁漂移", rows, "bad", "b", approval),
               ("候选清单漂移", rows, "a", "bad", approval),
               ("生产范围拒绝", rows, "a", "b", dict(approval, scope="production")),
               ("其它平台拒绝", rows, "a", "b", dict(approval, target="x86_64-apple-darwin"))]
    for name, changed, lock, manifest, specification in changes:
        try:
            check_bound_graph(specification, changed, lock, manifest)
        except Violation:
            tests.append({"name": name, "passed": True})
        else:
            raise Violation("自测应拒绝：" + name)
    require(sha256(FIXTURE / "Cargo.lock") == load_json(FIXTURE / "approval.json")["candidate_lock_sha256"], "批准锁自检失败")
    tests.append({"name": "正式批准锁身份", "passed": True})
    target = output / "artifact-target"
    triple = "aarch64-apple-darwin"
    artifact = {"reason": "compiler-artifact", "package_id": "aline-package", "target": {"name": "aline", "kind": ["bin"]},
                "profile": {"opt_level": "0", "test": False, "debug_assertions": True},
                "executable": str((target / triple / "debug/aline").resolve())}
    finished = {"reason": "build-finished", "success": True}
    messages = lambda rows: "\n".join(json.dumps(row) for row in rows)
    executable_artifact(messages([artifact, finished]), "aline-package", target, triple)
    tests.append({"name": "本轮显式 target 工件通过", "passed": True})
    invalid_artifacts = [("缺工件拒绝旧路径回退", [finished]), ("重复工件拒绝", [artifact, artifact, finished]),
                         ("错误包工件拒绝", [dict(artifact, package_id="other"), finished]),
                         ("错误 profile 拒绝", [dict(artifact, profile={"opt_level": "3", "test": False, "debug_assertions": False}), finished]),
                         ("错误 target 路径拒绝", [dict(artifact, executable=str((target / "debug/aline").resolve())), finished])]
    for name, rows in invalid_artifacts:
        try:
            executable_artifact(messages(rows), "aline-package", target, triple)
        except Violation:
            tests.append({"name": name, "passed": True})
        else:
            raise Violation("自测应拒绝：" + name)
    check_architectures("arm64\n")
    tests.append({"name": "单一 arm64 架构通过", "passed": True})
    try:
        check_architectures("x86_64 arm64")
    except Violation:
        tests.append({"name": "非批准通用架构拒绝", "passed": True})
    else:
        raise Violation("架构自测应拒绝 universal 工件")
    previous_target = os.environ.get("CARGO_BUILD_TARGET")
    try:
        os.environ["CARGO_BUILD_TARGET"] = "x86_64-apple-darwin"
        with run_environment(output / "unused-cache", target):
            require("CARGO_BUILD_TARGET" not in os.environ, "未清理继承的构建目标")
        require(os.environ.get("CARGO_BUILD_TARGET") == "x86_64-apple-darwin", "构建目标未恢复")
    finally:
        if previous_target is None:
            os.environ.pop("CARGO_BUILD_TARGET", None)
        else:
            os.environ["CARGO_BUILD_TARGET"] = previous_target
    tests.append({"name": "继承目标清理及恢复", "passed": True})
    verify_archive_bytes(b"approved-byte-fixture", hashlib.sha256(b"approved-byte-fixture").hexdigest())
    tests.append({"name": "精确补齐归档 checksum 通过", "passed": True})
    try:
        verify_archive_bytes(b"repacked-or-modified", hashlib.sha256(b"approved-byte-fixture").hexdigest())
    except Violation:
        tests.append({"name": "重打包归档不冒充原始字节", "passed": True})
    else:
        raise Violation("归档自测应拒绝不同字节")
    write_json(output / "self-test.json", {"passed": True, "tests": tests})
    print("self-test: " + str(len(tests)) + " 项通过")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--cargo-home", type=Path, default=Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")))
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--fetch-approved-archives", action="store_true",
                        help="仅补齐固定批准锁中缺失/错误的原始归档，必须匹配 checksum；不修改原缓存")
    args = parser.parse_args()
    try:
        output = output_directory("w1-offscreen", args.output_dir)
        if args.self_test:
            self_test(output)
        else:
            run(output, args.cargo_home, args.fetch_approved_archives)
        return 0
    except (OSError, ValueError, KeyError, Violation) as error:
        if "output" in locals():
            write_json(output / "summary.json", {"schema": "aline-w1-offscreen-runtime/v1", "passed": False, "reason": str(error)})
        print("W1 拒绝/失败：" + str(error))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
