#!/usr/bin/env python3
"""回收过期验证输出：默认只列清单，--apply 才删除。

只处理 ``logs/validation`` 内的旧 ``run-*``、空目录、空 stdout/stderr 和
Cargo 构建缓存。被 tracked 文档/OpenSpec 引用、当前任务目录与近期运行一律
跳过；这是本地维护脚本，不删除源码、测试夹具或任何被跟踪文件。
"""

import argparse
from datetime import timedelta
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from evidence_paths import display_path, write_json
from validation_output import ROOT, output_directory, require_validation_path

DEFAULT_KEEP_DIRS = ('m2-editable-core', 'qmd-w4', 'qmd-w5')
REFERENCE_PREFIXES = ('AGENTS.md', 'README.md', 'docs/', 'openspec/')
REFERENCE_PATTERN = re.compile(r'logs/validation/[A-Za-z0-9_.@/+%=-]+')
CACHE_NAMES = ('cargo-target', 'target')
LOG_SUFFIXES = ('.stdout.log', '.stderr.log')


def parse_age(value):
    """把 7d / 12h / 30m / 0 解析成时间差；无单位按天。"""
    match = re.fullmatch(r'(\d+)\s*([dhm]?)', str(value).strip())
    if match is None:
        raise ValueError('--older-than 只接受 7d / 12h / 30m / 0 形式')
    amount = int(match.group(1))
    unit = match.group(2) or 'd'
    if unit == 'd':
        return timedelta(days=amount)
    if unit == 'h':
        return timedelta(hours=amount)
    return timedelta(minutes=amount)


def validate_root(requested):
    """CLI 的 --root 只允许仓库根或系统临时目录（供自测隔离使用）。"""
    root = Path(requested).resolve()
    if root == ROOT.resolve():
        return root
    temp_root = Path(tempfile.gettempdir()).resolve()
    try:
        root.relative_to(temp_root)
        return root
    except ValueError as error:
        raise ValueError('--root 仅允许仓库根或系统临时目录') from error


def effective_mtime(path):
    """目录时间取其中文件的最新 mtime，空目录用目录自身。"""
    try:
        latest = path.stat().st_mtime
        if path.is_dir():
            for child in path.rglob('*'):
                try:
                    if child.is_file():
                        latest = max(latest, child.stat().st_mtime)
                except OSError:
                    continue
        return latest
    except OSError:
        return 0.0


def path_size(path):
    """只用于汇总，不把大小作为删除条件。"""
    if path.is_file():
        try:
            return path.stat().st_size
        except OSError:
            return 0
    total = 0
    try:
        for child in path.rglob('*'):
            try:
                if child.is_file():
                    total += child.stat().st_size
            except OSError:
                continue
    except OSError:
        return 0
    return total


def _tracked_files(root):
    try:
        result = subprocess.run(['git', 'ls-files'], cwd=root, capture_output=True,
                                text=True, timeout=10)
        if result.returncode == 0:
            files = [root / line for line in result.stdout.splitlines()
                     if line.startswith(REFERENCE_PREFIXES)]
            if files:
                return files
    except (OSError, subprocess.SubprocessError):
        pass
    files = []
    for relative in REFERENCE_PREFIXES:
        path = root / relative
        if path.is_file():
            files.append(path)
        elif path.is_dir():
            files.extend(child for child in path.rglob('*') if child.is_file())
    return files


def scan_pinned(root):
    """从 tracked 规范/文档中收集长期保留的证据路径。"""
    pinned = set()
    for path in _tracked_files(root):
        try:
            text = path.read_text(encoding='utf-8')
        except (OSError, UnicodeDecodeError):
            continue
        for match in REFERENCE_PATTERN.findall(text):
            reference = Path(match.rstrip('.,;:)`'))
            if len(reference.parts) < 3:
                continue
            if any(part in ('', '.', '..', '...') or part.startswith('<')
                   for part in reference.parts):
                continue
            pinned.add(reference)
    return pinned


def is_pinned(relative, pinned):
    """候选路径与引用互为祖先/后代时都视为受保护。"""
    for reference in pinned:
        if relative == reference or relative.is_relative_to(reference) or reference.is_relative_to(relative):
            return True
    return False


def in_keep_dir(relative, keep_dirs):
    return any(part in keep_dirs for part in relative.parts)


def dedupe(paths):
    """去掉已被更大候选覆盖的路径。"""
    unique = []
    for path in sorted(set(paths), key=str):
        if not any(other != path and path.is_relative_to(other) for other in paths):
            unique.append(path)
    return unique


