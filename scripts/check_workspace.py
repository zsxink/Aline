#!/usr/bin/env python3
"""顺序运行 M0 本地及 CI 门禁，分别保留结果，不安装工具或下载依赖。

调用方提供预热的隔离 Cargo 缓存；本机可复用已登记的候选缓存。
每条检查独立执行，即使前一条失败也不伪造后续结果或整体通过。
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
TOOLCHAIN = '1.98.1'


def invoke(argv, output, label, env):
    """记录实际命令和输出；失败仍完整留证。"""
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    try:
        result = subprocess.run(argv, cwd=ROOT, env=env, text=True, capture_output=True)
        code, stdout, stderr = result.returncode, result.stdout, result.stderr
    except OSError as error:
        code, stdout, stderr = 127, '', str(error)
    (output / (label + '.stdout.log')).write_text(stdout)
    (output / (label + '.stderr.log')).write_text(stderr)
    row = {'argv': argv, 'cwd': str(ROOT), 'started_utc': started,
           'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'exit_code': code, 'toolchain_bin': env.get('ALINE_M0_TOOLCHAIN_BIN'),
           'env_overrides': {key: env[key] for key in
               ('CARGO_HOME', 'CARGO_NET_OFFLINE', 'RUSTUP_TOOLCHAIN', 'PYTHONDONTWRITEBYTECODE') if key in env}}
    (output / (label + '.command.json')).write_text(json.dumps(row, ensure_ascii=False, indent=2) + '\n')
    print(label + ': exit=' + str(code), flush=True)
    return row, stdout


def run(args):
    output = args.output_dir.resolve() if args.output_dir else Path(tempfile.mkdtemp(prefix='aline-m0-gates-'))
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_NET_OFFLINE='true', RUSTUP_TOOLCHAIN=TOOLCHAIN,
               PYTHONDONTWRITEBYTECODE='1')
    home = args.cargo_home or os.environ.get('CARGO_HOME')
    if not home and not os.environ.get('CI'):
        isolation = ROOT / 'docs/validation/m0-engine-foundation/admission-2026-10-30/isolation.json'
        if isolation.is_file():
            home = json.loads(isolation.read_text()).get('cargo_home')
    if not home or not Path(home).is_dir():
        raise ValueError('必须提供已有的隔离 Cargo 缓存；不自动下载或使用未登记的全局配置')
    env['CARGO_HOME'] = str(Path(home).resolve())
    for name in ('RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS'):
        if os.environ.get(name):
            raise ValueError('存在未登记的编译覆盖变量：' + name)
    records = []
    row, installed = invoke(['rustup', 'toolchain', 'list'], output, 'installed-toolchains', env)
    records.append(row)
    if row['exit_code'] != 0 or not any(line.split()[0].startswith(TOOLCHAIN + '-') for line in installed.splitlines() if line.split()):
        raise ValueError('登记工具链未安装；本入口不执行安装')
    for tool in ('cargo', 'rustfmt', 'cargo-clippy'):
        row, resolved = invoke(['rustup', 'which', '--toolchain', TOOLCHAIN, tool], output, 'installed-' + tool, env)
        records.append(row)
        if row['exit_code'] != 0:
            raise ValueError('登记工具组件不可用：' + tool)
        if tool == 'cargo':
            toolchain_bin = str(Path(resolved.strip()).parent)
            env['PATH'] = toolchain_bin + os.pathsep + env.get('PATH', '')
            env['ALINE_M0_TOOLCHAIN_BIN'] = toolchain_bin
    checks = [
        ('rustc-version', ['rustc', '-Vv']),
        ('cargo-version', ['cargo', '-V']),
        ('fmt-version', ['cargo', 'fmt', '--version']),
        ('clippy-version', ['cargo', 'clippy', '--version']),
        ('fmt-effective-config', ['rustfmt', '--config-path', str(ROOT / 'rustfmt.toml'), '--print-config', 'current', str(ROOT / 'app/src/main.rs')]),
        ('fmt', ['cargo', 'fmt', '--all', '--check']),
        ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings']),
        ('test', ['cargo', 'test', '--workspace']),
        ('dependency', [sys.executable, str(ROOT / 'scripts/check_engineering.py'), '--output-dir', str(output / 'dependency-graphs')]),
        ('dependency-self-test', [sys.executable, str(ROOT / 'scripts/check_engineering.py'), '--self-test', '--output-dir', str(output / 'dependency-fixtures')]),
        ('lint', [sys.executable, str(ROOT / 'scripts/check_lints.py')]),
        ('lint-self-test', [sys.executable, str(ROOT / 'scripts/check_lints.py'), '--self-test', '--output-dir', str(output / 'lint-fixtures')]),
        ('safety', [sys.executable, str(ROOT / 'scripts/check_safety.py')]),
        ('safety-self-test', [sys.executable, str(ROOT / 'scripts/check_safety.py'), '--self-test', '--output-dir', str(output / 'safety-fixtures')]),
    ]
    for label, argv in checks:
        row, _ = invoke(argv, output, label, env)
        records.append(row)
    passed = all(row['exit_code'] == 0 for row in records)
    summary = {'passed': passed, 'scope': '当前 macOS 主机；目标图解析不等于跨平台构建；CI 是否执行另行记录',
               'catalog': 'cargo test --workspace 实际发现 app-core 正反例与 UI 消费测试',
               'results': records}
    (output / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
    print('证据：' + str(output), flush=True)
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--cargo-home', type=Path)
    args = parser.parse_args()
    try:
        return run(args)
    except (OSError, ValueError, KeyError) as error:
        print('检查入口拒绝：' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
