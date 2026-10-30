"""正式 workspace 依赖规则；只消费 Cargo 图，不处理源码安全或 catalog。

所有权在工程守卫：完整成员图检查方向与引擎闭包，实际 tree 限定当前组合选包。
GPUI 精确例外按完整子图、全入边和多根支配性核对，metadata 噪声不会被静默放行。
"""

import collections
import hashlib
import json
from pathlib import Path
import re

MEMBERS = {"aline", "aline-ui", "aline-plugin", "aline-edit", "aline-markdown", "aline-buffer", "aline-source", "aline-app-core"}
ENGINES = {"aline-source", "aline-buffer", "aline-markdown", "aline-edit", "aline-app-core"}
CHAIN = ["aline", "aline-ui", "aline-plugin", "aline-edit", "aline-markdown", "aline-buffer"]
FORBIDDEN = {"notify", "tokio", "regex", "regex-automata", "regex-syntax", "wasmtime", "wasi", "wasip1", "wasip2"}
PARSERS = {"comrak", "markdown", "markdown-it", "markdown-it-rust", "markdown-rs", "cmark", "commonmark", "hoedown"}


class Violation(Exception):
    """图无效、证据不足或违反当前工程规则。"""


def load_json(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError) as error:
        raise Violation("JSON 输入缺失或解析失败 %s: %s" % (path, error)) from error


def package_key(package):
    return package["name"] + "@" + package["version"]


def forbidden_name(name):
    return name in FORBIDDEN or name.startswith(("wasmtime-", "wasi-"))


def gpui_name(name):
    return name in ("gpui", "gpui-pre") or name.startswith("gpui-pre-")


def parse_lock(path):
    try:
        text = Path(path).read_text()
    except OSError as error:
        raise Violation("Cargo.lock 不可读: %s" % error) from error
    packages = {}
    for block in text.split("[[package]]")[1:]:
        entry = {}
        for line in block.splitlines():
            match = re.fullmatch(r'(name|version|source|checksum) = (".*")', line)
            if match:
                entry[match[1]] = json.loads(match[2])
        if not {"name", "version"}.issubset(entry):
            raise Violation("Cargo.lock package 缺少身份")
        key = package_key(entry)
        if key in packages:
            raise Violation("Cargo.lock 同名同版本来源歧义: " + key)
        packages[key] = entry
    if not packages:
        raise Violation("Cargo.lock 没有 package")
    return packages


def reachable(nodes, roots, selected=None, omit=None):
    found = set()
    queue = collections.deque(roots)
    while queue:
        identity = queue.popleft()
        if identity == omit or identity in found or (selected is not None and identity not in selected):
            continue
        found.add(identity)
        queue.extend(edge["pkg"] for edge in nodes[identity]["deps"])
    return found


def route(packages, nodes, roots, target):
    """最短路径仅用于诊断，不能用作例外授权依据。"""
    queue = collections.deque(roots)
    previous = {root: None for root in roots}
    while queue:
        identity = queue.popleft()
        if identity == target:
            result = [package_key(packages[identity])]
            while previous[identity] is not None:
                parent, edge = previous[identity]
                kinds = ",".join((kind.get("kind") or "normal") + ("@" + kind["target"] if kind.get("target") else "") for kind in edge["dep_kinds"])
                result.append("--%s[%s]-->" % (edge["name"], kinds))
                result.append(package_key(packages[parent]))
                identity = parent
            return " ".join(reversed(result))
        for edge in nodes[identity]["deps"]:
            if edge["pkg"] not in previous:
                previous[edge["pkg"]] = (identity, edge)
                queue.append(edge["pkg"])
    return package_key(packages[target])


def normalized_edge(packages, parent, edge):
    return {"from": package_key(packages[parent]), "to": package_key(packages[edge["pkg"]]), "alias": edge["name"],
            "kinds": sorted(edge["dep_kinds"], key=lambda kind: (kind.get("kind") or "", kind.get("target") or ""))}


