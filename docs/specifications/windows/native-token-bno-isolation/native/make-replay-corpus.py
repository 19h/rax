from pathlib import Path
import hashlib, json, re
B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = B / 'prototype-source'
D = R / 'docs/specifications/windows/native-token-bno-isolation'

def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))

def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))

launch = load(S / 'bno-isolation-child-launch.json')
assert launch['state'] == 'finished' and launch['exit-code'] == 0
children = load(S / 'bno-isolation-child-outcomes.json')
assert len(children) == 12 and all(row['exit-code'] == 0 for row in children)
profiles = ['arm64', 'x64', 'x86', 'x86-laa']
selected = [(profile, 'baseline', f'bno-isolation-survey-{profile}-class44.log') for profile in profiles]
selected += [(r['profile'], r['mode'], r['log']) for r in children]
groups = []
group_by_digest = {}
runs = []
for profile, mode, name in selected:
    lines = (S / name).read_text().splitlines()
    reference = fields(next(line for line in lines if line.startswith('reference ')))
    assert reference['status'] == '00000000'
    width = 4 if profile.startswith('x86') else 8
    header = 2 * width
    enabled = reference['enabled'] == '1'
    prefix = None
    if enabled:
        raw_prefix = reference['prefix-utf16']
        assert raw_prefix.endswith('0000') and 'FAULT' not in raw_prefix
        prefix = [int(raw_prefix[i:i+4], 16) for i in range(0, len(raw_prefix)-4, 4)]
        assert int(reference['prefix'], 16) == int(reference['buffer'], 16) + header
    else:
        assert reference['prefix'] == '0'
    assert int(reference['bytes']) == header + (2 * (len(prefix)+1) if enabled else 0)
    rows = [fields(line) for line in lines if line.startswith('case ')]
    assert [int(r['ordinal']) for r in rows] == list(range(4170))
    result = []
    for row in rows:
        assert row['escaped'] == '00000000' and row['last-error'] == '7E11ABCD'
        item = {key: row[key] for key in ['kind', 'role', 'length', 'status', 'output-protect', 'returned-protect']}
        native_output = int(row['output'], 16)
        pointer = (native_output + header).to_bytes(width, 'little') if enabled else bytes(width)
        pointer_cells = row['output-bytes-cells'][:2*width]
        published = enabled and pointer_cells == pointer.hex().upper()
        if enabled and row['status'] == '00000000':
            assert published
        for address_key, data_key in [('output', 'output-bytes-cells'), ('returned', 'return-bytes-cells')]:
            address = int(row[address_key], 16)
            raw = row[data_key]
            assert len(raw) == 64
            cells = [raw[i:i+2] for i in range(0, 64, 2)]
            if published:
                for index, cell in enumerate(cells):
                    offset = address + index - native_output
                    if 0 <= offset < width:
                        assert int(cell, 16) == pointer[offset]
                        cells[index] = '@' + str(offset)
            item[data_key] = ''.join(cells)
        result.append(item)
    canonical = json.dumps(result, separators=(',', ':'), sort_keys=True).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    if digest not in group_by_digest:
        group_by_digest[digest] = len(groups)
        groups.append({'sha256': digest, 'rows': result})
    runs.append({'profile': profile, 'mode': mode, 'prefix': prefix, 'group': group_by_digest[digest], 'native-log': name})
D.mkdir(parents=True, exist_ok=True)
(D/'cases.json').write_text(json.dumps({'schema':1,'total-calls':66720,'per-run':4170,'groups':groups,'runs':runs}, separators=(',', ':'))+'\n')
(D/'.gitattributes').write_text('# Preserve raw native proof bytes on every host.\n* -text\n')
print('Built', len(groups), 'native replay groups for', len(runs), 'runs / 66720 queries')
