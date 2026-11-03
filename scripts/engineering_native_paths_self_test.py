"""原生后端目标分支例外的独立最小图；不消费真实 Cargo 缓存或旧日志。"""

import argparse
import copy
import io
from pathlib import Path
import unittest

from engineering_dependencies import Violation
from evidence_paths import write_json, write_text
from validation_output import output_directory

REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
MAC = 'cfg(target_os = "macos")'
P2 = 'cfg(all(target_arch = "wasm32", target_os = "wasi", target_env = "p2"))'
WASI = "wasi@0.11.1+wasi-snapshot-preview1"
WASIP2 = "wasip2@1.0.4+wasi-0.2.12"
BACKEND = "gpui-pre-macos@0.3.9"
CORE = "gpui-pre@0.3.9"
DESKTOP = ["macos-arm", "macos-intel", "windows-x64", "linux-x64", "linux-arm"]


def edge(parent, child, alias, target=None):
    return {"from": parent, "to": child, "alias": alias, "kinds": [{"kind": None, "target": target}]}


def fixture():
    packages, nodes, lock = {}, {}, {}
    def add(key, workspace=False):
        name, version = key.split("@", 1); ident = ("workspace#" if workspace else REGISTRY + "#") + key
        source = None if workspace else REGISTRY
        packages[ident] = {"id": ident, "name": name, "version": version, "source": source}
        nodes[ident] = {"id": ident, "deps": [], "features": []}
        lock[key] = {"source": source, "checksum": "1" * 64}
        return ident
    def spec(key): return {"package": key, "source": REGISTRY, "checksum": "1" * 64}
    paths = {
        WASI: [edge(BACKEND, "gpui-pre-apple@0.3.9", "gpui_apple", MAC),
               edge("gpui-pre-apple@0.3.9", "zed-font-kit@0.14.1-zed", "font_kit", 'cfg(any(target_os = "macos", target_os = "ios"))'),
               edge("zed-font-kit@0.14.1-zed", "dirs@5.0.1", "dirs", 'cfg(not(any(target_arch = "wasm32", target_family = "windows", target_os = "android", target_env = "ohos")))'),
               edge("dirs@5.0.1", "dirs-sys@0.4.1", "dirs_sys"),
               edge("dirs-sys@0.4.1", "redox_users@0.4.6", "redox_users", 'cfg(target_os = "redox")'),
               edge("redox_users@0.4.6", "getrandom@0.2.17", "getrandom"),
               edge("getrandom@0.2.17", WASI, "wasi", 'cfg(target_os = "wasi")')],
        WASIP2: [edge(BACKEND, "gpui-pre-scheduler@0.3.9", "scheduler", MAC),
                 edge("gpui-pre-scheduler@0.3.9", "rand@0.9.5", "rand"),
                 edge("rand@0.9.5", "rand_core@0.9.5", "rand_core"),
                 edge("rand_core@0.9.5", "getrandom@0.3.4", "getrandom"),
                 edge("getrandom@0.3.4", WASIP2, "wasip2", P2)],
    }
    all_edges = [item for path in paths.values() for item in path]
    keys = {CORE, BACKEND} | {e[key] for e in all_edges for key in ("from", "to")}
    for key in keys: add(key)
    ui = add("aline-ui@0.1.0", True)
    incoming = edge("aline-ui@0.1.0", BACKEND, "gpui_macos", MAC)
    all_edges += [incoming, edge(BACKEND, CORE, "gpui"), edge(CORE, "gpui-pre-scheduler@0.3.9", "scheduler")]
    by_key = {p["name"] + "@" + p["version"]: ident for ident, p in packages.items()}
    for item in all_edges:
        nodes[by_key[item["from"]]]["deps"].append({"name": item["alias"], "pkg": by_key[item["to"]], "dep_kinds": copy.deepcopy(item["kinds"])})
    policy = {"native_backend": {"root": spec(BACKEND), "core": spec(CORE),
                               "packages": [spec(BACKEND), spec("gpui-pre-apple@0.3.9")],
                               "incoming": [incoming]}, "native_path_exceptions": {}}
    for key, path in paths.items():
        path_keys = [path[0]["from"]] + [e["to"] for e in path]
        policy["native_path_exceptions"][key] = {"path": [spec(k) for k in path_keys], "edges": path,
            "selected_scenarios": ["all", "all-features"], "unselected_scenarios": ["all", "all-features"] + DESKTOP,
            "basis": "用户批准的精确非桌面目标分支；桌面 tree 不得选中"}
    return {"packages": packages, "nodes": nodes, "roots": [ui], "lock": lock, "policy": policy,
            "selected": set(packages)}, by_key


