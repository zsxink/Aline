#!/usr/bin/env python3
"""任务 4.2 编码金样的独立复核：按规范用 Python 标准库重算，不调用 Rust 实现。

复核字段数、原始解码事实、原 bytes 通道、多数派编码（含孤立 CR 紧邻 CRLF 的
保留例外与行首正文 U+FEFF 的保护性 BOM）、再解码行尾表示等价与预算错误。
参考编码器按规范手写，只用于核对金样表，不参与 Rust 测试。
"""

from pathlib import Path
import sys
from typing import Tuple

ROOT = Path(__file__).resolve().parent
GOLDEN = ROOT / "encode-golden.v1.txt"
BOM = b"\xef\xbb\xbf"


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def decode(data: bytes) -> Tuple[bytes, bool, str]:
    """返回（源码, 是否有头部 BOM, 行尾策略）；假定输入是有效 UTF-8。"""
    data.decode("utf-8")
    if data.startswith(BOM):
        body = data[len(BOM):]
        has_bom = True
    else:
        body = data
        has_bom = False
    crlf = 0
    lf = 0
    index = 0
    while index < len(body):
        if body[index:index + 2] == b"\r\n":
            crlf += 1
            index += 2
        elif body[index:index + 1] == b"\n":
            lf += 1
            index += 1
        else:
            index += 1
    return body, has_bom, "crlf" if crlf > lf else "lf"


def canonical(data: bytes) -> bytes:
    """把每个 CRLF/未配对 LF 行尾单元归一为 LF，孤立 CR 原样保留。"""
    out = bytearray()
    index = 0
    while index < len(data):
        if data[index:index + 2] == b"\r\n":
            out += b"\n"
            index += 2
        elif data[index:index + 1] == b"\n":
            out += b"\n"
            index += 1
        else:
            out.append(data[index])
            index += 1
    return bytes(out)


def encode_reference(source: bytes, policy: str, has_bom: bool) -> bytes:
    """按已批准语义编码：重放 BOM，行尾单元按多数派，孤立 CR 保留。

    基线无 BOM 而源码以 U+FEFF 开头时补写一个头部 BOM，避免再解码删除该
    正文字符（2026-10-31 裁决），该 BOM 计入预算。
    LF 策略下，紧跟在孤立 CR 后的 CRLF 单元若编码为单个 LF，会与前一个 CR
    拼成新的 CRLF 而丢失未配对 CR，因此该单元保留 CRLF。
    """
    out = bytearray(BOM if has_bom or source.startswith(BOM) else b"")
    index = 0
    while index < len(source):
        if source[index:index + 2] == b"\r\n":
            guarded = policy == "lf" and index > 0 and source[index - 1] == 0x0D
            out += b"\r\n" if policy == "crlf" or guarded else b"\n"
            index += 2
        elif source[index:index + 1] == b"\n":
            out += b"\n" if policy == "lf" else b"\r\n"
            index += 1
        else:
            out.append(source[index])
            index += 1
    return bytes(out)


def main() -> None:
    if not GOLDEN.is_file():
        fail(f"缺少金样 {GOLDEN}")
    names = set()
    ok = 0
    err = 0
    for line_number, line in enumerate(GOLDEN.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("|")
        if len(fields) != 9:
            fail(f"第 {line_number} 行字段数为 {len(fields)}，不是 9")
        (
            name,
            input_hex,
            frozen_hex,
            budget_text,
            expected,
            output_hex,
            reused_text,
            redecode_hex,
            error_tag,
        ) = fields
        if name in names:
            fail(f"用例名重复: {name}")
        names.add(name)
        data = bytes.fromhex(input_hex)
        frozen = bytes.fromhex(frozen_hex)
        budget = int(budget_text)
        source, has_bom, policy = decode(data)
        frozen_text = frozen.decode("utf-8")
        if expected == "ok":
            if error_tag != "-":
                fail(f"{name}: 成功用例不应有错误标签")
            output = bytes.fromhex(output_hex)
            redecode = bytes.fromhex(redecode_hex)
            if len(output) > budget:
                fail(f"{name}: 输出 {len(output)} 字节超过预算 {budget}")
            if frozen == source:
                if reused_text != "1":
                    fail(f"{name}: 冻结源码与基线相等时必须走原 bytes 通道")
                if output != data:
                    fail(f"{name}: 原 bytes 通道必须逐字节返回输入")
            else:
                if reused_text != "0":
                    fail(f"{name}: 内容已变化时不得声称原 bytes 通道")
                expected_output = encode_reference(frozen, policy, has_bom)
                if output != expected_output:
                    fail(
                        f"{name}: 编码结果不符：期望 {expected_output.hex().upper()}，"
                        f"表中 {output_hex}"
                    )
            if redecode != decode(output)[0]:
                fail(f"{name}: 再解码源码与 redecode_hex 不符")
            if canonical(frozen) != canonical(redecode):
                fail(f"{name}: 冻结源码与再解码结果不满足限定行尾等价")
            expected_bom = has_bom or (frozen != source and frozen.startswith(BOM))
            if output.startswith(BOM) != expected_bom:
                fail(f"{name}: 输出 BOM 与重放/保护规则不一致")
            ok += 1
        elif expected == "err":
            if not error_tag.startswith("budget:"):
                fail(f"{name}: 只接受 budget 错误标签")
            _, needed_text, limit_text = error_tag.split(":")
            needed = int(needed_text)
            limit = int(limit_text)
            if limit != budget:
                fail(f"{name}: 错误 limit 与 decode 预算列不符")
            expected_output = data if frozen == source else encode_reference(frozen, policy, has_bom)
            if needed != len(expected_output):
                fail(f"{name}: 预算错误 needed 应为 {len(expected_output)}，表中 {needed}")
            if needed <= limit:
                fail(f"{name}: 预算错误 needed 必须大于 limit")
            if output_hex or redecode_hex or reused_text != "-":
                fail(f"{name}: 错误行的输出/再解码/复用列必须为空")
            err += 1
        else:
            fail(f"{name}: 未知 expected {expected}")
    print(f"checked={ok + err} ok={ok} err={err}")
    if (ok, err) != (34, 4):
        fail(f"用例数变化：实际 {(ok, err)}，报告预期 (34, 4)")


if __name__ == "__main__":
    main()
