#!/usr/bin/env python3
"""独立只读复查；不导入旧审计器，不移动目录，不写输出文件。"""
import ast
import csv
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path
from urllib.parse import unquote, urlsplit

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def authorized_icon_updates(baseline):
    """只接受用户本次授权的两份旧图标删除及三处入口引用替换。"""
    manifest = json.loads((ROOT / 'docs/prototype/inventory.json').read_text())
    records = manifest.get('authorized_updates', [])
    allowed = {'docs/assets/aline.png', 'docs/prototype/legacy/assets/aline.png',
               'docs/prototype/legacy/index.html'}
    original_hashes = {row['path']: row['sha256'] for row in baseline if row['kind'] == 'file'}
    errors, updates = [], {}
    if records and (len(records) != len(allowed) or {row['path'] for row in records} != allowed):
        errors.append('图标授权变更必须恰好覆盖两份旧图标和原型入口')
    for record in records:
        name = record['path']
        if name not in allowed or record['old_sha256'] != original_hashes.get(name):
            errors.append('图标授权变更与审计基线身份不符：' + name)
            continue
        replacement = ROOT / record['replacement']
        if (record['replacement'] != 'docs/assets/app-icon/aline.png' or not replacement.is_file() or
                digest(replacement.read_bytes()) != record['replacement_sha256']):
            errors.append('新共享图标缺失或身份不符：' + name)
            continue
        target = ROOT / name
        if name.endswith('.png'):
            valid = record['action'] == 'delete' and record['new_sha256'] is None and not target.exists() and not target.is_symlink()
        else:
            data = target.read_bytes() if target.is_file() else b''
            new_ref, old_ref = b'../../assets/app-icon/aline.png', b'./assets/aline.png'
            valid = (record['action'] == 'replace_icon_reference' and data.count(new_ref) == 3 and
                     digest(data) == record['new_sha256'] and
                     digest(data.replace(new_ref, old_ref)) == record['old_sha256'])
        if valid:
            updates[name] = record
        else:
            errors.append('图标授权变更当前状态不符：' + name)
    return updates, errors


def lines_outside_fences(content):
    """保持原行号，隔离历史原文；覆盖反引号与波浪号围栏。"""
    fence = None
    for number, line in enumerate(content.splitlines(), 1):
        match = re.match(r'^\s{0,3}(`{3,}|~{3,})', line)
        if match:
            marker = match[1]
            if fence is None:
                fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence):
                fence = None
            continue
        if fence is None:
            yield number, line


def anchors(content):
    result, counts = set(), Counter()
    for _, line in lines_outside_fences(content):
        match = re.match(r'^#{1,6}\s+(.+?)\s*#*$', line)
        if match:
            title = re.sub(r'!?\[([^\]]+)\]\([^)]*\)', r'\1', match[1])
            slug = re.sub(r'[^\w\- ]', '', title.lower()).replace(' ', '-')
            count = counts[slug]
            counts[slug] += 1
            result.add(slug + (f'-{count}' if count else ''))
        result.update(re.findall(r'\b(?:id|name)=[\"\']([^\"\']+)', line))
    return result


def references(paths):
    result = []
    for path in paths:
        content = path.read_text()
        for number, line in lines_outside_fences(content):
            line = re.sub(r'(`+)(.+?)\1', '', line)
            for match in re.finditer(r'!?\[[^\]]*\]\((<[^>]+>|[^\s)]+)(?:\s+"[^"]*")?\)', line):
                value = match[1].strip('<>')
                url = urlsplit(value)
                if url.scheme or url.netloc:
                    continue
                target = (path.parent / unquote(url.path)).resolve() if url.path else path
                record = {'source': str(path.relative_to(ROOT)), 'line': number,
                          'reference': value, 'target': str(target.relative_to(ROOT))
                          if target.is_relative_to(ROOT) else str(target),
                          'exists': target.exists()}
                if url.fragment and target.is_file() and target.suffix == '.md':
                    record['anchor_exists'] = unquote(url.fragment) in anchors(target.read_text())
                result.append(record)
    return result