def collect(base, keep_dirs, pinned, cutoff, keep_latest, drop_empty_logs):
    """按类别收集候选项，返回 (路径, 原因) 列表。"""
    run_dirs = []
    caches = []
    empty_runs = []
    empty_logs = []
    if not base.is_dir():
        return []

    for suite in sorted(path for path in base.iterdir() if path.is_dir()):
        relative_suite = suite.relative_to(base.parent.parent)
        if in_keep_dir(relative_suite, keep_dirs):
            continue
        runs = sorted((path for path in suite.iterdir()
                       if path.is_dir() and path.name.startswith('run-')),
                      key=effective_mtime)
        stale = runs[:-keep_latest] if keep_latest > 0 else runs
        for path in stale:
            relative = path.relative_to(base.parent.parent)
            if is_pinned(relative, pinned) or effective_mtime(path) >= cutoff:
                continue
            run_dirs.append(path)
        for path in runs:
            relative = path.relative_to(base.parent.parent)
            if path in run_dirs or is_pinned(relative, pinned):
                continue
            if effective_mtime(path) >= cutoff:
                continue
            try:
                if not any(path.iterdir()):
                    empty_runs.append(path)
            except OSError:
                continue

    for path in base.rglob('*'):
        relative = path.relative_to(base.parent.parent)
        if in_keep_dir(relative, keep_dirs) or is_pinned(relative, pinned):
            continue
        if effective_mtime(path) >= cutoff:
            continue
        if path.is_dir() and path.name in CACHE_NAMES:
            caches.append(path)
        elif drop_empty_logs and path.is_file() and path.name.endswith(LOG_SUFFIXES):
            try:
                if path.stat().st_size == 0:
                    empty_logs.append(path)
            except OSError:
                continue

    covered = dedupe(run_dirs + caches)
    result = [(path, '旧运行') for path in run_dirs]
    result += [(path, '构建缓存') for path in caches
               if not any(other != path and path.is_relative_to(other) for other in covered)]
    result += [(path, '空运行目录') for path in empty_runs
               if not any(other != path and path.is_relative_to(other) for other in covered)]
    result += [(path, '空日志') for path in empty_logs
               if not any(other != path and path.is_relative_to(other) for other in covered)]
    return sorted(result, key=lambda item: str(item[0]))


def remove_empty_dirs(base, keep_dirs, pinned):
    """删除回收后留下的空目录，不触碰 keep-dir 与引用路径。"""
    if not base.is_dir():
        return 0
    removed = 0
    for path in sorted((p for p in base.rglob('*') if p.is_dir()),
                       key=lambda p: len(p.parts), reverse=True):
        relative = path.relative_to(base.parent.parent)
        if in_keep_dir(relative, keep_dirs) or is_pinned(relative, pinned):
            continue
        try:
            if not any(path.iterdir()):
                path.rmdir()
                removed += 1
        except OSError:
            continue
    return removed


def prune(root, *, apply, keep_latest, older_than, keep_dirs,
          drop_empty_logs=True, pinned=None):
    """执行一次回收；返回可结构化记录的清单，不直接打印。"""
    root = Path(root).resolve()
    base = root / 'logs' / 'validation'
    keep_dirs = frozenset(keep_dirs)
    pinned = scan_pinned(root) if pinned is None else pinned
    cutoff = time.time() - older_than.total_seconds()
    actions = collect(base, keep_dirs, pinned, cutoff, keep_latest, drop_empty_logs)

    removed = []
    errors = []
    if apply:
        for path, reason in actions:
            if not path.exists() and not path.is_symlink():
                continue
            size = path_size(path)
            try:
                if path.is_dir() and not path.is_symlink():
                    shutil.rmtree(path)
                else:
                    path.unlink()
            except OSError as error:
                errors.append({'path': display_path(path, root=ROOT), 'error': str(error)})
                continue
            removed.append({'path': display_path(path, root=ROOT), 'reason': reason, 'bytes': size})
    return {
        'root': '<repo>' if root == ROOT.resolve() else '<tmp>',
        'dry_run': not apply,
        'pinned_references': len(pinned),
        'keep_dirs': sorted(keep_dirs),
        'keep_latest': keep_latest,
        'older_than_seconds': int(older_than.total_seconds()),
        'planned': [{'path': display_path(path, root=ROOT), 'reason': reason}
                    for path, reason in actions],
        'reclaimed_bytes': sum(item['bytes'] for item in removed),
        'removed': removed,
        'empty_dirs_removed': remove_empty_dirs(base, keep_dirs, pinned) if apply else 0,
        'errors': errors,
    }


def _touch(path, age_days, text='x'):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding='utf-8')
    stamp = time.time() - age_days * 86400
    os.utime(path, (stamp, stamp))
    try:
        os.utime(path.parent, (stamp, stamp))
    except OSError:
        pass


