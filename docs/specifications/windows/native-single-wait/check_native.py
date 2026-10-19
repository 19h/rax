"""Check raw native producer identity, matrices, priority and replay projection."""
from pathlib import Path
import hashlib
import itertools
import json
import re
import sys

PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
ALERTS = [0, 1, 2, 255, 256, 257, 0xFFFFFFFF]
SOURCES = {
    1: 'b09bd613fff198f612f79c4de2e1b8430d4f2e6e5b8c81facff8232fa8b9a704',
    2: 'ce05d688f3fa2dcb1897809b5eb0251f450cd99a9f6c6718efaa7a73d3879010',
    3: '9b84ac623008bd6450c2d8118ebbdaeec7ce32f0c1051db6aeb54be266cca713',
    4: 'e0bc5ff2ca9908b31881d9831f8bcee87d76349c24aed536bddccaac5a1b130b',
    5: '78cf015a27984c3a46d56a612af9e262060944025f2c0ba10f7123ef7d1e493c',
}
NTDLL = {'502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'}

def need(condition, message):
    if not condition:
        raise ValueError(message)

def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))

def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))

def check_native(root):
    baseline_waits = {}
    final = {}
    summaries = []
    for revision, source_hash in SOURCES.items():
        directory = root / ('native-v' + str(revision))
        stem = 'native-wait-single-v' + str(revision)
        need(hashlib.sha256((directory / (stem + '.cpp')).read_bytes()).hexdigest() == source_hash, stem + ' source drift')
        launch = load(directory / (stem + '-launch.json'))
        need(launch['state'] == 'finished' and launch['exit-code'] == 0, stem + ' producer incomplete')
        outcomes = load(directory / (stem + '-outcomes.json'))
        need([(row['profile'], row['exit-code']) for row in outcomes] == [(p, 0) for p in PROFILES], stem + ' profile outcomes')
        modules = load(directory / (stem + '-modules-before.json'))
        need(modules == load(directory / (stem + '-modules-after.json')), stem + ' provider drift')
        need(len(modules) == (6 if revision == 5 else 2), stem + ' provider count')
        hashes = {row['Hash'].lower() for row in modules}
        need(NTDLL <= hashes, stem + ' NTDLL version')
        for profile in PROFILES:
            label = stem + '-' + profile
            before = load(directory / (label + '-before.json'))
            need(before == load(directory / (label + '-after.json')), label + ' PE drift')
            need(before['profile'] == profile, label + ' PE profile')
            need(before['machine'] == {'arm64': 43620, 'x64': 34404, 'x86': 332, 'x86-laa': 332}[profile], label + ' PE machine')
            need(before['large-address-aware'] == (profile != 'x86'), label + ' PE LAA')
            need(load(directory / (label + '-source-after.json'))['Hash'].lower() == source_hash, label + ' post-source identity')
            ps = (directory / (stem + '.ps1')).read_text()
            need(source_hash in ps and '/we4473 /we4474 /we4477' in ps and '/MT /GS' in ps, label + ' checked build policy')
            lines = (directory / (label + '.log')).read_text().splitlines()
            header = fields(lines[0])
            need(header['profile'] == profile and int(header['page-bytes']) == 4096, label + ' runtime profile')
            need(int(header['pointer-bytes']) == (4 if profile.startswith('x86') else 8), label + ' pointer width')
            keys = ['wait', 'apc', 'win32-apc', 'time', 'multiple-apc', 'sleep-apc', 'object-apc']
            groups = {key: [fields(line) for line in lines if line.startswith(key + ' ')] for key in keys}
            expected = [560, 14, 14 if revision >= 2 else 0, 16 if revision >= 2 else 0,
                84 if revision >= 3 else 0, 7 if revision >= 3 else 0, 84 if revision >= 4 else 0]
            need([len(groups[key]) for key in keys] == expected, label + ' matrix counts')
            for key, rows in groups.items():
                need([int(row['case']) for row in rows] == list(range(len(rows))), label + ' ' + key + ' ordering')
                need(all(row.get('escaped', '00000000') == '00000000' for row in rows), label + ' escaped exception')
            need([(int(r['kind']), int(r['alertable'], 16), int(r['timeout'])) for r in groups['wait']] ==
                list(itertools.product(range(8), ALERTS, range(10))), label + ' fault coverage')
            if revision == 1:
                baseline_waits[profile] = groups['wait']
            else:
                need(groups['wait'] == baseline_waits[profile], label + ' fault matrix revision drift')
            for row in groups['wait']:
                kind, mode = int(row['kind']), int(row['timeout'])
                status = 0x80000001 if mode in [6, 7] else 0xC0000005 if mode in [5, 9] else 0xC0000022 if kind in [4, 5] else 0xC0000008 if kind in [6, 7] else 0
                need(int(row['status'], 16) == status, label + ' timeout/handle priority')
                protection = 2 if mode in [4, 7] else 1 if mode == 5 else 4
                need(int(row['protection'], 16) == protection, label + ' guard consumption')
                if status != 0 or kind in [1, 4, 5, 6, 7]:
                    need(row['after'] == row['before'], label + ' failed/manual wait mutated object')
            for key in ['apc', 'win32-apc']:
                rows = groups[key]
                need(not rows or [(int(r['signaled']), int(r['alertable'], 16)) for r in rows] == list(itertools.product(range(2), ALERTS)), label + ' ' + key + ' coverage')
                for row in rows:
                    alert = int(row['alertable'], 16) & 255 != 0
                    expected_status = 0xC0 if alert else 0 if row['signaled'] == '1' else 0x102
                    need(int(row['status'], 16) == expected_status and int(row['during']) == int(alert), label + ' APC/signal/timeout priority')
                    need(row['after-drain'] == '1' and row['drained'] == '00000000', label + ' APC drain')
                    need(int(row['next'], 16) == (0 if alert and row['signaled'] == '1' else 0x102), label + ' APC consumed signal')
            for row in groups['time']:
                need(int(row['status'], 16) == (0 if row['signaled'] == '1' or int(row['mode']) >= 6 else 0x102), label + ' signed timeout boundary')
            if groups['multiple-apc']:
                need([(int(r['mask']), int(r['all'], 16), int(r['alertable'], 16)) for r in groups['multiple-apc']] == list(itertools.product(range(4), [0, 1, 256], ALERTS)), label + ' multiple coverage')
            for row in groups['multiple-apc']:
                mask, all_, alert = int(row['mask']), int(row['all'], 16) != 0, int(row['alertable'], 16) & 255 != 0
                ready = mask == 3 if all_ else mask != 0
                status = 0xC0 if alert else (0 if all_ or mask & 1 else 1) if ready else 0x102
                remaining = mask if alert or not ready else 0 if all_ else mask & ~(1 if mask & 1 else 2)
                need(int(row['status'], 16) == status and int(row['during']) == int(alert), label + ' multiple status/APC')
                need(int(row['next-a'], 16) == (0 if remaining & 1 else 0x102) and int(row['next-b'], 16) == (0 if remaining & 2 else 0x102), label + ' multiple transactional consumption')
                need(row['after-drain'] == '1' and row['drained'] == '00000000', label + ' multiple drain')
            need(not groups['sleep-apc'] or [int(r['alertable'], 16) for r in groups['sleep-apc']] == ALERTS, label + ' SleepEx coverage')
            for row in groups['sleep-apc']:
                alert = int(row['alertable'], 16) & 255 != 0
                need(int(row['status'], 16) == (0xC0 if alert else 0) and int(row['during']) == int(alert), label + ' SleepEx normalization')
                need(row['after-drain'] == '1' and row['drained'] == '00000000', label + ' SleepEx drain')
            if groups['object-apc']:
                need([(int(r['kind']), int(r['alertable'], 16)) for r in groups['object-apc']] == list(itertools.product(range(12), ALERTS)), label + ' object APC coverage')
            for row in groups['object-apc']:
                kind, alert = int(row['kind']), int(row['alertable'], 16) & 255 != 0
                expected_status = 0xC0000022 if kind in [4, 5] else 0xC0000008 if kind in [6, 7] else 0xC0 if alert else 0 if kind in [0, 1, 2, 3, 8] else 0x102
                delivered = expected_status == 0xC0
                need(int(row['status'], 16) == expected_status and int(row['during']) == int(delivered), label + ' object access/APC priority')
                need(row['after-drain'] == '1' and row['drained'] == '00000000', label + ' object APC drain')
                if expected_status != 0 or kind == 1:
                    need(row['before'] == row['after'], label + ' object APC consumed signal or recursion')
            done = fields(lines[-1])
            need(done['before'] == done['after'], label + ' handle leak')
            if revision >= 4:
                context = fields(next(line for line in lines if line.startswith('context ')))
                need([context[k] for k in ['version-status', 'major', 'minor', 'build']] == ['00000000', '10', '0', '29683'], label + ' kernel context')
                need(context['precise-unbiased'] == ('1' if revision == 5 else '0'), label + ' precise clock context')
                if revision == 5:
                    provider = fields(next(line for line in lines if line.startswith('clock-provider ')))
                    need(provider == {'kernel32': '0', 'kernelbase': '1'} and int(context['awake-100ns']) > 0, label + ' clock provider')
            if revision == 5:
                final[profile] = groups
            summaries.append({'revision': revision, 'profile': profile, 'counts': dict(zip(keys, expected))})
    for profile, groups in final.items():
        for prefix, key, columns in [
            ('single', 'wait', ['case', 'kind', 'alertable', 'timeout', 'status', 'protection', 'before', 'after']),
            ('object-apc', 'object-apc', ['case', 'kind', 'alertable', 'status', 'before', 'after', 'during'])]:
            expected = ''.join('\t'.join(row[k] for k in columns) + '\n' for row in groups[key])
            need((root / 'replay' / (prefix + '-' + profile + '.tsv')).read_text() == expected, profile + ' replay projection')
    return summaries

if __name__ == '__main__':
    root = Path(sys.argv[1]) if len(sys.argv) == 2 else Path(__file__).resolve().parent
    summary = check_native(root)
    print(json.dumps({'native-runs': len(summary), 'status': 'passed', 'matrices': summary}))
