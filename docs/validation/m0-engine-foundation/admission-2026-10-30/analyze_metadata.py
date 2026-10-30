"""复算隔离候选的路径证据；不是 workspace 工程门禁。"""

import collections
import json
from pathlib import Path


def analyze(directory):
    """保留 metadata 过近似路径，并以 Cargo tree 的选包结果交叉核对。"""
    trees = {
        row["scenario"]: row
        for row in json.loads(
            (directory / "cargo-tree-selected-packages.json").read_text()
        )
    }
    results = []
    for file in sorted(directory.glob("metadata-*.json")):
        if file.name.endswith(".command.json"):
            continue
        command = directory / (file.stem + ".command.json")
        if not command.exists() or json.loads(command.read_text())["exit_code"] != 0:
            continue
        data = json.loads(file.read_text())
        packages = {package["id"]: package for package in data["packages"]}
        nodes = {node["id"]: node for node in data["resolve"]["nodes"]}
        root = data["resolve"]["root"]
        queue = collections.deque([root])
        parents = {root: None}
        while queue:
            current = queue.popleft()
            for edge in nodes[current]["deps"]:
                if edge["pkg"] not in parents:
                    parents[edge["pkg"]] = (current, edge)
                    queue.append(edge["pkg"])
        scenario = file.stem.replace("metadata-", "tree-")
        scenario = {
            "tree-base": "tree-all-base",
            "tree-base-final": "tree-all-base",
            "tree-test-support-resolve": "tree-all-test-support",
        }.get(scenario, scenario)
        tree = trees[scenario]
        selected = {
            (package["name"], package["version"])
            for package in tree["active_packages"]
        }
        hits = []
        for identity in parents:
            package = packages[identity]
            name = package["name"]
            if name not in ("regex", "notify", "tokio", "wasmtime", "wasi") and not name.startswith(
                ("wasmtime-", "wasi-", "wasip1", "wasip2")
            ):
                continue
            path = []
            cursor = identity
            while parents[cursor] is not None:
                previous, edge = parents[cursor]
                path.append(
                    {
                        "from": packages[previous]["name"] + "@" + packages[previous]["version"],
                        "to": packages[cursor]["name"] + "@" + packages[cursor]["version"],
                        "alias": edge["name"],
                        "dep_kinds": edge["dep_kinds"],
                    }
                )
                cursor = previous
            hits.append(
                {
                    "package": name,
                    "version": package["version"],
                    "metadata_shortest_path": list(reversed(path)),
                    "selected_by_cargo_tree": (name, package["version"]) in selected,
                    "metadata_features": nodes[identity]["features"],
                }
            )
        results.append(
            {
                "scenario": file.stem,
                "metadata_reachable_packages": len(parents),
                "cargo_tree_selected_packages": len(selected),
                "findings": hits,
                "admission": "未准入；regex 实际可达，与当前禁令冲突",
                "limits": "仅核查已知包名与最短路径；不穷举路径、不证明未知包用途或五引擎合规",
            }
        )
    return results


if __name__ == "__main__":
    directory = Path(__file__).resolve().parent
    result = analyze(directory)
    (directory / "closure-audit.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    )
    print("已复算", len(result), "组 metadata/tree 交叉核对结果；未判准入通过")
