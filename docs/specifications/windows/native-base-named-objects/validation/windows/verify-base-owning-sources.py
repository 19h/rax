from pathlib import Path
import datetime
import hashlib
import json

s = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
b = Path('C:/Windows/Temp/assist-native-rax-20261010')
r = b / 'src-base-named-objects'
m = json.loads((s / 'base-owning-source-hashes.json').read_text())
for row in m['sources']:
    data = (r / row['path']).read_bytes()
    if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('owning source ' + row['path'])
for row in m['helpers']:
    data = Path(row['native-path']).read_bytes()
    if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('owning helper ' + row['path'])
expected = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
if hashlib.sha256((r / '.cargo/config.toml').read_bytes()).hexdigest() != expected:
    raise ValueError('native Cargo adapter drift')
if (b / 'assist/vendor/rax').resolve() != r.resolve():
    raise ValueError('owning source junction drift')
record = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'source-epoch': m['epoch'], 'sources': m['sources'],
          'helpers': m['helpers'], 'native-cargo-adapter-sha256': expected,
          'scope': '419 selected source/fixture inputs; retained Assist carrier, not whole current root'}
(s / 'native-base-owning-source-identities.json').write_text(json.dumps(record, indent=2) + '\n')
print('Verified', len(m['sources']), 'owning source identities and', len(m['helpers']), 'helpers')
