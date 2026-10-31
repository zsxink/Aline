#!/usr/bin/env python3
"""组 6 独立金样核对：不导入 Rust，不调用 aline-markdown。

对每个夹具重算手工金样的结构约束：

1. 块与间隙区间严格升序、互不重叠、无缝覆盖 0..源长度；
2. 每个区间端点都是源文本的 UTF-8 字符边界；
3. 块尾不含 ``\\n``/``\\r``（分隔换行归入间隙）；
4. 间隙只由 ``\\n``/``\\r`` 组成；
5. 金样声明的源长度与磁盘字节数一致。

脚本只断言金样自身的结构与源码字节一致，不判断块种类是否符合
CommonMark——那属于 Rust 测试与独立复核的职责。退出码非 0 表示发现不一致。
"""

import sys
from pathlib import Path


def read_bytes(path):
    return path.read_bytes()


def parse_range(value):
    start, sep, end = value.partition("..")
    if not sep:
        raise ValueError(f"区间缺少 '..': {value!r}")
    return int(start), int(end)


def load_spec(tsv_path):
    """返回 {fixture: [(kind, start, end, block_kind)]}。"""
    entries = {}
    for line in tsv_path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        columns = line.split("\t")
        if len(columns) != 4:
            raise ValueError(f"列数不是 4: {line!r}")
        name, kind, span, block_kind = columns
        start, end = parse_range(span)
        entries.setdefault(name, []).append((kind, start, end, block_kind))
    return entries


def load_basic(tsv_path):
    entries = []
    for line in tsv_path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        columns = line.split("\t")
        if len(columns) != 3:
            raise ValueError(f"列数不是 3: {line!r}")
        kind, span, block_kind = columns
        start, end = parse_range(span)
        entries.append((kind, start, end, block_kind))
    return entries


def char_boundaries(data):
    """返回所有 UTF-8 字符边界（含 0 与 len）。"""
    boundaries = {0}
    index = 0
    while index < len(data):
        byte = data[index]
        if byte < 0x80:
            index += 1
        elif byte >> 5 == 0b110:
            index += 2
        elif byte >> 4 == 0b1110:
            index += 3
        elif byte >> 3 == 0b11110:
            index += 4
        else:
            raise ValueError(f"非法 UTF-8 起始字节 {byte:#x} @ {index}")
        boundaries.add(index)
    return boundaries


def check_fixture(name, entries, fixtures_dir):
    problems = []
    path = fixtures_dir / name
    data = read_bytes(path)
    boundaries = char_boundaries(data)

    ordered = sorted(entries, key=lambda entry: entry[1])
    cursor = 0
    for kind, start, end, block_kind in ordered:
        if start != cursor:
            problems.append(
                f"{name}: {kind} {start}..{end} 未接续上一个区间（期望起点 {cursor}）"
            )
        if end < start:
            problems.append(f"{name}: {kind} {start}..{end} 起点大于终点")
        if start not in boundaries or end not in boundaries:
            problems.append(f"{name}: {kind} {start}..{end} 端点不在 UTF-8 字符边界")
        piece = data[start:end]
        if kind == "block":
            if piece.endswith(b"\n") or piece.endswith(b"\r"):
                problems.append(f"{name}: 块 {start}..{end} 尾含分隔换行")
        elif kind == "gap":
            if piece and piece.strip(b"\r\n"):
                problems.append(f"{name}: 间隙 {start}..{end} 含非换行字节")
        else:
            problems.append(f"{name}: 未知区间种类 {kind!r}")
        cursor = end
    if cursor != len(data):
        problems.append(f"{name}: 覆盖终止于 {cursor}，源长度 {len(data)}")
    return problems


def main(argv):
    if len(argv) != 2:
        print("用法: verify_goldens.py <仓库根>", file=sys.stderr)
        return 2
    root = Path(argv[1])
    fixtures = root / "crates" / "aline-markdown" / "tests" / "fixtures" / "m0"

    problems = []

    spec_entries = load_spec(fixtures / "spec.blocks.tsv")
    total_spec = 0
    for name, entries in sorted(spec_entries.items()):
        total_spec += len(entries)
        problems.extend(check_fixture(name, entries, fixtures))

    basic_entries = load_basic(fixtures / "basic.blocks.tsv")
    problems.extend(check_fixture("basic.md", basic_entries, fixtures))

    fixture_files = sorted(path.name for path in fixtures.glob("*.md"))
    declared = set(spec_entries) | {"basic.md"}
    for name in fixture_files:
        if name == "README.md":
            continue
        if name not in declared:
            problems.append(f"{name}: 夹具没有对应的块/间隙金样")

    print(f"spec.blocks.tsv 夹具数: {len(spec_entries)}，区间数: {total_spec}")
    print(f"basic.blocks.tsv 区间数: {len(basic_entries)}")
    print(f"夹具文件数: {len([n for n in fixture_files if n != 'README.md'])}")
    if problems:
        for problem in problems:
            print(f"问题: {problem}")
        print(f"结果: {len(problems)} 个问题")
        return 1
    print("结果: 0 个问题；所有区间升序、无缝覆盖、端点合法、块尾无分隔换行、间隙只含换行")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
