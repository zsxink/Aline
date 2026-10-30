"""复核最后一次自测的原始命令、输出摘要和最小checksum反例，无新增测试范围。"""
import datetime
import difflib
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[4]
GROUP = Path(__file__).parent
SELF = GROUP / "engineering-self-test-reviewed"
summary = json.loads((SELF / "summary.json").read_text())
failures = []
for row in summary["records"]:
    path = Path(row["command_record"])
    record = json.loads(path.read_text())
    stem = path.name.removesuffix(".command.json")
    stdout = path.with_name(stem + ".stdout.log")
    stderr = path.with_name(stem + ".stderr.log")
    if record["exit_code"] != row["expected_exit_code"] or hashlib.sha256(stdout.read_bytes()).hexdigest() != record["stdout_sha256"] or hashlib.sha256(stderr.read_bytes()).hexdigest() != record["stderr_sha256"] or row["expected_diagnostic"] not in stdout.read_text() + stderr.read_text():
        failures.append(row["id"])
fixture = SELF / "fixtures/negative-metadata-only-checksum"
baseline = GROUP / "engineering-live-graphs"
metadata_same = json.loads((fixture / "metadata.json").read_text()) == json.loads((baseline / "all-metadata.stdout.log").read_text())
tree_same = (fixture / "tree.log").read_bytes() == (baseline / "all-tree.stdout.log").read_bytes()
lock_diff = list(difflib.unified_diff((ROOT / "Cargo.lock").read_text().splitlines(), (fixture / "Cargo.lock").read_text().splitlines()))
workspace_record = json.loads((GROUP / "engineering-complete-results/actual-workspace-reviewed.command.json").read_text())
workspace_stdout = GROUP / "engineering-complete-results/actual-workspace-reviewed.stdout.log"
workspace_result = json.loads(workspace_stdout.read_text())
workspace_ok = workspace_record["exit_code"] == 0 and hashlib.sha256(workspace_stdout.read_bytes()).hexdigest() == workspace_record["stdout_sha256"] and len(workspace_result["results"]) == 7
result = {"checked_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "scope": "独立复核已执行77子命令证据及恢复后的七图结果；不宣称平台构建", "records": len(summary["records"]), "record_mismatches": failures, "metadata_only_checksum_metadata_unchanged": metadata_same, "metadata_only_checksum_tree_unchanged": tree_same, "metadata_only_checksum_lock_diff": lock_diff, "restored_workspace_seven_scenarios_pass": workspace_ok, "reviewed_files_sha256": {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted((ROOT / "scripts").glob("engineering*.py")) + [ROOT / "scripts/check_engineering.py", ROOT / "scripts/engineering_policy.json"]}}
print(json.dumps(result, ensure_ascii=False, indent=2))
if failures or not metadata_same or not tree_same or not workspace_ok:
    sys.exit(1)