def check(data, scenario="all"):
    from engineering_native_paths import check_native_paths
    return check_native_paths(**data, scenario=scenario)


def branched_fixture():
    """独立写出获批的九条附加边，允许多路汇聚但不借用全闭包放行。"""
    data, ids = fixture(); packages = data["packages"]; nodes = data["nodes"]; lock = data["lock"]
    for key in ("image@0.25.10", "ravif@0.13.0", "rav1e@0.8.1", "rand_chacha@0.9.0"):
        name, version = key.split("@", 1); ident = REGISTRY + "#" + key; ids[key] = ident
        packages[ident] = {"id": ident, "name": name, "version": version, "source": REGISTRY}
        nodes[ident] = {"id": ident, "deps": [], "features": []}
        lock[key] = {"source": REGISTRY, "checksum": "1" * 64}; data["selected"].add(ident)
    extra = [edge(BACKEND, "gpui-pre-apple@0.3.9", "gpui_apple", MAC),
             edge(BACKEND, "image@0.25.10", "image", MAC),
             edge("gpui-pre-apple@0.3.9", "image@0.25.10", "image", 'cfg(any(target_os = "macos", target_os = "ios"))'),
             edge("image@0.25.10", "ravif@0.13.0", "ravif"),
             {"from": "ravif@0.13.0", "to": "rav1e@0.8.1", "alias": "rav1e", "kinds": [
                 {"kind": None, "target": None}, {"kind": None, "target": 'cfg(target = "wasm32-unknown-unknown")'}]},
             edge("rav1e@0.8.1", "rand@0.9.5", "rand", "cfg(fuzzing)"),
             edge("rav1e@0.8.1", "rand_chacha@0.9.0", "rand_chacha", "cfg(fuzzing)"),
             edge("rand@0.9.5", "rand_chacha@0.9.0", "rand_chacha"),
             edge("rand_chacha@0.9.0", "rand_core@0.9.5", "rand_core")]
    for item in extra[1:]:
        nodes[ids[item["from"]]]["deps"].append({"name": item["alias"], "pkg": ids[item["to"]], "dep_kinds": copy.deepcopy(item["kinds"])})
    rule = data["policy"]["native_path_exceptions"][WASIP2]
    inventory = rule.pop("path")
    inventory += [{"package": key, "source": REGISTRY, "checksum": "1" * 64}
                  for key in ("gpui-pre-apple@0.3.9", "image@0.25.10", "ravif@0.13.0", "rav1e@0.8.1", "rand_chacha@0.9.0")]
    rule["packages"] = inventory; rule["edges"] += extra
    return data, ids