def self_test(output):
    """用真实临时树证明保留规则、引用白名单与路径边界有效。"""
    output = output_directory('prune-validation-logs-self-test', output)
    root = Path(tempfile.mkdtemp(prefix='aline-prune-'))
    checks = {}
    try:
        base = root / 'logs' / 'validation'
        _touch(base / 'lint' / 'run-old' / 'summary.json', 10, '{}')
        _touch(base / 'lint' / 'run-mid' / 'summary.json', 5, '{}')
        _touch(base / 'lint' / 'run-new' / 'summary.json', 1, '{}')
        _touch(base / 'lint' / 'run-recent' / 'summary.json', 0, '{}')
        empty_run = base / 'lint' / 'run-empty'
        empty_run.mkdir(parents=True)
        stamp = time.time() - 10 * 86400
        os.utime(empty_run, (stamp, stamp))
        _touch(base / 'lint' / 'run-mid' / 'cargo-target' / 'dep.bin', 9)
        _touch(base / 'lint' / 'run-new' / 'cargo-target' / 'dep.bin', 9)
        _touch(base / 'lint' / 'run-new' / 'x.stdout.log', 9, '')
        for name, days in (('run-old', 10), ('run-mid', 5), ('run-new', 1), ('run-recent', 0)):
            stamp = time.time() - days * 86400
            os.utime(base / 'lint' / name, (stamp, stamp))
        _touch(base / 'safety' / 'run-a' / 'summary.json', 8, '{}')
        _touch(base / 'safety' / 'run-b' / 'summary.json', 7, '{}')
        _touch(base / 'ui-catalog' / 'run-pinned' / 'summary.json', 10, '{}')
        _touch(base / 'm2-editable-core' / 'run-current' / 'summary.json', 10, '{}')
        (root / 'docs').mkdir()
        (root / 'docs' / 'ref.md').write_text(
            '见 logs/validation/ui-catalog/run-pinned/summary.json\n', encoding='utf-8')
        (root / 'docs' / 'outside.txt').write_text('keep', encoding='utf-8')

        common = dict(apply=False, keep_latest=1,
                      older_than=timedelta(days=2), keep_dirs=('m2-editable-core',))
        dry = prune(root, **common)
        checks['dry_run_lists'] = bool(dry['planned']) and not dry['removed']
        checks['dry_run_keeps_files'] = (base / 'lint' / 'run-old').exists()

        applied = prune(root, **dict(common, apply=True))
        checks['old_run_removed'] = not (base / 'lint' / 'run-old').exists()
        checks['stale_run_removed'] = not (base / 'lint' / 'run-mid').exists()
        checks['empty_run_removed'] = not (base / 'lint' / 'run-empty').exists()
        checks['cache_in_kept_run_removed'] = not (
            base / 'lint' / 'run-new' / 'cargo-target').exists()
        checks['empty_log_removed'] = not (base / 'lint' / 'run-new' / 'x.stdout.log').exists()
        checks['recent_run_kept'] = (base / 'lint' / 'run-recent').is_dir()
        checks['age_filtered_run_kept'] = (base / 'lint' / 'run-new').is_dir()
        checks['keep_latest_keeps_newer'] = (base / 'safety' / 'run-b').is_dir()
        checks['keep_latest_removes_older'] = not (base / 'safety' / 'run-a').exists()
        checks['pinned_run_kept'] = (base / 'ui-catalog' / 'run-pinned').is_dir()
        checks['keep_dir_kept'] = (base / 'm2-editable-core' / 'run-current').is_dir()
        checks['outside_logs_untouched'] = (root / 'docs' / 'outside.txt').read_text(
            encoding='utf-8') == 'keep'
        checks['no_errors'] = not applied['errors']

        path_guard = False
        try:
            validate_root(ROOT / 'docs')
        except ValueError:
            path_guard = True
        checks['root_guard_rejects_outside'] = path_guard

        passed = all(checks.values())
        write_json(output / 'summary.json', {'passed': passed, 'checks': checks,
                                             'dry_run': dry, 'applied': applied})
        print('prune 自测：' + ('通过' if passed else '失败'), flush=True)
        return 0 if passed else 1
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='真正删除；默认只列清单')
    parser.add_argument('--keep-latest', type=int, default=1,
                        help='每个通用套件保留的最新运行数，默认 1')
    parser.add_argument('--older-than', default='7d', help='只处理早于该时长的条目，默认 7d')
    parser.add_argument('--keep-dir', default=','.join(DEFAULT_KEEP_DIRS),
                        help='逗号分隔的套件名，整目录跳过')
    parser.add_argument('--keep-empty-logs', action='store_true',
                        help='保留 0 字节的 stdout/stderr 日志')
    parser.add_argument('--root', type=Path, default=ROOT, help='仅自测使用的隔离根')
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--output-dir', type=Path, help='仅 --self-test 使用')
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test(args.output_dir)
        if args.output_dir is not None:
            require_validation_path(args.output_dir)
        if args.keep_latest < 0:
            raise ValueError('--keep-latest 不能为负')
        root = validate_root(args.root)
        keep_dirs = tuple(name for name in str(args.keep_dir).split(',') if name)
        report = prune(root, apply=args.apply, keep_latest=args.keep_latest,
                       older_than=parse_age(args.older_than), keep_dirs=keep_dirs,
                       drop_empty_logs=not args.keep_empty_logs)
        for item in report['planned']:
            print('[%s] %s' % (item['reason'], item['path']))
        print('引用白名单 %d 条；%s %d 项，约 %.1f MB；空目录 %d 个；错误 %d 个。'
              % (report['pinned_references'],
                 '已删除' if args.apply else '待删除',
                 len(report['removed']) if args.apply else len(report['planned']),
                 report['reclaimed_bytes'] / 1024 / 1024,
                 report['empty_dirs_removed'], len(report['errors'])))
        return 1 if report['errors'] else 0
    except (OSError, ValueError) as error:
        print('验证输出回收拒绝：' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
