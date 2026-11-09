#!/usr/bin/env python3
"""实际启动 Release 原生窗口，保存逐次收据、真实阶段边界与进程峰值 RSS。

此入口不把 draw 提交称为屏幕呈现，不把事件循环抖动称为精确主线程停顿。
尚缺这两类真实指标时仍保存可用数据，但矩阵摘要明确失败，退出 1。
冷条件是全新进程的 Aline/GPUI 缓存；热条件是同进程完整预热后重开。
不清空 OS 文件缓存，事前哈希读取也会影响 OS 缓存，不能宣称 OS 冷启动。
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import subprocess
import sys
import time

from collect_m1_readonly import collect, parse_spec, sha256
from evidence_paths import write_json, write_text
from generate_m1_fixtures import _patterns, SEED
from run_m1_readonly import CONDITIONS, MIN_SAMPLES, nearest_rank
from validation_output import output_directory

ROOT = Path(__file__).resolve().parent.parent
COLD = ("每次新建原生进程且不预热同一文档；正常空窗与字体启动装配仍会触及GPUI字形/管线，"
        "只声明无同文档预热，不声明这些启动缓存仍为空。未清空 OS 缓存，"
        "事前内容哈希已读取文件，因此不声明 OS 文件缓存为冷。")
HOT = ("同一原生进程先完整打开同一文件、draw 提交、滚动、模式往返、关闭，"
       "再执行测量打开；进程字体/字形缓存可复用，文档状态重新构造；未清空 OS 缓存。")
METHODS = {
    "read_decode": "后台实际文件打开前→有界读取/UTF-8解码/基线创建后，独立墙钟与Instant边界",
    "snapshot": "后台源码预算检查前→只读DocCore/快照/请求身份创建后，独立墙钟与Instant边界",
    "parse_mapping": "后台parse_with_cancel前→解析/映射及文档结果身份复验后，独立墙钟与Instant边界",
    "main_thread_adopt": "主线程read_finished调用前→候选结算后，独立墙钟与Instant边界",
    "background_dispatch_wait_ns": "后台任务提交前→后台任务体开始，独立墙钟与Instant边界；原始diagnostic保留",
    "main_thread_result_wait_ns": "后台候选完成→主线程采纳回调入口，独立墙钟与Instant边界；原始diagnostic保留",
    "first_layout": "首内容render入口→根子元素prepaint完成，独立墙钟与Instant边界",
    "timing_clock": "duration与单调起止offset均来自同一Instant时钟；墙钟独立捕获，仅关联外部时间线。clock_uncertainty_ns是捕获开销，不包括clock_getres分辨率，不把两种clock纳秒差值强制相等。",
    "mode_switch": "程序化真实模式切换请求前→平台draw提交后effect回调；不代表屏幕呈现",
    "scroll": "程序化真实PageDown请求前→平台draw提交后effect回调；不代表屏幕呈现",
    "peak_rss_bytes": "macOS getrusage(RUSAGE_CHILDREN)返回worker唯一原生子进程ru_maxrss（字节）；整进程高水位，含热条件预热，不是逐阶段RSS",
    "cold": COLD, "hot": HOT,
}
MISSING = [
    "first_present：锁定 GPUI 的 draw effect 只证明平台提交，未获得屏幕呈现完成时间。",
    "main_thread_stall_ns：未获得精确主线程调度/阻塞时间线；event_loop_gap 仅保留为诊断。",
]


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def command(argv, directory, stem, env=None):
    started = now()
    result = subprocess.run(argv, cwd=ROOT, env=env, capture_output=True, text=True, check=False)
    write_text(directory / (stem + '.stdout'), result.stdout)
    write_text(directory / (stem + '.stderr'), result.stderr)
    write_json(directory / (stem + '.command.json'), {
        'argv': argv, 'cwd': str(ROOT), 'started_utc': started,
        'finished_utc': now(), 'exit_code': result.returncode,
    })
    return result


def fixed_utf8(pattern, size):
    data = (pattern * (size // len(pattern) + 1))[:size]
    while True:
        try:
            data.decode('utf-8')
            return data + b' ' * (size - len(data))
        except UnicodeDecodeError:
            data = data[:-1]


def fixtures(directory):
    directory.mkdir()
    result = {}
    ordinary = b'## Section\n\nalpha beta gamma delta epsilon.\n\n'
    sizes = {'32KiB': 32768, '128KiB': 131072, '1MiB': 1048576, '4MiB': 4194304}
    for condition, size in sizes.items():
        result[condition] = fixed_utf8(ordinary, size)
    result['>4MiB'] = fixed_utf8('## 章节\n\n中文性能样本内容。\n\n'.encode(), 5242880)
    patterns = _patterns(SEED)
    result.update({
        'many-short-blocks': fixed_utf8(ordinary, 131072),
        'single-giant-block': fixed_utf8(patterns['single-giant-paragraph'], 131072),
        'giant-table': fixed_utf8(patterns['giant-table'], 131072),
        'dense-entities': fixed_utf8(patterns['dense-entities'], 131072),
    })
    manifest = {}
    for index, condition in enumerate(CONDITIONS):
        path = directory / ('fixture-' + str(index) + '.md')
        path.write_bytes(result[condition])
        manifest[condition] = {'path': str(path.relative_to(ROOT)), 'bytes': len(result[condition]),
                               'sha256': sha256(path)}
    write_json(directory / 'manifest.json', manifest)
    return manifest


def source_identity():
    paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard'],
                                    cwd=ROOT, text=True).splitlines()
    manifest = {}
    for name in sorted(set(paths)):
        path = ROOT / name
        if path.is_file() and not path.is_symlink():
            manifest[name] = sha256(path)
    encoded = json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()
    return {'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
            'manifest_sha256': hashlib.sha256(encoded).hexdigest(), 'files': manifest}


def wait_native(app, env, timeout):
    """一个 worker 只有一个原生子进程；RUSAGE_CHILDREN 因而绑定该 pid。"""
    started = now()
    process = subprocess.Popen([str(app)], cwd=ROOT, env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill()
        stdout, stderr = process.communicate()
        return process, stdout, stderr, started, now(), None, 'native-timeout'
    # communicate 已waitpid，RUSAGE_CHILDREN仅覆盖本worker唯一原生子进程。
    # worker不得在该调用前启动其它子进程，否则高水位不再唯一归属。
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return process, stdout, stderr, started, now(), int(usage.ru_maxrss), None


def worker(args):
    directory = output_directory("m1-performance-worker", args.worker)
    config = json.loads((directory / 'worker.json').read_text())
    app = ROOT / config['app']
    # 单次采样只启用自身探针；不继承其它M1实验(如逐事件快照)造成额外I/O。
    env = {key: value for key, value in os.environ.items() if not key.startswith('ALINE_M1_')}
    env.update({
        'ALINE_M1_PROBE_SPEC': str(directory / 'spec.txt'),
        'ALINE_M1_PROBE_OUT': str(directory / 'raw.tsv'),
        'ALINE_M1_PROBE_DONE': str(directory / 'done'),
        'ALINE_M1_PROBE_MEMORY_DIR': str(directory / 'memory'),
        'ALINE_M1_PROBE_RUN_ID': config['run_id'],
        'ALINE_M1_PROBE_APP_SHA256': config['app_sha256'],
        'ALINE_M1_PROBE_SPEC_SHA256': config['spec_sha256'],
    })
    process, out, err, started, finished, rss, error = wait_native(app, env, args.timeout)
    write_text(directory / 'native.stdout', out.decode('utf-8', errors='replace'))
    write_text(directory / 'native.stderr', err.decode('utf-8', errors='replace'))
    receipt = {'schema': 'aline-m1-launch/v1', 'pid': process.pid,
               'started_utc': started, 'finished_utc': finished, 'exit_code': process.returncode,
               **{key: config[key] for key in ('run_id', 'app_sha256', 'spec_sha256', 'fixtures')},
               'raw_sha256': sha256(directory / 'raw.tsv') if (directory / 'raw.tsv').is_file() else None,
               'app_sha256_after': sha256(app), 'error': error}
    metadata = {**config, 'run_started_utc': started, 'launch_receipt': receipt,
                'measurement_methods': METHODS,
                'process_metrics': {'method': 'getrusage-single-native-child-high-water',
                                    'pid': process.pid, 'peak_rss_bytes': rss}}
    write_json(directory / 'metadata.json', metadata)
    return 0 if not error and process.returncode == 0 else 1


def run(args, output):
    if platform.system() != 'Darwin' or platform.machine() != 'arm64':
        raise ValueError('此真实采样入口仅接受本轮 macOS arm64')
    environment = {'os': platform.platform(), 'arch': platform.machine(),
                   'cpu': platform.processor() or platform.machine(),
                   'memory_bytes': int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True)),
                   'gpui': 'gpui-pre=0.3.9; gpui-pre-macos=0.3.9',
                   'clock_resolutions': {name: {'clock_id': getattr(time, name),
                                               'resolution_seconds': time.clock_getres(getattr(time, name))}
                                         for name in ('CLOCK_REALTIME', 'CLOCK_UPTIME_RAW')}}
    for name, argv in [('rustc', ['rustc', '--version']), ('cargo', ['cargo', '--version']),
                       ('xcode', ['xcodebuild', '-version']), ('sw-vers', ['sw_vers'])]:
        result = command(argv, output, name)
        environment[name] = result.stdout.strip()
    before = source_identity()
    write_json(output / 'source-identity.json', before)
    result = command(['cargo', 'build', '-p', 'aline', '--release', '--locked', '--offline', '--message-format=json-render-diagnostics'], output, 'build')
    if result.returncode:
        raise ValueError('Release构建失败，见build.stdout/stderr')
    after = source_identity()
    if before != after:
        raise ValueError('构建期间源码身份变化，拒绝把新源码与旧二进制混合采样')
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
    binaries = [item for item in artifacts if item.get('reason') == 'compiler-artifact'
                and item.get('target', {}).get('name') == 'aline' and item.get('executable')]
    if len(binaries) != 1 or binaries[0].get('profile', {}).get('debug_assertions') is not False:
        raise ValueError('缺少唯一真实Release可执行工件或debug_assertions仍启用')
    app = Path(binaries[0]['executable']).resolve()
    app.relative_to(ROOT)
    arch_result = command(['lipo', '-archs', str(app)], output, 'binary-architecture')
    if arch_result.returncode != 0 or arch_result.stdout.strip() != 'arm64':
        raise ValueError('原生工件不是本轮要求的macOS arm64单架构')
    app_hash = sha256(app)
    build_environment = {key: value for key, value in os.environ.items()
                         if key in ('CARGO_HOME', 'RUSTUP_HOME', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                                    'CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR') or key.startswith('CARGO_PROFILE_RELEASE_')}
    write_json(output / 'build-identity.json', {'artifact': binaries[0], 'app_sha256': app_hash,
                                              'source_manifest_sha256': before['manifest_sha256'],
                                              'build_environment': build_environment})
    manifest = fixtures(output / 'fixtures')
    write_text(output / 'methods.md', '\n\n'.join([COLD, HOT, *MISSING,
        'RSS为每个原生进程生命周期高水位；热条件包括预热，不能称为逐阶段峰值。',
        '夹具尺寸组使用重复短块；>4 MiB为多字节中文短块，避免把ASCII映射点预算拒绝冒充Source回退。',
        '32/128 KiB、1/4/>4 MiB尺寸组与四类复杂度均独立成组；程序化真实动作不作为人工鼠标/键盘证据。']))
    matrix = []
    conditions = [args.condition] if args.condition else list(CONDITIONS)
    run_id = output.name
    for condition in conditions:
        for temperature in ('cold', 'hot'):
            for sample_id in range(args.samples):
                name = f'c{CONDITIONS.index(condition)}-{temperature}-{sample_id:02d}'
                directory = output / name
                directory.mkdir()
                (directory / 'memory').mkdir()
                entries = []
                if temperature == 'hot':
                    entries.append(('warmup-' + name, 'warmup'))
                entries.append((name, 'perf'))
                fixture = manifest[condition]
                spec = ''.join(f'{case}|{condition}|{temperature}|{sample_id}|{fixture["path"]}|{suite}\n'
                               for case, suite in entries)
                write_text(directory / 'spec.txt', spec)
                config = {'run_id': run_id, 'commit': before['commit'], 'source_manifest_sha256': before['manifest_sha256'],
                          'app': str(app.relative_to(ROOT)), 'app_sha256': app_hash, 'build_profile': 'release',
                          'environment': environment, 'spec_sha256': sha256(directory / 'spec.txt'),
                          'fixtures': {case: {**fixture, 'condition': condition, 'temperature': temperature,
                                            'sample_id': sample_id, 'suite': suite} for case, suite in entries}}
                write_json(directory / 'worker.json', config)
                result = command([sys.executable, str(Path(__file__).resolve()), '--worker', str(directory),
                                  '--timeout', str(args.timeout)], directory, 'worker')
                entry = {'case': name, 'condition': condition, 'temperature': temperature, 'sample_id': sample_id,
                         'metadata': str((directory / 'metadata.json').relative_to(output)), 'passed': False}
                if result.returncode == 0 and sha256(app) == app_hash:
                    try:
                        collect(directory / 'raw.tsv', directory / 'done', directory / 'metadata.json',
                                directory / 'spec.txt', 'performance', directory / 'memory', directory / 'measurements.jsonl')
                        entry['collected'] = True
                    except (ValueError, OSError, KeyError) as error:
                        entry['error'] = str(error)
                else:
                    entry['error'] = '原生worker失败或二进制在采样期间改变'
                matrix.append(entry)
                print(name + ': ' + ('collected; full acceptance missing present/stall' if entry.get('collected') else entry.get('error', 'failed')), flush=True)
                write_json(output / 'progress.json', {'runs': matrix, 'passed': False, 'missing': MISSING})
    groups = {}
    peaks = []
    for entry in matrix:
        if not entry.get('collected'):
            continue
        directory = output / entry['case']
        metadata = json.loads((directory / 'metadata.json').read_text())
        peaks.append(metadata['process_metrics']['peak_rss_bytes'])
        for row in map(json.loads, (directory / 'measurements.jsonl').read_text().splitlines()):
            if row.get('kind') != 'sample':
                continue
            key = (row['condition'], row['temperature'], row['stage'])
            groups.setdefault(key, []).append(row['duration_ns'])
    summary = {'schema': 'aline-m1-performance-matrix-observations/v1', 'passed': False,
               'run_id': run_id, 'commit': before['commit'], 'app_sha256': app_hash,
               'source_manifest_sha256': before['manifest_sha256'], 'environment': environment,
               'methods': METHODS, 'missing': MISSING, 'runs': matrix,
               'observed_process_peak_rss_bytes': max(peaks) if peaks else None,
               'groups': [{'condition': key[0], 'temperature': key[1], 'stage': key[2], 'count': len(values),
                           'p50_ns': nearest_rank(values, 50), 'p95_ns': nearest_rank(values, 95), 'max_ns': max(values)}
                          for key, values in sorted(groups.items())]}
    if args.samples < MIN_SAMPLES:
        summary['missing'].append('此次为能力冒烟，未达到每组至少20次。')
    ending_identity = source_identity()
    write_json(output / 'source-identity-after.json', ending_identity)
    if ending_identity != before:
        summary['missing'].append('采样期间源码发生变化；二进制身份固定，但新源码未包含于构建。')
        summary['changed_paths_during_sampling'] = [name for name in sorted(set(before['files']) | set(ending_identity['files']))
                                                    if before['files'].get(name) != ending_identity['files'].get(name)]
    write_json(output / 'matrix-summary.json', summary)
    return 1


def self_test(output):
    """仅验证runner契约；合成子进程不是原生窗口或性能验收证据。"""
    checks = []
    def check(name, condition):
        checks.append({"name": name, "passed": bool(condition)})
        if not condition:
            raise AssertionError(name)
    for size in range(1, 20):
        data = fixed_utf8("中文🙂".encode(), size)
        check("UTF-8精确尺寸-" + str(size), len(data) == size and data.decode('utf-8') is not None)
    left, right = fixtures(output / 'fixtures-a'), fixtures(output / 'fixtures-b')
    check('九组夹具独立且确定', len(left) == 9 and all(left[key]['sha256'] == right[key]['sha256'] for key in CONDITIONS))
    outside = ROOT / 'docs' / ('must-not-create-worker-' + output.name)
    result = command([sys.executable, str(Path(__file__).resolve()), '--worker', str(outside)], output, 'reject-worker')
    check('worker拒绝越界且无写入', result.returncode != 0 and not outside.exists())
    for scenario in ('ok', 'timeout'):
        directory = output / scenario
        directory.mkdir()
        (directory / 'memory').mkdir()
        app = directory / 'synthetic-child.py'
        script = ("#!/usr/bin/env python3\nimport os,time\nfrom pathlib import Path\n"
                  "print(os.getpid(), flush=True)\n")
        if scenario == 'timeout':
            script += 'time.sleep(10)\n'
        else:
            script += ("Path(os.environ['ALINE_M1_PROBE_OUT']).write_text('kind=done\\n')\n"
                       "Path(os.environ['ALINE_M1_PROBE_DONE']).write_text('ok')\n")
        write_text(app, script)
        app.chmod(0o700)
        write_text(directory / 'spec.txt', 'synthetic contract self-test only')
        config = {'app': str(app.relative_to(ROOT)), 'run_id': 'synthetic-contract-test',
                  'app_sha256': sha256(app), 'spec_sha256': sha256(directory / 'spec.txt'), 'fixtures': {}}
        write_json(directory / 'worker.json', config)
        result = command([sys.executable, str(Path(__file__).resolve()), '--worker', str(directory),
                          '--timeout', '2' if scenario == 'timeout' else '10'], directory, 'worker-test')
        metadata = json.loads((directory / 'metadata.json').read_text())
        receipt = metadata['launch_receipt']
        stdout_pid = int((directory / 'native.stdout').read_text().strip())
        check(scenario + '-收据绑定实际pid', receipt['pid'] == stdout_pid)
        if scenario == 'ok':
            check('成功子进程高水位为正数', result.returncode == 0 and metadata['process_metrics']['peak_rss_bytes'] > 0)
            check('收据原始输出哈希绑定', receipt['raw_sha256'] == sha256(directory / 'raw.tsv'))
        else:
            check('超时失败关闭且不伪造RSS', result.returncode == 1 and receipt['error'] == 'native-timeout'
                  and metadata['process_metrics']['peak_rss_bytes'] is None)
    write_json(output / 'summary.json', {'passed': all(item['passed'] for item in checks),
                                        'synthetic_contract_test_only': True, 'checks': checks})
    print('self-test: ' + str(len(checks)) + ' passed')
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--samples', type=int, default=MIN_SAMPLES)
    parser.add_argument('--condition', choices=CONDITIONS)
    parser.add_argument('--timeout', type=float, default=90)
    parser.add_argument('--worker', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.samples < 1 or args.timeout <= 0:
        parser.error('samples/timeout必须为正数')
    if args.worker:
        return worker(args)
    try:
        output = output_directory('m1-performance-native', args.output_dir)
        if args.self_test:
            return self_test(output)
        return run(args, output)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        if 'output' not in locals():
            print(str(error), file=sys.stderr)
            return 1
        write_json(output / 'failure.json', {'passed': False, 'error': str(error)})
        print('performance: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
