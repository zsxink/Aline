#!/usr/bin/env python3
"""只读核对历史原型副本与本地资源，不执行原型或生成器。"""
import hashlib
import json
import re
import threading
import xml.etree.ElementTree as ET
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit
from urllib.request import urlopen

BASE = Path(__file__).resolve().parents[1]
REPO = BASE.parents[1]
LEGACY = BASE / 'legacy'
manifest = json.loads((BASE / 'inventory.json').read_text())
errors = []
checked = []
missing_sources = []
updates = {}
shared_icon = REPO / 'docs/assets/app-icon/aline.png'
update_paths = {'docs/assets/aline.png', 'docs/prototype/legacy/assets/aline.png',
                'docs/prototype/legacy/index.html'}
records = manifest.get('authorized_updates', [])
if records and (len(records) != len(update_paths) or
                {record['path'] for record in records} != update_paths):
    errors.append('图标授权变更必须恰好覆盖两份旧图标和原型入口')
original_hashes = {entry['target']: entry['sha256'] for entry in manifest['entries'] if entry['target']}
for record in records:
    name = record['path']
    if name not in update_paths or record['old_sha256'] != original_hashes.get(name):
        errors.append('图标授权变更原始身份不符：' + name)
        continue
    if (record['replacement'] != 'docs/assets/app-icon/aline.png' or not shared_icon.is_file() or
            hashlib.sha256(shared_icon.read_bytes()).hexdigest() != record['replacement_sha256']):
        errors.append('新共享图标缺失或身份不符：' + name)
        continue
    target = REPO / name
    if name.endswith('.png'):
        valid = record['action'] == 'delete' and record['new_sha256'] is None and not target.exists() and not target.is_symlink()
    else:
        data = target.read_bytes() if target.is_file() else b''
        new_ref, old_ref = b'../../assets/app-icon/aline.png', b'./assets/aline.png'
        valid = (record['action'] == 'replace_icon_reference' and data.count(new_ref) == 3 and
                 hashlib.sha256(data).hexdigest() == record['new_sha256'] and
                 hashlib.sha256(data.replace(new_ref, old_ref)).hexdigest() == record['old_sha256'])
    if valid:
        updates[name] = record
    else:
        errors.append('图标授权变更当前状态不符：' + name)

for entry in manifest['entries']:
    source = REPO / entry['source']
    if source.is_file():
        if hashlib.sha256(source.read_bytes()).hexdigest() != entry['sha256']:
            errors.append('原件发生变化：' + entry['source'])
    else:
        missing_sources.append(entry['source'])
    if not entry['target']:
        continue
    if entry['target'] in updates:
        checked.append(entry['target'])
        continue
    target = REPO / entry['target']
    if not target.is_file() or hashlib.sha256(target.read_bytes()).hexdigest() != entry['sha256']:
        errors.append('副本缺失或字节不同：' + entry['target'])
    checked.append(entry['target'])
expected = {entry['target'] for entry in manifest['entries']
            if entry['target'] and '/legacy/' in entry['target'] and
            not (entry['target'] in updates and updates[entry['target']]['action'] == 'delete')}
actual = {str(path.relative_to(REPO)) for path in LEGACY.rglob('*') if path.is_file()}
if expected != actual:
    errors.append('历史目录文件集合不同：' + str(sorted(expected ^ actual)))

references = []
external_links = set()
for path in sorted(LEGACY.rglob('*')):
    if path.suffix not in {'.html', '.css', '.js', '.svg'}:
        continue
    text = path.read_text()
    if 'docs_bak' in text:
        errors.append('运行材料仍引用旧目录：' + str(path.relative_to(REPO)))
    if path.suffix == '.svg':
        ET.fromstring(text)
    # HTML 和脚本模板中的静态属性；动态主题路径在下方按索引完整展开。
    for kind, value in re.findall(r'\b(src|href)=[\'"]([^\'"]+)[\'"]', text):
        if '${' in value or value.startswith(('#', 'data:', 'blob:')):
            continue
        url = urlsplit(value)
        if url.scheme or url.netloc:
            external_links.add(value)
            if kind == 'src':
                errors.append('外部启动资源：' + value)
            continue
        target = (path.parent / unquote(url.path)).resolve()
        shared = target == shared_icon and len(updates) == len(update_paths)
        if (not target.is_relative_to(LEGACY) and not shared) or not target.is_file():
            errors.append('静态资源缺失或越界：' + value)
        references.append({'from':str(path.relative_to(REPO)),
                           'to':str(target.relative_to(REPO)) if target.is_relative_to(REPO) else str(target)})
    for value in re.findall(r'url\([\'"]?([^\)\'\"]+)', text):
        if value.startswith(('#', 'data:', 'blob:')) or '${' in value:
            continue
        errors.append('需人工复核 CSS URL：' + value)

index = json.loads((LEGACY / 'themes/index.json').read_text())
themes = []
for entry in index['themes']:
    folder = LEGACY / 'themes' / entry['id']
    theme = json.loads((LEGACY / 'themes' / entry['manifest']).read_text())
    if theme['id'] != entry['id']:
        errors.append('主题身份不匹配：' + entry['id'])
    paths = list(theme['files'].values()) + list(theme['previews'].values())
    for value in paths:
        target = (folder / value).resolve()
        if not target.is_relative_to(folder.resolve()) or not target.is_file():
            errors.append('主题资源缺失或越界：' + str(target))
    for value in theme['files'].values():
        json.loads((folder / value).read_text())
    themes.append({'id':theme['id'], 'schemaVersion':theme['schemaVersion'], 'resources':paths})

class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass

# 从 docs 根提供原型及唯一共享图标，与当前 README 的启动入口一致。
server = ThreadingHTTPServer(('127.0.0.1', 0), partial(QuietHandler, directory=str(REPO / 'docs')))
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
http_checked = 0
try:
    http_files = {path for path in LEGACY.rglob('*') if path.is_file()}
    if len(updates) == len(update_paths):
        http_files.add(shared_icon)
    for path in sorted(http_files):
        if not path.is_file():
            continue
        url = f'http://127.0.0.1:{server.server_port}/' + path.relative_to(REPO / 'docs').as_posix()
        with urlopen(url, timeout=5) as response:
            if response.status != 200 or response.read() != path.read_bytes():
                errors.append('HTTP 内容不符：' + str(path))
            http_checked += 1
finally:
    server.shutdown()
    server.server_close()
    thread.join()

print(json.dumps({'scope':'只读副本/静态资源/本机 HTTP，不执行 JS，不等于主题规范或 V 验收',
    'inventory_entries':len(manifest['entries']), 'legacy_files':len(actual),
    'authorized_icon_updates_checked':len(updates),
    'target_hashes_checked':len(checked), 'missing_originals':missing_sources,
    'static_references':references, 'external_hyperlinks':sorted(external_links),
    'themes':themes, 'http_files_checked':http_checked, 'errors':errors}, ensure_ascii=False, indent=2))
raise SystemExit(bool(errors))
