from pathlib import Path
import contextlib
import hashlib
import io
import json
import shutil
import sys
import tempfile

sys.dont_write_bytecode = True
from check import check

HERE = Path(__file__).resolve().parent


def write(root, name, data):
    path = root / name
    path.write_bytes(data)
    manifest = json.loads((root / 'native-artifacts.json').read_text())
    row = next(item for item in manifest['artifacts'] if item['path'] == name)
    row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    (root / 'native-artifacts.json').write_text(json.dumps(manifest))


def edit_identity(root, change):
    name = 'native-section-create-priority-identities.json'
    record = json.loads((root / name).read_text())
    change(record)
    write(root, name, json.dumps(record).encode())


def edit_stdout(root, change):
    name = 'native-section-create-priority-arm64.log'
    data = change((root / name).read_bytes())
    write(root, name, data)
    def update(record):
        record['producers'][0]['stdout'].update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    edit_identity(root, update)


probes = [
    ('machine', lambda root: edit_identity(root, lambda record: record['producers'][0]['executable'].update(machine=0x8664))),
    ('laa', lambda root: edit_identity(root, lambda record: record['producers'][2]['executable'].update(characteristics=0x20))),
    ('duplicate-producer', lambda root: edit_identity(root, lambda record: record['producers'].__setitem__(1, record['producers'][0]))),
    ('duplicate-case', lambda root: edit_stdout(root, lambda data: data.replace(b'case=1 ', b'case=0 ', 1))),
    ('missing-release', lambda root: edit_stdout(root, lambda data: data.replace(b'close status=00000000\r\n', b'', 1))),
    ('priority-input', lambda root: edit_stdout(root, lambda data: data.replace(b'case=231 group=maximum width=8 maximum=0 access=000F001F protection=00000000', b'case=231 group=maximum width=8 maximum=0 access=000F001F protection=00000004', 1))),
]
for name, mutate in probes:
    with tempfile.TemporaryDirectory(prefix='rax-section-create-rejection-') as temporary:
        root = Path(temporary) / 'corpus'
        shutil.copytree(HERE, root)
        mutate(root)
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                check(root)
        except ValueError:
            continue
        raise AssertionError('accepted rehashed mutation: ' + name)
print('Rejected6 rehashed machine,LAA,producer,case,lifetime and priority-input mutations')