def model(data, lock):
    try:
        packages = {p["id"]: p for p in data["packages"]}
        nodes = {n["id"]: n for n in data["resolve"]["nodes"]}
        roots = data["workspace_members"]
        if not roots or len(roots) != 8 or len(set(roots)) != len(roots):
            raise Violation("workspace 必须有八个不重复成员")
        names = {packages[root]["name"] for root in roots}
        if names != MEMBERS:
            raise Violation("workspace 成员实际包名不符: " + str(sorted(names)))
        if data["resolve"].get("root") not in (None, *roots):
            raise Violation("resolve.root 不属于 workspace")
        identities = {}
        for identity, package in packages.items():
            key = package_key(package)
            if key in identities:
                raise Violation("metadata/tree 同名同版本来源歧义: " + key)
            identities[key] = identity
            if identity not in nodes:
                raise Violation("package 缺少 resolve node: " + key)
            entry = lock.get(key)
            if entry is None or entry.get("source") != package.get("source"):
                raise Violation("完整图 lock/metadata 来源或身份不符: " + key)
            if package.get("source") and not re.fullmatch(r"[0-9a-f]{64}", entry.get("checksum", "")):
                raise Violation("完整图 registry checksum 无效: " + key)
            if package["name"] in MEMBERS and identity not in roots:
                raise Violation("内部成员包名被外部包冒用: " + key)
            for edge in nodes[identity]["deps"]:
                if edge["pkg"] not in packages or edge["pkg"] not in nodes or not edge.get("dep_kinds"):
                    raise Violation("metadata 边缺少真实 package/node/kind")
        if any(root not in nodes for root in roots):
            raise Violation("workspace root 缺少 resolve node")
        return packages, nodes, roots, identities
    except (KeyError, TypeError, ValueError) as error:
        raise Violation("metadata 结构错误: %s" % error) from error


def parse_tree(path, identities, roots):
    try:
        lines = Path(path).read_text().splitlines()
    except OSError as error:
        raise Violation("tree 不可读: %s" % error) from error
    selected = set()
    for line in lines:
        if not line.strip():
            continue
        match = re.match(r"^([A-Za-z0-9_-]+) v([^\s]+)(?:\s|$)", line)
        key = (match[1] + "@" + match[2]) if match else ""
        if not key or key not in identities:
            raise Violation("tree 行不能对应 metadata: " + line[:150])
        selected.add(identities[key])
    if not selected or not set(roots).issubset(selected):
        raise Violation("tree 为空或未包含全部八成员")
    return selected


def allowed_internal(parent, child):
    if child == "aline-app-core":
        return parent != child
    if child == "aline-source":
        return parent in ("aline", "aline-ui")
    return parent in CHAIN and child in CHAIN and CHAIN.index(parent) < CHAIN.index(child)


def check_structure(packages, nodes, roots):
    for parent in roots:
        for edge in nodes[parent]["deps"]:
            if edge["pkg"] in roots and not allowed_internal(packages[parent]["name"], packages[edge["pkg"]]["name"]):
                raise Violation("内部反向/非法依赖边: " + json.dumps(normalized_edge(packages, parent, edge), ensure_ascii=False))
        # declaration 检查保留未激活的内部 optional/target 边；外部闭包另用 all-features 实际解析。
        for dependency in packages[parent].get("dependencies", []):
            if dependency["name"] in MEMBERS and not allowed_internal(packages[parent]["name"], dependency["name"]):
                raise Violation("内部声明非法边: %s -> %s alias=%s kind=%s target=%s" % (packages[parent]["name"], dependency["name"], dependency.get("rename"), dependency.get("kind"), dependency.get("target")))
    full = reachable(nodes, roots)
    for identity in full:
        if packages[identity]["name"] == "gpui":
            raise Violation("禁止替代 GPUI 来源: " + route(packages, nodes, roots, identity))
    for root in roots:
        if packages[root]["name"] not in ENGINES:
            continue
        for identity in reachable(nodes, [root]):
            name = packages[identity]["name"]
            if gpui_name(name) or forbidden_name(name) or name in PARSERS:
                raise Violation("五引擎完整闭包泄漏: " + route(packages, nodes, [root], identity))
    return full


