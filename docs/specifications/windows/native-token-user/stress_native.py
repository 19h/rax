from pathlib import Path
import json
import runpy
import shutil
import tempfile

Q = Path(__file__).resolve().parent
D = Q / 'user-source/docs/specifications/windows/native-token-user'
module = runpy.run_path(str(D / 'check_native.py'))
check, load = module['check'], module['load']
STEM = module['STEM']


def edit_json(root, name, mutate):
    path = root / 'native' / name
    value = load(path)
    mutate(value)
    path.write_text(json.dumps(value) + '\n')


def edit_log(root, change):
    path = root / 'native' / (STEM + '-arm64.log')
    before = path.read_text()
    after = change(before)
    assert before != after
    path.write_text(after)


controls = [
    ('source', lambda r: (r / 'native' / (STEM + '.cpp')).write_text('changed\n')),
    ('source-after', lambda r: edit_json(r, STEM + '-arm64-source-after.json', lambda j: j.update(Hash='0' * 64))),
    ('machine', lambda r: edit_json(r, STEM + '-arm64-before.json', lambda j: j.update(machine=332))),
    ('module-after', lambda r: edit_json(r, STEM + '-modules-after.json', lambda j: j[0].update(Hash='0' * 64))),
    ('producer-failed', lambda r: edit_json(r, STEM + '-outcomes.json', lambda j: j[0].update({'exit-code': 1}))),
    ('ordinal', lambda r: edit_log(r, lambda t: t.replace('case ordinal=1 ', 'case ordinal=0 ', 1))),
    ('status', lambda r: edit_log(r, lambda t: t.replace('status=C0000023 escaped=', 'status=00000000 escaped=', 1))),
    ('attributes', lambda r: edit_log(r, lambda t: t.replace('attributes=00000000 sid-bytes=', 'attributes=00000001 sid-bytes=', 1))),
    ('required-size', lambda r: edit_log(r, lambda t: t.replace('user-reference required=28 ', 'user-reference required=20 ', 1))),
    ('guard', lambda r: edit_log(r, lambda t: t.replace('first-after=00000004 ', 'first-after=00000104 ', 1))),
    ('boundary-omitted', lambda r: edit_log(r, lambda t: t.replace('boundary ordinal=0 ', 'omitted ordinal=0 ', 1))),
    ('corpus-drift', lambda r: (r / 'cases.json').write_text('{}\n')),
]
check(D)
rows = []
with tempfile.TemporaryDirectory(prefix='native-user-controls-', dir=Q) as temp:
    for name, mutate in controls:
        root = Path(temp) / name
        shutil.copytree(D, root)
        mutate(root)
        try:
            check(root)
        except (AssertionError, KeyError, ValueError):
            rows.append({'name': name, 'rejected': True})
        else:
            raise AssertionError('Corruption accepted: ' + name)
(Q / 'native-user-negative-controls.json').write_text(json.dumps({'controls': rows, 'all-rejected': True}, indent=2) + '\n')
print('rejected', len(rows), 'native User corruptions')
