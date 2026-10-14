from pathlib import Path
import argparse
import hashlib
import json

def require(ok, reason):
    if not ok:
        raise ValueError(reason)

def identity(path):
    sha = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1048576), b''):
            sha.update(block)
    return {'bytes': path.stat().st_size, 'sha256': sha.hexdigest()}

parser = argparse.ArgumentParser()
parser.add_argument('--host', required=True, choices=('macos', 'linux'))
parser.add_argument('--target', required=True, type=Path)
parser.add_argument('--rax', required=True, type=Path)
args = parser.parse_args()
require(__debug__, 'optimized validation is rejected')
base = Path(__file__).parent
manifest = json.loads((base / 'section-map-owning-source-hashes.json').read_text())
sources = []
for item in manifest['sources']:
    require(identity(args.rax / item['path']) == {key: item[key] for key in ('bytes', 'sha256')}, 'current source changed: ' + item['path'])
    sources.append(item)
fingerprints = [path for path in (args.target / '.fingerprint').glob('rax-*/lib-rax.json') if not path.parent.name.startswith('rax-capi-')]
require(len(fingerprints) == 1, 'unique selected core fingerprint')
fingerprint = json.loads(fingerprints[0].read_text())
features = json.loads(fingerprint['features']) if isinstance(fingerprint['features'], str) else fingerprint['features']
require(features == [], 'empty-feature core')
suffix = fingerprints[0].parent.name.removeprefix('rax-')
archives = [args.target / 'libassist_rs.a', args.target / 'deps' / ('librax-' + suffix + '.rlib')]
rows = []
for path in archives:
    size = path.stat().st_size
    count = 0
    with path.open('rb') as stream:
        require(stream.read(8) == b'!<arch>\n', 'archive magic')
        while stream.tell() < size:
            header = stream.read(60)
            require(len(header) == 60 and header[58:60] == b'`\n', 'complete member header')
            length = int(header[48:58])
            require(length >= 0, 'member nonnegative extent')
            end = stream.tell() + length + (length & 1)
            require(end <= size, 'member inside archive')
            stream.seek(end)
            count += 1
        require(stream.tell() == size, 'complete archive member walk')
    rows.append({'path': str(path), **identity(path), 'members': count, 'structural-member-walk': 'pass'})
result = {'host': args.host, 'sources': sources, 'core-features': features, 'core-fingerprint': {'path': str(fingerprints[0]), **identity(fingerprints[0])}, 'archives': rows}
(base / (args.host + '-owning-archive-identities.json')).write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'host': args.host, 'source-units': len(sources), 'core-features': features, 'archives': rows}))
