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
import re
import resource
import signal
import subprocess
import sys
import time

from collect_m1_readonly import collect, parse_spec, sha256
from evidence_paths import write_json, write_text
from generate_m1_fixtures import _patterns, SEED
from m1_trace_metrics import derive as derive_trace_metrics, load_tables, parse_toc, parse_tsv
import m1_trace_record
from run_m1_readonly import CONDITIONS, MIN_SAMPLES, analyze_performance, nearest_rank
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
    "first_present": "external-trace-display-chain：Metal System Trace 的 present request/command buffer/displayed-surfaces-interval 链；取屏幕扫描开始，不声称像素已进入人眼",
    "main_thread_stall_ns": "external-trace-runloop-busy：窗口内主线程 individual_iteration 扣除 waiting_for_events 后的最长连续忙碌段",
    "timing_clock": "duration与单调起止offset均来自同一Instant时钟；墙钟独立捕获，仅关联外部时间线。clock_uncertainty_ns是捕获开销，不包括clock_getres分辨率，不把两种clock纳秒差值强制相等。",
    "display_environment": "每个样本启动前要求屏幕未锁定、显示器清醒且处于已登录控制台会话；目标窗口成为前台并有在屏普通窗口后才释放 gate；不满足即失败关闭并中止矩阵，不产生样本。",
    "mode_switch": "程序化真实模式切换请求前→平台draw提交后effect回调；不代表屏幕呈现",
    "scroll": "程序化真实PageDown请求前→平台draw提交后effect回调；不代表屏幕呈现",
    "peak_rss_bytes": "macOS getrusage(RUSAGE_CHILDREN)返回worker唯一原生子进程ru_maxrss（字节）；整进程高水位，含热条件预热，不是逐阶段RSS",
    "cold": COLD, "hot": HOT,
}
MISSING = [
    "first_present 取 displayed-surfaces-interval 的 start，是屏幕扫描开始的下界，不声称像素已进入人眼。",
    "main_thread_stall_ns 是窗口内主线程 runloop 忙碌段，不含 waiting_for_events，不是 hang 阈值事件。",
    "trace 录制本身有开销，阶段耗时与无 trace 矩阵不可直接混比。",
    "采样要求屏幕解锁且显示器常亮；锁屏/睡眠期间被拒绝的样本不是产品行为证据。",
    "Windows 10/Intel Mac/Linux 未实测，本轮仅 macOS arm64。",
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


def wait_native(app, env, timeout, directory):
    """一个 worker 只有一个原生子进程；RUSAGE_CHILDREN 因而绑定该 pid。"""
    started = now()
    process = subprocess.Popen([str(app)], cwd=ROOT, env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    # 父采样器要在 probe 进入测量序列前 attach；pid 必须先于任何等待落盘。
    write_text(directory / 'app.pid', str(process.pid))
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
        'ALINE_M1_PROBE_GATE': str(directory / 'gate'),
        'ALINE_M1_PROBE_GATE_READY': str(directory / 'gate.ready'),
    })
    process, out, err, started, finished, rss, error = wait_native(
        app, env, args.timeout, directory)
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


def _wait_file(path, timeout, label):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            return
        time.sleep(0.01)
    raise ValueError("等待 " + label + " 超时")


def _wait_text(path, timeout, label, pattern):
    """等待文件内容完整匹配；空文件或半写内容不算就绪。"""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            text = path.read_text(encoding='utf-8').strip()
        except FileNotFoundError:
            text = ''
        match = re.fullmatch(pattern, text)
        if match:
            return match
        time.sleep(0.01)
    raise ValueError("等待 " + label + " 超时")


class EnvironmentUnavailable(ValueError):
    """屏幕/会话状态不允许采集真实呈现证据；必须失败关闭并中止矩阵。"""


DISPLAY_TOOL_SOURCE = ROOT / 'scripts' / 'm1_display_state.swift'
DISPLAY_STATE_SCHEMA = 'aline-m1-display-state/v1'
ACTIVATION_SCHEMA = 'aline-m1-window-activation/v1'


