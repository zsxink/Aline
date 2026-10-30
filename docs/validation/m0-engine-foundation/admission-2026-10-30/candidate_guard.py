"""第 1 组隔离候选检查；不是正式八成员 workspace 工程守卫。

责任是核对留存 metadata、实际 Cargo tree 与 lock。metadata 中未选包仍被报告；
对已选包之间的 metadata 边保守检查，不能证明某边未启用时可能拒绝，不静默放行。
"""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
ENGINE_NAMES = {"aline-source", "aline-buffer", "aline-markdown", "aline-edit", "aline-app-core"}
FORBIDDEN = {"notify", "regex", "regex-automata", "regex-syntax", "tokio", "wasmtime", "wasi", "wasip1", "wasip2"}
PARSERS = {"comrak", "markdown", "markdown-it", "markdown-it-rust", "markdown-rs", "cmark", "commonmark", "hoedown"}


class Refusal(Exception):
    """输入缺失/不一致或违反准入约束。"""


def read_json(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError) as error:
        raise Refusal("无法解析 JSON %s: %s" % (path, error)) from error


def lock_packages(path):
    """只解析 Cargo.lock package 标量身份，拒绝歧义，不自行解析依赖表达式。"""
    try:
        source = Path(path).read_text()
    except OSError as error:
        raise Refusal("无法读取 Cargo.lock: %s" % error) from error
    result = {}
    for block in source.split("[[package]]")[1:]:
        item = {}
        for line in block.splitlines():
            match = re.fullmatch(r'(name|version|source|checksum) = (".*")', line)
            if match:
                item[match[1]] = json.loads(match[2])
        if not {"name", "version"}.issubset(item):
            raise Refusal("lock package 缺少身份")
        key = item["name"] + "@" + item["version"]
        if key in result:
            raise Refusal("lock 同名同版本来源歧义: " + key)
        result[key] = item
    if not result:
        raise Refusal("lock 没有 package 记录")
    return result


def key(package):
    return package["name"] + "@" + package["version"]


def graph_inputs(metadata, tree_path, lock_path):
    data = read_json(metadata)
    try:
        packages = {p["id"]: p for p in data["packages"]}
        nodes = {n["id"]: n for n in data["resolve"]["nodes"]}
        root = data["resolve"]["root"]
        if not packages or root not in packages or root not in nodes:
            raise Refusal("metadata 缺少有效 root/resolve")
        tuples = {}
        for identity, package in packages.items():
            pair = (package["name"], package["version"])
            if pair in tuples:
                raise Refusal("tree 名称/版本无法区分多个来源: " + key(package))
            tuples[pair] = identity
        text = Path(tree_path).read_text()
        if not text.strip():
            raise Refusal("Cargo tree 为空，不能当合规")
        selected = set()
        for line in text.splitlines():
            match = re.match(r"^([A-Za-z0-9_-]+) v([^\s]+)(?:\s|$)", line)
            if not match or (match[1], match[2]) not in tuples:
                raise Refusal("Cargo tree 行不能对应 metadata: " + line[:140])
            selected.add(tuples[(match[1], match[2])])
        if root not in selected:
            raise Refusal("Cargo tree 未包含 root")
        for identity, node in nodes.items():
            if identity not in packages:
                raise Refusal("resolve 节点没有 package")
            for edge in node["deps"]:
                if edge["pkg"] not in nodes or not edge.get("dep_kinds"):
                    raise Refusal("resolve 边缺少节点或 kind/target")
        locks = lock_packages(lock_path)
        for identity in selected:
            package = packages[identity]
            entry = locks.get(key(package))
            if entry is None or entry.get("source") != package.get("source"):
                raise Refusal("lock 与 metadata 身份/来源不符: " + key(package))
            if package.get("source") and not re.fullmatch(r"[0-9a-f]{64}", entry.get("checksum", "")):
                raise Refusal("registry 身份缺少 lock 校验和: " + key(package))
        return packages, nodes, root, selected, locks
    except (KeyError, TypeError, OSError, ValueError) as error:
        raise Refusal("metadata/tree/lock 结构错误: %s" % error) from error


def reachable(nodes, roots, allowed=None, remove=None):
    found = set()
    queue = collections.deque(roots)
    while queue:
        current = queue.popleft()
        if current == remove or current in found or (allowed is not None and current not in allowed):
            continue
        found.add(current)
        queue.extend(edge["pkg"] for edge in nodes[current]["deps"])
    return found


def edge_record(packages, previous, edge):
    return {"from": key(packages[previous]), "to": key(packages[edge["pkg"]]), "alias": edge["name"],
            "kinds": sorted(edge["dep_kinds"], key=lambda d: (d.get("kind") or "", d.get("target") or ""))}


