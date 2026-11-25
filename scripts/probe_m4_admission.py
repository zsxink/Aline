#!/usr/bin/env python3
"""M4 后端与平台适配准入探针：在工作区副本上重跑七场景工程门禁，把冻结的
准入期望（`scripts/m4_admission_rules.py`）与实测裁决逐项比对。

用法：

    python3 scripts/probe_m4_admission.py --self-test
    python3 scripts/probe_m4_admission.py --offline
    python3 scripts/probe_m4_admission.py --offline --groups baseline,svg-direct

`--self-test` 只校验规则层与策略快照，不复制工作区、不执行 cargo。真实运行：

1. 把八个 workspace 成员、`vendor/` 与四份根清单复制到临时目录；
2. 按分组把候选直接依赖写进副本清单（每组都从原始清单重新开始）；
3. 逐场景执行 `cargo metadata` / `cargo tree`，用**门禁自己的**
   `engineering_dependencies.validate()` 裁决；
4. 用 gate 自己的 `gpui_snapshot()`/`fingerprint()` 重算 GPUI 受审子图指纹，
   使被指纹检查拒绝的组也能给出**拟批准**的新指纹；
5. 逐项比对新增包身份（校验和/许可证/rust-version/来源）、新增禁库入边、
   绕过获批根的原生目标路径与逐场景裁决，写 `summary.json` 与 `report.md`。

为什么不走 `check_engineering.py --output-dir`：该入口把 argv 与路径归一化成
`<tmp>`，而本地来源身份只允许还原 `<repo>`，工作区副本必然报“本地来源身份/
路径漂移”。那是入口缺陷，不是依赖结论。本探针直接以原始 metadata/tree/lock
调 `validate()`，覆盖的正是要回答的图身份、原生路径、GPUI 指纹与运行时分组
四层。

为什么不用 `engineering_dependencies.reachable(omit=…)` 做原生路径判断：
`omit` 只接受**单个身份**，传集合时阻断失效。原生路径一律交给
`engineering_native_paths.check_native_paths()`，它内部用集合做阻断。

本脚本不改工作区依赖、不运行构建脚本、不开始任何生产接线；证据只写
`logs/validation/` 内。
"""

import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))

import engineering_dependencies as ed  # noqa: E402
from dependency_admission import license_review  # noqa: E402
from engineering_native_paths import _walk as native_walk  # 阻断参数是集合，语义正确
from evidence_paths import display_path, write_json, write_text  # noqa: E402
import m4_admission_rules as rules  # noqa: E402
from validation_output import ROOT, output_directory  # noqa: E402

WORKSPACE_FILES = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml")
WORKSPACE_TREES = ("app", "crates", "vendor")
CONSUMER_MANIFESTS = ("Cargo.toml", "crates/aline-plugin/Cargo.toml",
                      "crates/aline-ui/Cargo.toml")
# 工作区清单里候选依赖的注入锚点；要求逐字出现且仅一次。
ANCHOR = "\n# 已准入的本地来源"
GPUI_UI_ANCHOR = "gpui.workspace = true\n"

SCENARIO_TARGET = {
    "all": None,
    "macos-arm": "aarch64-apple-darwin",
    "macos-intel": "x86_64-apple-darwin",
    "windows-x64": "x86_64-pc-windows-msvc",
    "linux-x64": "x86_64-unknown-linux-gnu",
    "linux-arm": "aarch64-unknown-linux-gnu",
    "all-features": None,
}


class ProbeError(RuntimeError):
    """探针无法完成测量；调用方非零退出并保留已写证据。"""


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run_cargo(args, cwd, env, timeout=1800):
    return subprocess.run(["cargo"] + args, cwd=cwd, text=True, capture_output=True,
                          env=env, timeout=timeout)


def policy_snapshot():
    """核对门禁策略文件是否仍与冻结表同源。"""
    rows = []
    for relative, expected in rules.POLICY_SHA256.items():
        path = ROOT / relative
        actual = sha256_file(path) if path.is_file() else None
        rows.append({"path": relative, "expected": expected, "actual": actual,
                     "match": actual == expected})
    return rows


