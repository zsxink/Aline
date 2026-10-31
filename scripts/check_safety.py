#!/usr/bin/env python3
"""自有 Rust 源码安全补充检查；Clippy、坏输入测试和人工调用路径审查不可省略。

五引擎包括测试/示例/基准禁 unwrap，生产断言/显式 panic 出口按测试作用域区分。
只检查直接 token；别名跨模块、过程宏、隐式索引/容量失败及第三方行为须人工审查。
不读取或改动 Cargo 配置，不承诺 Rust 全语义证明，不自动批准 lint 例外。
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys

from safety_lex import tokenize, test_tokens, closing

ROOT = Path(__file__).resolve().parent.parent
ENGINES = {'aline-source', 'aline-buffer', 'aline-markdown', 'aline-edit', 'aline-app-core'}
RUNTIME = {'OnceLock', 'LazyLock', 'Mutex', 'RwLock'}
INTERIOR = RUNTIME | {'AtomicBool', 'AtomicUsize', 'AtomicU64', 'AtomicI64', 'AtomicPtr', 'Cell', 'RefCell', 'UnsafeCell'}
PANIC_MACROS = {'panic', 'unreachable', 'assert', 'assert_eq', 'assert_ne', 'todo', 'unimplemented'}


def scan(source, path, kind):
    """返回结构化拒绝；未知/损坏词法也拒绝，不退化成无命中。"""
    failures = []
    def reject(token, rule):
        failures.append({'file': str(path), 'line': token.line, 'rule': rule})
    try:
        tokens = tokenize(source)
        marked, external = test_tokens(tokens)
    except ValueError as error:
        return [{'file': str(path), 'line': 1, 'rule': 'lexical-failure', 'detail': str(error)}], []
    engine = kind.startswith('engine')
    for index, token in enumerate(tokens[:-1]):
        if token.value == '#' and tokens[index + 1].value == '[':
            end = closing(tokens, index + 1)
            values = [item.value for item in tokens[index + 2:end]]
            if any(values[i:i + 2] == ['path', '='] for i in range(len(values) - 1)):
                reject(token, 'unproven-module-path')
    for index, token in enumerate(tokens):
        value = token.value
        previous = tokens[index - 1].value if index else ''
        following = tokens[index + 1].value if index + 1 < len(tokens) else ''
        testing = kind.endswith('-test') or index in marked
        if value == 'unsafe':
            reject(token, 'unsafe-owned')
        if value == 'static' and following == 'mut':
            reject(token, 'mutable-static')
        if engine and value == 'unwrap':
            reject(token, 'engine-unwrap')
        if not testing and value == 'expect' and previous in ('.', '::'):
            reject(token, ('engine' if engine else 'owned') + '-production-expect')
        if not testing and value in PANIC_MACROS and following == '!':
            reject(token, ('engine' if engine else 'owned') + '-production-' + value)
        if not engine:
            continue
        if value in RUNTIME:
            reject(token, 'engine-runtime-state')
        if value == 'thread' and (previous in ('::', '{', ',') or following in ('::', ';', 'as')):
            reject(token, 'engine-thread')
        if value == 'thread_local' and following == '!':
            reject(token, 'engine-thread-local')
        if value == 'static':
            end = index + 1
            while end < len(tokens) and tokens[end].value != ';':
                end += 1
            values = {t.value for t in tokens[index + 1:end]}
            if values & INTERIOR or any(value.startswith('Atomic') for value in values):
                reject(token, 'engine-mutable-static')
    # 同一行的构造器与类型只给一个同规则诊断，减少重复输出。
    unique = {(r['file'], r['line'], r['rule']): r for r in failures}
    return list(unique.values()), external


def workspace_scan(root):
    """src 下的外置 cfg(test) 模块只按真实声明传播，不凭文件名推断。"""
    failures, scanned, member_reads = [], [], []
    directories = [root / 'app'] + sorted((root / 'crates').glob('*'))
    for directory in directories:
        if not (directory / 'Cargo.toml').is_file():
            continue
        if directory.name not in ENGINES | {'app', 'aline-ui', 'aline-plugin'}:
            failures.append({'file': str(directory), 'line': 1, 'rule': 'unrecognized-member-path'})
            continue
        # Cargo 自己解析清单，不能用正则把多行 description 中的假 name 当包身份。
        argv = ['cargo', 'read-manifest', '--manifest-path', str(directory / 'Cargo.toml'), '--offline']
        started = datetime.datetime.now(datetime.timezone.utc).isoformat()
        read = subprocess.run(argv, cwd=root, text=True, capture_output=True)
        member_reads.append({'argv': argv, 'cwd': str(root), 'started_utc': started,
                             'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                             'exit_code': read.returncode, 'stdout': read.stdout, 'stderr': read.stderr})
        package_name = json.loads(read.stdout).get('name') if read.returncode == 0 else None
        expected = 'aline' if directory.name == 'app' else directory.name
        if package_name != expected:
            failures.append({'file': str(directory / 'Cargo.toml'), 'line': 1, 'rule': 'member-package-identity'})
            continue
        engine = directory.name in ENGINES
        files = sorted(directory.rglob('*.rs'))
        test_files = {p for p in files if p.relative_to(directory).parts[0] == 'tests'}
        test_targets, production_targets = set(), set()
        # 汇集每个文件的所有消费者，生产声明优先，不能见一次 cfg(test) 就给整文件豁免。
        for path in files:
            tokens = tokenize(path.read_text())
            _, external = test_tokens(tokens)
            explicit_test = {index: name for name, index in external}
            declarations = []
            depth = 0
            for i, token in enumerate(tokens[:-2]):
                if token.value == 'mod' and tokens[i + 2].value == ';':
                    name = explicit_test.get(i, tokens[i + 1].value)
                    declarations.append((name if depth == 0 else '@nested:' + name, i in explicit_test))
                depth += (token.value == '{') - (token.value == '}')
            # Cargo 把 tests/<name>.rs 当作集成测试 crate 根：其 mod 声明相对 tests/ 解析，
            # 而不是 tests/<name>/。benches/examples 同属此规则，但当前仓库没有这类文件，
            # 其解析失败仍按 test-module-resolution 拒绝（fail-closed），不做推断。
            relative = path.relative_to(directory)
            is_crate_root = path.name in ('lib.rs', 'main.rs', 'mod.rs') or (
                len(relative.parts) == 2 and relative.parts[0] == 'tests')
            home = path.parent if is_crate_root else path.parent / path.stem
            for name, testing in declarations:
                candidates = [home / (name + '.rs'), home / name / 'mod.rs']
                existing = [candidate for candidate in candidates if candidate.is_file()]
                if len(existing) != 1:
                    failures.append({'file': str(path), 'line': 1, 'rule': 'test-module-resolution', 'detail': name})
                elif existing[0] == path:
                    # `mod self;` 解析到声明文件自身：rustc 报 circular modules，按解析失败拒绝。
                    failures.append({'file': str(path), 'line': 1, 'rule': 'test-module-resolution', 'detail': name})
                elif testing:
                    test_targets.add(existing[0])
                else:
                    production_targets.add(existing[0])
        test_files.update(test_targets - production_targets)
        for path in files:
            kind = ('engine' if engine else 'ui') + ('-test' if path in test_files else '-production')
            result, _ = scan(path.read_text(), path, kind)
            failures.extend(result)
            scanned.append({'file': str(path.relative_to(root)), 'kind': kind})
    return {'failures': failures, 'scanned': scanned, 'member_reads': member_reads}


def self_test(output):
    """独立契约期望预先存于 cases.json；逐例通过 CLI 验证退出码及诊断。"""
    fixtures = ROOT / 'docs/validation/m0-engine-foundation/group-2/safety-fixtures'
    cases = json.loads((fixtures / 'cases.json').read_text())
    output.mkdir(parents=True, exist_ok=True)
    records = []
    for case in cases:
        fixture = fixtures / (case['name'] + '.rs')
        fixture.write_text(case['source'] + '\n')
        argv = [sys.executable, str(Path(__file__).resolve()), '--file', str(fixture), '--kind', case['kind']]
        start = datetime.datetime.now(datetime.timezone.utc).isoformat()
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1')
        result = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True, env=env)
        (output / (case['name'] + '.stdout.json')).write_text(result.stdout)
        (output / (case['name'] + '.stderr.log')).write_text(result.stderr)
        actual = json.loads(result.stdout)
        rules = sorted({item['rule'] for item in actual['failures']})
        passed = rules == sorted(case['expected']) and result.returncode == (1 if case['expected'] else 0)
        records.append({'name': case['name'], 'argv': argv, 'cwd': str(ROOT), 'started_utc': start,
                        'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        'exit_code': result.returncode, 'expected_rules': case['expected'], 'actual_rules': rules,
                        'passed': passed, 'independent_expectation': 'Rust S-7/10/13、tasks 2.3；预先手写 cases.json'})
    module_cases = [
        ('external-test-module', '#[cfg(test)] mod checks;', '#[test] fn check() { assert!(true); }', []),
        ('filename-is-not-test-scope', 'mod checks;', 'fn f() { assert!(true); }', ['engine-production-assert']),
        ('nested-module-does-not-authorize-sibling', 'mod checks; #[cfg(test)] mod outer { #[cfg(test)] mod checks; }',
         'pub fn check() { assert!(false); }', ['engine-production-assert', 'test-module-resolution']),
        ('production-consumer-wins', '#[cfg(test)] mod checks; #[cfg(not(test))] mod checks;',
         'pub fn check() { assert!(false); }', ['engine-production-assert']),
        ('binary-and-lib-shared', '#[cfg(test)] mod checks;',
         'pub fn check() { assert!(false); }', ['engine-production-assert']),
        ('unrecognized-member-path', 'fn f() {}', '', ['unrecognized-member-path']),
        ('wrong-package-in-known-ui-path', 'fn f() { assert!(false); }', '', ['member-package-identity']),
        ('quoted-package-and-description-decoy', 'fn f() {}', '', ['member-package-identity']),
    ]
    for name, parent, child, expected in module_cases:
        fake_root = fixtures / name
        path = 'crates/source' if name == 'unrecognized-member-path' else 'crates/aline-ui' if name in ('wrong-package-in-known-ui-path', 'quoted-package-and-description-decoy') else 'crates/aline-buffer'
        crate = fake_root / path
        (crate / 'src').mkdir(parents=True, exist_ok=True)
        package = 'aline-source' if name in ('unrecognized-member-path', 'wrong-package-in-known-ui-path') else 'aline-buffer'
        (crate / 'Cargo.toml').write_text('[package]\nname="' + package + '"\nversion="0.0.0"\n[workspace]\n')
        if name == 'quoted-package-and-description-decoy':
            (crate / 'Cargo.toml').write_text('[package]\n"name"="aline-source"\nversion="0.0.0"\nedition="2024"\ndescription="""\nname = "aline-ui"\n"""\n[workspace]\n')
        (crate / 'src/lib.rs').write_text(parent + '\n')
        (crate / 'src/checks.rs').write_text(child + '\n')
        if name == 'binary-and-lib-shared':
            (crate / 'src/main.rs').write_text('mod checks; fn main() { checks::check(); }\n')
        argv = [sys.executable, str(Path(__file__).resolve()), '--root', str(fake_root)]
        start = datetime.datetime.now(datetime.timezone.utc).isoformat()
        result = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True,
                                env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'))
        (output / (name + '.stdout.json')).write_text(result.stdout)
        (output / (name + '.stderr.log')).write_text(result.stderr)
        actual = json.loads(result.stdout)
        rules = sorted({item['rule'] for item in actual['failures']})
        records.append({'name': name, 'argv': argv, 'cwd': str(ROOT), 'started_utc': start,
                        'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        'exit_code': result.returncode, 'expected_rules': expected, 'actual_rules': rules,
                        'passed': rules == expected and result.returncode == (1 if expected else 0),
                        'independent_expectation': '精确 cfg(test) 声明才授予测试断言范围；文件名不构成证据'})
    # 集成测试 crate 根：Cargo 把 tests/<name>.rs 当作独立 crate 根，其 mod 相对 tests/ 解析。
    # 期望先于实现手写；用例只验证解析位置，不扩大任何测试作用域豁免。
    integration_cases = [
        ('integration-test-crate-root', {
            'tests/check.rs': 'mod common;\n#[test] fn check() { common::helper(); }\n',
            'tests/common/mod.rs': 'pub fn helper() {}\n'}, []),
        ('integration-test-missing-module', {
            'tests/check.rs': 'mod common;\n'}, ['test-module-resolution']),
        ('integration-nested-module-resolution', {
            'tests/check.rs': 'mod common;\n',
            'tests/common/mod.rs': 'mod nested;\n',
            'tests/common/nested.rs': 'pub fn helper() {}\n'}, []),
        ('integration-sibling-file-is-own-root', {
            'tests/check.rs': 'mod sibling;\n',
            'tests/sibling.rs': 'pub fn helper() {}\n'}, []),
        ('integration-root-does-not-see-src-module', {
            'tests/check.rs': 'mod checks;\n',
            'src/checks.rs': 'pub fn helper() {}\n'}, ['test-module-resolution']),
        # 安全评审 F1：自引用解析到声明文件自身，rustc 报 circular modules，必须拒绝。
        ('integration-self-reference-is-rejected', {
            'tests/selfref.rs': 'mod selfref;\n'}, ['test-module-resolution']),
        # 安全评审 F2：只有 tests/ 直接子文件才是 crate 根，嵌套文件仍按自身目录解析。
        ('integration-nested-file-is-not-crate-root', {
            'tests/sub/dir/file.rs': 'mod x;\n',
            'tests/sub/dir/x.rs': 'pub fn helper() {}\n'}, ['test-module-resolution']),
        # 安全评审 F3：旧规则接受的位置 tests/<name>/<module>.rs 必须被拒绝（rustc E0583）。
        ('integration-old-wrong-location-is-rejected', {
            'tests/check.rs': 'mod common;\n',
            'tests/check/common.rs': 'pub fn helper() {}\n'}, ['test-module-resolution']),
    ]
    for name, contents, expected in integration_cases:
        fake_root = fixtures / name
        crate = fake_root / 'crates/aline-buffer'
        (crate / 'src').mkdir(parents=True, exist_ok=True)
        (crate / 'Cargo.toml').write_text('[package]\nname="aline-buffer"\nversion="0.0.0"\n[workspace]\n')
        (crate / 'src/lib.rs').write_text('pub fn root() {}\n')
        for relative, text in contents.items():
            target = crate / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
        argv = [sys.executable, str(Path(__file__).resolve()), '--root', str(fake_root)]
        start = datetime.datetime.now(datetime.timezone.utc).isoformat()
        result = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True,
                                env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'))
        (output / (name + '.stdout.json')).write_text(result.stdout)
        (output / (name + '.stderr.log')).write_text(result.stderr)
        actual = json.loads(result.stdout)
        rules = sorted({item['rule'] for item in actual['failures']})
        records.append({'name': name, 'argv': argv, 'cwd': str(ROOT), 'started_utc': start,
                        'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        'exit_code': result.returncode, 'expected_rules': expected, 'actual_rules': rules,
                        'passed': rules == expected and result.returncode == (1 if expected else 0),
                        'independent_expectation': 'Cargo 集成测试根：tests/<name>.rs 的 mod 相对 tests/ 解析；缺失模块必须拒绝'})
    restored = workspace_scan(ROOT)
    summary = {'cases': records, 'workspace_after_fixtures': restored,
               'passed': all(row['passed'] for row in records) and not restored['failures']}
    (output / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0 if summary['passed'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--file', type=Path)
    parser.add_argument('--kind', choices=['engine-production', 'engine-test', 'ui', 'ui-production', 'ui-test'], default='engine-production')
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'docs/validation/m0-engine-foundation/group-2/safety-results')
    args = parser.parse_args()
    try:
        if args.self_test:
            return self_test(args.output_dir)
        result = {'failures': scan(args.file.read_text(), args.file, args.kind)[0]} if args.file else workspace_scan(args.root.resolve())
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 1 if result['failures'] else 0
    except (OSError, ValueError, KeyError) as error:
        print(json.dumps({'failures': [{'rule': 'input-failure', 'detail': str(error)}]}, ensure_ascii=False))
        return 1


if __name__ == '__main__':
    sys.exit(main())
