#!/usr/bin/env python3
"""验证 workspace lint 声明和成员继承，并运行隔离的 Rust 诊断样例。

本脚本自带 TOML 1.0 子集解析器。成员 [lints] 的判定必须落在真实解析结果上：
逐行扫描会把多行字符串里的伪 [lints] 表当成真实表（F-08）。无法安全解释的
构造一律抛 CheckError，不静默忽略。
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
MEMBERS = {
    "aline": "app",
    "aline-app-core": "crates/aline-app-core",
    "aline-buffer": "crates/aline-buffer",
    "aline-edit": "crates/aline-edit",
    "aline-markdown": "crates/aline-markdown",
    "aline-plugin": "crates/aline-plugin",
    "aline-source": "crates/aline-source",
    "aline-ui": "crates/aline-ui",
}
EXPECTED_RUST = {"unsafe_code": "forbid", "missing_docs": "warn"}
EXPECTED_CLIPPY = {
    "unwrap_used": "deny",
    "expect_used": "deny",
    "panic": "deny",
    "unreachable": "deny",
    "cast_possible_truncation": "deny",
    "cast_sign_loss": "deny",
    "dbg_macro": "deny",
    "todo": "deny",
    "unimplemented": "deny",
}

# F-08 夹具原文：伪继承成员的 Cargo.toml。真实 Cargo 把 [lints] / workspace = true
# 读作 description 字符串内容，该成员实际没有 [lints] 表。
DECOY_MEMBER_TEXT = (
    '[package]\n'
    'description = """\n'
    '[lints]\n'
    'workspace = true\n'
    '[package.metadata.decoy]\n'
    '"""\n'
    'name = "aline-buffer"\n'
    'version = "0.0.0"\n'
    'edition = "2024"\n'
)
REAL_MEMBER_TEXT = (
    '[package]\n'
    'name = "aline-buffer"\n'
    'version = "0.0.0"\n'
    'edition = "2024"\n'
    '[lints]\n'
    'workspace = true\n'
)


class CheckError(Exception):
    """lint 声明、隔离夹具或实际诊断与预期不符。"""


class TomlDatetime(object):
    """TOML 日期/时间字面量的不透明表示；仅保证解析完整，不参与 lint 语义。"""

    __slots__ = ("text",)

    def __init__(self, text):
        self.text = text

    def __eq__(self, other):
        return isinstance(other, TomlDatetime) and self.text == other.text

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        return hash(("toml-datetime", self.text))

    def __repr__(self):
        return "TomlDatetime(%r)" % self.text


_BARE_KEY_CHARS = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-")
_ESCAPES = {"b": "\b", "t": "\t", "n": "\n", "f": "\f", "r": "\r", '"': '"', "\\": "\\"}
_HEX_INT = re.compile(r"0x[0-9A-Fa-f](?:_?[0-9A-Fa-f])*")
_OCT_INT = re.compile(r"0o[0-7](?:_?[0-7])*")
_BIN_INT = re.compile(r"0b[01](?:_?[01])*")
_DEC_INT = re.compile(r"[+-]?(?:0|[1-9](?:_?[0-9])*)")
_FLOAT = re.compile(
    r"[+-]?(?:inf|nan)"
    r"|[+-]?(?:0|[1-9](?:_?[0-9])*)\.[0-9](?:_?[0-9])*(?:[eE][+-]?[0-9](?:_?[0-9])*)?"
    r"|[+-]?(?:0|[1-9](?:_?[0-9])*)[eE][+-]?[0-9](?:_?[0-9])*"
)
_DATETIME = re.compile(
    r"(?P<date>\d{4}-\d{2}-\d{2})[Tt ](?P<time>\d{2}:\d{2}:\d{2})(?:\.\d+)?"
    r"(?P<offset>[Zz]|[+-]\d{2}:\d{2})?"
)
_DATE = re.compile(r"\d{4}-\d{2}-\d{2}")
_TIME = re.compile(r"\d{2}:\d{2}:\d{2}(?:\.\d+)?")
_VALUE_END_CHARS = " \t\n,]}#"


