#!/usr/bin/env python3
"""验证输出路径的正反例；夹具与自测结果也只写 logs/validation。"""

import argparse
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from evidence_paths import write_json, write_text
from validation_output import ROOT, output_directory

OUTPUT = None
FIXTURES = None

# M1 7.1 external-trace 证据链的违规覆盖矩阵。
# 每项都要求同一脚本里存在一个“必须拒绝”的反例和一个“必须接受”的正例，
# 且两者都在该脚本 --self-test 的结构化记录里被标成对应 kind 并通过。
# 只声明要求，不代替脚本自身的断言：映射不到实际记录即判失败。
EVIDENCE_CASE_MATRIX = (
    {
        'id': 'pseudo-present',
        'title': '伪 present：有 present request 但没有上屏 display 证据',
        'script': 'm1_trace_metrics.py',
        'reject': 'pseudo present without display evidence',
        'accept': '显示链与最长忙碌段基线',
    },
    {
        'id': 'missing-present-request',
        'title': '缺 present request',
        'script': 'm1_trace_metrics.py',
        'reject': 'candidate request missing',
        'accept': '显示链与最长忙碌段基线',
    },
    {
        'id': 'display-before-commit',
        'title': 'display 早于 commit',
        'script': 'm1_trace_metrics.py',
        'reject': 'display before commit',
        'accept': 'secondary composite frame',
    },
    {
        'id': 'display-unavailable',
        'title': '锁屏或显示器睡眠时 fail-closed，不产生样本',
        'script': 'sample_m1_performance.py',
        'reject': '锁屏/睡眠/非控制台会话失败关闭',
        'accept': '清醒解锁会话通过前置检查',
    },
    {
        'id': 'identity-drift-run-id',
        'title': 'run_id 身份漂移',
        'script': 'run_m1_readonly.py',
        'reject': '跨 run_id 被拒绝',
        'accept': '20 个成功样本通过并汇总指标',
    },
    {
        'id': 'identity-drift-app-sha256',
        'title': 'app_sha256 身份漂移',
        'script': 'run_m1_readonly.py',
        'reject': '逐次启动应用哈希漂移被拒绝',
        'accept': '逐次启动收据绑定通过',
    },
    {
        'id': 'identity-drift-pid',
        'title': 'pid 身份漂移',
        'script': 'run_m1_readonly.py',
        'reject': '重复启动 pid 被拒绝',
        'accept': '逐次启动收据绑定通过',
    },
    {
        'id': 'rederive-cross-source-identity',
        'title': '重推跨来源身份不一致',
        'script': 'm1_rederive_performance.py',
        'reject': '跨来源 run_id 不一致被拒绝',
        'accept': '跨来源补采按顺序合并',
    },
    {
        'id': 'refill-duplicate-sample-id',
        'title': '补采编号重复',
        'script': 'm1_rederive_performance.py',
        'reject': '跨来源重复采样编号被拒绝',
        'accept': '补采计划为不足组分配连续新编号',
    },
    {
        'id': 'output-directory-escape',
        'title': '输出目录越界（含相对路径与符号链接逃逸）',
        'script': None,
        'reject': 'test_cli_rejects_bad_output_before_running_checks',
        'accept': 'test_relative_explicit_path_is_repository_relative',
    },
)

# 守卫自身证明不了的事项，与各脚本自报的 blind_spots 一起写入 case-matrix.md。
GUARD_BLIND_SPOTS = (
    '覆盖矩阵只证明声明的反例/正例存在、被标为对应 kind 且通过；不证明用例本身足以发现真实设备故障',
    '各脚本自测用的是合成夹具，不等于真实 xctrace 录制或真实原生窗口',
    '输出目录越界只覆盖 CLI 边界与符号链接逃逸，不对抗并发替换路径的进程',
    '本守卫只在当前 macOS 主机执行，不外推到 Windows/Linux，也不改任何验收口径',
    'run_m1_readonly.py 与 sample_m1_performance.py 的旧自测不逐条自报 blind_spots，'
    '其盲区由用例名与本文件的上列条目共同限定',
)


