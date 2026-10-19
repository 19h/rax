from pathlib import Path
import importlib.util
import json
import re
import shutil
import sys
import tempfile


sys.dont_write_bytecode = True
O = Path(__file__).resolve().parent
D = O / 'semaphore-source/docs/specifications/windows/native-semaphore'
spec = importlib.util.spec_from_file_location('semaphore_check_native', D / 'check_native.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def rewrite_json(path, change):
    data = checker.load(path)
    change(data)
    path.write_text(json.dumps(data, indent=2) + '\n')


def rewrite_line(path, prefix, change):
    lines = path.read_text().splitlines()
    found = False
    for i, line in enumerate(lines):
        if not found and line.startswith(prefix):
            lines[i] = change(line)
            found = True
    assert found, prefix
    path.write_text('\n'.join(lines) + '\n')


def corrupt(root, name):
    v1 = root / 'native-v1'
    stem = 'native-semaphore-v1'
    log = v1 / (stem + '-arm64.log')
    if name == 'source':
        path = v1 / (stem + '.cpp')
        path.write_bytes(path.read_bytes() + b'\n')
    elif name == 'unfinished':
        rewrite_json(v1 / (stem + '-launch.json'), lambda d: d.update(state='running'))
    elif name == 'missing-profile':
        rewrite_json(v1 / (stem + '-outcomes.json'), lambda d: d.pop())
    elif name == 'provider':
        for suffix in ('before', 'after'):
            rewrite_json(v1 / (stem + '-modules-' + suffix + '.json'),
                         lambda d: d[0].update(Hash='0' * 64))
    elif name == 'pe-machine':
        for suffix in ('before', 'after'):
            rewrite_json(v1 / (stem + '-arm64-' + suffix + '.json'), lambda d: d.update(machine=332))
    elif name == 'post-source':
        rewrite_json(v1 / (stem + '-arm64-source-after.json'), lambda d: d.update(Hash='0' * 64))
    elif name == 'case-count':
        rewrite_line(log, 'summary ', lambda line: line.replace('cases=5235', 'cases=5234'))
    elif name == 'leaked-handle':
        rewrite_line(log, 'summary ', lambda line: re.sub(r'handles-after=(\d+)',
                     lambda m: 'handles-after=' + str(int(m.group(1)) + 1), line))
    elif name == 'escaped-exception':
        rewrite_line(log, 'create ', lambda line: line.replace('escaped=00000000', 'escaped=00000001'))
    elif name == 'count-validation':
        rewrite_line(log, 'create ordinal=32 ', lambda line: line.replace('status=C000000D', 'status=00000000'))
    elif name == 'open-precedence':
        rewrite_line(log, 'open ', lambda line: line.replace('status=C000000D', 'status=00000000'))
    elif name == 'release-precedence':
        rewrite_line(log, 'release ', lambda line: line.replace('status=C000000D', 'status=00000000'))
    elif name == 'release-mutated-on-failure':
        rewrite_line(log, 'release ', lambda line: line.replace('count=1 ', 'count=2 '))
    elif name == 'query-precedence':
        rewrite_line(log, 'query ', lambda line: line.replace('status=00000000', 'status=C0000003'))
    elif name == 'bridge-identity':
        path = root / 'native-names-v2/native-semaphore-names-v2-arm64.log'
        rewrite_line(path, 'bridge-open ', lambda line: line.replace('same=1', 'same=0'))
    elif name == 'existing-count-validation':
        path = root / 'native-names-v2/native-semaphore-names-v2-arm64.log'
        rewrite_line(path, 'named ordinal=156 ', lambda line: line.replace('status=C000000D', 'status=40000000'))
    elif name == 'replay-bytes':
        path = root / 'replay/query-arm64.tsv'
        path.write_text(path.read_text().replace('A5A5', 'A4A4', 1))
    else:
        raise ValueError(name)


names = ('source', 'unfinished', 'missing-profile', 'provider', 'pe-machine', 'post-source',
         'case-count', 'leaked-handle', 'escaped-exception', 'count-validation', 'open-precedence',
         'release-precedence', 'release-mutated-on-failure', 'query-precedence', 'bridge-identity',
         'existing-count-validation', 'replay-bytes')
report = []
for name in names:
    with tempfile.TemporaryDirectory(prefix='semaphore-negative-', dir=O) as temporary:
        target = Path(temporary) / 'native-semaphore'
        shutil.copytree(D, target)
        corrupt(target, name)
        try:
            # Rebind replay files for native-record corruptions so a projection
            # mismatch cannot mask a missing independent semantic check.
            checker.check(target, write=name != 'replay-bytes')
        except ValueError as error:
            report.append({'case': name, 'rejected': True, 'reason': str(error)})
        else:
            raise AssertionError('accepted corrupted evidence: ' + name)
(O / 'semaphore-native-stress.json').write_text(json.dumps(report, indent=2) + '\n')
print('Rejected', len(report), 'native evidence corruptions, including rebound replay projections.')
