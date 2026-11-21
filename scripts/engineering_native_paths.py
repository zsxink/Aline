"""已裁决 macOS 后端的精确非桌面目标分支；不改变 GPUI 核心指纹。"""

import hashlib
import json
from pathlib import Path
import re

from engineering_dependencies import Violation, normalized_edge, package_key

REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
LOCAL_PATCH = "local-patch"
SCENARIOS = {"all", "all-features", "macos-arm", "macos-intel", "windows-x64", "linux-x64", "linux-arm"}
IDENTITY_FIELDS = {"package", "source", "checksum"}
LOCAL_FIELDS = {"package", "source", "path", "tree_sha256", "files"}
HEX64 = r"[0-9a-f]{64}"


def _sha256(path):
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError as error:
        raise Violation("本地来源文件不可读: %s" % error) from error


def _local_relative(value):
    """本地来源内的相对路径：绝对路径、反斜杠与 . / .. 一律拒绝。"""
    return (isinstance(value, str) and bool(value) and not value.startswith("/") and "\\" not in value
            and all(part not in ("", ".", "..") for part in value.split("/")))


def _vendor_relative(value):
    """登记目录必须是仓库内相对路径，且落在 vendor/ 下。"""
    return _local_relative(value) and value.startswith("vendor/")


def local_patch_spec(spec):
    """校验本地来源精确身份（发布包 + 逐文件锚点 + 全树摘要）并原样返回。

    后端规则与准入账本共用同一套规则，避免两处各写一份而不一致。
    """
    if (not isinstance(spec, dict) or set(spec) != LOCAL_FIELDS
            or not isinstance(spec["package"], str)
            or not re.fullmatch(r"[A-Za-z0-9_-]+@[A-Za-z0-9.+-]+", spec["package"])
            or spec["source"] != LOCAL_PATCH
            or not _vendor_relative(spec["path"])
            or not isinstance(spec["tree_sha256"], str)
            or not re.fullmatch(HEX64, spec["tree_sha256"])
            or not isinstance(spec["files"], dict) or not spec["files"]):
        raise Violation("本地来源精确身份规则无效")
    for path, digest in spec["files"].items():
        if not _local_relative(path) or not isinstance(digest, str) or not re.fullmatch(HEX64, digest):
            raise Violation("本地来源文件身份规则无效")
    return spec


def _tree_rows(root):
    """按相对路径字符串排序遍历；符号链接直接拒绝，避免同一路径两种字节来源。"""
    rows = []
    for path in root.rglob("*"):
        if path.is_symlink():
            raise Violation("本地来源含符号链接")
        if path.is_file():
            rows.append((path.relative_to(root).as_posix(), path))
    return sorted(rows, key=lambda row: row[0])


def _path_components(repo_root, relative):
    """登记路径逐级必须是真实目录；任一级是符号链接即失败关闭。"""
    current = Path(repo_root)
    for part in relative.split("/"):
        current = current / part
        if current.is_symlink():
            raise Violation("本地来源路径含符号链接: " + relative)
    return current


def _evidence_path(value, repo_root):
    """还原证据日志的 <repo> 占位符，并把真实路径解析回来。

    这里不能用 evidence_paths.normalize_text 求同：它的通用规则会把任意
    ``/Users/<用户>/Project/editor/Aline`` 都替换成 ``<repo>``，仓库外同构路径
    因此会与当前仓库不可区分。只允许把 <repo> 还原为当前工作树根；出现其它
    占位符一律失败关闭。
    """
    if not isinstance(value, str) or not value:
        return None
    resolved = value.replace("<repo>", str(Path(repo_root).resolve()))
    if "<" in resolved or ">" in resolved:
        return None
    return Path(resolved).resolve()


