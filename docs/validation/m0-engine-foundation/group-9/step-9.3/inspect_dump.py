"""对照既有 dump、手写金样及当前源字节；不运行解析器。"""

import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[5]
fixture = ROOT / 'crates/aline-markdown/tests/fixtures/m0/basic.md'
golden = fixture.with_name('basic.blocks.tsv')
dump = ROOT / 'docs/validation/m0-engine-foundation/group-6/step-6.3/dump-tree.stdout.log'
source = fixture.read_bytes()
entries = []
for line in golden.read_text().splitlines():
    if not line or line.startswith('#'):
        continue
    kind, span, label = line.split('\t')
    start, end = map(int, span.split('..'))
    entries.append({'kind': kind, 'start': start, 'end': end, 'label': label,
                    'source': source[start:end].decode('utf-8')})
expected = [f"{item['kind']} {item['start']}..{item['end']}" + (f" {item['label']}" if item['kind'] == 'block' else '') for item in entries]
actual = [line for line in dump.read_text().splitlines() if line.startswith(('block ', 'gap '))]
coverage = entries[0]['start'] == 0 and entries[-1]['end'] == len(source) and all(first['end'] == second['start'] for first, second in zip(entries, entries[1:]))
print(json.dumps({'kind': '人工金样/原始dump与源字节核查，不是解析器重跑',
                  'fixture_bytes': len(source), 'golden_matches_dump': expected == actual,
                  'partition_exactly_covers_source': coverage,
                  'entries': entries,
                  'protected_source': [{'range': [start, end], 'kind': kind, 'source': source[int(start):int(end)].decode('utf-8')} for start, end, kind in re.findall(r'^protected (\d+)\.\.(\d+) (\w+)$', dump.read_text(), re.MULTILINE)],
                  'sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in [fixture, golden, dump]}}, ensure_ascii=False, indent=2))