def _build_display_tool(output):
    """编译一次只读环境检查工具，记录源码哈希与编译器版本。"""
    directory = output / 'tools'
    directory.mkdir(exist_ok=True)
    binary = directory / 'm1_display_state'
    version = command(['swiftc', '--version'], output, 'swiftc-version')
    if version.returncode != 0:
        raise ValueError('swiftc 不可用，无法验证屏幕/窗口呈现环境')
    built = command(['swiftc', '-O', '-o', str(binary), str(DISPLAY_TOOL_SOURCE)],
                    output, 'display-tool-build')
    if built.returncode != 0:
        raise ValueError('display state 工具编译失败，见 display-tool-build.stderr')
    write_json(output / 'display-tool.json', {
        'source': str(DISPLAY_TOOL_SOURCE.relative_to(ROOT)),
        'source_sha256': sha256(DISPLAY_TOOL_SOURCE),
        'swiftc_version': version.stdout.strip(),
        'binary_sha256': sha256(binary)})
    return binary


def _tool_json(tool, argv, label):
    result = subprocess.run([str(tool), *argv], cwd=ROOT, capture_output=True,
                            text=True, check=False)
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        raise ValueError(label + '输出无效: ' + result.stdout[:200] + result.stderr[:200])
    return result.returncode, payload


def _require_display_ready(payload):
    """只有屏幕未锁定、显示器清醒且处于已登录控制台会话才允许采样。"""
    if payload.get('schema') != DISPLAY_STATE_SCHEMA:
        raise ValueError('display state schema 不符')
    if payload.get('screen_locked') is not False:
        raise EnvironmentUnavailable('屏幕已锁定，无法采集真实屏幕呈现证据')
    if payload.get('display_asleep') is not False:
        raise EnvironmentUnavailable('显示器睡眠，无法采集真实屏幕呈现证据')
    if payload.get('display_active') is not True:
        raise EnvironmentUnavailable('显示器未激活，无法采集真实屏幕呈现证据')
    if payload.get('on_console') is not True or payload.get('login_done') is not True:
        raise EnvironmentUnavailable('当前不是已登录的控制台会话，无法采集真实屏幕呈现证据')


def _display_check(tool):
    _, payload = _tool_json(tool, ['check'], 'display state ')
    return payload


def _require_activation(payload):
    """窗口必须在屏且为前台，否则 display 链不能代表用户可见呈现。"""
    if payload.get('schema') != ACTIVATION_SCHEMA:
        raise ValueError('window activation schema 不符')
    if payload.get('found') is not True:
        raise ValueError('目标进程不存在，无法确认在屏窗口')
    if payload.get('frontmost') is not True:
        raise ValueError('目标窗口未成为前台应用，拒绝释放测量 gate')
    if not isinstance(payload.get('layer0_windows'), int) or payload['layer0_windows'] < 1:
        raise ValueError('目标应用没有在屏普通窗口，拒绝释放测量 gate')
    if payload.get('has_nonzero_bounds') is not True:
        raise ValueError('目标窗口边界为零，拒绝释放测量 gate')


def _window_activation(tool, app_pid, timeout_ms=5000):
    _, payload = _tool_json(tool, ['activate', str(app_pid), str(timeout_ms)],
                            'window activation ')
    return payload


def _start_caffeinate(output):
    """矩阵期间阻止显示器空闲睡眠；不改变任何测量值，只维持可见会话。"""
    try:
        process = subprocess.Popen(['caffeinate', '-dimsu', '-w', str(os.getpid())],
                                   cwd=ROOT, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.PIPE)
    except FileNotFoundError:
        raise ValueError('caffeinate 不可用，无法保持显示器常亮')
    time.sleep(1)
    if process.poll() is not None:
        error = process.stderr.read().decode('utf-8', errors='replace')
        raise ValueError('caffeinate 启动失败: ' + error)
    assertions = command(['pmset', '-g', 'assertions'], output, 'assertions')
    match = re.search(r'PreventUserIdleDisplaySleep\s+(\d+)', assertions.stdout)
    if not match or int(match.group(1)) < 1:
        process.kill()
        process.wait()
        raise ValueError('caffeinate 未取得 PreventUserIdleDisplaySleep 断言')
    write_json(output / 'caffeinate.json', {
        'pid': process.pid, 'argv': ['caffeinate', '-dimsu', '-w', '<sampler-pid>'],
        'prevent_user_idle_display_sleep': int(match.group(1))})
    return process


