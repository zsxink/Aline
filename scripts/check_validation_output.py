#!/usr/bin/env python3
"""验证输出路径的正反例；夹具与自测结果也只写 logs/validation。"""

import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from evidence_paths import write_json, write_text
from validation_output import ROOT, output_directory

OUTPUT = None
FIXTURES = None


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
                       'check_safety.py', 'check_public_api.py'):
            modes = ([],) if script == 'check_workspace.py' else ([], ['--self-test'])
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