def load_policies():
    return (json.loads((ROOT / "scripts/engineering_policy.json").read_text()),
            json.loads((ROOT / "scripts/dependency_admission_policy.json").read_text()))


# ---------------------------------------------------------------- 工作区副本

def prepare(work):
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    for name in WORKSPACE_FILES:
        shutil.copy2(ROOT / name, work / name)
    for name in WORKSPACE_TREES:
        shutil.copytree(ROOT / name, work / name,
                        ignore=shutil.ignore_patterns("target"))
    for relative in CONSUMER_MANIFESTS:
        shutil.copy2(work / relative, work / (relative + ".orig"))
    return work


def reset(work):
    for relative in CONSUMER_MANIFESTS:
        shutil.copy2(work / (relative + ".orig"), work / relative)


def manifest_line(spec):
    """分组里的依赖项既可以是冻结的直接依赖身份，也可以是字面清单行。"""
    if spec in rules.DIRECT:
        return rules.DIRECT[spec]["line"]
    if " = " in spec:
        return spec
    raise ProbeError("分组依赖既不在 DIRECT 中，也不是清单行: " + spec)


def apply_group(work, group):
    item = rules.GROUPS[group]
    reset(work)

    if item["workspace_deps"]:
        path = work / "Cargo.toml"
        text = path.read_text()
        if text.count(ANCHOR) != 1:
            raise ProbeError("工作区清单注入锚点不唯一: " + ANCHOR)
        block = "\n".join(manifest_line(spec) for spec in item["workspace_deps"])
        path.write_text(text.replace(ANCHOR, "\n" + block + ANCHOR, 1))

    if item["plugin_deps"]:
        path = work / "crates/aline-plugin/Cargo.toml"
        text = path.read_text()
        if "[dependencies]" in text:
            raise ProbeError("aline-plugin 清单已出现 [dependencies]，注入方式需更新")
        path.write_text(text + "\n[dependencies]\n"
                        + "".join("%s.workspace = true\n" % name
                                  for name in item["plugin_deps"]))

    if item["ui_deps"]:
        path = work / "crates/aline-ui/Cargo.toml"
        text = path.read_text()
        if text.count(GPUI_UI_ANCHOR) != 1:
            raise ProbeError("aline-ui 清单 gpui 锚点不唯一")
        path.write_text(text.replace(
            GPUI_UI_ANCHOR,
            GPUI_UI_ANCHOR + "".join("%s.workspace = true\n" % name
                                     for name in item["ui_deps"]), 1))


# ---------------------------------------------------------------- 采集

def read_metadata(work, env, all_features=False, evidence=None):
    args = ["metadata", "--format-version", "1"]
    if all_features:
        args.append("--all-features")
    done = run_cargo(args, work, env)
    if done.returncode != 0:
        raise ProbeError("cargo metadata 失败:\n" + done.stderr[-4000:])
    if evidence is not None:
        write_text(evidence, done.stdout)
    return json.loads(done.stdout)


def read_tree(work, env, scenario, evidence):
    args = ["tree", "--workspace", "--target", SCENARIO_TARGET[scenario] or "all",
            "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}"]
    if scenario == "all-features":
        args.append("--all-features")
    done = run_cargo(args, work, env)
    if done.returncode != 0:
        raise ProbeError("cargo tree 失败 (%s):\n%s" % (scenario, done.stderr[-4000:]))
    write_text(evidence, done.stdout)
    return evidence


# ---------------------------------------------------------------- 事实提取

def identity_names(packages):
    return {identity: ed.package_key(package) for identity, package in packages.items()}


def new_packages(packages, lock, baseline_keys):
    rows = []
    for identity, package in sorted(packages.items(),
                                    key=lambda item: ed.package_key(item[1])):
        key = ed.package_key(package)
        if key in baseline_keys:
            continue
        locked = lock.get(key, {})
        frozen = rules.PACKAGES.get(key)
        rows.append({
            "package": key,
            "source": package.get("source"),
            "checksum": locked.get("checksum"),
            "license": package.get("license"),
            "rust_version": package.get("rust_version"),
            "frozen": frozen is not None,
            "checksum_match": frozen is not None
            and frozen["checksum"] == locked.get("checksum"),
            "license_match": frozen is not None
            and frozen["license"] == package.get("license"),
            "rust_version_match": frozen is not None
            and frozen["rust_version"] == package.get("rust_version"),
            "source_match": package.get("source") == rules.SOURCE,
        })
    return rows


