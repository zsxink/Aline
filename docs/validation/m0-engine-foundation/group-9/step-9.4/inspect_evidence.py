#!/usr/bin/env python3
"""读取现有源码和原始测试日志；不执行 Cargo 或宣称重新运行测试。"""

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[5]
OUTPUT = Path(__file__).resolve().parent
AFTER_CLEANUP = "--after-cleanup" in sys.argv[1:]
SUFFIX = "-after-cleanup" if AFTER_CLEANUP else ""
RAW = OUTPUT / ("inspection" + SUFFIX)
RAW.mkdir(exist_ok=True)


def utc():
    return subprocess.check_output(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"], text=True).strip()


def record(name, argv):
    started = utc()
    completed = subprocess.run(argv, cwd=ROOT, capture_output=True)
    (RAW / f"{name}.stdout.log").write_bytes(completed.stdout)
    (RAW / f"{name}.stderr.log").write_bytes(completed.stderr)
    with (OUTPUT / "commands.log").open("a") as log:
        log.write(f"{started} start cmd={' '.join(argv)}\n{utc()} exit={completed.returncode}\n")
    return completed.returncode


record("head", ["git", "rev-parse", "HEAD"])
record("status", ["git", "status", "--short"])
record("host", ["uname", "-sm"])
gate = ROOT / ("docs/validation/m0-engine-foundation/group-9/step-9.1/workspace-gates" + SUFFIX)
summary = json.loads((gate / "summary.json").read_text())
stdout = (gate / "test.stdout.log").read_text()
observed = set(re.findall(r"^test (\S+) \.\.\. ok$", stdout, re.MULTILINE))
tests = []
for crate in ("aline-source", "aline-buffer", "aline-edit", "aline-ui"):
    for source in sorted((ROOT / "crates" / crate / "tests").glob("*.rs")):
        content = source.read_text()
        for match in re.finditer(r"#\[test\]\s*fn\s+(\w+)\s*\(", content):
            name = match.group(1)
            tests.append({"source": str(source.relative_to(ROOT)), "line": content[:match.start()].count("\n") + 1,
                          "test": name, "observed_ok_in_existing_log": name in observed})
payload = {"kind": "读取已有日志，未重新运行测试", "inspection_utc": utc(),
           "gate_path": str(gate.relative_to(ROOT)),
           "gate_checks": len(summary["results"]),
           "gate_exit_zero": sum(item["exit_code"] == 0 for item in summary["results"]),
           "workspace_passed": sum(map(int, re.findall(r"test result: ok\. (\d+) passed", stdout))),
           "workspace_failed": sum(map(int, re.findall(r"test result: .*?; (\d+) failed", stdout))),
           "test_functions": tests}
(OUTPUT / ("observed-tests" + SUFFIX + ".json")).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
paths = [gate / "summary.json", gate / "test.stdout.log", gate / "test.stderr.log"]
paths += [ROOT / item["source"] for item in tests]
for crate in ("aline-source", "aline-buffer", "aline-edit", "aline-ui"):
    paths += sorted((ROOT / "crates" / crate / "src").rglob("*.rs"))
paths += [ROOT / "crates/aline-markdown/tests/mapping.rs",
          ROOT / "docs/validation/m0-engine-foundation/group-4/step-4.3/roundtrip-golden.v1.txt",
          ROOT / "docs/validation/m0-engine-foundation/group-5/step-5.1/goldens/forward-inverse.tsv"]
paths += [ROOT / "docs/v1-prd.md", ROOT / "docs/v1-validation-plan.md", ROOT / "docs/v1-architecture.md",
          ROOT / "openspec/changes/m0-engine-foundation/proposal.md"]
paths += [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
paths += sorted((ROOT / "crates").glob("*/Cargo.toml"))
paths += [ROOT / "app/Cargo.toml"]
unique = sorted(set(paths))
(OUTPUT / ("source-fingerprints" + SUFFIX + ".tsv")).write_text("path\tsha256\n" + "".join(
    f"{path.relative_to(ROOT)}\t{hashlib.sha256(path.read_bytes()).hexdigest()}\n" for path in unique))
print(json.dumps({"kind": payload["kind"], "checks": payload["gate_checks"],
                  "exit_zero": payload["gate_exit_zero"], "workspace_passed": payload["workspace_passed"],
                  "workspace_failed": payload["workspace_failed"], "external_tests": len(tests),
                  "unobserved": [item["test"] for item in tests if not item["observed_ok_in_existing_log"]]}, ensure_ascii=False))
