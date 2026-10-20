from pathlib import Path
import hashlib
import json
import shutil

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('//Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
TARGET = BASE / 'src-file-attributes-epoch1/target/retained-owning-epoch1'

def identity(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            digest.update(chunk)
    return {'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}

ordinary = json.loads((SHARE / 'file-attributes-native-ordinary-epoch1.json').read_text())
capi = json.loads((SHARE / 'file-attributes-owning-capi-epoch1.json').read_text())
assert ordinary['state']=='finished' and ordinary['exit-code']==0
assert capi['state']=='finished' and capi['exit-code']==0
assert capi['outcomes'][1]['reason']==5 and 'NtOpenFile (0x33)' in capi['outcomes'][1]['diagnostic']
assert not TARGET.exists()
TARGET.mkdir()
rows = []
for label, receipt in [('core', ordinary['core-after']), ('carrier', capi['carrier-after']),
                       ('ordinary', ordinary['executable-after']), ('capi', capi['executable-after'])]:
    source = Path(receipt['path'])
    expected = {key: receipt[key] for key in ['bytes', 'sha256']}
    assert identity(source) == expected
    destination = TARGET / source.name
    if not destination.exists():
        shutil.copy2(source, destination)
    assert identity(destination) == identity(source) == expected
    rows.append({'role': label, 'source': str(source), 'retained': str(destination), **expected})
result = {'state': 'finished', 'exit-code': 0, 'files': rows,
          'scope': 'Four actual observed owning artifacts copied independently; no hard links or regeneration.'}
(SHARE / 'file-attributes-retained-owning-epoch1.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2), flush=True)
