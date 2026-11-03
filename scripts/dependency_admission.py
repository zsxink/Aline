"""逐依赖准入账本：精确包身份、用途分组与许可证策略判定。

只消费已采集的 cargo metadata/tree/Cargo.lock 事实；任何缺失、无法解析或
未登记的分支都 fail closed。许可证结论是本工程准入策略的可复核查表，
不是法律意见；metadata 过近似包与实际选包在账本中分别标记。
"""

import json
import re
from collections import defaultdict

from engineering_dependencies import Violation, normalized_edge, package_key, reachable, route

_TOKEN = re.compile(r"\s*(\(|\)|[A-Za-z0-9][A-Za-z0-9.+-]*)")
_KEYWORDS = {"AND", "OR", "WITH"}


class ExpressionError(ValueError):
    """SPDX 表达式结构无效。"""


def normalize_license(expression, policy):
    """旧式 '/' 并列写法按 OR 归一；其余表达式保持原文。"""
    if policy.get("legacy_slash_means_or"):
        expression = re.sub(r"(?<=[A-Za-z0-9)])\s*/\s*(?=[A-Za-z0-9(])", " OR ", expression)
    return expression


def _tokens(expression):
    tokens = []
    position = 0
    while position < len(expression):
        match = _TOKEN.match(expression, position)
        if not match:
            raise ExpressionError("位置 %d 起无法识别" % position)
        tokens.append(match.group(1))
        position = match.end()
    if not tokens:
        raise ExpressionError("空表达式")
    return tokens


def parse_expression(expression):
    """递归下降解析 ID / AND / OR / WITH / 括号，返回语法树。"""
    tokens = _tokens(expression)
    index = 0

    def peek():
        return tokens[index] if index < len(tokens) else None

    def take():
        nonlocal index
        token = peek()
        if token is None:
            raise ExpressionError("表达式意外结束")
        index += 1
        return token

    def atom():
        token = peek()
        if token == "(":
            take()
            node = disjunction()
            if peek() != ")":
                raise ExpressionError("缺少右括号")
            take()
            return node
        if token is None or token == ")" or token in _KEYWORDS:
            raise ExpressionError("缺少许可证标识")
        name = take()
        if peek() == "WITH":
            take()
            exception = peek()
            if exception is None or exception == ")" or exception in _KEYWORDS:
                raise ExpressionError("WITH 缺少例外标识")
            name = name + " WITH " + take()
        return ("atom", name)

    def conjunction():
        node = atom()
        while peek() == "AND":
            take()
            node = ("and", node, atom())
        return node

    def disjunction():
        node = conjunction()
        while peek() == "OR":
            take()
            node = ("or", node, conjunction())
        return node

    node = disjunction()
    if index != len(tokens):
        raise ExpressionError("多余标记: " + " ".join(tokens[index:]))
    return node


def atoms(node):
    if node[0] == "atom":
        return {node[1]}
    return atoms(node[1]) | atoms(node[2])


def satisfied(node, allowed):
    """允许集合作为真值赋值；表达式可满足即存在合规选择（OR 选允许分支）。"""
    if node[0] == "atom":
        return node[1] in allowed
    if node[0] == "and":
        return satisfied(node[1], allowed) and satisfied(node[2], allowed)
    return satisfied(node[1], allowed) or satisfied(node[2], allowed)


def special_form(node):
    """AND/WITH 或含未允许原子的表达式需要在账本中单独列出选择依据。"""
    if node[0] == "atom":
        return " WITH " in node[1]
    return node[0] == "and" or special_form(node[1]) or special_form(node[2])


