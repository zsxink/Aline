"""输出正式 workspace 与历史 GPUI 候选的具体差异；不自动授予新的例外。"""

import json
from pathlib import Path
from engineering_dependencies import gpui_snapshot, parse_lock, parse_tree, fingerprint, package_key

ROOT = Path(__file__).resolve().parent.parent
OLD = ROOT / "docs/validation/m0-engine-foundation/admission-2026-10-30"
NEW = ROOT / "docs/validation/m0-engine-foundation/group-2/engineering-live-graphs"


def snapshot(metadata, tree, lock):
    data = json.loads(Path(metadata).read_text())
    packages = {p["id"]: p for p in data["packages"]}
    nodes = {n["id"]: n for n in data["resolve"]["nodes"]}
    identities = {package_key(p): p["id"] for p in data["packages"]}
    selected = parse_tree(tree, identities, data["workspace_members"])
    gpui = next(i for i in packages if packages[i]["name"] == "gpui-pre")
    return gpui_snapshot(packages, nodes, gpui, selected, parse_lock(lock))


def main():
    old_policy = json.loads((OLD / "guard_policy.json").read_text())
    mappings = {"all": "base-final", "all-features": "base-final", "macos-arm": "macos-arm", "macos-intel": "macos-intel",
                "windows-x64": "windows-x64", "linux-x64": "linux-x64-both", "linux-arm": "linux-arm-both"}
    audit, scenarios = {}, {}
    for scenario, previous in mappings.items():
        entry = old_policy["scenarios"][previous]
        old = snapshot(OLD / entry["metadata"], OLD / entry["tree"], OLD / "candidate/Cargo.lock")
        new = snapshot(NEW / (scenario + "-metadata.stdout.log"), NEW / (scenario + "-tree.stdout.log"), ROOT / "Cargo.lock")
        o, n = {r["package"]: r for r in old}, {r["package"]: r for r in new}
        audit[scenario] = {"previous_scenario": previous, "added_packages": sorted(n.keys()-o.keys()), "removed_packages": sorted(o.keys()-n.keys()),
                           "changed": {k: {f: {"before": o[k][f], "after": n[k][f]} for f in o[k] if o[k][f] != n[k][f]} for k in sorted(o.keys()&n.keys()) if o[k] != n[k]},
                           "old_sha256": fingerprint(old), "new_sha256": fingerprint(new)}
        scenarios[scenario] = {"gpui_subtree_sha256": fingerprint(new)}
    (NEW.parent / "engineering-baseline-diff.json").write_text(json.dumps(audit, ensure_ascii=False, indent=2) + "\n")
    exceptions = old_policy["exceptions"]
    for key, entry in exceptions.items():
        entry["scenarios"] = ["all", "all-features"] if key.startswith("wasip2@") else list(mappings)
    policy = {"format": 1, "reviewed": False, "review_evidence": "docs/validation/m0-engine-foundation/group-2/engineering-baseline-review.md",
              "purpose": "正式八成员 workspace 的 GPUI 精确子图；不准许自动更新指纹",
              "workspace_features": {name: {} for name in ["aline", "aline-ui", "aline-plugin", "aline-edit", "aline-markdown", "aline-buffer", "aline-source", "aline-app-core"]},
              "scenarios": scenarios, "exceptions": exceptions}
    (ROOT / "scripts/engineering_policy.json").write_text(json.dumps(policy, ensure_ascii=False, indent=2) + "\n")
    print("已生成待审计差异与reviewed=false策略，不能直接作为通过证据")


if __name__ == "__main__":
    main()
