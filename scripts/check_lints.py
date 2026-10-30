#!/usr/bin/env python3
"""验证 workspace lint 声明和成员继承，并运行隔离的 Rust 诊断样例。"""

import argparse
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
MEMBERS = {
    "aline": "app",
    "aline-app-core": "crates/aline-app-core",
    "aline-buffer": "crates/aline-buffer",
    "aline-edit": "crates/aline-edit",
    "aline-markdown": "crates/aline-markdown",
    "aline-plugin": "crates/aline-plugin",
    "aline-source": "crates/aline-source",
    "aline-ui": "crates/aline-ui",
}
EXPECTED_RUST = {"unsafe_code": "forbid", "missing_docs": "warn"}
EXPECTED_CLIPPY = {
    "unwrap_used": "deny",
    "expect_used": "deny",
    "panic": "deny",
    "unreachable": "deny",
    "cast_possible_truncation": "deny",
    "cast_sign_loss": "deny",
    "dbg_macro": "deny",
    "todo": "deny",
    "unimplemented": "deny",
}


class CheckError(Exception):
    """lint 声明、隔离夹具或实际诊断与预期不符。"""


def tables(path):
    """读取本仓库所用的简单 TOML 键值表，遇到未知语法就拒绝。"""
    result = {}
    current = None
    for line_number, raw_line in enumerate(Path(path).read_text().splitlines(), 1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("[") and line.endswith("]"):
            current = line[1:-1]
            if current in result:
                raise CheckError("重复 TOML 表 %s:%d" % (path, line_number))
            if current in ("workspace.lints.rust", "workspace.lints.clippy", "lints") or current.startswith("lints."):
                result[current] = {}
            continue
        if current not in result:
            continue
        match = re.fullmatch(r'([A-Za-z0-9_-]+)\s*=\s*("([^"\\]*)"|true|false)', line)
        if current is None or match is None:
            raise CheckError("无法安全解释 TOML 行 %s:%d" % (path, line_number))
        value = match.group(3) if match.group(3) is not None else match.group(2) == "true"
        if match.group(1) in result[current]:
            raise CheckError("重复 TOML 键 %s:%d" % (path, line_number))
        result[current][match.group(1)] = value
    return result


def audit_manifests(root=ROOT):
    manifest_tables = tables(root / "Cargo.toml")
    rust_lints = manifest_tables.get("workspace.lints.rust")
    clippy_lints = manifest_tables.get("workspace.lints.clippy")
    if rust_lints != EXPECTED_RUST:
        raise CheckError("根 workspace rust lint 与登记策略不符: " + repr(rust_lints))
    if clippy_lints != EXPECTED_CLIPPY:
        raise CheckError("根 workspace clippy lint 与登记策略不符: " + repr(clippy_lints))
    checked = {}
    for name, relative in MEMBERS.items():
        manifest = root / relative / "Cargo.toml"
        parsed = tables(manifest)
        if parsed.get("lints") != {"workspace": True} or any(key.startswith("lints.") for key in parsed):
            raise CheckError("成员未完整继承 workspace lint: " + name)
        checked[name] = str(manifest.relative_to(root))
    return checked


def audit_member_manifest(name, manifest):
    parsed = tables(manifest)
    if parsed.get("lints") != {"workspace": True} or any(key.startswith("lints.") for key in parsed):
        raise CheckError("成员未完整继承 workspace lint: " + name)


def record(argv, cwd, output, label, env_overrides):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update(env_overrides)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    try:
        result = subprocess.run(argv, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        code, stdout, stderr = result.returncode, result.stdout, result.stderr
    except OSError as error:
        code, stdout, stderr = 127, "", str(error)
    (output / (label + ".stdout.log")).write_text(stdout)
    (output / (label + ".stderr.log")).write_text(stderr)
    metadata = {
        "argv": argv,
        "cwd": str(cwd),
        "started_utc": started,
        "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "exit_code": code,
        "env_overrides": env_overrides,
    }
    (output / (label + ".command.json")).write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
    return code, stdout + stderr, metadata


def check_expected(label, argv, cwd, output, env, expected_code, diagnostic=""):
    code, text, _ = record(argv, cwd, output, label, env)
    needle = diagnostic.replace("_", "-") if diagnostic.startswith("clippy::") else diagnostic
    passed = code == expected_code and needle in text
    print("%s: %s (exit=%d; 预期=%d; 诊断=%s)" % (
        label, "符合预期" if passed else "不符合预期", code, expected_code, diagnostic or "无"), flush=True)
    if not passed:
        raise CheckError("夹具结果不符；查看 " + str(output))


def cargo_toml(root_lints, package_name, edition="2024", inherit=True, workspace_members=None):
    rust_lines = "\n".join('%s = "%s"' % item for item in sorted(root_lints["rust"].items()))
    clippy_lines = "\n".join('%s = "%s"' % item for item in sorted(root_lints["clippy"].items()))
    inheritance = "\n[lints]\nworkspace = true\n" if inherit else ""
    return (
        '[workspace]\nresolver = "3"\nmembers = %s\n'
        '[workspace.package]\nedition = "%s"\n'
        '[workspace.lints.rust]\n%s\n'
        '[workspace.lints.clippy]\n%s\n'
        '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "%s"\n'
        % (json.dumps(workspace_members or [package_name]), edition, rust_lines, clippy_lines, package_name, edition)
    ) + inheritance


def write_fixture(project, package_name, source, root_lints, target=None, inherit=True, edition="2024"):
    member = Path(project) / package_name
    (member / "src").mkdir(parents=True, exist_ok=True)
    workspace_text = cargo_toml(root_lints, package_name, inherit=inherit)
    (Path(project) / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    manifest = member / "Cargo.toml"
    manifest.write_text('[package]\nname = "%s"\nversion = "0.0.0"\nedition = "%s"\n' % (package_name, edition))
    if inherit:
        manifest.write_text(manifest.read_text() + "\n[lints]\nworkspace = true\n")
    if target is None or target == "lib":
        manifest.write_text(manifest.read_text() + "\n[lib]\npath = \"src/lib.rs\"\n")
        (member / "src/lib.rs").write_text(source)
    elif target == "test":
        manifest.write_text(manifest.read_text() + "\n[lib]\npath = \"src/lib.rs\"\n")
        (member / "src/lib.rs").write_text("//! 隔离 lint 样例库。\n")
        (member / "tests").mkdir(exist_ok=True)
        (member / "tests/fixture.rs").write_text(source)
    elif target == "example":
        manifest.write_text(manifest.read_text() + "\n[[example]]\nname = \"fixture\"\npath = \"examples/fixture.rs\"\n")
        (member / "examples").mkdir(exist_ok=True)
        (member / "examples/fixture.rs").write_text(source)
    elif target == "bench":
        manifest.write_text(manifest.read_text() + "\n[[bench]]\nname = \"fixture\"\nharness = false\npath = \"benches/fixture.rs\"\n")
        (member / "benches").mkdir(exist_ok=True)
        (member / "benches/fixture.rs").write_text(source)
    return Path(project) / "Cargo.toml", member


def run_self_test(output):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    actual_members = audit_manifests()
    print("八个正式成员均声明 workspace lint 继承", flush=True)
    root_tables = tables(ROOT / "Cargo.toml")
    root_lints = {"rust": root_tables["workspace.lints.rust"], "clippy": root_tables["workspace.lints.clippy"]}
    cargo_home = os.environ.get("CARGO_HOME")
    if not cargo_home:
        isolation = json.loads((ROOT / "docs/validation/m0-engine-foundation/admission-2026-10-30/isolation.json").read_text())
        cargo_home = isolation["cargo_home"]
    target_dir = str(output / "cargo-target")
    env = {"CARGO_HOME": cargo_home, "CARGO_TARGET_DIR": target_dir, "CARGO_NET_OFFLINE": "true", "RUSTUP_TOOLCHAIN": "1.98.1"}

    version_dir = output / "versions"
    for label, argv in (
        ("cargo-version", ["cargo", "-V"]),
        ("rustc-version", ["rustc", "-Vv"]),
        ("clippy-version", ["cargo", "clippy", "--version"]),
    ):
        code, text, _ = record(argv, ROOT, version_dir, label, env)
        if code != 0:
            raise CheckError("工具版本查询失败: " + label)
        print("%s: %s" % (label, text.splitlines()[0] if text else "(无输出)"), flush=True)

    cases_dir = output / "fixtures"
    cases_dir.mkdir(exist_ok=True)
    inherit_project = cases_dir / "member-inheritance"
    rust = "//! 隔离 lint 继承样例。\n#[allow(dead_code)]\nunsafe fn forbidden() {}\n"
    manifests = []
    inherit_project.mkdir(exist_ok=True)
    workspace_text = cargo_toml(root_lints, "fixture-root", workspace_members=list(MEMBERS))
    (inherit_project / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    for name in MEMBERS:
        member = inherit_project / name
        (member / "src").mkdir(parents=True, exist_ok=True)
        manifest = member / "Cargo.toml"
        manifest.write_text('[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n\n[lints]\nworkspace = true\n' % name)
        (member / "src/lib.rs").write_text(rust)
        manifests.append((name, manifest))
    for name, _ in manifests:
        argv = ["cargo", "check", "--manifest-path", str(inherit_project / "Cargo.toml"), "--package", name, "--offline"]
        check_expected("inherit-unsafe-" + name, argv, inherit_project, output / "results", env, 101, "unsafe-code")

    # 每个目标单独建最小 workspace，使错误的目标源码只参与本次诊断。
    samples = [
        ("positive-assert-const", "//! 允许预期测试断言和编译期只读常量的正例。\n/// 编译期只读实体名。\npub const ENTITY_NAMES: [&str; 1] = [\"amp\"];\n#[cfg(test)] mod tests { #[test] fn assertion_is_test_expectation() { let left = 1; let right = 1; assert_eq!(left, right); } }\n", "lib", 0, ""),
        ("negative-unwrap-lib", "//! 隔离违规样例。\n/// 故意触发 unwrap lint。\npub fn forbidden(value: Option<u8>) -> u8 { value.unwrap() }\n", "lib", 101, "clippy::unwrap_used"),
        ("negative-unwrap-test", "//! 隔离违规样例。\n#[test] fn forbidden() { let value = Some(1_u8); let _copy = value.unwrap(); }\n", "test", 101, "clippy::unwrap_used"),
        ("negative-unwrap-example", "//! 隔离违规样例。\nfn main() { let value = Some(1_u8); let _copy = value.unwrap(); }\n", "example", 101, "clippy::unwrap_used"),
        ("negative-unwrap-bench", "//! 隔离违规样例。\nfn main() { let value = Some(1_u8); let _copy = value.unwrap(); }\n", "bench", 101, "clippy::unwrap_used"),
        ("negative-expect", "//! 隔离违规样例。\n/// 故意触发 expect lint。\npub fn forbidden(value: Option<u8>) -> u8 { value.expect(\"fixture\") }\n", "lib", 101, "clippy::expect_used"),
        ("negative-panic", "//! 隔离违规样例。\n/// 故意触发 panic lint。\npub fn forbidden() { panic!(\"fixture\") }\n", "lib", 101, "clippy::panic"),
        ("negative-unreachable", "//! 隔离违规样例。\n/// 故意触发 unreachable lint。\npub fn forbidden() -> u8 { unreachable!() }\n", "lib", 101, "clippy::unreachable"),
        ("negative-coordinate-cast-truncation", "//! 隔离源码/绘制坐标窄化样例。\n/// 故意把绘制字节偏移窄化。\npub fn invalid_drawn_offset(drawn_off: u64) -> u16 { drawn_off as u16 }\n", "lib", 101, "clippy::cast_possible_truncation"),
        ("negative-cast-sign-loss", "//! 隔离违规样例。\n/// 故意触发符号损失 lint。\npub fn forbidden(value: i32) -> usize { value as usize }\n", "lib", 101, "clippy::cast_sign_loss"),
    ]
    profiles = {"debug": [], "release": ["--release"]}
    for label, source, target, expected, diagnostic in samples:
        project = cases_dir / label
        manifest, _ = write_fixture(project, "lint-fixture", source, root_lints, target)
        for profile, profile_args in profiles.items():
            argv = ["cargo", "clippy", "--manifest-path", str(manifest), "--offline", "--all-targets"] + profile_args + ["--", "-D", "warnings"]
            check_expected(label + "-" + profile, argv, project, output / "results", env, expected, diagnostic)

    unsafe_source = "//! 隔离违规样例。\n#![allow(unsafe_code)]\n/// 故意尝试降低 forbid。\npub unsafe fn forbidden() {}\n"
    unsafe_project = cases_dir / "negative-unsafe-allow"
    unsafe_manifest, _ = write_fixture(unsafe_project, "lint-fixture", unsafe_source, root_lints)
    check_expected("unsafe-forbid-cannot-be-lowered", ["cargo", "check", "--manifest-path", str(unsafe_manifest), "--offline"], unsafe_project, output / "results", env, 101, "forbid")

    missing_project = cases_dir / "negative-member-inheritance"
    missing_manifest, _ = write_fixture(missing_project, "aline-source", "//! 漏继承夹具。\npub fn ordinary() {}\n", root_lints, inherit=False)
    try:
        audit_member_manifest("aline-source", missing_manifest)
        raise CheckError("成员未继承 workspace lint 时守卫未拒绝")
    except CheckError as error:
        if "未完整继承" not in str(error):
            raise
    # 错误配置的 Cargo 负例明确编译无 lint 源码；该结果不冒充 lint 诊断。
    code, text, _ = record(["cargo", "check", "--manifest-path", str(missing_manifest), "--offline"], missing_project, output / "results", "missing-inherit-cargo-control", env)
    if code != 0:
        raise CheckError("漏继承控制样例的普通源码没有通过 Cargo，无法隔离继承检查")

    complete_negative = cases_dir / "negative-one-member-inheritance"
    complete_negative.mkdir(exist_ok=True)
    workspace_text = cargo_toml(root_lints, "fixture-root", workspace_members=list(MEMBERS.values()))
    (complete_negative / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    for name, relative in MEMBERS.items():
        member = complete_negative / relative
        (member / "src").mkdir(parents=True, exist_ok=True)
        member_manifest = member / "Cargo.toml"
        member_manifest.write_text('[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n' % name)
        if name != "aline-source":
            member_manifest.write_text(member_manifest.read_text() + "\n[lints]\nworkspace = true\n")
        (member / "src/lib.rs").write_text("//! 成员漏继承隔离反例。\n")
    check_expected("negative-one-member-inheritance-guard",
                   ["python3", str(Path(__file__).resolve()), "--root", str(complete_negative)],
                   complete_negative, output / "results", env, 1, "成员未完整继承 workspace lint: aline-source")

    print(json.dumps({"status": "lint 隔离样例通过", "formal_members": actual_members, "self_test_cases": len(samples), "profiles": sorted(profiles)}, ensure_ascii=False))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path, default=ROOT, help="待审 Cargo workspace 根目录")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "docs/validation/m0-engine-foundation/group-2/lint-self-test")
    args = parser.parse_args()
    try:
        if args.self_test:
            return run_self_test(args.output_dir)
        members = audit_manifests(args.root.resolve())
        print(json.dumps({"status": "workspace lint 声明有效", "members": members}, ensure_ascii=False, indent=2))
        return 0
    except (CheckError, OSError, ValueError, KeyError) as error:
        print("lint 检查拒绝: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
