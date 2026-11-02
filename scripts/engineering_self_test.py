"""实际隔离 Cargo 正反例及明确标记的图变异；输出可重跑的完整命令与诊断。"""

import copy
import json
import os
from pathlib import Path
import sys

from engineering_dependencies import MEMBERS, load_json
from evidence_paths import display_path, write_json
from validation_output import output_directory
from check_engineering import ADMISSION_POLICY, ROOT, POLICY, SCENARIOS, record_command, sha256


def run(output=None):
    output = output_directory('engineering-self-test', output)
    fixtures = output / "fixtures"
    fixtures.mkdir(exist_ok=True)
    results = []

    def command(label, argv, expected=0, diagnostic="", expectation="现行架构 §3.2/4 与任务 2.2"):
        out, rec = record_command(argv, output / "results", label)
        record = load_json(rec)
        text = out.read_text() + out.with_name(label + ".stderr.log").read_text()
        passed = record["exit_code"] == expected and diagnostic in text
        row = {"id": label, "exit_code": record["exit_code"], "expected_exit_code": expected,
               "expected_diagnostic": diagnostic, "passed": passed, "independent_expectation": expectation,
               "command_record": str(rec)}
        results.append(row)
        print(label, "符合预期" if passed else "失败", flush=True)
        return out, rec

    def invoke(label, project, metadata, tree, meta_record, tree_record, scenario="all", expected=1, diagnostic="", admission=None):
        argv = [sys.executable, str(ROOT / "scripts/check_engineering.py"), "--metadata", str(metadata), "--tree", str(tree),
                "--lock", str(project / "Cargo.lock"), "--scenario", scenario, "--metadata-command", str(meta_record), "--tree-command", str(tree_record),
                "--output-dir", str(output / "checks" / label)]
        if admission is not None:
            argv += ["--admission-policy", str(admission)]
        return command(label, argv, expected, diagnostic)

    # 所有同名禁库均为0.0.0本地合成包；不会安装真实发布物。
    packages = fixtures / "packages"
    def package(path, name, extra="", workspace=False):
        (path / "src").mkdir(parents=True, exist_ok=True)
        (path / "Cargo.toml").write_text('[package]\nname = "' + name + '"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n' + ('\n[workspace]\n' if workspace else '') + '\n' + extra)
        (path / "src/lib.rs").write_text('//! 明确的工程守卫合成夹具，不是真实发布库。\n')
    for name in ("gpui-pre", "gpui", "notify", "regex", "tokio", "wasmtime", "wasi", "comrak"):
        package(packages / name, name, workspace=True)

    def edge(name, path, table="dependencies", optional=False, manifest_dir=None):
        """依赖 path 相对清单目录书写，夹具文件因此不含本机绝对位置。"""
        target = os.path.relpath(path, manifest_dir) if manifest_dir is not None else str(path)
        return '[' + table + ']\nhidden_alias = { package = "' + name + '", path = "' + target + '"' + (', optional = true' if optional else '') + ' }\n' + ('[features]\nviolation = ["dep:hidden_alias"]\n' if optional else '')

    cases = [("positive-clean", None, None, "", "", None, 0, ""),
             ("positive-downward", None, None, "", "", None, 0, ""),
             ("negative-buffer-source", "aline-buffer", "aline-source", "dependencies", "", None, 1, "非法依赖边"),
             ("negative-markdown-edit", "aline-markdown", "aline-edit", "dependencies", "", None, 1, "非法依赖边"),
             ("negative-plugin-ui", "aline-plugin", "aline-ui", "dependencies", "", None, 1, "非法依赖边"),
             ("negative-buffer-bridge-source", "aline-buffer", "aline-source", "dependencies", "bridge", None, 1, "完整闭包内部非法方向"),
             ("negative-plugin-bridge-ui", "aline-plugin", "aline-ui", "dependencies", "bridge", None, 1, "完整闭包内部非法方向")]
    for kind, table, mode in [("alias", "dependencies", ""), ("dev", "dev-dependencies", ""), ("build", "build-dependencies", ""),
                              ("target", "target.'cfg(target_os = \"windows\")'.dependencies", ""), ("feature", "dependencies", "feature"), ("intermediate", "dependencies", "bridge")]:
        cases.append(("negative-engine-gpui-" + kind, "aline-buffer", "gpui-pre", table, mode, "all-features" if mode == "feature" else None, 1, "五引擎完整闭包泄漏"))
    for name in ("notify", "regex", "tokio", "wasmtime", "wasi", "comrak", "gpui"):
        diagnostic = "第二 Markdown" if name == "comrak" else ("禁止替代 GPUI" if name == "gpui" else "禁库或 GPUI 例外旁路")
        cases.append(("negative-ui-" + name, "aline-ui", name, "dependencies", "", None, 1, diagnostic))
    for label, parent, child, table, mode, feature, expected, diagnostic in cases:
        project = fixtures / label
        project.mkdir(exist_ok=True)
        (project / "Cargo.toml").write_text('[workspace]\nresolver = "3"\nmembers = [' + ','.join('"' + name + '"' for name in sorted(MEMBERS)) + ']\n')
        extras = {name: "" for name in MEMBERS}
        if parent:
            child_path = project / child if child in MEMBERS else packages / child
            if mode == "bridge":
                helper = packages / (label + "-helper")
                package(helper, "fixture-helper", edge(child, child_path, manifest_dir=helper), workspace=True)
                extras[parent] = edge("fixture-helper", helper, table, manifest_dir=project / parent)
            else:
                extras[parent] = edge(child, child_path, table, mode == "feature",
                                      manifest_dir=project / parent)
        if label == "positive-downward":
            chain = ["aline", "aline-ui", "aline-plugin", "aline-edit", "aline-markdown", "aline-buffer"]
            for a,b in zip(chain,chain[1:]):
                extras[a] = edge(b, project / b, manifest_dir=project / a)
            extras["aline-ui"] += '[dependencies.aline-source]\npath = "' + os.path.relpath(project / "aline-source", project / "aline-ui") + '"\n'
        for name in MEMBERS:
            package(project / name, name, extras[name])
        scenario = feature or "all"
        lock_generation = ["cargo", "generate-lockfile", "--manifest-path", str(project / "Cargo.toml"), "--offline"]
        command(label + "-initial-lock-generation", lock_generation,
                expectation="新 fixture 的首次锁文件生成例外；随后所有检查都使用 --locked")
        common = ["--manifest-path", str(project / "Cargo.toml"), "--locked", "--offline"]
        feature_args = ["--all-features"] if feature else []
        metadata, mr = command(label + "-metadata", ["cargo", "metadata"] + common + ["--format-version", "1"] + feature_args,
                               expectation="实际 Cargo 解析本地隔离 fixture 应成功；后续守卫判断违规")
        tree, tr = command(label + "-tree", ["cargo", "tree"] + common + ["--workspace", "--target", "all", "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}"] + feature_args,
                           expectation="实际 Cargo tree 保留真实包名/alias/kind/target；未执行构建")
        invoke(label, project, metadata, tree, mr, tr, scenario, expected, diagnostic)

    # 全部漂移样例基于正式实际图，但明确是合成变异，不当作 Cargo 原输出。
    baseline = ROOT / "scripts/fixtures/engineering/live-graphs"
    base = load_json(baseline / "all-metadata.json")
    base_tree = (baseline / "all-tree.txt").read_text()
    base_lock = (ROOT / "Cargo.lock").read_text()
    def mutation(label, mutate, diagnostic, expected=1):
        project = fixtures / label
        project.mkdir(exist_ok=True)
        data, tree, lock = mutate(copy.deepcopy(base), base_tree, base_lock)
        metadata, tree_path = project / "metadata.json", project / "tree.log"
        metadata.write_text(json.dumps(data)); tree_path.write_text(tree); (project / "Cargo.lock").write_text(lock)
        # 变异身份显式记录；仅为规则判定测试构造采集信封，不声称真实Cargo执行。
        records=[]
        for kind,path in (("metadata",metadata),("tree",tree_path)):
            argv=["cargo",kind,"--manifest-path",str(project / "Cargo.toml")]
            argv += ["--format-version","1"] if kind=="metadata" else ["--workspace","--target","all","-e","normal,build,dev"]
            record=project/(kind+".synthetic-command.json")
            write_json(record, {"evidence_kind":"明确合成变异，不是实际Cargo命令输出", "argv":argv,"exit_code":0,"stdout_sha256":sha256(path)})
            records.append(record)
        write_json(project / "fixture.json", {"kind":"明确的图/lock变异", "base":str(baseline),"transformation":label,"independent_expectation":"现行架构§3.2精确身份/边/target/features；任务2.2"})
        invoke(label,project,metadata,tree_path,*records,expected=expected,diagnostic=diagnostic)
    def node(data,name):
        identity=next(p["id"] for p in data["packages"] if p["name"]==name)
        return next(n for n in data["resolve"]["nodes"] if n["id"]==identity)
    def add_edge(data,parent,child):
        node(data,parent)["deps"].append({"name":"hidden_alias","pkg":node(data,child)["id"],"dep_kinds":[{"kind":None,"target":None}]})
    def bypass(data,tree,lock):
        add_edge(data,"aline-ui","regex");return data,tree,lock
    mutation("negative-existing-regex-bypass",bypass,"例外旁路")
    def noise_bypass(data,tree,lock):
        add_edge(data,"aline-ui","wasip2");tree="\n".join(line for line in tree.splitlines() if not line.startswith(("wasip2 v", "wit-bindgen v")))+"\n";return data,tree,lock
    mutation("negative-metadata-only-wasip2-bypass",noise_bypass,"metadata-only 自有禁库旁路")
    def checksum(data,tree,lock):
        from engineering_dependencies import parse_lock
        entry=parse_lock(ROOT/"Cargo.lock")["regex@1.13.1"]
        return data,tree,lock.replace(entry["checksum"],"0"*64)
    mutation("negative-regex-checksum",checksum,"来源/校验和漂移")
    def metadata_checksum(data,tree,lock):
        from engineering_dependencies import parse_lock
        entry=next(v for k,v in parse_lock(ROOT/"Cargo.lock").items() if k.startswith("rav1e@"))
        return data,tree,lock.replace(entry["checksum"],"0"*64)
    mutation("negative-metadata-only-checksum",metadata_checksum,"子图身份/来源/features/边/选包漂移")
    # getrandom存在多版本，按实际wasip2入边定位。
    def target(data,tree,lock):
        edge=next(e for n in data["resolve"]["nodes"] for e in n["deps"] if 'wasip2@' in e["pkg"])
        edge["dep_kinds"][0]["target"]='cfg(target_os = "windows")';return data,tree,lock
    mutation("negative-wasip2-target",target,"新增入边/target/kind")
    def kind(data,tree,lock):
        next(e for e in node(data,"gpui-pre")["deps"] if '#regex@' in e["pkg"])["dep_kinds"][0]["kind"]="build";return data,tree,lock
    mutation("negative-regex-kind",kind,"新增入边/target/kind")
    def features(data,tree,lock):
        node(data,"gpui-pre")["features"].append("screen-capture");return data,tree,lock
    mutation("negative-gpui-feature",features,"子图身份/来源/features/边/选包漂移")
    def version(data,tree,lock):
        p=next(p for p in data["packages"] if p["name"]=="gpui-pre");p["version"]="0.3.8"
        return data,tree.replace('gpui-pre v0.3.9','gpui-pre v0.3.8'),lock.replace('name = "gpui-pre"\nversion = "0.3.9"','name = "gpui-pre"\nversion = "0.3.8"')
    mutation("negative-gpui-version",version,"精确例外新增入边")
    def source(data,tree,lock):
        p=next(p for p in data["packages"] if p["name"]=="regex");p["source"]="registry+https://example.invalid/index"
        old='name = "regex"\nversion = "1.13.1"\nsource = "registry+https://github.com/rust-lang/crates.io-index"'
        new=old.replace('https://github.com/rust-lang/crates.io-index','https://example.invalid/index')
        return data,tree,lock.replace(old,new)
    mutation("negative-regex-source",source,"来源/校验和漂移")
    mutation("positive-synthetic-baseline",lambda d,t,l:(d,t,l),"",expected=0)
    # 许可证/分组准入缺失覆盖的正反例：仅变异已采集图，不安装任何真实库。
    def license_change(name, value, label, diagnostic, expected=1):
        def mutate(data, tree, lock):
            next(p for p in data["packages"] if p["name"] == name)["license"] = value
            return data, tree, lock
        mutation(label, mutate, diagnostic, expected=expected)
    license_change("serde", "GPL-3.0-only", "negative-license-gpl", "许可证未准入")
    license_change("serde", None, "negative-license-missing", "缺少许可证")
    license_change("serde", "MIT OR", "negative-license-expression", "许可证表达式无法解析")
    license_change("serde", "MIT AND LGPL-2.1-or-later", "negative-license-and", "许可证未准入")
    license_change("serde", "MIT OR (Apache-2.0", "negative-license-unclosed", "许可证表达式无法解析")
    license_change("rav1e", "GPL-3.0-only", "negative-license-metadata-only", "许可证未准入")
    # SPDX 正例：括号、WITH、旧式斜杠 OR、含拒绝原子的可满足 OR 均须通过。
    license_change("serde", "(MIT OR Apache-2.0) AND Unicode-3.0", "positive-license-parentheses", "", expected=0)
    license_change("serde", "Apache-2.0 WITH LLVM-exception OR MIT", "positive-license-with", "", expected=0)
    license_change("serde", "MIT/Apache-2.0", "positive-license-legacy-slash", "", expected=0)
    license_change("serde", "MIT OR GPL-3.0-only", "positive-license-or-election", "", expected=0)
    def local_path(data, tree, lock):
        identity = "path+file://<tmp>/aline-unreviewed-local#0.0.0"
        data["packages"].append({"name": "aline-unreviewed-local", "version": "0.0.0", "id": identity,
                                 "license": "MIT", "license_file": None, "source": None,
                                 "dependencies": [], "targets": [], "features": {}})
        data["resolve"]["nodes"].append({"id": identity, "dependencies": [], "deps": [], "features": []})
        ui = node(data, "aline-ui")
        ui["dependencies"].append(identity)
        ui["deps"].append({"name": "unreviewed_local", "pkg": identity,
                           "dep_kinds": [{"kind": None, "target": None}]})
        return data, tree, lock + '\n[[package]]\nname = "aline-unreviewed-local"\nversion = "0.0.0"\n'
    mutation("negative-admission-unreviewed-local-path", local_path, "未登记的本地路径包")
    def disconnected(data, tree, lock):
        identity = "registry+https://example.invalid/index#notify@0.0.0"
        data["packages"].append({"name": "notify", "version": "0.0.0", "id": identity,
                                 "license": "MIT", "license_file": None, "source": "registry+https://example.invalid/index",
                                 "dependencies": [], "targets": [], "features": {}})
        data["resolve"]["nodes"].append({"id": identity, "dependencies": [], "deps": [], "features": []})
        return data, tree, lock + '\n[[package]]\nname = "notify"\nversion = "0.0.0"\nsource = "registry+https://example.invalid/index"\nchecksum = "' + "0" * 64 + '"\n'
    mutation("negative-disconnected-metadata-package", disconnected, "不可从 workspace 到达")
    def extra_lock(data, tree, lock):
        return data, tree, lock + '\n[[package]]\nname = "orphan-lock-entry"\nversion = "0.0.0"\n'
    mutation("negative-extra-lock-package", extra_lock, "Cargo.lock 存在 metadata 未出现的包")
    empty_allowed = load_json(ADMISSION_POLICY)
    empty_allowed["license"]["allowed"] = []
    empty_allowed_path = fixtures / "negative-admission-empty-allowed-policy.json"
    empty_allowed_path.write_text(json.dumps(empty_allowed, ensure_ascii=False))
    invoke("negative-admission-empty-allowed", ROOT, baseline / "all-metadata.json", baseline / "all-tree.txt",
           baseline / "all-metadata.command.json", baseline / "all-tree.command.json",
           admission=empty_allowed_path, diagnostic="允许许可证集合为空")
    null_purpose = load_json(ADMISSION_POLICY)
    next(group for group in null_purpose["groups"] if group["id"] == "workspace")["purpose"] = None
    null_purpose_path = fixtures / "negative-admission-null-purpose-policy.json"
    null_purpose_path.write_text(json.dumps(null_purpose, ensure_ascii=False))
    invoke("negative-admission-null-purpose", ROOT, baseline / "all-metadata.json", baseline / "all-tree.txt",
           baseline / "all-metadata.command.json", baseline / "all-tree.command.json",
           admission=null_purpose_path, diagnostic="准入分组说明不完整")
    selected_disallowed = load_json(ADMISSION_POLICY)
    next(group for group in selected_disallowed["groups"] if group["id"] == "engine-app-core")["selected_allowed"] = False
    selected_disallowed_path = fixtures / "negative-admission-selected-disallowed-policy.json"
    selected_disallowed_path.write_text(json.dumps(selected_disallowed, ensure_ascii=False))
    invoke("negative-admission-selected-disallowed", ROOT, baseline / "all-metadata.json", baseline / "all-tree.txt",
           baseline / "all-metadata.command.json", baseline / "all-tree.command.json",
           admission=selected_disallowed_path, diagnostic="已选包未进入运行时准入分组")
    unassigned_policy = load_json(ADMISSION_POLICY)
    unassigned_policy["groups"] = [group for group in unassigned_policy["groups"] if group["id"] != "engine-app-core"]
    unassigned_path = fixtures / "negative-admission-unassigned-policy.json"
    unassigned_path.write_text(json.dumps(unassigned_policy, ensure_ascii=False))
    invoke("negative-admission-unassigned-group", ROOT, baseline / "all-metadata.json", baseline / "all-tree.txt",
           baseline / "all-metadata.command.json", baseline / "all-tree.command.json",
           admission=unassigned_path, diagnostic="已选包未进入运行时准入分组")
    branch_policy = load_json(ADMISSION_POLICY)
    del next(group for group in branch_policy["groups"] if group["id"] == "gpui")["branch_purposes"]["lyon"]
    branch_policy_path = fixtures / "negative-admission-branch-policy.json"
    branch_policy_path.write_text(json.dumps(branch_policy, ensure_ascii=False))
    invoke("negative-admission-unreviewed-branch", ROOT, baseline / "all-metadata.json", baseline / "all-tree.txt",
           baseline / "all-metadata.command.json", baseline / "all-tree.command.json",
           admission=branch_policy_path, diagnostic="直接依赖分支未登记")
    # 采集错误/标签/损坏输出的最小信封反例，不修改正式原始证据。
    for label,modify,diagnostic in [
        ("negative-acquisition-exit",lambda m,t:m.update(exit_code=101),"Cargo 采集失败"),
        ("negative-acquisition-hash",lambda m,t:m.update(stdout_sha256="0"*64),"输出 hash 不符"),
        ("negative-acquisition-target",lambda m,t:t["argv"].__setitem__(t["argv"].index("--target")+1,"x86_64-pc-windows-msvc"),"target 与场景标签不符"),
        ("negative-acquisition-feature",lambda m,t:m["argv"].append("--all-features"),"feature 与场景标签不符"),
        ("negative-acquisition-duplicate-target",lambda m,t:t["argv"].extend(["--target","x86_64-pc-windows-msvc"]),"重复采集参数")]:
        project=fixtures/label;project.mkdir(exist_ok=True)
        m=load_json(baseline/"all-metadata.command.json");t=load_json(baseline/"all-tree.command.json")
        # 使用正式文件和锁，不需要伪造内容。
        modify(m,t);mp=project/"metadata.command.json";tp=project/"tree.command.json";mp.write_text(json.dumps(m));tp.write_text(json.dumps(t))
        invoke(label,ROOT,baseline/"all-metadata.json",baseline/"all-tree.txt",mp,tp,diagnostic=diagnostic)
    # 确认失败Cargo本身不会转成通过，保留真实失败原输出。
    failed,fr=command("negative-real-cargo-acquisition",["cargo","metadata","--locked","--offline","--format-version","1","--manifest-path",str(fixtures/"missing/Cargo.toml")],101,"does not exist",expectation="不存在清单应由Cargo报错；不是合规图")
    invoke("negative-real-cargo-error-not-pass",ROOT,failed,baseline/"all-tree.txt",fr,baseline/"all-tree.command.json",diagnostic="Cargo 采集失败")
    summary={"passed":all(r["passed"] for r in results),"records":results,"limits":"本机host为aarch64 macOS；桌面target仅依赖解析。局部合成包均0.0.0，图漂移明确标记synthetic；未构建任何违规库或宣称平台支持。"}
    write_json(output/"summary.json", summary)
    print(json.dumps({"passed":summary["passed"],"commands":len(results),"summary":str(output/"summary.json")},ensure_ascii=False))
    return 0 if summary["passed"] else 1
