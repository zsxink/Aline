#!/usr/bin/env python3
"""检查 UI 生产源码的字面颜色集中于私有 token 模块。

复用现有 Rust 词法/测试作用域检查；不是类型检查器，不展开过程宏或解析变量值。
字符串中的颜色配置不在此守卫覆盖内；正式主题包校验仍归 M4。
"""

import argparse
from pathlib import Path
import re
import sys
import unittest

from evidence_paths import write_json
from safety_lex import test_tokens, tokenize
from validation_output import ROOT, output_directory

TOKEN_FILE = Path('native_window/tokens.rs')


def scan(source, path, testing=False):
    """返回行号与规则；词法损坏失败关闭，不把扫描失败当无违规。"""
    try:
        tokens = tokenize(source)
        marked, _ = test_tokens(tokens)
    except ValueError as error:
        return [{'file': str(path), 'line': 1, 'rule': 'lexical-failure',
                 'detail': str(error)}]
    failures = []
    for index, token in enumerate(tokens):
        if testing or index in marked:
            continue
        following = tokens[index + 1].value if index + 1 < len(tokens) else ''
        # 既有词法器把 0xFFFFFF 切成数字 0 与标识符 xFFFFFF；支持下划线与整型后缀。
        hexadecimal = token.value == '0' and re.fullmatch(
            r'x(?:[0-9a-fA-F]_*){6,8}(?:u(?:8|16|32|64|128|size)|i(?:8|16|32|64|128|size))?',
            following)
        direct_constructor = (
            token.value in ('rgb', 'rgba', 'hsla')
            and following == '(' and index + 2 < len(tokens)
            and tokens[index + 2].value in tuple('0123456789')
        )
        if hexadecimal or direct_constructor:
            failures.append({'file': str(path), 'line': token.line,
                             'rule': 'ui-color-outside-token-module'})
    return failures


def source_failures(source_root):
    """精确 cfg(test) 外置模块才豁免；生产者同时引用时仍按生产源码检查。"""
    files = sorted(source_root.rglob('*.rs'))
    production, tests = set(), set()
    failures = []
    for path in files:
        try:
            tokens = tokenize(path.read_text(encoding='utf-8'))
            _, external = test_tokens(tokens)
            explicit = {index: name for name, index in external}
            home = path.parent if path.name in ('lib.rs', 'main.rs', 'mod.rs') else path.parent / path.stem
            for index, token in enumerate(tokens[:-2]):
                if token.value != 'mod' or tokens[index + 2].value != ';':
                    continue
                name = tokens[index + 1].value
                candidates = [home / (name + '.rs'), home / name / 'mod.rs']
                existing = [item for item in candidates if item.is_file()]
                if len(existing) != 1:
                    failures.append({'file': str(path), 'line': token.line,
                                     'rule': 'unresolved-module-scope'})
                elif index in explicit and not explicit[index].startswith('@nested:'):
                    tests.add(existing[0])
                else:
                    production.add(existing[0])
        except (OSError, ValueError) as error:
            failures.append({'file': str(path), 'line': 1,
                             'rule': 'source-read-or-lexical-failure', 'detail': str(error)})
    for path in files:
        if path.relative_to(source_root) == TOKEN_FILE:
            continue
        try:
            failures.extend(scan(path.read_text(encoding='utf-8'), path,
                                 path in tests and path not in production))
        except OSError as error:
            failures.append({'file': str(path), 'line': 1,
                             'rule': 'source-read-failure', 'detail': str(error)})
    return failures


class TokenTests(unittest.TestCase):
    def test_semantic_reference_is_allowed(self):
        self.assertEqual(scan('rgb(tokens::TEXT); rgba(tokens::HOVER);', 'component.rs'), [])

    def test_hex_literal_and_direct_constructor_are_rejected(self):
        for code in ('rgb(0xFFFFFF)', 'let color = 0x09_69_DAu32;',
                     'rgba(0x00000000)', 'rgb(16777215)', 'hsla(0.0, 0.0, 0.0, 1.0)'):
            with self.subTest(code=code):
                self.assertTrue(scan(code, 'component.rs'))

    def test_strings_comments_and_exact_tests_are_allowed(self):
        self.assertEqual(scan('''// 0xFFFFFF
/* rgb(0xFFFFFF) /* 0xFFFFFF */ */
let text = r#"rgb(0xFFFFFF)"#;
#[cfg(test)] mod checks { let color = 0xFFFFFF; }
#[test] fn expected_value() { rgb(0xFFFFFF); }
''', 'component.rs'), [])

    def test_test_scope_does_not_authorize_following_production(self):
        self.assertTrue(scan('#[cfg(test)] mod checks { rgb(0xFFFFFF); }\n'
                             'fn production() { rgb(0xFFFFFF); }', 'component.rs'))
        self.assertTrue(scan('#[cfg(any(test, debug_assertions))] mod checks '
                             '{ rgb(0xFFFFFF); }', 'component.rs'))

    def test_broken_source_is_rejected(self):
        self.assertTrue(scan('/* 0xFFFFFF', 'component.rs'))

    def test_external_test_module_requires_proven_scope(self):
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'lib.rs').write_text('#[cfg(test)] mod checks;', encoding='utf-8')
            (root / 'checks.rs').write_text('fn check() { rgb(0xFFFFFF); }', encoding='utf-8')
            self.assertEqual(source_failures(root), [])
            (root / 'lib.rs').write_text('mod checks;', encoding='utf-8')
            self.assertTrue(source_failures(root))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    try:
        output = output_directory('ui-tokens', args.output_dir)
    except ValueError as error:
        print(str(error), file=sys.stderr)
        return 1
    if args.self_test:
        import io
        stream = io.StringIO()
        result = unittest.TextTestRunner(stream=stream).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(TokenTests))
        write_json(output / 'self-test.json', {'passed': result.wasSuccessful(),
                   'tests': result.testsRun, 'output': stream.getvalue()})
        print('ui-tokens self-test: ' + ('通过' if result.wasSuccessful() else '未通过'))
        return 0 if result.wasSuccessful() else 1
    failures = source_failures(ROOT / 'crates/aline-ui/src')
    write_json(output / 'summary.json', {'passed': not failures, 'failures': failures,
               'scope': 'crates/aline-ui/src',
               'limits': '不展开宏、不解析变量值或字符串配置；真实视觉另行截图验证'})
    print('ui-tokens: ' + ('通过' if not failures else '未通过'))
    return 1 if failures else 0


if __name__ == '__main__':
    raise SystemExit(main())
