#!/usr/bin/env python3
"""核对 E4/E7/E6 证据链接与当前交接输入；不执行产品实验。"""

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = Path(__file__).resolve().parents[5]
AFTER_CLEANUP = "--after-cleanup" in sys.argv[1:]
SUFFIX = "-after-cleanup" if AFTER_CLEANUP else ""
RAW = HERE / ("inspection" + SUFFIX)
RAW.mkdir(exist_ok=True)


def utc():
    return subprocess.check_output(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"], text=True).strip()


def record(name, argv):
    started = utc()
    run = subprocess.run(argv, cwd=ROOT, capture_output=True)
    (RAW / f"{name}.stdout.log").write_bytes(run.stdout)
    (RAW / f"{name}.stderr.log").write_bytes(run.stderr)
    with (HERE / "commands.log").open("a") as log:
        log.write(f"{started} start cmd={' '.join(argv)}\n{utc()} exit={run.returncode}\n")
    return run.returncode


record("head", ["git", "rev-parse", "HEAD"])
record("diff-check", ["git", "diff", "--check", "--", str(HERE.relative_to(ROOT)),
                       "docs/validation/m0-engine-foundation/group-9/step-9.4"])
inputs = ["docs/v1-prd.md", "docs/v1-validation-plan.md", "docs/v1-architecture.md",
          "openspec/changes/m0-engine-foundation/proposal.md", "app/src/main.rs", "Cargo.lock",
          "crates/aline-ui/Cargo.toml", "crates/aline-plugin/Cargo.toml"]
manifest = []
for name in inputs:
    source = ROOT / name
    manifest.append({"path": name, "sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
    (RAW / (name.replace("/", "__") + ".snapshot")).write_bytes(source.read_bytes())
reports = [HERE / "report.md", HERE.parent / "step-9.4/report.md"]
links = []
for report in reports:
    for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", report.read_text()):
        filepart = target.split("#", 1)[0]
        if not filepart or "://" in filepart:
            continue
        resolved = report.parent / filepart
        links.append({"report": str(report.relative_to(ROOT)), "target": target,
                      "exists": resolved.exists()})
payload = {"inspection_utc": utc(), "kind": "只读链接/输入核对，未执行平台或产品实验",
           "inputs": manifest, "links": links}
(HERE / ("handoff-inputs-and-links" + SUFFIX + ".json")).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
missing = [item for item in links if not item["exists"]]
print(json.dumps({"checked_links": len(links), "missing": missing, "kind": payload["kind"]}, ensure_ascii=False))
raise SystemExit(bool(missing))