def _kill_pid(pid):
    if not isinstance(pid, int) or pid <= 0:
        return
    try:
        os.kill(pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        # 目标已退出时 macOS 可能返回 EPERM；清理失败不得掩盖原始错误。
        pass


def _kill_process_group(process):
    """终止子进程组并等待回收；清理失败不得掩盖调用方的原始错误。

    进程组 leader 已退出但未被 wait 时，macOS 对 killpg 返回 EPERM；
    该错误必须在此收敛，否则会顶替掉真正的失败原因。
    """
    if process is None or process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    process.wait()


def _release_gate(directory, app_pid, trace_argv, marker_timeout=20,
                  settle_ns=4_000_000_000):
    """trace helper 确认开始录制后才释放 gate；否则不释放并终止 helper。"""
    marker = directory / 'trace.recording.json'
    stdout = open(directory / 'trace.stdout', 'wb')
    stderr = open(directory / 'trace.stderr', 'wb')
    process = subprocess.Popen(trace_argv, cwd=ROOT, stdout=stdout, stderr=stderr,
                               start_new_session=True)
    try:
        _wait_file(marker, marker_timeout, 'trace recording marker')
        payload = json.loads(marker.read_text(encoding='utf-8'))
        if (payload.get('schema') != 'aline-m1-trace-recording/v1'
                or payload.get('pid') != app_pid):
            raise ValueError('trace marker 未绑定目标 pid')
        settle = int(payload['epoch_ns']) + settle_ns - time.time_ns()
        if settle > 0:
            time.sleep(settle / 1_000_000_000)
        write_text(directory / 'gate', str(time.time_ns()))
        return process
    except BaseException:
        _kill_process_group(process)
        raise
    finally:
        stdout.close()
        stderr.close()


TRACE_TIME_LIMIT = 30
# marker→trace 实际起点的历史最大延迟 2.5 s；runloop/Metal 仪器还要
# 数百毫秒才覆盖测量窗口，因此释放 gate 前留 4 s 余量。
TRACE_SETTLE_NS = 4_000_000_000


def sampled(directory, args, app, app_hash, display_tool):
    """真实启动 worker 与 trace 兄弟进程，导出逐样本指标并交给采集器。

    worker 只 wait 唯一原生子进程，RUSAGE_CHILDREN 因此仍绑定该 pid；
    trace helper 由本进程作为兄弟启动，不进入 worker 的 children 统计。
    """
    config = json.loads((directory / 'worker.json').read_text(encoding='utf-8'))
    worker_argv = [sys.executable, str(Path(__file__).resolve()), '--worker',
                   str(directory), '--timeout', str(args.timeout)]
    started = now()
    worker_stdout = open(directory / 'worker.stdout', 'wb')
    worker_stderr = open(directory / 'worker.stderr', 'wb')
    worker_process = subprocess.Popen(worker_argv, cwd=ROOT,
                                      stdout=worker_stdout, stderr=worker_stderr)
    trace_process = None
    app_pid = None
    trace_argv = None
    trace_done = directory / 'trace.done.json'
    try:
        _wait_file(directory / 'app.pid', 30, 'app.pid')
        app_pid = int((directory / 'app.pid').read_text(encoding='utf-8').strip())
        ready = _wait_text(directory / 'gate.ready', 30, 'gate.ready', r'pid=(\d+)')
        ready_pid = int(ready.group(1))
        if ready_pid != app_pid:
            raise ValueError('gate ready pid 与 app.pid 不同')
        # 探针已建窗并等待 gate；先确认窗口在屏且为前台，再让 trace 覆盖测量窗口。
        activation = _window_activation(display_tool, app_pid)
        write_json(directory / 'window-activation.json', activation)
        _require_activation(activation)
        trace_argv = [sys.executable,
                      str(ROOT / 'scripts/m1_trace_record.py'), '--record',
                      '--pid', str(app_pid),
                      '--output', str(directory / 'trace.trace'),
                      '--time-limit', str(TRACE_TIME_LIMIT),
                      '--marker', str(directory / 'trace.recording.json'),
                      '--done', str(trace_done),
                      '--stdout', str(directory / 'trace.stdout'),
                      '--stderr', str(directory / 'trace.stderr')]
        trace_process = _release_gate(directory, app_pid, trace_argv,
                                      settle_ns=TRACE_SETTLE_NS)
        worker_process.communicate(timeout=args.timeout + 30)
        # xctrace 启动与导出都可能很慢；超时过紧会留下空 JSON 冒充结果。
        trace_process.communicate(timeout=TRACE_TIME_LIMIT + 90)
    except BaseException:
        _kill_pid(app_pid)
        _kill_process_group(trace_process)
        if worker_process.poll() is None:
            worker_process.kill()
            worker_process.wait()
        raise
    finally:
        worker_stdout.close()
        worker_stderr.close()
        write_json(directory / 'worker.command.json', {
            'argv': worker_argv, 'cwd': str(ROOT), 'started_utc': started,
            'finished_utc': now(), 'exit_code': worker_process.returncode})
    if worker_process.returncode != 0:
        raise ValueError('原生 worker 未成功退出')
    if trace_process.returncode != 0 or not trace_done.is_file():
        raise ValueError('trace 录制未成功结束')
    done_payload = json.loads(trace_done.read_text(encoding='utf-8'))
    if done_payload.get('recording_started') is not True:
        raise ValueError('trace 未确认开始录制')
    write_json(directory / 'trace.command.json', {
        'argv': trace_argv, 'cwd': str(ROOT),
        'exit_code': trace_process.returncode})
    if sha256(app) != app_hash:
        raise ValueError('二进制在采样期间改变')
    paths = m1_trace_record.export_trace(directory / 'trace.trace', directory / 'trace')
    tables = load_tables({name: paths[name] for name in
                          ('assignment', 'request', 'display', 'runloop')})
    toc = parse_toc(paths['toc'])
    raw_rows = parse_tsv(directory / 'raw.tsv')
    case = directory.name
    fixture = config['fixtures'][case]
    expected = {'run_id': config['run_id'], 'app_sha256': config['app_sha256'],
                'spec_sha256': config['spec_sha256'], 'pid': app_pid,
                'case': case, 'condition': fixture['condition'],
                'temperature': fixture['temperature'],
                'sample_id': fixture['sample_id'],
                'fixture_sha256': fixture['sha256']}
    metrics = derive_trace_metrics(raw_rows, toc, tables, expected)
    write_json(directory / 'trace-metrics.json', metrics)
    collect(directory / 'raw.tsv', directory / 'done', directory / 'metadata.json',
            directory / 'spec.txt', 'performance', directory / 'memory',
            directory / 'measurements.jsonl', trace_metrics=metrics)


def load_refill_plan(path):
    """补采计划：只补已失败且需要新编号的 (条件, 冷热, sample_id)。

    计划必须显式列出编号；不按目录猜测，也不复用已失败编号。
    """
    plan = json.loads(Path(path).read_text(encoding='utf-8'))
    if not isinstance(plan, list) or not plan:
        raise ValueError('refill 计划必须是非空列表')
    seen = set()
    cases = []
    for item in plan:
        if not isinstance(item, dict):
            raise ValueError('refill 计划条目必须是对象')
        condition = item.get('condition')
        temperature = item.get('temperature')
        sample_id = item.get('sample_id')
        if condition not in CONDITIONS:
            raise ValueError('refill 条件未登记：' + str(condition))
        if temperature not in ('cold', 'hot'):
            raise ValueError('refill 冷热未登记：' + str(temperature))
        if (not isinstance(sample_id, int) or isinstance(sample_id, bool)
                or sample_id < 0):
            raise ValueError('refill sample_id 必须是非负整数')
        key = (condition, temperature, sample_id)
        if key in seen:
            raise ValueError('refill 计划重复：' + '/'.join(map(str, key)))
        seen.add(key)
        cases.append(key)
    return cases


def run(args, output):
    if platform.system() != 'Darwin' or platform.machine() != 'arm64':
        raise ValueError('此真实采样入口仅接受本轮 macOS arm64')
    # 环境不允许真实上屏时立即失败，不先花时间构建。
    display_tool = _build_display_tool(output)
    before_display = _display_check(display_tool)
    write_json(output / 'display-state-before.json', before_display)
    _require_display_ready(before_display)
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
        '32/128 KiB、1/4/>4 MiB尺寸组与四类复杂度均独立成组；程序化真实动作不作为人工鼠标/键盘证据。',
        '屏幕/会话/窗口状态由 scripts/m1_display_state.swift 只读查询；不满足时不产生样本。']))
    if args.keep_display_awake:
        _start_caffeinate(output)
    matrix = []
    environment_failure = None
    run_id = args.run_id or output.name
    if args.refill_plan:
        if (output / 'progress.json').exists():
            raise ValueError('refill 输出目录已包含 progress.json，拒绝混用')
        case_list = load_refill_plan(args.refill_plan)
    else:
        conditions = [args.condition] if args.condition else list(CONDITIONS)
        case_list = [(condition, temperature, sample_id)
                     for condition in conditions
                     for temperature in ('cold', 'hot')
                     for sample_id in range(args.samples)]
    try:
        for condition, temperature, sample_id in case_list:
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
            entry = {'case': name, 'condition': condition, 'temperature': temperature, 'sample_id': sample_id,
                     'metadata': str((directory / 'metadata.json').relative_to(output)), 'passed': False}
            display_state = _display_check(display_tool)
            write_json(directory / 'display-state.json', display_state)
            try:
                _require_display_ready(display_state)
            except EnvironmentUnavailable as error:
                entry['error'] = str(error)
                entry['environment_failure'] = True
                matrix.append(entry)
                environment_failure = name + ': ' + str(error)
                write_json(output / 'progress.json', {'runs': matrix, 'passed': False, 'missing': MISSING})
                raise
            try:
                sampled(directory, args, app, app_hash, display_tool)
                entry['collected'] = True
            except EnvironmentUnavailable as error:
                entry['error'] = str(error)
                entry['environment_failure'] = True
                matrix.append(entry)
                environment_failure = name + ': ' + str(error)
                write_json(output / 'progress.json', {'runs': matrix, 'passed': False, 'missing': MISSING})
                raise
            except (ValueError, OSError, KeyError, json.JSONDecodeError,
                    subprocess.SubprocessError) as error:
                entry['error'] = str(error)
            matrix.append(entry)
            print(name + ': ' + ('collected' if entry.get('collected') else entry.get('error', 'failed')), flush=True)
            write_json(output / 'progress.json', {'runs': matrix, 'passed': False, 'missing': MISSING})
    except EnvironmentUnavailable:
        pass
    merged = []
    groups = {}
    peaks = []
    for entry in matrix:
        if not entry.get('collected'):
            continue
        directory = output / entry['case']
        metadata = json.loads((directory / 'metadata.json').read_text())
        peaks.append(metadata['process_metrics']['peak_rss_bytes'])
        for row in map(json.loads, (directory / 'measurements.jsonl').read_text().splitlines()):
            merged.append(row)
            if row.get('kind') != 'sample':
                continue
            key = (row['condition'], row['temperature'], row['stage'])
            groups.setdefault(key, []).append(row['duration_ns'])
    write_text(output / 'samples.jsonl',
               '\n'.join(json.dumps(row, ensure_ascii=False) for row in merged) + '\n')
    require_matrix = (args.condition is None and not args.refill_plan
                      and args.samples >= MIN_SAMPLES)
    try:
        acceptance = analyze_performance(merged, require_matrix) if merged else {
            'passed': False, 'failures': ['没有可验收的测量记录'], 'groups': []}
    except (ValueError, KeyError) as error:
        acceptance = {'passed': False, 'failures': ['验收器异常：' + str(error)],
                      'groups': []}
    summary = {'schema': 'aline-m1-performance-matrix-observations/v1',
               'passed': bool(acceptance.get('passed')) and all(
                   entry.get('collected') for entry in matrix),
               'run_id': run_id, 'commit': before['commit'], 'app_sha256': app_hash,
               'source_manifest_sha256': before['manifest_sha256'], 'environment': environment,
               'methods': METHODS, 'missing': MISSING, 'runs': matrix,
               'display_environment': before_display,
               'environment_failure': environment_failure,
               'keep_display_awake': bool(args.keep_display_awake),
               'observed_process_peak_rss_bytes': max(peaks) if peaks else None,
               'groups': [{'condition': key[0], 'temperature': key[1], 'stage': key[2], 'count': len(values),
                           'p50_ns': nearest_rank(values, 50), 'p95_ns': nearest_rank(values, 95), 'max_ns': max(values)}
                          for key, values in sorted(groups.items())],
               'acceptance': acceptance,
               'acceptance_failures': acceptance.get('failures', [])}
    if args.samples < MIN_SAMPLES:
        summary['missing'].append('此次为能力冒烟，未达到每组至少20次。')
    ending_identity = source_identity()
    write_json(output / 'source-identity-after.json', ending_identity)
    if ending_identity != before:
        summary['missing'].append('采样期间源码发生变化；二进制身份固定，但新源码未包含于构建。')
        summary['changed_paths_during_sampling'] = [name for name in sorted(set(before['files']) | set(ending_identity['files']))
                                                    if before['files'].get(name) != ending_identity['files'].get(name)]
    write_json(output / 'matrix-summary.json', summary)
    return 0 if summary['passed'] else 1