def license_facts(packages, admission_policy, baseline_keys):
    rows = []
    for identity, package in sorted(packages.items(),
                                    key=lambda item: ed.package_key(item[1])):
        key = ed.package_key(package)
        if key in baseline_keys:
            continue
        try:
            outcome = license_review(package, admission_policy["license"], False)
        except ed.Violation as error:
            rows.append({"package": key, "ok": False, "error": str(error)})
        else:
            rows.append({"package": key, "ok": True, "verdict": outcome["verdict"],
                         "normalized": outcome.get("normalized")})
    return rows


def forbidden_edges(packages, nodes):
    """metadata 全图中“非禁库父包 → 禁库子包”的边；target/kind 一并记录。"""
    names = identity_names(packages)
    rows = set()
    for parent, node in nodes.items():
        if ed.forbidden_name(packages[parent]["name"]):
            continue
        for edge in node["deps"]:
            if not ed.forbidden_name(packages[edge["pkg"]]["name"]):
                continue
            rows.add((names[parent], names[edge["pkg"]],
                      json.dumps(sorted(edge["dep_kinds"],
                                        key=lambda kind: (kind.get("kind") or "",
                                                          kind.get("target") or "")),
                                 sort_keys=True)))
    return [{"from": row[0], "to": row[1], "kinds": json.loads(row[2])}
            for row in sorted(rows)]


def route_avoiding(packages, nodes, roots, target, blocked):
    """绕开 blocked 的最短路径；`ed.route` 会给出穿过 GPUI 的更短路线，不能定位旁路。"""
    previous = {root: None for root in roots if root not in blocked}
    queue = collections.deque(previous)
    while queue:
        identity = queue.popleft()
        if identity == target:
            parts = [ed.package_key(packages[identity])]
            while previous[identity] is not None:
                parent, edge = previous[identity]
                kinds = ",".join((kind.get("kind") or "normal")
                                 + ("@" + kind["target"] if kind.get("target") else "")
                                 for kind in edge["dep_kinds"])
                parts.append("--%s[%s]-->" % (edge["name"], kinds))
                parts.append(ed.package_key(packages[parent]))
                identity = parent
            return " ".join(reversed(parts))
        for edge in nodes[identity]["deps"]:
            child = edge["pkg"]
            if child in blocked or child in previous:
                continue
            previous[child] = (identity, edge)
            queue.append(child)
    return ed.package_key(packages[target])


def outside_routes(packages, nodes, roots, selected):
    """绕过原生后端根（gpui-pre-macos）与 core（gpui-pre）仍可达的 wasi/wasip2。

    阻断集合由门禁自己的 `_walk` 消费；`engineering_dependencies.reachable` 的
    `omit` 只接受单身份，传集合会静默失效，故不在此使用。
    """
    names = identity_names(packages)
    blocked = {identity for identity, package in packages.items()
               if package["name"] in ("gpui-pre", "gpui-pre-macos")}
    return [{"package": names[identity],
             "selected": identity in selected,
             "route": route_avoiding(packages, nodes, roots, identity, blocked)}
            for identity in sorted(native_walk(nodes, roots, blocked),
                                   key=lambda item: names[item])
            if packages[identity]["name"] in ("wasi", "wasip1", "wasip2")]


def drift(rows, base_rows):
    index = {row["package"]: row for row in rows}
    base = {row["package"]: row for row in base_rows}
    return (sorted(set(index) - set(base)),
            sorted(key for key in set(index) & set(base) if index[key] != base[key]))


def check_outside(facts, expected, scenario):
    index = {row["package"]: row for row in facts}
    if sorted(index) != sorted(item["package"] for item in expected):
        return False
    for item in expected:
        row = index[item["package"]]
        # 只有桌面 target tree 才证明“未被选包”；`all`/`all-features` 会选上 target 分支。
        if (scenario not in ("all", "all-features")
                and row["selected"] != item["selected_by_desktop_trees"]):
            return False
        if item["target_condition"] not in row["route"]:
            return False
        position = 0
        for name in item["route_contains"]:
            position = row["route"].find(name, position)
            if position < 0:
                return False
            position += len(name)
    return True


