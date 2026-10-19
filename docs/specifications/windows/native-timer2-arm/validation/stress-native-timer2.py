from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

T = Path(__file__).resolve().parent
D = T / 'timer2-source/docs/specifications/windows/native-timer2-arm'
CHECK = D / 'check-native-timer2.py'


def edit_json(path, change):
    raw = path.read_bytes()
    data = json.loads(raw.decode('utf-16' if raw[:2] == b'\xff\xfe' else 'utf-8-sig'))
    change(data)
    path.write_text(json.dumps(data, indent=2) + '\n')


def text_replace(path, before, after):
    text = path.read_text()
    assert before in text, (path, before)
    path.write_text(text.replace(before, after, 1))


def control(root, name):
    v1 = root / 'native-v1'
    v2 = root / 'native-fault-v2'
    v4 = root / 'native-period-v4'
    if name == 'source-byte':
        p = v1 / 'native-timer2-arm-v1.cpp'
        p.write_bytes(p.read_bytes() + b'\n')
    elif name == 'failed-producer':
        edit_json(v1 / 'native-timer2-arm-v1-outcomes.json', lambda x: x[0].update({'exit-code': 1}))
    elif name == 'missing-abi-profile':
        edit_json(v1 / 'native-timer2-arm-v1-outcomes.json', lambda x: x.pop())
    elif name == 'pe-machine':
        for phase in ['before', 'after']:
            edit_json(v1 / ('native-timer2-arm-v1-arm64-' + phase + '.json'), lambda x: x.update(machine=0x8664))
    elif name == 'pe-laa':
        for phase in ['before', 'after']:
            edit_json(v1 / ('native-timer2-arm-v1-x86-laa-' + phase + '.json'), lambda x: x.update({'large-address-aware': False}))
    elif name == 'source-after':
        edit_json(v1 / 'native-timer2-arm-v1-arm64-source-after.json', lambda x: x.update(Hash='0' * 64))
    elif name == 'module-after':
        edit_json(v1 / 'native-timer2-arm-v1-modules-after.json', lambda x: x[0].update(Hash='0' * 64))
    elif name == 'pe-after':
        edit_json(v1 / 'native-timer2-arm-v1-arm64-after.json', lambda x: x.update(sha256='0' * 64))
    elif name == 'value-status':
        text_replace(v1 / 'native-timer2-arm-v1-arm64.log', 'status=00000000', 'status=C000000D')
    elif name == 'native-guard-consumption':
        text_replace(v2 / 'native-timer2-arm-fault-v2-arm64.log', 'due-guard-before=256 due-guard-after=0', 'due-guard-before=256 due-guard-after=256')
    elif name == 'wow-guard-order':
        p = v2 / 'native-timer2-arm-fault-v2-x86.log'
        # Both guarded inputs must preserve the measured WoW64 read order.
        lines = p.read_text().splitlines()
        for i, line in enumerate(lines):
            if 'due-mode=4' in line and 'params-mode=4' in line and 'params-guard-before=256' in line:
                assert 'params-guard-after=0' in line
                lines[i] = line.replace('params-guard-after=0', 'params-guard-after=256')
                break
        else:
            raise AssertionError('missing double guard row')
        p.write_text('\n'.join(lines) + '\n')
    elif name == 'reserved-cancel-pointer':
        p = v2 / 'native-timer2-arm-fault-v2-arm64.log'
        lines = p.read_text().splitlines()
        for i, line in enumerate(lines):
            if line.startswith('CANCEL-FAULT ') and 'params-mode=4' in line:
                lines[i] = line.replace('guard-after=256', 'guard-after=0')
                break
        else:
            raise AssertionError('missing guarded cancel')
        p.write_text('\n'.join(lines) + '\n')
    elif name == 'truncated-case-sequence':
        text_replace(v2 / 'native-timer2-arm-fault-v2-arm64.log', 'FAULT case=0 ', 'FAULT case=1 ')
    elif name == 'maximum-period-early-expiry':
        p = v4 / 'native-timer2-arm-period-v4-arm64.log'
        lines = p.read_text().splitlines()
        for i, line in enumerate(lines):
            if 'period=7FFFFFFFFFFFFFFF' in line:
                lines[i] = line.replace('w0=00000102', 'w0=00000000')
                break
        p.write_text('\n'.join(lines) + '\n')
    else:
        raise AssertionError(name)


names = ['source-byte', 'failed-producer', 'missing-abi-profile', 'pe-machine', 'pe-laa',
         'source-after', 'module-after', 'pe-after', 'value-status', 'native-guard-consumption',
         'wow-guard-order', 'reserved-cancel-pointer', 'truncated-case-sequence', 'maximum-period-early-expiry']
rows = []
with tempfile.TemporaryDirectory(prefix='timer2-negative-') as temp:
    root = Path(temp)
    for name in names:
        target = root / name
        target.mkdir()
        for sub in ['native-v1', 'native-fault-v2', 'native-state-v3', 'native-period-v4']:
            shutil.copytree(D / sub, target / sub)
        control(target, name)
        result = subprocess.run([sys.executable, '-B', str(CHECK), str(target)], capture_output=True)
        assert result.returncode != 0, name
        rows.append({'control': name, 'rejected': True, 'exit-code': result.returncode,
                     'diagnostic-sha256': hashlib.sha256(result.stderr).hexdigest()})
report = {'checker-sha256': hashlib.sha256(CHECK.read_bytes()).hexdigest(),
          'controls': rows, 'rejected': len(rows), 'scope': 'Logical/identity corruptions in private copies; production native capture bytes remain unchanged.'}
(T / 'timer2-native-negative-controls.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'rejected': len(rows), 'total': len(names)}))
