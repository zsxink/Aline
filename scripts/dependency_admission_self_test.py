"""精确许可证例外的独立最小图回归；所有图均为明确的合成夹具。"""

import argparse
import copy
import io
import json
from pathlib import Path
import unittest

from dependency_admission import check_admission
from engineering_dependencies import Violation
from evidence_paths import write_json, write_text
from validation_output import output_directory

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
MAC_TARGET = 'cfg(target_os = "macos")'
APPLE_TARGET = 'cfg(any(target_os = "macos", target_os = "ios"))'
FONTCONFIG_NON_DESKTOP_TARGET = 'cfg(not(any(target_family = "windows", target_os = "macos", target_os = "ios", target_arch = "wasm32", target_env = "ohos")))'
CHECKSUMS = {
    "gpui-pre-macos@0.3.9": "c7c988b7e49775cc8542dbe48d5a10e073d8e898a023ef3ff3d24f33363e9863",
    "gpui-pre-apple@0.3.9": "b52f58893fe1a0836464428dfad2b5eaf42c9c303a9638f4c2aced5a69072a15",
    "zed-font-kit@0.14.1-zed": "a3898e450f36f852edda72e3f985c34426042c4951790b23b107f93394f9bff5",
    "yeslogic-fontconfig-sys@6.0.1": "1d8b8abf912b9a29ff112e1671c97c33636903d13a69712037190e6805af4f76",
    "dlib@0.5.3": "ab8ecd87370524b461f8557c119c405552c396ed91fc0a8eec68679eab26f94a",
    "sys-locale@0.3.2": "8eab9a99a024a169fe8a903cf9d4a3b3601109bcc13bd9e3c6fff259138626c4",
    "cbindgen@0.28.0": "eadd868a2ce9ca38de7eeafdcec9c7065ef89b42b32f0839278d55f35c54d1ff",
    "option-ext@0.2.0": "04744f49eae99ab78e0d5c0b603ab218f515ea8cfe5a456d7629ad883a3b6e7d",
    "dwrote@0.11.5": "9e1b35532432acc8b19ceed096e35dfa088d3ea037fe4f3c085f1f97f33b4d02",
    "libloading@0.8.9": "d7c4b02199fee7c5d21a5ae7d8cfa79a6ef5bb2fc834d6e9058e89c825efdc55",
}


def edge_spec(parent, child, alias, kind=None, target=None):
    return {"from": parent, "to": child, "alias": alias,
            "kinds": [{"kind": kind, "target": target}]}


def identity_spec(key):
    return {"package": key, "source": REGISTRY, "checksum": CHECKSUMS[key]}


def group(group_id, root_key, incoming):
    return {"id": group_id,
            "rule": {"kind": "closure", "roots": [root_key.split("@")[0]], "optional": True,
                     "root_identities": [identity_spec(root_key)], "root_incoming": incoming},
            "selected_allowed": True, "purpose": "独立最小图的已裁决用途",
            "consumer": "aline-ui 或 app", "alternatives": "本夹具不安装依赖",
            "necessity": "验证精确来源、闭包和拒绝边界"}


