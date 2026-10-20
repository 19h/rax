"""Project native capture statuses, copy extents, fixed fields and fault effects.

Allocation base/size/commit bytes are masked because the guest fixture uses
its own allocations. Independent lifecycle tests cover those quantities.
"""
from pathlib import Path
import hashlib
import json

CACHE = Path(__file__).parent
ROOT = Path('/Users/int/.assist-region-20261020')
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
DOCS = ROOT / 'docs/specifications/windows/native-region-information'


def fields(line):
    return dict(part.split('=', 1) for part in line.split() if '=' in part)


def run():
    rows = []
    inputs = []
    for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
        width = 4 if profile.startswith('x86') else 8
        for version in [1, 2, 16]:
            path = SHARE / ('native-region-query-v' + str(version) + '-' + profile + '.log')
            raw = path.read_bytes()
            inputs.append({'path': path.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
            for line in raw.decode().splitlines():
                group = line.split()[0] if line else ''
                if (version in [1, 16] and group != 'matrix') or (version == 2 and group not in ['boundary', 'rights']):
                    continue
                record = fields(line)
                if record['class'] not in ['3', '7']:
                    continue
                assert record['exception'] == '00000000'
                row = {'profile': profile, 'group': group, 'class': int(record['class']),
                       'length': int(record['length']), 'status': int(record['status'], 16),
                       'bytes': record['bytes'], 'masked-byte-indices': []}
                if group == 'matrix':
                    row.update({'target': int(record['target']), 'returned-value': int(record['returned'])})
                else:
                    row.update({'role': int(record['role']), 'guards': record['guards'],
                                'returned-bytes': record['returned']})
                output_offset = 128
                if group == 'boundary':
                    output_offset = {1: 132, 2: 130}.get(row['role'], 128)
                    if row['role'] in [3, 4, 14, 16, 17, 18, 27, 35, 36, 37]:
                        output_offset = 4096
                # Mask only the three allocation-dependent integer fields in
                # successful copies within the producer's captured output window.
                if row['status'] == 0 and output_offset < 192:
                    copied = min(row['length'] // width * width, 28 if width == 4 else 48)
                    starts = [0, width + 8, width * 2 + 8]
                    if group == 'boundary' and row['role'] == 22:
                        starts.remove(0)  # aliased ReturnLength overwrites the base field
                    indices = []
                    for start in starts:
                        indices.extend(output_offset - 128 + index for index in range(start, min(start + width, copied))
                                       if 0 <= output_offset - 128 + index < 64)
                    row['masked-byte-indices'] = indices
                rows.append(row)
    assert len(rows) == 1608
    assert {group: sum(row['group'] == group for row in rows) for group in ['matrix', 'boundary', 'rights']} == {
        'matrix': 1256, 'boundary': 304, 'rights': 48}
    value = {'scope': __doc__.strip(), 'native-log-inputs': inputs, 'rows': rows}
    DOCS.mkdir(parents=True, exist_ok=True)
    path = DOCS / 'modelled-query-cases.json'
    assert hashlib.sha256(path.read_bytes()).hexdigest() == 'af0aaf64ed7062a1a61b6fba218fd26d7c3ff96ab7d4ad3483f6437e820d4a00'
    path.write_text(json.dumps(value, indent=2) + '\n')
    print(len(rows), path.stat().st_size, flush=True)


if __name__ == '__main__':
    run()
