#!/usr/bin/env python3
"""R11 采集器：逐条执行命令并落 argv/cwd/起止 UTC/exit/原始输出。

只做只读观察，不构建、不联网。每条命令一对 <label>.command.json +
<label>.stdout.log + <label>.stderr.log，落在本脚本同目录的 commands/ 下。
"""

import datetime
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / 'commands'
ROOT = pathlib.Path('/Users/xian/Project/editor/Aline')

ENV_OVERRIDES = {
    'CARGO_NET_OFFLINE': 'true',
    'RUSTUP_TOOLCHAIN': '1.98.1',
}


def utc_now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')


def run(label, argv, cwd=ROOT):
    started = utc_now()
    env = dict(os.environ)
    env.update(ENV_OVERRIDES)
    try:
        proc = subprocess.run(argv, cwd=str(cwd), text=True, capture_output=True, env=env)
        exit_code, stdout, stderr = proc.returncode, proc.stdout, proc.stderr
    except FileNotFoundError as error:
        exit_code, stdout, stderr = None, '', 'FileNotFoundError: ' + str(error)
    finished = utc_now()
    record = {
        'id': 'R11-' + label,
        'argv': argv,
        'cwd': str(cwd),
        'env_overrides': ENV_OVERRIDES,
        'started_utc': started,
        'finished_utc': finished,
        'exit_code': exit_code,
    }
    (OUT / (label + '.command.json')).write_text(
        json.dumps(record, ensure_ascii=False, indent=1) + '\n')
    (OUT / (label + '.stdout.log')).write_text(stdout)
    (OUT / (label + '.stderr.log')).write_text(stderr)
    return record


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    labels = []
    cases = [
        ('toolchain-rustc-Vv', ['rustc', '-Vv']),
        ('toolchain-cargo-V', ['cargo', '-V']),
        ('toolchain-cargo-fmt-version', ['cargo', 'fmt', '--version']),
        ('toolchain-cargo-clippy-version', ['cargo', 'clippy', '--version']),
        ('toolchain-rustup-show', ['rustup', 'show']),
        ('toolchain-rustup-toolchain-list', ['rustup', 'toolchain', 'list']),
        ('toolchain-python3-version', ['python3', '--version']),
        ('device-sw-vers', ['sw_vers']),
        ('device-uname-a', ['uname', '-a']),
        ('device-system-profiler-hardware', ['system_profiler', 'SPHardwareDataType']),
        ('device-system-profiler-displays', ['system_profiler', 'SPDisplaysDataType']),
        ('disk-df-repo', ['df', '-h', str(ROOT)]),
        ('isolation-cargo-home-exists', ['test', '-d',
            '/var/folders/rx/cwp1dm6536789l30qt9hf9rm0000gn/T/aline-m0-admission-k4_p5j6a/cargo-home']),
        ('isolation-cargo-home-ls', ['ls', '-la',
            '/var/folders/rx/cwp1dm6536789l30qt9hf9rm0000gn/T/aline-m0-admission-k4_p5j6a']),
        ('isolation-read-isolation-json', ['cat',
            str(ROOT / 'docs/validation/m0-engine-foundation/admission-2026-10-30/isolation.json')]),
    ]
    # 可选按 label 过滤：只重跑指定命令，避免覆写已打码的 device 日志。
    wanted = set(sys.argv[1:])
    for label, argv in cases:
        if wanted and label not in wanted:
            continue
        record = run(label, argv)
        labels.append(label)
        print(label, 'exit=' + str(record['exit_code']), flush=True)
    print('collected', len(labels), 'commands', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