def fixture():
    """用真实已批准的身份常量，构造不消费 Cargo/网络的最短依赖图。"""
    packages, nodes, lock = {}, {}, {}

    def package(name, version, license_value="MIT", workspace=False):
        key = name + "@" + version
        source = None if workspace else REGISTRY
        ident = "workspace#" + key if workspace else REGISTRY + "#" + key
        packages[ident] = {"id": ident, "name": name, "version": version,
                           "source": source, "license": None if workspace else license_value}
        nodes[ident] = {"id": ident, "deps": [], "features": []}
        lock[key] = {"name": name, "version": version, "source": source,
                     "checksum": CHECKSUMS.get(key, "1" * 64)}
        return ident

    app = package("aline", "0.1.0", workspace=True)
    ui = package("aline-ui", "0.1.0", workspace=True)
    backend = package("gpui-pre-macos", "0.3.9", "Apache-2.0")
    apple = package("gpui-pre-apple", "0.3.9", "Apache-2.0")
    dirs = package("dirs-sys", "0.4.1")
    cbindgen = package("cbindgen", "0.28.0", "MPL-2.0")
    option_ext = package("option-ext", "0.2.0", "MPL-2.0")
    font_kit = package("zed-font-kit", "0.14.1-zed", "MIT OR Apache-2.0")
    dwrote = package("dwrote", "0.11.5", "MPL-2.0")
    yeslogic = package("yeslogic-fontconfig-sys", "6.0.1")
    dlib = package("dlib", "0.5.3")
    libloading = package("libloading", "0.8.9", "ISC")
    locale = package("sys-locale", "0.3.2", "MIT OR Apache-2.0")

    def connect(parent, child, alias, kind=None, target=None):
        nodes[parent]["deps"].append({"pkg": child, "name": alias,
                                      "dep_kinds": [{"kind": kind, "target": target}]})

    connect(ui, backend, "gpui_macos", target=MAC_TARGET)
    connect(backend, apple, "gpui_apple", target=MAC_TARGET)
    connect(backend, dirs, "dirs_sys")
    connect(apple, cbindgen, "cbindgen", "build", APPLE_TARGET)
    connect(apple, font_kit, "font_kit", target=APPLE_TARGET)
    connect(font_kit, dwrote, "dwrote", target='cfg(target_family = "windows")')
    connect(font_kit, yeslogic, "fontconfig_sys", target=FONTCONFIG_NON_DESKTOP_TARGET)
    connect(yeslogic, dlib, "dlib")
    connect(dlib, libloading, "libloading")
    connect(dirs, option_ext, "option_ext")
    connect(app, locale, "sys_locale")
    policy = {"format": 1, "reviewed": True,
              "license": {"allowed": ["MIT", "Apache-2.0"], "exact_exceptions": []},
              "groups": [
                  {"id": "workspace", "rule": {"kind": "workspace"}, "purpose": "自有夹具",
                   "consumer": "自有夹具", "alternatives": "无", "necessity": "验证根"},
                  group("gpui-macos-backend", "gpui-pre-macos@0.3.9", [
                      edge_spec("aline-ui@0.1.0", "gpui-pre-macos@0.3.9", "gpui_macos", target=MAC_TARGET)]),
                  group("system-locale", "sys-locale@0.3.2", [
                      edge_spec("aline@0.1.0", "sys-locale@0.3.2", "sys_locale")]),
                  {"id": "unassigned", "rule": {"kind": "remaining"}, "selected_allowed": False,
                   "purpose": "兜底", "consumer": "无", "alternatives": "无", "necessity": "拒绝遗漏"},
              ]}
    for key, incoming in [
        ("cbindgen@0.28.0", edge_spec("gpui-pre-apple@0.3.9", "cbindgen@0.28.0", "cbindgen", "build", APPLE_TARGET)),
        ("option-ext@0.2.0", edge_spec("dirs-sys@0.4.1", "option-ext@0.2.0", "option_ext")),
        ("dwrote@0.11.5", edge_spec("zed-font-kit@0.14.1-zed", "dwrote@0.11.5", "dwrote", target='cfg(target_family = "windows")')),
    ]:
        basis = ("2026-11-02 用户批准此精确 metadata-only 身份/路径；完整 target 条件矛盾，desktop target tree 不选中"
                 if key == "dwrote@0.11.5" else "2026-11-02 用户仅批准此精确包及 macOS 后端闭包的例外")
        policy["license"]["exact_exceptions"].append({
            **identity_spec(key), "license": "MPL-2.0", "group": "gpui-macos-backend",
            "root": "gpui-pre-macos@0.3.9", "incoming": [incoming],
            "basis": basis})
    policy["license"]["exact_exceptions"].append({
        **identity_spec("libloading@0.8.9"), "license": "ISC", "group": "gpui-macos-backend",
        "root": "gpui-pre-macos@0.3.9",
        "incoming": [edge_spec("dlib@0.5.3", "libloading@0.8.9", "libloading")],
        "path": {
            "packages": [identity_spec(key) for key in (
                "gpui-pre-macos@0.3.9", "gpui-pre-apple@0.3.9", "zed-font-kit@0.14.1-zed",
                "yeslogic-fontconfig-sys@6.0.1", "dlib@0.5.3", "libloading@0.8.9")],
            "edges": [
                edge_spec("gpui-pre-macos@0.3.9", "gpui-pre-apple@0.3.9", "gpui_apple", target=MAC_TARGET),
                edge_spec("gpui-pre-apple@0.3.9", "zed-font-kit@0.14.1-zed", "font_kit", target=APPLE_TARGET),
                edge_spec("zed-font-kit@0.14.1-zed", "yeslogic-fontconfig-sys@6.0.1", "fontconfig_sys", target=FONTCONFIG_NON_DESKTOP_TARGET),
                edge_spec("yeslogic-fontconfig-sys@6.0.1", "dlib@0.5.3", "dlib"),
                edge_spec("dlib@0.5.3", "libloading@0.8.9", "libloading"),
            ]},
        "basis": "2026-11-02 用户仅批准此精确 ISC package/checksum、macOS 后端 root 与完整 metadata 路径；macOS 与 non-macOS fontconfig target 条件矛盾，desktop target tree 不选中；不扩展 ISC 全局允许集合"})
    return {"packages": packages, "nodes": nodes, "roots": [app, ui], "selected": set(packages),
            "lock": lock, "policy": policy}