def case_rows(summary):
    """把不同脚本的自测记录统一成 {用例名: {kind, passed}}。

    三种既有形态：cases[].name（新 CLI）、cases[].case（汇总器）、
    checks[].name（采样器）；缺 kind 时按 accept 处理，由矩阵断言暴露差异。
    """
    rows = summary.get('cases')
    if rows is None:
        rows = summary.get('checks')
    result = {}
    for row in rows or []:
        name = row.get('name') or row.get('case')
        if not name:
            continue
        result[name] = {'kind': row.get('kind', 'accept'),
                        'passed': bool(row.get('passed'))}
    return result


def load_self_test_summary(directory):
    """读取一次自测的结构化记录；多套件脚本按子目录合并。

    run_m1_readonly.py 不带 --suite 时把三套件分别写到 <dir>/<suite>/self-test.json，
    没有顶层 summary.json，因此这里按子目录合并，避免把“没有顶层文件”误判为通过。
    """
    path = directory / 'summary.json'
    if path.is_file():
        return json.loads(path.read_text())
    merged = []
    passed = True
    found = False
    for child in sorted(item for item in directory.iterdir() if item.is_dir()):
        candidate = child / 'self-test.json'
        if not candidate.is_file():
            continue
        found = True
        payload = json.loads(candidate.read_text())
        merged.extend(payload.get('cases', []))
        passed = passed and bool(payload.get('passed'))
    if not found:
        return {}
    return {'passed': passed, 'cases': merged}


def verify_case_matrix(matrix, loaded):
    """按矩阵核对已加载的自测记录；返回违规列表，空列表表示全部成立。

    loaded 形如 {脚本: {'exit_code': int, 'summary': dict}}；桩数据可直接构造，
    因此“矩阵会拒绝伪造记录”本身可被自测证明。
    """
    violations = []
    for entry in matrix:
        script = entry.get('script')
        if script is None:
            continue
        record = loaded.get(script)
        if record is None:
            violations.append({'id': entry['id'], 'reason': '未运行该脚本的自测'})
            continue
        if record.get('exit_code') != 0:
            violations.append({'id': entry['id'],
                               'reason': '自测退出码非 0：' + str(record.get('exit_code'))})
            continue
        summary = record.get('summary') or {}
        if not summary:
            violations.append({'id': entry['id'], 'reason': '缺少结构化自测记录'})
            continue
        if not summary.get('passed'):
            violations.append({'id': entry['id'], 'reason': '自测记录 passed 不为真'})
            continue
        rows = case_rows(summary)
        for kind, label in (('reject', '反例'), ('accept', '正例')):
            name = entry[kind]
            row = rows.get(name)
            if row is None:
                violations.append({'id': entry['id'],
                                   'reason': '缺少' + label + '用例：' + name})
                continue
            if row['kind'] != kind:
                violations.append({'id': entry['id'],
                                   'reason': '用例 kind 不符：' + name +
                                             '（期望 ' + kind + '，实际 ' + row['kind'] + '）'})
            if not row['passed']:
                violations.append({'id': entry['id'], 'reason': '用例未通过：' + name})
    return violations



