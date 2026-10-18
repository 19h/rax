from pathlib import Path
import hashlib, json, re
D = Path(__file__).resolve().parent
S = D / 'native'
EXPECTED_GROUPS = ['2b0af40d5cbb758a2baad6f4c84da10fe87181c50a0452e74f2abfac0adf0647', '09f0abd5e3da33a39d796b0d7017e9c90d6a735a40e3109f20de504a9258e103', '2f91f54fa601bd37b38ada71caa09f7c5c3f2d8b14e8473e9f15be82a07d930a', '1899ce7a12aa7fc9b04d73a68c5ea9389056cf4a095942b7ed41dae07038e33f', 'f661fb1ea3982f935965eb142001b9a7a4e92c1d2603080a5e632499b7fbca54', 'ee4f7b31f8f6cfb26222dff43cf518825367c5f4a29d83bdc1e6b3e2398531b6', '19ffbfb21e434d99aaf47d5d33dd9ab6329fe00286af63c1a35e073f3de5404d', 'ac2142183642985243e6ced65dcddcd85b5d93486d3163961603b7075ea871d4', 'b88c653ee8b3d2a1afce70082da16945458972d02a047221b6a3e605fe2cfcfc']
if not __debug__:
    raise RuntimeError('optimized verification is not admitted')

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
expected = {'schema':1,'total-calls':66720,'per-run':4170,'groups':groups,'runs':runs}
assert load(D/'cases.json') == expected, 'native corpus differs from raw calls'
assert [group['sha256'] for group in groups] == EXPECTED_GROUPS, 'native semantic fingerprints changed'
for row in load(D/'native-inputs.json')['files']:
    raw = (D/row['path']).read_bytes()
    assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], 'native input identity: '+row['path']
for name, digest in [('native-bno-isolation-survey.cpp', '7e5c5a7a8b05128140c0c4c61a0398767407157a7459993ff6d8a8f09ce27565'),
                     ('native-bno-isolation-child.cpp', 'f6b8a02161abdc141cf3c1b8cc09dd9cf1f6824fe5e4ad14b9a69dfa0c4abb9a')]:
    assert hashlib.sha256((S/name).read_bytes()).hexdigest() == digest, 'native producer source'
post = load(S/'bno-native-post-identities.json')
assert len(post['files']) == 10, 'post-run artifact count'
for row in post['files']:
    name = row['path'].replace('\\','/').split('/')[-1]
    if name == 'probe.exe':
        profile = row['profile']
        before = load(S/f'bno-isolation-survey-{profile}-identity-before.json')
        after = load(S/f'bno-isolation-survey-{profile}-identity.json')
        assert before == after and row['sha256'] == before['sha256'] and row['bytes'] == before['bytes']
        assert before['machine'] == (332 if profile.startswith('x86') else 43620 if profile == 'arm64' else 34404)
        assert before['large-address-aware'] == (profile != 'x86')
    elif name == 'child-control.exe':
        before = load(S/'bno-isolation-child-identity-before.json')
        assert before == load(S/'bno-isolation-child-identity-after.json') and row['sha256'] == before['sha256']
    else:
        assert name in ['native-bno-isolation-survey.cpp','native-bno-isolation-child.cpp']
        assert row['sha256'] == hashlib.sha256((S/name).read_bytes()).hexdigest()
for row in children:
    lines = (S/row['log']).read_text().splitlines()
    control = fields(lines[0])
    reference = fields(lines[2])
    assert control['enabled'] == reference['enabled']
    assert lines[1] == 'child-create ok=1 error=0'
    assert lines[-2] == 'complete cases=4170 closed-handles=3 released-arenas=1'
    assert lines[-1] == 'child-complete wait=00000000 queried=1 exit=0 closed=1'
    if reference['enabled'] == '1':
        assert reference['prefix-utf16'] == control['prefix-utf16']+'0000'
print('Native BNO proof: 66720 calls, 16 successful runs, 10 post-run identities')
