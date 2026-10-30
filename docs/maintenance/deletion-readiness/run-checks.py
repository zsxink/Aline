#!/usr/bin/env python3
"""重建本轮机器检查附件；只写本目录输出，不触碰两个原目录。"""
import json
import subprocess
import sys
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]


def main():
    commands = {
        'check-current.json': [sys.executable, str(BASE / 'check.py')],
        'check-A.json': [sys.executable, str(BASE / 'check.py'), 'docs_bak'],
        'check-B.json': [sys.executable, str(BASE / 'check.py'), 'reports'],
        'check-C.json': [sys.executable, str(BASE / 'check.py'), 'docs_bak', 'reports'],
        'prototype-check.json': [sys.executable, str(ROOT / 'docs/prototype/tools/verify-reference.py')],
    }
    summary = []
    for name, command in commands.items():
        completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
        if not completed.stdout:
            raise RuntimeError(completed.stderr)
        parsed = json.loads(completed.stdout)
        # 完整结果先捕获到内存再写，避免检查器读到重定向截断的自身JSON输出。
        (BASE / name).write_text(completed.stdout)
        summary.append({'output': name, 'exit_code': completed.returncode,
                        'errors': len(parsed['errors'])})
        if completed.returncode:
            print(json.dumps(summary, ensure_ascii=False, indent=2))
            return completed.returncode
    (BASE / 'run-summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