def verify_local_identity(package, entry, spec, repo_root):
    """核对 metadata/lock 身份与磁盘字节；路径必须绑定到工作树根，缺少即失败关闭。"""
    local_patch_spec(spec)
    if repo_root is None:
        raise Violation("本地来源身份缺少可核对的工作树根: " + spec["package"])
    root = Path(repo_root) / spec["path"]
    identity = package.get("id")
    # 只做后缀匹配会让“门禁核对仓库内副本、构建使用仓库外副本”通过：
    # metadata id 与 manifest_path 都必须绑定到工作树根下的登记路径。
    prefix = "path+file://"
    if not isinstance(identity, str) or not identity.startswith(prefix) or "#" not in identity:
        raise Violation("本地来源身份/路径漂移: " + spec["package"])
    location, fragment = identity[len(prefix):].split("#", 1)
    if (fragment != spec["package"] or _evidence_path(location, repo_root) != root.resolve()
            or _evidence_path(package.get("manifest_path"), repo_root) != (root / "Cargo.toml").resolve()
            or package.get("source") is not None or entry.get("source") is not None
            or "checksum" in entry):
        raise Violation("本地来源身份/路径漂移: " + spec["package"])
    root = _path_components(repo_root, spec["path"])
    if not root.is_dir():
        raise Violation("本地来源目录缺失: " + spec["path"])
    rows = _tree_rows(root)
    present = {relative for relative, _ in rows}
    for relative, digest in spec["files"].items():
        if relative not in present:
            raise Violation("本地来源已审查文件缺失: " + relative)
    for relative, path in rows:
        expected_file = spec["files"].get(relative)
        if expected_file is not None and _sha256(path) != expected_file:
            raise Violation("本地来源已审查文件漂移: " + relative)
    digest = hashlib.sha256(("\n".join(relative + "\0" + _sha256(path) for relative, path in rows) + "\n").encode()).hexdigest()
    if digest != spec["tree_sha256"]:
        raise Violation("本地来源目录树漂移: " + spec["path"])


def _spec(spec, allow_local=False):
    if isinstance(spec, dict) and spec.get("source") == LOCAL_PATCH:
        if not allow_local:
            raise Violation("本地来源身份不允许出现在该位置")
        local_patch_spec(spec)
        return
    if (not isinstance(spec, dict) or set(spec) != IDENTITY_FIELDS
            or not isinstance(spec["package"], str) or not re.fullmatch(r"[A-Za-z0-9_-]+@[A-Za-z0-9.+-]+", spec["package"])
            or spec["source"] != REGISTRY or not isinstance(spec["checksum"], str)
            or not re.fullmatch(HEX64, spec["checksum"])):
        raise Violation("原生路径精确身份规则无效")


def _identity(packages, lock, spec, repo_root=None):
    name = spec["package"].split("@")[0]
    matches = [ident for ident, package in packages.items() if package["name"] == name]
    # getrandom 等多版本包只按精确 key 选取；同名的错误替代版本仍明确拒绝。
    exact = [ident for ident in matches if package_key(packages[ident]) == spec["package"]]
    if len(exact) != 1:
        raise Violation("原生路径精确身份缺失或不唯一: " + spec["package"])
    ident = exact[0]; package = packages[ident]; entry = lock.get(spec["package"], {})
    if spec["source"] == LOCAL_PATCH:
        verify_local_identity(package, entry, spec, repo_root)
        return ident
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


def check_native_paths(packages, nodes, roots, selected, lock, policy, scenario, repo_root=None):
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
    # 只有 core 允许换成已登记的本地来源；root 与旁支仍必须是 crates.io 精确身份。
    _spec(backend["root"]); _spec(backend["core"], allow_local=True); _edges(backend["incoming"])
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
    root = _identity(packages, lock, backend["root"], repo_root); core = _identity(packages, lock, backend["core"], repo_root)
    sidecars = {_identity(packages, lock, spec, repo_root) for spec in backend["packages"]}
    incoming = [normalized_edge(packages, parent, edge) for parent in nodes for edge in nodes[parent]["deps"] if edge["pkg"] == root]
    _same_edges(incoming, backend["incoming"])
    forward = _walk(nodes, [root], {core})
    if not sidecars.issubset(forward): raise Violation("原生后端旁支不属于获批根路径")
    allowed = set()
    for target_key, rule in rules.items():
        if not any(package["name"] == target_key.split("@")[0] for package in packages.values()):
            continue
        identities = {spec["package"]: _identity(packages, lock, spec, repo_root) for spec in inventories[target_key]}
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