class NativePathTests(unittest.TestCase):
    def test_exact_branched_inventory_and_fourteen_edge_union_are_accepted(self):
        data, ids = branched_fixture()
        self.assertEqual(len(data["policy"]["native_path_exceptions"][WASIP2]["edges"]), 14)
        self.assertEqual(check(data)["forbidden"], {ids[WASI], ids[WASIP2]})

    def test_branched_actual_missing_or_extra_edge_is_rejected(self):
        for change in ("missing", "extra"):
            data, ids = branched_fixture()
            if change == "missing": data["nodes"][ids["rand_chacha@0.9.0"]]["deps"] = []
            else: data["nodes"][ids[BACKEND]]["deps"].append({"name": "bypass", "pkg": ids["rand@0.9.5"], "dep_kinds": [{"kind": None, "target": None}]})
            with self.assertRaisesRegex(Violation, "路径|入边"): check(data)

    def test_branched_actual_alias_kind_duplicate_edge_and_workspace_bypass_are_rejected(self):
        for change in ("alias", "kind", "duplicate-edge", "workspace-bypass"):
            data, ids = branched_fixture()
            if change == "alias":
                item = next(item for item in data["nodes"][ids[BACKEND]]["deps"] if item["pkg"] == ids["image@0.25.10"])
                item["name"] = "unreviewed_image_alias"
            elif change == "kind":
                item = next(item for item in data["nodes"][ids["ravif@0.13.0"]]["deps"] if item["pkg"] == ids["rav1e@0.8.1"])
                item["dep_kinds"][0]["kind"] = "build"
            elif change == "duplicate-edge":
                item = next(item for item in data["nodes"][ids["rand@0.9.5"]]["deps"] if item["pkg"] == ids["rand_chacha@0.9.0"])
                data["nodes"][ids["rand@0.9.5"]]["deps"].append(copy.deepcopy(item))
            else:
                data["nodes"][ids["aline-ui@0.1.0"]]["deps"].append({
                    "name": "unreviewed_image_bypass", "pkg": ids["image@0.25.10"],
                    "dep_kinds": [{"kind": None, "target": None}],
                })
            with self.assertRaisesRegex(Violation, "路径|入边"):
                check(data)

    def test_branched_identity_and_cfg_fuzzing_drift_are_rejected(self):
        for change in ("checksum", "source", "version", "target"):
            data, ids = branched_fixture()
            if change == "checksum": data["lock"]["image@0.25.10"]["checksum"] = "0" * 64
            elif change == "source": data["packages"][ids["rav1e@0.8.1"]]["source"] = "registry+https://example.invalid/index"
            elif change == "version": data["packages"][ids["image@0.25.10"]]["version"] = "99.0.0"
            else: data["nodes"][ids["rav1e@0.8.1"]]["deps"][0]["dep_kinds"][0]["target"] = None
            with self.assertRaises(Violation): check(data)

    def test_branched_policy_rejects_unused_node_cycle_unknown_endpoint_and_duplicate_edge(self):
        for change in ("unused", "cycle", "unknown-endpoint", "duplicate-edge", "both-shapes"):
            data, _ = branched_fixture(); rule = data["policy"]["native_path_exceptions"][WASIP2]
            if change == "unused": rule["packages"].append({"package": "unused@1.0.0", "source": REGISTRY, "checksum": "1" * 64})
            elif change == "cycle": rule["edges"].append(edge("rand_core@0.9.5", "rand@0.9.5", "cycle"))
            elif change == "unknown-endpoint": rule["edges"][0]["to"] = "unknown@1.0.0"
            elif change == "duplicate-edge": rule["edges"].append(copy.deepcopy(rule["edges"][0]))
            else: rule["path"] = copy.deepcopy(rule["packages"])
            with self.assertRaisesRegex(Violation, "规则|路径|身份"): check(data)

    def test_branched_union_keeps_desktop_no_selection(self):
        for scenario in DESKTOP:
            data, ids = branched_fixture(); data["selected"] -= {ids[WASI], ids[WASIP2]}
            self.assertEqual(len(check(data, scenario)["forbidden"]), 2)
            data["selected"].add(ids[WASIP2])
            with self.assertRaisesRegex(Violation, "实际选包场景"): check(data, scenario)

    def test_exact_merged_paths_and_sidecar_identities_are_accepted(self):
        data, ids = fixture(); result = check(data)
        self.assertEqual(result["forbidden"], {ids[WASI], ids[WASIP2]})
        self.assertEqual(result["sidecars"], {ids[BACKEND], ids["gpui-pre-apple@0.3.9"]})

    def test_desktop_metadata_only_is_allowed_but_target_tree_selection_is_rejected(self):
        for scenario in DESKTOP:
            with self.subTest(scenario=scenario):
                data, ids = fixture(); data["selected"] -= {ids[WASI], ids[WASIP2]}
                self.assertEqual(len(check(data, scenario)["forbidden"]), 2)
                for target in (WASI, WASIP2):
                    changed = copy.deepcopy(data); changed["selected"].add(ids[target])
                    with self.assertRaisesRegex(Violation, "实际选包场景"): check(changed, scenario)

    def test_unknown_scenarios_do_not_borrow_merged_permission(self):
        data, _ = fixture()
        with self.assertRaisesRegex(Violation, "场景"): check(data, "wasi-runtime")

    def test_every_path_node_requires_exact_identity_and_checksum(self):
        for key in (BACKEND, CORE, "zed-font-kit@0.14.1-zed", "getrandom@0.2.17", WASI, WASIP2):
            for field in ("version", "source", "checksum"):
                with self.subTest(key=key, field=field):
                    data, ids = fixture()
                    if field == "checksum": data["lock"][key][field] = "0" * 64
                    else: data["packages"][ids[key]][field] = "99.0.0" if field == "version" else "registry+https://example.invalid/index"
                    with self.assertRaisesRegex(Violation, "身份"): check(data)

    def test_workspace_bypass_and_alternative_internal_path_are_rejected(self):
        for parent in ("aline-ui@0.1.0", BACKEND):
            with self.subTest(parent=parent):
                data, ids = fixture()
                data["nodes"][ids[parent]]["deps"].append({"name": "bypass", "pkg": ids[WASI], "dep_kinds": [{"kind": None, "target": None}]})
                with self.assertRaisesRegex(Violation, "路径|入边"): check(data)

    def test_target_alias_and_root_target_drift_are_rejected(self):
        for parent, child in (("getrandom@0.2.17", WASI), ("getrandom@0.3.4", WASIP2), ("aline-ui@0.1.0", BACKEND)):
            data, ids = fixture(); item = next(e for e in data["nodes"][ids[parent]]["deps"] if e["pkg"] == ids[child])
            item["dep_kinds"][0]["target"] = None
            with self.assertRaisesRegex(Violation, "路径|入边"): check(data)

    def test_absent_native_root_keeps_existing_graph_rules_independent(self):
        data, _ = fixture(); data["policy"] = {}
        self.assertEqual(check(data), {"forbidden": set(), "sidecars": set()})

    def test_malformed_path_policy_fails_closed(self):
        for change in ("missing-source", "duplicate-node", "missing-edge", "unknown-field", "empty-scenarios"):
            data, _ = fixture(); rule = data["policy"]["native_path_exceptions"][WASI]
            if change == "missing-source": del rule["path"][1]["source"]
            elif change == "duplicate-node": rule["path"].append(copy.deepcopy(rule["path"][0]))
            elif change == "missing-edge": rule["edges"].pop()
            elif change == "unknown-field": rule["unreviewed"] = True
            else: rule["selected_scenarios"] = []
            with self.assertRaisesRegex(Violation, "规则|身份|场景"): check(data)


def run(output=None):
    output = output_directory("native-path-self-test", output); stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream, verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(NativePathTests))
    text = stream.getvalue(); write_text(output / "tests.txt", text)
    write_json(output / "summary.json", {"passed": result.wasSuccessful(), "tests": result.testsRun,
        "failures": len(result.failures), "errors": len(result.errors), "kind": "独立合成的最短路径回归，不是实际 Cargo 图或原生运行"})
    print(text); return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument("--output-dir", type=Path)
    raise SystemExit(run(parser.parse_args().output_dir))
