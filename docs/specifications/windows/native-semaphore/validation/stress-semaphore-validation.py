from pathlib import Path
import hashlib
import importlib.util
import json
import shutil
import sys
import tempfile


sys.dont_write_bytecode = True
O = Path(__file__).resolve().parent
R = O / 'semaphore-integrated-source'
D = R / 'docs/specifications/windows/native-semaphore'
spec = importlib.util.spec_from_file_location('semaphore_final_check', D / 'check_validation.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + '\n')


def overlay(parent):
    target = parent / 'source'
    target.mkdir()
    special = ['docs', 'specifications', 'windows', 'native-semaphore']
    source_level, target_level = R, target
    for name in special:
        for child in source_level.iterdir():
            if child.name != name:
                (target_level / child.name).symlink_to(child, target_is_directory=child.is_dir())
        source_level = source_level / name
        target_level = target_level / name
        if name != special[-1]:
            target_level.mkdir()
    shutil.copytree(D, target_level)
    return target_level


def gate_log(v, filename, name, old, new):
    path = v / filename
    data = checker.load(path)
    row = next(row for row in data['steps'] if row.get('name', row.get('gate')) == name)
    output = v / row['log']
    text = output.read_text()
    assert old in text
    output.write_text(text.replace(old, new))
    raw = output.read_bytes()
    row.update(sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw))
    if 'results' in row:
        row['results'] = [result.replace(old, new) for result in row['results']]
    save(path, data)


def corrupt(d, name):
    v = d / 'validation'
    if name == 'full-count':
        gate_log(v, 'semaphore-macos-epoch1-gates.json', 'full', '7970 passed', '7969 passed')
    elif name == 'filtered-full':
        gate_log(v, 'semaphore-macos-epoch1-gates.json', 'full', '0 filtered out', '1 filtered out')
    elif name == 'windows-failure-name':
        gate_log(v, 'semaphore-native-epoch1-gates.json', 'full', 'lowers_vector_fp16_arithmetic_runtime', 'invented_failure')
    elif name == 'zero-capi':
        gate_log(v, 'semaphore-macos-epoch2-gates.json', 'capi', '183 passed', '0 passed')
    elif name == 'zero-ctest':
        gate_log(v, 'macos-semaphore-epoch2-owning-gates.json', 'ctest', '7/7 Test #61:', '0/7 Test #61:')
    elif name in ('core-features', 'archive-members'):
        for suffix in ('pre-cpp-archives.json', 'final-owning-archive-identities.json'):
            path = v / ('native-semaphore-epoch2-' + suffix)
            data = checker.load(path)
            if name == 'core-features':
                data['core-features'] = ['jit']
            else:
                data['archives'][1]['members'] = 0
            save(path, data)
    elif name == 'old-cpp-carrier':
        path = v / 'semaphore-native-owning-epoch2.json'
        data = checker.load(path)
        row = next(row for row in data['steps'] if row['name'] == 'cpp')
        output = v / row['log']
        lines = output.read_text().splitlines()
        for index, line in enumerate(lines):
            if line.startswith(('linked-archive-before ', 'linked-archive-after ')):
                prefix, encoded = line.split(' ', 1)
                linked = json.loads(encoded)
                linked['sha256'] = '0' * 64
                lines[index] = prefix + ' ' + json.dumps(linked)
        output.write_text('\n'.join(lines) + '\n')
        raw = output.read_bytes()
        row.update(sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw))
        save(path, data)
    elif name == 'current-root-claim':
        path = v / 'consumer-summary.json';data = checker.load(path)
        data['whole-current-root-product-validated'] = True;save(path, data)
    elif name == 'guest-exit-claim':
        path = v / 'ordinary-program-summary.json';data = checker.load(path)
        data['programs'][0]['guest-exit-success'] = True;save(path, data)
    elif name == 'non-capi-integration':
        path = v / 'semaphore-integration-lineage.json';data = checker.load(path)
        data['incoming-files'][0]['path'] = 'src/user/windows/dll/native/semaphore.rs';save(path, data)
    elif name == 'selected-source':
        path = v / 'semaphore-owning-source-hashes-epoch2.json';data = checker.load(path)
        data['sources'][0]['sha256'] = '0' * 64;save(path, data)
    elif name == 'program-identity':
        path = v / 'semaphore-owner-epoch1-launch.json';data = checker.load(path)
        data['outcomes'][0]['program-before']['sha256'] = '0' * 64
        data['outcomes'][0]['program-after']['sha256'] = '0' * 64;save(path, data)
        path = v / 'ordinary-program-summary.json';data = checker.load(path)
        data['programs'][0]['program']['sha256'] = '0' * 64;save(path, data)
    else:
        raise ValueError(name)


names = ('full-count', 'filtered-full', 'windows-failure-name', 'zero-capi', 'zero-ctest',
         'core-features', 'archive-members', 'old-cpp-carrier', 'current-root-claim',
         'guest-exit-claim', 'non-capi-integration', 'selected-source', 'program-identity')
report = []
for name in names:
    with tempfile.TemporaryDirectory(prefix='semaphore-final-negative-', dir=O) as temporary:
        root = overlay(Path(temporary))
        corrupt(root, name)
        manifest = checker.load(root / 'evidence.json')
        for row in manifest['files']:
            raw = (root / row['path']).read_bytes()
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
        save(root / 'evidence.json', manifest)
        try:
            checker.check(root)
        except ValueError as error:
            report.append({'case': name, 'rejected': True, 'reason': str(error)})
        else:
            raise AssertionError('accepted corrupted final evidence: ' + name)
save(O / 'semaphore-final-stress.json', report)
print('Rejected', len(report), 'final evidence corruptions after rebinding outer hashes.')
