#!/usr/bin/env python3
"""核对 UI 字面翻译 key 和直接输出的字面文案。

复用既有 Rust 词法/测试作用域与 TOML 解析器；动态内容、宏展开、别名和
字符串变量的数据流仍需审查，不把本检查称为全部用户可见字符串的证明。
"""

import argparse
import io
from pathlib import Path
import re
import sys
import unittest

from check_lints import CheckError, parse_toml
from evidence_paths import write_json
from safety_lex import test_tokens, tokenize
from validation_output import ROOT, output_directory

KEY = re.compile(r'[a-z][a-z_0-9]*(?:\.[a-z_0-9]+)+')
OUTPUT_METHODS = {'child', 'tooltip', 'label', 'title', 'prompt', 'aria_label',
                  'set_window_title'}


def literal(value):
    """只解释无需转义的普通/原始字符串；不能安全解释时返回 None。"""
    if value.startswith('"') and value.endswith('"'):
        body = value[1:-1]
        return body if '\\' not in body else None
    raw = re.fullmatch(r'r(\#*)"(.*)"\1', value, re.S)
    return raw[2] if raw else None


def scan(source, keys, path):
    try:
        tokens = tokenize(source, preserve_strings=True)
        testing, _ = test_tokens(tokens)
    except ValueError as error:
        return [{'file': str(path), 'line': 1, 'rule': 'lexical-failure', 'detail': str(error)}]
    failures = []
    for index, token in enumerate(tokens):
        if index in testing:
            continue
        text = literal(token.value)
        string_token = token.value.startswith('"') or re.match(r'r\#*"', token.value)
        if not string_token:
            continue
        previous = [item.value for item in tokens[max(0, index - 3):index]]
        method = previous[-2] if len(previous) >= 2 and previous[-1] == '(' else None
        if method in OUTPUT_METHODS and len(previous) == 3 and previous[0] == '.':
            failures.append({'file': str(path), 'line': token.line,
                             'rule': 'direct-ui-literal', 'detail': method})
        # 点分字符串也可能是文件名；只有翻译调用首参才按 catalog key 核对。
        # 通过变量/数组间接传入的 key 仍属于文档列明的人工数据流复核边界。
        if method in ('t', 't_with', 'text'):
            if text is None or not KEY.fullmatch(text):
                failures.append({'file': str(path), 'line': token.line,
                                 'rule': 'non-key-translation-literal'})
            elif text not in keys:
                failures.append({'file': str(path), 'line': token.line,
                                 'rule': 'missing-catalog-key', 'detail': text})
    return failures


def catalog_keys(path):
    def leaves(node, prefix=''):
        result = set()
        for key, value in node.items():
            name = prefix + key
            if isinstance(value, dict):
                result.update(leaves(value, name + '.'))
            elif isinstance(value, str):
                result.add(name)
            else:
                raise ValueError('catalog 非字符串叶值：' + name)
        return result
    return leaves(parse_toml(path.read_text(encoding='utf-8'), str(path)))


def source_failures(roots, keys):
    files = sorted(path for root in roots for path in root.rglob('*.rs'))
    tests, production = set(), set()
    failures = []
    for path in files:
        try:
            tokens = tokenize(path.read_text(encoding='utf-8'))
            _, external = test_tokens(tokens)
            explicit = {index for name, index in external if not name.startswith('@nested:')}
            home = path.parent if path.name in ('lib.rs', 'main.rs', 'mod.rs') else path.parent / path.stem
            for index in range(len(tokens) - 2):
                if tokens[index].value == 'mod' and tokens[index + 2].value == ';':
                    name = tokens[index + 1].value
                    existing = [item for item in (home / (name + '.rs'), home / name / 'mod.rs') if item.is_file()]
                    if len(existing) != 1:
                        failures.append({'file': str(path), 'line': tokens[index].line,
                                         'rule': 'unresolved-module-scope'})
                    else:
                        (tests if index in explicit else production).add(existing[0])
        except (OSError, ValueError) as error:
            failures.append({'file': str(path), 'line': 1, 'rule': 'source-read-or-lexical-failure',
                             'detail': str(error)})
    for path in files:
        if path in tests and path not in production:
            continue
        try:
            failures.extend(scan(path.read_text(encoding='utf-8'), keys, path))
        except OSError as error:
            failures.append({'file': str(path), 'line': 1, 'rule': 'source-read-failure', 'detail': str(error)})
    return failures