def license_review(package, policy, workspace, exact_exception=None):
    """返回单个包的许可证判定；未登记或许可证不可满足时抛 Violation。"""
    key = package_key(package)
    if workspace:
        return {"verdict": "workspace-exempt", "raw": package.get("license"), "normalized": None}
    raw = package.get("license")
    if not raw:
        detail = "仅声明 license-file" if package.get("license_file") else "无 license 字段"
        raise Violation("依赖缺少许可证准入: %s (%s)" % (key, detail))
    normalized = normalize_license(raw, policy)
    try:
        tree = parse_expression(normalized)
    except ExpressionError as error:
        raise Violation("依赖许可证表达式无法解析: %s (%s): %s" % (key, raw, error)) from error
    allowed = set(policy["allowed"])
    if exact_exception is not None:
        if normalized != exact_exception["license"]:
            raise Violation("精确许可证例外的许可证漂移: " + key)
        return {"verdict": "allowed-exact-exception", "raw": raw, "normalized": normalized,
                "allowed_atoms": [normalized], "basis": exact_exception["basis"],
                "exception_group": exact_exception["group"], "exception_root": exact_exception["root"]}
    if not satisfied(tree, allowed):
        denied = sorted(atoms(tree) - allowed)
        raise Violation("依赖许可证未准入: %s (%s); 未被允许集合覆盖: %s" % (key, raw, ", ".join(denied)))
    return {"verdict": "allowed", "raw": raw, "normalized": normalized,
            "allowed_atoms": sorted(atoms(tree) & allowed),
            "basis": "SPDX 表达式在允许集合下可满足；OR 可选允许分支，AND 全部需允许"}


def _root_identity(packages, name):
    matches = [identity for identity in packages if packages[identity]["name"] == name]
    if len(matches) != 1:
        raise Violation("准入分组根必须唯一: " + name)
    return matches[0]


def _branch_partition(packages, nodes, root):
    """按根的直接依赖划分闭包；交叉路径归首个分支，保留直接边事实。"""
    direct = {}
    for edge in sorted(nodes[root]["deps"], key=lambda edge: (packages[edge["pkg"]]["name"], edge["name"], package_key(packages[edge["pkg"]]))):
        direct.setdefault(edge["pkg"], edge)
    assignment = {root: None}
    for identity in direct:
        assignment[identity] = packages[identity]["name"]
    for identity, branch in list(assignment.items()):
        if identity == root:
            continue
        for reached in reachable(nodes, [identity], omit=root):
            assignment.setdefault(reached, branch)
    return assignment, direct


def _identity_spec(spec, label):
    if (not isinstance(spec, dict) or set(spec) != {"package", "source", "checksum"}
            or not isinstance(spec["package"], str) or not re.fullmatch(r"[A-Za-z0-9_-]+@[A-Za-z0-9.+-]+", spec["package"])
            or spec["source"] != "registry+https://github.com/rust-lang/crates.io-index"
            or not isinstance(spec["checksum"], str) or not re.fullmatch(r"[0-9a-f]{64}", spec["checksum"])):
        raise Violation("精确准入身份格式无效: " + label)


def _check_identity(package, lock, spec, label):
    entry = lock.get(spec["package"], {})
    if (package_key(package) != spec["package"] or package.get("source") != spec["source"]
            or package.get("id") != spec["source"] + "#" + spec["package"]
            or entry.get("source") != spec["source"] or entry.get("checksum") != spec["checksum"]):
        raise Violation("精确准入身份/来源/校验和漂移: " + label)


def _edge_specs(edges, label):
    if not isinstance(edges, list) or not edges:
        raise Violation("精确准入新增入边规则缺失: " + label)
    seen = set()
    for edge in edges:
        if (not isinstance(edge, dict) or set(edge) != {"from", "to", "alias", "kinds"}
                or any(not isinstance(edge[key], str) or not edge[key] for key in ("from", "to", "alias"))
                or not isinstance(edge["kinds"], list) or not edge["kinds"]):
            raise Violation("精确准入新增入边规则无效: " + label)
        for kind in edge["kinds"]:
            if (not isinstance(kind, dict) or set(kind) != {"kind", "target"}
                    or kind["kind"] not in (None, "build", "dev")
                    or (kind["target"] is not None and not isinstance(kind["target"], str))):
                raise Violation("精确准入新增入边 kind/target 无效: " + label)
        encoded = json.dumps(edge, sort_keys=True)
        if encoded in seen:
            raise Violation("精确准入新增入边规则重复: " + label)
        seen.add(encoded)


