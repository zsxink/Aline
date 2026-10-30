"""定向复核独立发现的外置模块作用域漏洞；夹具不进入正式成员源码。"""
import datetime
import hashlib
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[4]
GROUP = Path(__file__).parent
fixtures = GROUP / 'safety-review-fixtures'
cases = [
    ('path-shared-production', 'crates/aline-source', 'aline-source', '#[cfg(test)] mod tests;\n#[path="tests.rs"] mod production;\npub fn run() { production::check(); }', {'tests.rs': 'pub fn check() { assert!(false); }'}, 'unproven-module-path'),
    ('nested-sibling-name', 'crates/aline-source', 'aline-source', 'mod tests;\n#[cfg(test)] mod outer { #[cfg(test)] mod tests; }\npub fn run() { tests::check(); }', {'tests.rs': 'pub fn check() { assert!(false); }', 'outer/tests.rs': '#[test] fn allowed() { assert!(true); }'}, 'engine-production-assert'),
    ('dual-cfg-production', 'crates/aline-source', 'aline-source', '#[cfg(test)] mod shared;\n#[cfg(not(test))] mod shared;\npub fn run() { shared::check(); }', {'shared.rs': 'pub fn check() { assert!(false); }'}, 'engine-production-assert'),
    ('renamed-engine-directory', 'crates/source', 'aline-source', 'pub fn run() { assert!(false); }', {}, 'unrecognized-member-path'),
    ('quoted-name-and-description', 'crates/aline-ui', 'aline-source', 'pub fn runtime() { let _state = std::sync::OnceLock::<u8>::new(); }', {}, 'member-package-identity'),
    ('engine-in-known-ui-directory', 'crates/aline-ui', 'aline-source', 'pub fn run() { assert!(false); }', {}, 'member-package-identity'),
]
results = []
for label, location, name, source, children, expected in cases:
    project = fixtures / label
    crate = project / location
    (crate / 'src').mkdir(parents=True, exist_ok=True)
    (crate / 'Cargo.toml').write_text('[package]\nname = "' + name + '"\nversion = "0.0.0"\nedition = "2024"\n[workspace]\n')
    if label == 'quoted-name-and-description':
        (crate / 'Cargo.toml').write_text('[package]\n"name" = "aline-source"\nversion = "0.0.0"\nedition = "2024"\ndescription = """\nname = "aline-ui"\n"""\n[workspace]\n')
    (crate / 'src/lib.rs').write_text(source + '\n')
    for child, text in children.items():
        path = crate / 'src' / child
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text + '\n')
    argv = [sys.executable, str(ROOT / 'scripts/check_safety.py'), '--root', str(project)]
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    outcome = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True)
    (project / 'stdout.json').write_text(outcome.stdout)
    (project / 'stderr.log').write_text(outcome.stderr)
    data = json.loads(outcome.stdout)
    rules = sorted({row['rule'] for row in data['failures']})
    results.append({'id': label, 'argv': argv, 'cwd': str(ROOT), 'started_utc': started, 'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'exit_code': outcome.returncode, 'expected_rule': expected, 'actual_rules': rules, 'passed': outcome.returncode == 1 and expected in rules})
print(json.dumps({'scope': '定向静态守卫反例；不声称完整Rust或平台运行验证', 'results': results, 'reviewed_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [ROOT/'scripts/check_safety.py', ROOT/'scripts/safety_lex.py']}}, ensure_ascii=False, indent=2))
if not all(row['passed'] for row in results):
    sys.exit(1)