class CatalogTests(unittest.TestCase):
    def test_translated_content_and_valid_keys_are_allowed(self):
        self.assertEqual(scan('div().child(copy.open); translator.t("action.open"); '
                              'let key = r#"action.open"#;', {'action.open'}, 'ui.rs'), [])

    def test_direct_ui_literals_and_missing_keys_are_rejected(self):
        for source in ('div().child("打开");', 'button.tooltip("Open");',
                       'div().aria_label("打开文件");',
                       'window.set_window_title(r#"Aline"#);',
                       r'div().child("Open\nfile");',
                       'translator.t("missing.key");', 'text("action.opne");',
                       'translator.t_with("action.opne", &[]);',
                       'translator.t("打开");'):
            with self.subTest(source=source):
                self.assertTrue(scan(source, {'action.open'}, 'ui.rs'))

    def test_comments_tests_and_string_contents_do_not_fake_calls(self):
        self.assertEqual(scan('// .child("打开")\nlet sample = r#".child("打开")"#; '
                              '#[cfg(test)] mod checks { let sample = "missing.key"; }',
                              {'action.open'}, 'ui.rs'), [])

    def test_accessible_names_and_window_titles_accept_translated_values(self):
        self.assertEqual(scan('div().aria_label(copy.open); '
                              'window.set_window_title(copy.title.as_ref()); '
                              'window.set_window_title(translator.t("window.empty")?);',
                              {'window.empty'}, 'ui.rs'), [])

    def test_dotted_filenames_are_not_translation_calls(self):
        source = ('new_file(&output, "opened.bin", bytes)?; '
                  'new_file(&output, "stored.bin", bytes)?; '
                  'new_file(&output, "state.tsv", bytes)?; '
                  'let unrelated = "action.opne"; path.join("action.opne");')
        self.assertEqual(scan(source, {'action.open'}, 'arbitrary.rs'), [])
        for source in ('translator.t("opened.bin");', 'text("state.tsv");',
                       'translator.t("action.opne");'):
            with self.subTest(source=source):
                failures = scan(source, {'action.open'}, 'arbitrary.rs')
                self.assertEqual([failure['rule'] for failure in failures],
                                 ['missing-catalog-key'])

    def test_broken_source_and_non_test_production_are_rejected(self):
        self.assertTrue(scan('/* broken', set(), 'ui.rs'))
        self.assertTrue(scan('#[cfg(test)] mod checks { } fn production() { div().child("Open"); }',
                             set(), 'ui.rs'))

    def test_external_test_module_requires_exact_test_scope(self):
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'lib.rs').write_text('#[cfg(test)] mod checks;', encoding='utf-8')
            (root / 'checks.rs').write_text('fn check() { div().child("Open"); }', encoding='utf-8')
            self.assertEqual(source_failures([root], set()), [])
            (root / 'lib.rs').write_text('mod checks;', encoding='utf-8')
            self.assertTrue(source_failures([root], set()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    try:
        output = output_directory('ui-catalog', args.output_dir)
        if args.self_test:
            stream = io.StringIO()
            result = unittest.TextTestRunner(stream=stream).run(
                unittest.defaultTestLoader.loadTestsFromTestCase(CatalogTests))
            write_json(output / 'summary.json', {'passed': result.wasSuccessful(),
                       'tests': result.testsRun, 'output': stream.getvalue()})
            return 0 if result.wasSuccessful() else 1
        chinese = catalog_keys(ROOT / 'crates/aline-app-core/locales/zh-CN.toml')
        english = catalog_keys(ROOT / 'crates/aline-app-core/locales/en.toml')
        failures = [] if chinese == english else [{'rule': 'catalog-key-set-mismatch'}]
        failures.extend(source_failures([ROOT / 'crates/aline-ui/src', ROOT / 'app/src'], chinese & english))
        write_json(output / 'summary.json', {'passed': not failures, 'failures': failures,
                   'limits': '仅检查直接UI字面值和t/t_with/text调用的字面首参；不展开宏、不追踪变量/数组/调用别名的数据流，间接key须人工复核'})
        print('ui-catalog: ' + ('通过' if not failures else '未通过'))
        return 1 if failures else 0
    except (CheckError, OSError, ValueError) as error:
        print('ui-catalog: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
