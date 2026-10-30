"""独立复核依赖图及两个已发现漏洞；只修改内存副本，不改正式 Cargo 清单。

policy.reviewed 仅在本进程副本设真以检查待批准指纹，不构成正式准入批准。
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("review_dependencies", ROOT / "scripts/engineering_dependencies.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)
BASE = Path(__file__).parent / "engineering-dependency-baseline"
policy = json.loads((ROOT / "scripts/engineering_policy.json").read_text())
policy["reviewed"] = True
results = []
for scenario in policy["scenarios"]:
    metadata = BASE / (scenario + "-metadata.stdout.json")
    tree = BASE / (scenario + "-tree.stdout.log")
    if not tree.exists():
        results.append({"scenario": scenario, "result": "未执行：缺少tree"})
        continue
    try:
        value = g.validate(json.loads(metadata.read_text()), tree, ROOT / "Cargo.lock", policy, scenario)
        results.append({"scenario": scenario, "result": "通过", "packages": value["selected_packages"], "gpui_sha256": value["gpui_subtree_sha256"]})
    except Exception as error:
        results.append({"scenario": scenario, "result": "失败", "error": str(error)})

original = json.loads((BASE / "all-metadata.stdout.json").read_text())
packages, nodes, roots, identities = g.model(copy.deepcopy(original), g.parse_lock(ROOT / "Cargo.lock"))
buffer = next(i for i in roots if packages[i]["name"] == "aline-buffer")
source = next(i for i in roots if packages[i]["name"] == "aline-source")
bridge = "path+file:///tmp/aline-review-bridge#0.0.0"
packages[bridge] = {"id": bridge, "name": "review-bridge", "version": "0.0.0", "source": None, "dependencies": [{"name": "aline-source", "rename": None, "kind": None, "target": None}]}
nodes[bridge] = {"id": bridge, "deps": [{"pkg": source, "name": "aline_source", "dep_kinds": [{"kind": None, "target": None}]}], "features": []}
nodes[buffer]["deps"].append({"pkg": bridge, "name": "bridge", "dep_kinds": [{"kind": None, "target": None}]})
try:
    g.check_structure(packages, nodes, roots)
    results.append({"case": "buffer-via-intermediate-to-source", "result": "错误放行"})
except g.Violation as error:
    results.append({"case": "buffer-via-intermediate-to-source", "result": "预期拒绝", "diagnostic": str(error)})

mutated = copy.deepcopy(original)
ui = next(i for i in roots if packages[i]["name"] == "aline-ui")
wasi = next(p["id"] for p in mutated["packages"] if p["name"] == "wasip2")
node = next(n for n in mutated["resolve"]["nodes"] if n["id"] == ui)
node["deps"].append({"pkg": wasi, "name": "review_hidden_wasi", "dep_kinds": [{"kind": None, "target": 'cfg(target_os = "review_only")'}]})
try:
    g.validate(mutated, BASE / "all-tree.stdout.log", ROOT / "Cargo.lock", policy, "all")
    results.append({"case": "ui-to-existing-wasip2", "result": "错误放行"})
except g.Violation as error:
    results.append({"case": "ui-to-existing-wasip2", "result": "预期拒绝", "diagnostic": str(error)})
print(json.dumps({"scope": "内存图变异复核；非真实Cargo夹具，非平台构建", "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "results": results}, ensure_ascii=False, indent=2))
if any(r["result"] in ("失败", "错误放行") or r["result"].startswith("未执行") for r in results):
    sys.exit(1)
