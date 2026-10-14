from pathlib import Path, PureWindowsPath
import hashlib
import json
import re
import sys

if not __debug__:
    raise RuntimeError('Optimized evidence verification is unsupported')


def require(value, why):
    if not value:
        raise ValueError(why)


def identity(path, recorded):
    data = path.read_bytes()
    require(len(data) == recorded['bytes'], 'byte count: ' + str(path))
    require(hashlib.sha256(data).hexdigest() == recorded['sha256'].lower(), 'SHA256: ' + str(path))


def rows(text, prefix):
    return [dict(re.findall(r'([\w-]+)=([^\s]+)', line))
            for line in text.splitlines() if line.startswith(prefix)]


def verify(root):
    base = json.loads((root / 'native-section-map-fresh-identities.json').read_text())
    count = 0
    families = [('access', 914), ('geometry', 15), ('extra', 35), ('query-control', 4),
                ('projection', 21), ('equivalence', 21), ('low-address', 145), ('collision', 3)]
    for family, expected in families:
        stem = 'native-section-map-' + family
        report = json.loads((root / (stem + '-identities.json')).read_text())
        require(report['kernel'] == base['kernel'] and report['native-machine'] == 0xAA64,
                'same measured ARM64 kernel: ' + family)
        require(report['principal'] == base['principal'] and 'S-1-5-18' in report['principal'],
                'measured SYSTEM subject: ' + family)
        require(len(report['libraries']) == len(base['libraries']) == 4, 'all selected libraries')
        for actual, original in zip(report['libraries'], base['libraries']):
            require(all(actual[k] == original[k] for k in ['path', 'bytes', 'sha256', 'machine', 'characteristics']),
                    'unchanged native libraries: ' + family)
        require([p['profile'] for p in report['producers']] == ['arm64', 'x64', 'x86', 'x86-laa'],
                'all four profiles: ' + family)
        outcomes = json.loads((root / (stem + '-outcomes.json')).read_text(encoding='utf-8-sig'))
        freeze = json.loads((root / (family + '-source-freeze.json')).read_text())
        for producer in report['producers']:
            profile = producer['profile']
            source = root / PureWindowsPath(producer['source']['path']).name
            log = root / PureWindowsPath(producer['stdout']['path']).name
            identity(source, producer['source'])
            identity(source, freeze[source.name])
            identity(log, producer['stdout'])
            executable = producer['executable']
            require(executable['machine'] == {'arm64': 0xAA64, 'x64': 0x8664,
                    'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'actual executable machine')
            require(bool(executable['characteristics'] & 0x20) == (profile != 'x86'),
                    'actual executable LAA')
            outcome = next(o for o in outcomes if o['profile'] == profile)
            require(outcome['exit-code'] == 0, 'producer exit')
            require(outcome['source-hash'].lower() == producer['source']['sha256'], 'executed source')
            require(outcome['exe-sha256'].lower() == executable['sha256']
                    and outcome['exe-bytes'] == executable['bytes'], 'executed binary')
            text = log.read_text()
            matrix = rows(text, 'case=')
            require(len(matrix) == expected and [int(r['case']) for r in matrix] == list(range(expected)),
                    'ordered complete matrix: ' + log.name)
            require('failure=' not in text and
                    f'complete cases={expected} lifetime=1 pointer-bytes={4 if profile.startswith("x86") else 8}' in text,
                    'complete producer footer')
            require(len(rows(text, 'lifetime status=')) == 1 and 'closed=1' in text,
                    'view survives handle close')
            if family == 'collision':
                require(len(rows(text, 'collision ')) == 3, 'three live-view controls')
            count += expected
    require(count == 4632, 'complete independent matrix total')
    for family, expected in [('fresh', 2), ('startup-controls', 7)]:
        stem = 'native-section-map-' + family
        report = json.loads((root / (stem + '-identities.json')).read_text())
        require(report['kernel'] == base['kernel'] and report['principal'] == base['principal'],
                'startup kernel/subject')
        producer = report['producers'][0]
        identity(root / (stem + '.cpp'), producer['source'])
        log = root / (stem + '-arm64.log')
        identity(log, producer['stdout'])
        text = log.read_text()
        require(f'complete cases={expected} warmup=1' in text and 'failure=' not in text,
                'original-entry startup completeness')
        for ordinal in range(1, expected + 1):
            found = re.search(rf'case={ordinal} warmup=0 installed=1 done=1 calls=1 exit=77 handles-before=(\d+) handles-after=(\d+) exact-handle-set=1', text)
            require(found is not None and found[1] == found[2], 'exact startup handle restoration')
    for source in json.loads((root / 'references/sources.json').read_text()):
        identity(root / 'references' / source['file'], source)
        require(source['url'].startswith('https://') and source['retrieved-utc'], 'primary retrieval provenance')
    sdk = json.loads((root / 'native-sdk-map-status.json').read_text())
    require(any('STATUS_IMAGE_AT_DIFFERENT_BASE   ((NTSTATUS)0x40000036L)' in '\n'.join(v['definitions']) for v in sdk),
            'installed SDK status declaration')
    hashes = {}
    for profile in ['arm64', 'x64']:
        text = (root / ('native-section-map-equivalence-' + profile + '.log')).read_text()
        hashes[profile] = {int(r['case']): r for r in rows(text, 'image-byte-hash ')}
    for projected, native, size in [(9, 1, 1511424), (12, 4, 4665344)]:
        a, b = hashes['arm64'][projected], hashes['x64'][native]
        require(a['sha256'] == b['sha256'] and int(a['size']) == int(b['size']) == size,
                'whole-image AMD64 projection equivalence')
    return {'matrix-observations': count, 'original-entry-measured-cases': 9,
            'whole-image-projection-bytes': 6176768, 'producer-profiles': 4,
            'physical-kernel': 'ARM64; x64/x86 compatibility processes'}


if __name__ == '__main__':
    print(json.dumps(verify(Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent), sort_keys=True))
