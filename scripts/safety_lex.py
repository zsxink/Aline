"""安全守卫的 Rust 词法与测试作用域；不是完整 Rust 解析器或宏展开器。

仅向 check_safety 提供不含字符串/注释的 token 与行号。不能判断调用目标、
类型内部可变性、过程宏输出或分配/索引的隐式 panic；这些边界由 Rust 检查及评审承担。
"""

from dataclasses import dataclass
import re


@dataclass
class Token:
    """保留原始行号供最小诊断定位。"""

    value: str
    line: int


def tokenize(source, preserve_strings=False):
    """跳过注释/字符，默认跳过字符串；可保留完整字符串 token 供 UI 检查。

    保留模式不解码 Rust 字符串，调用方按自身受检边界解释；字符串里的括号
    不会参与作用域计算。默认行为保持不变，不把生命周期当字符字面量。
    """
    result = []
    cursor, line = 0, 1
    while cursor < len(source):
        start, start_line = cursor, line
        if source.startswith('//', cursor):
            end = source.find('\n', cursor)
            cursor = len(source) if end < 0 else end
        elif source.startswith('/*', cursor):
            cursor += 2
            depth = 1
            while cursor < len(source) and depth:
                if source.startswith('/*', cursor):
                    depth += 1
                    cursor += 2
                elif source.startswith('*/', cursor):
                    depth -= 1
                    cursor += 2
                else:
                    cursor += 1
            if depth:
                raise ValueError('未闭合块注释，行 %s' % start_line)
        else:
            raw = re.match(r'(?:br|cr|r)(#*)"', source[cursor:])
            if raw:
                terminator = '"' + raw[1]
                end = source.find(terminator, cursor + raw.end())
                if end < 0:
                    raise ValueError('未闭合原始字符串，行 %s' % start_line)
                cursor = end + len(terminator)
                if preserve_strings:
                    result.append(Token(source[start:cursor], start_line))
            elif source[cursor] == '"' or source.startswith(('b"', 'c"'), cursor):
                cursor += 1 if source[cursor] == '"' else 2
                closed = False
                while cursor < len(source):
                    if source[cursor] == '\\':
                        cursor += 2
                    elif source[cursor] == '"':
                        cursor += 1
                        closed = True
                        break
                    else:
                        cursor += 1
                if not closed:
                    raise ValueError('未闭合字符串，行 %s' % start_line)
                if preserve_strings:
                    result.append(Token(source[start:cursor], start_line))
            else:
                character = re.match(r"(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'", source[cursor:])
                identifier = re.match(r'(?:r#)?[A-Za-z_][A-Za-z_0-9]*', source[cursor:])
                if character:
                    cursor += character.end()
                elif identifier:
                    result.append(Token(identifier[0].removeprefix('r#'), line))
                    cursor += identifier.end()
                elif source.startswith('::', cursor):
                    result.append(Token('::', line))
                    cursor += 2
                else:
                    if not source[cursor].isspace():
                        result.append(Token(source[cursor], line))
                    cursor += 1
        line += source[start:cursor].count('\n')
    return result


def closing(tokens, start):
    """匹配分隔符，失败拒绝扫描而不是当作安全。"""
    pairs = {'[': ']', '(': ')', '{': '}'}
    stack = []
    for index in range(start, len(tokens)):
        value = tokens[index].value
        if value in pairs:
            stack.append(pairs[value])
        elif value in pairs.values():
            if not stack or stack.pop() != value:
                raise ValueError('分隔符不匹配，行 %s' % tokens[index].line)
            if not stack:
                return index
    raise ValueError('未闭合分隔符，行 %s' % tokens[start].line)


def test_tokens(tokens):
    """只认可精确 cfg(test) 或 test 函数；复杂 cfg 保守按生产扫描。

    返回内联测试体 token 集及外置 cfg(test) 模块名。#[test] 只放行函数体，
    不放行后续生产项；不凭模块名 tests 猜作用域。
    """
    marked, external = set(), []
    depths, depth = [], 0
    for token in tokens:
        depths.append(depth)
        depth += (token.value == '{') - (token.value == '}')
    index = 0
    while index < len(tokens):
        if tokens[index].value != '#' or index + 1 >= len(tokens) or tokens[index + 1].value != '[':
            index += 1
            continue
        end = closing(tokens, index + 1)
        attribute = [token.value for token in tokens[index + 2:end]]
        is_cfg = attribute == ['cfg', '(', 'test', ')']
        is_test = attribute == ['test']
        following = end + 1
        while following + 1 < len(tokens) and tokens[following].value == '#' and tokens[following + 1].value == '[':
            following = closing(tokens, following + 1) + 1
        if is_cfg or is_test:
            body = following
            while body < len(tokens) and tokens[body].value not in ('{', ';'):
                body += 1
            header = [token.value for token in tokens[following:body]]
            if body < len(tokens) and tokens[body].value == '{' and (is_cfg or 'fn' in header):
                marked.update(range(body, closing(tokens, body) + 1))
            elif is_cfg and len(header) >= 2 and header[-2] == 'mod':
                # 未实现内联模块名字空间解析时拒绝，不能错误授权同名顶层文件。
                external.append((header[-1] if depths[index] == 0 else '@nested:' + header[-1], body - 2))
        index = end + 1
    return marked, external