def archive_check(hidden):
    """无原件时仍核片段哈希；原件可见时额外核原行与整文件。"""
    manifest = json.loads((ROOT / 'docs/archive/migration-evidence/index.json').read_text())
    result, errors = [], []
    for record in manifest['records']:
        target = ROOT / record['target']
        data = target.read_bytes()
        entry = {'source': record['source'], 'target': record['target'],
                 'target_hash_matches': digest(data) == record['target_sha256'], 'excerpts': []}
        if not entry['target_hash_matches']:
            errors.append('归档整文件哈希：' + record['target'])
        # 保全格式为独立八波浪号围栏，不将原文中的三反引号误拆为片段。
        chunks = re.findall(rb'^~~~~~~~~markdown\r?\n(.*?)^~~~~~~~~\s*$', data, re.M | re.S)
        if len(chunks) != len(record['excerpts']):
            errors.append('归档片段数量：' + record['target'])
        source = ROOT / record['source']
        source_visible = not any(source.is_relative_to(ROOT / name) for name in hidden)
        source_data = source.read_bytes() if source_visible and source.is_file() else None
        entry['source_checked'] = source_data is not None
        if source_data is not None and digest(source_data) != record['source_sha256']:
            errors.append('来源整文件哈希：' + record['source'])
        for excerpt, chunk in zip(record['excerpts'], chunks):
            # 归档在闭围栏前额外插入一个分隔换行；按哈希确认究竟是哪一份。
            variants = (chunk, chunk[:-1] if chunk.endswith(b'\n') else chunk)
            retained = next((value for value in variants if digest(value) == excerpt['sha256']), None)
            item = {'start_line': excerpt['start_line'], 'end_line': excerpt['end_line'],
                    'retained_hash_matches': retained is not None}
            if retained is None:
                errors.append('归档片段哈希：' + record['target'] + ':' + str(excerpt['start_line']))
            if source_data is not None:
                original = b''.join(source_data.splitlines(keepends=True)[excerpt['start_line'] - 1:excerpt['end_line']])
                item['source_range_matches'] = original == retained
                if original != retained:
                    errors.append('原行范围与归档不符：' + record['source'] + ':' + str(excerpt['start_line']))
            entry['excerpts'].append(item)
        result.append(entry)
    return result, errors


def prior_evidence_check(hidden):
    """自行还原精选证据，不采信 manifest 中的通过布尔值。"""
    manifest = json.loads((BASE / 'prior-evidence/manifest.json').read_text())
    result, errors = [], []
    for record in manifest['records']:
        data = (ROOT / record['target']).read_bytes()
        target_ok = digest(data) == record['target_sha256']
        if record['mode'] == 'fenced_original':
            match = re.search(rb'^(~{8,})markdown\r?\n(.*?)^\1\s*$', data, re.M | re.S)
            chunk = match[2] if match else b''
            variants = (chunk, chunk[:-1] if chunk.endswith(b'\n') else chunk)
            retained = next((value for value in variants
                             if digest(value) == record['source_sha256']), None)
        else:
            retained = data if digest(data) == record['source_sha256'] else None
        source_ok = None
        if 'reports' not in hidden and (ROOT / record['source']).is_file():
            source_ok = (ROOT / record['source']).read_bytes() == retained
        item = {'source': record['source'], 'target': record['target'],
                'target_hash_matches': target_ok, 'retained_source_hash_matches': retained is not None,
                'source_bytes_match_when_present': source_ok}
        result.append(item)
        if not target_ok or retained is None or source_ok is False:
            errors.append('精选证据字节不符：' + record['source'])
    history = manifest['scan_history']
    raw = (ROOT / history['target']).read_bytes()
    if digest(raw) != history['target_sha256']:
        errors.append('历史扫描集合哈希不符')
    stored = json.loads(raw)

    def decode(value):
        if isinstance(value, dict):
            if set(value) == {'$record'}:
                return stored['records'][value['$record']]
            if set(value) == {'$list'}:
                return [decode(entry) for entry in value['$list']]
            return {key: decode(entry) for key, entry in value.items()}
        return value

    decoded = []
    for scan in stored['scans'].values():
        restored = decode(scan['data'])
        same = None
        if 'reports' not in hidden:
            source = (ROOT / scan['source']).read_bytes()
            same = digest(source) == scan['source_sha256'] and json.loads(source) == restored
            if not same:
                errors.append('历史扫描语义还原不符：' + scan['source'])
        decoded.append({'source': scan['source'], 'semantic_roundtrip_when_present': same})
    checkpoints = stored['historical_checkpoints']
    for name, checkpoint in checkpoints['checkpoints'].items():
        restored = [checkpoints['fingerprints'][key] for key in checkpoint['entries']]
        same = None
        if 'reports' not in hidden:
            source = (ROOT / name).read_bytes()
            same = digest(source) == checkpoint['source_sha256'] and json.loads(source) == restored
            if not same:
                errors.append('历史基线语义还原不符：' + name)
        decoded.append({'source': name, 'semantic_roundtrip_when_present': same})
    identities = stored['historical_inventory_identities']
    identity_matches = None
    if 'reports' not in hidden:
        original = (ROOT / identities['source']).read_bytes()
        original_rows = {parts[1].strip(): parts[2].strip()
                         for line in original.decode().splitlines() if line.startswith('| docs/')
                         for parts in [line.split('|')]}
        identity_matches = (digest(original) == identities['source_sha256'] and
                            original_rows == identities['identity_by_path'])
        if not identity_matches:
            errors.append('历史76行身份字符串还原不符')
    decoded.append({'source': identities['source'], 'identity_rows': len(identities['identity_by_path']),
                    'semantic_roundtrip_when_present': identity_matches})
    questions = json.loads((BASE / 'prior-evidence/confirmed-question-sources.json').read_text())
    confirmation = (ROOT / 'docs/archive/2026-10-30-migration-decisions.md').read_text()
    if questions['audit_questions'] not in confirmation:
        errors.append('确认归档的完整原始13题发生变化')
    return {'original_records': result, 'decoded_snapshots': decoded}, errors


