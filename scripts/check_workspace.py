#!/usr/bin/env python3
"""顺序运行 workspace 本地及 CI 门禁，分别保留结果，不安装工具或下载依赖。

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

from evidence_paths import resolve_path, write_json, write_text
from validation_output import output_directory

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
    write_text(output / (label + '.stdout.log'), stdout)
    write_text(output / (label + '.stderr.log'), stderr)
    row = {'argv': argv, 'cwd': str(ROOT), 'started_utc': started,
           'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'exit_code': code, 'toolchain_bin': env.get('ALINE_M0_TOOLCHAIN_BIN'),
           'env_overrides': {key: env[key] for key in
               ('CARGO_HOME', 'CARGO_NET_OFFLINE', 'RUSTUP_TOOLCHAIN', 'PYTHONDONTWRITEBYTECODE') if key in env}}
    write_json(output / (label + '.command.json'), row)
    print(label + ': exit=' + str(code), flush=True)
    return row, stdout


def run_checks(checks, output, env):
    """执行所有检查；单项失败不跳过后续项，整体结论由全部退出码决定。"""
    records = []
    for label, argv in checks:
        row, _ = invoke(argv, output, label, env)
        records.append(row)
    return records


def self_test(output):
    """以真实独立进程证明成功、失败及无法启动均保留证据且不短路。"""
    output = output_directory('workspace-self-test', output)
    records = run_checks([
        ('failure', [sys.executable, '-c', 'raise SystemExit(3)']),
        ('success-after-failure', [sys.executable, '-c', 'print("continued")']),
        ('missing-command', [str(output / 'command-that-does-not-exist')]),
        ('success-after-missing', [sys.executable, '-c', 'print("continued again")']),
    ], output, dict(os.environ))
    expected = [3, 0, 127, 0]
    passed = [row['exit_code'] for row in records] == expected
    passed = passed and all((output / (label + '.command.json')).is_file()
                            for label in ('failure', 'success-after-failure',
                                          'missing-command', 'success-after-missing'))
    write_json(output / 'summary.json', {'passed': passed, 'expected_exit_codes': expected,
                                       'results': records})
    return 0 if passed else 1


def run(args):
    output = output_directory('workspace', args.output_dir)
    env = dict(os.environ, CARGO_NET_OFFLINE='true', RUSTUP_TOOLCHAIN=TOOLCHAIN,
               PYTHONDONTWRITEBYTECODE='1')
    home = args.cargo_home or os.environ.get('CARGO_HOME')
    if not home and not os.environ.get('CI'):
        isolation = ROOT / 'scripts/fixtures/isolation.json'
        if isolation.is_file():
            registered = json.loads(isolation.read_text()).get('cargo_home')
            home = resolve_path(registered) if registered else None
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
        ('output-paths', [sys.executable, str(ROOT / 'scripts/check_validation_output.py'), '--output-dir', str(output / 'output-paths')]),
        ('workspace-self-test', [sys.executable, str(ROOT / 'scripts/check_workspace.py'), '--self-test', '--output-dir', str(output / 'workspace-self-test')]),
        ('m1-summary-self-test', [sys.executable, str(ROOT / 'scripts/run_m1_readonly.py'), '--self-test', '--output-dir', str(output / 'm1-summary-self-test')]),
        ('m1-collector-self-test', [sys.executable, str(ROOT / 'scripts/collect_m1_readonly.py'), '--self-test', '--output-dir', str(output / 'm1-collector-self-test')]),
        ('m1-performance-sampler-self-test', [sys.executable, str(ROOT / 'scripts/sample_m1_performance.py'), '--self-test', '--output-dir', str(output / 'm1-performance-sampler-self-test')]),
        ('m1-readonly-actions-self-test', [sys.executable, str(ROOT / 'scripts/capture_m1_readonly_actions.py'), '--self-test', '--output-dir', str(output / 'm1-readonly-actions-self-test')]),
        ('m1-readonly-window-driver-self-test', [sys.executable, str(ROOT / 'scripts/drive_m1_readonly_window.py'), '--self-test']),
        ('m1-scroll-window-self-test', [sys.executable, str(ROOT / 'scripts/capture_m1_scroll_window.py'), '--self-test']),
        ('window-failure-probe-self-test', [sys.executable, str(ROOT / 'scripts/run_window_failure_probe.py'), '--self-test', '--output-dir', str(output / 'window-failure-probe-self-test')]),
        ('signal-probe-self-test', [sys.executable, str(ROOT / 'scripts/run_signal_probe.py'), '--self-test', '--output-dir', str(output / 'signal-probe-self-test')]),
        ('m1-trace-metrics-self-test', [sys.executable, str(ROOT / 'scripts/m1_trace_metrics.py'), '--self-test', '--output-dir', str(output / 'm1-trace-metrics-self-test')]),
        ('m1-trace-record-self-test', [sys.executable, str(ROOT / 'scripts/m1_trace_record.py'), '--self-test', '--output-dir', str(output / 'm1-trace-record-self-test')]),
        ('m1-rederive-performance-self-test', [sys.executable, str(ROOT / 'scripts/m1_rederive_performance.py'), '--self-test', '--output-dir', str(output / 'm1-rederive-performance-self-test')]),
        ('w1-offscreen-self-test', [sys.executable, str(ROOT / 'scripts/run_w1_offscreen.py'), '--self-test', '--output-dir', str(output / 'w1-offscreen-self-test')]),
        ('ui-tokens', [sys.executable, str(ROOT / 'scripts/check_ui_tokens.py'), '--output-dir', str(output / 'ui-tokens')]),
        ('ui-tokens-self-test', [sys.executable, str(ROOT / 'scripts/check_ui_tokens.py'), '--self-test', '--output-dir', str(output / 'ui-tokens-self-test')]),
        ('ui-catalog', [sys.executable, str(ROOT / 'scripts/check_ui_catalog.py'), '--output-dir', str(output / 'ui-catalog')]),
        ('ui-catalog-self-test', [sys.executable, str(ROOT / 'scripts/check_ui_catalog.py'), '--self-test', '--output-dir', str(output / 'ui-catalog-self-test')]),
        ('rustc-version', ['rustc', '-Vv']),
        ('cargo-version', ['cargo', '-V']),
        ('fmt-version', ['cargo', 'fmt', '--version']),
        ('clippy-version', ['cargo', 'clippy', '--version']),
        ('fmt-effective-config', ['rustfmt', '--config-path', str(ROOT / 'rustfmt.toml'), '--print-config', 'current', str(ROOT / 'app/src/main.rs')]),
        ('fmt', ['cargo', 'fmt', '--all', '--check']),
        ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings']),
        ('test', ['cargo', 'test', '--workspace', '--locked', '--offline']),
        ('dependency', [sys.executable, str(ROOT / 'scripts/check_engineering.py'), '--output-dir', str(output / 'dependency-graphs')]),
        ('dependency-self-test', [sys.executable, str(ROOT / 'scripts/check_engineering.py'), '--self-test', '--output-dir', str(output / 'dependency-fixtures')]),
        ('lint', [sys.executable, str(ROOT / 'scripts/check_lints.py'), '--output-dir', str(output / 'lint')]),
        ('lint-self-test', [sys.executable, str(ROOT / 'scripts/check_lints.py'), '--self-test', '--output-dir', str(output / 'lint-fixtures')]),
        ('safety', [sys.executable, str(ROOT / 'scripts/check_safety.py'), '--output-dir', str(output / 'safety')]),
        ('safety-self-test', [sys.executable, str(ROOT / 'scripts/check_safety.py'), '--self-test', '--output-dir', str(output / 'safety-fixtures')]),
        ('public-api', [sys.executable, str(ROOT / 'scripts/check_public_api.py'), '--self-test', '--output-dir', str(output / 'public-api')]),
    ]
    records.extend(run_checks(checks, output, env))
    passed = all(row['exit_code'] == 0 for row in records)
    # 只补范围文案：M1 7.1 新增的三个 CLI 自测已纳入本清单，失败仍不短路后续项。
    summary = {'passed': passed, 'scope': '当前 macOS 主机；目标图解析不等于跨平台构建；CI 是否执行另行记录；'
               'M1 7.1 新增入口 m1_trace_metrics.py / m1_trace_record.py / m1_rederive_performance.py '
               '的 --self-test 已纳入顺序清单，与其它条目同样单条失败不短路；'
               'M1 7.3 只读真实窗口驱动 drive_m1_readonly_window.py 的合成自测同样纳入清单，'
               '它只证明驱动判定与等待策略，不证明真实窗口七步；'
               'M1 7.3 标准入口 readonly-bytes 自测还覆盖门禁回读原始证据的反例，'
               '合成证据树不构成任何保真证据；'
               'M1 8.2 真实窗口滚动入口 capture_m1_scroll_window.py 的合成自测同样纳入清单，'
               '它只证明截图读数与判定逻辑，不证明真实窗口滚动；'
               'M1 startup 4.3 的窗口失败复查 run_window_failure_probe.py 与启动信号探针 '
               'run_signal_probe.py 的自测同样纳入清单，它们只证明文案分类/信号解析与证据'
               '解释逻辑，不执行真实沙箱或窗口实验',
               'catalog': 'cargo test --workspace 实际发现 app-core 正反例与 UI 消费测试',
               'results': records}
    write_json(output / 'summary.json', summary)
    print('证据：' + str(output), flush=True)
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--cargo-home', type=Path)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    try:
        if args.self_test:
            return self_test(args.output_dir)
        return run(args)
    except (OSError, ValueError, KeyError) as error:
        print('检查入口拒绝：' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