def _check_edges(actual, expected, label):
    key = lambda value: json.dumps(value, sort_keys=True)
    if sorted(actual, key=key) != sorted(expected, key=key):
        raise Violation("精确准入新增入边/target/kind: " + label)


def _apply_groups(packages, nodes, roots, policy, lock, incoming):
    groups = policy["groups"]
    by_id = {group["id"]: group for group in groups}
    if len(by_id) != len(groups):
        raise Violation("准入分组 id 重复")
    assigned = {}
    branches = {}
    group_roots = {}
    for group in groups:
        rule = group["rule"]
        kind = rule.get("kind")
        if kind == "workspace":
            members = set(roots)
            local_branches = {}
            root_ids = []
        elif kind == "closure":
            optional = rule.get("optional", False)
            if not isinstance(optional, bool):
                raise Violation("准入分组 optional 必须为布尔值: " + group["id"])
            exact = rule.get("root_identities")
            if exact is None and "root_incoming" in rule:
                raise Violation("精确准入分组根身份缺失: " + group["id"])
            if exact is not None:
                if not isinstance(exact, list) or len(exact) != len(rule["roots"]):
                    raise Violation("精确准入分组根身份缺失: " + group["id"])
                for spec in exact:
                    _identity_spec(spec, group["id"])
                if {spec["package"].split("@")[0] for spec in exact} != set(rule["roots"]):
                    raise Violation("精确准入分组根身份不匹配: " + group["id"])
                _edge_specs(rule.get("root_incoming"), group["id"])
                if {edge["to"] for edge in rule["root_incoming"]} != {spec["package"] for spec in exact}:
                    raise Violation("精确准入分组根入边目标不匹配: " + group["id"])
            root_ids = []
            for name in rule["roots"]:
                matches = [identity for identity, package in packages.items() if package["name"] == name]
                if not matches and optional:
                    continue
                root_id = _root_identity(packages, name)
                if exact is not None:
                    spec = next(spec for spec in exact if spec["package"].split("@")[0] == name)
                    _check_identity(packages[root_id], lock, spec, group["id"])
                    expected = [edge for edge in rule["root_incoming"] if edge["to"] == spec["package"]]
                    _check_edges(incoming[root_id], expected, group["id"])
                root_ids.append(root_id)
            members = reachable(nodes, root_ids)
            local_branches = {}
        elif kind == "branches":
            matches = [identity for identity in packages if packages[identity]["name"] == rule["root"]]
            if len(matches) > 1:
                raise Violation("准入分组根必须唯一: " + rule["root"])
            # 该依赖族未出现在当前图（如最小隔离夹具）时分组为空；出现即须逐分支已审。
            root_ids = matches
            local_branches, direct = _branch_partition(packages, nodes, matches[0]) if matches else ({}, {})
            members = set(local_branches)
            expected = set(group.get("branch_purposes", {}))
            actual = {packages[edge["pkg"]]["name"] for edge in direct.values()}
            # 新增直接分支必须重新审查；已登记分支暂时消失不构成未审代码。
            if matches and (actual - expected or not expected):
                raise Violation("准入分组直接依赖分支未登记: %s 实际=%s" % (group["id"], sorted(actual - expected)))
        elif kind == "remaining":
            root_ids = roots
            members = set(packages) - set(assigned)
            local_branches = {}
        else:
            raise Violation("未知准入分组规则: " + str(kind))
        for identity in members:
            if identity in assigned:
                continue
            assigned[identity] = group["id"]
            branches[identity] = local_branches.get(identity)
        group_roots[group["id"]] = root_ids
        if identity_problems(group):
            raise Violation("准入分组说明不完整: " + group["id"])
    missing = set(packages) - set(assigned)
    if missing:
        raise Violation("依赖未进入任何准入分组: " + ", ".join(sorted(package_key(packages[i]) for i in missing)))
    return assigned, branches, by_id, group_roots