def disposition_check(hidden, baseline, icon_updates):
    errors = []
    rows = list(csv.DictReader((BASE / 'file-disposition.csv').open()))
    expected = {row['path']: row for row in baseline if row['kind'] == 'file' and
                row['path'].startswith(('docs_bak/', 'reports/'))}
    actual = [row['old_path'] for row in rows]
    if len(rows) != 86 or len(set(actual)) != len(actual) or set(actual) != set(expected):
        errors.append('86文件处置表覆盖/唯一性不符')
    for row in rows:
        entry = expected.get(row['old_path'], {})
        if row['sha256'] != entry.get('sha256') or int(row['bytes']) != entry.get('bytes'):
            errors.append('逐文件身份不符：' + row['old_path'])
        if row['blocks_deletion'] != '否':
            errors.append('处置表仍有阻塞：' + row['old_path'])
    topics = list(csv.DictReader((BASE / 'migration-topics.csv').open()))
    if len(topics) != 419:
        errors.append('419主题账本数量不符')
    target_mentions = 0
    for row in rows + topics:
        for target in re.findall(r'docs/[^\s；;，、]+', row['independent_target']):
            target = target.rstrip('：:。)）')
            target_mentions += 1
            superseded = target in icon_updates and icon_updates[target]['action'] == 'delete'
            if not (ROOT / target).exists() and not superseded:
                errors.append('处置表独立目标缺失：' + target)
    checked = 0
    covered = {}
    if 'docs_bak' not in hidden and (ROOT / 'docs_bak').is_dir():
        for row in topics:
            source = (ROOT / row['old_path']).read_bytes().splitlines(keepends=True)
            start, end = int(row['start_line']), int(row['end_line'])
            chunk = b''.join(source[start - 1:end])
            if digest(chunk) != row['sha256']:
                errors.append('主题原行哈希不符：' + row['old_path'] + ':' + str(start))
            checked += 1
            covered.setdefault(row['old_path'], set()).update(range(start, end + 1))
        for name, line_numbers in covered.items():
            total = len((ROOT / name).read_bytes().splitlines())
            if line_numbers != set(range(1, total + 1)):
                errors.append('主题原行覆盖不完整：' + name)
    requirements = list(csv.DictReader((BASE / 'spec-trace.csv').open()))
    acceptance = list(csv.DictReader((BASE / 'acceptance-trace.csv').open()))
    if len(requirements) != 89 or len({row['requirement'] for row in requirements}) != 89:
        errors.append('89需求追踪数量或唯一性不符')
    if {row['id'] for row in acceptance} != {f'AC-{number:02d}' for number in range(1, 21)}:
        errors.append('20项AC追踪不完整')
    return {'files': len(rows), 'topics': len(topics), 'source_topic_hashes_checked': checked,
            'source_documents_line_coverage_checked': len(covered),
            'independent_target_mentions_checked': target_mentions,
            'requirements': len(requirements), 'acceptance': len(acceptance)}, errors


