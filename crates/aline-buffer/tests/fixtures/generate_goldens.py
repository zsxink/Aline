#!/usr/bin/env python3
"""组 5 手工/固定种子金样生成器；只使用标准库，不写入仓库源码。"""

from __future__ import annotations

import bisect
import random
from pathlib import Path

HERE = Path(__file__).resolve().parent
SEED = 20261031

Patch = tuple[int, int, str]
BytePatch = tuple[int, int, bytes]


def utf8(value: str) -> bytes:
    return value.encode("utf-8")


def hexbytes(value: bytes) -> str:
    return value.hex()


def apply_bytes(before: bytes, patches: list[BytePatch]) -> bytes:
    """按 before 坐标逐段拼接；独立于 Rust 实现。"""
    out = bytearray()
    cursor = 0
    previous_end = 0
    for start, end, replacement in patches:
        if start < previous_end or start > end or end > len(before):
            raise ValueError("非法补丁顺序")
        out.extend(before[cursor:start])
        out.extend(replacement)
        cursor = end
        previous_end = end
    out.extend(before[cursor:])
    return bytes(out)


def inverse_bytes(before: bytes, patches: list[BytePatch]) -> list[BytePatch]:
    """按 after 坐标生成逆补丁，并合并相接的逆范围。"""
    inverse: list[BytePatch] = []
    delta = 0
    for start, end, replacement in patches:
        after_start = start + delta
        after_end = after_start + len(replacement)
        restored = before[start:end]
        if inverse and inverse[-1][1] == after_start:
            previous_start, _, previous_text = inverse[-1]
            inverse[-1] = (previous_start, after_end, previous_text + restored)
        else:
            inverse.append((after_start, after_end, restored))
        delta += len(replacement) - (end - start)
    return inverse


def effective(before: bytes, patches: list[BytePatch]) -> list[BytePatch]:
    return [
        (start, end, replacement)
        for start, end, replacement in patches
        if before[start:end] != replacement
    ]


def patch_text(patches: list[BytePatch]) -> str:
    if not patches:
        return "-"
    return ",".join(f"{start}:{end}:{hexbytes(text)}" for start, end, text in patches)


def parse_patch_hint(hint: str) -> list[BytePatch]:
    if hint == "-":
        return []
    result: list[BytePatch] = []
    for item in hint.split(","):
        start_text, end_text, replacement_hex = item.split(":")
        result.append((int(start_text), int(end_text), bytes.fromhex(replacement_hex)))
    return result


def check_handwritten(
    case_id: str,
    before_text: str,
    patches: list[Patch],
    after_text: str,
    inverse: list[Patch],
    before_selection: tuple[int, int],
    after_selection: tuple[int, int],
) -> None:
    before = utf8(before_text)
    encoded_patches = [(start, end, utf8(value)) for start, end, value in patches]
    encoded_inverse = [(start, end, utf8(value)) for start, end, value in inverse]
    after = apply_bytes(before, encoded_patches)
    if after != utf8(after_text):
        raise ValueError(f"{case_id}: 手写 after 与正补丁复算不一致: {after!r}")
    actual_inverse = inverse_bytes(before, effective(before, encoded_patches))
    if actual_inverse != encoded_inverse:
        raise ValueError(f"{case_id}: 手写 inverse 与独立复算不一致: {actual_inverse!r}")
    if encoded_inverse and apply_bytes(after, encoded_inverse) != before:
        raise ValueError(f"{case_id}: 逆补丁不能恢复 before")
    for offset in before_selection:
        if offset > len(before):
            raise ValueError(f"{case_id}: before 选区越界")
    for offset in after_selection:
        if offset > len(after):
            raise ValueError(f"{case_id}: after 选区越界")


