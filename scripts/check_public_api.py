#!/usr/bin/env python3
"""实际编译公共边界正例和最小类型违规反例，不将任意编译失败当通过。"""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def execute(argv, directory, output, label, env):
    """保存真实 Cargo 命令、结果和环境，不安装工具或下载依赖。"""
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    result = subprocess.run(argv, cwd=directory, env=env, text=True, capture_output=True)
    (output / (label + '.stdout.log')).write_text(result.stdout)
    (output / (label + '.stderr.log')).write_text(result.stderr)
    record = {'argv': argv, 'cwd': str(directory), 'started_utc': started,
              'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'exit_code': result.returncode,
              'env_overrides': {key: env[key] for key in ('CARGO_HOME', 'RUSTUP_TOOLCHAIN', 'CARGO_NET_OFFLINE', 'CARGO_TARGET_DIR')}}
    (output / (label + '.command.json')).write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n')
    return result


def run(output):
    output.mkdir(parents=True, exist_ok=True)
    isolation = json.loads((ROOT / 'docs/validation/m0-engine-foundation/admission-2026-10-30/isolation.json').read_text())
    env = dict(os.environ, CARGO_HOME=os.environ.get('CARGO_HOME') or isolation['cargo_home'],
               RUSTUP_TOOLCHAIN='1.98.1', CARGO_NET_OFFLINE='true', CARGO_TARGET_DIR=str(output / 'cargo-target'))
    setup = '''
let snapshot = aline_buffer::TextSnapshot::new(String::from("甲😀"), aline_buffer::TextBudget::new(64))?;
let projection = aline_markdown::Projection::identity(snapshot.clone(), snapshot.src_range(0..7)?, aline_markdown::MappingBudget::new(64, 16))?;
let storage = aline_source::StorageSnapshot::new(vec![b'x'], aline_source::DiskVersion::initial(), aline_source::StorageBudget::new(8))?;
'''
    cases = [
        ('positive', '', '''
let source: aline_buffer::SrcOffset = projection.source_at(&projection.drawn_offset(3)?, aline_buffer::Affinity::Before)?.into_value();
let _drawn: aline_markdown::DrawnOffset = projection.drawn_at(&source, aline_buffer::Affinity::After)?.into_value();
let _utf16: aline_markdown::Utf16Offset = projection.utf16_from_source(&source, aline_buffer::Affinity::After)?.into_value();
let _stored: aline_source::StorageRange = storage.range(0..1)?;
let tuples: Vec<(std::ops::Range<usize>, String)> = vec![(0..0, String::from("!"))];
let batch = aline_buffer::CheckedBatch::new(&snapshot, tuples, aline_buffer::PatchBudget::new(4, 64, 64))?;
let _borrowed: &[(std::ops::Range<usize>, String)] = batch.patches();
''', 0, []),
        ('drawn-is-not-source', 'fn need(_: aline_buffer::SrcOffset) {}', 'need(projection.drawn_offset(0)?);', 101, ['E0308', 'SrcOffset', 'DrawnOffset']),
        ('drawn-is-not-utf16', 'fn need(_: aline_markdown::Utf16Offset) {}', 'need(projection.drawn_offset(0)?);', 101, ['E0308', 'Utf16Offset', 'DrawnOffset']),
        ('source-is-not-storage', 'fn need(_: aline_source::StorageRange) {}', 'need(snapshot.src_range(0..0)?);', 101, ['E0308', 'StorageRange', 'SrcRange']),
        ('private-source-fields', '', 'let _forged = aline_buffer::SrcOffset { byte: 0, snapshot: snapshot.id().clone() };', 101, ['E0451', 'private']),
        ('private-snapshot-id', '', 'let _forged = aline_buffer::SnapshotId(std::sync::Arc::new(()));', 101, ['E0423', 'private']),
        ('private-projection-id', '', 'let _forged = aline_markdown::ProjectionId(std::sync::Arc::new(()));', 101, ['E0423', 'private']),
        ('tuple-view-is-immutable', '', 'let batch = aline_buffer::CheckedBatch::new(&snapshot, vec![(0..0, String::new())], aline_buffer::PatchBudget::new(4, 64, 64))?; batch.patches()[0].1.push_str("!");', 101, ['E0596', 'cannot borrow']),
    ]
    records = []
    for name, helper, body, expected, needles in cases:
        fixture = output / 'fixtures' / name
        (fixture / 'src').mkdir(parents=True, exist_ok=True)
        dependencies = '\n'.join(key + ' = { path = ' + json.dumps(str(ROOT / 'crates' / key)) + ' }'
                                 for key in ('aline-buffer', 'aline-markdown', 'aline-source'))
        (fixture / 'Cargo.toml').write_text('[package]\nname="aline-m0-api-' + name + '"\nversion="0.0.0"\nedition="2024"\npublish=false\n[workspace]\n[dependencies]\n' + dependencies + '\n[lints.rust]\nunsafe_code="forbid"\n')
        # 所有故意违规仅在隔离证据中，不进入正式 crate。
        (fixture / 'src/main.rs').write_text('//! 公共边界隔离消费夹具。\n' + helper + '\nfn main() -> Result<(), Box<dyn std::error::Error>> {\n' + setup + body + '\nOk(())\n}\n')
        generated = execute(['cargo', 'generate-lockfile', '--manifest-path', str(fixture / 'Cargo.toml'), '--offline'], fixture, output, name + '-lock', env)
        if generated.returncode != 0:
            records.append({'name': name, 'passed': False, 'reason': '隔离 lock 生成失败，未将其算作编译反例通过'})
            continue
        result = execute(['cargo', 'check', '--manifest-path', str(fixture / 'Cargo.toml'), '--locked', '--offline'], fixture, output, name + '-check', env)
        actual = result.stdout + result.stderr
        passed = result.returncode == expected and all(needle in actual for needle in needles)
        records.append({'name': name, 'expected_exit_code': expected, 'actual_exit_code': result.returncode,
                        'required_diagnostics': needles, 'passed': passed,
                        'independent_expectation': 'Rust newtype/私有字段的语言边界与任务 3.2，非任意非零即通过'})
        print(name, '符合预期' if passed else '失败', flush=True)
    summary = {'passed': all(record['passed'] for record in records), 'cases': records,
               'scope': '当前 macOS 工具链真实编译，未运行非法夹具，不声明其他平台或完整引擎业务'}
    (output / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
    return 0 if summary['passed'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true', help='编译正例及全部最小违规反例（默认同样执行）')
    parser.add_argument('--output-dir', type=Path)
    args = parser.parse_args()
    try:
        return run(args.output_dir.resolve() if args.output_dir else Path(tempfile.mkdtemp(prefix='aline-public-api-')))
    except (OSError, ValueError, KeyError) as error:
        print('公共边界检查拒绝：' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