class OutputDirectoryTests(unittest.TestCase):
    """使用隔离小仓库验证落点与重复运行，不进行 Rust 编译。"""

    def setUp(self):
        self.root = FIXTURES / self._testMethodName
        self.root.mkdir(parents=True)

    def assert_rejected_without_write(self, requested, unexpected):
        with self.assertRaisesRegex(ValueError, 'logs/validation'):
            output_directory('check', requested, self.root)
        self.assertFalse(unexpected.exists())
        self.assertFalse((self.root / 'logs').exists())

    def test_default_runs_are_unique_and_preserve_previous_output(self):
        first = output_directory('example', root=self.root)
        (first / 'summary.json').write_text('first')
        second = output_directory('example', root=self.root)
        self.assertNotEqual(first, second)
        self.assertEqual(first.parent, self.root / 'logs/validation/example')
        self.assertTrue(first.name.startswith('run-'))
        self.assertEqual((first / 'summary.json').read_text(), 'first')

    def test_relative_explicit_path_is_repository_relative(self):
        output = output_directory('example', 'logs/validation/chosen', self.root)
        self.assertEqual(output, self.root / 'logs/validation/chosen')
        self.assertTrue(output.is_dir())

    def test_nested_workspace_outputs_stay_in_same_run(self):
        run = output_directory('workspace', root=self.root)
        child = output_directory('engineering', run / 'dependency-graphs', self.root)
        self.assertEqual(child.parent, run)
        self.assertEqual(output_directory('engineering', child, self.root), child)

    def test_docs_path_is_rejected_before_writing(self):
        self.assert_rejected_without_write('docs/report', self.root / 'docs')

    def test_absolute_external_path_is_rejected_before_writing(self):
        outside = OUTPUT / 'must-not-create-outside'
        self.assert_rejected_without_write(outside, outside)

    def test_parent_traversal_is_rejected_before_writing(self):
        self.assert_rejected_without_write('logs/validation/../../docs/report', self.root / 'docs')

    def test_symlink_escape_is_rejected_before_writing(self):
        base = self.root / 'logs/validation'
        base.mkdir(parents=True)
        outside = self.root / 'outside'
        outside.mkdir()
        (base / 'escape').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'logs/validation'):
            output_directory('example', base / 'escape/new', self.root)
        self.assertFalse((outside / 'new').exists())

    def test_symlinked_logs_root_is_rejected_before_writing(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (self.root / 'logs').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'logs/validation'):
            output_directory('example', root=self.root)
        self.assertFalse((outside / 'validation').exists())

    def test_cli_rejects_bad_output_before_running_checks(self):
        bad = ROOT / 'docs' / ('validation-output-must-not-create-' + OUTPUT.name)
        for script in ('check_workspace.py', 'check_engineering.py', 'check_lints.py',
                       'check_safety.py', 'check_public_api.py', 'run_m1_readonly.py',
                       'collect_m1_readonly.py', 'check_ui_tokens.py', 'check_ui_catalog.py',
                       'check_m1_cjk_typography.py', 'probe_m1_visual_evidence.py',
                       'sample_m1_performance.py', 'capture_m1_readonly_actions.py',
                       'run_w1_offscreen.py', 'm1_trace_metrics.py',
                       'm1_trace_record.py', 'm1_rederive_performance.py',
                       'run_window_failure_probe.py', 'run_signal_probe.py'):
            modes = ([], ['--self-test'])
            for mode in modes:
                with self.subTest(script=script, mode=mode):
                    result = subprocess.run([sys.executable, str(ROOT / 'scripts' / script),
                                             '--output-dir', str(bad)] + mode, cwd=ROOT,
                                            env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                                            text=True, capture_output=True)
                    label = script + ('-self-test' if mode else '')
                    write_text(OUTPUT / (label + '.stdout.log'), result.stdout)
                    write_text(OUTPUT / (label + '.stderr.log'), result.stderr)
                    self.assertEqual(result.returncode, 1)
                    self.assertIn('logs/validation', result.stdout + result.stderr)
                    self.assertFalse(bad.exists())

    def test_performance_worker_rejects_bad_output_before_loading_configuration(self):
        bad = ROOT / 'docs' / ('validation-worker-must-not-create-' + OUTPUT.name)
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/sample_m1_performance.py'),
                                 '--worker', str(bad)], cwd=ROOT,
                                env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                                text=True, capture_output=True)
        write_text(OUTPUT / 'sample_m1_performance-worker.stdout.log', result.stdout)
        write_text(OUTPUT / 'sample_m1_performance-worker.stderr.log', result.stderr)
        self.assertEqual(result.returncode, 1)
        self.assertIn('logs/validation', result.stdout + result.stderr)
        self.assertFalse(bad.exists())

    def test_lint_cli_accepts_workspace_nested_output(self):
        run = output_directory('workspace', OUTPUT / 'workspace-nested')
        output = run / 'lint'
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/check_lints.py'),
                                 '--output-dir', str(output)], cwd=ROOT,
                                env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                                text=True, capture_output=True)
        write_text(OUTPUT / 'nested-lint.stdout.log', result.stdout)
        write_text(OUTPUT / 'nested-lint.stderr.log', result.stderr)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((output / 'summary.json').is_file())

    def test_safety_self_test_can_reuse_explicit_output(self):
        output = OUTPUT / 'reused-safety-output'
        work_roots = []
        for attempt in (1, 2):
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/check_safety.py'),
                                     '--self-test', '--output-dir', str(output)], cwd=ROOT,
                                    env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                                    text=True, capture_output=True)
            label = 'reused-safety-' + str(attempt)
            write_text(OUTPUT / (label + '.stdout.log'), result.stdout)
            write_text(OUTPUT / (label + '.stderr.log'), result.stderr)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            summary = json.loads((output / 'summary.json').read_text())
            self.assertTrue(summary['passed'])
            work_roots.append(summary['fixture_root'])
        self.assertNotEqual(work_roots[0], work_roots[1])


    def test_evidence_case_matrix_accepts_and_rejects(self):
        """逐项核对违规覆盖矩阵：每项都要有真实的反例与正例记录。"""
        loaded = {}
        records = []
        for script in sorted({entry['script'] for entry in EVIDENCE_CASE_MATRIX
                              if entry['script']}):
            directory = OUTPUT / 'self-test' / Path(script).stem
            # 采样器等自测要求全新目录（夹具 create_new），复用前必须清空自己的临时落点。
            shutil.rmtree(directory, ignore_errors=True)
            result = subprocess.run([sys.executable, str(ROOT / 'scripts' / script),
                                     '--self-test', '--output-dir', str(directory)],
                                    cwd=ROOT,
                                    env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                                    text=True, capture_output=True)
            label = 'matrix-' + Path(script).stem
            write_text(OUTPUT / (label + '.stdout.log'), result.stdout)
            write_text(OUTPUT / (label + '.stderr.log'), result.stderr)
            summary = load_self_test_summary(directory)
            loaded[script] = {'exit_code': result.returncode, 'summary': summary}
            self.assertEqual(result.returncode, 0,
                             script + ' 自测未通过：' + result.stdout + result.stderr)
            self.assertTrue(summary.get('passed'), script + ' 结构化记录未通过')

        violations = verify_case_matrix(EVIDENCE_CASE_MATRIX, loaded)
        self.assertEqual(violations, [],
                         '覆盖矩阵不成立：' + json.dumps(violations, ensure_ascii=False))

        blind = list(GUARD_BLIND_SPOTS)
        for entry in EVIDENCE_CASE_MATRIX:
            script = entry['script']
            if script is None:
                continue
            rows = case_rows(loaded[script]['summary'])
            for kind in ('reject', 'accept'):
                records.append({'id': entry['id'], 'title': entry['title'],
                                'script': script, 'case': entry[kind], 'kind': kind,
                                'passed': rows[entry[kind]]['passed']})
            for item in loaded[script]['summary'].get('blind_spots', []):
                line = script + '：' + item
                if line not in blind:
                    blind.append(line)

        # 输出目录越界这一项由本守卫自己证明：反例必须拒绝且不建目录，正例必须接受。
        escape_root = FIXTURES / 'matrix-escape'
        escape_root.mkdir(parents=True)
        bad = ROOT / 'docs' / ('matrix-escape-must-not-create-' + OUTPUT.name)
        for script in ('m1_trace_metrics.py', 'm1_trace_record.py',
                       'm1_rederive_performance.py', 'run_window_failure_probe.py',
                       'run_signal_probe.py'):
            for mode in ([], ['--self-test']):
                result = subprocess.run([sys.executable, str(ROOT / 'scripts' / script),
                                         '--output-dir', str(bad)] + mode, cwd=ROOT,
                                        env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode, 1, script + str(mode))
                self.assertIn('logs/validation', result.stdout + result.stderr)
                self.assertFalse(bad.exists())
                records.append({'id': 'output-directory-escape', 'script': script,
                                'case': script + (' --self-test' if mode else ''),
                                'kind': 'reject', 'passed': True})
        accepted = output_directory('example', 'logs/validation/chosen', escape_root)
        self.assertEqual(accepted, escape_root / 'logs/validation/chosen')
        self.assertTrue(accepted.is_dir())
        records.append({'id': 'output-directory-escape', 'script': 'validation_output.py',
                        'case': 'test_relative_explicit_path_is_repository_relative',
                        'kind': 'accept', 'passed': True})

        write_json(OUTPUT / 'case-matrix.json', {
            'passed': True,
            'violations': [],
            'records': records,
            'blind_spots': blind,
            'self_tests': {script: loaded[script]['exit_code'] for script in loaded},
        })
        lines = ['# M1 7.1 external-trace 违规覆盖矩阵', '',
                 '| 违规项 | 脚本 | 反例用例 | 正例用例 | 结论 |',
                 '| --- | --- | --- | --- | --- |']
        for entry in EVIDENCE_CASE_MATRIX:
            if entry['script'] is None:
                lines.append('| ' + entry['title'] + ' | 本守卫 | ' + entry['reject']
                             + ' | ' + entry['accept'] + ' | 反例拒绝且不建目录，正例接受 |')
            else:
                lines.append('| ' + entry['title'] + ' | ' + entry['script'] + ' | `'
                             + entry['reject'] + '` | `' + entry['accept']
                             + '` | 两者均在结构化记录中通过 |')
        lines.extend(['', '## 盲区', ''])
        lines.extend('- ' + item for item in blind)
        write_text(OUTPUT / 'case-matrix.md', chr(10).join(lines) + chr(10))

    def test_case_matrix_rejects_missing_or_fabricated_rows(self):
        """用桩记录证明覆盖矩阵会拒绝缺项、伪造 kind、未通过与脚本失败。"""
        entry = EVIDENCE_CASE_MATRIX[0]
        script = entry['script']

        def stub():
            return {script: {'exit_code': 0, 'summary': {'passed': True, 'cases': [
                {'name': entry['reject'], 'kind': 'reject', 'passed': True},
                {'name': entry['accept'], 'kind': 'accept', 'passed': True}]}}}

        self.assertEqual(verify_case_matrix((entry,), stub()), [])
        self.assertTrue(verify_case_matrix((entry,), {
            script: {'exit_code': 0, 'summary': {'passed': True, 'cases': [
                {'name': entry['accept'], 'kind': 'accept', 'passed': True}]}}}))
        self.assertTrue(verify_case_matrix((entry,), {
            script: {'exit_code': 0, 'summary': {'passed': True, 'cases': [
                {'name': entry['reject'], 'kind': 'accept', 'passed': True},
                {'name': entry['accept'], 'kind': 'accept', 'passed': True}]}}}))
        self.assertTrue(verify_case_matrix((entry,), {
            script: {'exit_code': 0, 'summary': {'passed': True, 'cases': [
                {'name': entry['reject'], 'kind': 'reject', 'passed': False},
                {'name': entry['accept'], 'kind': 'accept', 'passed': True}]}}}))
        self.assertTrue(verify_case_matrix((entry,), {
            script: {'exit_code': 1, 'summary': {'passed': True, 'cases': []}}}))
        self.assertTrue(verify_case_matrix((entry,), {
            script: {'exit_code': 0, 'summary': {'passed': False, 'cases': []}}}))
        self.assertTrue(verify_case_matrix((entry,), {}))
        stub_dir = FIXTURES / 'matrix-stub'
        stub_dir.mkdir(parents=True)
        write_json(stub_dir / 'fabricated.json', {
            'note': '桩记录只用于证明矩阵会拒绝，不是任何证据',
            'cases': [{'name': entry['reject'], 'kind': 'accept', 'passed': True}]})
        self.assertTrue(verify_case_matrix((entry,), {
            script: {'exit_code': 0,
                     'summary': json.loads((stub_dir / 'fabricated.json').read_text())}}))


def main():
    global OUTPUT, FIXTURES
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path)
    args = parser.parse_args()
    try:
        OUTPUT = output_directory('output-paths', args.output_dir)
        FIXTURES = Path(tempfile.mkdtemp(prefix='work-', dir=OUTPUT))
    except (OSError, ValueError) as error:
        print('输出路径自测拒绝：' + str(error), file=sys.stderr)
        return 1
    stream = io.StringIO()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(OutputDirectoryTests)
    result = unittest.TextTestRunner(stream=stream, verbosity=2).run(suite)
    write_text(OUTPUT / 'tests.stdout.log', stream.getvalue())
    write_json(OUTPUT / 'summary.json', {'passed': result.wasSuccessful(), 'tests': result.testsRun,
                                       'failures': len(result.failures), 'errors': len(result.errors),
                                       'fixture_root': str(FIXTURES)})
    print(stream.getvalue(), end='')
    print('输出：' + str(OUTPUT.relative_to(ROOT)))
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    sys.exit(main())
