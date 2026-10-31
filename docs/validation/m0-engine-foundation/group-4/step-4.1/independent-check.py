#!/usr/bin/env python3
"""任务 4.1 金样的独立复核：只按规范和 Python 标准库重算，不调用 Rust 实现。

检查字段数、成功用例的 UTF-8 解码与 BOM 剥离、CRLF/未配对 LF 计数、多数派、
未改 bytes 列，以及错误用例的预算先验和 UTF-8/UTF-16 分类。
"""

from pathlib import Path
import sys
from typing import Optional, Tuple

ROOT = Path(__file__).resolve().parent
GOLDEN = ROOT / "decode-golden.v1.txt"
BOM = b"\xef\xbb\xbf"


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def count_line_endings(data: bytes) -> Tuple[int, int]:
    crlf = 0
    unpaired_lf = 0
    index = 0
    while index < len(data):
        if data[index : index + 2] == b"\r\n":
            crlf += 1
            index += 2
        elif data[index : index + 1] == b"\n":
            unpaired_lf += 1
            index += 1
        else:
            index += 1
    return crlf, unpaired_lf


def utf8_error(data: bytes) -> Optional[Tuple[int, Optional[int]]]:
    try:
        data.decode("utf-8")
    except UnicodeDecodeError as error:
        if error.reason == "unexpected end of data":
            return (error.start, None)
        return (error.start, error.end - error.start)
    return None


def main() -> None:
    if not GOLDEN.is_file():
        fail(f"缺少金样 {GOLDEN}")
    ok = 0
    err = 0
    for line_number, line in enumerate(GOLDEN.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("|")
        if len(fields) != 11:
            fail(f"第 {line_number} 行字段数为 {len(fields)}，不是 11")
        (
            name,
            input_hex,
            budget_text,
            expected,
            source_hex,
            bom_text,
            crlf_text,
            lf_text,
            policy,
            unchanged_hex,
            error_tag,
        ) = fields
        data = bytes.fromhex(input_hex)
        source = bytes.fromhex(source_hex)
        unchanged = bytes.fromhex(unchanged_hex)
        budget = int(budget_text)
        if expected == "ok":
            if error_tag != "-":
                fail(f"{name}: 成功用例不应有错误标签")
            if budget != len(data):
                fail(f"{name}: 成功用例预算 {budget} 不等于输入长度 {len(data)}")
            text = data.decode("utf-8")  # 独立确认有效 UTF-8
            if bom_text == "1":
                if not data.startswith(BOM):
                    fail(f"{name}: 标记有 BOM 但字节不以 BOM 开头")
                body = data[len(BOM) :]
            else:
                body = data
            if body != source:
                fail(f"{name}: 仅剥离一个头部 BOM 后与源码列不符")
            if unchanged != data:
                fail(f"{name}: 未改 encode 列不等于输入 bytes")
            crlf, lf = count_line_endings(body)
            if (str(crlf), str(lf)) != (crlf_text, lf_text):
                fail(f"{name}: CRLF/未配对 LF 计数不符：实际 {(crlf, lf)}")
            expected_policy = "crlf" if crlf > lf else "lf"
            if policy != expected_policy:
                fail(f"{name}: 行尾策略应为 {expected_policy}，表中为 {policy}")
            if len(text.encode("utf-8")) != len(data):
                fail(f"{name}: UTF-8 往返长度不一致")
            ok += 1
        elif expected == "err":
            if error_tag == "-":
                fail(f"{name}: 错误用例缺少标签")
            if error_tag.startswith("budget:"):
                _, needed_text, limit_text = error_tag.split(":")
                if int(needed_text) != len(data):
                    fail(f"{name}: 预算错误 needed 与输入长度不符")
                if int(limit_text) != budget:
                    fail(f"{name}: 预算错误 limit 与 decode_budget 不符")
            else:
                if budget != len(data):
                    fail(f"{name}: 编码错误用例的预算必须等于输入长度，避免预算先拦截")
                if error_tag in ("utf16-bom:little", "utf16-bom:big"):
                    prefix = b"\xff\xfe" if error_tag.endswith("little") else b"\xfe\xff"
                    if not data.startswith(prefix):
                        fail(f"{name}: UTF-16 BOM 分类与字节前缀不符")
                elif error_tag.startswith("invalid-utf8:"):
                    _, valid_text, length_text = error_tag.split(":")
                    actual = utf8_error(data)
                    if actual is None:
                        fail(f"{name}: 标记为非法 UTF-8 但 Python 解码成功")
                    expected_length = None if length_text == "none" else int(length_text)
                    if actual != (int(valid_text), expected_length):
                        fail(f"{name}: UTF-8 错误偏移/长度不符：实际 {actual}")
                else:
                    fail(f"{name}: 未知错误标签 {error_tag}")
            err += 1
        else:
            fail(f"{name}: 未知 expected {expected}")
    print(f"checked={ok + err} ok={ok} err={err}")
    if (ok, err) != (21, 11):
        fail(f"用例数变化：实际 {(ok, err)}，报告预期 (21, 11)")


if __name__ == "__main__":
    main()
