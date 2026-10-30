#!/usr/bin/env python3
"""开发前终审复查：只写本审计附件，不运行应用或改动历史材料。"""
import ast
import csv
import hashlib
import json
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urldefrag
from urllib.request import Request, urlopen

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]


def write_json(name, value):
    (BASE / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def run_existing(name, relative):
    command = [sys.executable, str(ROOT / relative)]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    data = json.loads(result.stdout)
    # 子进程结束并完成解析后才写附件，避免被检查器读到截断输出。
    write_json(name, data)
    return {'command': command, 'exit_code': result.returncode,
            'errors': data['errors'], 'output': name}


class IdParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()

    def handle_starttag(self, tag, attrs):
        self.ids.update(value for key, value in attrs if key in {'id', 'name'})


def check_remote(url):
    address, fragment = urldefrag(url)
    try:
        with urlopen(Request(address, headers={'User-Agent': 'Aline-document-review/1.0'}), timeout=12) as response:
            body = response.read(4 * 1024 * 1024)
            parser = IdParser()
            parser.feed(body.decode('utf-8', errors='replace'))
            return {'url': url, 'status': response.status, 'final_url': response.url,
                    'fragment_exists': unquote(fragment) in parser.ids if fragment else None}
    except Exception as error:
        return {'url': url, 'error': str(error)}


def main():
    summary = [run_existing('document-check.json', 'docs/maintenance/deletion-readiness/check.py'),
               run_existing('prototype-check.json', 'docs/prototype/tools/verify-reference.py')]
    errors = []
    node = shutil.which('node')
    js = []
    for path in sorted((ROOT / 'docs/prototype/legacy').glob('*.js')):
        if node:
            result = subprocess.run([node, '--check', str(path)], text=True, capture_output=True)
            js.append({'path': str(path.relative_to(ROOT)), 'exit_code': result.returncode,
                       'stderr': result.stderr})
            if result.returncode:
                errors.append('JS语法：' + str(path))
    svg = []
    for path in sorted((ROOT / 'docs').rglob('*.svg')):
        element = ET.parse(path).getroot()
        svg.append({'path': str(path.relative_to(ROOT)), 'viewBox': element.get('viewBox'),
                    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    ast.parse(Path(__file__).read_text())
    prd = (ROOT / 'docs/v1-prd.md').read_text()
    identifiers = set(re.findall(r'\b(?:FILE|SAFE|EDIT|FIND|MD|IMAGE|PLUGIN|EXPORT|THEME|APP|FORMAT|PREVIEW)-\d{2}\b', prd))
    rows = list(csv.DictReader((BASE / 'requirement-trace.csv').open()))
    if len(rows) != len(identifiers) or {row['requirement'] for row in rows} != identifiers:
        errors.append('本轮需求追踪集合/唯一性不符')
    for row in rows:
        if any(not value for value in row.values()):
            errors.append('需求追踪字段空白：' + row['requirement'])
    application_files = [str(path.relative_to(ROOT)) for pattern in ('Cargo.toml', 'Cargo.lock',
                         'rust-toolchain*', '**/*.rs', '.github/workflows/*')
                         for path in ROOT.glob(pattern) if path.is_file() and '.git' not in path.parts]
    if application_files:
        errors.append('出现应用实现/配置文件，须人工核对：' + repr(application_files))
    write_json('additional-checks.json', {'javascript': js, 'node_available': bool(node),
               'python_version': sys.version.split()[0],
               'node_version': subprocess.check_output([node, '--version'], text=True).strip() if node else None,
               'svg_xml': svg, 'requirements': len(rows), 'application_files': application_files,
               'deleted_directories_absent': {name: not (ROOT / name).exists() for name in ('docs_bak', 'reports')},
               'errors': errors, 'limits': 'JS仅语法，SVG仅XML；需求集合检查不替代逐行语义审查；未运行Cargo或V实验。'})
    if '--network' in sys.argv:
        urls = set()
        source_pages = [*sorted((ROOT / 'docs').glob('*.md'))]
        icon_readme = ROOT / 'docs/assets/app-icon/README.md'
        if icon_readme.exists():
            source_pages.append(icon_readme)
        for path in source_pages:
            urls.update(re.findall(r'https?://[^\s)<>`"\]]+', path.read_text()))
        with ThreadPoolExecutor(max_workers=8) as executor:
            results = list(executor.map(check_remote, sorted(urls)))
        write_json('external-links.json', {'scope': '现行顶层文档及新增图标说明外链；历史围栏/原型导航不作为启动依赖',
                   'results': results, 'limits': 'HTTP与HTML锚点不证明内容正确、许可或锁定版本能力。网络错误不自动判为死链。'})
    summary.append({'output': 'additional-checks.json', 'errors': errors})
    write_json('check-summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return bool(errors) or any(item.get('exit_code', 0) for item in summary)


if __name__ == '__main__':
    raise SystemExit(main())
