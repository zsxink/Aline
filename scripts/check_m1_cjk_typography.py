#!/usr/bin/env python3
"""W2 中文排版六组合的人工证据完整性检查；不判定字体或排版能力通过。"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

from evidence_paths import display_path, normalize_text, resolve_path, write_json
from validation_output import output_directory

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "scripts/fixtures/m1-cjk-typography.md"
MATRIX = {(width, mode) for width in (320, 480, 768) for mode in ("visual", "source")}
SCHEMA = "aline-m1-cjk-observations/v1"
OBSERVATIONS = ("line_boundaries", "line_height", "fallback", "italic", "punctuation_width", "smoothing")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def template():
    return {"schema": SCHEMA, "run_id": None, "commit": None, "app_sha256": None,
            "fixture_sha256": sha256(FIXTURE),
            "environment": {"os": None, "arch": None, "gpui": None},
            "records": [{"content_width_px": width, "mode": mode, "scale": None,
                         "window_size_pt": None, "font": None, "screenshot": None,
                         "screenshot_sha256": None, "measurement_method": None,
                         **{key: None for key in OBSERVATIONS}}
                        for width, mode in sorted(MATRIX)]}


def inspect_evidence(data):
    """只核对身份、六组合与证据文件；真实观测内容仍需独立人工复核。"""
    errors = []
    if not isinstance(data, dict):
        return ["输入必须为对象"]
    if data.get("schema") != SCHEMA:
        errors.append("schema 不匹配")
    for key in ("run_id", "commit"):
        if not isinstance(data.get(key), str) or not data[key].strip():
            errors.append("缺少 " + key)
    for key in ("app_sha256", "fixture_sha256"):
        value = data.get(key)
        if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
            errors.append(key + " 必须为 SHA-256")
    if data.get("fixture_sha256") != sha256(FIXTURE):
        errors.append("夹具身份不匹配")
    environment = data.get("environment")
    if not isinstance(environment, dict) or any(not environment.get(key) for key in ("os", "arch", "gpui")):
        errors.append("缺少平台/GPUI 身份")
    records = data.get("records")
    if not isinstance(records, list):
        return errors + ["缺少 records 数组"]
    seen = set()
    for record in records:
        if not isinstance(record, dict):
            errors.append("观测必须为对象")
            continue
        width, mode = record.get("content_width_px"), record.get("mode")
        if type(width) is not int or not isinstance(mode, str):
            errors.append("宽度/模式无效")
            continue
        key = (width, mode)
        if key not in MATRIX or key in seen:
            errors.append("组合未知或重复：" + str(key))
        seen.add(key)
        scale = record.get("scale")
        if type(scale) not in (int, float) or not 0 < scale < 10:
            errors.append("缺少有效缩放：" + str(key))
        for field in ("window_size_pt", "font", "measurement_method", *OBSERVATIONS):
            if not isinstance(record.get(field), str) or not record[field].strip():
                errors.append("缺少 " + field + "：" + str(key))
        screenshot = record.get("screenshot")
        if not isinstance(screenshot, str) or not screenshot:
            errors.append("缺少截图：" + str(key))
            continue
        resolved = resolve_path(screenshot)
        path = Path(resolved) if resolved else None
        if path is not None and not path.is_absolute():
            path = ROOT / path
        try:
            if path is None:
                raise ValueError("不可还原路径")
            path.resolve().relative_to((ROOT / "logs/validation").resolve())
            if path.suffix.lower() != ".png" or not path.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
                raise ValueError("截图不是 PNG")
            if sha256(path) != record.get("screenshot_sha256"):
                raise ValueError("截图 SHA-256 不匹配")
        except (OSError, ValueError):
            errors.append("截图文件/身份无效：" + str(key))
    if seen != MATRIX:
        errors.append("六组合不完整")
    return errors


class EvidenceTests(unittest.TestCase):
    def test_empty_template_fails(self):
        self.assertTrue(inspect_evidence(template()))

    def test_complete_records_and_minimal_violations(self):
        # 合成 PNG 头仅用于检查器自测，不作平台截图或排版证据。
        with tempfile.TemporaryDirectory(dir=ROOT / "logs/validation") as directory:
            screenshot = Path(directory) / "synthetic.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\nsynthetic-validator-test")
            data = template()
            data.update(run_id="synthetic-test", commit="test", app_sha256="a" * 64,
                        environment={"os": "synthetic", "arch": "synthetic", "gpui": "synthetic"})
            for row in data["records"]:
                row.update(scale=2, window_size_pt="测试", font="测试", measurement_method="测试",
                           screenshot=display_path(screenshot), screenshot_sha256=sha256(screenshot))
                row.update({key: "合成自测，不是平台证据" for key in OBSERVATIONS})
            self.assertEqual(inspect_evidence(data), [])
            for mutate in (lambda d: d["records"].pop(),
                           lambda d: d["records"].append(d["records"][0]),
                           lambda d: d.update(fixture_sha256="0" * 64),
                           lambda d: d["records"][0].update(screenshot_sha256="0" * 64),
                           lambda d: d["records"][0].update(italic=None)):
                broken = json.loads(json.dumps(data))
                mutate(broken)
                self.assertTrue(inspect_evidence(broken))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--observations", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        output = output_directory("m1-cjk-typography", args.output_dir)
        if args.self_test:
            result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(EvidenceTests))
            write_json(output / "self-test.json", {"passed": result.wasSuccessful(), "tests": result.testsRun})
            return 0 if result.wasSuccessful() else 1
        write_json(output / "observations-template.json", template())
        data = json.loads(args.observations.read_text()) if args.observations else template()
        failures = inspect_evidence(data)
        write_json(output / "summary.json", {"evidence_complete": not failures,
                   "capability_passed": None, "failures": failures,
                   "fixture": display_path(FIXTURE), "fixture_sha256": sha256(FIXTURE),
                   "boundary": "完整性不证明实际排版/字体能力；须独立复核真实截图与观测。"})
        print("W2 观测完整性：" + ("齐全，仍需人工复核" if not failures else "缺失或无效"))
        return 0 if not failures else 1
    except (OSError, ValueError, TypeError) as error:
        print("W2 检查失败：" + normalize_text(str(error)), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
