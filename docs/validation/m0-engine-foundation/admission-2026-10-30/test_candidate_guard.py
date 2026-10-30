"""候选守卫正反例；所有本地 0.0.0 禁库包均是合成夹具，不是真实发布物。"""

import copy
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
RESULTS = HERE / "guard-results"
FIXTURES = HERE / "guard-fixtures"
GUARD = HERE / "candidate_guard.py"
RESULTS.mkdir(exist_ok=True)
FIXTURES.mkdir(exist_ok=True)
ROOT = Path(tempfile.mkdtemp(prefix="aline-candidate-guard-tests-"))
BASE = json.loads((HERE / "metadata-base-final.json").read_text())
TREE = (HERE / "tree-all-base-packages.stdout.log").read_text()
LOCK = (HERE / "candidate" / "Cargo.lock").read_text()
POLICY = json.loads((HERE / "guard_policy.json").read_text())
RECORDS = []
SHAPES = []
ENV = os.environ.copy()
ENV.update(CARGO_HOME=json.loads((HERE / "isolation.json").read_text())["cargo_home"], RUSTUP_TOOLCHAIN="1.98.1")


def run(label, argv, expected_code, diagnostic="", stdout_path=None, expected_source="现行准入规则"):
    """记录实际执行；非零必须同时匹配预期诊断，不能把任意失败当拦截成功。"""
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    result = subprocess.run(argv, env=ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    stdout = stdout_path or RESULTS / (label + ".stdout.log")
    stdout.write_text(result.stdout)
    (RESULTS / (label + ".stderr.log")).write_text(result.stderr)
    record = {"id": label, "argv": argv, "cwd": str(Path.cwd()), "started_utc": started,
              "exit_code": result.returncode, "expected_exit_code": expected_code,
              "expected_diagnostic": diagnostic, "independent_expectation": expected_source,
              "passed": result.returncode == expected_code and diagnostic in result.stdout + result.stderr,
              "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    (RESULTS / (label + ".command.json")).write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
    RECORDS.append(record)
    print(label, result.returncode, "预期一致" if record["passed"] else "失败", flush=True)
    return result


def invoke(label, metadata, tree, lock, scenario="base-final", scope="app", expected=1, diagnostic="", meta_command=None, tree_command=None):
    args = [sys.executable, str(GUARD), "--metadata", str(metadata), "--tree", str(tree), "--lock", str(lock),
            "--scenario", scenario, "--scope", scope]
    if meta_command:
        args += ["--metadata-command", str(meta_command)]
    if tree_command:
        args += ["--tree-command", str(tree_command)]
    return run(label, args, expected, diagnostic)


def mutation(label, mutate, diagnostic, expected=1, scope="app", scenario="base-final", base_scenario="base-final"):
    baseline = POLICY["scenarios"][base_scenario]
    base_metadata, base_tree = HERE / baseline["metadata"], HERE / baseline["tree"]
    data, tree, lock = json.loads(base_metadata.read_text()), base_tree.read_text(), LOCK
    data, tree, lock = mutate(data, tree, lock)
    directory = ROOT / label
    directory.mkdir()
    metadata, tree_file, lock_file = directory / "metadata.json", directory / "tree.stdout.log", directory / "Cargo.lock"
    metadata.write_text(json.dumps(data))
    tree_file.write_text(tree)
    lock_file.write_text(lock)
    (FIXTURES / (label + ".json")).write_text(json.dumps({"kind": "明确合成的输入变异，不是真实 Cargo 输出", "base": baseline["metadata"] + " / " + baseline["tree"] + " / candidate/Cargo.lock", "transformation": label, "expected_diagnostic": diagnostic, "expected_exit_code": expected, "metadata_sha256": hashlib.sha256(metadata.read_bytes()).hexdigest(), "temporary_inputs": str(directory)}, ensure_ascii=False, indent=2) + "\n")
    return invoke(label, metadata, tree_file, lock_file, scenario, scope, expected, diagnostic,
                  base_metadata.with_suffix(".command.json"), base_tree.with_name(base_tree.name.replace(".stdout.log", ".command.json")))


def add_package(data, tree, lock, name, parent=None, kind=None, target=None, alias=None):
    identity = "path+file:///explicit-synthetic-fixture/" + name + "#" + name + "@0.0.0"
    data["packages"].append({"id": identity, "name": name, "version": "0.0.0", "source": None})
    data["resolve"]["nodes"].append({"id": identity, "deps": [], "dependencies": [], "features": []})
    root = parent or data["resolve"]["root"]
    node = next(n for n in data["resolve"]["nodes"] if n["id"] == root)
    node["deps"].append({"name": alias or name.replace("-", "_"), "pkg": identity, "dep_kinds": [{"kind": kind, "target": target}]})
    node["dependencies"].append(identity)
    return data, tree + name + " v0.0.0 (/explicit-synthetic-fixture)\n", lock + '\n[[package]]\nname = "' + name + '"\nversion = "0.0.0"\n'


def local_package(path, name, extra=""):
    (path / "src").mkdir(parents=True, exist_ok=True)
    (path / "Cargo.toml").write_text('[package]\nname = "' + name + '"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[lints.rust]\nunsafe_code = "forbid"\n\n' + extra)
    (path / "src" / "lib.rs").write_text('//! 第 1 组显式本地合成夹具，不是同名真实库的发布内容。\npub fn fixture() {}\n')


# 已审实际组合正例，直接消费留存的 Cargo 输出。
for scenario, record in POLICY["scenarios"].items():
    if scenario == "clean":
        continue
    invoke("positive-" + scenario, HERE / record["metadata"], HERE / record["tree"], HERE / "candidate" / "Cargo.lock", scenario, expected=0)

# 真实 Cargo 解析的本地最小夹具；名称用来验证真实 package/别名/kind/target 采集。
packages = FIXTURES / "packages"
for name in ("notify", "regex", "tokio", "wasmtime", "wasi", "comrak", "gpui"):
    local_package(packages / name, name)
local_package(packages / "bridge", "fixture-bridge", '[dependencies]\nparser_alias = { package = "comrak", path = "../comrak" }\n')
cases = [
    ("positive-clean", "", None, 0, ""),
    ("negative-real-alias-regex", '[dependencies]\nhidden = { package = "regex", path = "../../packages/regex" }\n', None, 1, "禁用包"),
    ("negative-real-target-notify", '[target.\'cfg(target_os = "windows")\'.dependencies]\nwatcher = { package = "notify", path = "../../packages/notify" }\n', None, 1, "禁用包"),
    ("negative-real-build-tokio", '[build-dependencies]\nruntime_alias = { package = "tokio", path = "../../packages/tokio" }\n', None, 1, "禁用包"),
    ("negative-real-dev-wasmtime", '[dev-dependencies]\nhost_alias = { package = "wasmtime", path = "../../packages/wasmtime" }\n', None, 1, "禁用包"),
    ("negative-real-feature-wasi", '[dependencies]\nsandbox = { package = "wasi", path = "../../packages/wasi", optional = true }\n[features]\nviolation = ["dep:sandbox"]\n', "violation", 1, "禁用包"),
    ("negative-real-intermediate-parser", '[dependencies]\nbridge = { package = "fixture-bridge", path = "../../packages/bridge" }\n', None, 1, "第二 Markdown"),
    ("negative-real-alias-gpui", '[dependencies]\ngui_alias = { package = "gpui", path = "../../packages/gpui" }\n', None, 1, "禁止替代 GPUI"),
]
for label, extra, feature, expected, diagnostic in cases:
    project = FIXTURES / "real-cargo" / label
    local_package(project, "fixture-" + label, extra)
    metadata = RESULTS / (label + ".metadata.json")
    tree = RESULTS / (label + ".tree.stdout.log")
    common = ["--offline", "--manifest-path", str(project / "Cargo.toml")]
    if feature:
        common += ["--features", feature]
    meta_record = run(label + ".metadata", ["cargo", "+1.98.1", "metadata", "--format-version", "1"] + common, 0, stdout_path=metadata, expected_source="本地 Cargo 图应成功解析；违规由后续守卫判定")
    tree_record = run(label + ".tree", ["cargo", "+1.98.1", "tree", "--target", "all", "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}"] + common, 0, stdout_path=tree, expected_source="本地 Cargo 图应成功解析；与最小 manifest 独立核对别名/kind/target")
    if meta_record.returncode or tree_record.returncode:
        continue
    if expected:
        data = json.loads(metadata.read_text())
        nodes = {n["id"]: n for n in data["resolve"]["nodes"]}
        expected_edge = {
            "negative-real-alias-regex": ("hidden", None, None),
            "negative-real-target-notify": ("watcher", None, 'cfg(target_os = "windows")'),
            "negative-real-build-tokio": ("runtime_alias", "build", None),
            "negative-real-dev-wasmtime": ("host_alias", "dev", None),
            "negative-real-feature-wasi": ("sandbox", None, None),
            "negative-real-intermediate-parser": ("bridge", None, None),
            "negative-real-alias-gpui": ("gui_alias", None, None),
        }[label]
        alias, kind, target = expected_edge
        edges = nodes[data["resolve"]["root"]]["deps"]
        shape_ok = any(e["name"] == alias and {"kind": kind, "target": target} in e["dep_kinds"] for e in edges)
        if label == "negative-real-intermediate-parser":
            shape_ok = shape_ok and any(e["name"] == "parser_alias" for n in nodes.values() for e in n["deps"])
        SHAPES.append({"fixture": label, "expected_alias_kind_target": expected_edge, "passed": shape_ok,
                       "independent_expectation": "真实夹具 manifest 中明确声明的 alias/kind/target"})
    invoke(label, metadata, tree, project / "Cargo.lock", "clean", "engine", expected, diagnostic)
    if label == "negative-real-alias-gpui":
        invoke("negative-real-alias-gpui-app", metadata, tree, project / "Cargo.lock", "clean", "app", 1, "禁止替代 GPUI")

# 精确例外存在时，额外入边仍必须拒绝。
def regex_side(data, tree, lock):
    regex = next(p["id"] for p in data["packages"] if p["name"] == "regex")
    node = next(n for n in data["resolve"]["nodes"] if n["id"] == data["resolve"]["root"])
    node["deps"].append({"name": "hidden_regex_alias", "pkg": regex, "dep_kinds": [{"kind": None, "target": None}]})
    node["dependencies"].append(regex)
    return data, tree, lock
mutation("negative-existing-regex-bypass", regex_side, "旁路")
mutation("negative-existing-gpui-pre-plus-raw-gpui", lambda d,t,l:add_package(d,t,l,"gpui"), "禁止替代 GPUI")
mutation("negative-engine-gpui-exception", lambda d,t,l: (d,t,l), "五引擎禁止 GPUI", scope="engine")

def nested_engine(data, tree, lock):
    data, tree, lock = add_package(data, tree, lock, "aline-buffer")
    engine = next(n for n in data["resolve"]["nodes"] if "#aline-buffer@0.0.0" in n["id"])
    gpui = next(p["id"] for p in data["packages"] if p["name"] == "gpui-pre")
    engine["deps"].append({"name": "gui_alias", "pkg": gpui, "dep_kinds": [{"kind": None, "target": None}]})
    engine["dependencies"].append(gpui)
    return data, tree, lock
mutation("negative-nested-engine-gpui", nested_engine, "实际五引擎旁路")

def kind_drift(data, tree, lock):
    gpui = next(n for n in data["resolve"]["nodes"] if "#gpui-pre@" in n["id"])
    next(e for e in gpui["deps"] if "#regex@" in e["pkg"])["dep_kinds"][0]["kind"] = "build"
    return data, tree, lock
mutation("negative-kind-drift", kind_drift, "target/kind")

def change_feature(data, tree, lock):
    node = next(n for n in data["resolve"]["nodes"] if "#gpui-pre@" in n["id"])
    node["features"].append("screen-capture")
    return data, tree, lock
mutation("negative-feature-drift", change_feature, "子图")

def change_target(data, tree, lock):
    for node in data["resolve"]["nodes"]:
        for edge in node["deps"]:
            if "#wasip2@" in edge["pkg"]:
                edge["dep_kinds"][0]["target"] = None
    return data, tree, lock
mutation("negative-target-drift", change_target, "target/kind")

def checksum(data, tree, lock):
    name = 'name = "regex"\nversion = "1.13.1"'
    start = lock.index(name)
    end = lock.index('[[package]]',start)
    entry = lock[start:end]
    import re
    entry = re.sub(r'checksum = "[0-9a-f]+"', 'checksum = "'+'0'*64+'"', entry)
    return data, tree, lock[:start]+entry+lock[end:]
mutation("negative-checksum-drift", checksum, "校验和")

def source_drift(data, tree, lock):
    next(p for p in data["packages"] if p["name"] == "regex")["source"] = "registry+https://synthetic.invalid/index"
    return data, tree, lock
mutation("negative-source-drift", source_drift, "来源")

def gpui_source_drift(data, tree, lock):
    package = next(p for p in data["packages"] if p["name"] == "gpui-pre")
    original = package["source"]
    package["source"] = "registry+https://explicit-synthetic-fixture.invalid/index"
    start = lock.index('name = "gpui-pre"\nversion = "0.3.9"')
    end = lock.index('[[package]]', start)
    return data, tree, lock[:start] + lock[start:end].replace(original, package["source"]) + lock[end:]
mutation("negative-gpui-source-drift", gpui_source_drift, "子图")

def unselected_source_drift(data, tree, lock):
    package = next(p for p in data["packages"] if p["name"] == "wasi" and p["version"] == "0.11.1+wasi-snapshot-preview1")
    assert 'wasi v0.11.1+wasi-snapshot-preview1' not in tree
    start = lock.index('name = "wasi"\nversion = "0.11.1+wasi-snapshot-preview1"')
    end = lock.index('[[package]]', start)
    return data, tree, lock[:start] + lock[start:end].replace(package["source"], "registry+https://explicit-synthetic-fixture.invalid/index") + lock[end:]
mutation("negative-unselected-gpui-lock-source", unselected_source_drift, "完整 GPUI 子图 lock/metadata 来源不符", scenario="test-support-resolve", base_scenario="test-support-resolve")

def unselected_checksum_invalid(data, tree, lock):
    assert 'wasi v0.11.1+wasi-snapshot-preview1' not in tree
    start = lock.index('name = "wasi"\nversion = "0.11.1+wasi-snapshot-preview1"')
    end = lock.index('[[package]]', start)
    import re
    entry = re.sub(r'checksum = "[0-9a-f]+"', 'checksum = "explicit-invalid-fixture"', lock[start:end])
    return data, tree, lock[:start] + entry + lock[end:]
mutation("negative-unselected-gpui-checksum-invalid", unselected_checksum_invalid, "完整 GPUI 子图 registry 校验和无效", scenario="test-support-resolve", base_scenario="test-support-resolve")

def version_drift(data, tree, lock):
    next(p for p in data["packages"] if p["name"] == "regex")["version"] = "0.0.0-fixture-drift"
    return data, tree, lock
mutation("negative-version-drift", version_drift, "不能对应 metadata")
mutation("negative-unreviewed-scenario", lambda d,t,l:(d,t,l), "未审组合", scenario="unreviewed-inspector")
mutation("negative-empty-tree", lambda d,t,l:(d,"",l), "tree 为空")
mutation("negative-omitted-selected-package", lambda d,t,l:(d,t.replace('regex v1.13.1\n',''),l), "可达性不一致")
def unselected_notify(data, tree, lock):
    data, changed_tree, lock = add_package(data, tree, lock, "notify")
    return data, tree, lock
mutation("negative-unselected-own-notify", unselected_notify, "选包差异")

# 临时候选根可替换，不能要求整个锁文件字节一致。
def root_rename(data, tree, lock):
    root = next(p for p in data["packages"] if p["id"] == data["resolve"]["root"])
    old = root["name"]
    root["name"] = "app"
    return data, tree.replace(old+' v0.0.0','app v0.0.0'), lock.replace('name = "'+old+'"','name = "app"')
mutation("positive-renamed-root", root_rename, "", expected=0)
mutation("positive-added-clean-node", lambda d,t,l:add_package(d,t,l,"fixture-clean-helper"), "", expected=0)

# Cargo 失败必须由命令记录拒绝；记录来自真实失败的 cargo metadata 调用。
failed_meta = RESULTS / "negative-cargo-failed.metadata.json"
run("negative-cargo-failed.metadata", ["cargo", "+1.98.1", "metadata", "--offline", "--format-version", "1", "--manifest-path", str(ROOT / "missing" / "Cargo.toml")], 101, "does not exist", stdout_path=failed_meta)
invoke("negative-cargo-failed", failed_meta, HERE / "tree-all-base-packages.stdout.log", HERE / "candidate" / "Cargo.lock", expected=1, diagnostic="采集命令未成功", tree_command=HERE / "tree-all-base-packages.command.json")
missing = ROOT / "missing-tree.stdout.log"
invoke("negative-tree-missing", HERE / "metadata-base-final.json", missing, HERE / "candidate" / "Cargo.lock", expected=1, diagnostic="结构错误", tree_command=HERE / "tree-all-base-packages.command.json")
invoke("negative-metadata-missing", ROOT / "missing-metadata.json", HERE / "tree-all-base-packages.stdout.log", HERE / "candidate" / "Cargo.lock", expected=1, diagnostic="无法解析 JSON", meta_command=HERE / "metadata-base-final.command.json", tree_command=HERE / "tree-all-base-packages.command.json")
corrupt = ROOT / "corrupt.json"
corrupt.write_text('{"packages":')
invoke("negative-metadata-malformed", corrupt, HERE / "tree-all-base-packages.stdout.log", HERE / "candidate" / "Cargo.lock", expected=1, diagnostic="无法解析 JSON", meta_command=HERE / "metadata-base-final.command.json", tree_command=HERE / "tree-all-base-packages.command.json")

# 移除变异后直接复查原始数据，证明没有污染正例。
invoke("positive-restored-original", HERE / "metadata-base-final.json", HERE / "tree-all-base-packages.stdout.log", HERE / "candidate" / "Cargo.lock", expected=0)
summary = {"actual_commands": len(RECORDS), "all_expected": all(r["passed"] for r in RECORDS) and all(s["passed"] for s in SHAPES), "fixture_shape_checks": SHAPES, "temporary_mutation_root": str(ROOT), "records": RECORDS,
           "fixture_notice": "guard-fixtures/packages 的同名 0.0.0 库明确为本地合成夹具；负例不是实际禁库发布包兼容验证。"}
(RESULTS / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2)+'\n')
sys.exit(0 if summary["all_expected"] else 1)
