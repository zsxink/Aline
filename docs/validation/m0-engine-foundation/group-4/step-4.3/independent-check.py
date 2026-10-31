#!/usr/bin/env python3
"""标准库独立复核 4.3 手写金样，不调用 Rust 或读取其输出。"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BOM = b'\xef\xbb\xbf'


def body(data):
    data.decode('utf-8')
    return data[len(BOM):] if data.startswith(BOM) else data


def units(data):
    return re.findall(rb'\r\n|\n|[^\n\r]+|\r', data)


def canonical(data):
    return b''.join(b'\n' if chunk == b'\r\n' else chunk for chunk in units(data))


def main():
    names = set()
    for number, line in enumerate((ROOT / 'roundtrip-golden.v1.txt').read_text().splitlines(), 1):
        if not line or line.startswith('#'):
            continue
        name, raw, span, replacement, frozen, encoded, reopened = line.split('|')
        assert name not in names, (number, '重复名称')
        names.add(name)
        raw, replacement, frozen, encoded, reopened = map(bytes.fromhex, (raw, replacement, frozen, encoded, reopened))
        source = body(raw)
        start, end = map(int, span.split(':'))
        assert 0 <= start <= end <= len(source), name
        source[:start].decode('utf-8')
        source[:end].decode('utf-8')
        assert source[:start] + replacement + source[end:] == frozen, name
        assert body(encoded) == reopened, name
        assert canonical(frozen) == canonical(reopened), name
        if source == frozen:
            assert encoded == raw, name
        else:
            chunks = units(source)
            crlf_policy = chunks.count(b'\r\n') > chunks.count(b'\n')
            target = b'\r\n' if crlf_policy else b'\n'
            output = []
            for chunk in units(frozen):
                if chunk in (b'\r\n', b'\n'):
                    # LF 策略的孤立 CR 保护；其余单元遵守多数派。
                    output.append(b'\r\n' if chunk == b'\r\n' and output and output[-1].endswith(b'\r') else target)
                else:
                    output.append(chunk)
            header = BOM if raw.startswith(BOM) or frozen.startswith(BOM) else b''
            assert header + b''.join(output) == encoded, name
        restored = frozen[:start] + source[start:end] + frozen[start + len(replacement):]
        assert restored == source, name
    assert len(names) == 16, len(names)
    print(f'checked={len(names)} roundtrip/patch/inverse/encoding/isolated-CR/BOM expectations')


if __name__ == '__main__':
    main()
