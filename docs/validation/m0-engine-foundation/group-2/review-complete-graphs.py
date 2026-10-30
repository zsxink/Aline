"""独立复核修正后的七种图与精确 GPUI 基线；不改正式 policy 或 Cargo 配置。"""
import copy
import hashlib
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "scripts"))
import check_engineering as entry
import engineering_audit_baseline as audit
import engineering_dependencies as rules

GROUP = Path(__file__).parent
LIVE = GROUP / "engineering-live-graphs"
OLD = ROOT / "docs/validation/m0-engine-foundation/admission-2026-10-30"
policy = json.loads((ROOT / "scripts/engineering_policy.json").read_text())
original_reviewed = policy["reviewed"]
policy["reviewed"] = True
results = []
for scenario, target in entry.SCENARIOS.items():
    try:
        value = entry.check_inputs(LIVE / (scenario + "-metadata.stdout.log"), LIVE / (scenario + "-tree.stdout.log"), ROOT / "Cargo.lock", policy, scenario,
            LIVE / (scenario + "-metadata.command.json"), LIVE / (scenario + "-tree.command.json"),
            LIVE / (scenario + "-target-metadata.stdout.log") if target else None,
            LIVE / (scenario + "-target-metadata.command.json") if target else None)
        results.append({"scenario": scenario, "status": "通过", "selected": value["selected_packages"], "engines": value["engines"], "gpui_sha256": value["gpui_subtree_sha256"]})
    except Exception as error:
        results.append({"scenario": scenario, "status": "失败", "error": str(error)})
old_policy = json.loads((OLD / "guard_policy.json").read_text())
row = old_policy["scenarios"]["base-final"]
old = audit.snapshot(OLD / row["metadata"], OLD / row["tree"], OLD / "candidate/Cargo.lock")
new = audit.snapshot(LIVE / "all-metadata.stdout.log", LIVE / "all-tree.stdout.log", ROOT / "Cargo.lock")
o, n = {r["package"]: r for r in old}, {r["package"]: r for r in new}
changes = {k: {f: {"before": o[k][f], "after": n[k][f]} for f in o[k] if o[k][f] != n[k][f]} for k in sorted(o.keys() & n.keys()) if o[k] != n[k]}
expected = {"toml_datetime@1.1.2+spec-1.1.0": {"features": {"before": ["alloc", "default", "std"], "after": ["alloc", "default", "serde", "std"]}}}
identity_ok = o.keys() == n.keys() and changes == expected
results.append({"case": "历史完整all-target子图→正式完整子图", "status": "通过" if identity_ok else "失败", "added": sorted(n.keys()-o.keys()), "removed": sorted(o.keys()-n.keys()), "changes": changes})
exceptions_ok = True
for name, exception in policy["exceptions"].items():
    for field in ("source", "checksum", "incoming"):
        exceptions_ok = exceptions_ok and exception[field] == old_policy["exceptions"][name][field]
results.append({"case": "精确例外source/checksum/全部入边与已审历史一致", "status": "通过" if exceptions_ok else "失败"})
print(json.dumps({"scope": "只审核已采集图及精确基线；policy.reviewed只在本进程副本设真，不宣称平台构建", "formal_policy_reviewed_before_run": original_reviewed, "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "results": results}, ensure_ascii=False, indent=2))
if any(r["status"] != "通过" for r in results):
    sys.exit(1)