# ---------------------------------------------------------------- 分组测量

def measure(work, group, out, env, engineering, admission, baseline):
    """返回 (事实, 基线状态)。基线组自己就是参照系。"""
    apply_group(work, group)
    data = read_metadata(work, env, evidence=out / ("%s.metadata.json" % group))
    data_features = (read_metadata(work, env, all_features=True,
                                   evidence=out / ("%s.all-features-metadata.json" % group))
                     if "all-features" in rules.SCENARIOS else None)
    lock_path = work / "Cargo.lock"
    shutil.copy2(lock_path, out / ("%s.Cargo.lock" % group))
    lock = ed.parse_lock(lock_path)
    packages, nodes, roots, identities = ed.model(data, lock)
    names = identity_names(packages)
    baseline_keys = baseline["packages"] if baseline is not None else set(names.values())

    gpui_ids = [identity for identity in packages if packages[identity]["name"] == "gpui-pre"]
    if len(gpui_ids) != 1:
        raise ProbeError("GPUI 根身份不唯一: " + group)
    gpui = gpui_ids[0]

    edges_all = forbidden_edges(packages, nodes)
    baseline_edges = baseline["forbidden_edges"] if baseline is not None else set()

    facts = {
        "group": group,
        "workspace_deps": list(rules.GROUPS[group]["workspace_deps"]),
        "plugin_deps": list(rules.GROUPS[group]["plugin_deps"]),
        "ui_deps": list(rules.GROUPS[group]["ui_deps"]),
        "metadata_packages": len(packages),
        "new_packages": new_packages(packages, lock, baseline_keys),
        "licenses": license_facts(packages, admission, baseline_keys),
        "forbidden_edges_all": edges_all,
        "forbidden_edges": [] if baseline is None else
        [edge for edge in edges_all
         if (edge["from"], edge["to"]) not in baseline_edges],
        "scenarios": {},
    }
    scenario_rows = {}

    for scenario in rules.SCENARIOS:
        tree_path = read_tree(work, env, scenario,
                              out / ("%s-%s.tree.txt" % (group, scenario)))
        try:
            selected = ed.parse_tree(tree_path, identities, roots)
        except ed.Violation as error:
            facts["scenarios"][scenario] = {"verdict": "tree-failed",
                                            "message": str(error), "match": False}
            continue
        rows = ed.gpui_snapshot(packages, nodes, gpui, selected, lock)
        scenario_rows[scenario] = rows
        fingerprint = ed.fingerprint(rows)
        if baseline is None:
            row_match, new_rows, changed_rows = True, [], []
        else:
            new_rows, changed_rows = drift(rows, baseline["rows"][scenario])
            expected_new, expected_changed = rules.GROUPS[group]["gpui_drift"][scenario]
            row_match = (new_rows == sorted(expected_new)
                         and changed_rows == sorted(expected_changed))
        outside = outside_routes(packages, nodes, roots, selected)
        outside_match = check_outside(outside, rules.GROUPS[group]["outside_paths"],
                                      scenario)
        outside_error = None

        expected = rules.GROUPS[group]["expected_verdicts"][scenario]
        try:
            result = ed.validate(data_features if scenario == "all-features" else data,
                                 tree_path, lock_path, engineering, scenario,
                                 admission, False)
        except ed.Violation as error:
            actual = rules.verdict_class(str(error))
            message = str(error)
            result = None
        else:
            actual, message = rules.PASS, None

        prefix = rules.expected_rejection(group, scenario)
        matched = (actual == rules.PASS) if prefix is None else (
            actual == expected and message is not None and message.startswith(prefix))
        entry = {
            "selected_packages": len(selected),
            "gpui_subtree_sha256": fingerprint,
            "gpui_new_rows": new_rows,
            "gpui_changed_rows": changed_rows,
            "gpui_rows_match": row_match,
            "outside_native": outside,
            "outside_match": outside_match,
            "outside_error": outside_error,
            "verdict": actual,
            "expected": expected,
            "message": message,
            "match": matched and row_match and outside_match,
        }
        if result is not None and expected == rules.PASS:
            entry["approved_fingerprint_match"] = (
                fingerprint == rules.APPROVED_GPUI_FINGERPRINTS[scenario])
            entry["selected_metadata"] = result["selected_packages"]
        facts["scenarios"][scenario] = entry

    expected_keys = sorted(rules.GROUPS[group]["new_packages"])
    facts["expectation_match"] = (
        sorted(row["package"] for row in facts["new_packages"]) == expected_keys
        and all(row["checksum_match"] and row["license_match"]
                and row["rust_version_match"] and row["source_match"]
                for row in facts["new_packages"])
        and all(row["ok"] for row in facts["licenses"])
        and [(row["from"], row["to"]) for row in facts["forbidden_edges"]]
        == sorted(rules.GROUPS[group]["forbidden_edges"])
        and all(row["match"] for row in facts["scenarios"].values())
    )
    return facts, {"packages": set(names.values()), "rows": scenario_rows,
                   "forbidden_edges": {(edge["from"], edge["to"]) for edge in edges_all}}