class _TomlParser(object):
    """TOML 1.0 子集递归下降解析器；遇到无法安全解释的构造抛 CheckError。"""

    def __init__(self, text, source):
        if text.startswith("\ufeff"):
            raise CheckError("%s:1: 拒绝带 BOM 的 TOML 文件（无法安全解释）" % source)
        text = text.replace("\r\n", "\n")
        if "\r" in text:
            raise CheckError("%s:1: 拒绝裸 CR 行结束符（无法安全解释）" % source)
        self.s = text
        self.n = len(text)
        self.i = 0
        self.source = source
        self.doc = {}
        self.current = self.doc
        # 表/数组的身份状态：explicit（[t] 定义）、implicit（被点分键或子表隐式创建）、
        # array（[[t]] 的元素或数组本身）、inline（内联表，TOML 禁止之后再扩展）。
        self.state = {}

    # -- 基础工具 ---------------------------------------------------------

    def _peek(self):
        return self.s[self.i] if self.i < self.n else ""

    def _line(self, pos=None):
        return self.s.count("\n", 0, self.i if pos is None else pos) + 1

    def _fail(self, message, pos=None):
        raise CheckError("%s:%d: %s" % (self.source, self._line(pos), message))

    def _skip_inline_ws(self):
        while self.i < self.n and self.s[self.i] in " \t":
            self.i += 1

    def _skip_comment(self):
        self.i += 1  # '#'
        while self.i < self.n and self.s[self.i] != "\n":
            code = ord(self.s[self.i])
            if (code < 0x20 and code != 0x09) or code == 0x7F:
                self._fail("注释中出现控制字符")
            self.i += 1

    def _skip_ws_nl(self):
        while self.i < self.n:
            ch = self.s[self.i]
            if ch in " \t\n":
                self.i += 1
            elif ch == "#":
                self._skip_comment()
            else:
                break

    def _expect_line_end(self):
        self._skip_inline_ws()
        if self._peek() == "#":
            self._skip_comment()
        if self.i >= self.n:
            return
        if self.s[self.i] != "\n":
            self._fail("行尾存在无法解释的内容")
        self.i += 1

    def _value_boundary(self, pos):
        return pos >= self.n or self.s[pos] in _VALUE_END_CHARS

    # -- 文档结构 ---------------------------------------------------------

    def parse(self):
        while True:
            self._skip_ws_nl()
            if self.i >= self.n:
                return self.doc
            if self.s[self.i] == "[":
                self._parse_header()
            else:
                self._parse_key_value()

    def _parse_header(self):
        start = self.i
        if self.s.startswith("[[", self.i):
            self.i += 2
            keys = self._parse_key()
            if not self.s.startswith("]]", self.i):
                self._fail("数组表头缺少 ']]'", start)
            self.i += 2
            self._define_array_table(keys, start)
        else:
            self.i += 1
            keys = self._parse_key()
            if self._peek() != "]":
                self._fail("表头缺少 ']'", start)
            self.i += 1
            self._define_table(keys, start)
        self._expect_line_end()

    def _parse_key(self):
        keys = []
        while True:
            self._skip_inline_ws()
            ch = self._peek()
            if ch == '"':
                keys.append(self._parse_basic_string())
            elif ch == "'":
                keys.append(self._parse_literal_string())
            elif ch in _BARE_KEY_CHARS:
                begin = self.i
                while self.i < self.n and self.s[self.i] in _BARE_KEY_CHARS:
                    self.i += 1
                keys.append(self.s[begin:self.i])
            else:
                self._fail("无法识别 TOML 键")
            self._skip_inline_ws()
            if self._peek() == ".":
                self.i += 1
                continue
            return keys

    def _parse_key_value(self):
        start = self.i
        keys = self._parse_key()
        self._skip_inline_ws()
        if self._peek() != "=":
            self._fail("键值对缺少 '='", start)
        self.i += 1
        self._skip_inline_ws()
        value = self._parse_value()
        self._insert(self.current, keys, value, start)
        self._expect_line_end()

    def _insert(self, table, keys, value, pos):
        node = table
        for key in keys[:-1]:
            if key not in node:
                node[key] = {}
                # 点分键创建的表之后不得再被 [表头] 定义（Cargo 视为 duplicate key）。
                self.state[id(node[key])] = "dotted"
            child = node[key]
            if not isinstance(child, dict):
                self._fail("点分键 %r 与非表值冲突" % key, pos)
            if self.state.get(id(child)) == "inline":
                self._fail("内联表 %r 不可再扩展" % key, pos)
            node = child
        last = keys[-1]
        if last in node:
            self._fail("重复 TOML 键 %r" % last, pos)
        node[last] = value
        self._mark_inline(value)

    def _mark_inline(self, value):
        """值位置出现的表（内联表）及其嵌套结构都不可再被 [表头] 定义。"""
        if isinstance(value, dict):
            self.state[id(value)] = "inline"
            for child in value.values():
                self._mark_inline(child)
        elif isinstance(value, list):
            for child in value:
                self._mark_inline(child)

    def _descend(self, node, key, pos):
        if key not in node:
            node[key] = {}
            self.state[id(node[key])] = "implicit"
        child = node[key]
        if isinstance(child, list):
            if not child or not isinstance(child[-1], dict):
                self._fail("表路径 %r 无法安全解析" % key, pos)
            child = child[-1]
        if not isinstance(child, dict):
            self._fail("表路径 %r 与非表值冲突" % key, pos)
        if self.state.get(id(child)) == "inline":
            self._fail("内联表 %r 不可再扩展" % key, pos)
        return child

    def _define_table(self, keys, pos):
        node = self.doc
        for key in keys[:-1]:
            node = self._descend(node, key, pos)
        last = keys[-1]
        if last in node:
            child = node[last]
            if isinstance(child, list):
                if not child or not isinstance(child[-1], dict):
                    self._fail("表 %r 与已有数组表同名" % ".".join(keys), pos)
                child = child[-1]
            elif not isinstance(child, dict):
                self._fail("表 %r 与非表值冲突" % ".".join(keys), pos)
            kind = self.state.get(id(child))
            if kind == "explicit":
                self._fail("重复 TOML 表 %r" % ".".join(keys), pos)
            if kind == "array":
                self._fail("普通表 %r 与已有数组表同名" % ".".join(keys), pos)
            if kind == "inline":
                self._fail("内联表 %r 不可再扩展" % ".".join(keys), pos)
            if kind == "dotted":
                # 点分键已定义过该表；Cargo 报 duplicate key，[表头] 不得再次定义它。
                self._fail("点分键已定义表 %r，不得再用表头定义" % ".".join(keys), pos)
            node = child
        else:
            node[last] = {}
            node = node[last]
        self.state[id(node)] = "explicit"
        self.current = node

    def _define_array_table(self, keys, pos):
        node = self.doc
        for key in keys[:-1]:
            node = self._descend(node, key, pos)
        last = keys[-1]
        if last not in node:
            node[last] = []
            self.state[id(node[last])] = "array"
        child = node[last]
        if not isinstance(child, list):
            self._fail("数组表 %r 与已有非数组值冲突" % ".".join(keys), pos)
        if self.state.get(id(child)) != "array":
            self._fail("数组表 %r 与已有非数组表冲突" % ".".join(keys), pos)
        element = {}
        self.state[id(element)] = "array"
        child.append(element)
        self.current = element

    # -- 值 ---------------------------------------------------------------

    def _parse_value(self):
        ch = self._peek()
        if ch == '"':
            if self.s.startswith('"""', self.i):
                return self._parse_multiline_basic_string()
            return self._parse_basic_string()
        if ch == "'":
            if self.s.startswith("'''", self.i):
                return self._parse_multiline_literal_string()
            return self._parse_literal_string()
        if ch == "[":
            return self._parse_array()
        if ch == "{":
            return self._parse_inline_table()
        if ch in ("t", "f"):
            return self._parse_bool()
        return self._parse_number_or_datetime()

    def _parse_bool(self):
        for literal, value in (("true", True), ("false", False)):
            if self.s.startswith(literal, self.i):
                end = self.i + len(literal)
                if end < self.n and self.s[end] in _BARE_KEY_CHARS:
                    self._fail("布尔字面量后紧跟无法解释的字符")
                self.i = end
                return value
        self._fail("无法识别的 TOML 值")

    def _parse_array(self):
        self.i += 1  # '['
        items = []
        while True:
            self._skip_ws_nl()
            if self._peek() == "]":
                self.i += 1
                return items
            items.append(self._parse_value())
            self._skip_ws_nl()
            ch = self._peek()
            if ch == ",":
                self.i += 1
                continue
            if ch == "]":
                self.i += 1
                return items
            self._fail("数组中缺少 ',' 或 ']'")

    def _parse_inline_table(self):
        self.i += 1  # '{'
        table = {}
        self._skip_ws_nl()
        if self._peek() == "}":
            self.i += 1
            return table
        while True:
            self._skip_ws_nl()
            start = self.i
            keys = self._parse_key()
            self._skip_inline_ws()
            if self._peek() != "=":
                self._fail("内联表键值对缺少 '='", start)
            self.i += 1
            self._skip_inline_ws()
            value = self._parse_value()
            self._insert(table, keys, value, start)
            self._skip_ws_nl()
            ch = self._peek()
            if ch == ",":
                self.i += 1
                self._skip_ws_nl()
                if self._peek() == "}":
                    self._fail("内联表不允许尾随逗号")
                continue
            if ch == "}":
                self.i += 1
                return table
            self._fail("内联表中缺少 ',' 或 '}'")

    def _parse_number_or_datetime(self):
        start = self.i
        match = _DATETIME.match(self.s, self.i)
        if match and self._value_boundary(match.end()):
            self.i = match.end()
            self._validate_date(match.group("date"), start)
            self._validate_time(match.group("time"), start)
            offset = match.group("offset")
            if offset and offset not in ("Z", "z"):
                hours, minutes = offset[1:].split(":")
                if int(hours) > 23 or int(minutes) > 59:
                    self._fail("日期时间偏移非法", start)
            return TomlDatetime(match.group(0))
        match = _DATE.match(self.s, self.i)
        if match and self._value_boundary(match.end()):
            self.i = match.end()
            self._validate_date(match.group(0), start)
            return TomlDatetime(match.group(0))
        match = _TIME.match(self.s, self.i)
        if match and self._value_boundary(match.end()):
            self.i = match.end()
            self._validate_time(match.group(0), start)
            return TomlDatetime(match.group(0))
        match = _FLOAT.match(self.s, self.i)
        if match and self._value_boundary(match.end()):
            self.i = match.end()
            return float(match.group(0).replace("_", ""))
        for pattern, base in ((_HEX_INT, 16), (_OCT_INT, 8), (_BIN_INT, 2)):
            match = pattern.match(self.s, self.i)
            if match and self._value_boundary(match.end()):
                self.i = match.end()
                return self._make_integer(match.group(0), base, start)
        match = _DEC_INT.match(self.s, self.i)
        if match and self._value_boundary(match.end()):
            self.i = match.end()
            return self._make_integer(match.group(0), 10, start)
        self._fail("无法识别的 TOML 值", start)

    def _make_integer(self, text, base, start):
        cleaned = text.replace("_", "")
        value = int(cleaned, 10) if base == 10 else int(cleaned[2:], base)
        if value < -(2 ** 63) or value >= 2 ** 63:
            self._fail("整数超出 TOML i64 范围: %r" % text, start)
        return value

    def _validate_date(self, text, start):
        try:
            datetime.date(int(text[0:4]), int(text[5:7]), int(text[8:10]))
        except ValueError:
            self._fail("非法日期字面量 %r" % text, start)

    def _validate_time(self, text, start):
        hour, minute, second = (int(part) for part in text.split(":")[:3])
        if hour > 23 or minute > 59 or second > 60:
            self._fail("非法时间字面量 %r" % text, start)

    # -- 字符串 -----------------------------------------------------------

    def _parse_escape(self):
        start = self.i
        self.i += 1  # '\\'
        if self.i >= self.n:
            self._fail("转义序列不完整", start)
        ch = self.s[self.i]
        if ch in _ESCAPES:
            self.i += 1
            return _ESCAPES[ch]
        if ch in ("u", "U"):
            width = 4 if ch == "u" else 8
            self.i += 1
            digits = self.s[self.i:self.i + width]
            if len(digits) != width or not all(c in "0123456789abcdefABCDEF" for c in digits):
                self._fail("\\%s 转义需要 %d 位十六进制" % (ch, width), start)
            self.i += width
            code = int(digits, 16)
            if 0xD800 <= code <= 0xDFFF or code > 0x10FFFF:
                self._fail("\\%s 转义码位非法" % ch, start)
            return chr(code)
        self._fail("未知转义序列 \\%s" % ch, start)

    def _reject_control(self, ch, allow_tab=True):
        code = ord(ch)
        if (code < 0x20 and (not allow_tab or code != 0x09)) or code == 0x7F:
            self._fail("字符串中出现控制字符")

    def _parse_basic_string(self):
        start = self.i
        self.i += 1  # '"'
        out = []
        while True:
            if self.i >= self.n:
                self._fail("基本字符串未闭合", start)
            ch = self.s[self.i]
            if ch == '"':
                self.i += 1
                return "".join(out)
            if ch == "\\":
                out.append(self._parse_escape())
                continue
            if ch == "\n":
                self._fail("基本字符串不能跨行", start)
            self._reject_control(ch)
            out.append(ch)
            self.i += 1

    def _parse_literal_string(self):
        start = self.i
        self.i += 1  # "'"
        out = []
        while True:
            if self.i >= self.n:
                self._fail("字面字符串未闭合", start)
            ch = self.s[self.i]
            if ch == "'":
                self.i += 1
                return "".join(out)
            if ch == "\n":
                self._fail("字面字符串不能跨行", start)
            self._reject_control(ch)
            out.append(ch)
            self.i += 1

    def _parse_multiline_basic_string(self):
        start = self.i
        self.i += 3  # '"""'
        if self.i < self.n and self.s[self.i] == "\n":
            self.i += 1
        out = []
        while True:
            if self.i >= self.n:
                self._fail("多行基本字符串未闭合", start)
            if self.s.startswith('"""', self.i):
                count = 0
                while self.i + count < self.n and self.s[self.i + count] == '"':
                    count += 1
                if count > 5:
                    self._fail("多行基本字符串结尾引号过多", start)
                out.append('"' * (count - 3))
                self.i += count
                return "".join(out)
            ch = self.s[self.i]
            if ch == "\\":
                j = self.i + 1
                while j < self.n and self.s[j] in " \t":
                    j += 1
                if j < self.n and self.s[j] == "\n":
                    self.i = j + 1
                    while self.i < self.n and self.s[self.i] in " \t\n":
                        self.i += 1
                    continue
                out.append(self._parse_escape())
                continue
            if ch == "\n":
                out.append("\n")
                self.i += 1
                continue
            self._reject_control(ch)
            out.append(ch)
            self.i += 1

    def _parse_multiline_literal_string(self):
        start = self.i
        self.i += 3  # "'''"
        if self.i < self.n and self.s[self.i] == "\n":
            self.i += 1
        out = []
        while True:
            if self.i >= self.n:
                self._fail("多行字面字符串未闭合", start)
            if self.s.startswith("'''", self.i):
                count = 0
                while self.i + count < self.n and self.s[self.i + count] == "'":
                    count += 1
                if count > 5:
                    self._fail("多行字面字符串结尾引号过多", start)
                out.append("'" * (count - 3))
                self.i += count
                return "".join(out)
            ch = self.s[self.i]
            if ch == "\n":
                out.append("\n")
                self.i += 1
                continue
            self._reject_control(ch)
            out.append(ch)
            self.i += 1