CASES: list[tuple] = [
    ("hand-01", "abcXYZ", [(1, 2, "QQ"), (3, 6, "!")], "aQQc!", [(1, 3, "b"), (4, 5, "XYZ")], (6, 2), (5, 1), "changed"),
    ("hand-02", "a", [(1, 1, "b")], "ab", [(1, 2, "")], (1, 1), (2, 2), "changed"),
    ("hand-03", "ab", [(0, 1, "")], "b", [(0, 0, "a")], (0, 2), (1, 0), "changed"),
    ("hand-04", "abc", [(0, 1, ""), (1, 2, "")], "c", [(0, 0, "ab")], (3, 3), (1, 1), "changed"),
    ("hand-05", "ab", [(0, 1, "X"), (1, 1, "Y")], "XYb", [(0, 2, "a")], (2, 1), (3, 1), "changed"),
    ("hand-06", "甲a乙", [(3, 4, "🙂")], "甲🙂乙", [(3, 7, "a")], (7, 3), (10, 3), "changed"),
    ("hand-07", "a\r\nb", [(1, 3, "\n")], "a\nb", [(1, 2, "\r\n")], (4, 3), (3, 2), "changed"),
    ("hand-08", "0123456789", [(1, 3, "AB"), (6, 8, "")], "0AB34589", [(1, 3, "12"), (6, 6, "67")], (10, 0), (8, 2), "changed"),
    ("hand-09", "abc", [(0, 0, "甲")], "甲abc", [(0, 3, "")], (3, 0), (6, 3), "changed"),
    ("hand-10", "abc", [(1, 2, "b")], "abc", [], (2, 1), (2, 1), "unchanged"),
    ("hand-11", "abc", [(1, 1, "")], "abc", [], (1, 1), (1, 1), "unchanged"),
    ("hand-12", "abc", [(0, 0, ""), (1, 2, "B")], "aBc", [(1, 2, "b")], (3, 2), (3, 2), "changed"),
    ("hand-13", "abcd", [(1, 2, "X"), (2, 3, "")], "aXd", [(1, 2, "bc")], (4, 1), (3, 1), "changed"),
    ("hand-14", "abcdef", [(1, 2, "X"), (4, 5, "Y")], "aXcdYf", [(1, 2, "b"), (4, 5, "e")], (6, 2), (6, 2), "changed"),
]

ALPHABET = ["a", "b", "c", "甲", "乙", "😀", "\r\n", "\n", "\t", " "]


def boundaries(text: str) -> list[int]:
    values = [0]
    total = 0
    for char in text:
        total += len(char.encode("utf-8"))
        values.append(total)
    return values


def random_text(rng: random.Random, minimum: int = 8, maximum: int = 18) -> str:
    return "".join(rng.choice(ALPHABET) for _ in range(rng.randint(minimum, maximum)))


def random_case(rng: random.Random, index: int) -> tuple:
    while True:
        text = random_text(rng)
        cuts = sorted(rng.sample(range(len(boundaries(text))), 2 * rng.randint(1, 4)))
        patches: list[BytePatch] = []
        for position in range(0, len(cuts), 2):
            start_index = cuts[position]
            end_index = cuts[position + 1]
            starts = boundaries(text)
            start = starts[start_index]
            end = starts[end_index]
            if rng.random() < 0.25:
                end = start
            replacement = random_text(rng, 0, 2)
            patches.append((start, end, utf8(replacement)))
        patches.sort(key=lambda value: (value[0], value[1]))
        if not effective(utf8(text), patches):
            continue
        after = apply_bytes(utf8(text), patches)
        if len(after) > 256:
            continue
        inverse = inverse_bytes(utf8(text), effective(utf8(text), patches))
        if not inverse:
            continue
        after_starts = boundaries(after.decode("utf-8"))
        before_starts = boundaries(text)
        before_anchor = rng.choice(before_starts)
        before_head = rng.choice(before_starts)
        after_anchor = rng.choice(after_starts)
        after_head = rng.choice(after_starts)
        return (
            f"random-{index:03d}",
            text,
            patches,
            after.decode("utf-8"),
            inverse,
            (before_anchor, before_head),
            (after_anchor, after_head),
            "changed",
        )


def line_starts(text: bytes) -> list[int]:
    starts = [0]
    for offset, byte in enumerate(text):
        if byte == 0x0A:
            starts.append(offset + 1)
    return starts


def line_range(text: bytes, starts: list[int], line: int) -> tuple[int, int]:
    start = starts[line]
    end = starts[line + 1] if line + 1 < len(starts) else len(text)
    if end > start and text[end - 1] == 0x0A:
        end -= 1
        if end > start and text[end - 1] == 0x0D:
            end -= 1
    return start, end


def line_of_offset(starts: list[int], offset: int) -> int:
    return bisect.bisect_right(starts, offset) - 1


