"""已裁决 macOS 后端的精确非桌面目标分支；不改变 GPUI 核心指纹。"""

import json
import re

from engineering_dependencies import Violation, normalized_edge, package_key

REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
SCENARIOS = {"all", "all-features", "macos-arm", "macos-intel", "windows-x64", "linux-x64", "linux-arm"}


def _spec(spec):
    if (not isinstance(spec, dict) or set(spec) != {"package", "source", "checksum"}
            or not isinstance(spec["package"], str) or not re.fullmatch(r"[A-Za-z0-9_-]+@[A-Za-z0-9.+-]+", spec["package"])
            or spec["source"] != REGISTRY or not isinstance(spec["checksum"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", spec["checksum"])):
        raise Violation("原生路径精确身份规则无效")


def _identity(packages, lock, spec):
    name = spec["package"].split("@")[0]
    matches = [ident for ident, package in packages.items() if package["name"] == name]
    # getrandom 等多版本包只按精确 key 选取；同名的错误替代版本仍明确拒绝。
    exact = [ident for ident in matches if package_key(packages[ident]) == spec["package"]]
    if len(exact) != 1:
        raise Violation("原生路径精确身份缺失或不唯一: " + spec["package"])
    ident = exact[0]; package = packages[ident]; entry = lock.get(spec["package"], {})
    if (package.get("id") != spec["source"] + "#" + spec["package"]
            or package.get("source") != spec["source"] or entry.get("source") != spec["source"]
            or entry.get("checksum") != spec["checksum"]):
        raise Violation("原生路径精确身份/来源/校验和漂移: " + spec["package"])
    return ident


def _edges(edges):
    if not isinstance(edges, list) or not edges:
        raise Violation("原生路径入边规则缺失")
    for edge in edges:
        if (not isinstance(edge, dict) or set(edge) != {"from", "to", "alias", "kinds"}
                or any(not isinstance(edge[key], str) or not edge[key] for key in ("from", "to", "alias"))
                or not isinstance(edge["kinds"], list) or not edge["kinds"]):
            raise Violation("原生路径入边规则无效")
        for kind in edge["kinds"]:
            if (not isinstance(kind, dict) or set(kind) != {"kind", "target"}
                    or kind["kind"] not in (None, "build", "dev")
                    or (kind["target"] is not None and not isinstance(kind["target"], str))):
                raise Violation("原生路径入边 target/kind 规则无效")


def _same_edges(actual, expected):
    encode = lambda edge: json.dumps(edge, sort_keys=True)
    if sorted(map(encode, actual)) != sorted(map(encode, expected)):
        raise Violation("原生路径新增路径/入边/alias/target/kind")


def _walk(nodes, starts, blocked):
    found, pending = set(), list(starts)
    while pending:
        ident = pending.pop()
        if ident in found or ident in blocked:
            continue
        found.add(ident); pending.extend(edge["pkg"] for edge in nodes[ident]["deps"])
    return found


def _graph_rule(keys, edges, root, target):
    """精确图只能登记根到目标的完整无环路径区域，不能夹带无关节点或边。"""
    adjacency = {key: set() for key in keys}; reverse = {key: set() for key in keys}
    encoded = set()
    for edge in edges:
        text = json.dumps(edge, sort_keys=True)
        if edge["from"] not in adjacency or edge["to"] not in adjacency or text in encoded:
            raise Violation("原生目标分支图路径规则含未知节点或重复边")
        encoded.add(text); adjacency[edge["from"]].add(edge["to"]); reverse[edge["to"]].add(edge["from"])
    def walk(graph, start):
        found, pending = set(), [start]
        while pending:
            key = pending.pop()
            if key not in found: found.add(key); pending.extend(graph[key])
        return found
    inventory = set(keys)
    if walk(adjacency, root) != inventory or walk(reverse, target) != inventory:
        raise Violation("原生目标分支图路径规则含未使用或缺失的节点/边")
    degrees = {key: len(parents) for key, parents in reverse.items()}
    pending = [key for key, degree in degrees.items() if degree == 0]; visited = 0
    while pending:
        key = pending.pop(); visited += 1
        for child in adjacency[key]:
            degrees[child] -= 1
            if degrees[child] == 0: pending.append(child)
    if visited != len(keys):
        raise Violation("原生目标分支图路径规则不得含环")


def _rule(rule, target, root_key):
    common = {"edges", "selected_scenarios", "unselected_scenarios", "basis"}
    if not isinstance(rule, dict):
        raise Violation("原生目标分支规则格式无效")
    shape = "path" if set(rule) == common | {"path"} else "packages" if set(rule) == common | {"packages"} else None
    if (shape is None or not isinstance(rule[shape], list) or len(rule[shape]) < 2
            or not isinstance(rule["basis"], str) or not rule["basis"]):
        raise Violation("原生目标分支规则格式无效")
    for spec in rule[shape]: _spec(spec)
    keys = [spec["package"] for spec in rule[shape]]
    if (len(set(keys)) != len(keys) or root_key not in keys or target not in keys
            or target.split("@")[0] not in {"wasi", "wasip2"}):
        raise Violation("原生目标分支路径身份规则无效")
    _edges(rule["edges"])
    if shape == "path":
        if (keys[0] != root_key or keys[-1] != target or len(rule["edges"]) != len(keys) - 1
                or any(edge["from"] != keys[i] or edge["to"] != keys[i + 1] for i, edge in enumerate(rule["edges"]))):
            raise Violation("原生目标分支路径规则不连续")
    else:
        _graph_rule(keys, rule["edges"], root_key, target)
    for key in ("selected_scenarios", "unselected_scenarios"):
        scenarios = rule[key]
        if (not isinstance(scenarios, list) or not scenarios or len(set(scenarios)) != len(scenarios)
                or not set(scenarios).issubset(SCENARIOS)):
            raise Violation("原生目标分支场景规则无效")
    if not set(rule["selected_scenarios"]).issubset({"all", "all-features"}):
        raise Violation("原生目标分支实际选包场景不得含桌面 target")
    return rule[shape]


def check_native_paths(packages, nodes, roots, selected, lock, policy, scenario):
    """返回可跳过旧 core-only 支配检查的精确身份，旧指纹仍由调用者核对。"""
    empty = {"forbidden": set(), "sidecars": set()}
    backend = policy.get("native_backend")
    rules = policy.get("native_path_exceptions", {})
    if backend is None:
        if rules: raise Violation("原生目标分支缺少后端根规则")
        return empty
    if (not isinstance(backend, dict) or set(backend) != {"root", "core", "packages", "incoming"}
            or not isinstance(backend["packages"], list)):
        raise Violation("原生后端规则格式无效")
    _spec(backend["root"]); _spec(backend["core"]); _edges(backend["incoming"])
    for spec in backend["packages"]: _spec(spec)
    if ({spec["package"].split("@")[0] for spec in backend["packages"]} != {"gpui-pre-macos", "gpui-pre-apple"}
            or len(backend["packages"]) != 2 or backend["root"] not in backend["packages"]):
        raise Violation("原生后端旁支身份规则无效")
    if not isinstance(rules, dict): raise Violation("原生目标分支规则必须为映射")
    inventories = {target: _rule(rule, target, backend["root"]["package"]) for target, rule in rules.items()}
    root_name = backend["root"]["package"].split("@")[0]
    if not any(package["name"] == root_name for package in packages.values()):
        if any(package["name"] == "gpui-pre-apple" for package in packages.values()):
            raise Violation("原生后端旁支缺少获批根身份")
        return empty
    root = _identity(packages, lock, backend["root"]); core = _identity(packages, lock, backend["core"])
    sidecars = {_identity(packages, lock, spec) for spec in backend["packages"]}
    incoming = [normalized_edge(packages, parent, edge) for parent in nodes for edge in nodes[parent]["deps"] if edge["pkg"] == root]
    _same_edges(incoming, backend["incoming"])
    forward = _walk(nodes, [root], {core})
    if not sidecars.issubset(forward): raise Violation("原生后端旁支不属于获批根路径")
    allowed = set()
    for target_key, rule in rules.items():
        if not any(package["name"] == target_key.split("@")[0] for package in packages.values()):
            continue
        identities = {spec["package"]: _identity(packages, lock, spec) for spec in inventories[target_key]}
        target = identities[target_key]
        scenarios = rule["selected_scenarios"] if target in selected else rule["unselected_scenarios"]
        if scenario not in scenarios:
            qualifier = "实际选包场景" if target in selected else "metadata-only 场景"
            raise Violation("原生目标分支" + qualifier + "未批准: " + target_key)
        if target in _walk(nodes, roots, {root, core}):
            raise Violation("原生目标分支出现获批根以外路径: " + target_key)
        ancestors, pending = {target}, [target]
        reverse = {}
        for parent in forward:
            for edge in nodes[parent]["deps"]:
                if edge["pkg"] in forward: reverse.setdefault(edge["pkg"], []).append(parent)
        while pending:
            for parent in reverse.get(pending.pop(), []):
                if parent not in ancestors: ancestors.add(parent); pending.append(parent)
        actual = [normalized_edge(packages, parent, edge) for parent in forward & ancestors
                  for edge in nodes[parent]["deps"] if edge["pkg"] in ancestors]
        _same_edges(actual, rule["edges"])
        allowed.add(target)
    return {"forbidden": allowed, "sidecars": sidecars}