def gpui_snapshot(packages, nodes, gpui, selected, lock):
    rows = []
    for identity in sorted(reachable(nodes, [gpui]), key=lambda identity: package_key(packages[identity])):
        package = packages[identity]
        rows.append({"package": package_key(package), "source": package.get("source"), "checksum": lock[package_key(package)].get("checksum"),
                     "features": sorted(nodes[identity]["features"]), "selected": identity in selected,
                     "edges": sorted([normalized_edge(packages, identity, edge) for edge in nodes[identity]["deps"]], key=lambda edge: json.dumps(edge, sort_keys=True))})
    return rows


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def validate(data, tree, lock_path, policy, scenario):
    lock = parse_lock(lock_path)
    packages, nodes, roots, identities = model(data, lock)
    full = check_structure(packages, nodes, roots)
    selected = parse_tree(tree, identities, roots)
    if reachable(nodes, roots, selected) != selected:
        raise Violation("metadata/tree 选包可达性不一致")
    gpuis = [identity for identity in full if packages[identity]["name"] == "gpui-pre"]
    if len(gpuis) > 1:
        raise Violation("GPUI 多版本或多来源")
    gpui = gpuis[0] if gpuis else None
    outside = reachable(nodes, roots, selected, omit=gpui) if gpui else selected
    gpui_full = reachable(nodes, [gpui]) if gpui else set()
    incoming = collections.defaultdict(list)
    for parent in selected:
        for edge in nodes[parent]["deps"]:
            if edge["pkg"] in selected:
                incoming[edge["pkg"]].append(normalized_edge(packages, parent, edge))
    for identity in selected:
        name = packages[identity]["name"]
        if name in PARSERS:
            raise Violation("第二 Markdown 解析器: " + route(packages, nodes, roots, identity))
        if not forbidden_name(name):
            continue
        key = package_key(packages[identity])
        if not gpui or identity in outside:
            raise Violation("禁库或 GPUI 例外旁路: " + route(packages, nodes, roots, identity))
        exception = policy["exceptions"].get(key)
        if exception is None:
            raise Violation("无精确依赖例外: " + key)
        if packages[identity].get("source") != exception["source"] or lock[key].get("checksum") != exception["checksum"]:
            raise Violation("精确例外来源/校验和漂移: " + key)
        if scenario not in exception["scenarios"]:
            raise Violation("精确例外目标组合不允许: " + key)
        for edge in incoming[identity]:
            if edge not in exception["incoming"]:
                raise Violation("精确例外新增入边/target/kind: " + json.dumps(edge, ensure_ascii=False))
    for identity in full - selected - gpui_full:
        name = packages[identity]["name"]
        if forbidden_name(name) or gpui_name(name) or name in PARSERS:
            raise Violation("未经审查的 metadata-only 自有禁库路径: " + route(packages, nodes, roots, identity))
    if gpui:
        expected = policy["scenarios"].get(scenario)
        if expected is None or not policy.get("reviewed"):
            raise Violation("正式 GPUI 子图尚未独立批准基线: " + scenario)
        rows = gpui_snapshot(packages, nodes, gpui, selected, lock)
        if fingerprint(rows) != expected["gpui_subtree_sha256"]:
            raise Violation("正式 GPUI 子图身份/来源/features/边/选包漂移，须重新审计")
    return {"scenario": scenario, "workspace_members": sorted(packages[root]["name"] for root in roots), "selected_packages": len(selected),
            "metadata_packages": len(packages), "metadata_unselected": sorted(package_key(packages[i]) for i in full-selected),
            "engines": {packages[root]["name"]:len(reachable(nodes,[root])) for root in roots if packages[root]["name"] in ENGINES},
            "gpui_subtree_sha256":fingerprint(gpui_snapshot(packages,nodes,gpui,selected,lock)) if gpui else None}