def line_index_rows() -> list[tuple]:
    cases = [
        ("empty", ""),
        ("ascii", "abc"),
        ("lf", "a\nb"),
        ("trailing-lf", "a\n"),
        ("crlf-mixed", "a\r\nb\nc\r\n"),
        ("isolated-cr", "a\rb"),
        ("crlf-only", "\r\n"),
        ("blank-lines", "a\r\n\r\nb"),
        ("unicode", "甲\n乙"),
        ("double-lf", "a\n\nb"),
    ]
    rows = []
    for case_id, text in cases:
        encoded = utf8(text)
        starts = line_starts(encoded)
        ranges = [line_range(encoded, starts, line) for line in range(len(starts))]
        offsets = [(offset, line_of_offset(starts, offset)) for offset in range(len(encoded) + 1)]
        rows.append(
            (
                case_id,
                hexbytes(encoded),
                100,
                len(starts),
                ",".join(f"{start}:{end}" for start, end in ranges),
                ",".join(f"{offset}:{line}" for offset, line in offsets),
                "ok",
            )
        )
    rows.append(("budget-reject", hexbytes(utf8("a\nb\nc")), 2, "-", "-", "-", "line_count_exceeded"))
    return rows


def main() -> None:
    for case in CASES:
        case_id, before, patches, after, inverse, before_selection, after_selection, _ = case
        check_handwritten(case_id, before, patches, after, inverse, before_selection, after_selection)

    rows = []
    for case in CASES:
        case_id, before, patches, after, inverse, before_selection, after_selection, expect = case
        before_bytes = utf8(before)
        after_bytes = utf8(after)
        encoded_patches = [(start, end, utf8(value)) for start, end, value in patches]
        encoded_inverse = [(start, end, utf8(value)) for start, end, value in inverse]
        actual = apply_bytes(before_bytes, encoded_patches)
        if actual != after_bytes:
            raise ValueError(f"{case_id}: after 复算失败")
        if expect == "changed":
            restored = apply_bytes(after_bytes, encoded_inverse)
            if restored != before_bytes:
                raise ValueError(f"{case_id}: 逆补丁往返失败")
        rows.append(
            (
                case_id,
                hexbytes(before_bytes),
                patch_text(encoded_patches),
                hexbytes(after_bytes),
                patch_text(encoded_inverse),
                before_selection[0],
                before_selection[1],
                after_selection[0],
                after_selection[1],
                expect,
            )
        )

    rng = random.Random(SEED)
    for index in range(120):
        case_id, before, patches, after, inverse, before_selection, after_selection, expect = random_case(rng, index)
        before_bytes = utf8(before)
        after_bytes = utf8(after)
        if apply_bytes(before_bytes, patches) != after_bytes:
            raise ValueError(f"{case_id}: 随机构造失败")
        if apply_bytes(after_bytes, inverse) != before_bytes:
            raise ValueError(f"{case_id}: 随机逆补丁往返失败")
        rows.append(
            (
                case_id,
                hexbytes(before_bytes),
                patch_text(patches),
                hexbytes(after_bytes),
                patch_text(inverse),
                before_selection[0],
                before_selection[1],
                after_selection[0],
                after_selection[1],
                expect,
            )
        )

    header = "\t".join(
        [
            "case_id",
            "before_hex",
            "patches",
            "after_hex",
            "inverse",
            "before_anchor",
            "before_head",
            "after_anchor",
            "after_head",
            "expect",
        ]
    )
    lines = [
        "# 组 5 正逆补丁金样；坐标 UTF-8 bytes，正补丁 before、逆补丁 after；seed=20261031",
        header,
    ]
    lines.extend("\t".join(map(str, row)) for row in rows)
    (HERE / "forward-inverse.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")

    line_header = "\t".join(["case_id", "text_hex", "max_lines", "line_count", "ranges", "offsets", "expect"])
    line_lines = [
        "# 组 5 行索引金样；行内容范围不含末尾 LF/CRLF；offset 归属其所在行",
        line_header,
    ]
    line_lines.extend("\t".join(map(str, row)) for row in line_index_rows())
    (HERE / "line-index.tsv").write_text("\n".join(line_lines) + "\n", encoding="utf-8")

    print(f"handwritten={len(CASES)} random=120 total={len(rows)}")
    print("forward-inverse.tsv lines=" + str(len(lines)))
    print("line-index.tsv lines=" + str(len(line_lines)))


if __name__ == "__main__":
    main()
