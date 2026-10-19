from pathlib import Path
import json
import runpy
import shutil
import tempfile

U = Path(__file__).resolve().parent
D = U / 'loader-source/docs/specifications/windows/native-loader-registry'
module = runpy.run_path(str(U / 'check-native-loader-registry.py'))
check, load = module['check'], module['load']


def text(root, path, before, after):
    p = root / path
    value = p.read_text()
    assert before in value
    p.write_text(value.replace(before, after, 1))


def record(root, path, change):
    p = root / path
    value = load(p)
    change(value)
    p.write_text(json.dumps(value) + '\n')


controls = []
for folder, source, _, present in module['FAMILIES']:
    prefix = folder + '/' + source
    controls.extend([
        (source + '-status', lambda r, p=prefix, present=present: text(r, p + '-arm64.log',
            'status=00000000' if present else 'status=C0000034',
            'status=C0000034' if present else 'status=00000000')),
        (source + '-duplicate', lambda r, p=prefix: text(r, p + '-arm64.log',
            'sample=0 path=1 desired=00000001', 'sample=0 path=0 desired=00000001')),
        (source + '-source-after', lambda r, p=prefix: record(r, p + '-arm64-source-after.json', lambda x: x.update(Hash='0' * 64))),
        (source + '-module-after', lambda r, p=prefix: record(r, p + '-modules-after.json', lambda x: x[0].update(Hash='0' * 64))),
        (source + '-pe-machine', lambda r, p=prefix: record(r, p + '-arm64-before.json', lambda x: x.update(machine=0x8664))),
        (source + '-producer-failed', lambda r, p=prefix: record(r, p + '-outcomes.json', lambda x: x[0].update(**{'exit-code': 1}))),
    ])
assert check(D)['native-cases'] == 960
rows = []
with tempfile.TemporaryDirectory(prefix='loader-native-controls-', dir=U) as temporary:
    for name, change in controls:
        root = Path(temporary) / name
        root.mkdir()
        for folder, _, _, _ in module['FAMILIES']:
            shutil.copytree(D / folder, root / folder)
        change(root)
        try:
            check(root)
        except (AssertionError, ValueError, KeyError) as error:
            rows.append({'name': name, 'rejected': True, 'error-type': type(error).__name__})
        else:
            raise AssertionError('Corrupt native evidence accepted: ' + name)
(U / 'loader-native-negative-controls.json').write_text(json.dumps({'controls': rows, 'all-rejected': True}, indent=2) + '\n')
print('rejected', len(rows), 'native evidence corruptions')
