#!/usr/bin/env python3
"""核对最终 manifest 空白修正；保留既有门禁指纹，仅生成补充记录。"""

import hashlib
import json
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = Path(__file__).resolve().parents[5]
OTHER = HERE.parent / "step-9.5"
name = "crates/aline-plugin/Cargo.toml"
old = (OTHER / "inspection-after-cleanup/crates__aline-plugin__Cargo.toml.snapshot").read_bytes()
current = (ROOT / name).read_bytes()
utc = subprocess.check_output(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"], text=True).strip()
old_rows = {}
for row in (HERE / "source-fingerprints-after-cleanup.tsv").read_text().splitlines()[1:]:
    path, fingerprint = row.split("\t")
    old_rows[path] = fingerprint
paths = [ROOT / "Cargo.lock", ROOT / "Cargo.toml", ROOT / "app/Cargo.toml"]
paths += sorted((ROOT / "crates").glob("*/Cargo.toml"))
manifests = [{"path": str(path.relative_to(ROOT)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
              "previous_sha256": old_rows[str(path.relative_to(ROOT))]} for path in paths]
changed = [item["path"] for item in manifests if item["sha256"] != item["previous_sha256"]]
payload = {"inspection_utc": utc, "kind": "最终manifest空白核查，未运行Cargo",
           "gate_source": "step-9.1/workspace-gates-after-cleanup",
           "previous_fingerprint": "step-9.4/source-fingerprints-after-cleanup.tsv",
           "plugin_previous_snapshot_sha256": hashlib.sha256(old).hexdigest(),
           "plugin_is_exactly_one_trailing_lf_removed": old == current + b"\n",
           "changed_manifests": changed, "current_manifest_fingerprints": manifests}
for directory in (HERE, OTHER):
    (directory / "manifest-whitespace-final.json").write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(payload, ensure_ascii=False, indent=2))
raise SystemExit(not (old == current + b"\n" and changed == [name]))