def subtree_record(packages, nodes, gpui, selected, locks):
    """完整 GPUI metadata 子图也锁定，避免未审 features 或噪声被当成新例外。"""
    records = []
    for identity in sorted(reachable(nodes, [gpui]), key=lambda identity: key(packages[identity])):
        package = packages[identity]
        entry = locks.get(key(package))
        if entry is None:
            raise Refusal("GPUI 子图包不在 lock: " + key(package))
        if entry.get("source") != package.get("source"):
            raise Refusal("完整 GPUI 子图 lock/metadata 来源不符: " + key(package))
        if package.get("source") and not re.fullmatch(r"[0-9a-f]{64}", entry.get("checksum", "")):
            raise Refusal("完整 GPUI 子图 registry 校验和无效: " + key(package))
        records.append({"package": key(package), "source": package.get("source"), "checksum": entry.get("checksum"),
                        "features": sorted(nodes[identity]["features"]), "selected": identity in selected,
                        "edges": sorted([edge_record(packages, identity, edge) for edge in nodes[identity]["deps"]],
                                        key=lambda edge: json.dumps(edge, sort_keys=True))})
    return records


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def check(metadata, tree, lock, scenario, scope, policy_path=HERE / "guard_policy.json"):
    packages, nodes, root, selected, locks = graph_inputs(metadata, tree, lock)
    policy = read_json(policy_path)
    if scenario not in policy["scenarios"]:
        raise Refusal("未审组合: " + scenario)
    alternatives = [key(packages[i]) for i in selected if packages[i]["name"] == "gpui"]
    if alternatives:
        raise Refusal("禁止替代 GPUI 包；仅允许锁定 gpui-pre 来源: " + ", ".join(alternatives))
    gpuis = [identity for identity in selected if packages[identity]["name"] == "gpui-pre"]
    if len(gpuis) > 1:
        raise Refusal("GPUI 多身份")
    gpui = gpuis[0] if gpuis else None
    if scope == "engine" and gpui:
        raise Refusal("五引擎禁止 GPUI: " + key(packages[gpui]))
    incoming = collections.defaultdict(list)
    for previous in selected:
        for edge in nodes[previous]["deps"]:
            if edge["pkg"] in selected:
                incoming[edge["pkg"]].append(edge_record(packages, previous, edge))
    reached = reachable(nodes, [root], selected)
    if reached != selected:
        raise Refusal("tree 选包与 metadata root 可达性不一致")
    outside_gpui = reachable(nodes, [root], selected, remove=gpui) if gpui else reached
    full_outside_gpui = reachable(nodes, [root], remove=gpui)
    hidden = full_outside_gpui - selected
    if hidden:
        raise Refusal("自有路径 metadata/tree 选包差异未经审查: " + ", ".join(sorted(key(packages[i]) for i in hidden)))
    for identity in sorted(selected):
        package = packages[identity]
        name = package["name"]
        if name in PARSERS:
            raise Refusal("第二 Markdown 解析器: " + key(package))
        banned = name in FORBIDDEN or name.startswith(("wasmtime-", "wasi-"))
        if not banned:
            continue
        if scope == "engine" or not gpui or identity in outside_gpui:
            raise Refusal("禁用包/GPUI 旁路: %s，入边 %s" % (key(package), json.dumps(incoming[identity], ensure_ascii=False)))
        permitted = policy["exceptions"].get(key(package))
        if not permitted:
            raise Refusal("无精确例外: " + key(package))
        actual = {"source": package.get("source"), "checksum": locks[key(package)].get("checksum")}
        if any(actual[k] != permitted[k] for k in actual):
            raise Refusal("例外来源/校验和漂移: " + key(package))
        for edge in incoming[identity]:
            if edge not in permitted["incoming"]:
                raise Refusal("例外新入边/target/kind 漂移: " + json.dumps(edge, ensure_ascii=False))
        if scenario not in permitted["scenarios"]:
            raise Refusal("例外不允许该目标/feature 组合: " + key(package))
    for identity in reached:
        if packages[identity]["name"] in ENGINE_NAMES:
            for child in reachable(nodes, [identity], selected):
                name = packages[child]["name"]
                if name == "gpui-pre" or name in FORBIDDEN or name in PARSERS or name.startswith(("wasmtime-", "wasi-")):
                    raise Refusal("实际五引擎旁路: %s -> %s" % (key(packages[identity]), key(packages[child])))
    baseline = policy["scenarios"][scenario]
    if bool(gpui) != baseline["has_gpui"]:
        raise Refusal("组合 GPUI 存在性漂移")
    if gpui:
        record = subtree_record(packages, nodes, gpui, selected, locks)
        if digest(record) != baseline["gpui_subtree_sha256"]:
            raise Refusal("受审 GPUI 子图身份/版本/features/边/选包漂移，必须重新审计")
    noise = sorted(key(package) for identity, package in packages.items() if identity not in selected)
    return {"status": "该候选组合通过", "scenario": scenario, "selected_packages": len(selected),
            "metadata_unselected_packages": noise, "limits": "完整 metadata 已保留；已选包之间的边保守核对，无法消除的过近似边拒绝；不是正式 workspace 或平台验证"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", required=True)
    parser.add_argument("--tree", required=True, help="--prefix none --format '{p}' -e normal,build,dev 的成功输出")
    parser.add_argument("--lock", required=True)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--scope", choices=("app", "engine"), required=True)
    parser.add_argument("--policy", default=str(HERE / "guard_policy.json"))
    parser.add_argument("--metadata-command", help="默认读取 metadata 同名 command.json")
    parser.add_argument("--tree-command", help="默认读取 tree 同名 command.json")
    args = parser.parse_args()
    try:
        records = [args.metadata_command or str(Path(args.metadata).with_suffix(".command.json")),
                   args.tree_command or str(Path(args.tree).with_name(Path(args.tree).name.replace(".stdout.log", ".command.json")))]
        for path in records:
            record = read_json(path)
            if not isinstance(record, dict) or record.get("exit_code") != 0 or not isinstance(record.get("argv"), list):
                raise Refusal("Cargo 采集命令未成功或缺少命令记录: " + path)
        result = check(args.metadata, args.tree, args.lock, args.scenario, args.scope, args.policy)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    except (Refusal, KeyError, TypeError, ValueError) as error:
        print("拒绝: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
