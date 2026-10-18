from pathlib import Path
import json, re
B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = B/'prototype-source/docs/specifications/windows/native-token-bno-isolation'
def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))
def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))
all_runs = []
for prefix, count in [('bno-copy', 12), ('bno-copy-curve', 34)]:
    launch = load(S/(prefix+'-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    outcomes = load(S/(prefix+'-outcomes.json'))
    assert len(outcomes) == count and all(row['exit-code'] == 0 for row in outcomes)
    for row in outcomes:
        lines = (S/row['log']).read_text().splitlines()
        control, reference = fields(lines[0]), fields(lines[2])
        assert lines[1] == 'child-create ok=1 error=0'
        width = int(reference['pointer-bytes'])
        assert width in [4, 8] and reference['enabled'] == '1'
        units = control['prefix-utf16']
        prefix_units = [int(units[i:i+4],16) for i in range(0,len(units),4)]
        assert len(prefix_units) == int(reference['prefix-units'])
        assert int(reference['required']) == 2*width + 2*(len(prefix_units)+1)
        cases = []
        for line in lines:
            if not line.startswith('copy-case '):
                continue
            item = fields(line)
            assert item['escaped'] == '00000000' and item['last-error'] == '7E11ABCD'
            remaining = int(item['remaining'])
            output = int(item['output'],16)
            cells = [item['prefix'][i:i+2] for i in range(0,len(item['prefix']),2)]
            assert len(cells) == remaining
            pointer = (output+2*width).to_bytes(width,'little')
            if remaining >= width and item['prefix'][:2*width] == pointer.hex().upper():
                for index in range(width):
                    cells[index] = '@'+str(index)
            cases.append({key:item[key] for key in ['remaining','protection','after','length','status','returned']} | {'output':''.join(cells)})
        expected_count = (int(reference['required'])+33)*12
        assert len(cases) == expected_count
        assert lines[-2] == f'copy-complete cases={expected_count} closed-handles=1 released-arenas=1'
        assert lines[-1] == 'child-complete wait=00000000 queried=1 exit=0 closed=1'
        all_runs.append({'profile':row['profile'],'prefix':prefix_units,'native-log':row['log'],'cases':cases})
result = {'runs':all_runs,'total-calls':sum(len(row['cases'])for row in all_runs)}
(D/'copy-cases.json').write_text(json.dumps(result,separators=(',',':'))+'\n')
print('Copy corpus',len(all_runs),'runs',result['total-calls'],'queries')