# ---------------------------------------------------------------- 自测

def self_test():
    """不执行 cargo 的规则层自测；返回失败项列表。"""
    failures = []

    def check(name, condition, detail=""):
        if not condition:
            failures.append("%s: %s" % (name, detail))

    for group, item in rules.GROUPS.items():
        check("group-verdicts", set(item["expected_verdicts"]) == set(rules.SCENARIOS), group)
        for scenario, verdict in item["expected_verdicts"].items():
            check("group-verdict-class", verdict in rules.VERDICTS,
                  "%s/%s=%s" % (group, scenario, verdict))

    referenced = set()
    for item in rules.GROUPS.values():
        referenced |= set(item["new_packages"])
    check("packages-covered", referenced == set(rules.PACKAGES),
          "未登记=%s 多余=%s" % (sorted(referenced - set(rules.PACKAGES)),
                                sorted(set(rules.PACKAGES) - referenced)))
    for key, item in rules.PACKAGES.items():
        check("checksum-format", bool(re.fullmatch(r"[0-9a-f]{64}", item["checksum"])), key)
        check("license-present", bool(item["license"]), key)

    for key, item in rules.DIRECT.items():
        check("direct-key", key in rules.PACKAGES or key in rules.BASELINE_DIRECT, key)
        check("direct-new", not (key in rules.PACKAGES and key in rules.BASELINE_DIRECT), key)
        check("direct-consumer", item["consumer"] in ("aline-plugin", "aline-ui"), key)
        check("direct-status", item["status"] in ("admissible-now", "needs-approval",
                                                  "needs-license-ruling",
                                                  "needs-guard-ruling"), key)
        check("direct-necessity", key in rules.NECESSITY, key)
        check("feature-not-default", "default" not in item["features"], key)

    used = set()
    for item in rules.GROUPS.values():
        used |= set(item["workspace_deps"])
    direct_keys = {key for key in used if key in rules.DIRECT}
    check("direct-all-used", direct_keys == set(rules.DIRECT),
          "未使用=%s" % sorted(set(rules.DIRECT) - direct_keys))

    direct = lambda key: rules.DIRECT.get(key, {})
    check("docx-no-image", "image" not in direct("docx-rs@0.4.22")["features"],
          "docx-rs 必须关闭 image")
    check("mermaid-no-png", "png" not in direct("mermaid-rs-renderer@0.3.1")["features"],
          "mermaid 不得启用 png（会带入 usvg/resvg 0.47）")
    check("syntect-no-onig", "regex-onig" not in direct("syntect@5.3.0")["features"],
          "syntect 不得启用 onig")
    check("latex-no-png", "png" not in direct("latex-rust@2.1.1")["features"],
          "latex-rust 不得启用 tiny-skia png feature")
    check("zip-backend", "deflate-flate2-zlib-rs" in direct("zip@8.6.0")["features"],
          "zip 必须显式选择 flate2 后端")

    union = (set(rules.GROUPS["krilla"]["new_packages"])
             | set(rules.GROUPS["docx-plain"]["new_packages"])
             | set(rules.GROUPS["latex"]["new_packages"]))
    check("full-plain-union", union == set(rules.GROUPS["full-plain"]["new_packages"]),
          "差集=%s" % sorted(union ^ set(rules.GROUPS["full-plain"]["new_packages"])))

    for item in rules.RULINGS_REQUIRED:
        for candidate in item["candidates"]:
            check("ruling-candidate", candidate in rules.PACKAGES,
                  item["id"] + "/" + candidate)

    admission = json.loads((ROOT / "scripts/dependency_admission_policy.json").read_text())
    allowed = set(admission["license"]["allowed"])
    for licensed in rules.EMBEDDED_ASSET_LICENSES:
        check("embedded-not-allowed", licensed not in allowed,
              "%s 已在白名单，EMBEDDED_ASSETS 的待裁决理由失效" % licensed)

    for key, item in rules.PACKAGES.items():
        name, version = rules.split_key(key)
        package = {"name": name, "version": version, "source": rules.SOURCE,
                   "license": item["license"]}
        try:
            license_review(package, admission["license"], False)
        except ed.Violation as error:
            failures.append("license-review: %s: %s" % (key, error))

    for item in rules.PLATFORM_ADAPTERS:
        check("platform-no-dependency", item["dependency"] is None, item["capability"])

    for verdict, prefix in rules.REJECTION_PREFIX.items():
        try:
            actual = rules.verdict_class(prefix)
        except rules.ContractError as error:
            failures.append("verdict-roundtrip: %s" % error)
        else:
            if actual != verdict:
                failures.append("verdict-roundtrip: %s → %s" % (prefix, actual))

    for row in policy_snapshot():
        check("policy-snapshot", row["match"], row["path"])
    return failures