def main():
    hidden = tuple(sys.argv[1:])
    if any(name not in {'docs_bak', 'reports'} for name in hidden):
        raise SystemExit('仅接受 docs_bak 和 reports 作为只读路径模拟参数')
    absent = tuple(name for name in ('docs_bak', 'reports') if not (ROOT / name).exists())
    source_unavailable = tuple(dict.fromkeys((*hidden, *absent)))
    baseline = json.loads((BASE / 'baseline-inventory.json').read_text())['entries']
    errors, unchanged = [], {}
    icon_updates, icon_errors = authorized_icon_updates(baseline)
    errors.extend(icon_errors)
    for directory in ('docs_bak', 'reports'):
        if directory in hidden:
            unchanged[directory] = '模拟不可见，不读取'
            continue
        if directory in absent:
            unchanged[directory] = '原目录实际不存在，跳过可选原件比对；删除前核对见执行记录'
            continue
        expected = {row['path']: row['sha256'] for row in baseline
                    if row['kind'] == 'file' and row['path'].startswith(directory + '/')}
        actual = {str(path.relative_to(ROOT)): digest(path.read_bytes())
                  for path in (ROOT / directory).rglob('*') if path.is_file()}
        changed = [name for name in sorted(expected.keys() | actual.keys())
                   if expected.get(name) != actual.get(name)]
        unchanged[directory] = {'files': len(actual), 'changes': changed}
        errors.extend('原目录改变：' + name for name in changed)
    current = [ROOT / 'AGENTS.md', ROOT / 'CLAUDE.md', *sorted((ROOT / 'docs').rglob('*.md'))]
    # 隔离的历史正文由围栏跳过，归档外层入口仍与当前文档一起核对。
    refs = references(current)
    for entry in refs:
        entry['visible'] = entry['exists'] and not any(
            entry['target'] == name or entry['target'].startswith(name + '/') for name in hidden)
    broken = [entry for entry in refs if not entry['visible'] or entry.get('anchor_exists') is False]
    errors.extend('当前引用失效：' + str(entry) for entry in broken)
    archives, archive_errors = archive_check(source_unavailable)
    errors.extend(archive_errors)
    prior, prior_errors = prior_evidence_check(source_unavailable)
    errors.extend(prior_errors)
    disposition, disposition_errors = disposition_check(source_unavailable, baseline, icon_updates)
    errors.extend(disposition_errors)
    protected = [row for row in baseline if row['kind'] == 'file' and (
        '/prototype/legacy/' in row['path'] or
        (row['path'].startswith('docs/archive/2026-10-') and
         '2026-10-30-migration-decisions.md' not in row['path']))]
    protected_changes = [row['path'] for row in protected
                         if row['path'] not in icon_updates and
                         (not (ROOT / row['path']).is_file() or
                          digest((ROOT / row['path']).read_bytes()) != row['sha256'])]
    errors.extend('原型或已有确认原文改变：' + name for name in protected_changes)
    parsed = {'json': [], 'python': []}
    for path in sorted((ROOT / 'docs').rglob('*')):
        # 大型基线/快照和输出结果属于数据，不执行其中命令。
        if path.suffix == '.json':
            json.loads(path.read_text())
            parsed['json'].append(str(path.relative_to(ROOT)))
        if path.suffix == '.py':
            ast.parse(path.read_text())
            parsed['python'].append(str(path.relative_to(ROOT)))
    result = {'date': '2026-10-30', 'hidden_by_path_resolution_only': hidden,
              'actually_absent_original_directories': absent,
              'original_directories': unchanged, 'current_markdown_files': len(current),
              'local_links': refs, 'broken_links': broken, 'archives': archives,
              'prior_evidence': prior, 'protected_changes': protected_changes,
              'authorized_icon_updates_checked': sorted(icon_updates),
              'disposition': disposition,
              'syntax_checked': parsed, 'errors': errors,
              'limits': '路径模拟不移动目录；静态Markdown解析不是完整CommonMark/浏览器。HTML/CSS/JS动态主题与HTTP由prototype工具和人工审计负责；不联网、不验第三方许可、不执行历史生成器/应用/Cargo/V实验。原目录不存在时省略可选来源比对，不宣称恢复了缺失原文。'}
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return bool(errors)


if __name__ == '__main__':
    raise SystemExit(main())
