#!/usr/bin/env python3
"""自有 Rust 源码安全补充检查；Clippy、坏输入测试和人工调用路径审查不可省略。

五引擎包括测试/示例/基准禁 unwrap，生产断言/显式 panic 出口按测试作用域区分。
只检查直接 token；别名跨模块、过程宏、隐式索引/容量失败及第三方行为须人工审查。
不读取或改动 Cargo 配置，不承诺 Rust 全语义证明，不自动批准 lint 例外。
扫描根、固定八成员与 Cargo 实际 target/build script 入口都受检；空扫描或清单读取失败
一律 fail-closed，不把「零命中」或任意非零退出当成通过。
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from safety_lex import tokenize, test_tokens, closing
from evidence_paths import write_json, write_text

ROOT = Path(__file__).resolve().parent.parent
ENGINES = {'aline-source', 'aline-buffer', 'aline-markdown', 'aline-edit', 'aline-app-core'}
# 固定八成员：相对根路径 → Cargo 真实包名。目录缺失、清单缺失、身份不符都不静默跳过。
CANONICAL_MEMBERS = {
    'app': 'aline',
    'crates/aline-app-core': 'aline-app-core',
    'crates/aline-source': 'aline-source',
    'crates/aline-buffer': 'aline-buffer',
    'crates/aline-markdown': 'aline-markdown',
    'crates/aline-edit': 'aline-edit',
    'crates/aline-plugin': 'aline-plugin',
    'crates/aline-ui': 'aline-ui',
}
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


def cargo_read(argv, cwd, member_reads, label):
    """只读 Cargo 调用；完整 argv/cwd/时间/退出码/原始输出进入 member_reads。"""
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    result = subprocess.run(argv, cwd=str(cwd), text=True, capture_output=True)
    member_reads.append({'label': label, 'argv': argv, 'cwd': str(cwd), 'started_utc': started,
                         'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                         'exit_code': result.returncode, 'stdout': result.stdout,
                         'stderr': result.stderr})
    return result


def inside(path, directory):
    """真实路径比较；目录外 target/build script 不因相对写法或符号链接漏判。"""
    real_path = os.path.realpath(path)
    real_directory = os.path.realpath(directory)
    return real_path == real_directory or real_path.startswith(real_directory + os.sep)


def member_scope(root, metadata):
    """固定八成员与 Cargo 真实包身份；缺失、未声明、越界都给出具体规则。"""
    failures, valid, canonical = [], [], set()
    packages = {}
    for package in metadata.get('packages') or []:
        manifest_path = package.get('manifest_path')
        if manifest_path:
            packages[os.path.realpath(manifest_path)] = package
    for relative, expected in CANONICAL_MEMBERS.items():
        directory = root / relative
        manifest = directory / 'Cargo.toml'
        if not directory.is_dir() or not manifest.is_file():
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'missing-member',
                             'detail': relative})
            continue
        canonical.add(os.path.realpath(manifest))
        package = packages.get(os.path.realpath(manifest))
        if package is None:
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'member-not-in-workspace',
                             'detail': relative})
            continue
        if package.get('name') != expected:
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'member-package-identity',
                             'detail': 'expected %s, got %s' % (expected, package.get('name'))})
            continue
        valid.append((relative, package))
    reported = {os.path.realpath(item['file']) for item in failures}
    for key, package in packages.items():
        if key not in canonical:
            manifest = str(package.get('manifest_path', key))
            if os.path.realpath(manifest) not in reported:
                failures.append({'file': manifest, 'line': 1,
                                 'rule': 'unrecognized-member-path', 'detail': str(package.get('name'))})
                reported.add(os.path.realpath(manifest))
    crates = root / 'crates'
    if crates.is_dir():
        for extra in sorted(crates.glob('*')):
            manifest = extra / 'Cargo.toml'
            relative = str(extra.relative_to(root))
            if (extra.is_dir() and manifest.is_file() and relative not in CANONICAL_MEMBERS
                    and os.path.realpath(manifest) not in reported):
                failures.append({'file': str(manifest), 'line': 1, 'rule': 'unrecognized-member-path',
                                 'detail': relative})
                reported.add(os.path.realpath(manifest))
    return failures, valid


def member_scan(root, relative, package, scanned):
    """按 Cargo 实际 targets 校验入口，再按真实声明传播测试作用域并扫描成员源码。"""
    failures = []
    directory = root / relative
    manifest = directory / 'Cargo.toml'
    engine = directory.name in ENGINES
    target_files, rejected_external = set(), set()
    for target in package.get('targets') or []:
        kinds = target.get('kind') or []
        source = target.get('src_path')
        if not source:
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'target-path-missing',
                             'detail': ','.join(kinds)})
            continue
        real_source = Path(os.path.realpath(source))
        # Cargo 合法但落在成员目录外的 target/build script 一律 fail-closed；
        # 拒绝理由与原始 src_path 写入 detail，完整 argv/stdout 见 member_reads。
        if not inside(real_source, directory):
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'external-target-path',
                             'detail': '%s %s' % (','.join(kinds), source)})
            rejected_external.add(real_source)
            continue
        if not real_source.is_file():
            # 声明路径不存在或为悬空符号链接：Cargo 接受清单，rustc 编译时才失败。
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'target-path-missing',
                             'detail': '%s %s' % (','.join(kinds), source)})
            rejected_external.add(real_source)
            continue
        target_files.add(real_source)
    files = set(target_files)

    def collect(start):
        """显式下降成员目录；符号链接按真实落点判定，逃逸成员外一律拒绝。

        `Path.rglob` 在 Python 3.9 不下降目录符号链接，会让 `mod linkdir;` 加载的成员外
        源码静默漏扫（rustc 却会编译它），故不依赖 rglob。留在成员内的符号链接按真实路径
        计入，与普通文件同权；指向成员外的文件或目录都给出 external-target-path。
        """
        visited = {Path(os.path.realpath(start))}
        pending = [start]
        while pending:
            current = pending.pop()
            for name in sorted(os.listdir(str(current))):
                candidate = current / name
                real_path = Path(os.path.realpath(candidate))
                if not inside(real_path, directory):
                    if real_path not in rejected_external:
                        failures.append({'file': str(candidate), 'line': 1, 'rule': 'external-target-path',
                                         'detail': 'symlink escapes member: %s' % real_path})
                    continue
                if candidate.is_dir():
                    if real_path not in visited:
                        visited.add(real_path)
                        pending.append(real_path)
                    continue
                if name.endswith('.rs'):
                    if real_path in rejected_external:
                        # target 级检查已给出具体规则，不重复报告同一落点。
                        continue
                    if not real_path.is_file():
                        # 悬空符号链接：Cargo 接受该声明，rustc 编译时才失败，这里明确拒绝。
                        failures.append({'file': str(candidate), 'line': 1, 'rule': 'target-path-missing',
                                         'detail': 'dangling symlink: %s' % candidate})
                        continue
                    files.add(real_path)

    collect(directory)
    files = sorted(files)
    test_files = {path for path in files if path.relative_to(directory).parts[0] == 'tests'}
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
        relative_path = path.relative_to(directory)
        is_crate_root = path.name in ('lib.rs', 'main.rs', 'mod.rs') or (
            len(relative_path.parts) == 2 and relative_path.parts[0] == 'tests')
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
    test_files |= test_targets - production_targets
    for path in files:
        kind = ('engine' if engine else 'ui') + ('-test' if path in test_files else '-production')
        result, _ = scan(path.read_text(), path, kind)
        failures.extend(result)
        scanned.append({'file': str(path.relative_to(root)), 'kind': kind})
    return failures


def workspace_scan(root):
    """根、固定八成员与 Cargo 实际 target/build script 入口全部受检。

    根不存在、清单不是 workspace、成员缺失/未声明/身份不符、目录外 target、空扫描
    一律给出具体规则并非零退出；不把零命中、Cargo 失败或任意非零当成通过。
    """
    root = Path(os.path.realpath(root))
    failures, scanned, member_reads = [], [], []
    manifest = root / 'Cargo.toml'
    attempted = False
    if not root.is_dir():
        failures.append({'file': str(root), 'line': 1, 'rule': 'root-missing',
                         'detail': 'root directory does not exist'})
    elif not manifest.is_file():
        failures.append({'file': str(manifest), 'line': 1, 'rule': 'root-not-workspace',
                         'detail': 'root has no Cargo.toml'})
    else:
        # Cargo 自己解析 workspace 成员、包名与实际 target；只读，不联网。
        argv = ['cargo', 'metadata', '--no-deps', '--format-version', '1',
                '--manifest-path', str(manifest), '--locked', '--offline']
        read = cargo_read(argv, root, member_reads, 'workspace-metadata')
        metadata = None
        if read.returncode != 0:
            failures.append({'file': str(manifest), 'line': 1, 'rule': 'workspace-metadata-failure',
                             'detail': read.stderr.strip()[:2000]})
        else:
            try:
                metadata = json.loads(read.stdout)
            except ValueError as error:
                failures.append({'file': str(manifest), 'line': 1, 'rule': 'workspace-metadata-failure',
                                 'detail': 'invalid metadata JSON: %s' % error})
        if metadata is not None:
            root_is_package = any(
                os.path.realpath(package.get('manifest_path') or '') == str(manifest)
                for package in metadata.get('packages') or [])
            if os.path.realpath(metadata.get('workspace_root') or '') != str(root) or root_is_package:
                failures.append({'file': str(manifest), 'line': 1, 'rule': 'root-not-workspace',
                                 'detail': 'workspace_root=%s root_is_package=%s'
                                           % (metadata.get('workspace_root'), root_is_package)})
            else:
                attempted = True
                scope_failures, valid = member_scope(root, metadata)
                failures.extend(scope_failures)
                for relative, package in valid:
                    failures.extend(member_scan(root, relative, package, scanned))
    if attempted and not scanned:
        failures.append({'file': str(root), 'line': 1, 'rule': 'empty-scan',
                         'detail': 'no source file was scanned'})
    unique = {}
    for item in failures:
        unique.setdefault((item['file'], item['line'], item['rule'], item.get('detail')), item)
    return {'failures': list(unique.values()), 'scanned': scanned, 'member_reads': member_reads}


def build_workspace(base, omit=(), undeclared=(), extra_members=(), overrides=None):
    """在受控临时目录构造固定八成员夹具；省略/未声明/增补成员只用于反例。"""
    overrides = overrides or {}
    members = [path for path in CANONICAL_MEMBERS
               if path not in omit and path not in undeclared] + list(extra_members)
    base.mkdir(parents=True, exist_ok=True)
    (base / 'Cargo.toml').write_text('[workspace]\nresolver = "3"\nmembers = [%s]\n'
                                     % ', '.join('"%s"' % member for member in members))
    for relative, name in CANONICAL_MEMBERS.items():
        if relative in omit:
            continue
        directory = base / relative
        (directory / 'src').mkdir(parents=True, exist_ok=True)
        (directory / 'Cargo.toml').write_text(
            '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n' % name)
        root_file = 'main.rs' if relative == 'app' else 'lib.rs'
        (directory / 'src' / root_file).write_text(
            'fn main() {}\n' if relative == 'app' else 'pub fn root() {}\n')
    for relative, text in overrides.items():
        path = base / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def self_test(output):
    """独立契约期望预先存于 cases.json；逐例通过 CLI 验证退出码及诊断。

    期望先于实现手写；夹具只写受控临时目录，不写入仓库受跟踪路径。
    """
    fixtures = ROOT / 'docs/validation/m0-engine-foundation/group-2/safety-fixtures'
    cases = json.loads((fixtures / 'cases.json').read_text())
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='aline-safety-self-test-'))
    records = []

    def run_case(name, argv, expected, expectation, count=None):
        start = datetime.datetime.now(datetime.timezone.utc).isoformat()
        result = subprocess.run(argv, cwd=str(ROOT), text=True, capture_output=True,
                                env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'))
        write_text(output / (name + '.stdout.json'), result.stdout)
        write_text(output / (name + '.stderr.log'), result.stderr)
        actual = json.loads(result.stdout)
        rules = sorted({item['rule'] for item in actual['failures']})
        failures = len(actual['failures'])
        # 规则集合相等不足以发现「同一落点被报告两次」这类回归，可选断言失败条数。
        matches = rules == sorted(expected) and (count is None or failures == count)
        records.append({'name': name, 'argv': argv, 'cwd': str(ROOT), 'started_utc': start,
                        'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        'exit_code': result.returncode, 'expected_rules': expected, 'actual_rules': rules,
                        'expected_failures': count, 'actual_failures': failures,
                        'passed': matches and result.returncode == (1 if expected else 0),
                        'independent_expectation': expectation})

    for case in cases:
        fixture = work / (case['name'] + '.rs')
        fixture.write_text(case['source'] + '\n')
        run_case(case['name'],
                 [sys.executable, str(Path(__file__).resolve()), '--file', str(fixture), '--kind', case['kind']],
                 case['expected'], 'Rust S-7/10/13、tasks 2.3；预先手写 cases.json')
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
        ('wrong-package-in-known-ui-path', 'fn f() { assert!(false); }', '', ['member-package-identity', 'missing-member']),
        ('quoted-package-and-description-decoy', 'fn f() {}', '', ['member-package-identity', 'missing-member']),
    ]
    for name, parent, child, expected in module_cases:
        fake_root = work / 'workspace-fixtures' / name
        overrides = {}
        extra, omit = (), ()
        if name not in ('wrong-package-in-known-ui-path', 'quoted-package-and-description-decoy'):
            overrides = {'crates/aline-buffer/src/lib.rs': parent + '\n',
                         'crates/aline-buffer/src/checks.rs': child + '\n'}
        if name == 'unrecognized-member-path':
            extra = ('crates/source',)
            overrides['crates/source/Cargo.toml'] = '[package]\nname = "aline-rogue"\nversion = "0.0.0"\n'
            overrides['crates/source/src/lib.rs'] = parent + '\n'
        if name == 'wrong-package-in-known-ui-path':
            # 真实 aline-source 包放进已知 UI 目录：工作区里 aline-source 只能有一份，
            # 故省略规范成员目录，并期望 member-package-identity + missing-member。
            omit = ('crates/aline-source',)
            overrides['crates/aline-ui/Cargo.toml'] = '[package]\nname = "aline-source"\nversion = "0.0.0"\n'
            overrides['crates/aline-ui/src/lib.rs'] = parent + '\n'
        if name == 'quoted-package-and-description-decoy':
            omit = ('crates/aline-source',)
            overrides['crates/aline-ui/Cargo.toml'] = ('[package]\n"name" = "aline-source"\nversion = "0.0.0"\n'
                                                       'edition = "2024"\ndescription = """\nname = "aline-ui"\n"""\n')
            overrides['crates/aline-ui/src/lib.rs'] = parent + '\n'
        if name == 'binary-and-lib-shared':
            overrides['crates/aline-buffer/src/main.rs'] = 'mod checks; fn main() { checks::check(); }\n'
        build_workspace(fake_root, omit=omit, extra_members=extra, overrides=overrides)
        run_case(name, [sys.executable, str(Path(__file__).resolve()), '--root', str(fake_root)], expected,
                 '精确 cfg(test) 声明才授予测试断言范围；文件名不构成证据；成员集合与包身份受检')
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
        fake_root = work / 'workspace-fixtures' / name
        overrides = {'crates/aline-buffer/' + relative: text for relative, text in contents.items()}
        build_workspace(fake_root, overrides=overrides)
        run_case(name, [sys.executable, str(Path(__file__).resolve()), '--root', str(fake_root)], expected,
                 'Cargo 集成测试根：tests/<name>.rs 的 mod 相对 tests/ 解析；缺失模块必须拒绝')
    # R06 F-09/F-15 反例：目录外真实 target/build script、根缺失、根不是 workspace、
    # 成员未声明、零文件扫描。期望先于实现手写。
    scope_cases = [
        ('external-lib-target-path', {'crates/aline-buffer/Cargo.toml':
            '[package]\nname = "aline-buffer"\nversion = "0.0.0"\nedition = "2024"\n\n[lib]\npath = "../../outside.rs"\n',
            'outside.rs': '#[allow(clippy::unwrap_used)]\npub fn bad(v: Option<u8>) -> u8 { v.unwrap() }\n'},
         {}, ['external-target-path']),
        ('external-build-script-path', {'crates/aline-buffer/Cargo.toml':
            '[package]\nname = "aline-buffer"\nversion = "0.0.0"\nedition = "2024"\nbuild = "../../outside-build.rs"\n',
            'outside-build.rs': '#[allow(clippy::unwrap_used)]\npub fn bad(v: Option<u8>) -> u8 { v.unwrap() }\n'},
         {}, ['external-target-path']),
        ('member-not-in-workspace', {}, ('crates/aline-edit',), ['member-not-in-workspace']),
        # 复核 R06 D2：target 声明指向不存在的路径。Cargo metadata 接受该清单（实测 exit 0），
        # rustc 编译时才失败，故守卫必须显式拒绝，不能因「路径不存在」静默少扫一个入口。
        ('missing-target-path', {'crates/aline-buffer/Cargo.toml':
            '[package]\nname = "aline-buffer"\nversion = "0.0.0"\nedition = "2024"\n\n[lib]\npath = "src/absent.rs"\n'},
         {}, ['target-path-missing']),
    ]
    for name, overrides, undeclared, expected in scope_cases:
        fake_root = work / 'workspace-fixtures' / name
        # 目录与清单都真实存在，只是没写进 workspace.members：Cargo 不认它为成员。
        build_workspace(fake_root, undeclared=undeclared, overrides=overrides)
        run_case(name, [sys.executable, str(Path(__file__).resolve()), '--root', str(fake_root)], expected,
                 'Cargo 实际 target 入口受审：目录外 target/build script 明确拒绝；未声明成员不静默跳过')
    # 复核 R06 D1/D2：符号链接落点。成员内链接逃逸到成员外时 rustc 仍会编译目标，
    # 必须拒绝；链接留在成员内时按真实路径计入扫描，不得漏扫也不得误拒。
    symlink_cases = [
        ('symlink-directory-escapes-member', 'dir', ('external-target-path',), 'directory link'),
        ('symlink-file-escapes-member', 'file', ('external-target-path',), 'file link'),
        ('symlink-inside-member-is-scanned', 'inside', (), 'internal link'),
    ]
    for name, shape, expected, label in symlink_cases:
        fake_root = work / 'workspace-fixtures' / name
        build_workspace(fake_root)
        outside_dir = work / 'symlink-outside' / name
        outside_dir.mkdir(parents=True, exist_ok=True)
        target = outside_dir / ('mod.rs' if shape == 'dir' else 'checks.rs')
        target.write_text('#[allow(clippy::unwrap_used)]\npub fn bad(v: Option<u8>) -> u8 { v.unwrap() }\n')
        link = fake_root / 'crates/aline-buffer/src' / ('linkdir' if shape == 'dir' else 'linked.rs')
        if shape == 'inside':
            target.write_text('pub fn ok() {}\n')
            target = fake_root / 'crates/aline-buffer/src/real_checks.rs'
            target.write_text('pub fn ok() {}\n')
            link.symlink_to(target)
        else:
            link.symlink_to(outside_dir if shape == 'dir' else target)
        run_case(name, [sys.executable, str(Path(__file__).resolve()), '--root', str(fake_root)], list(expected),
                 '符号链接按真实落点判定：逃逸成员外必须拒绝（%s）；留在成员内必须照常受扫' % label)
    # 复核 R06：悬空符号链接作 target 时，target 声明与目录枚举会看到同一落点；
    # 规则集合相同但重复诊断会误导修复方向，故此处额外断言失败条数为 1。
    dangling_root = work / 'workspace-fixtures' / 'dangling-target-symlink'
    build_workspace(dangling_root)
    dangling = dangling_root / 'crates/aline-buffer/src/dangling.rs'
    dangling.symlink_to(dangling_root / 'crates/aline-buffer/src/missing.rs')
    (dangling_root / 'crates/aline-buffer/Cargo.toml').write_text(
        '[package]\nname = "aline-buffer"\nversion = "0.0.0"\nedition = "2024"\n\n[lib]\npath = "src/dangling.rs"\n')
    run_case('dangling-target-symlink', [sys.executable, str(Path(__file__).resolve()), '--root', str(dangling_root)],
             ['target-path-missing'], '悬空符号链接必须拒绝，且同一落点只报一条', count=1)
    run_case('root-missing', [sys.executable, str(Path(__file__).resolve()), '--root',
                              str(work / 'workspace-fixtures' / 'absent-root')], ['root-missing'],
             '不存在的根必须非零退出并给出 root-missing；空扫描不等于通过')
    member_root = work / 'workspace-fixtures' / 'member-manifest-as-root'
    build_workspace(member_root)
    run_case('root-is-member-manifest', [sys.executable, str(Path(__file__).resolve()), '--root',
                                         str(member_root / 'crates/aline-buffer')], ['root-not-workspace'],
             '根必须指向 workspace 根而非成员清单；成员清单当根必须拒绝')
    no_manifest = work / 'workspace-fixtures' / 'root-without-manifest'
    no_manifest.mkdir(parents=True, exist_ok=True)
    run_case('root-not-workspace', [sys.executable, str(Path(__file__).resolve()), '--root', str(no_manifest)],
             ['root-not-workspace'], '根目录没有 Cargo.toml 必须拒绝，不能当作空扫描通过')
    empty_workspace = work / 'workspace-fixtures' / 'empty-workspace'
    empty_workspace.mkdir(parents=True, exist_ok=True)
    (empty_workspace / 'Cargo.toml').write_text('[workspace]\nresolver = "3"\nmembers = []\n')
    run_case('empty-scan', [sys.executable, str(Path(__file__).resolve()), '--root', str(empty_workspace)],
             ['empty-scan', 'missing-member'], '零成员/零文件扫描必须失败；缺成员逐条给出 missing-member')
    restored = workspace_scan(ROOT)
    summary = {'cases': records, 'workspace_after_fixtures': restored, 'fixture_root': str(work),
               'passed': all(row['passed'] for row in records) and not restored['failures']}
    write_json(output / 'summary.json', summary)
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