# ---------------------------------------------------------------- 报告

def environment_facts():
    def version(command):
        done = subprocess.run(command, text=True, capture_output=True)
        return done.stdout.strip() if done.returncode == 0 else None
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": sys.version.split()[0],
        "cargo": version(["cargo", "-V"]),
        "rustc": version(["rustc", "-V"]),
    }


def render_report(summary):
    lines = ["# M4 准入探针报告", ""]
    verified = all(row["match"] for row in summary["policy_snapshot"])
    lines.append("策略快照：%s" % ("与冻结表同源" if verified
                                   else "**不一致（期望需重新冻结）**"))
    lines.append("")
    lines.append("## 逐组裁决")
    lines.append("")
    lines.append("| 分组 | 新增包 | 预期 | 实测（七场景） | 期望一致 |")
    lines.append("|---|---|---|---|---|")
    for group, item in summary["groups"].items():
        verdicts = sorted({row["verdict"] for row in item["scenarios"].values()})
        expected = sorted({row["expected"] for row in item["scenarios"].values()})
        lines.append("| %s | %d | %s | %s | %s |" % (
            group, len(item["new_packages"]), ",".join(expected), ",".join(verdicts),
            "是" if item["expectation_match"] else "**否**"))
    lines.append("")
    lines.append("## 新增包身份")
    lines.append("")
    lines.append("| 包 | 校验和 | 许可证 | rust-version | 与冻结表一致 |")
    lines.append("|---|---|---|---|---|")
    for group, item in summary["groups"].items():
        if group == "baseline":
            continue
        lines.append("| **%s** | | | | |" % group)
        for row in item["new_packages"]:
            lines.append("| %s | `%s` | %s | %s | %s |" % (
                row["package"], row["checksum"], row["license"] or "—",
                row["rust_version"] or "—",
                "是" if (row["checksum_match"] and row["license_match"]
                         and row["rust_version_match"] and row["source_match"]) else "**否**"))
    lines.append("")
    lines.append("## GPUI 受审子图指纹")
    lines.append("")
    lines.append("| 分组 | " + " | ".join(rules.SCENARIOS) + " |")
    lines.append("|---|" + "---|" * len(rules.SCENARIOS))
    for group, item in summary["groups"].items():
        if group == "baseline":
            continue
        cells = ["`%s`" % item["scenarios"][scenario]["gpui_subtree_sha256"][:16]
                 for scenario in rules.SCENARIOS]
        lines.append("| %s | %s |" % (group, " | ".join(cells)))
    lines.append("")
    lines.append("完整指纹见 `summary.json` 的 `scenarios[*].gpui_subtree_sha256`。")
    lines.append("")
    lines.append("## 待用户裁决")
    lines.append("")
    for item in summary["rulings_required"]:
        lines.append("- **%s**：%s" % (item["id"], item["question"]))
    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------- 入口