def parse_toml(text, source="<toml>"):
    """按 TOML 语法解析为字典；无法安全解释的构造抛 CheckError。"""
    return _TomlParser(text, source).parse()


def _flatten_tables(node, prefix, out):
    for key, value in node.items():
        if isinstance(value, dict):
            path = prefix + (key,)
            out[".".join(path)] = value
            _flatten_tables(value, path, out)


def tables(path):
    """读取 TOML 文件，返回全部表（点分路径 -> 表字典）。"""
    path = Path(path)
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise CheckError("无法读取 TOML %s: %s" % (path, error))
    result = {}
    _flatten_tables(parse_toml(text, str(path)), (), result)
    return result


def _strict_equal(left, right):
    """区分 bool/int 的严格相等，避免 True == 1 混过 lint 值校验。"""
    if isinstance(left, bool) or isinstance(right, bool):
        return isinstance(left, bool) and isinstance(right, bool) and left == right
    if isinstance(left, dict) and isinstance(right, dict):
        if set(left) != set(right):
            return False
        return all(_strict_equal(left[key], right[key]) for key in left)
    if isinstance(left, list) and isinstance(right, list):
        return len(left) == len(right) and all(_strict_equal(a, b) for a, b in zip(left, right))
    if type(left) is not type(right):
        return False
    return left == right