def admit(data):
    return check_admission(**data, scenario="synthetic-exact-license", with_ledger=True)


def find(data, name):
    return next(ident for ident, package in data["packages"].items() if package["name"] == name)


class ExactLicenseTests(unittest.TestCase):
    def test_formal_policy_registers_only_the_approved_exact_rules(self):
        policy = json.loads((ROOT / "scripts/dependency_admission_policy.json").read_text())
        expected = fixture()["policy"]
        self.assertNotIn("MPL-2.0", policy["license"]["allowed"])
        self.assertEqual(policy["license"]["exact_exceptions"], expected["license"]["exact_exceptions"])
        for group_id in ("gpui-macos-backend", "system-locale"):
            actual = next(group for group in policy["groups"] if group["id"] == group_id)
            required = next(group for group in expected["groups"] if group["id"] == group_id)
            self.assertEqual(actual["rule"], required["rule"])

    def test_only_exact_approved_packages_receive_visible_exception_verdicts(self):
        result = admit(fixture())
        exceptions = [row for row in result["ledger"]["packages"]
                      if row["license"]["verdict"] == "allowed-exact-exception"]
        self.assertEqual({row["package"] for row in exceptions}, {"cbindgen@0.28.0", "option-ext@0.2.0", "dwrote@0.11.5", "libloading@0.8.9"})
        self.assertTrue(all(row["group"] == "gpui-macos-backend" for row in exceptions))

    def test_unrelated_mpl_and_different_version_remain_rejected(self):
        for field, value in [("license", "MPL-2.0 AND GPL-3.0-only"), ("version", "0.28.1")]:
            with self.subTest(field=field):
                data = fixture()
                data["packages"][find(data, "cbindgen")][field] = value
                with self.assertRaises(Violation): admit(data)
        data = fixture()
        data["packages"][find(data, "dirs-sys")]["license"] = "MPL-2.0"
        with self.assertRaisesRegex(Violation, "许可证未准入"): admit(data)

    def test_dwrote_exception_is_exactly_metadata_only_and_target_bound(self):
        data = fixture(); ident = find(data, "dwrote")
        exception = next(item for item in data["policy"]["license"]["exact_exceptions"] if item["package"] == "dwrote@0.11.5")
        self.assertEqual(exception["incoming"], [edge_spec(
            "zed-font-kit@0.14.1-zed", "dwrote@0.11.5", "dwrote", target='cfg(target_family = "windows")')])
        data["nodes"][find(data, "zed-font-kit")]["deps"][0]["dep_kinds"][0]["target"] = 'cfg(target_os = "macos")'
        with self.assertRaisesRegex(Violation, "新增入边"):
            admit(data)
        data = fixture(); data["policy"]["license"]["exact_exceptions"].remove(
            next(item for item in data["policy"]["license"]["exact_exceptions"] if item["package"] == "dwrote@0.11.5"))
        with self.assertRaisesRegex(Violation, "许可证未准入"):
            admit(data)
        data = fixture(); data["lock"]["dwrote@0.11.5"]["checksum"] = "0" * 64
        with self.assertRaisesRegex(Violation, "精确.*身份"):
            admit(data)

    def test_libloading_exception_is_bound_to_complete_mutually_exclusive_metadata_path(self):
        data = fixture()
        rule = next(item for item in data["policy"]["license"]["exact_exceptions"] if item["package"] == "libloading@0.8.9")
        self.assertEqual([item["package"] for item in rule["path"]["packages"]], [
            "gpui-pre-macos@0.3.9", "gpui-pre-apple@0.3.9", "zed-font-kit@0.14.1-zed",
            "yeslogic-fontconfig-sys@6.0.1", "dlib@0.5.3", "libloading@0.8.9"])
        result = admit(data)
        row = next(row for row in result["ledger"]["packages"] if row["package"] == "libloading@0.8.9")
        self.assertEqual(row["license"]["verdict"], "allowed-exact-exception")
        for mutation in ("target", "alias", "intermediate-checksum", "alternate-route", "missing-path"):
            with self.subTest(mutation=mutation):
                data = fixture()
                if mutation == "target":
                    data["nodes"][find(data, "zed-font-kit")]["deps"][1]["dep_kinds"][0]["target"] = MAC_TARGET
                elif mutation == "alias":
                    data["nodes"][find(data, "dlib")]["deps"][0]["name"] = "other"
                elif mutation == "intermediate-checksum":
                    data["lock"]["dlib@0.5.3"]["checksum"] = "0" * 64
                elif mutation == "alternate-route":
                    data["nodes"][find(data, "gpui-pre-macos")]["deps"].append({
                        "pkg": find(data, "libloading"), "name": "libloading",
                        "dep_kinds": [{"kind": None, "target": None}]})
                else:
                    data["policy"]["license"]["exact_exceptions"].pop()
                with self.assertRaises(Violation):
                    admit(data)

    def test_source_checksum_and_missing_identity_are_rejected(self):
        for change in ("source", "checksum", "id"):
            with self.subTest(change=change):
                data = fixture(); ident = find(data, "cbindgen")
                if change == "checksum": data["lock"]["cbindgen@0.28.0"]["checksum"] = "0" * 64
                elif change == "source": data["packages"][ident]["source"] = "registry+https://example.invalid/index"
                else: del data["packages"][ident]["id"]
                with self.assertRaisesRegex(Violation, "精确.*身份"): admit(data)

    def test_all_workspace_paths_must_pass_backend_root(self):
        data = fixture(); ui = find(data, "aline-ui"); target = find(data, "cbindgen")
        data["nodes"][ui]["deps"].append({"pkg": target, "name": "bypass", "dep_kinds": [{"kind": None, "target": None}]})
        with self.assertRaisesRegex(Violation, "闭包外路径|新增入边"): admit(data)

    def test_new_incoming_inside_backend_and_wrong_group_are_rejected(self):
        data = fixture(); root = find(data, "gpui-pre-macos"); target = find(data, "cbindgen")
        data["nodes"][root]["deps"].append({"pkg": target, "name": "extra", "dep_kinds": [{"kind": None, "target": None}]})
        with self.assertRaisesRegex(Violation, "新增入边"): admit(data)
        data = fixture()
        data["policy"]["license"]["exact_exceptions"][0]["group"] = "system-locale"
        with self.assertRaisesRegex(Violation, "分组|根"): admit(data)

    def test_root_identity_and_macos_target_are_required(self):
        for change in ("target", "version", "source", "checksum"):
            with self.subTest(change=change):
                data = fixture(); root = find(data, "gpui-pre-macos")
                if change == "target": data["nodes"][find(data, "aline-ui")]["deps"][0]["dep_kinds"][0]["target"] = None
                elif change == "checksum": data["lock"]["gpui-pre-macos@0.3.9"]["checksum"] = "0" * 64
                else: data["packages"][root][change] = "0.3.10" if change == "version" else "registry+https://example.invalid/index"
                with self.assertRaises(Violation): admit(data)

    def test_optional_groups_allow_absent_backends_but_no_orphan_exception(self):
        data = fixture(); remove = set(data["packages"]) - set(data["roots"])
        for ident in remove: data["packages"].pop(ident); data["nodes"].pop(ident); data["selected"].discard(ident)
        for node in data["nodes"].values(): node["deps"] = []
        self.assertEqual(admit(data)["summary"]["packages"], 2)
        data = fixture(); root = find(data, "gpui-pre-macos")
        data["packages"].pop(root); data["nodes"].pop(root); data["selected"].discard(root)
        for node in data["nodes"].values(): node["deps"] = [edge for edge in node["deps"] if edge["pkg"] != root]
        with self.assertRaises(Violation): admit(data)

    def test_malformed_exception_rules_fail_closed(self):
        for change in ("missing-checksum", "duplicate", "unknown-key", "missing-incoming", "unknown-group", "wrong-to"):
            with self.subTest(change=change):
                data = fixture(); rules = data["policy"]["license"]["exact_exceptions"]
                if change == "missing-checksum": del rules[0]["checksum"]
                elif change == "duplicate": rules.append(copy.deepcopy(rules[0]))
                elif change == "unknown-key": rules[0]["unreviewed"] = True
                elif change == "missing-incoming": rules[0]["incoming"] = []
                elif change == "unknown-group": rules[0]["group"] = "unregistered"
                else: rules[0]["incoming"][0]["to"] = "different@1.0.0"
                with self.assertRaisesRegex(Violation, "精确许可证例外|精确准入|分组"): admit(data)

    def test_malformed_exact_group_fields_fail_closed(self):
        for change in ("missing-root-identity", "missing-root-incoming", "bad-optional", "duplicate-incoming", "wrong-root-edge"):
            with self.subTest(change=change):
                data = fixture(); rule = data["policy"]["groups"][1]["rule"]
                if change == "missing-root-identity": del rule["root_identities"]
                elif change == "missing-root-incoming": del rule["root_incoming"]
                elif change == "bad-optional": rule["optional"] = "true"
                elif change == "duplicate-incoming": rule["root_incoming"].append(copy.deepcopy(rule["root_incoming"][0]))
                else: rule["root_incoming"][0]["to"] = "different@1.0.0"
                with self.assertRaisesRegex(Violation, "精确准入|optional"): admit(data)

    def test_locale_must_be_precise_and_explicitly_grouped(self):
        result = admit(fixture())
        locale = next(row for row in result["ledger"]["packages"] if row["name"] == "sys-locale")
        self.assertEqual(locale["group"], "system-locale")
        self.assertEqual(locale["license"]["verdict"], "allowed")
        data = fixture(); data["policy"]["groups"] = [g for g in data["policy"]["groups"] if g["id"] != "system-locale"]
        with self.assertRaisesRegex(Violation, "已选包未进入运行时准入分组"): admit(data)
        for field in ("version", "source", "checksum"):
            data = fixture(); ident = find(data, "sys-locale")
            if field == "checksum": data["lock"]["sys-locale@0.3.2"]["checksum"] = "0" * 64
            else: data["packages"][ident][field] = "0.3.3" if field == "version" else "registry+https://example.invalid/index"
            with self.assertRaises(Violation): admit(data)


def run(output=None):
    output = output_directory("dependency-admission-self-test", output)
    stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream, verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(ExactLicenseTests))
    text = stream.getvalue()
    write_text(output / "tests.txt", text)
    write_json(output / "summary.json", {"passed": result.wasSuccessful(), "tests": result.testsRun,
               "failures": len(result.failures), "errors": len(result.errors),
               "expectation": "用户批准的两个精确 MPL 包只可经 macOS 后端根与已登记全部入边进入；locale 独立准入",
               "limits": "明确合成的最短图；不代表真实 Cargo 闭包或原生平台构建"})
    print(text)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    raise SystemExit(run(parser.parse_args().output_dir))