def main(argv=None):
    parser = argparse.ArgumentParser(description="M4 后端与平台适配准入探针")
    parser.add_argument("--self-test", action="store_true", help="只跑规则层自测")
    parser.add_argument("--output-dir", default=None,
                        help="证据目录（必须在 logs/validation 内）")
    parser.add_argument("--groups", default=None, help="逗号分隔的分组子集")
    parser.add_argument("--offline", action="store_true", help="强制 cargo 离线")
    parser.add_argument("--keep-work", action="store_true", help="保留临时工作区副本")
    parser.add_argument("--work-dir", default=None, help="指定临时工作区副本路径")
    args = parser.parse_args(argv)

    if args.self_test:
        failures = self_test()
        for failure in failures:
            print("SELFTEST_FAIL " + failure)
        if failures:
            print("SELFTEST_FAILED count=%d" % len(failures))
            return 1
        print("SELFTEST_OK groups=%d packages=%d scenarios=%d"
              % (len(rules.GROUPS), len(rules.PACKAGES), len(rules.SCENARIOS)))
        return 0

    groups = list(rules.GROUPS)
    if args.groups:
        groups = [name.strip() for name in args.groups.split(",") if name.strip()]
        unknown = [name for name in groups if name not in rules.GROUPS]
        if unknown:
            raise ProbeError("未知分组: " + ", ".join(unknown))
        if "baseline" not in groups:
            groups.insert(0, "baseline")

    output = output_directory("m4-admission-review", args.output_dir)
    env = dict(os.environ)
    if args.offline:
        env["CARGO_NET_OFFLINE"] = "true"
    snapshot = policy_snapshot()
    if not all(row["match"] for row in snapshot):
        # 策略已变还按旧期望裁决，会把“策略更新”误报成“门禁行为变化”。
        raise ProbeError("门禁策略已变化，先重新冻结 m4_admission_rules.py 的期望与快照")

    work = Path(args.work_dir).resolve() if args.work_dir else Path(
        tempfile.mkdtemp(prefix="aline-m4-admission-"))
    prepare(work)
    engineering, admission = load_policies()
    baseline = None
    facts = {}
    try:
        for group in groups:
            print("group %s ..." % group, flush=True)
            item, state = measure(work, group, output, env, engineering, admission,
                                  baseline)
            facts[group] = item
            if group == "baseline":
                baseline = state
            print("group %s verdicts=%s match=%s" % (
                group,
                sorted({row["verdict"] for row in item["scenarios"].values()}),
                item["expectation_match"]), flush=True)
    finally:
        if not args.keep_work:
            shutil.rmtree(work, ignore_errors=True)

    summary = {
        "generated_by": "scripts/probe_m4_admission.py",
        "cargo_offline": bool(args.offline),
        "scenarios": list(rules.SCENARIOS),
        "gate_order": list(rules.GATE_ORDER),
        "policy_snapshot": snapshot,
        "approved_fingerprints": rules.APPROVED_GPUI_FINGERPRINTS,
        "environment": environment_facts(),
        "groups": facts,
        "rulings_required": list(rules.RULINGS_REQUIRED),
        "platform_adapters": list(rules.PLATFORM_ADAPTERS),
        "embedded_assets": list(rules.EMBEDDED_ASSETS),
    }
    write_json(output / "summary.json", summary)
    write_text(output / "report.md", render_report(summary))
    failed = [group for group, item in facts.items() if not item["expectation_match"]]
    print("PROBE_%s groups=%d failed=%d output=%s" % (
        "OK" if not failed else "FAILED", len(facts), len(failed), display_path(output)))
    return 0 if not failed else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ProbeError, rules.ContractError) as error:
        print("PROBE_ERROR " + str(error), file=sys.stderr)
        sys.exit(2)