def self_test(output):
    """仅验证runner契约；合成子进程不是原生窗口或性能验收证据。"""
    checks = []

    def case_kind(name):
        """否定措辞的用例名即“必须拒绝”的反例；只用于自测证据，不参与验收判定。"""
        if "失败关闭" in name or "被拒绝" in name or "不得" in name or "拒绝越界" in name:
            return "reject"
        return "accept"

    def check(name, condition):
        checks.append({"name": name, "kind": case_kind(name), "passed": bool(condition)})
        if not condition:
            raise AssertionError(name)
    for size in range(1, 20):
        data = fixed_utf8("中文🙂".encode(), size)
        check("UTF-8精确尺寸-" + str(size), len(data) == size and data.decode('utf-8') is not None)
    left, right = fixtures(output / 'fixtures-a'), fixtures(output / 'fixtures-b')
    check('九组夹具独立且确定', len(left) == 9 and all(left[key]['sha256'] == right[key]['sha256'] for key in CONDITIONS))
    plan_path = output / 'refill-plan.json'
    write_json(plan_path, [{'condition': CONDITIONS[0], 'temperature': 'cold', 'sample_id': 20}])
    check('补采计划接受唯一新编号',
          load_refill_plan(plan_path) == [(CONDITIONS[0], 'cold', 20)])
    write_json(plan_path, [{'condition': CONDITIONS[0], 'temperature': 'cold', 'sample_id': 20},
                           {'condition': CONDITIONS[0], 'temperature': 'cold', 'sample_id': 20}])
    try:
        load_refill_plan(plan_path)
        check('补采计划重复编号被拒绝', False)
    except ValueError:
        check('补采计划重复编号被拒绝', True)
    write_json(plan_path, [{'condition': 'nope', 'temperature': 'cold', 'sample_id': 20}])
    try:
        load_refill_plan(plan_path)
        check('补采计划未登记条件被拒绝', False)
    except ValueError:
        check('补采计划未登记条件被拒绝', True)
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
    gate_dir = output / 'gate-orchestration'
    gate_dir.mkdir()
    marker = gate_dir / 'trace.recording.json'
    fake_trace = [sys.executable, '-c',
                  "import json, sys, time\nfrom pathlib import Path\n"
                  "Path(sys.argv[1]).write_text(json.dumps({'schema': "
                  "'aline-m1-trace-recording/v1','pid':int(sys.argv[2]),"
                  "'epoch_ns':time.time_ns()}))\n"
                  "time.sleep(0.05)\n", str(marker), '4242']
    process = _release_gate(gate_dir, 4242, fake_trace, settle_ns=0)
    process.wait()
    check('gate 在确认录制后释放',
          (gate_dir / 'gate').is_file() and process.returncode == 0)
    failed_gate = output / 'gate-no-marker'
    failed_gate.mkdir()
    try:
        _release_gate(failed_gate, 4242,
                      [sys.executable, '-c', 'import time; time.sleep(0.05)'],
                      marker_timeout=0.2, settle_ns=0)
    except ValueError:
        pass
    else:
        raise AssertionError('未确认录制不得释放 gate')
    check('未确认录制不得释放 gate', not (failed_gate / 'gate').exists())
    wrong_pid_marker = failed_gate / 'trace.recording.json'
    wrong_pid = [sys.executable, '-c',
                 "import json, sys\nfrom pathlib import Path\n"
                 "Path(sys.argv[1]).write_text(json.dumps({'schema': "
                 "'aline-m1-trace-recording/v1','pid':9999,'epoch_ns':0}))\n",
                 str(wrong_pid_marker)]
    try:
        _release_gate(failed_gate, 4242, wrong_pid, marker_timeout=2, settle_ns=0)
    except ValueError:
        pass
    else:
        raise AssertionError('错误 trace pid 不得释放 gate')
    check('错误 trace pid 不得释放 gate', not (failed_gate / 'gate').exists())
    # 进程组已消失时 macOS 对 killpg 返回 EPERM；清理失败不得掩盖原始错误。
    eperm_dir = output / 'gate-killpg-eperm'
    eperm_dir.mkdir()
    eperm_marker = eperm_dir / 'trace.recording.json'
    # helper 写完 marker 后保持存活，确保进入清理分支。
    eperm_trace = [sys.executable, '-c',
                    "import json, sys, time\nfrom pathlib import Path\n"
                    "Path(sys.argv[1]).write_text(json.dumps({'schema': "
                    "'aline-m1-trace-recording/v1','pid':9999,'epoch_ns':0}))\n"
                    "time.sleep(30)\n",
                    str(eperm_marker)]
    real_killpg = os.killpg
    killpg_calls = []

    def deny_killpg(pid, sig):
        """模拟 macOS：进程组已消失时 killpg 返回 EPERM。

        先真的终止进程组以复现“组已消失”的前提，再抛 EPERM；
        否则 wait() 会挂住，测试既慢又测不到清理分支。
        """
        killpg_calls.append(pid)
        real_killpg(pid, sig)
        raise PermissionError(1, 'Operation not permitted')

    os.killpg = deny_killpg
    try:
        _release_gate(eperm_dir, 4242, eperm_trace, marker_timeout=2, settle_ns=0)
    except ValueError as error:
        check('killpg 权限错误不掩盖原始 marker 错误',
              'marker' in str(error))
    except BaseException as error:
        raise AssertionError('killpg 权限错误掩盖了原始 marker 错误: '
                             + type(error).__name__ + ': ' + str(error))
    else:
        raise AssertionError('killpg 权限错误时不得释放 gate')
    finally:
        os.killpg = real_killpg
    check('killpg 权限错误时仍尝试清理进程组', len(killpg_calls) == 1)
    check('killpg 权限错误时不释放 gate', not (eperm_dir / 'gate').exists())
    # 单进程 kill 同理：权限错误不得让清理失败掩盖原始错误。
    real_kill = os.kill

    def deny_kill(pid, sig):
        raise PermissionError(1, 'Operation not permitted')

    os.kill = deny_kill
    try:
        _kill_pid(4242)
    except BaseException as error:
        raise AssertionError('kill 权限错误未在清理阶段收敛: '
                             + type(error).__name__ + ': ' + str(error))
    else:
        check('kill 权限错误在清理阶段收敛', True)
    finally:
        os.kill = real_kill
    _kill_pid(0)
    check('kill 忽略非法 pid', True)
    # 进程组清理共用辅助：权限错误必须在此收敛。
    group_dir = output / 'group-eperm'
    group_dir.mkdir()
    group_marker = group_dir / 'trace.recording.json'
    group_trace = [sys.executable, '-c',
                   "import json, sys, time\nfrom pathlib import Path\n"
                   "Path(sys.argv[1]).write_text(json.dumps({'schema': "
                   "'aline-m1-trace-recording/v1','pid':9999,'epoch_ns':0}))\n"
                   "time.sleep(30)\n",
                   str(group_marker)]
    group_process = subprocess.Popen(group_trace, cwd=ROOT, start_new_session=True)
    group_calls = []

    def deny_group_killpg(pid, sig):
        group_calls.append(pid)
        real_killpg(pid, sig)
        raise PermissionError(1, 'Operation not permitted')

    os.killpg = deny_group_killpg
    try:
        _kill_process_group(group_process)
    except BaseException as error:
        raise AssertionError('进程组清理未收敛权限错误: '
                             + type(error).__name__ + ': ' + str(error))
    else:
        check('进程组清理收敛权限错误', True)
    finally:
        os.killpg = real_killpg
        if group_process.poll() is None:
            group_process.kill()
            group_process.wait()
    check('进程组清理确实走到 killpg', len(group_calls) == 1)
    check('进程组清理后子进程已回收', group_process.poll() is not None)
    ready_dir = output / 'gate-ready-format'
    ready_dir.mkdir()
    ready_file = ready_dir / 'gate.ready'
    ready_file.write_text('', encoding='utf-8')
    writer = subprocess.Popen([sys.executable, '-c',
                               "import sys, time\nfrom pathlib import Path\n"
                               "time.sleep(0.05)\n"
                               "Path(sys.argv[1]).write_text('pid=4242\\n')\n",
                               str(ready_file)])
    try:
        match = _wait_text(ready_file, 2, 'gate.ready', r'pid=(\d+)')
        writer.wait()
        check('gate.ready 空文件等待到完整内容', match.group(1) == '4242')
    finally:
        if writer.poll() is None:
            writer.kill()
            writer.wait()
    empty_file = ready_dir / 'empty'
    empty_file.write_text('', encoding='utf-8')
    try:
        _wait_text(empty_file, 0.2, 'empty', r'pid=(\d+)')
    except ValueError:
        pass
    else:
        raise AssertionError('空 gate.ready 不得视为就绪')
    check('空 gate.ready 超时失败关闭', True)
    display_ok = {'schema': DISPLAY_STATE_SCHEMA, 'screen_locked': False,
                  'display_asleep': False, 'display_active': True,
                  'on_console': True, 'login_done': True}
    _require_display_ready(dict(display_ok))
    check('清醒解锁会话通过前置检查', True)
    for field, value in (('screen_locked', True), ('display_asleep', True),
                         ('display_active', False), ('on_console', False),
                         ('login_done', False), ('screen_locked', None)):
        broken = dict(display_ok)
        broken[field] = value
        try:
            _require_display_ready(broken)
        except EnvironmentUnavailable:
            pass
        else:
            raise AssertionError('锁屏/睡眠/非控制台会话不得通过: ' + field)
    check('锁屏/睡眠/非控制台会话失败关闭', True)
    activation_ok = {'schema': ACTIVATION_SCHEMA, 'found': True, 'frontmost': True,
                     'layer0_windows': 1, 'has_nonzero_bounds': True}
    _require_activation(dict(activation_ok))
    check('前台在屏窗口通过激活检查', True)
    for field, value in (('found', False), ('frontmost', False),
                         ('layer0_windows', 0), ('has_nonzero_bounds', False),
                         ('frontmost', None)):
        broken = dict(activation_ok)
        broken[field] = value
        try:
            _require_activation(broken)
        except ValueError:
            pass
        else:
            raise AssertionError('非前台或无在屏窗口不得通过: ' + field)
    check('非前台或无在屏窗口失败关闭', True)
    write_json(output / 'summary.json', {'passed': all(item['passed'] for item in checks),
                                        'synthetic_contract_test_only': True, 'checks': checks,
                                        'reject_cases': sorted(item['name'] for item in checks
                                                               if item['kind'] == 'reject')})
    print('self-test: ' + str(len(checks)) + ' passed')
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--samples', type=int, default=MIN_SAMPLES)
    parser.add_argument('--condition', choices=CONDITIONS)
    parser.add_argument('--refill-plan', type=Path,
                        help='只补采计划列出的 (条件, 冷热, 新 sample_id)')
    parser.add_argument('--run-id',
                        help='覆盖 run_id（补采需与主矩阵共享同一运行身份）')
    parser.add_argument('--timeout', type=float, default=90)
    parser.add_argument('--keep-display-awake', action='store_true',
                        help='矩阵期间用 caffeinate 阻止显示器空闲睡眠')
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
