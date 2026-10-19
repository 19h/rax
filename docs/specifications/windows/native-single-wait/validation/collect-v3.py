from pathlib import Path
import hashlib
import itertools
import json
import re
import shutil

N = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
B = N / 'native-v3'
B.mkdir(exist_ok=True)
for path in S.glob('native-wait-single-v3*'):
    if path.is_file() and path.suffix in ['.json', '.log', '.cpp', '.ps1']:
        shutil.copy2(path, B / path.name)


def load(p):
    raw = p.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))


meta = load(N / 'probe-v3-source.json')
assert hashlib.sha256((B / meta['source']).read_bytes()).hexdigest() == meta['sha256']
launch = load(B / 'native-wait-single-v3-launch.json')
assert launch['state'] == 'finished' and launch['exit-code'] == 0
modules = load(B / 'native-wait-single-v3-modules-before.json')
assert modules == load(B / 'native-wait-single-v3-modules-after.json')
assert {row['Hash'].lower() for row in modules} == {
    '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'}
summary = []
for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
    stem = 'native-wait-single-v3-' + profile
    before = load(B / (stem + '-before.json'))
    assert before == load(B / (stem + '-after.json'))
    assert before['machine'] == (43620 if profile == 'arm64' else 34404 if profile == 'x64' else 332)
    assert before['large-address-aware'] == (profile != 'x86')
    assert load(B / (stem + '-source-after.json'))['Hash'].lower() == meta['sha256']
    lines = (B / (stem + '.log')).read_text().splitlines()
    groups = {key: [fields(line) for line in lines if line.startswith(key + ' ')]
              for key in ['wait', 'apc', 'win32-apc', 'time', 'multiple-apc', 'sleep-apc']}
    assert {key: len(rows) for key, rows in groups.items()} == {
        'wait': 560, 'apc': 14, 'win32-apc': 14, 'time': 16, 'multiple-apc': 84, 'sleep-apc': 7}
    old = [fields(line) for line in (N / 'native-v1' / ('native-wait-single-v1-' + profile + '.log')).read_text().splitlines()
           if line.startswith('wait ')]
    assert groups['wait'] == old
    assert all(row['escaped'] == '00000000' for key, rows in groups.items() if key != 'sleep-apc' for row in rows)
    done = fields(lines[-1])
    assert done['before'] == done['after']
    alerts = [0, 1, 2, 255, 256, 257, 0xFFFFFFFF]
    assert [(int(row['mask']), int(row['all'], 16), int(row['alertable'], 16)) for row in groups['multiple-apc']] == list(
        itertools.product(range(4), [0, 1, 256], alerts))
    for row in groups['multiple-apc']:
        mask = int(row['mask'])
        all_ = int(row['all'], 16) != 0
        alert = int(row['alertable'], 16) & 255 != 0
        ready = mask == 3 if all_ else mask != 0
        if alert:
            status = 0xC0
        elif ready:
            status = 0 if all_ or mask & 1 else 1
        else:
            status = 0x102
        assert int(row['status'], 16) == status, (profile, row, status)
        assert int(row['during']) == int(alert)
        remaining = mask if alert or not ready else 0 if all_ else mask & ~(1 if mask & 1 else 2)
        assert int(row['next-a'], 16) == (0 if remaining & 1 else 0x102)
        assert int(row['next-b'], 16) == (0 if remaining & 2 else 0x102)
        assert row['after-drain'] == '1'
    for key in ['apc', 'win32-apc']:
        for row in groups[key]:
            alert = int(row['alertable'], 16) & 255 != 0
            expected = 0xC0 if alert else 0 if row['signaled'] == '1' else 0x102
            assert int(row['status'], 16) == expected
            assert int(row['during']) == int(alert) and row['after-drain'] == '1'
            assert int(row['next'], 16) == (0 if alert and row['signaled'] == '1' else 0x102)
    for row in groups['time']:
        expected = 0 if row['signaled'] == '1' or int(row['mode']) >= 6 else 0x102
        assert int(row['status'], 16) == expected
    print(profile, 'SleepEx', [(row['alertable'], row['status'], row['during']) for row in groups['sleep-apc']])
    summary.append({'profile': profile, 'counts': {key: len(rows) for key, rows in groups.items()}, 'sleep-rows': groups['sleep-apc'],
                    'waits-identical-to-v1': True, 'handle-count-stable': True})
(N / 'native-v3-reviewed.json').write_text(json.dumps({'source': meta, 'runs': summary,
    'finding': 'Win32 multiple waits retain full-value WaitAll and low-byte alertable behavior, with queued APC before signal consumption. SleepEx rows are retained for interpretation.'}, indent=2) + '\n')
print('Verified native revision3 across four ABI profiles')
