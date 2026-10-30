"""正式依赖守卫的局部回归；不是任务 2.2 所需的完整 --self-test。"""

import copy
import json
from pathlib import Path
import sys

from engineering_dependencies import Violation, check_structure, model, parse_lock

ROOT = Path(__file__).resolve().parent.parent
BASE = ROOT / "docs/validation/m0-engine-foundation/group-2/engineering-dependency-baseline/all-metadata.stdout.json"


def main():
    original = json.loads(BASE.read_text())
    results = []
    for parent, child, intermediate in [("aline-buffer", "aline-source", False), ("aline-markdown", "aline-edit", False),
                                       ("aline-plugin", "aline-ui", False), ("aline-buffer", "aline-source", True),
                                       ("aline-plugin", "aline-ui", True), ("aline-ui", "aline-source", False)]:
        data = copy.deepcopy(original)
        packages, nodes, roots, _ = model(data, parse_lock(ROOT / "Cargo.lock"))
        parent_id = next(i for i in roots if packages[i]["name"] == parent)
        child_id = next(i for i in roots if packages[i]["name"] == child)
        if intermediate:
            identity = "path+file:///synthetic-regression/helper#0.0.0"
            packages[identity] = {"name": "synthetic-helper", "version": "0.0.0"}
            nodes[identity] = {"id": identity, "deps": [{"name": "child_alias", "pkg": child_id, "dep_kinds": [{"kind": None, "target": None}]}]}
            child_id = identity
        nodes[parent_id]["deps"].append({"name": "hidden_alias", "pkg": child_id, "dep_kinds": [{"kind": None, "target": None}]})
        expected = parent == "aline-ui"
        diagnostic = ""
        try:
            check_structure(packages, nodes, roots)
            accepted = True
        except Violation as error:
            accepted, diagnostic = False, str(error)
        passed = accepted == expected and (accepted or parent in diagnostic)
        results.append({"synthetic_graph_mutation": {"parent": parent, "child": child, "intermediate": intermediate},
                        "expected_accept": expected, "accepted": accepted, "diagnostic": diagnostic, "passed": passed,
                        "independent_expectation": "架构 §4 允许方向与任务 2.2 明列反例；没有冒充实际 Cargo 采集"})
    print(json.dumps({"scope": "局部回归，完整自测未执行", "results": results}, ensure_ascii=False, indent=2))
    return 0 if all(row["passed"] for row in results) else 1


if __name__ == "__main__":
    sys.exit(main())