def audit_manifests(root=ROOT):
    manifest_tables = tables(root / "Cargo.toml")
    rust_lints = manifest_tables.get("workspace.lints.rust")
    clippy_lints = manifest_tables.get("workspace.lints.clippy")
    if not _strict_equal(rust_lints, EXPECTED_RUST):
        raise CheckError("根 workspace rust lint 与登记策略不符: " + repr(rust_lints))
    if not _strict_equal(clippy_lints, EXPECTED_CLIPPY):
        raise CheckError("根 workspace clippy lint 与登记策略不符: " + repr(clippy_lints))
    checked = {}
    for name, relative in MEMBERS.items():
        manifest = root / relative / "Cargo.toml"
        audit_member_manifest(name, manifest)
        checked[name] = str(manifest.relative_to(root))
    return checked


def audit_member_manifest(name, manifest):
    parsed = tables(manifest)
    lints = parsed.get("lints")
    # 恰好是 {"workspace": true}：子表 [lints.*]、数组表 [[lints]]、其他键都在这里被拒。
    if not isinstance(lints, dict) or set(lints) != {"workspace"} or lints["workspace"] is not True:
        raise CheckError("成员未完整继承 workspace lint: " + name)


def record(argv, cwd, output, label, env_overrides):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update(env_overrides)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    try:
        result = subprocess.run(argv, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        code, stdout, stderr = result.returncode, result.stdout, result.stderr
    except OSError as error:
        code, stdout, stderr = 127, "", str(error)
    (output / (label + ".stdout.log")).write_text(stdout)
    (output / (label + ".stderr.log")).write_text(stderr)
    metadata = {
        "argv": argv,
        "cwd": str(cwd),
        "started_utc": started,
        "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "exit_code": code,
        "env_overrides": env_overrides,
    }
    (output / (label + ".command.json")).write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
    return code, stdout + stderr, metadata


def check_expected(label, argv, cwd, output, env, expected_code, diagnostic=""):
    code, text, _ = record(argv, cwd, output, label, env)
    needle = diagnostic.replace("_", "-") if diagnostic.startswith("clippy::") else diagnostic
    passed = code == expected_code and needle in text
    print("%s: %s (exit=%d; 预期=%d; 诊断=%s)" % (
        label, "符合预期" if passed else "不符合预期", code, expected_code, diagnostic or "无"), flush=True)
    if not passed:
        raise CheckError("夹具结果不符；查看 " + str(output))


def cargo_toml(root_lints, package_name, edition="2024", inherit=True, workspace_members=None):
    rust_lines = "\n".join('%s = "%s"' % item for item in sorted(root_lints["rust"].items()))
    clippy_lines = "\n".join('%s = "%s"' % item for item in sorted(root_lints["clippy"].items()))
    inheritance = "\n[lints]\nworkspace = true\n" if inherit else ""
    return (
        '[workspace]\nresolver = "3"\nmembers = %s\n'
        '[workspace.package]\nedition = "%s"\n'
        '[workspace.lints.rust]\n%s\n'
        '[workspace.lints.clippy]\n%s\n'
        '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "%s"\n'
        % (json.dumps(workspace_members or [package_name]), edition, rust_lines, clippy_lines, package_name, edition)
    ) + inheritance


def write_fixture(project, package_name, source, root_lints, target=None, inherit=True, edition="2024"):
    member = Path(project) / package_name
    (member / "src").mkdir(parents=True, exist_ok=True)
    workspace_text = cargo_toml(root_lints, package_name, inherit=inherit)
    (Path(project) / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    manifest = member / "Cargo.toml"
    manifest.write_text('[package]\nname = "%s"\nversion = "0.0.0"\nedition = "%s"\n' % (package_name, edition))
    if inherit:
        manifest.write_text(manifest.read_text() + "\n[lints]\nworkspace = true\n")
    if target is None or target == "lib":
        manifest.write_text(manifest.read_text() + "\n[lib]\npath = \"src/lib.rs\"\n")
        (member / "src/lib.rs").write_text(source)
    elif target == "test":
        manifest.write_text(manifest.read_text() + "\n[lib]\npath = \"src/lib.rs\"\n")
        (member / "src/lib.rs").write_text("//! 隔离 lint 样例库。\n")
        (member / "tests").mkdir(exist_ok=True)
        (member / "tests/fixture.rs").write_text(source)
    elif target == "example":
        manifest.write_text(manifest.read_text() + "\n[[example]]\nname = \"fixture\"\npath = \"examples/fixture.rs\"\n")
        (member / "examples").mkdir(exist_ok=True)
        (member / "examples/fixture.rs").write_text(source)
    elif target == "bench":
        manifest.write_text(manifest.read_text() + "\n[[bench]]\nname = \"fixture\"\nharness = false\npath = \"benches/fixture.rs\"\n")
        (member / "benches").mkdir(exist_ok=True)
        (member / "benches/fixture.rs").write_text(source)
    return Path(project) / "Cargo.toml", member


def write_member_project(project, root_lints, decoy):
    """按真实仓库的八个成员布局生成隔离 workspace；aline-buffer 用伪/真继承清单。"""
    project = Path(project)
    project.mkdir(parents=True, exist_ok=True)
    workspace_text = cargo_toml(root_lints, "aline-buffer", workspace_members=list(MEMBERS.values()))
    (project / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    lock = [
        "# This file is automatically @generated by Cargo.",
        "# It is not intended for manual editing.",
        "version = 4",
        "",
    ]
    for name, relative in MEMBERS.items():
        member = project / relative
        (member / "src").mkdir(parents=True, exist_ok=True)
        manifest = member / "Cargo.toml"
        if name == "aline-buffer":
            manifest.write_text(DECOY_MEMBER_TEXT if decoy else REAL_MEMBER_TEXT)
        else:
            manifest.write_text(
                '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n\n[lints]\nworkspace = true\n' % name)
        (member / "src/lib.rs").write_text("//! 继承解析隔离夹具。\npub fn undocumented() {}\n")
        lock.extend(['[[package]]', 'name = "%s"' % name, 'version = "0.0.0"', ""])
    (project / "Cargo.lock").write_text("\n".join(lock))
    return project


def check_inheritance_parsing(cases_dir, output, env, root_lints):
    """F-08 回归：伪继承必须被守卫拒绝，真实继承必须通过，并用 Cargo 实际诊断对照。"""
    results = output / "results"
    decoy_project = write_member_project(cases_dir / "f08-decoy-inheritance", root_lints, decoy=True)
    real_project = write_member_project(cases_dir / "f08-real-inheritance", root_lints, decoy=False)
    script = str(Path(__file__).resolve())
    check_expected("f08-decoy-guard", [sys.executable, script, "--root", str(decoy_project)],
                   decoy_project, results, env, 1, "成员未完整继承 workspace lint: aline-buffer")
    check_expected("f08-real-guard", [sys.executable, script, "--root", str(real_project)],
                   real_project, results, env, 0, "workspace lint 声明有效")
    clippy = ["cargo", "clippy", "--manifest-path", str(decoy_project / "Cargo.toml"),
              "-p", "aline-buffer", "--locked", "--offline", "--", "-D", "warnings"]
    check_expected("f08-decoy-cargo-control", clippy, decoy_project, results, env, 0)
    clippy = ["cargo", "clippy", "--manifest-path", str(real_project / "Cargo.toml"),
              "-p", "aline-buffer", "--locked", "--offline", "--", "-D", "warnings"]
    check_expected("f08-real-cargo-control", clippy, real_project, results, env, 101, "missing documentation")
    return 4


PARSER_CASES = [
    ("accept-decoy-description-multiline", DECOY_MEMBER_TEXT, "ok", ""),
    ("accept-real-inheritance", REAL_MEMBER_TEXT, "ok", ""),
    ("accept-multiline-literal-decoy",
     "[package]\ndescription = '''\n[lints]\nworkspace = true\n'''\n", "ok", ""),
    ("accept-basic-escapes",
     '[package]\ndescription = "quote \\" and tab \\t and \\u0041"\n', "ok", ""),
    ("accept-comments-and-multiline-array",
     '[package]\nkeywords = [\n  "a", # 行内注释\n  "b",\n]\n', "ok", ""),
    ("accept-inline-table",
     '[package.metadata.x]\nvalue = { a = 1, b = "two", c = true }\n', "ok", ""),
    ("accept-quoted-and-dotted-keys",
     '[package]\n"a.b" = 1\nmeta.x = { y = 2 }\n', "ok", ""),
    ("accept-array-of-tables",
     '[[bin]]\nname = "one"\n[[bin]]\nname = "two"\n', "ok", ""),
    ("accept-numbers-and-datetime",
     '[package]\nchecksum = 0xff\nratio = 1.5\ndate = 1979-05-27T07:32:00Z\n', "ok", ""),
    ("accept-super-table-after-subtable",
     '[a.b]\nx = 1\n[a]\ny = 2\n', "ok", ""),
    ("reject-inline-table-extended-by-header",
     'a = {x = 1}\n[a]\ny = 2\n', "error", "内联表"),
    ("reject-array-table-then-normal-table",
     '[[a]]\nx = 1\n[a]\n', "error", "数组表"),
    ("reject-dotted-key-then-header",
     'a.b = 1\n[a]\nc = 2\n', "error", "点分键已定义表"),
    ("reject-newline-in-basic-string", '[package]\nname = "abc\n', "error", "跨行"),
    ("reject-unterminated-string", '[package]\nname = "abc', "error", "未闭合"),
    ("reject-unknown-escape", '[package]\nname = "a\\q"\n', "error", "未知转义"),
    ("reject-duplicate-key", '[package]\nname = "a"\nname = "b"\n', "error", "重复 TOML 键"),
    ("reject-duplicate-table", '[package]\n[package]\n', "error", "重复 TOML 表"),
    ("reject-inline-table-trailing-comma",
     '[package.metadata]\nx = { a = 1, }\n', "error", "尾随逗号"),
    ("reject-leading-zero-integer", '[package]\ncount = 01\n', "error", "无法识别"),
    ("reject-bare-cr", '[package]\rname = "a"\n', "error", "裸 CR"),
    ("reject-bom", "\ufeff[package]\n", "error", "BOM"),
    ("reject-bad-header", '[package\nname = "a"\n', "error", "表头"),
]


def run_parser_self_checks(directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    for index, (label, text, expect, needle) in enumerate(PARSER_CASES, 1):
        (directory / ("%02d-%s.toml" % (index, label))).write_text(text)
        try:
            parse_toml(text, label)
        except CheckError as error:
            if expect != "error" or needle not in str(error):
                raise CheckError("解析器自检不符合预期: %s (%s)" % (label, error))
            print("parser %s: 拒绝符合预期 (%s)" % (label, error), flush=True)
        else:
            if expect != "ok":
                raise CheckError("解析器自检应拒绝但通过了: " + label)
            print("parser %s: 解析符合预期" % label, flush=True)
    decoy = parse_toml(DECOY_MEMBER_TEXT, "decoy")
    if "lints" in decoy:
        raise CheckError("多行字符串内的伪 [lints] 表被当成真实表")
    description = decoy.get("package", {}).get("description", "")
    if "[lints]" not in description:
        raise CheckError("多行字符串内容未完整保留，无法证明伪继承被解析为字符串")
    print("parser 语义: 伪 [lints] 保留在 description 中，未生成 lints 表", flush=True)
    return len(PARSER_CASES)


def run_self_test(output):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    actual_members = audit_manifests()
    print("八个正式成员均声明 workspace lint 继承", flush=True)
    root_tables = tables(ROOT / "Cargo.toml")
    root_lints = {"rust": root_tables["workspace.lints.rust"], "clippy": root_tables["workspace.lints.clippy"]}
    cargo_home = os.environ.get("CARGO_HOME")
    if not cargo_home:
        isolation = json.loads((ROOT / "docs/validation/m0-engine-foundation/admission-2026-10-30/isolation.json").read_text())
        cargo_home = isolation["cargo_home"]
    target_dir = str(output / "cargo-target")
    env = {"CARGO_HOME": cargo_home, "CARGO_TARGET_DIR": target_dir, "CARGO_NET_OFFLINE": "true", "RUSTUP_TOOLCHAIN": "1.98.1"}

    version_dir = output / "versions"
    for label, argv in (
        ("cargo-version", ["cargo", "-V"]),
        ("rustc-version", ["rustc", "-Vv"]),
        ("clippy-version", ["cargo", "clippy", "--version"]),
    ):
        code, text, _ = record(argv, ROOT, version_dir, label, env)
        if code != 0:
            raise CheckError("工具版本查询失败: " + label)
        print("%s: %s" % (label, text.splitlines()[0] if text else "(无输出)"), flush=True)

    cases_dir = output / "fixtures"
    cases_dir.mkdir(exist_ok=True)
    inherit_project = cases_dir / "member-inheritance"
    rust = "//! 隔离 lint 继承样例。\n#[allow(dead_code)]\nunsafe fn forbidden() {}\n"
    manifests = []
    inherit_project.mkdir(exist_ok=True)
    workspace_text = cargo_toml(root_lints, "fixture-root", workspace_members=list(MEMBERS))
    (inherit_project / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    for name in MEMBERS:
        member = inherit_project / name
        (member / "src").mkdir(parents=True, exist_ok=True)
        manifest = member / "Cargo.toml"
        manifest.write_text('[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n\n[lints]\nworkspace = true\n' % name)
        (member / "src/lib.rs").write_text(rust)
        manifests.append((name, manifest))
    for name, _ in manifests:
        argv = ["cargo", "check", "--manifest-path", str(inherit_project / "Cargo.toml"), "--package", name, "--offline"]
        check_expected("inherit-unsafe-" + name, argv, inherit_project, output / "results", env, 101, "unsafe-code")

    # 每个目标单独建最小 workspace，使错误的目标源码只参与本次诊断。
    samples = [
        ("positive-assert-const", "//! 允许预期测试断言和编译期只读常量的正例。\n/// 编译期只读实体名。\npub const ENTITY_NAMES: [&str; 1] = [\"amp\"];\n#[cfg(test)] mod tests { #[test] fn assertion_is_test_expectation() { let left = 1; let right = 1; assert_eq!(left, right); } }\n", "lib", 0, ""),
        ("negative-unwrap-lib", "//! 隔离违规样例。\n/// 故意触发 unwrap lint。\npub fn forbidden(value: Option<u8>) -> u8 { value.unwrap() }\n", "lib", 101, "clippy::unwrap_used"),
        ("negative-unwrap-test", "//! 隔离违规样例。\n#[test] fn forbidden() { let value = Some(1_u8); let _copy = value.unwrap(); }\n", "test", 101, "clippy::unwrap_used"),
        ("negative-unwrap-example", "//! 隔离违规样例。\nfn main() { let value = Some(1_u8); let _copy = value.unwrap(); }\n", "example", 101, "clippy::unwrap_used"),
        ("negative-unwrap-bench", "//! 隔离违规样例。\nfn main() { let value = Some(1_u8); let _copy = value.unwrap(); }\n", "bench", 101, "clippy::unwrap_used"),
        ("negative-expect", "//! 隔离违规样例。\n/// 故意触发 expect lint。\npub fn forbidden(value: Option<u8>) -> u8 { value.expect(\"fixture\") }\n", "lib", 101, "clippy::expect_used"),
        ("negative-panic", "//! 隔离违规样例。\n/// 故意触发 panic lint。\npub fn forbidden() { panic!(\"fixture\") }\n", "lib", 101, "clippy::panic"),
        ("negative-unreachable", "//! 隔离违规样例。\n/// 故意触发 unreachable lint。\npub fn forbidden() -> u8 { unreachable!() }\n", "lib", 101, "clippy::unreachable"),
        ("negative-coordinate-cast-truncation", "//! 隔离源码/绘制坐标窄化样例。\n/// 故意把绘制字节偏移窄化。\npub fn invalid_drawn_offset(drawn_off: u64) -> u16 { drawn_off as u16 }\n", "lib", 101, "clippy::cast_possible_truncation"),
        ("negative-cast-sign-loss", "//! 隔离违规样例。\n/// 故意触发符号损失 lint。\npub fn forbidden(value: i32) -> usize { value as usize }\n", "lib", 101, "clippy::cast_sign_loss"),
    ]
    profiles = {"debug": [], "release": ["--release"]}
    for label, source, target, expected, diagnostic in samples:
        project = cases_dir / label
        manifest, _ = write_fixture(project, "lint-fixture", source, root_lints, target)
        for profile, profile_args in profiles.items():
            argv = ["cargo", "clippy", "--manifest-path", str(manifest), "--offline", "--all-targets"] + profile_args + ["--", "-D", "warnings"]
            check_expected(label + "-" + profile, argv, project, output / "results", env, expected, diagnostic)

    unsafe_source = "//! 隔离违规样例。\n#![allow(unsafe_code)]\n/// 故意尝试降低 forbid。\npub unsafe fn forbidden() {}\n"
    unsafe_project = cases_dir / "negative-unsafe-allow"
    unsafe_manifest, _ = write_fixture(unsafe_project, "lint-fixture", unsafe_source, root_lints)
    check_expected("unsafe-forbid-cannot-be-lowered", ["cargo", "check", "--manifest-path", str(unsafe_manifest), "--offline"], unsafe_project, output / "results", env, 101, "forbid")

    missing_project = cases_dir / "negative-member-inheritance"
    missing_manifest, _ = write_fixture(missing_project, "aline-source", "//! 漏继承夹具。\npub fn ordinary() {}\n", root_lints, inherit=False)
    try:
        audit_member_manifest("aline-source", missing_manifest)
        raise CheckError("成员未继承 workspace lint 时守卫未拒绝")
    except CheckError as error:
        if "未完整继承" not in str(error):
            raise
    # 错误配置的 Cargo 负例明确编译无 lint 源码；该结果不冒充 lint 诊断。
    code, text, _ = record(["cargo", "check", "--manifest-path", str(missing_manifest), "--offline"], missing_project, output / "results", "missing-inherit-cargo-control", env)
    if code != 0:
        raise CheckError("漏继承控制样例的普通源码没有通过 Cargo，无法隔离继承检查")

    complete_negative = cases_dir / "negative-one-member-inheritance"
    complete_negative.mkdir(exist_ok=True)
    workspace_text = cargo_toml(root_lints, "fixture-root", workspace_members=list(MEMBERS.values()))
    (complete_negative / "Cargo.toml").write_text(workspace_text.split("\n[package]\n", 1)[0] + "\n")
    for name, relative in MEMBERS.items():
        member = complete_negative / relative
        (member / "src").mkdir(parents=True, exist_ok=True)
        member_manifest = member / "Cargo.toml"
        member_manifest.write_text('[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n' % name)
        if name != "aline-source":
            member_manifest.write_text(member_manifest.read_text() + "\n[lints]\nworkspace = true\n")
        (member / "src/lib.rs").write_text("//! 成员漏继承隔离反例。\n")
    check_expected("negative-one-member-inheritance-guard",
                   ["python3", str(Path(__file__).resolve()), "--root", str(complete_negative)],
                   complete_negative, output / "results", env, 1, "成员未完整继承 workspace lint: aline-source")

    inheritance_cases = check_inheritance_parsing(cases_dir, output, env, root_lints)
    parser_cases = run_parser_self_checks(output / "parser-cases")

    print(json.dumps({
        "status": "lint 隔离样例通过",
        "formal_members": actual_members,
        "self_test_cases": len(samples),
        "profiles": sorted(profiles),
        "inheritance_cases": inheritance_cases,
        "parser_cases": parser_cases,
    }, ensure_ascii=False))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path, default=ROOT, help="待审 Cargo workspace 根目录")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "docs/validation/m0-engine-foundation/group-2/lint-self-test")
    args = parser.parse_args()
    try:
        if args.self_test:
            return run_self_test(args.output_dir)
        members = audit_manifests(args.root.resolve())
        print(json.dumps({"status": "workspace lint 声明有效", "members": members}, ensure_ascii=False, indent=2))
        return 0
    except (CheckError, OSError, ValueError, KeyError) as error:
        print("lint 检查拒绝: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