def _exact_license_exceptions(policy, packages, nodes, roots, lock, assigned, groups, group_roots, incoming):
    rules = policy.get("exact_exceptions", [])
    if not isinstance(rules, list):
        raise Violation("精确许可证例外必须为列表")
    required = {"package", "source", "checksum", "license", "group", "root", "incoming", "basis"}
    seen, result = set(), {}
    for rule in rules:
        if (not isinstance(rule, dict) or set(rule) not in (required, required | {"path"})
                or any(not isinstance(rule[key], str) or not rule[key] for key in ("license", "group", "root", "basis"))):
            raise Violation("精确许可证例外格式无效")
        _identity_spec({key: rule[key] for key in ("package", "source", "checksum")}, "许可证例外")
        _edge_specs(rule["incoming"], rule["package"])
        if any(edge["to"] != rule["package"] for edge in rule["incoming"]):
            raise Violation("精确许可证例外入边目标不匹配: " + rule["package"])
        if rule["package"] in seen or rule["group"] not in groups:
            raise Violation("精确许可证例外重复或分组未登记: " + rule["package"])
        seen.add(rule["package"])
        group = groups[rule["group"]]
        exact_roots = group["rule"].get("root_identities", [])
        if group["rule"].get("kind") != "closure" or rule["root"] not in {item["package"] for item in exact_roots}:
            raise Violation("精确许可证例外分组根未精确登记: " + rule["package"])
        try:
            if atoms(parse_expression(rule["license"])) != {rule["license"]}:
                raise ExpressionError("例外只接受一个精确许可证原子")
        except ExpressionError as error:
            raise Violation("精确许可证例外许可证格式无效: " + str(error)) from error
        matches = [ident for ident, package in packages.items() if package_key(package) == rule["package"]]
        if not matches:
            continue
        if len(matches) != 1:
            raise Violation("精确许可证例外身份不唯一: " + rule["package"])
        ident = matches[0]
        _check_identity(packages[ident], lock, {key: rule[key] for key in ("package", "source", "checksum")}, rule["package"])
        if assigned[ident] != rule["group"]:
            raise Violation("精确许可证例外分组不符: " + rule["package"])
        root_ids = [root for root in group_roots[rule["group"]] if package_key(packages[root]) == rule["root"]]
        if len(root_ids) != 1 or ident in reachable(nodes, roots, omit=root_ids[0]):
            raise Violation("精确许可证例外出现闭包外路径: " + rule["package"])
        _check_edges(incoming[ident], rule["incoming"], rule["package"])
        if "path" in rule:
            path = rule["path"]
            if (not isinstance(path, dict) or set(path) != {"packages", "edges"}
                    or not isinstance(path["packages"], list) or len(path["packages"]) < 2
                    or not isinstance(path["edges"], list) or len(path["edges"]) != len(path["packages"]) - 1):
                raise Violation("精确许可证例外路径格式无效: " + rule["package"])
            for spec in path["packages"]:
                _identity_spec(spec, rule["package"] + " 路径")
            path_keys = [spec["package"] for spec in path["packages"]]
            if (len(set(path_keys)) != len(path_keys) or path_keys[0] != rule["root"]
                    or path_keys[-1] != rule["package"]):
                raise Violation("精确许可证例外路径根/终点不匹配: " + rule["package"])
            _edge_specs(path["edges"], rule["package"] + " 路径")
            if any(edge["from"] != path_keys[index] or edge["to"] != path_keys[index + 1]
                   for index, edge in enumerate(path["edges"])):
                raise Violation("精确许可证例外路径不连续: " + rule["package"])
            for spec in path["packages"]:
                path_id = next((candidate for candidate, package in packages.items()
                                if package_key(package) == spec["package"]), None)
                if path_id is None:
                    raise Violation("精确许可证例外路径包缺失: " + spec["package"])
                _check_identity(packages[path_id], lock, spec, rule["package"] + " 路径")
            for index, package_key_value in enumerate(path_keys[1:]):
                path_id = next(candidate for candidate, package in packages.items()
                               if package_key(package) == package_key_value)
                _check_edges(incoming[path_id], [path["edges"][index]],
                             rule["package"] + " 路径/" + package_key_value)
        result[ident] = rule
    return result


