#!/usr/bin/env python3
"""隔离编译 4.3 的正常消费者和最小拒绝反例；不添加依赖。"""
import json
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[4]
CASES = [('api-valid', None), ('unchanged-bool', 'E0061'), ('old-equality', 'E0308')]


def main():
    assert (REPO / 'crates/aline-source/Cargo.toml').is_file(), REPO
    with tempfile.TemporaryDirectory(prefix='aline-source-roundtrip-api-') as temporary:
        workspace = Path(temporary)
        (workspace / 'src').mkdir()
        manifest = '[package]\nname="aline-source-roundtrip-api"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\naline-source={path=' + json.dumps(str(REPO / 'crates/aline-source')) + '}\n'
        (workspace / 'Cargo.toml').write_text(manifest)
        for name, expected_error in CASES:
            (workspace / 'src/main.rs').write_text((ROOT / 'fixtures' / f'{name}.rs').read_text())
            command = ['cargo', 'check', '--offline', '--message-format=json', '--manifest-path', str(workspace / 'Cargo.toml')]
            result = subprocess.run(command, env=os.environ.copy(), text=True, capture_output=True, check=False)
            errors = []
            for line in result.stdout.splitlines():
                message = json.loads(line)
                if message.get('reason') == 'compiler-message' and message['message']['level'] == 'error':
                    error = message['message']
                    errors.append(error.get('code', {}).get('code') if error.get('code') else None)
                    print(error['rendered'])
            print(json.dumps({'case': name, 'command': command, 'exit_code': result.returncode, 'error_codes': errors, 'expected_error': expected_error}, ensure_ascii=False))
            if expected_error is None:
                assert result.returncode == 0 and not errors, result.stderr
            else:
                assert result.returncode != 0 and expected_error in errors, result.stderr
    print('checked=3 public API fixtures (1 positive, 2 negative)')


if __name__ == '__main__':
    main()