def identity_problems(group):
    required = ("id", "purpose", "consumer", "alternatives", "necessity")
    return [name for name in required
            if not isinstance(group.get(name), str) or not group[name].strip()]


def check_admission(packages, nodes, roots, selected, lock, policy, scenario, with_ledger=False):
    if policy.get("format") != 1 or not policy.get("reviewed"):
        raise Violation("依赖准入策略未登记或未审查")
    license_policy = policy.get("license", {})
    if not license_policy.get("allowed"):
        raise Violation("依赖准入允许许可证集合为空")
    engine_roots = {packages[root]["name"]: root for root in roots if packages[root]["name"] in
                    ("aline-source", "aline-buffer", "aline-markdown", "aline-edit", "aline-app-core")}
    engine_closures = {name: reachable(nodes, [root]) for name, root in engine_roots.items()}
    incoming = defaultdict(list)
    for parent in nodes:
        for edge in nodes[parent]["deps"]:
            incoming[edge["pkg"]].append(normalized_edge(packages, parent, edge))
    assigned, branches, groups, group_roots = _apply_groups(packages, nodes, roots, policy, lock, incoming)
    exceptions = _exact_license_exceptions(license_policy, packages, nodes, roots, lock, assigned, groups, group_roots, incoming)
    rows = []
    group_counts = defaultdict(int)
    verdicts = defaultdict(int)
    elections = []
    local_paths = []
    for identity in sorted(packages, key=lambda value: package_key(packages[value])):
        package = packages[identity]
        key = package_key(package)
        workspace = identity in roots
        group = groups[assigned[identity]]
        branch = branches.get(identity)
        if identity in selected and not group.get("selected_allowed", True):
            raise Violation("已选包未进入运行时准入分组: " + key)
        if not workspace and package.get("source") is None:
            local_paths.append(key)
        review = license_review(package, license_policy, workspace, exceptions.get(identity))
        verdicts[review["verdict"]] += 1
        group_counts[group["id"]] += 1
        if review["verdict"] == "allowed":
            tree = parse_expression(review["normalized"])
            if special_form(tree) or (atoms(tree) - set(license_policy["allowed"])):
                elections.append({"package": key, "license": review["raw"], "allowed_atoms": review["allowed_atoms"]})
        purpose = group.get("branch_purposes", {}).get(branch) if branch else None
        if branch and not purpose:
            raise Violation("准入分支缺少用途说明: %s/%s" % (group["id"], branch))
        rows.append({
            "package": key, "name": package["name"], "version": package["version"],
            "source": package.get("source"), "checksum": lock[key].get("checksum"),
            "group": group["id"] + (("/" + branch) if branch else ""),
            "selected": identity in selected, "features": sorted(nodes[identity]["features"]),
            "engine_consumers": sorted(name for name, closure in engine_closures.items() if identity in closure),
            "license": review, "incoming": sorted(incoming[identity], key=lambda edge: json.dumps(edge, sort_keys=True)),
            "route": route(packages, nodes, group_roots[group["id"]], identity),
            "purpose": purpose or group["purpose"],
            "consumer": group["consumer"], "alternatives": group["alternatives"], "necessity": group["necessity"],
        })
    if local_paths and license_policy.get("forbid_unreviewed_local_paths", True):
        raise Violation("依赖含未登记的本地路径包: " + ", ".join(local_paths))
    summary = {"scenario": scenario, "policy_reviewed": True,
               "packages": len(packages), "selected": len(selected),
               "groups": dict(sorted(group_counts.items())),
               "license_verdicts": dict(sorted(verdicts.items())),
               "elections": elections,
               "limits": "本账本核对登记策略与 metadata/lock/tree 身份；不构成法律意见，"
                         "不把 metadata 包存在当作已构建选包，也不证明跨平台原生构建。"}
    ledger = {"format": 1, "summary": summary, "groups": groups, "packages": rows,
              "policy": {"format": policy.get("format"), "review_evidence": policy.get("review_evidence"),
                         "license": license_policy}}
    return {"summary": summary, "ledger": ledger if with_ledger else None}
